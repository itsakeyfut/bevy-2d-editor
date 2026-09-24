//! The inspector: what the active entity is, and what it is made of.
//!
//! What it shows, what it calls an entity, and in what order are settled in
//! [`docs/specs/ui.md` §6](../../../docs/specs/ui.md); what it lets the user
//! edit, and what that rejected, is
//! [§7](../../../docs/specs/ui.md). That the panel is rebuilt rather than
//! reconciled is
//! [ADR-0002](../../../docs/adr/0002-rebuild-the-inspector-rather-than-diff-it.md),
//! and that it is left alone while the user is in it is
//! [ADR-0003](../../../docs/adr/0003-the-inspector-panel-belongs-to-the-user-while-focus-is-in-it.md).

use core::any::TypeId;

use b2d_editor_ui::{field_line, field_row};
use bevy::feathers::containers::{pane_body, pane_header};
use bevy::feathers::controls::{
    FeathersNumberInput, NumberFormat, NumberInputValue, UpdateNumberInput,
};
use bevy::feathers::display::{label, label_dim};
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::reflect::{ReflectMut, ReflectRef};
use bevy::ui::ScrollPosition;
use bevy::ui_widgets::ValueChange;

use crate::{Region, Selection};

/// How far a field row sits in from the component name above it.
///
/// The drawing in `docs/specs/data-model.md` §3 is a tree, and this is what
/// makes it read as one without drawing the branches.
const FIELD_INDENT: Val = Val::Px(12.0);

/// What a row says when the component it names has no reflection.
///
/// A row rather than nothing, and a fixed phrase rather than a name, because in
/// this workspace's feature configuration there is no name to be had. See
/// [`show`] for why, and `docs/specs/ui.md` §6 for why the row stays.
const UNREGISTERED: &str = "<no reflection>";

/// What the inspector is currently showing.
///
/// Compared rather than watched. Change detection on [`Selection`] alone would
/// not see a component added to an entity that is already selected, and
/// comparing the finished picture needs no second mechanism to keep in step
/// with the first.
#[derive(Resource, Default, PartialEq)]
struct Shown(Option<Shape>);

/// The panel's whole content, as one value that can be compared.
#[derive(PartialEq)]
struct Shape {
    /// Which entity this is about.
    ///
    /// **Nothing draws it, and it is read for two different reasons.**
    ///
    /// It was put here because the comparison has to carry the subject: two
    /// entities with the same `Name` and the same components compared equal, so
    /// moving the selection between them left the panel alone. Showing the
    /// values took that pair away, because two entities that sit in different
    /// places differ in `Transform`, `GlobalTransform` and `Aabb`, and two that
    /// sit in the same place cannot both be clicked, `select` in `selection.rs`
    /// taking the nearest. For one commit that left this field reaching
    /// nothing: measured, replacing it with `Entity::PLACEHOLDER` left all 85
    /// tests green.
    ///
    /// **What reaches it now is the scroll position.** The pane keeps where it
    /// was scrolled to across a rebuild, and the one case where that position
    /// means nothing is the selection moving to another entity, which is this
    /// field changing. Mutation: replace it with `Entity::PLACEHOLDER`, and
    /// `choosing_another_entity_puts_the_panel_back_at_the_top` fails.
    ///
    /// The original reason stands underneath: filtering which components are
    /// listed, and collapsing a component so its values are not drawn, are both
    /// deferred in [`docs/specs/ui.md` §6](../../../docs/specs/ui.md), and
    /// either one brings look-alikes back.
    of: Entity,
    /// Which region the children were spawned under.
    ///
    /// The same argument as `of`, one level out: without it this value is a
    /// claim about a particular pane's children compared without reference to
    /// that pane. Replace the region and the comparison still says "already
    /// shown", so the new pane stays blank until the selection next changes.
    /// Measured before this field was here: despawning the inspector region and
    /// spawning another left the panel empty with an entity still selected.
    ///
    /// **Nothing can reach that today**, because `spawn_regions` runs once at
    /// `Startup` and nothing despawns a region. Docking is what brings it, and
    /// docking is deferred with a trigger in
    /// `docs/specs/open-questions.md` §1.
    into: Entity,
    /// What the header calls the entity.
    title: String,
    /// One per component, in the order they are drawn.
    rows: Vec<Row>,
}

/// One component's row, and what it opens into.
///
/// **What the rows sort on is [`Row::key`] and not a derive.** It used to be
/// `#[derive(Ord)]`, which ranked by variant declaration order and then by the
/// fields, so the order on screen turned on the order of two lines in this
/// enum and on `name` being declared first. That could not survive a `f32`
/// arriving in [`Leaf`], which has no `Ord`, and saying the rule outright is
/// better than a derive whose meaning has to be explained.
#[derive(PartialEq)]
enum Row {
    /// A component whose value could be read, and the lines under it.
    Named {
        /// What the panel calls it.
        name: String,
        /// Which type it is, by the same id the walk read the value with.
        ///
        /// Nothing draws it. It is what [`write_leaf`] reaches the component
        /// with when a box under this row is committed, so that the write goes
        /// to the type the panel read rather than to one looked up again from
        /// a name.
        of: TypeId,
        /// One per line under the name, in the order they are drawn.
        fields: Vec<Field>,
    },
    /// A component with no reflection, so with neither a name nor a value.
    Unregistered,
}

impl Row {
    /// What the rows sort on: every unregistered row after every named one,
    /// then the name.
    ///
    /// Mutation: drop the `bool`, and
    /// `the_rows_are_in_the_order_the_names_sort_in` fails, because
    /// `<no reflection>` sorts among the names rather than after them.
    fn key(&self) -> (bool, &str) {
        match self {
            Row::Named { name, .. } => (false, name.as_str()),
            Row::Unregistered => (true, ""),
        }
    }
}

/// One line under a component.
///
/// **The values are part of what the panel is compared on**, which is what
/// makes a component that changed in the world redraw: [`Shown`] holds the
/// finished picture, and a picture that carried only the names would be equal
/// to itself while the numbers moved.
///
/// Mutation: write `PartialEq` by hand so that [`Field::Text`] compares `name`
/// and not `value`, and `a_field_row_follows_the_component_it_reads` fails.
#[derive(PartialEq)]
enum Field {
    /// A line carrying text, which is what every line was before
    /// [`docs/specs/ui.md` §7](../../../docs/specs/ui.md).
    Text {
        /// The field's name, or `None` when the component is not a named
        /// struct and the line carries the whole value.
        name: Option<String>,
        /// What the value reads as.
        value: String,
    },
    /// A line whose value is numbers, drawn as one box per number.
    Numbers {
        /// The field's name. There is always one, because only a named field
        /// can reach this arm.
        name: String,
        /// One per box, in the order the type declares them.
        leaves: Vec<Leaf>,
    },
}

/// One editable number under a field.
///
/// **`PartialEq` is written by hand, and compares the bits.** A derived one
/// would use `f32`'s, under which `NaN` is equal to nothing including itself,
/// so a `Transform` holding a `NaN` would make [`Shape`] differ from itself
/// every frame and the panel would be despawned and respawned every frame.
/// That is row 5 of `CLAUDE.md`'s list, and it is the failure
/// [ADR-0002](../../../docs/adr/0002-rebuild-the-inspector-rather-than-diff-it.md)
/// rejected *rebuild every frame* over.
///
/// Mutation: derive `PartialEq` instead, and
/// `a_panel_holding_a_nan_is_not_rebuilt_every_frame` fails.
struct Leaf {
    /// What the box is labelled with, from the type's own field name.
    name: String,
    /// What it holds.
    value: f32,
}

impl PartialEq for Leaf {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.value.to_bits() == other.value.to_bits()
    }
}

/// What a number box writes to when it is committed.
///
/// Patched onto the [`FeathersNumberInput`] entity itself rather than onto the
/// line it sits in, because that entity is what `ValueChange::source` names.
///
/// Mutation: put it on the line instead, and
/// `a_number_box_carries_what_it_writes_to` fails.
#[derive(Component, Clone)]
struct Writes {
    /// The entity the panel was drawn about.
    of: Entity,
    /// The component, by the id the walk read it with.
    component: TypeId,
    /// The field's name, from the same walk.
    field: String,
    /// The leaf's name, likewise.
    leaf: String,
}

/// The inspector, showing what is selected.
///
/// Mutation: leave its row out of the editor's member table, and
/// `the_group_carries_the_members_the_table_names` fails.
pub struct InspectorPlugin;

impl Plugin for InspectorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Shown>()
            .add_systems(Update, show)
            .add_observer(commit);
    }
}

/// Put the active entity and its components in the inspector region.
///
/// # What Bevy 0.19.1 provides here, and what it does not
///
/// Measured against this workspace's feature line, which `docs/specs/ui.md` §3
/// settles:
///
/// * **`ComponentInfo::name()` is no source of a name.** It returns a
///   `DebugName`, which carries nothing unless `bevy_utils/debug` is on, and
///   this build does not turn it on: every component answers
///   `"<Enable the debug feature to see the name>"`.
/// * **The value carries its own name**, through
///   `PartialReflect::reflect_short_type_path()`. That is a `DynamicTypePath`
///   method, so it is callable on the `&dyn Reflect` this already has, and it
///   answers what the type registry's `type_path_table().short_path()` answers:
///   measured equal for all 14 readable components on a placeholder. So the
///   registry is not read here at all, and the name and the value cannot
///   disagree about which type they came from.
/// * **A component nobody registered has no name at all.** There is no
///   fallback to drop to, which is why [`UNREGISTERED`] exists. Measured on a
///   placeholder that has been clicked: **14 components, 13 readable and one
///   not**, and the one is `Selectable`. Thirteen before the click, because
///   `PickingInteraction` arrives with it.
/// * **The value is [`World::get_reflect`], which is the engine's own.** It
///   needs only `ReflectFromPtr`, which `#[derive(Reflect)]` inserts
///   unconditionally, where `ReflectComponent` additionally needs
///   `#[reflect(Component)]` on the type and would therefore see fewer
///   components than the engine's own call does. Its `Err` has four variants
///   and only `MissingReflectFromPtrTypeData` can arrive here: the type id came
///   from the entity's own `ComponentInfo`, and `AppTypeRegistry` exists. **One
///   consequence has no test and cannot get one**: a type implementing
///   `Reflect` by hand and registered without `ReflectFromPtr` would have been
///   named by the registry and reads [`UNREGISTERED`] here. Nothing in the tree
///   is such a type, because the derive always inserts it.
/// * **What a value reads as is its `Debug`**, which is total over
///   `ReflectRef` and so has no arm that can panic. A named-field struct opens
///   into its fields and nothing descends further; `docs/specs/ui.md` §6 has
///   what that turned down. Measured on a clicked placeholder: of its 13
///   readable components, 6 are named-field structs and 7 are not, so the
///   unnamed line in [`fields_of`] is the ordinary case rather than the edge
///   one. Two of the six open into nothing, being structs with no fields.
/// * **`reflect_auto_register` is on**, through Bevy's `default_app`. A
///   component that derives `Reflect` is registered without anybody calling
///   `register_type`, so phase 3's user-defined components arrive here on their
///   own.
/// * **The walk is [`World::inspect_entity`], which is the engine's own.** It
///   returns `Result<impl Iterator<Item = &ComponentInfo>, EntityNotSpawnedError>`,
///   the `Err` covering an entity that is gone. It needs `&World`, and taking
///   `&World` does **not** make a system exclusive: Bevy reserves that word for
///   a system taking `&mut World`. Nor does it cost any parallelism against
///   `Query<EntityRef>`, which was the other spelling: both reach
///   `Access::read_all`, so the scheduler sees one footprint. An earlier
///   version of this comment claimed otherwise, and ADR-0002 records what that
///   was worth.
///
/// # Why the resource is read and written through two different paths
///
/// [`Shown`] is a `Res` here and goes back through `Commands`, **not a
/// `ResMut`**. `&World` claims read access to everything, resources included,
/// so a `ResMut` beside it is a conflict the engine panics on at first run:
///
/// ```text
/// error[B0002]: ResMut<<Enable the debug feature to see the name>> in system
/// <Enable the debug feature to see the name> conflicts with a previous system
/// parameter.
/// ```
///
/// Measured under both this spelling and the `Query<EntityRef>` one it
/// replaced. Note that the message can name neither the resource nor the
/// system, for the same reason the components have no names.
///
/// # The empty state is an empty pane
///
/// Nothing selected, or a selection naming an entity that is gone, leaves the
/// region with no children. That is Unity's inspector, which
/// `docs/specs/ui.md` §2 makes the target, and it is what the criterion asks
/// for: the previous entity's components are not what is on screen.
///
/// Mutation: replace `.last()` with `.first()`, and
/// `the_inspector_shows_the_last_entity_chosen` fails. Mutation: drop the
/// `shown.0 == wanted` guard, and
/// `the_inspector_does_not_rebuild_what_has_not_changed` fails. Mutation: drop
/// the `rows.sort_by`, and `the_rows_are_in_the_order_the_names_sort_in`
/// fails. Mutation: delete the focus guard, and
/// `a_focused_field_survives_the_edit_it_commits` fails, along with
/// `the_panel_is_frozen_while_a_box_has_focus_and_catches_up_after`. Mutation:
/// make that guard read `focus.get().is_some()`, and
/// `focus_outside_the_panel_does_not_freeze_it` fails. Mutation: drop the
/// `UpdateNumberInput` where a box is spawned, and
/// `the_row_shows_what_the_component_holds_after_a_commit` fails on an empty
/// box. Mutation: give `Writes::of` the first selected entity rather than
/// `Shape::of`, and `an_edit_touches_one_field_of_one_entity` fails.
/// Mutation: return `Row::Named` for the unregistered arm, and
/// `a_component_with_no_reflection_appears_as_unregistered` fails. Mutation:
/// despawn the children only when there is something to put back, and
/// `the_inspector_is_empty_when_nothing_is_selected` fails. Mutation: format the
/// entity without asking for its `Name`, and
/// `the_header_uses_the_entitys_name_when_it_has_one` fails. Mutation: give a
/// field row an empty value in [`fields_of`], and
/// `a_field_row_reads_the_entitys_own_value` fails. Mutation: write to
/// the entity through `commands` here, and
/// `showing_a_component_does_not_change_it` fails. Mutation: drop the count
/// from the title, and `the_header_says_how_many_are_selected_when_several_are`
/// fails, naming the entity without it. Mutation: drop the
/// `of` field from [`Shape`], and **nothing fails**, which is measured and is
/// written on that field rather than here. Mutation: drop the
/// `into` field from [`Shape`], and
/// `the_panel_is_rebuilt_when_the_region_it_draws_into_is_replaced` fails. Each
/// was applied and the named test watched to fail.
fn show(
    world: &World,
    shown: Res<Shown>,
    focus: Res<InputFocus>,
    regions: Query<(Entity, &Region)>,
    mut commands: Commands,
) {
    // Before `spawn_regions` has run there is nowhere to put anything, and
    // writing `Shown` then would record a picture that was never drawn.
    let Some((panel, _)) = regions
        .iter()
        .find(|(_, region)| **region == Region::Inspector)
    else {
        return;
    };

    // While the user is in the panel, the panel is theirs: a rebuild would
    // despawn the box being typed into, and committing a value is itself a
    // change that would trigger one.
    // [ADR-0003](../../../docs/adr/0003-the-inspector-panel-belongs-to-the-user-while-focus-is-in-it.md)
    // is where the options were weighed.
    //
    // `Shown` is left unwritten too. Writing it would record a picture that
    // was never drawn, and the panel would then compare equal to something
    // that is not on screen, which is what `Shape::into` exists to prevent one
    // level out.
    if focus
        .get()
        .is_some_and(|focused| is_under(world, focused, panel))
    {
        return;
    }

    // The last element is the one Unity calls active, and `Selection` in
    // `selection.rs` is where that condition is written down; this is its first
    // reader.
    //
    // **It says less than it looks.** That file also says that several entities
    // added by one gesture go on the end together in the order the world
    // iterates them, and that only the boundary between gestures is meaningful.
    // So after a click the last element is the one just clicked, and after a box
    // drag it is an arbitrary member of what the box covered. Measured: a box
    // over two placeholders selects both and this names the second. What the
    // header should say in that case is not decided; the deferral and its
    // trigger are in `docs/specs/open-questions.md` §1.
    let selection = world.resource::<Selection>();
    let wanted = selection.entities().last().and_then(|entity| {
        // `Err` rather than a panic when the entity is gone. **No test holds
        // this, because nothing can currently reach it**: `forget_what_is_gone`
        // in `selection.rs` observes `Remove<Selectable>`, observers run at the
        // despawn rather than a frame later, and `Selection` only ever names
        // something that was `Selectable`. Measured: replacing this with
        // `.expect()` leaves all twelve tests in this module green. It stays
        // because `inspect_entity` returns a `Result` and taking the safe arm
        // of one costs nothing, and because the entity that reaches here first
        // will be one some later gesture put in the selection without that
        // observer's knowledge. Saying so beats letting a green suite imply a
        // guard, which is RK-005's rule.
        let mut rows: Vec<Row> = world
            .inspect_entity(*entity)
            .ok()?
            .map(|info| {
                let Some(of) = info.type_id() else {
                    return Row::Unregistered;
                };
                world
                    .get_reflect(*entity, of)
                    .map_or(Row::Unregistered, |value| Row::Named {
                        name: value.reflect_short_type_path().to_owned(),
                        of,
                        fields: fields_of(value.as_partial_reflect()),
                    })
            })
            .collect();
        rows.sort_by(|a, b| a.key().cmp(&b.key()));
        let named = world.get::<Name>(*entity).map_or_else(
            || format!("Entity {entity}"),
            |name| name.as_str().to_owned(),
        );
        // How many are selected, not which one of them this is: `selection.rs`
        // says a boxed group has no order inside it, so there is no index to
        // report. `docs/specs/ui.md` §6 has why the panel says it at all.
        let title = match selection.entities().len() {
            0 | 1 => named,
            several => format!("{named} (1 of {several} selected)"),
        };
        Some(Shape {
            of: *entity,
            into: panel,
            title,
            rows,
        })
    });

    if shown.0 == wanted {
        return;
    }

    // A rebuild leaves the pane's `ScrollPosition` alone, because the pane
    // outlives the panel drawn into it and because most rebuilds are the same
    // entity's values moving: hovering a sprite changes `PickingInteraction`,
    // and a scroll that jumped back to the top whenever the pointer crossed
    // something would be unusable. Moving to **another entity** is the case
    // where the position means nothing, so it goes back to the top there.
    //
    // This is the second reader of [`Shape::of`], and the first one any test
    // reaches.
    if shown.0.as_ref().map(|shape| shape.of) != wanted.as_ref().map(|shape| shape.of) {
        commands.entity(panel).insert(ScrollPosition::DEFAULT);
    }

    commands.entity(panel).despawn_children();
    if let Some(shape) = &wanted {
        let header = commands
            .spawn_scene(pane_header())
            .insert(ChildOf(panel))
            .id();
        commands
            .spawn_scene(label(shape.title.clone()))
            .insert(ChildOf(header));

        let body = commands
            .spawn_scene(pane_body())
            .insert(ChildOf(panel))
            .id();
        for row in &shape.rows {
            match row {
                Row::Named { name, of, fields } => {
                    let group = commands
                        .spawn_scene(bsn! {
                            Node { flex_direction: FlexDirection::Column }
                        })
                        .insert(ChildOf(body))
                        .id();
                    commands
                        .spawn_scene(label(name.clone()))
                        .insert(ChildOf(group));
                    for field in fields {
                        // The indent is the panel's business rather than the
                        // widget's, so it is written here and not inside
                        // `field_row`, which stays a plain two-string row.
                        // It is **patched onto the scene** rather than put on a
                        // node of its own: neither `field_row` nor `label_dim`
                        // sets `padding`, so this merges into the row's own
                        // `Node`. That is what `bsn!` composition is for, and
                        // `spawn_regions` in `lib.rs` already patches Feathers'
                        // `pane()` the same way. Measured: a wrapper entity per
                        // line was 119 entities under the panel against 97,
                        // which ADR-0002 spawns and despawns on every rebuild.
                        let indent = UiRect::left(FIELD_INDENT);
                        match field {
                            Field::Text {
                                name: Some(name),
                                value,
                            } => {
                                let (name, value) = (name.clone(), value.clone());
                                commands
                                    .spawn_scene(bsn! {
                                        field_row(name, value)
                                        Node { padding: {indent} }
                                    })
                                    .insert(ChildOf(group));
                            }
                            Field::Text { name: None, value } => {
                                let value = value.clone();
                                commands
                                    .spawn_scene(bsn! {
                                        label_dim(value)
                                        Node { padding: {indent} }
                                    })
                                    .insert(ChildOf(group));
                            }
                            Field::Numbers { name, leaves } => {
                                let named = name.clone();
                                let line = commands
                                    .spawn_scene(bsn! {
                                        field_line(named)
                                        Node { padding: {indent} }
                                    })
                                    .insert(ChildOf(group))
                                    .id();
                                for leaf in leaves {
                                    commands
                                        .spawn_scene(label_dim(leaf.name.clone()))
                                        .insert(ChildOf(line));
                                    // The box goes up empty and is filled by
                                    // the event `bevy_feathers` provides for
                                    // it. The `ValueChange` that comes back
                                    // out carries `is_final: false`, which is
                                    // what `commit` declines to act on.
                                    let cell = commands
                                        .spawn_scene(bsn! {
                                            @FeathersNumberInput {
                                                @number_format: NumberFormat::F32,
                                            }
                                        })
                                        .insert((
                                            ChildOf(line),
                                            Writes {
                                                of: shape.of,
                                                component: *of,
                                                field: name.clone(),
                                                leaf: leaf.name.clone(),
                                            },
                                        ))
                                        .id();
                                    commands.trigger(UpdateNumberInput {
                                        entity: cell,
                                        value: NumberInputValue::F32(leaf.value),
                                    });
                                }
                            }
                        }
                    }
                }
                Row::Unregistered => {
                    commands
                        .spawn_scene(label_dim(UNREGISTERED))
                        .insert(ChildOf(body));
                }
            }
        }
    }
    commands.insert_resource(Shown(wanted));
}

/// What a component opens into.
///
/// A named-field struct becomes one [`Field`] per field, in the order the type
/// declares them, which is what makes `Transform` read
/// `translation, rotation, scale` rather than alphabetically: the order carries
/// meaning and sorting it would take that away. Every other shape, and that is
/// the tuple structs, the enums and the opaque types, becomes a single unnamed
/// [`Field`] carrying the whole value. A unit struct is a named-field struct
/// with no fields and so becomes nothing, which is a component name with
/// nothing under it.
///
/// **Nothing here can panic**, and that is the point: `Debug` on a
/// `PartialReflect` is defined for every `ReflectRef` arm, so a component shape
/// this code has never seen is a row rather than a crash. What is on screen,
/// and what that turned down, is `docs/specs/ui.md` §6.
///
/// Mutation: return an empty `Vec` from the unnamed arm, and
/// `a_component_that_is_not_a_named_struct_still_produces_a_row` fails.
fn fields_of(value: &dyn PartialReflect) -> Vec<Field> {
    match value.reflect_ref() {
        ReflectRef::Struct(shape) => (0..shape.field_len())
            .map(|index| {
                let name = shape.name_at(index);
                let field = shape.field_at(index);
                match (name, field.and_then(leaves_of)) {
                    (Some(name), Some(leaves)) => Field::Numbers {
                        name: name.to_owned(),
                        leaves,
                    },
                    _ => Field::Text {
                        name: name.map(ToOwned::to_owned),
                        value: field.map_or_else(String::new, |field| format!("{field:?}")),
                    },
                }
            })
            .collect(),
        _ => vec![Field::Text {
            name: None,
            value: format!("{value:?}"),
        }],
    }
}

/// The numbers a field opens into, or `None` when it is not a line of numbers.
///
/// A named-field struct whose fields are **all** `f32`, and at least one of
/// them. `Vec3` and `Quat` are both that; `Srgba` is not, because it is behind
/// an enum, and `Aabb` is, which
/// [`docs/specs/ui.md` §7](../../../docs/specs/ui.md) carries as an accepted
/// risk.
///
/// **This is not a recursion, and that is the whole reason it is allowed to
/// exist.** [`docs/specs/ui.md` §6](../../../docs/specs/ui.md) rejected
/// descending to the leaves because a recursion whose stop condition is wrong
/// is row 5, the viewport not answering. This looks at one level and answers
/// yes or no: there is no depth to bound, and no enum, map or list to decide
/// about, because anything that is not a struct of `f32` is simply a `no`.
///
/// The empty struct answers `no` rather than an empty line of boxes, which
/// keeps §6's rule that a struct with no fields opens into nothing.
///
/// Mutation: return `None` always, and
/// `a_field_of_numbers_opens_into_one_box_per_leaf` fails. Mutation: accept a
/// struct with one field that is not an `f32`, and
/// `a_field_that_is_not_all_numbers_stays_one_line` fails.
fn leaves_of(value: &dyn PartialReflect) -> Option<Vec<Leaf>> {
    let ReflectRef::Struct(shape) = value.reflect_ref() else {
        return None;
    };
    if shape.field_len() == 0 {
        return None;
    }
    (0..shape.field_len())
        .map(|index| {
            Some(Leaf {
                name: shape.name_at(index)?.to_owned(),
                value: *shape.field_at(index)?.try_downcast_ref::<f32>()?,
            })
        })
        .collect()
}

/// Whether one entity is the panel, or sits somewhere under it.
///
/// Walks `ChildOf` upwards, which is bounded by the depth of the panel's tree.
/// The alternative, asking the panel for its descendants, would walk the whole
/// subtree to answer a question about one entity.
fn is_under(world: &World, entity: Entity, panel: Entity) -> bool {
    let mut next = Some(entity);
    while let Some(current) = next {
        if current == panel {
            return true;
        }
        next = world.entity(current).get::<ChildOf>().map(ChildOf::parent);
    }
    false
}

/// Commit what a number box holds, when the user says so.
///
/// **`is_final` is the whole shape of this.** It is Enter, or the box losing
/// focus. `bevy_feathers` also emits on every keystroke with it false, and once
/// more when [`UpdateNumberInput`] fills a box the panel has just spawned:
/// measured with a throwaway probe, pushing `12.5` into a fresh box produced a
/// `ValueChange` carrying `12.5` straight back out. Acting on those would move
/// the sprite through `-`, `-1`, `-12` on the way to `-120`, each of which is
/// an edit nothing can take back while there is no undo, and would make the
/// panel's own initial draw write to the world.
/// [`docs/specs/ui.md` §7](../../../docs/specs/ui.md) carries why.
///
/// Mutation: drop the `is_final` check, and
/// `a_value_that_is_not_committed_leaves_the_component_alone` fails.
fn commit(change: On<ValueChange<f32>>, writes: Query<&Writes>, mut commands: Commands) {
    if !change.is_final {
        return;
    }
    let Ok(target) = writes.get(change.source) else {
        return;
    };
    let target = target.clone();
    let value = change.value;
    commands.queue(move |world: &mut World| write_leaf(world, &target, value));
}

/// Write one committed number onto the component it came from.
///
/// It takes `&mut World` because [`World::get_reflect_mut`] does, and that call
/// is the mirror of the read: [`show`] reads through `World::get_reflect`
/// because `ReflectComponent` would see fewer components than the engine's own
/// call does, so the set that can be written is exactly the set that is shown.
///
/// **This is the function undo wraps rather than replaces.**
/// `docs/specs/data-model.md` §1 says the path is an `EditorCommand` through an
/// Editor Model, and records why there is no Editor Model to write one against
/// yet and what ends that. `EditorCommand::execute` takes the same `&mut World`
/// this does.
///
/// **Silent on every failure.** Each one means the component went away or
/// changed shape between the panel being drawn and Enter being pressed, and the
/// panel is rebuilt from the world the moment focus leaves the box, which is
/// where the user finds out. Nothing in the editor can reach it today, because
/// nothing removes a component.
///
/// # Where this lands on `CLAUDE.md`'s failure list
///
/// **Row 4**: it is on screen and the user can see it. An edit cannot be taken
/// back, because undo is later in the phase, and what that costs is retyping a
/// number that was on screen the whole time.
///
/// **What would make it row 6**, the row that destroys somebody's work, is a
/// write the user does not see happening. Three things keep it off that row,
/// and each has a guard rather than an intention:
///
/// * the write only ever happens on a box the user is in, because [`commit`]
///   acts on `is_final` alone, which is Enter or the focus leaving that box:
///   `a_value_that_is_not_committed_leaves_the_component_alone`
/// * it lands on the entity the header names and on nothing else:
///   `an_edit_touches_one_field_of_one_entity`
/// * nothing is written that could not be parsed, so no zero goes over a
///   number: `an_unparseable_value_is_never_written`, which pins
///   `bevy_feathers` rather than this file and says so
///
/// The fourth thing, that the box and the component agree afterwards, is
/// `the_row_shows_what_the_component_holds_after_a_commit`. A read path and a
/// write path that disagree is row 6 in miniature.
///
/// Mutation: replace the `try_apply` with `Ok(())`, and
/// `committing_a_field_writes_it_to_the_component` fails.
fn write_leaf(world: &mut World, writes: &Writes, value: f32) {
    let Ok(mut component) = world.get_reflect_mut(writes.of, writes.component) else {
        return;
    };
    let ReflectMut::Struct(shape) = component.reflect_mut() else {
        return;
    };
    let Some(field) = shape.field_mut(&writes.field) else {
        return;
    };
    let ReflectMut::Struct(field) = field.reflect_mut() else {
        return;
    };
    let Some(leaf) = field.field_mut(&writes.leaf) else {
        return;
    };
    let _ = leaf.try_apply(&value);
}

#[cfg(test)]
mod tests {
    use super::{UNREGISTERED, Writes};
    use crate::pointer::{click_at, hold_key, in_window, release_key, scroll_at};
    use crate::{Region, Selection, editor, headless};
    use bevy::feathers::controls::FeathersNumberInput;
    use bevy::input_focus::{FocusCause, InputFocus};
    use bevy::picking::pointer::PointerButton;
    use bevy::prelude::*;
    use bevy::text::{EditableText, TextEdit};

    /// A component this code has never heard of, standing in for one a user
    /// defines in phase 3.
    ///
    /// It derives `Reflect` and nothing registers it: `reflect_auto_register`
    /// is what puts it in the registry, which is the same thing that will carry
    /// a user's own components in. A component without that derive is the other
    /// case, and `Selectable` is already it.
    #[derive(Component, Reflect)]
    struct Unheard;

    /// A component that is a tuple struct rather than a named one.
    ///
    /// Declared here rather than borrowed from the engine, so that the test
    /// asserting its text asserts something this file wrote down. `Anchor` and
    /// `Name` are the engine's own examples, and their `Debug` is Bevy's to
    /// change.
    #[derive(Component, Reflect)]
    struct Weight(f32);

    /// A component that is an enum.
    #[derive(Component, Reflect)]
    enum Stance {
        /// The variant the test puts on the entity.
        Guarding,
    }

    /// A component with no fields at all.
    ///
    /// A named-field struct with nothing in it, which is the shape
    /// `TransformTreeChanged` has: it opens into no lines, rather than into a
    /// line saying nothing.
    #[derive(Component, Reflect)]
    struct Marked;

    /// A named-field struct whose field is not a number.
    ///
    /// The line it draws is the one every line was before
    /// `docs/specs/ui.md` §7, and it is here so that the tests about that
    /// shape do not have to borrow a field off an engine type whose contents
    /// are Bevy's to change. `Transform`'s three fields are all numbers now,
    /// so it can no longer stand in for this.
    #[derive(Component, Reflect)]
    struct Titled {
        /// What the line reads.
        title: String,
    }

    /// A named-field struct of numbers that this file declares.
    ///
    /// The second level opens on the shape rather than on the type, so this
    /// reaches it without `Vec3`, and the test that says a mixed struct does
    /// **not** open has a matching pair to compare against.
    #[derive(Reflect)]
    struct Pair {
        /// One number.
        width: f32,
        /// Another.
        height: f32,
    }

    /// The same shape with one field that is not a number.
    #[derive(Reflect)]
    struct Mixed {
        /// A number.
        width: f32,
        /// Not one.
        wide: bool,
    }

    /// A component carrying both, so one entity answers both questions.
    #[derive(Component, Reflect)]
    struct Measured {
        /// Opens into two boxes.
        pair: Pair,
        /// Stays one line of text.
        mixed: Mixed,
    }

    /// The editor, run until a test can look at the world.
    ///
    /// The same one frame `outline::tests` settles for, and for the same
    /// reason: before it there are no regions, so there is nothing to click and
    /// nowhere to draw.
    fn inspector_editor() -> App {
        let mut app = editor(headless());
        app.update();
        app
    }

    /// The inspector region.
    fn panel_of(app: &mut App) -> Entity {
        app.world_mut()
            .query::<(Entity, &Region)>()
            .iter(app.world())
            .find(|(_, region)| **region == Region::Inspector)
            .map(|(entity, _)| entity)
            .expect("the inspector region is on screen")
    }

    /// Every entity under the inspector, in the order they are drawn.
    fn under_the_panel(app: &mut App) -> Vec<Entity> {
        let panel = panel_of(app);
        let mut found = Vec::new();
        let mut stack = vec![panel];
        while let Some(entity) = stack.pop() {
            if entity != panel {
                found.push(entity);
            }
            if let Some(children) = app.world().entity(entity).get::<Children>() {
                let kids: Vec<Entity> = children.iter().collect();
                for child in kids.into_iter().rev() {
                    stack.push(child);
                }
            }
        }
        found
    }

    /// Every word the inspector has on screen, in the order they are drawn.
    ///
    /// The first is the header's; the rest are the rows.
    fn on_screen(app: &mut App) -> Vec<String> {
        under_the_panel(app)
            .into_iter()
            .filter_map(|entity| {
                app.world()
                    .entity(entity)
                    .get::<Text>()
                    .map(|text| text.0.clone())
            })
            .collect()
    }

    /// What the header calls the active entity, or `None` when the panel is
    /// empty.
    fn title(app: &mut App) -> Option<String> {
        on_screen(app).first().cloned()
    }

    /// Every word under the header, the field rows included.
    fn rows(app: &mut App) -> Vec<String> {
        on_screen(app).into_iter().skip(1).collect()
    }

    /// Every word under one entity, in the order it is drawn.
    fn text_under(world: &World, entity: Entity) -> Vec<String> {
        let mut found = Vec::new();
        let mut stack = vec![entity];
        while let Some(next) = stack.pop() {
            if let Some(text) = world.entity(next).get::<Text>() {
                found.push(text.0.clone());
            }
            if let Some(children) = world.entity(next).get::<Children>() {
                let kids: Vec<Entity> = children.iter().collect();
                for child in kids.into_iter().rev() {
                    stack.push(child);
                }
            }
        }
        found
    }

    /// One entry per component row, in the order they are drawn.
    ///
    /// The panel is a tree now, so this is the level `docs/specs/ui.md` §6
    /// sorts: the component's own name, without the lines under it.
    fn components(app: &mut App) -> Vec<String> {
        let panel = panel_of(app);
        let world = app.world();
        let Some(children) = world.entity(panel).get::<Children>() else {
            return Vec::new();
        };
        // The header is the first child of the panel and the body the second,
        // in the order `show` spawns them.
        let Some(body) = children.iter().nth(1) else {
            return Vec::new();
        };
        let rows: Vec<Entity> = world
            .entity(body)
            .get::<Children>()
            .map(|rows| rows.iter().collect())
            .unwrap_or_default();
        rows.into_iter()
            .filter_map(|row| text_under(world, row).first().cloned())
            .collect()
    }

    /// The lines under one component, each as the words in it.
    ///
    /// A named field is two words, the name and the value; a component that is
    /// not a named struct is one, the value alone.
    fn lines_under(app: &mut App, component: &str) -> Vec<Vec<String>> {
        let panel = panel_of(app);
        let world = app.world();
        let body = world
            .entity(panel)
            .get::<Children>()
            .and_then(|children| children.iter().nth(1))
            .expect("the panel has a body");
        let rows: Vec<Entity> = world
            .entity(body)
            .get::<Children>()
            .map(|rows| rows.iter().collect())
            .unwrap_or_default();
        let row = rows
            .into_iter()
            .find(|row| {
                text_under(world, *row)
                    .first()
                    .is_some_and(|first| first == component)
            })
            .unwrap_or_else(|| panic!("no component row says {component}"));
        world
            .entity(row)
            .get::<Children>()
            .map(|lines| {
                lines
                    .iter()
                    // The first child is the component's own name.
                    .skip(1)
                    .map(|line| text_under(world, line))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The line one field draws, as one entity.
    ///
    /// The lines under a component are its row's children after the first,
    /// which is the component's own name, and they are in the order the type
    /// declares its fields.
    fn line_of(app: &mut App, component: &str, index: usize) -> Entity {
        let panel = panel_of(app);
        let world = app.world();
        let body = world
            .entity(panel)
            .get::<Children>()
            .and_then(|children| children.iter().nth(1))
            .expect("the panel has a body");
        let row = world
            .entity(body)
            .get::<Children>()
            .map(|rows| rows.iter().collect::<Vec<Entity>>())
            .unwrap_or_default()
            .into_iter()
            .find(|row| {
                text_under(world, *row)
                    .first()
                    .is_some_and(|first| first == component)
            })
            .unwrap_or_else(|| panic!("no component row says {component}"));
        world
            .entity(row)
            .get::<Children>()
            .and_then(|children| children.iter().nth(index + 1))
            .unwrap_or_else(|| panic!("{component} has no line {index}"))
    }

    /// The number boxes on one line, in the order they are drawn.
    ///
    /// A box carries no `Text`, so nothing that reads the panel's words can
    /// see it: what it holds is in `EditableText`, on a child of the entity
    /// the box's own components sit on.
    fn boxes_on(app: &mut App, line: Entity) -> Vec<Entity> {
        let world = app.world();
        world
            .entity(line)
            .get::<Children>()
            .map(|children| {
                children
                    .iter()
                    .filter(|child| world.entity(*child).contains::<FeathersNumberInput>())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// What the boxes on one line read, in the order they are drawn.
    fn numbers_on(app: &mut App, line: Entity) -> Vec<String> {
        boxes_on(app, line)
            .into_iter()
            .map(|cell| {
                let inner = editable_under(app.world(), cell);
                app.world()
                    .entity(inner)
                    .get::<EditableText>()
                    .expect("the buffer is on the entity that was found by it")
                    .value()
                    .to_string()
            })
            .collect()
    }

    /// The entity inside a box that holds the text being edited.
    ///
    /// `bevy_feathers` puts `EditableText` on a child of the box rather than
    /// on the box itself, and focus is about that child.
    fn editable_under(world: &World, cell: Entity) -> Entity {
        let mut stack = vec![cell];
        while let Some(next) = stack.pop() {
            if next != cell && world.entity(next).contains::<EditableText>() {
                return next;
            }
            if let Some(children) = world.entity(next).get::<Children>() {
                for child in children.iter() {
                    stack.push(child);
                }
            }
        }
        panic!("the box has nothing to type into");
    }

    /// Type into a box, and commit it by letting the focus go.
    ///
    /// The whole path, rather than a `ValueChange` made up here: the buffer is
    /// edited, the box takes focus, and the focus leaves, which is one of the
    /// two things `docs/specs/ui.md` §7 commits on.
    fn type_and_commit(app: &mut App, cell: Entity, text: &str) {
        type_into(app, cell, text);
        let inner = editable_under(app.world(), cell);
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(inner, FocusCause::Pressed);
        app.update();
        app.world_mut().resource_mut::<InputFocus>().clear();
        app.update();
        app.update();
    }

    /// Put something in a box without committing it.
    fn type_into(app: &mut App, cell: Entity, text: &str) {
        let inner = editable_under(app.world(), cell);
        let mut entity = app.world_mut().entity_mut(inner);
        let mut buffer = entity
            .get_mut::<EditableText>()
            .expect("the buffer is on the entity that was found by it");
        buffer.queue_edit(TextEdit::SelectAll);
        buffer.queue_edit(TextEdit::Insert(text.into()));
        app.update();
        app.update();
    }

    /// Select the middle placeholder, and say which entity it is.
    fn select_the_middle(app: &mut App) -> Entity {
        click_at(app, in_window(Vec2::ZERO), PointerButton::Primary);
        let selection = app.world().resource::<Selection>().entities();
        assert_eq!(selection.len(), 1, "the click selected nothing");
        selection[0]
    }

    /// Select the middle placeholder and then the left one, and say which they
    /// are, in the order they were chosen.
    ///
    /// Two rather than one, per RK-007: with one entity selected, `.last()` and
    /// `.first()` are the same program.
    fn select_the_middle_then_the_left(app: &mut App) -> [Entity; 2] {
        click_at(app, in_window(Vec2::ZERO), PointerButton::Primary);
        hold_key(app, KeyCode::ControlLeft);
        click_at(
            app,
            in_window(Vec2::new(-200.0, 0.0)),
            PointerButton::Primary,
        );
        release_key(app, KeyCode::ControlLeft);
        let selection = app.world().resource::<Selection>().entities();
        assert_eq!(selection.len(), 2, "two clicks did not select two things");
        [selection[0], selection[1]]
    }

    /// The inspector shows the last entity chosen.
    ///
    /// The first test in the workspace of the ordering `Selection` documents.
    /// Two entities are selected because one guards nothing here: with a single
    /// element, taking the last and taking the first are the same program,
    /// which is RK-007.
    ///
    /// Mutation: replace `.last()` with `.first()` in `show`, and this fails
    /// naming the middle placeholder.
    #[test]
    fn the_inspector_shows_the_last_entity_chosen() {
        let mut app = inspector_editor();
        let [middle, left] = select_the_middle_then_the_left(&mut app);

        assert_eq!(
            title(&mut app),
            Some(format!("Entity {left} (1 of 2 selected)"))
        );
        assert_ne!(
            title(&mut app),
            Some(format!("Entity {middle} (1 of 2 selected)")),
            "the inspector is showing the first entity chosen"
        );
    }

    /// Choosing a different entity changes what is shown.
    ///
    /// The other half of "the last one chosen": a panel that showed the right
    /// entity once and then stopped answering would pass the test above.
    ///
    /// Mutation: return early in `show` when `Shown` already holds something,
    /// and this fails with the first placeholder still named.
    #[test]
    fn choosing_a_different_entity_changes_what_is_shown() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        let before = title(&mut app);
        assert_eq!(before, Some(format!("Entity {middle}")));

        click_at(
            &mut app,
            in_window(Vec2::new(-200.0, 0.0)),
            PointerButton::Primary,
        );

        let after = title(&mut app);
        assert_ne!(after, before, "the header did not follow the selection");
        assert!(after.is_some(), "the inspector went empty instead");
    }

    /// The inspector is empty when nothing is selected.
    ///
    /// The position clicked is inside the viewport region and away from every
    /// placeholder, which `docs/specs/ui.md` §4 binds to clearing the
    /// selection.
    ///
    /// Mutation: leave the children in place when the wanted shape is `None`,
    /// and this fails with the previous entity's components still on screen.
    #[test]
    fn the_inspector_is_empty_when_nothing_is_selected() {
        let mut app = inspector_editor();
        select_the_middle(&mut app);
        assert!(
            !on_screen(&mut app).is_empty(),
            "nothing was shown to clear"
        );

        click_at(
            &mut app,
            in_window(Vec2::new(0.0, -180.0)),
            PointerButton::Primary,
        );

        assert!(
            app.world().resource::<Selection>().entities().is_empty(),
            "the click did not clear the selection"
        );
        assert_eq!(
            on_screen(&mut app),
            Vec::<String>::new(),
            "the inspector outlived the selection"
        );
    }

    /// The inspector lists the components the entity has.
    ///
    /// Asserted against names written out here rather than against the list the
    /// code produced, which is RK-001: a test that reads its expectation from
    /// the thing under test agrees with it whatever it means. The count is
    /// asserted as non-empty for the same reason.
    ///
    /// Mutation: drop either `Sprite` or `Transform` from what `show` collects,
    /// and this fails naming it.
    #[test]
    fn the_inspector_lists_the_components_the_entity_has() {
        let mut app = inspector_editor();
        select_the_middle(&mut app);

        let listed = rows(&mut app);

        assert!(!listed.is_empty(), "the inspector listed nothing");
        for wanted in ["Sprite", "Transform"] {
            assert!(
                listed.iter().any(|row| row == wanted),
                "{wanted} is not in {listed:?}"
            );
        }
    }

    /// A component the inspector does not know still appears.
    ///
    /// `Unheard` is declared in this module and registered by nothing. It
    /// appears because it derives `Reflect` and `reflect_auto_register` is on,
    /// which is the mechanism a user's own components will arrive by in phase
    /// 3, rather than a convenience of the test.
    ///
    /// Mutation: key the rows on a fixed list of component types in `show`, and
    /// this fails.
    #[test]
    fn a_component_the_inspector_does_not_know_still_appears() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        app.world_mut().entity_mut(middle).insert(Unheard);
        app.update();

        let listed = rows(&mut app);

        assert!(
            listed.iter().any(|row| row == "Unheard"),
            "a component this code never heard of is not in {listed:?}"
        );
    }

    /// A component with no reflection appears as unregistered.
    ///
    /// `Selectable` is one: it has a `TypeId` and no registration, and in this
    /// workspace's feature configuration there is no name to fall back to. The
    /// row says so rather than vanishing, which is `docs/specs/ui.md` §6.
    ///
    /// Mutation: skip the `Row::Unregistered` arm in `show`, and this fails
    /// with the entity one row shorter than the components it carries.
    #[test]
    fn a_component_with_no_reflection_appears_as_unregistered() {
        let mut app = inspector_editor();
        select_the_middle(&mut app);

        let listed = rows(&mut app);

        assert!(
            listed.iter().any(|row| row == UNREGISTERED),
            "nothing said the entity carries a component with no reflection: {listed:?}"
        );
    }

    /// The rows are in the order the names sort in.
    ///
    /// Two claims, because the archetype's own order satisfies neither: it is
    /// the type registration order, which is plugin build order, and it puts
    /// `GlobalTransform` before `Transform`.
    ///
    /// Mutation: drop the `rows.sort_by` in `show`, and this fails. Mutation:
    /// drop the `bool` from [`Row::key`], so that `<no reflection>` sorts
    /// among the names rather than after them, and the second half fails.
    ///
    /// It reads the component level rather than every word on screen, because
    /// a component opens into field rows now and those are in the order the
    /// type declares its fields: `translation, rotation, scale` is what
    /// `Transform` means, and sorting it would be sorting away the meaning.
    #[test]
    fn the_rows_are_in_the_order_the_names_sort_in() {
        let mut app = inspector_editor();
        select_the_middle(&mut app);

        let listed = components(&mut app);
        let named: Vec<&String> = listed.iter().filter(|row| *row != UNREGISTERED).collect();
        let mut sorted = named.clone();
        sorted.sort();

        assert!(
            named.len() > 1,
            "one row cannot be out of order: {listed:?}"
        );
        assert_eq!(named, sorted, "the named rows are not in name order");

        let last_named = listed.iter().rposition(|row| row != UNREGISTERED);
        let first_unregistered = listed.iter().position(|row| row == UNREGISTERED);
        assert!(
            first_unregistered.is_some(),
            "there is no unregistered row to place"
        );
        assert!(
            first_unregistered > last_named,
            "an unregistered row is above a named one: {listed:?}"
        );
    }

    /// An entity despawned while selected leaves nothing behind.
    ///
    /// **One claim, not two.** The panel empties. That nothing panics on the
    /// way is held by `forget_what_is_gone` in `selection.rs` and not by
    /// anything here: it clears the selection at the despawn, so `show` never
    /// looks up a dead entity, and replacing the lookup's `.ok()?` with
    /// `.expect()` leaves this and the other ten tests in this module green,
    /// measured. The comment on that line says so rather than letting this name
    /// imply otherwise.
    ///
    /// Mutation: despawn the panel's children only when there is something to
    /// put back, and this fails with the dead entity's components still on
    /// screen.
    #[test]
    fn an_entity_despawned_while_selected_leaves_nothing_behind() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        assert!(!on_screen(&mut app).is_empty(), "nothing was shown to lose");

        app.world_mut().entity_mut(middle).despawn();
        app.update();

        assert_eq!(
            on_screen(&mut app),
            Vec::<String>::new(),
            "the despawned entity's components are still on screen"
        );
    }

    /// The header uses the entity's name when it has one.
    ///
    /// The other side of `Entity 356v0`. Nothing in the world carries a `Name`
    /// yet, so without this the `Name` branch is written and unguarded, which
    /// is the shape RK-006 describes: a fixture that cannot reach a path makes
    /// the tests around it read as coverage.
    ///
    /// Mutation: drop the `found.get::<Name>()` in `show` and always format the
    /// entity, and this fails.
    #[test]
    fn the_header_uses_the_entitys_name_when_it_has_one() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        app.world_mut()
            .entity_mut(middle)
            .insert(Name::new("Player"));
        app.update();

        assert_eq!(title(&mut app), Some("Player".to_owned()));
    }

    /// The inspector does not rebuild what has not changed.
    ///
    /// Read through the entity ids under the panel: a rebuild despawns them and
    /// spawns new ones, so ids that survive two updates are a panel that was
    /// left alone. Without this the panel is destroyed and recreated every
    /// frame, which is what #44's input field would be handed.
    ///
    /// Mutation: remove the `shown.0 == wanted` guard in `show`, and this
    /// fails.
    #[test]
    fn the_inspector_does_not_rebuild_what_has_not_changed() {
        let mut app = inspector_editor();
        select_the_middle(&mut app);
        let before = under_the_panel(&mut app);
        assert!(!before.is_empty(), "nothing was built to keep");

        app.update();
        app.update();

        assert_eq!(
            under_the_panel(&mut app),
            before,
            "the inspector rebuilt a panel nothing had changed"
        );
    }

    /// The panel is rebuilt when the region it draws into is replaced.
    ///
    /// The compared value is a claim about three things, and this is the third:
    /// the content, the entity it is about, and the pane its children were
    /// spawned under. Here the content and the entity are both the same and
    /// only the pane differs, so a comparison that carried the first two and
    /// not the third would say "already shown" and leave the new pane blank.
    /// Nothing else in this module ever hands `show` a second region, which is
    /// why nothing else can reach it.
    ///
    /// The same argument one level in, about the entity, is on `Shape::of`,
    /// which no test reaches any more and which says so.
    ///
    /// **Nothing in the editor can reach it either**, today: `spawn_regions`
    /// runs once at `Startup`. This is written for the docking that
    /// `docs/specs/open-questions.md` §1 defers, which is exactly a pane being
    /// torn down and put back somewhere else.
    ///
    /// Mutation: drop the `into` field from `Shape`, and this fails with the
    /// replacement pane empty. Every other test in this module passes under it.
    #[test]
    fn the_panel_is_rebuilt_when_the_region_it_draws_into_is_replaced() {
        let mut app = inspector_editor();
        let chosen = select_the_middle(&mut app);
        let before = on_screen(&mut app);
        assert!(!before.is_empty(), "nothing was built to replace");

        let old = panel_of(&mut app);
        let parent = app
            .world()
            .entity(old)
            .get::<ChildOf>()
            .expect("the region hangs from the middle row")
            .parent();
        app.world_mut().entity_mut(old).despawn();
        app.world_mut().spawn((Region::Inspector, ChildOf(parent)));
        app.update();
        app.update();

        assert_eq!(
            app.world().resource::<Selection>().entities(),
            [chosen],
            "replacing the region changed the selection, so this tests something else"
        );
        assert_ne!(panel_of(&mut app), old, "the region was not replaced");
        assert_eq!(
            on_screen(&mut app),
            before,
            "the replacement pane never filled"
        );
    }

    /// Showing a selection leaves the selection as it found it.
    ///
    /// Choosing what to look at is not a selection gesture, which is the claim
    /// `docs/specs/ui.md` §5 makes about the outline. Asserted over two
    /// entities so that an order the inspector rewrote would show.
    ///
    /// Mutation: write anything to `Selection` from `show`, and this fails.
    #[test]
    fn the_inspector_leaves_the_selection_as_it_found_it() {
        let mut app = inspector_editor();
        let chosen = select_the_middle_then_the_left(&mut app);

        let before = app.world().resource::<Selection>().entities().to_vec();
        app.update();
        let after = app.world().resource::<Selection>().entities().to_vec();

        assert_eq!(before, chosen.to_vec(), "the fixture is not what it says");
        assert_eq!(before, after, "showing the inspector changed the selection");
    }

    /// A field row reads the entity's own value, not the type's default.
    ///
    /// The translation is written out here rather than read back from the
    /// component, which is RK-001: a test that takes its expectation from the
    /// thing under test agrees with it whatever it means. It is also not a
    /// value any placeholder starts with, so a row showing `Transform`'s
    /// default cannot pass.
    ///
    /// Both line shapes at once, because there are two now and each has its
    /// own arm: the boxes read the numbers, and a field that is not a number
    /// still reads its `Debug` text.
    ///
    /// Mutation: give a field row an empty value in `fields_of`, and the text
    /// half fails. Mutation: pass `NumberInputValue::F32(0.0)` rather than the
    /// leaf's own value where a box is spawned, and the numbers half does.
    /// There is no default path to mutate into: the value comes off the entity
    /// by construction, which is row 1 rather than row 2.
    #[test]
    fn a_field_row_reads_the_entitys_own_value() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        app.world_mut().entity_mut(middle).insert((
            Transform::from_xyz(12.5, -7.25, 3.0),
            Titled {
                title: "the middle one".to_owned(),
            },
        ));
        app.update();

        let numbers = line_of(&mut app, "Transform", 0);
        let text = lines_under(&mut app, "Titled");

        assert_eq!(
            lines_under(&mut app, "Transform")
                .first()
                .map(Vec::as_slice),
            Some(
                [
                    "translation".to_owned(),
                    "x".to_owned(),
                    "y".to_owned(),
                    "z".to_owned()
                ]
                .as_slice()
            ),
            "the translation line is not one box per leaf"
        );
        assert_eq!(
            numbers_on(&mut app, numbers),
            ["12.5", "-7.25", "3"],
            "the boxes are not the entity's own numbers"
        );
        assert_eq!(
            text.first().map(Vec::as_slice),
            Some(["title".to_owned(), "\"the middle one\"".to_owned()].as_slice()),
            "a field that is not a number stopped reading the entity's own value: {text:?}"
        );
    }

    /// A field row follows the component it reads.
    ///
    /// The value on screen is the one in the world in the frame it is drawn,
    /// which is what makes the panel a view rather than a snapshot taken when
    /// the selection last changed.
    ///
    /// Mutation: leave `fields` out of what `Shape` compares, and this fails
    /// with the first value still on screen. That is RK-009's rule applied to
    /// this change: the compared value has to carry everything the panel is a
    /// claim about, and the values are new in it.
    #[test]
    fn a_field_row_follows_the_component_it_reads() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        app.world_mut()
            .entity_mut(middle)
            .insert(Transform::from_xyz(1.0, 0.0, 0.0));
        app.update();
        let line = line_of(&mut app, "Transform", 0);
        let before = numbers_on(&mut app, line);

        app.world_mut()
            .entity_mut(middle)
            .insert(Transform::from_xyz(2.0, 0.0, 0.0));
        app.update();
        let line = line_of(&mut app, "Transform", 0);
        let after = numbers_on(&mut app, line);

        assert_eq!(
            before.first().map(String::as_str),
            Some("1"),
            "the fixture never showed the first value: {before:?}"
        );
        assert_eq!(
            after.first().map(String::as_str),
            Some("2"),
            "the row did not follow the component: {after:?}"
        );
    }

    /// A component that is not a named struct still produces a row.
    ///
    /// Three shapes at once, because they are three arms and not one: a tuple
    /// struct, an enum and a struct with no fields. Measured on a clicked
    /// placeholder, 7 of its 13 readable components take the unnamed line and 2
    /// more open into nothing, so all three of these are the ordinary case
    /// rather than the edge one.
    ///
    /// **`Weight` reads as its whole type path and `Anchor` does not**, which
    /// is not this code choosing: `bevy_reflect` falls back to printing the
    /// type path for a tuple struct, and an engine type escapes that by
    /// carrying `#[reflect(Debug)]`. A component a user writes without that
    /// attribute reads the way `Weight` does here, so the expectation is
    /// written out as it actually appears rather than tidied up by giving the
    /// fixture an attribute a user's component would not have.
    /// `docs/specs/ui.md` §6 carries it as an accepted risk.
    ///
    /// Mutation: return an empty `Vec` from the unnamed arm of `fields_of`, and
    /// this fails on the first two. Mutation: panic there instead, and it fails
    /// by panicking, which is the failure this criterion is against.
    #[test]
    fn a_component_that_is_not_a_named_struct_still_produces_a_row() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        app.world_mut()
            .entity_mut(middle)
            .insert((Weight(2.5), Stance::Guarding, Marked));
        app.update();

        let listed = components(&mut app);
        for wanted in ["Weight", "Stance", "Marked"] {
            assert!(
                listed.iter().any(|row| row == wanted),
                "{wanted} has no row at all: {listed:?}"
            );
        }

        assert_eq!(
            lines_under(&mut app, "Weight"),
            vec![vec!["b2d_editor::inspector::tests::Weight(2.5)".to_owned()]],
            "a tuple struct is not showing its value"
        );
        assert_eq!(
            lines_under(&mut app, "Stance"),
            vec![vec!["Guarding".to_owned()]],
            "an enum is not showing its variant"
        );
        assert_eq!(
            lines_under(&mut app, "Marked"),
            Vec::<Vec<String>>::new(),
            "a component with no fields opened into something"
        );
    }

    /// Showing a component does not change it.
    ///
    /// **The stronger half of this is the compiler's, not this test's.** `show`
    /// takes `&World`, so there is no way to write a component through
    /// reflection from it: `World::get_reflect_mut` needs `&mut World` and does
    /// not compile here, which is row 1 of `CLAUDE.md`'s list rather than row 2.
    /// What this holds is the rest of the path, the commands included, and it
    /// holds it over two entities because one guards no plural (RK-007).
    ///
    /// Mutation: insert a changed `Transform` from `show`, and this fails.
    #[test]
    fn showing_a_component_does_not_change_it() {
        let mut app = inspector_editor();
        let chosen = select_the_middle_then_the_left(&mut app);

        app.update();
        app.update();

        let where_they_are: Vec<Vec3> = chosen
            .iter()
            .map(|entity| {
                app.world()
                    .entity(*entity)
                    .get::<Transform>()
                    .expect("a placeholder has a transform")
                    .translation
            })
            .collect();
        // The two places `viewport.rs` puts them, written out rather than read
        // back after the fact: a `before` taken once the panel has already been
        // drawn cannot see a write that happened while drawing it. Measured, on
        // a version of this test that took one: inserting a `Transform` from
        // `show` left it green.
        assert_eq!(
            where_they_are,
            vec![Vec3::ZERO, Vec3::new(-200.0, 0.0, 0.0)],
            "showing the panel moved what it was showing"
        );
    }

    /// The header says how many are selected when several are.
    ///
    /// After a box drag the active entity is an arbitrary member of what the
    /// box covered, which `selection.rs` says has no order inside it. The panel
    /// carries values now, so a header naming one entity of several without
    /// saying so reads as a claim about the only thing selected.
    /// `docs/specs/ui.md` §6 has the decision, and
    /// `docs/specs/open-questions.md` §1 has the half of it still open.
    ///
    /// Mutation: drop the count from the title in `show`, and this fails.
    #[test]
    fn the_header_says_how_many_are_selected_when_several_are() {
        let mut app = inspector_editor();
        let [_, left] = select_the_middle_then_the_left(&mut app);

        assert_eq!(
            title(&mut app),
            Some(format!("Entity {left} (1 of 2 selected)"))
        );
    }

    /// The regions keep their places when the inspector is full.
    ///
    /// A panel whose content is longer than the window is what this change
    /// first put on screen: 14 components and 22 value lines, which with the
    /// header is 37 rows, in a pane 512 pixels tall.
    ///
    /// **Measured before `spawn_regions` bounded the middle row**, in the
    /// default 1280 by 720 window: one click made that row 882 pixels, which
    /// left the menu bar and the bottom panel **one pixel each** where
    /// `docs/specs/ui.md` §1 asks for 28 and 180, and moved the viewport out
    /// from under the pointer. Eight tests in this module failed with it, and
    /// none of them named the cause. Bounding the row with `min_height` alone
    /// was not enough: the row then measured 598 and the two bars 17 and 105,
    /// because the row still asked for its content and the bars shrank against
    /// it.
    ///
    /// Mutation: drop `flex_basis: px(0)` or `min_height: px(0)` from the
    /// middle row in `lib.rs`, and this fails.
    #[test]
    fn the_regions_keep_their_places_when_the_inspector_is_full() {
        let mut app = inspector_editor();
        let places = |app: &mut App| -> Vec<(Region, Vec2)> {
            let mut found: Vec<(Region, Vec2)> = app
                .world_mut()
                .query::<(&Region, &ComputedNode)>()
                .iter(app.world())
                .map(|(region, node)| (*region, node.size()))
                .collect();
            found.sort_by_key(|(region, _)| format!("{region:?}"));
            found
        };
        let empty = places(&mut app);

        select_the_middle(&mut app);

        assert!(
            components(&mut app).len() > 10,
            "the panel is not full, so this tests nothing"
        );
        assert_eq!(
            places(&mut app),
            empty,
            "a full inspector moved the regions around it"
        );
    }

    /// A row past the bottom of the panel is clipped rather than drawn over
    /// what is under it.
    ///
    /// Fourteen components open into 22 value lines, which with their own
    /// names and the header is 37 rows in a pane 512 pixels tall, so the panel
    /// is longer than the space it has from the first click. Scrolling and collapsing are both deferred in
    /// `docs/specs/ui.md` §6, and what is left is that the overflow has to stop
    /// at the pane's edge: without it the rows carry on down the window, over
    /// the bottom panel.
    ///
    /// It is read through `CalculatedClip`, which is the engine's own answer
    /// rather than the setting this code wrote: `bevy_ui` puts it on the
    /// descendants of a node that clips, and it is absent when nothing does.
    ///
    /// Mutation: drop `overflow` from the inspector region in `lib.rs`, and
    /// this fails with the rows unclipped.
    #[test]
    fn a_row_past_the_bottom_of_the_panel_is_clipped() {
        let mut app = inspector_editor();
        select_the_middle(&mut app);

        let panel = panel_of(&mut app);
        let world = app.world();
        let pane = world
            .entity(panel)
            .get::<ComputedNode>()
            .expect("the pane is laid out");
        let at = world
            .entity(panel)
            .get::<UiGlobalTransform>()
            .expect("the pane is placed");
        let bottom = at.translation.y + pane.size().y / 2.0;

        let past: Vec<Entity> = under_the_panel(&mut app)
            .into_iter()
            .filter(|entity| {
                app.world()
                    .entity(*entity)
                    .get::<UiGlobalTransform>()
                    .is_some_and(|at| at.translation.y > bottom)
            })
            .collect();

        assert!(
            !past.is_empty(),
            "nothing is past the bottom of the pane, so this tests nothing"
        );
        for entity in past {
            let clip = app
                .world()
                .entity(entity)
                .get::<bevy::ui::CalculatedClip>()
                .map(|clip| clip.clip);
            assert!(
                clip.is_some_and(|clip| clip.max.y <= bottom),
                "a row below the pane is not clipped to it: {clip:?}"
            );
        }
    }

    /// A field line carries its indent on the row itself.
    ///
    /// The indent is patched onto the row's own scene rather than put on a node
    /// wrapping it, which is what `bsn!` composition is for: neither `field_row`
    /// nor `label_dim` sets `padding`, so the caller's `Node` merges into the
    /// row's. **Measured**: a wrapper entity per line cost 119 entities under
    /// the panel against 97, one extra per value line, and ADR-0002 spawns and
    /// despawns all of them on every rebuild.
    ///
    /// Read through both line shapes, because there are two now: a line of
    /// text, and a line of number boxes. They are spawned by different arms of
    /// `show` and each one could grow a wrapper on its own.
    ///
    /// Mutation: spawn the indent as a node of its own and parent either line
    /// under it, and this fails with that line's own padding at zero.
    #[test]
    fn a_field_line_carries_its_indent_on_the_row_itself() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        app.world_mut().entity_mut(middle).insert(Titled {
            title: "the middle one".to_owned(),
        });
        app.update();

        let text = line_of(&mut app, "Titled", 0);
        let numbers = line_of(&mut app, "Transform", 0);

        for (line, what) in [(text, "a line of text"), (numbers, "a line of boxes")] {
            assert_eq!(
                app.world()
                    .entity(line)
                    .get::<Node>()
                    .expect("a row is a node")
                    .padding
                    .left,
                Val::Px(12.0),
                "{what} does not carry the indent on this entity at all"
            );
        }

        // **What the line draws has to hang directly off it**, and reading the
        // padding alone does not say that: a node wrapping the row carries the
        // same padding and the same words underneath it, so the assertion
        // above holds under either shape. Measured, on a version of this test
        // that stopped there: the wrapper form left all 93 green. That is
        // RK-012.
        assert_eq!(
            directly_under(app.world(), text),
            vec!["\"the middle one\"".to_owned()],
            "the indent is on a node wrapping the line of text rather than on it"
        );
        assert_eq!(
            directly_under(app.world(), numbers),
            vec!["x".to_owned(), "y".to_owned(), "z".to_owned()],
            "the indent is on a node wrapping the line of boxes rather than on it"
        );
        assert_eq!(
            boxes_on(&mut app, numbers).len(),
            3,
            "the boxes are not children of the line either"
        );
    }

    /// The words on the direct children of one entity, in the order they are
    /// drawn.
    ///
    /// Direct rather than every descendant, which is the whole point: a
    /// flattened read is satisfied by a wrapper and guards nothing about the
    /// tree. RK-012.
    fn directly_under(world: &World, entity: Entity) -> Vec<String> {
        world
            .entity(entity)
            .get::<Children>()
            .map(|children| {
                children
                    .iter()
                    .filter_map(|child| {
                        world.entity(child).get::<Text>().map(|text| text.0.clone())
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// A window position inside the inspector pane.
    ///
    /// The pane is 300 wide at the right-hand end of a 1280 window and starts
    /// 28 down, which `docs/specs/ui.md` §1 lays out and `spawn_regions`
    /// builds. Spelled out here for the reason `in_window` gives: a test that
    /// computes it the way the editor does agrees with the editor rather than
    /// with the drawing.
    fn in_inspector() -> Vec2 {
        Vec2::new(1100.0, 200.0)
    }

    /// Where a row sits on screen, by the words on it.
    fn top_of(app: &mut App, row: &str) -> f32 {
        let found = under_the_panel(app)
            .into_iter()
            .find(|entity| {
                app.world()
                    .entity(*entity)
                    .get::<Text>()
                    .is_some_and(|text| text.0 == row)
            })
            .unwrap_or_else(|| panic!("no row says {row}"));
        app.world()
            .entity(found)
            .get::<UiGlobalTransform>()
            .expect("a row is placed")
            .translation
            .y
    }

    /// What the viewport camera is showing, so that a wheel that reached it
    /// would be visible here.
    fn world_per_pixel_of(app: &mut App) -> f32 {
        let projection = app
            .world_mut()
            .query_filtered::<&Projection, With<crate::ViewportCamera>>()
            .single(app.world())
            .expect("there is one viewport camera");
        let Projection::Orthographic(orthographic) = projection else {
            panic!("the viewport camera is not orthographic");
        };
        orthographic.scale
    }

    /// Where the inspector's contents have been scrolled to.
    fn scrolled_to(app: &mut App) -> f32 {
        let panel = panel_of(app);
        app.world()
            .entity(panel)
            .get::<ScrollPosition>()
            .expect("the pane scrolls")
            .y
    }

    /// The wheel scrolls the panel.
    ///
    /// The panel is longer than the pane from the first click, so the rows past
    /// the bottom have to be reachable. The wheel is the engine's:
    /// `ScrollAreaPlugin` arrives with `DefaultPlugins`, and `Pointer<Scroll>`
    /// bubbles from the label under the pointer up to the pane, which is where
    /// `ScrollArea` sits.
    ///
    /// Asserted on what moved rather than on `ScrollPosition` alone: a position
    /// that changed while the rows stayed put is not scrolling.
    ///
    /// Mutation: drop `ScrollArea` from the inspector region in `lib.rs`, and
    /// this fails with the position still at the top.
    #[test]
    fn the_wheel_scrolls_the_panel() {
        let mut app = inspector_editor();
        select_the_middle(&mut app);
        let before = top_of(&mut app, "Aabb");
        assert_eq!(
            scrolled_to(&mut app),
            0.0,
            "the panel did not start at the top"
        );

        scroll_at(&mut app, in_inspector(), -5.0);

        assert!(
            scrolled_to(&mut app) > 0.0,
            "the wheel did not move the scroll position"
        );
        let after = top_of(&mut app, "Aabb");
        assert!(
            after < before,
            "the rows did not move: {before} then {after}"
        );
    }

    /// Scrolling the panel does not zoom the viewport.
    ///
    /// The viewport observes the wheel too, for the zoom `docs/specs/ui.md` §4
    /// decided.
    ///
    /// **What keeps them apart is the hierarchy, not the scrolling.** A
    /// `Pointer<Scroll>` bubbles to its ancestors, and the viewport is the
    /// inspector's sibling rather than its parent, so the wheel here never
    /// reaches `zoom` whatever this pane does with it. Measured: taking the
    /// scrolling away again, so the pane merely clips, leaves this green.
    ///
    /// It is still a guard, of the thing that would actually break it.
    /// Mutation: add `zoom` as a global observer in `ViewportPlugin` rather
    /// than attaching it to the viewport's own node, and this fails with the
    /// camera zoomed by a wheel the user turned over the inspector. Applied,
    /// and two of `viewport`'s own tests fail with it.
    #[test]
    fn scrolling_the_panel_does_not_zoom_the_viewport() {
        let mut app = inspector_editor();
        select_the_middle(&mut app);
        let before = world_per_pixel_of(&mut app);

        scroll_at(&mut app, in_inspector(), -5.0);

        assert_eq!(
            world_per_pixel_of(&mut app),
            before,
            "the wheel over the inspector reached the viewport's camera"
        );
    }

    /// Choosing another entity puts the panel back at the top.
    ///
    /// Scrolled to the bottom of one entity's components and then handed
    /// another, the panel would otherwise open part way down a list the user
    /// has not seen. It is **the entity changing** that resets it and not the
    /// rebuild: the values of the entity being looked at change on their own,
    /// `PickingInteraction` every time the pointer crosses a sprite, and a
    /// scroll that jumped to the top for those would be unusable.
    ///
    /// Mutation: reset the scroll on every rebuild rather than on a change of
    /// entity, and the second half of this fails.
    #[test]
    fn choosing_another_entity_puts_the_panel_back_at_the_top() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        scroll_at(&mut app, in_inspector(), -5.0);
        assert!(scrolled_to(&mut app) > 0.0, "nothing was scrolled to reset");

        // A rebuild of the same entity: one more component, same selection.
        app.world_mut().entity_mut(middle).insert(Unheard);
        app.update();
        assert!(
            scrolled_to(&mut app) > 0.0,
            "a rebuild of the same entity lost the scroll position"
        );

        click_at(
            &mut app,
            in_window(Vec2::new(-200.0, 0.0)),
            PointerButton::Primary,
        );

        assert_eq!(
            scrolled_to(&mut app),
            0.0,
            "the panel stayed where the last entity was scrolled to"
        );
    }

    /// A name too long for its column is cut rather than drawn over its value.
    ///
    /// `should_block_lower` on `Pickable` is the one in the tree: measured in
    /// the running editor at a 110 pixel column, it overlapped the `true`
    /// beside it and both were unreadable. The column is wider now, and it
    /// clips, so the next name that does not fit is cut instead.
    ///
    /// Read through `CalculatedClip`, which `bevy_ui` puts on the descendants
    /// of a node that clips and takes away when nothing does.
    ///
    /// Mutation: drop `overflow` from the name column in `field.rs`, and this
    /// fails.
    #[test]
    fn a_name_too_long_for_its_column_is_cut_rather_than_drawn_over_its_value() {
        let mut app = inspector_editor();
        select_the_middle(&mut app);

        let named = under_the_panel(&mut app)
            .into_iter()
            .find(|entity| {
                app.world()
                    .entity(*entity)
                    .get::<Text>()
                    .is_some_and(|text| text.0 == "should_block_lower")
            })
            .expect("Pickable opened into should_block_lower");

        let clip = app
            .world()
            .entity(named)
            .get::<bevy::ui::CalculatedClip>()
            .map(|clip| clip.clip);
        let column = clip.expect("the name is clipped by its column");
        assert!(
            column.width() <= 140.0,
            "the name is clipped by something wider than its column: {column:?}"
        );
    }

    /// A field of numbers opens into one box per leaf.
    ///
    /// `docs/specs/ui.md` §6's second level, on a struct this file declares
    /// rather than on `Vec3`: the descent opens on the shape and not on the
    /// type, and saying so with a type Bevy does not own is what shows it.
    ///
    /// Mutation: return `None` from `leaves_of` always, and this fails with
    /// the line reading its `Debug` text instead.
    #[test]
    fn a_field_of_numbers_opens_into_one_box_per_leaf() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        app.world_mut().entity_mut(middle).insert(Measured {
            pair: Pair {
                width: 4.0,
                height: 8.0,
            },
            mixed: Mixed {
                width: 4.0,
                wide: true,
            },
        });
        app.update();

        let line = line_of(&mut app, "Measured", 0);

        assert_eq!(
            directly_under(app.world(), line),
            vec!["width".to_owned(), "height".to_owned()],
            "the leaves are not labelled one per box"
        );
        assert_eq!(
            numbers_on(&mut app, line),
            ["4", "8"],
            "the boxes do not hold the numbers the field holds"
        );
    }

    /// A field that is not all numbers stays one line.
    ///
    /// The bound on the second level. `Mixed` is a named-field struct whose
    /// first field is an `f32` and whose second is not, so anything that
    /// looked at the first field and stopped would open it.
    ///
    /// Mutation: accept a struct with one field that is not an `f32` in
    /// `leaves_of`, for instance by returning the leaves it did manage to
    /// read, and this fails with boxes on a line that should be text.
    #[test]
    fn a_field_that_is_not_all_numbers_stays_one_line() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        app.world_mut().entity_mut(middle).insert(Measured {
            pair: Pair {
                width: 4.0,
                height: 8.0,
            },
            mixed: Mixed {
                width: 4.0,
                wide: true,
            },
        });
        app.update();

        let line = line_of(&mut app, "Measured", 1);

        assert!(
            boxes_on(&mut app, line).is_empty(),
            "a field with something that is not a number in it opened into boxes"
        );
        assert_eq!(
            text_under(app.world(), line).first().map(String::as_str),
            Some("mixed"),
            "the line being read is not the one this test is about"
        );
    }

    /// A number box carries what it writes to.
    ///
    /// Read off the `FeathersNumberInput` entity itself, because that is the
    /// entity `ValueChange::source` names: on the line, or on a node wrapping
    /// the box, the observer would find nothing. RK-012.
    ///
    /// Mutation: insert `Writes` on the line rather than on the box, and this
    /// fails.
    #[test]
    fn a_number_box_carries_what_it_writes_to() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);

        let line = line_of(&mut app, "Transform", 0);
        let cells = boxes_on(&mut app, line);

        assert_eq!(cells.len(), 3, "translation did not open into three boxes");
        let leaves: Vec<String> = cells
            .iter()
            .map(|cell| {
                let writes = app
                    .world()
                    .entity(*cell)
                    .get::<Writes>()
                    .expect("a box says what it writes to, on the box itself");
                assert_eq!(writes.of, middle, "a box names the wrong entity");
                assert_eq!(
                    writes.component,
                    core::any::TypeId::of::<Transform>(),
                    "a box names the wrong component"
                );
                assert_eq!(writes.field, "translation", "a box names the wrong field");
                writes.leaf.clone()
            })
            .collect();
        assert_eq!(leaves, ["x", "y", "z"]);
    }

    /// Committing a field writes it to the component.
    ///
    /// The whole path: the buffer is edited, the box takes focus, and the
    /// focus leaves, which is one of the two things `docs/specs/ui.md` §7
    /// commits on.
    ///
    /// Mutation: replace the `try_apply` in `write_leaf` with `Ok(())`, and
    /// this fails with the placeholder still where it started.
    #[test]
    fn committing_a_field_writes_it_to_the_component() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);

        let line = line_of(&mut app, "Transform", 0);
        let cell = boxes_on(&mut app, line)[1];
        type_and_commit(&mut app, cell, "-42.5");

        assert_eq!(
            app.world()
                .entity(middle)
                .get::<Transform>()
                .expect("a placeholder has a transform")
                .translation,
            Vec3::new(0.0, -42.5, 0.0),
            "the committed number did not reach the component"
        );
    }

    /// A value that is not committed leaves the component alone.
    ///
    /// `bevy_feathers` emits a value on every keystroke, carrying
    /// `is_final: false`, and once more when `UpdateNumberInput` fills a box
    /// the panel has just spawned. Neither is a commit.
    ///
    /// Mutation: drop the `is_final` check in `commit`, and this fails with
    /// the placeholder already moved.
    #[test]
    fn a_value_that_is_not_committed_leaves_the_component_alone() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);

        let line = line_of(&mut app, "Transform", 0);
        let cell = boxes_on(&mut app, line)[1];
        type_into(&mut app, cell, "-42.5");

        assert_eq!(
            app.world()
                .entity(middle)
                .get::<Transform>()
                .expect("a placeholder has a transform")
                .translation,
            Vec3::ZERO,
            "typing moved the placeholder before anything was committed"
        );
    }

    /// A value that is not a number is never written.
    ///
    /// `bevy_feathers` refuses every character that is not a digit or one of
    /// `.-+eE`, so what can get this far is something like `1.2.3`, and it
    /// emits nothing at all for it. **Measured with a throwaway probe**: the
    /// same run that heard `(12.5, false)` and `(12.5, true)` for `12.5` heard
    /// nothing at either point for `1.2.3`.
    ///
    /// Saying so in the row is upstream's open question and is not guarded
    /// here, which `docs/specs/ui.md` §7 records.
    ///
    /// **No mutation of this file makes this fail, and that is what it is
    /// for.** Measured: with the `is_final` check deleted, so that `commit`
    /// writes whatever it is handed, this still passed, because for `1.2.3`
    /// `bevy_feathers` hands it nothing. What it pins is the upstream
    /// behaviour `docs/specs/ui.md` §7's rationale rests on. A release that
    /// started emitting a parsed-as-far-as-possible value, or a zero, would
    /// put that value into somebody's component, which is row 6, and this is
    /// what would say so. Written down rather than left to a green suite to
    /// imply a guard, per RK-005.
    #[test]
    fn an_unparseable_value_is_never_written() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        app.world_mut()
            .entity_mut(middle)
            .insert(Transform::from_xyz(0.0, 7.5, 0.0));
        // Twice, because the first frame draws the new `Transform` beside a
        // `GlobalTransform` that has not caught up yet and the second one
        // rebuilds the panel again. A box taken from the first would be
        // despawned before it could be typed into.
        app.update();
        app.update();

        let line = line_of(&mut app, "Transform", 0);
        let cell = boxes_on(&mut app, line)[1];
        type_and_commit(&mut app, cell, "1.2.3");

        assert_eq!(
            app.world()
                .entity(middle)
                .get::<Transform>()
                .expect("a placeholder has a transform")
                .translation,
            Vec3::new(0.0, 7.5, 0.0),
            "something that is not a number reached the component"
        );
    }

    /// An edit touches one field of one entity.
    ///
    /// Two entities are selected, per RK-007: with one, "the active entity"
    /// and "every selected entity" are the same program. The whole world's
    /// transforms are compared, so an edit that reached anything else shows
    /// up wherever it landed.
    ///
    /// Mutation: have `write_leaf` write every leaf of the field rather than
    /// the one `writes.leaf` names, and this fails on the leaf half. Mutation:
    /// give `Writes::of` the first selected entity rather than
    /// `Shape::of`, and it fails on the entity half. Both applied, and this
    /// test watched to fail.
    #[test]
    fn an_edit_touches_one_field_of_one_entity() {
        let mut app = inspector_editor();
        let [_, active] = select_the_middle_then_the_left(&mut app);

        let transforms = |app: &mut App| -> Vec<(Entity, Transform)> {
            let mut found: Vec<(Entity, Transform)> = app
                .world_mut()
                .query::<(Entity, &Transform)>()
                .iter(app.world())
                .map(|(entity, at)| (entity, *at))
                .collect();
            found.sort_by_key(|(entity, _)| *entity);
            found
        };
        let before = transforms(&mut app);

        let line = line_of(&mut app, "Transform", 0);
        let cell = boxes_on(&mut app, line)[1];
        type_and_commit(&mut app, cell, "-42.5");
        let after = transforms(&mut app);

        assert_eq!(
            before.len(),
            after.len(),
            "the edit added or removed something with a transform"
        );
        let moved: Vec<Entity> = before
            .iter()
            .zip(&after)
            .filter(|((_, was), (_, now))| was.translation != now.translation)
            .map(|((entity, _), _)| *entity)
            .collect();
        assert_eq!(
            moved,
            [active],
            "the edit did not land on the active entity alone"
        );
        let (_, now) = after
            .iter()
            .find(|(entity, _)| *entity == active)
            .expect("the active entity still has a transform");
        let (_, was) = before
            .iter()
            .find(|(entity, _)| *entity == active)
            .expect("the active entity had a transform");
        assert_eq!(
            (now.rotation, now.scale),
            (was.rotation, was.scale),
            "the edit reached a field it was not committed on"
        );
        assert_eq!(
            (now.translation.x, now.translation.z),
            (was.translation.x, was.translation.z),
            "the edit reached a leaf it was not committed on"
        );
    }

    /// The row shows what the component holds after a commit.
    ///
    /// The read path and the write path disagreeing is what row 6 of
    /// `CLAUDE.md`'s list is about, in miniature. Once focus has left, the
    /// panel is rebuilt from the world, so the box has to come back up holding
    /// what was actually written.
    ///
    /// Mutation: drop the `UpdateNumberInput` trigger where a box is spawned,
    /// and this fails with an empty box.
    #[test]
    fn the_row_shows_what_the_component_holds_after_a_commit() {
        let mut app = inspector_editor();
        select_the_middle(&mut app);

        let line = line_of(&mut app, "Transform", 0);
        let cell = boxes_on(&mut app, line)[1];
        type_and_commit(&mut app, cell, "-42.5");

        let line = line_of(&mut app, "Transform", 0);
        assert_eq!(
            numbers_on(&mut app, line),
            ["0", "-42.5", "0"],
            "the row and the component do not agree after a commit"
        );
    }

    /// A focused field survives the edit it commits.
    ///
    /// Enter commits without the focus going anywhere, so the box the user is
    /// in is still the box they are in afterwards. `Shape` carries the values,
    /// so without the guard in `show` the commit changes `Transform`, the
    /// panel is rebuilt, and the box is despawned under the cursor.
    /// [ADR-0003](../../../docs/adr/0003-the-inspector-panel-belongs-to-the-user-while-focus-is-in-it.md)
    /// is where that was weighed.
    ///
    /// Mutation: delete the focus guard at the top of `show`, and this fails
    /// with the box gone.
    #[test]
    fn a_focused_field_survives_the_edit_it_commits() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);

        let line = line_of(&mut app, "Transform", 0);
        let cell = boxes_on(&mut app, line)[1];
        let inner = editable_under(app.world(), cell);
        type_into(&mut app, cell, "-42.5");
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(inner, FocusCause::Pressed);
        app.update();
        hold_key(&mut app, KeyCode::Enter);
        app.update();
        app.update();
        release_key(&mut app, KeyCode::Enter);
        app.update();

        assert_eq!(
            app.world()
                .entity(middle)
                .get::<Transform>()
                .expect("a placeholder has a transform")
                .translation,
            Vec3::new(0.0, -42.5, 0.0),
            "Enter did not commit, so this says nothing about surviving it"
        );
        assert!(
            app.world().get_entity(cell).is_ok(),
            "the box was despawned by the rebuild its own commit caused"
        );
        assert_eq!(
            app.world().resource::<InputFocus>().get(),
            Some(inner),
            "the focus did not survive either"
        );
    }

    /// The panel is frozen while a box has focus, and catches up after.
    ///
    /// The cost of ADR-0003, stated as a guard rather than left to be
    /// discovered: every other line is stale for as long as one box holds
    /// focus. The second half is what makes it a freeze rather than a stop.
    ///
    /// Mutation: delete the focus guard at the top of `show`, and the first
    /// assertion fails. Mutation: make that return happen whenever anything is
    /// already shown, rather than only while the focus is in the panel, and
    /// the last one does. Both applied, and this test watched to fail.
    #[test]
    fn the_panel_is_frozen_while_a_box_has_focus_and_catches_up_after() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);

        let line = line_of(&mut app, "Transform", 0);
        let cell = boxes_on(&mut app, line)[1];
        let inner = editable_under(app.world(), cell);
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(inner, FocusCause::Pressed);
        app.update();

        // Something else moves the entity while the user is typing.
        app.world_mut()
            .entity_mut(middle)
            .insert(Transform::from_xyz(0.0, 99.0, 0.0));
        app.update();
        app.update();

        assert_eq!(
            numbers_on(&mut app, line),
            ["0", "0", "0"],
            "the panel was rebuilt while a box had focus"
        );

        app.world_mut().resource_mut::<InputFocus>().clear();
        app.update();
        app.update();

        let line = line_of(&mut app, "Transform", 0);
        assert_eq!(
            numbers_on(&mut app, line),
            ["0", "99", "0"],
            "the panel never caught up after the focus left"
        );
    }

    /// Focus outside the panel does not freeze it.
    ///
    /// The guard is about the inspector's own subtree, not about focus in
    /// general. Anything else the editor grows that can take focus, a toolbar
    /// or a search field, would otherwise stop the panel from following the
    /// world.
    ///
    /// Mutation: return whenever `InputFocus` holds anything at all, rather
    /// than only when it is inside the panel, and this fails.
    #[test]
    fn focus_outside_the_panel_does_not_freeze_it() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);

        let elsewhere = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(elsewhere, FocusCause::Pressed);
        app.update();

        app.world_mut()
            .entity_mut(middle)
            .insert(Transform::from_xyz(0.0, 99.0, 0.0));
        app.update();
        app.update();

        let line = line_of(&mut app, "Transform", 0);
        assert_eq!(
            numbers_on(&mut app, line),
            ["0", "99", "0"],
            "focus somewhere else stopped the panel following the world"
        );
    }

    /// A panel holding a `NaN` is not rebuilt every frame.
    ///
    /// `f32`'s own `PartialEq` says `NaN` is equal to nothing, itself
    /// included, so a compared value holding one differs from itself and the
    /// panel is despawned and respawned on every update. That is row 5 of
    /// `CLAUDE.md`'s list, and it is the failure ADR-0002 rejected *rebuild
    /// every frame* over. `Leaf` compares the bits for this reason.
    ///
    /// Mutation: derive `PartialEq` on `Leaf` instead of writing it, and this
    /// fails with none of the entities surviving.
    #[test]
    fn a_panel_holding_a_nan_is_not_rebuilt_every_frame() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        app.world_mut()
            .entity_mut(middle)
            .insert(Transform::from_xyz(f32::NAN, 0.0, 0.0));
        app.update();
        app.update();

        let before = under_the_panel(&mut app);
        app.update();
        app.update();
        let after = under_the_panel(&mut app);

        assert!(
            !before.is_empty(),
            "the panel is empty, so this tests nothing"
        );
        assert_eq!(
            before, after,
            "a NaN made the panel rebuild itself every frame"
        );
    }
}
