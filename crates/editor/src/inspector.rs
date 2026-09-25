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
use bevy::clipboard::Clipboard;
use bevy::feathers::containers::{pane_body, pane_header};
use bevy::feathers::controls::{
    FeathersNumberInput, NumberFormat, NumberInputValue, UpdateNumberInput,
};
use bevy::feathers::display::{label, label_dim};
use bevy::input_focus::{InputFocus, IsFocused};
use bevy::prelude::*;
use bevy::reflect::{ReflectMut, ReflectRef};
use bevy::text::{EditableText, FontCx, LayoutCx, TextEdit};
use bevy::ui::ScrollPosition;
use bevy::ui_widgets::ValueChange;

use crate::history::{EditorCommand, History};
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

/// Whether the focus was inside the panel the last time [`show`] looked.
///
/// **The panel is held for one frame after the focus leaves it, and this is
/// what remembers that it has to be.** A box commits when it loses focus, and
/// the two do not happen in the same place in the frame: the engine clears
/// `InputFocus` in `PreUpdate`, `show` runs in `Update`, and `FocusLost`
/// reaches the widget in `PostUpdate`. Without this, clicking away from a box
/// rebuilds the panel in between, so `bevy_feathers` finds a despawned entity,
/// emits nothing, and the number the user typed is discarded in silence.
///
/// Measured before this existed: typing into a box and clicking a different
/// placeholder left the component untouched, while clearing `InputFocus` by
/// hand in a world where nothing else changed committed correctly. The suite
/// was green, because the only test of this path did the second thing.
///
/// What it costs is one frame in which the panel is a frame out of date, which
/// nobody can see, and it is written on
/// [ADR-0003](../../../docs/adr/0003-the-inspector-panel-belongs-to-the-user-while-focus-is-in-it.md).
#[derive(Resource, Default)]
struct FocusWasInside(bool);

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
        ///
        /// **Named for the thing rather than called `of`**, which [`Shape`]
        /// already uses for an `Entity`: the two are read on the same line
        /// where a box is spawned, and [`Writes`] calls this same quantity
        /// `component` too.
        component: TypeId,
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
    /// **Only the value half of this is guarded, and the name half cannot
    /// be.** Measured: deleting the name comparison leaves the whole suite
    /// green. Nothing can reach it either, because a leaf's name comes from
    /// the field names of the component's own type, so for two leaves in the
    /// same position to differ by name the component would have to be a
    /// different type, which [`Row`] already compares. It is written out
    /// rather than dropped because a `PartialEq` that ignores half of what it
    /// is defined over is a trap for whoever adds the next field, and saying
    /// so beats letting a green suite imply a guard, which is RK-005.
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
            .init_resource::<FocusWasInside>()
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
    was_inside: Res<FocusWasInside>,
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
    // **And for one frame after it leaves**, which is [`FocusWasInside`] and
    // is the difference between a box that commits when the user clicks away
    // and one that throws their number away. The engine clears the focus in
    // `PreUpdate` and the widget hears about it in `PostUpdate`, so this
    // system sees no focus at all in the frame that has to keep the box alive.
    //
    // `Shown` is left unwritten either way. Writing it would record a picture
    // that was never drawn, and the panel would then compare equal to
    // something that is not on screen, which is what `Shape::into` exists to
    // prevent one level out.
    let inside = world.is_focus_within(panel);
    if inside != was_inside.0 {
        commands.insert_resource(FocusWasInside(inside));
    }
    if inside || was_inside.0 {
        return;
    }

    // The last element is the one Unity calls active, and `Selection` in
    // `selection.rs` is where that condition is written down; this is its first
    // reader.
    //
    // **It says less than it looks.** After a click the last element is the
    // one just clicked. After a box drag it is the front-most of what the box
    // covered, ranked by `z` as `docs/specs/ui.md` §4 decided, **except where
    // the box covered several at one depth**: a tie keeps the order the world
    // iterated in, which is arbitrary. Every placeholder is at `z == 0`, so
    // for them it still is, and an edit committed through a field row below
    // can land on an entity the user did not single out. The header's count
    // is what makes that visible.
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
                        component: of,
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
        // ranks a boxed group by depth and leaves a tie in the world's order,
        // so there is no index that would mean anything to report. `docs/specs/ui.md` §6 has why the panel says it at all.
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
                Row::Named {
                    name,
                    component,
                    fields,
                } => {
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
                                                component: *component,
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
/// The empty struct answers `no` rather than an empty line of boxes. Every
/// field of it is an `f32`, vacuously, so the question alone would open it into
/// nothing at all: a name with a gap beside it, where the line used to read the
/// value's own text.
///
/// Mutation: return `None` always, and
/// `a_field_of_numbers_opens_into_one_box_per_leaf` fails. Mutation: accept a
/// struct with one field that is not an `f32`, and
/// `a_field_that_is_not_all_numbers_stays_one_line` fails. Mutation: drop the
/// `field_len() == 0` check, and `a_field_with_nothing_in_it_stays_one_line`
/// fails.
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

/// Commit what a number box holds, when the user says so.
///
/// **`is_final` is the whole shape of this.** It is Enter, or the box losing
/// focus. `bevy_feathers` also emits on every keystroke with it false, and once
/// more when [`UpdateNumberInput`] fills a box the panel has just spawned:
/// measured with a throwaway probe, pushing `12.5` into a fresh box produced a
/// `ValueChange` carrying `12.5` straight back out. Acting on those would move
/// the sprite through `-`, `-1`, `-12` on the way to `-120`, each of them an
/// entry in the history, and would make the panel's own initial draw write to
/// the world.
/// [`docs/specs/ui.md` §7](../../../docs/specs/ui.md) carries why.
///
/// The write goes through [`History::record`], and only when [`SetLeaf::read`]
/// finds the value different from what is there.
///
/// Mutation: drop the `is_final` check, and
/// `a_value_that_is_not_committed_leaves_the_component_alone` fails. Mutation:
/// call [`write_leaf`] here instead of recording, and
/// `undoing_a_commit_puts_the_component_back_bit_for_bit` fails, because the
/// history is empty when Ctrl+Z reaches it.
fn commit(change: On<ValueChange<f32>>, writes: Query<&Writes>, mut commands: Commands) {
    if !change.is_final {
        return;
    }
    let Ok(target) = writes.get(change.source) else {
        return;
    };
    let target = target.clone();
    let value = change.value;
    commands.queue(move |world: &mut World| {
        if let Some(set) = SetLeaf::read(world, target, value) {
            History::record(world, set);
        }
    });
}

/// One committed number, and the number it replaced.
///
/// The command of [`docs/specs/data-model.md` §1](../../../docs/specs/data-model.md),
/// writing the component directly because there is no Editor Model yet; that
/// section says what ends that.
struct SetLeaf {
    /// Where it writes.
    target: Writes,
    /// What the leaf held before, which is what `undo` writes back.
    old: f32,
    /// What was committed.
    new: f32,
}

impl SetLeaf {
    /// The command for committing `new`, or `None` when there is nothing to
    /// record.
    ///
    /// **A commit that leaves the leaf's bits as they were is not an entry.**
    /// One Enter is two commits, because `bevy_feathers` emits on the key's
    /// press and on its release, and letting go of the box is a third.
    /// Recorded, each would be a Ctrl+Z that does nothing visible.
    /// [`docs/specs/ui.md` §8](../../../docs/specs/ui.md) has the measurement.
    ///
    /// **The old value is read here, before anything is written.**
    ///
    /// Mutation: drop the bits comparison, and
    /// `a_commit_that_changes_nothing_is_not_an_entry` fails. Mutation: read
    /// `old` after writing `new`, and
    /// `undoing_a_commit_puts_the_component_back_bit_for_bit` fails, because
    /// the undo writes back the committed value.
    fn read(world: &World, target: Writes, new: f32) -> Option<SetLeaf> {
        let old = read_leaf(world, &target)?;
        (old.to_bits() != new.to_bits()).then_some(SetLeaf { target, old, new })
    }
}

impl EditorCommand for SetLeaf {
    fn execute(&mut self, world: &mut World) {
        write_leaf(world, &self.target, self.new);
    }

    fn undo(&mut self, world: &mut World) {
        write_leaf(world, &self.target, self.old);
    }
}

/// The focused box, and its leaf's value, when what is typed in it is not
/// that value.
///
/// "Typed" is decided by comparing the box's text with the leaf: the leaf is
/// what the last commit wrote, so a box holding a number with other bits
/// holds something the user has not committed. Undo takes that back
/// ([`take_back_typing`]) and redo waits for it; both ask here, so the two
/// keys cannot disagree about what typing is.
/// [`docs/specs/ui.md` §8 and §9](../../../docs/specs/ui.md) are what this is
/// for.
///
/// **A number is compared by its bits and not by its text**, because the text
/// a commit leaves is the user's own: `10.50` committed is a leaf of `10.5`,
/// and calling that typing would make the next Ctrl+Z put back `10.5` and
/// change nothing visible. **Text that is not a number is compared with
/// [`shown_for`]**, which is what the panel itself leaves there, so the empty
/// box a value too long for it is left with is not taken for typing.
///
/// Mutation: answer `None` always, and
/// `ctrl_z_in_a_box_takes_back_what_was_typed_and_not_the_last_commit` fails.
/// Mutation: count text that does not parse as the leaf, with
/// `unwrap_or(leaf)`, and
/// `ctrl_z_in_a_box_holding_what_is_not_a_number_takes_back_the_typing`
/// fails. Mutation: count every such text as typed, and
/// `ctrl_z_in_a_box_left_empty_by_a_long_value_reaches_the_history` fails,
/// because Ctrl+Z in the empty box never reaches the history.
pub(crate) fn typing_in_focus(world: &World) -> Option<(Entity, f32)> {
    let inner = world.resource::<InputFocus>().get()?;
    let target = world
        .get::<ChildOf>(inner)
        .and_then(|parent| world.get::<Writes>(parent.parent()))?;
    let leaf = read_leaf(world, target)?;
    let text = world.get::<EditableText>(inner)?;
    let written = text.value().to_string();
    let typed = match written.trim().parse::<f32>() {
        Ok(number) => number.to_bits() != leaf.to_bits(),
        Err(_) => written != shown_for(leaf, text),
    };
    typed.then_some((inner, leaf))
}

/// Take back what is typed in the focused box, and say whether there was any.
///
/// Put back, the box reads the leaf again and the history is left alone.
/// [`typing_in_focus`] decides what counts as typed.
pub(crate) fn take_back_typing(world: &mut World) -> bool {
    let Some((inner, leaf)) = typing_in_focus(world) else {
        return false;
    };
    put_in_box(world, inner, leaf);
    true
}

/// Give every box on the panel its leaf's value, without rebuilding it.
///
/// Called after an undo. With no box focused the panel is rebuilt anyway, and
/// this is harmless. With one focused, the panel is frozen
/// ([ADR-0003](../../../docs/adr/0003-the-inspector-panel-belongs-to-the-user-while-focus-is-in-it.md)),
/// and **the focused box is the one that matters**: `UpdateNumberInput` skips
/// it, and left holding the value from before the undo it writes that value
/// back when it is let go. So its text is replaced by hand, and the others get
/// the engine's event.
///
/// Mutation: send the focused box `UpdateNumberInput` like the rest, and
/// `ctrl_z_in_a_box_with_nothing_typed_takes_back_the_last_commit` fails.
/// Mutation: do nothing for the others, and
/// `an_undo_reaches_the_boxes_that_are_not_focused` fails.
pub(crate) fn show_values_in_place(world: &mut World) {
    let focused = world.resource::<InputFocus>().get();
    let cells: Vec<(Entity, Writes)> = world
        .query::<(Entity, &Writes)>()
        .iter(world)
        .map(|(cell, writes)| (cell, writes.clone()))
        .collect();
    for (cell, target) in cells {
        let Some(value) = read_leaf(world, &target) else {
            continue;
        };
        let typing_in_it = focused.filter(|inner| {
            world
                .get::<ChildOf>(*inner)
                .is_some_and(|parent| parent.parent() == cell)
        });
        match typing_in_it {
            Some(inner) => put_in_box(world, inner, value),
            None => world.trigger(UpdateNumberInput {
                entity: cell,
                value: NumberInputValue::F32(value),
            }),
        }
    }
}

/// Replace what a box's text holds with a value, **now**, and not in a later
/// frame.
///
/// The edits are the ones `bevy_feathers` queues in `number_input_on_update`,
/// `SelectAll` and then `Insert`, but they are applied here through
/// `EditableText::apply_pending_edits` rather than left for the engine's
/// `apply_text_edits`. That system runs before `take_back` in `PostUpdate`, so
/// a queued edit would not land until the next frame, and **the next frame's
/// keys reach the box first**, in `PreUpdate`: Enter pressed right after
/// Ctrl+Z would commit the text from before it. That is how a value the user
/// had just taken back went into the history, found by review.
///
/// The character filter is not applied. `EditableTextFilter` keeps its
/// function private, and what is inserted is this function's own formatting of
/// an `f32`, not something a user typed.
///
/// **A value the box cannot hold leaves it empty**, which is what the panel
/// draws for such a value anyway; [`shown_for`] says why and how.
///
/// Mutation: only queue the edits, and
/// `enter_right_after_ctrl_z_commits_what_the_box_shows` fails.
fn put_in_box(world: &mut World, inner: Entity, value: f32) {
    world.resource_scope(|world, mut fonts: Mut<FontCx>| {
        world.resource_scope(|world, mut layout: Mut<LayoutCx>| {
            world.resource_scope(|world, mut clipboard: Mut<Clipboard>| {
                let Some(mut text) = world.get_mut::<EditableText>(inner) else {
                    return;
                };
                let shown = shown_for(value, &text);
                text.queue_edit(TextEdit::SelectAll);
                text.queue_edit(TextEdit::Insert(shown.into()));
                text.apply_pending_edits(&mut fonts, &mut layout.0, &mut clipboard, |_| true);
            });
        });
    });
}

/// What [`put_in_box`] leaves in a box for `value`: its text, or nothing
/// when that is longer than the box holds.
///
/// `f32`'s `Display` never uses an exponent, so a value like `1e20` is more
/// characters than a `FeathersNumberInput` holds, and the engine refuses such
/// an insert whole rather than cutting it. Inserted anyway, the box would keep
/// the text from before the undo and write it over the undo when it is let
/// go, found by review. An empty box writes nothing when it is let go.
///
/// **This is also what "typed" is measured against**, in
/// [`typing_in_focus`]: a box whose text is not this holds something the
/// panel did not put there. One rule for both, so that the text Ctrl+Z puts
/// back is never itself taken for typing, which would leave the key taking
/// back the same nothing for ever.
///
/// Mutation: return the text whatever its length, and
/// `an_undo_to_a_value_the_box_cannot_hold_is_not_written_over` fails.
fn shown_for(value: f32, text: &EditableText) -> String {
    let shown = NumberInputValue::F32(value).to_string();
    if text
        .max_characters
        .is_some_and(|limit| shown.chars().count() > limit)
    {
        String::new()
    } else {
        shown
    }
}

/// The number a box writes to, as the component holds it now.
///
/// The mirror of [`write_leaf`], through [`World::get_reflect`] as [`show`]
/// reads, so that the value a command remembers is read the way it is shown.
fn read_leaf(world: &World, writes: &Writes) -> Option<f32> {
    let component = world.get_reflect(writes.of, writes.component).ok()?;
    let ReflectRef::Struct(shape) = component.reflect_ref() else {
        return None;
    };
    let ReflectRef::Struct(field) = shape.field(&writes.field)?.reflect_ref() else {
        return None;
    };
    field
        .field(&writes.leaf)?
        .try_downcast_ref::<f32>()
        .copied()
}

/// Write one committed number onto the component it came from.
///
/// It takes `&mut World` because [`World::get_reflect_mut`] does, and that call
/// is the mirror of the read: [`show`] reads through `World::get_reflect`
/// because `ReflectComponent` would see fewer components than the engine's own
/// call does, so the set that can be written is exactly the set that is shown.
///
/// **Reached only through [`SetLeaf`]**, both ways: `execute` writes the new
/// value and `undo` the old one, through this same path.
///
/// **Silent on every failure.** Each one means the component went away or
/// changed shape between the panel being drawn and Enter being pressed, and the
/// panel is rebuilt from the world the moment focus leaves the box, which is
/// where the user finds out. Nothing in the editor can reach it today, because
/// nothing removes a component. **It is also what an undo does when the entity
/// is gone**: nothing is written, the entry is used up, and `Entity`'s
/// generation means a reused index is never written to by mistake. Issue #55 is
/// what makes a deletion undoable without that.
///
/// # Where this lands on `CLAUDE.md`'s failure list
///
/// **Row 4**: it is on screen and the user can see it, and Ctrl+Z takes it
/// back.
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
    use crate::pointer::{
        click_at, hold_key, hold_letter, in_window, release_key, release_letter, scroll_at,
        write_input,
    };
    use crate::{Region, Selection, editor, headless};
    use bevy::feathers::controls::FeathersNumberInput;
    use bevy::input_focus::{FocusCause, InputFocus};
    use bevy::picking::pointer::{PointerAction, PointerButton};
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

    /// A named-field struct with nothing in it.
    #[derive(Reflect)]
    struct Nothing;

    /// A component carrying both, so one entity answers both questions.
    #[derive(Component, Reflect)]
    struct Measured {
        /// Opens into two boxes.
        pair: Pair,
        /// Stays one line of text.
        mixed: Mixed,
        /// Stays one line of text too, for a different reason.
        nothing: Nothing,
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

    /// Type into a box, and commit it by clicking somewhere else.
    ///
    /// **The gesture, rather than the focus resource.** An earlier version of
    /// this helper cleared `InputFocus` by hand between updates, and that is
    /// the one arrangement in which the panel has nothing else to redraw, so
    /// the box happened to survive long enough to commit. Every test that went
    /// through it passed while clicking away silently threw the number
    /// away. Clicking is what a person does, so clicking is what this does.
    fn type_and_commit(app: &mut App, cell: Entity, text: &str) {
        type_into(app, cell, text);
        focus(app, cell);
        // Onto the middle placeholder, which is the one these tests have
        // already selected. Clicking it takes the focus out of the box, which
        // is the commit, and changes its `PickingInteraction`, which is what
        // makes the panel want to rebuild in the same frame. Clicking a
        // different entity would do both of those and move the selection as
        // well, which is a second thing for a test to have to reason about.
        click_at(app, in_window(Vec2::ZERO), PointerButton::Primary);
        app.update();
        app.update();
    }

    /// Put the focus in a box, the way pressing on it does.
    ///
    /// `bevy_ui_widgets`' own `on_pointer_press` sets `InputFocus` to the
    /// entity carrying the text, with `FocusCause::Pressed`, so this is that
    /// call rather than a pointer event: the press would land on a node whose
    /// size a headless layout has not settled.
    fn focus(app: &mut App, cell: Entity) {
        let inner = editable_under(app.world(), cell);
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(inner, FocusCause::Pressed);
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
    /// After a box drag over several entities at one depth the active entity is
    /// an arbitrary member of them, which `selection.rs` says of a tie. The panel
    /// carries values now, so a header naming one entity of several without
    /// saying so reads as a claim about the only thing selected.
    /// `docs/specs/ui.md` §6 has the decision, and
    /// `docs/specs/open-questions.md` §1 has the ranking that followed it, which
    /// leaves a tie as arbitrary as before.
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
            nothing: Nothing,
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
            nothing: Nothing,
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

    /// A field with nothing in it stays one line.
    ///
    /// The other end of the bound, and the one the fixture could not reach on
    /// its own: every field of a struct with no fields is an `f32`, vacuously.
    /// Opening it would draw a name with a gap beside it and lose the text the
    /// line used to read, which is `docs/specs/ui.md` §6's rule about a struct
    /// with no fields seen one level down.
    ///
    /// **Found by the coverage pass rather than written with the rest.** The
    /// mutation below left the whole suite green, because nothing on a
    /// placeholder has a field of this shape.
    ///
    /// Mutation: drop the `field_len() == 0` check in `leaves_of`, and this
    /// fails with the line drawing no text at all.
    #[test]
    fn a_field_with_nothing_in_it_stays_one_line() {
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
            nothing: Nothing,
        });
        app.update();

        let line = line_of(&mut app, "Measured", 2);

        assert!(
            boxes_on(&mut app, line).is_empty(),
            "a field with no fields at all opened into boxes"
        );
        assert_eq!(
            text_under(app.world(), line),
            vec![
                "nothing".to_owned(),
                "b2d_editor::inspector::tests::Nothing".to_owned()
            ],
            "the line lost the text it used to read"
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
    /// **What moves here is the scale, and the box that holds focus is on the
    /// translation.** They have to be different fields, because letting a box
    /// go commits what is in it: focusing a box, moving the same number behind
    /// the panel's back and then letting go writes the box's stale value over
    /// the new one, which is a real property of this design and belongs to its
    /// own place rather than to this test. `docs/specs/ui.md` §7 records it.
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

        // Something else resizes the entity while the user is in a box on its
        // translation.
        app.world_mut()
            .entity_mut(middle)
            .insert(Transform::from_scale(Vec3::splat(3.0)));
        app.update();
        app.update();

        let scale = line_of(&mut app, "Transform", 2);
        assert_eq!(
            numbers_on(&mut app, scale),
            ["1", "1", "1"],
            "the panel was rebuilt while a box had focus"
        );

        app.world_mut().resource_mut::<InputFocus>().clear();
        // Four, and each one is doing something. The panel is held for a frame
        // after the focus leaves, so that a box outlives its own commit. Then
        // it rebuilds. Then it rebuilds again, because `GlobalTransform` has
        // caught up with the `Transform` inserted above and the compared value
        // carries both. And a box's buffer is filled by the text edit queue on
        // the frame after the box is spawned, so the last rebuild's numbers are
        // not readable until one more.
        app.update();
        app.update();
        app.update();
        app.update();

        let scale = line_of(&mut app, "Transform", 2);
        assert_eq!(
            numbers_on(&mut app, scale),
            ["3", "3", "3"],
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

    /// Press a letter with modifiers held, and let everything go.
    ///
    /// Two updates after, per RK-013: the undo lands in the first, and the
    /// panel's rebuild after it settles in the second.
    fn press_with(app: &mut App, modifiers: &[KeyCode], key: KeyCode, letter: &str) {
        for modifier in modifiers {
            hold_key(app, *modifier);
        }
        hold_letter(app, key, letter);
        app.update();
        release_letter(app, key, letter);
        for modifier in modifiers {
            release_key(app, *modifier);
        }
        app.update();
        app.update();
    }

    /// Ctrl+Z, as a user on a US layout presses it.
    fn undo(app: &mut App) {
        press_with(app, &[KeyCode::ControlLeft], KeyCode::KeyZ, "z");
    }

    /// Press Enter in the focused box and let it go.
    fn enter(app: &mut App) {
        hold_key(app, KeyCode::Enter);
        app.update();
        release_key(app, KeyCode::Enter);
        app.update();
        app.update();
    }

    /// Where an entity is.
    fn translation(app: &App, entity: Entity) -> Vec3 {
        app.world()
            .entity(entity)
            .get::<Transform>()
            .expect("a placeholder has a transform")
            .translation
    }

    /// What a box's text holds.
    fn text_in(app: &App, cell: Entity) -> String {
        let inner = editable_under(app.world(), cell);
        app.world()
            .entity(inner)
            .get::<EditableText>()
            .expect("the buffer is on the entity that was found by it")
            .value()
            .to_string()
    }

    /// Commit a number into box `index` of the middle placeholder's
    /// translation, by clicking away.
    fn commit_translation(app: &mut App, index: usize, text: &str) {
        let line = line_of(app, "Transform", 0);
        let cell = boxes_on(app, line)[index];
        type_and_commit(app, cell, text);
    }

    /// Undoing a commit puts the component back to what it held, bit for bit.
    ///
    /// The value it starts from is `PI` rather than the placeholder's zero,
    /// so that a restore that went through text, or through a rounding, would
    /// show.
    ///
    /// Mutation: return before calling `undo` in `History::undo`, and this
    /// fails holding the committed value. Mutation: read `old` in
    /// `SetLeaf::read` after writing `new`, and it fails the same way, because
    /// the undo writes back what was committed. Mutation: call `write_leaf`
    /// from `commit` instead of recording, and it fails the same way again,
    /// because the history is empty: this is the guard that the inspector's
    /// commit goes through a command.
    #[test]
    fn undoing_a_commit_puts_the_component_back_bit_for_bit() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        app.world_mut()
            .entity_mut(middle)
            .get_mut::<Transform>()
            .expect("a placeholder has a transform")
            .translation
            .y = core::f32::consts::PI;
        app.update();
        app.update();

        commit_translation(&mut app, 1, "-42.5");
        assert_eq!(
            translation(&app, middle).y,
            -42.5,
            "the commit did not land"
        );
        undo(&mut app);

        assert_eq!(
            translation(&app, middle).y.to_bits(),
            core::f32::consts::PI.to_bits(),
            "Ctrl+Z did not put back what the component held before the commit"
        );
    }

    /// After an undo, the box shows the restored value.
    ///
    /// With no box focused, the panel is rebuilt from the world as it is
    /// after any change, so what holds this is `show`'s own rebuild, guarded
    /// by its tests. The case that is new code, a box that is focused when
    /// the undo lands, is
    /// `ctrl_z_in_a_box_with_nothing_typed_takes_back_the_last_commit`.
    ///
    /// Mutation: return before calling `undo` in `History::undo`, and this
    /// fails reading `-42.5`.
    #[test]
    fn after_an_undo_the_box_shows_the_restored_value() {
        let mut app = inspector_editor();
        select_the_middle(&mut app);

        commit_translation(&mut app, 1, "-42.5");
        undo(&mut app);

        let line = line_of(&mut app, "Transform", 0);
        assert_eq!(
            numbers_on(&mut app, line),
            ["0", "0", "0"],
            "the row does not show what the undo restored"
        );
    }

    /// Undo with nothing in the history changes nothing, and does not panic.
    ///
    /// Mutation: `expect` the `pop` in `History::undo`, and this panics.
    #[test]
    fn undo_with_nothing_in_the_history_changes_nothing() {
        let mut app = inspector_editor();
        select_the_middle(&mut app);
        let before: Vec<Transform> = app
            .world_mut()
            .query::<&Transform>()
            .iter(app.world())
            .copied()
            .collect();

        undo(&mut app);

        let after: Vec<Transform> = app
            .world_mut()
            .query::<&Transform>()
            .iter(app.world())
            .copied()
            .collect();
        assert_eq!(
            before, after,
            "an undo with nothing to undo moved something"
        );
    }

    /// Two commits come back in reverse order.
    ///
    /// `x` first and then `y`, so that the first undo taking back the wrong
    /// one is visible: it leaves `x` at zero and `y` at twenty.
    ///
    /// Mutation: `remove(0)` in place of `pop()` in `History::undo`, and this
    /// fails on the first undo.
    #[test]
    fn two_commits_come_back_in_reverse_order() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);

        commit_translation(&mut app, 0, "10");
        commit_translation(&mut app, 1, "20");
        assert_eq!(translation(&app, middle), Vec3::new(10.0, 20.0, 0.0));

        undo(&mut app);
        assert_eq!(
            translation(&app, middle),
            Vec3::new(10.0, 0.0, 0.0),
            "the first undo did not take back the last commit"
        );
        undo(&mut app);
        assert_eq!(
            translation(&app, middle),
            Vec3::ZERO,
            "the second undo did not take back the first commit"
        );
    }

    /// A commit that changes nothing is not an entry.
    ///
    /// One Enter is two commits, on the key's press and on its release, and
    /// letting go of the box is a third. All three carry `10`, and only the
    /// first changes anything, so one Ctrl+Z is enough to take it back.
    /// `docs/specs/ui.md` §8 has why.
    ///
    /// Mutation: drop the bits comparison in `SetLeaf::read`, and this fails
    /// with the placeholder still at ten, because the undo took back a commit
    /// of ten over ten.
    #[test]
    fn a_commit_that_changes_nothing_is_not_an_entry() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);

        let line = line_of(&mut app, "Transform", 0);
        let cell = boxes_on(&mut app, line)[1];
        type_into(&mut app, cell, "10");
        focus(&mut app, cell);
        enter(&mut app);
        click_at(&mut app, in_window(Vec2::ZERO), PointerButton::Primary);
        app.update();
        app.update();
        assert_eq!(translation(&app, middle).y, 10.0, "the commit did not land");

        undo(&mut app);

        assert_eq!(
            translation(&app, middle).y,
            0.0,
            "one Ctrl+Z did not take back one edit"
        );
    }

    /// Ctrl+Z in a box takes back what was typed, and not the last commit.
    ///
    /// Ten is committed, ninety nine is typed over it and not committed, and
    /// Ctrl+Z puts the box back to ten. Letting go afterwards commits ten over
    /// ten, which is nothing.
    ///
    /// Mutation: answer `None` always from `typing_in_focus`, and this fails
    /// with the commit of ten taken back instead.
    #[test]
    fn ctrl_z_in_a_box_takes_back_what_was_typed_and_not_the_last_commit() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        commit_translation(&mut app, 1, "10");

        let line = line_of(&mut app, "Transform", 0);
        let cell = boxes_on(&mut app, line)[1];
        type_into(&mut app, cell, "99");
        focus(&mut app, cell);
        undo(&mut app);

        assert_eq!(text_in(&app, cell), "10", "the box still holds the typing");
        assert_eq!(
            translation(&app, middle).y,
            10.0,
            "taking back the typing touched the component"
        );
        click_at(&mut app, in_window(Vec2::ZERO), PointerButton::Primary);
        app.update();
        app.update();
        assert_eq!(
            translation(&app, middle).y,
            10.0,
            "letting go of the box wrote something other than the commit"
        );
    }

    /// Ctrl+Z in a box holding something that is not a number takes back the
    /// typing, and not the last commit.
    ///
    /// A lone `-` is where every negative number starts, and it does not
    /// parse. It is still typing the user has not committed, so it is what
    /// Ctrl+Z takes back.
    ///
    /// Mutation: in `typing_in_focus`, parse with `unwrap_or(leaf)` so that
    /// text which does not parse counts as the leaf, and this fails with the
    /// commit of ten taken back instead.
    #[test]
    fn ctrl_z_in_a_box_holding_what_is_not_a_number_takes_back_the_typing() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        commit_translation(&mut app, 1, "10");

        let line = line_of(&mut app, "Transform", 0);
        let cell = boxes_on(&mut app, line)[1];
        type_into(&mut app, cell, "-");
        focus(&mut app, cell);
        undo(&mut app);

        assert_eq!(text_in(&app, cell), "10", "the box still holds the typing");
        assert_eq!(
            translation(&app, middle).y,
            10.0,
            "Ctrl+Z took back the commit rather than the typing"
        );
    }

    /// Enter in the frame after Ctrl+Z commits what the box shows, not what
    /// it held before.
    ///
    /// Ninety nine is typed over a committed ten and taken back with Ctrl+Z.
    /// Enter goes down in the very next frame, which reaches the box in
    /// `PreUpdate`, before anything else has run. If the box's text were only
    /// queued, Enter would read ninety nine and commit the value the user had
    /// just taken back, and the next Ctrl+Z would restore it.
    ///
    /// Mutation: in `put_in_box`, only queue the edits and do not apply them,
    /// and this fails with ninety nine coming back.
    #[test]
    fn enter_right_after_ctrl_z_commits_what_the_box_shows() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        commit_translation(&mut app, 1, "10");

        let line = line_of(&mut app, "Transform", 0);
        let cell = boxes_on(&mut app, line)[1];
        type_into(&mut app, cell, "99");
        focus(&mut app, cell);

        hold_key(&mut app, KeyCode::ControlLeft);
        hold_letter(&mut app, KeyCode::KeyZ, "z");
        app.update();
        release_letter(&mut app, KeyCode::KeyZ, "z");
        release_key(&mut app, KeyCode::ControlLeft);
        hold_key(&mut app, KeyCode::Enter);
        app.update();
        release_key(&mut app, KeyCode::Enter);
        app.update();
        app.update();
        assert_eq!(
            translation(&app, middle).y,
            10.0,
            "Enter committed the typing"
        );

        undo(&mut app);

        assert_eq!(
            translation(&app, middle).y,
            0.0,
            "Ctrl+Z restored a value the user had taken back and never committed"
        );
    }

    /// Put a value on the middle placeholder's `translation.y` directly, and
    /// let the panel settle around it.
    fn set_y(app: &mut App, entity: Entity, y: f32) {
        app.world_mut()
            .entity_mut(entity)
            .get_mut::<Transform>()
            .expect("a placeholder has a transform")
            .translation
            .y = y;
        app.update();
        app.update();
    }

    /// An undo to a value the box cannot hold is not written over when the
    /// box is let go.
    ///
    /// `1e20` prints as twenty one digits, one more than the box holds, and
    /// the engine refuses an insert that is too long rather than cutting it.
    /// Five is committed over it with Enter, and Ctrl+Z puts `1e20` back. If
    /// the box kept `5`, letting go would write five over the undo.
    ///
    /// Mutation: return the text from `shown_for` whatever its length, and
    /// this fails with five written back.
    #[test]
    fn an_undo_to_a_value_the_box_cannot_hold_is_not_written_over() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        set_y(&mut app, middle, 1e20);

        let line = line_of(&mut app, "Transform", 0);
        let cell = boxes_on(&mut app, line)[1];
        type_into(&mut app, cell, "5");
        focus(&mut app, cell);
        enter(&mut app);
        assert_eq!(translation(&app, middle).y, 5.0, "Enter did not commit");

        undo(&mut app);
        assert_eq!(
            translation(&app, middle).y,
            1e20,
            "the commit was not taken back"
        );
        click_at(&mut app, in_window(Vec2::ZERO), PointerButton::Primary);
        app.update();
        app.update();

        assert_eq!(
            translation(&app, middle).y,
            1e20,
            "letting go of the box wrote the undone value back"
        );
    }

    /// Ctrl+Z in a box left empty by a value it cannot hold reaches the
    /// history.
    ///
    /// The box under `1e20` is empty because the panel could not put the value
    /// there, not because anybody typed. Taking that for typing would make
    /// Ctrl+Z put back the same nothing every time it was pressed, and never
    /// reach the commit on `x`.
    ///
    /// Mutation: in `typing_in_focus`, count every text that is not a number
    /// as typed, and this fails with `x` still at three.
    #[test]
    fn ctrl_z_in_a_box_left_empty_by_a_long_value_reaches_the_history() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        set_y(&mut app, middle, 1e20);

        // With Enter rather than by clicking away: the placeholder is now far
        // off screen, so a click at the origin lands on empty space and takes
        // the selection with it.
        let line = line_of(&mut app, "Transform", 0);
        let [x, cell] = [boxes_on(&mut app, line)[0], boxes_on(&mut app, line)[1]];
        type_into(&mut app, x, "3");
        focus(&mut app, x);
        enter(&mut app);
        assert_eq!(translation(&app, middle).x, 3.0, "Enter did not commit");
        assert_eq!(
            text_in(&app, cell),
            "",
            "the box is not empty, so this says nothing"
        );
        focus(&mut app, cell);
        undo(&mut app);

        assert_eq!(
            translation(&app, middle).x,
            0.0,
            "Ctrl+Z did not reach the history"
        );
    }

    /// Ctrl+Z in a box with nothing typed takes back the last commit, and the
    /// box follows it.
    ///
    /// Enter commits without the focus leaving, so the box is still focused
    /// when the undo lands and the panel is frozen. `UpdateNumberInput` skips
    /// a focused box, so unless its text is replaced by hand it keeps ten, and
    /// letting go of it writes ten back over the undo.
    ///
    /// **This is also the test that reports Bevy's own text undo arriving.**
    /// If the box starts taking Ctrl+Z for itself, the key stops reaching the
    /// history here and this fails. `docs/specs/ui.md` §8's last accepted risk
    /// is that.
    ///
    /// Mutation: send the focused box `UpdateNumberInput` like the others in
    /// `show_values_in_place`, and this fails on the box's text and then on
    /// the component.
    #[test]
    fn ctrl_z_in_a_box_with_nothing_typed_takes_back_the_last_commit() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);

        let line = line_of(&mut app, "Transform", 0);
        let cell = boxes_on(&mut app, line)[1];
        type_into(&mut app, cell, "10");
        focus(&mut app, cell);
        enter(&mut app);
        assert_eq!(translation(&app, middle).y, 10.0, "Enter did not commit");

        undo(&mut app);

        assert_eq!(
            translation(&app, middle).y,
            0.0,
            "the commit was not taken back"
        );
        assert_eq!(
            text_in(&app, cell),
            "0",
            "the focused box kept the undone value"
        );
        click_at(&mut app, in_window(Vec2::ZERO), PointerButton::Primary);
        app.update();
        app.update();
        assert_eq!(
            translation(&app, middle).y,
            0.0,
            "letting go of the box wrote the undone value back"
        );
    }

    /// A box other than the focused one shows what an undo restored, while the
    /// panel is still frozen.
    ///
    /// `x` is committed, then the focus goes into `y`'s box with nothing typed,
    /// so Ctrl+Z takes back `x`. The panel is not rebuilt while `y` has focus,
    /// so `x`'s box shows the restored value only because it was sent it. Row 4
    /// if it did not, and only until the focus left, but
    /// `show_values_in_place` says it does this and a claim with no test is
    /// what RK-005 is about.
    ///
    /// Mutation: do nothing in the `None` arm of `show_values_in_place`, and
    /// this fails with `x`'s box still reading ten.
    #[test]
    fn an_undo_reaches_the_boxes_that_are_not_focused() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        commit_translation(&mut app, 0, "10");

        let line = line_of(&mut app, "Transform", 0);
        let [x, y] = [boxes_on(&mut app, line)[0], boxes_on(&mut app, line)[1]];
        focus(&mut app, y);
        undo(&mut app);

        assert_eq!(
            translation(&app, middle).x,
            0.0,
            "the commit was not taken back"
        );
        assert_eq!(
            app.world().resource::<InputFocus>().get(),
            Some(editable_under(app.world(), y)),
            "the focus left, so the panel was rebuilt and this says nothing"
        );
        assert_eq!(
            text_in(&app, x),
            "0",
            "the box that was not focused kept ten"
        );
    }

    /// Letting go of a box and pressing Ctrl+Z in one frame takes back what
    /// letting go committed.
    ///
    /// The press lands on empty space in the viewport, which takes the focus
    /// out of the box, and Ctrl+Z goes down in the same frame. The box commits
    /// in `PostUpdate`; the undo has to come after it.
    ///
    /// Written as one frame by hand rather than through `click_at`, which
    /// runs updates between its actions, per RK-014: the defect this guards
    /// lives inside a frame.
    ///
    /// Mutation: add `take_back` to `Update` instead of after
    /// `FocusChangeEvents`, and this fails with ten still in place.
    #[test]
    fn letting_go_of_a_box_and_pressing_ctrl_z_in_one_frame_takes_back_what_letting_go_committed() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);

        let line = line_of(&mut app, "Transform", 0);
        let cell = boxes_on(&mut app, line)[1];
        type_into(&mut app, cell, "10");
        focus(&mut app, cell);

        let empty = in_window(Vec2::new(0.0, 200.0));
        write_input(&mut app, empty, PointerAction::Move { delta: Vec2::ONE });
        app.update();
        app.update();
        hold_key(&mut app, KeyCode::ControlLeft);
        hold_letter(&mut app, KeyCode::KeyZ, "z");
        write_input(
            &mut app,
            empty,
            PointerAction::Press(PointerButton::Primary),
        );
        app.update();
        assert_ne!(
            app.world().resource::<InputFocus>().get(),
            Some(editable_under(app.world(), cell)),
            "the press did not take the focus out of the box in its own frame"
        );
        write_input(
            &mut app,
            empty,
            PointerAction::Release(PointerButton::Primary),
        );
        release_letter(&mut app, KeyCode::KeyZ, "z");
        release_key(&mut app, KeyCode::ControlLeft);
        app.update();
        app.update();

        assert_eq!(
            translation(&app, middle).y,
            0.0,
            "the undo ran before the commit it was pressed after"
        );
    }

    /// Undoing an edit to an entity that is gone writes nothing, and does not
    /// panic.
    ///
    /// Nothing in the editor despawns an editable entity yet; this does it by
    /// hand. `docs/specs/ui.md` §8 decided nothing is written, and issue #55
    /// is what makes a deletion undoable.
    ///
    /// Mutation: `expect` the `get_reflect_mut` in `write_leaf`, and this
    /// panics.
    #[test]
    fn undoing_an_edit_to_an_entity_that_is_gone_writes_nothing() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        commit_translation(&mut app, 1, "10");
        let others: Vec<(Entity, Transform)> = app
            .world_mut()
            .query::<(Entity, &Transform)>()
            .iter(app.world())
            .filter(|(entity, _)| *entity != middle)
            .map(|(entity, transform)| (entity, *transform))
            .collect();

        app.world_mut().entity_mut(middle).despawn();
        app.update();
        undo(&mut app);

        for (entity, transform) in others {
            assert_eq!(
                app.world().get::<Transform>(entity),
                Some(&transform),
                "an undo for an entity that is gone moved something else"
            );
        }
    }

    /// Ctrl+Shift+Z is not undo. It is left for redo.
    ///
    /// Mutation: drop the Shift check in `take_back`, and this fails with the
    /// commit taken back.
    #[test]
    fn ctrl_shift_z_is_not_undo() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        commit_translation(&mut app, 1, "10");

        press_with(
            &mut app,
            &[KeyCode::ControlLeft, KeyCode::ShiftLeft],
            KeyCode::KeyZ,
            "Z",
        );

        assert_eq!(translation(&app, middle).y, 10.0, "Ctrl+Shift+Z undid");
    }

    /// Cmd+Z is undo, as Ctrl+Z is, on every platform.
    ///
    /// Mutation: drop the `Super` keys in `take_back`, and this fails with the
    /// commit still in place.
    #[test]
    fn cmd_z_is_undo_as_ctrl_z_is() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        commit_translation(&mut app, 1, "10");

        press_with(&mut app, &[KeyCode::SuperLeft], KeyCode::KeyZ, "z");

        assert_eq!(translation(&app, middle).y, 0.0, "Cmd+Z did not undo");
    }

    /// Undo is the key that says Z, wherever it sits.
    ///
    /// On a French keyboard the key that produces `z` is where a US keyboard
    /// has W, so this presses `KeyCode::KeyW` producing `z`.
    ///
    /// Mutation: read `KeyCode::KeyZ` in `take_back` in place of the letter,
    /// and this fails with the commit still in place.
    #[test]
    fn undo_is_the_key_that_says_z_wherever_it_is() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        commit_translation(&mut app, 1, "10");

        press_with(&mut app, &[KeyCode::ControlLeft], KeyCode::KeyW, "z");

        assert_eq!(
            translation(&app, middle).y,
            0.0,
            "the key that says z did not undo"
        );
    }

    /// Ctrl+Y, as a user on a US layout presses it.
    fn redo(app: &mut App) {
        press_with(app, &[KeyCode::ControlLeft], KeyCode::KeyY, "y");
    }

    /// Redo puts back the commit undo took back, and the box shows it.
    ///
    /// Mutation: return before calling `execute` in `History::redo`, and this
    /// fails holding the undone value. Mutation: drop the `undone.push` in
    /// `History::undo`, and it fails the same way, because there is nothing
    /// to redo.
    #[test]
    fn redo_puts_back_the_commit_undo_took_back_and_the_box_shows_it() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        commit_translation(&mut app, 1, "10");
        undo(&mut app);
        assert_eq!(translation(&app, middle).y, 0.0, "the undo did not land");

        redo(&mut app);

        assert_eq!(
            translation(&app, middle).y,
            10.0,
            "Ctrl+Y did not put back the commit"
        );
        let line = line_of(&mut app, "Transform", 0);
        assert_eq!(
            numbers_on(&mut app, line),
            ["0", "10", "0"],
            "the row does not show what the redo put back"
        );
    }

    /// Redo with nothing undone changes nothing, and does not panic.
    ///
    /// There is a commit in the history, so a redo that reached for the wrong
    /// side would find something to write.
    ///
    /// Mutation: `expect` the `pop` in `History::redo`, and this panics.
    #[test]
    fn redo_with_nothing_undone_changes_nothing() {
        let mut app = inspector_editor();
        select_the_middle(&mut app);
        commit_translation(&mut app, 1, "10");
        let before: Vec<Transform> = app
            .world_mut()
            .query::<&Transform>()
            .iter(app.world())
            .copied()
            .collect();

        redo(&mut app);

        let after: Vec<Transform> = app
            .world_mut()
            .query::<&Transform>()
            .iter(app.world())
            .copied()
            .collect();
        assert_eq!(before, after, "a redo with nothing to redo moved something");
    }

    /// A new commit after an undo leaves nothing to redo.
    ///
    /// Ten is committed and taken back, and twenty is committed in its place.
    /// A redo that still held ten would write it over twenty, a value the user
    /// did not choose: row 6. `docs/specs/ui.md` §9 has why the history stays
    /// one line.
    ///
    /// Mutation: leave out the `clear` in `History::record`, and this fails
    /// with ten written over twenty.
    #[test]
    fn a_new_commit_after_an_undo_leaves_nothing_to_redo() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        commit_translation(&mut app, 1, "10");
        undo(&mut app);
        commit_translation(&mut app, 1, "20");

        redo(&mut app);

        assert_eq!(
            translation(&app, middle).y,
            20.0,
            "the redo put back a commit the new one had replaced"
        );
    }

    /// Two undos come back in order under two redos.
    ///
    /// `x` and then `y` are committed and both taken back, so the first redo
    /// has to put back `x`, the one undone last.
    ///
    /// Mutation: `remove(0)` in place of `pop()` in `History::redo`, and this
    /// fails on the first redo.
    #[test]
    fn two_undos_come_back_in_order_under_two_redos() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        commit_translation(&mut app, 0, "10");
        commit_translation(&mut app, 1, "20");
        undo(&mut app);
        undo(&mut app);
        assert_eq!(
            translation(&app, middle),
            Vec3::ZERO,
            "the undos did not land"
        );

        redo(&mut app);
        assert_eq!(
            translation(&app, middle),
            Vec3::new(10.0, 0.0, 0.0),
            "the first redo did not put back the first commit"
        );
        redo(&mut app);
        assert_eq!(
            translation(&app, middle),
            Vec3::new(10.0, 20.0, 0.0),
            "the second redo did not put back the second commit"
        );
    }

    /// Letting go of a box after an undo leaves the redo in place.
    ///
    /// The focus goes into the box with nothing typed and Ctrl+Z takes back
    /// the commit, which gives the box the undone value. Letting go commits
    /// that value over itself, which is not an entry, so the redo side
    /// survives it. If it did not, clicking away after Ctrl+Z would make
    /// Ctrl+Y do nothing.
    ///
    /// Mutation: drop the bits comparison in `SetLeaf::read`, and this fails
    /// with the commit not put back, because letting go became an entry.
    #[test]
    fn letting_go_of_a_box_after_an_undo_leaves_the_redo_in_place() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        commit_translation(&mut app, 1, "10");

        let line = line_of(&mut app, "Transform", 0);
        let cell = boxes_on(&mut app, line)[1];
        focus(&mut app, cell);
        undo(&mut app);
        click_at(&mut app, in_window(Vec2::ZERO), PointerButton::Primary);
        app.update();
        app.update();
        assert_eq!(translation(&app, middle).y, 0.0, "the undo did not land");

        redo(&mut app);

        assert_eq!(
            translation(&app, middle).y,
            10.0,
            "letting go of the box dropped what could be redone"
        );
    }

    /// Letting go of a box and pressing Ctrl+Y in one frame does not redo
    /// under the commit letting go makes.
    ///
    /// Five is committed and taken back, and seven is typed. The press on
    /// empty space and Ctrl+Y land in one frame. Letting go commits seven,
    /// which drops the redo side, so the redo finds nothing. Had the redo run
    /// first, five would be put back and seven committed over it, and the next
    /// Ctrl+Z would stop at five: an entry the user never made.
    ///
    /// Written as one frame by hand, per RK-014.
    ///
    /// Mutation: add `put_back` to `Update` instead of after
    /// `FocusChangeEvents`, and this fails with the undo stopping at five.
    #[test]
    fn letting_go_of_a_box_and_pressing_ctrl_y_in_one_frame_does_not_redo_under_the_commit() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        commit_translation(&mut app, 1, "5");
        undo(&mut app);

        let line = line_of(&mut app, "Transform", 0);
        let cell = boxes_on(&mut app, line)[1];
        type_into(&mut app, cell, "7");
        focus(&mut app, cell);

        let empty = in_window(Vec2::new(0.0, 200.0));
        write_input(&mut app, empty, PointerAction::Move { delta: Vec2::ONE });
        app.update();
        app.update();
        hold_key(&mut app, KeyCode::ControlLeft);
        hold_letter(&mut app, KeyCode::KeyY, "y");
        write_input(
            &mut app,
            empty,
            PointerAction::Press(PointerButton::Primary),
        );
        app.update();
        write_input(
            &mut app,
            empty,
            PointerAction::Release(PointerButton::Primary),
        );
        release_letter(&mut app, KeyCode::KeyY, "y");
        release_key(&mut app, KeyCode::ControlLeft);
        app.update();
        app.update();
        assert_eq!(
            translation(&app, middle).y,
            7.0,
            "letting go did not commit seven"
        );

        undo(&mut app);

        assert_eq!(
            translation(&app, middle).y,
            0.0,
            "the redo ran before the commit it was pressed with"
        );
    }

    /// Redo does nothing while the focused box holds typing, and the typing
    /// stays.
    ///
    /// Ten is committed and taken back, and ninety nine is typed. Ctrl+Y
    /// leaves both alone. Ctrl+Z then takes the typing back, and Ctrl+Y puts
    /// back ten, because the redo side was kept. `docs/specs/ui.md` §9 has
    /// why.
    ///
    /// Mutation: drop the `typing_in_focus` check in `put_back`, and this
    /// fails with ten put back over the typing.
    #[test]
    fn redo_does_nothing_while_the_focused_box_holds_typing() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        commit_translation(&mut app, 1, "10");
        undo(&mut app);

        let line = line_of(&mut app, "Transform", 0);
        let cell = boxes_on(&mut app, line)[1];
        type_into(&mut app, cell, "99");
        focus(&mut app, cell);
        redo(&mut app);

        assert_eq!(
            translation(&app, middle).y,
            0.0,
            "Ctrl+Y redid while something was typed"
        );
        assert_eq!(text_in(&app, cell), "99", "Ctrl+Y threw the typing away");

        undo(&mut app);
        redo(&mut app);

        assert_eq!(
            translation(&app, middle).y,
            10.0,
            "the redo side did not survive the typing"
        );
    }

    /// Ctrl+Y in a box with nothing typed reaches the history, and the box
    /// follows it.
    ///
    /// Enter commits without the focus leaving, so the box is still focused
    /// through the undo and the redo, and its text is only right because
    /// `show_values_in_place` was called. Letting go afterwards then commits
    /// ten over ten.
    ///
    /// **This is also the test that reports Bevy's own text undo arriving**,
    /// if its box starts taking Ctrl+Y for itself. `docs/specs/ui.md` §9's
    /// last accepted risk is that.
    ///
    /// Mutation: in `put_back`, redo only when `typing_in_focus` finds
    /// something, and this fails with the commit not put back. Mutation: do
    /// not call `show_values_in_place` after the redo, and this fails on the
    /// box's text.
    #[test]
    fn ctrl_y_in_a_box_with_nothing_typed_reaches_the_history() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);

        let line = line_of(&mut app, "Transform", 0);
        let cell = boxes_on(&mut app, line)[1];
        type_into(&mut app, cell, "10");
        focus(&mut app, cell);
        enter(&mut app);
        undo(&mut app);
        assert_eq!(translation(&app, middle).y, 0.0, "the undo did not land");

        redo(&mut app);

        assert_eq!(
            translation(&app, middle).y,
            10.0,
            "Ctrl+Y in the box did not reach the history"
        );
        assert_eq!(
            text_in(&app, cell),
            "10",
            "the focused box kept the undone value"
        );
        click_at(&mut app, in_window(Vec2::ZERO), PointerButton::Primary);
        app.update();
        app.update();
        assert_eq!(
            translation(&app, middle).y,
            10.0,
            "letting go of the box wrote the undone value back"
        );
    }

    /// Ctrl+Shift+Z is redo, as Ctrl+Y is.
    ///
    /// Mutation: drop the Shift and Z arm in `put_back`, and this fails with
    /// the commit still taken back.
    #[test]
    fn ctrl_shift_z_is_redo_as_ctrl_y_is() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        commit_translation(&mut app, 1, "10");
        undo(&mut app);

        press_with(
            &mut app,
            &[KeyCode::ControlLeft, KeyCode::ShiftLeft],
            KeyCode::KeyZ,
            "Z",
        );

        assert_eq!(
            translation(&app, middle).y,
            10.0,
            "Ctrl+Shift+Z did not redo"
        );
    }

    /// Cmd+Y is redo, as Ctrl+Y is, on every platform.
    ///
    /// Mutation: drop the `Super` keys in `put_back`, and this fails with the
    /// commit still taken back.
    #[test]
    fn cmd_y_is_redo_as_ctrl_y_is() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        commit_translation(&mut app, 1, "10");
        undo(&mut app);

        press_with(&mut app, &[KeyCode::SuperLeft], KeyCode::KeyY, "y");

        assert_eq!(translation(&app, middle).y, 10.0, "Cmd+Y did not redo");
    }

    /// Redo is the key that says Y, wherever it sits.
    ///
    /// On a German keyboard the key that produces `y` is where a US keyboard
    /// has Z, so this presses `KeyCode::KeyZ` producing `y`.
    ///
    /// Mutation: read `KeyCode::KeyY` in `put_back` in place of the letter,
    /// and this fails with the commit still taken back.
    #[test]
    fn redo_is_the_key_that_says_y_wherever_it_is() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        commit_translation(&mut app, 1, "10");
        undo(&mut app);

        press_with(&mut app, &[KeyCode::ControlLeft], KeyCode::KeyZ, "y");

        assert_eq!(
            translation(&app, middle).y,
            10.0,
            "the key that says y did not redo"
        );
    }

    /// Ctrl+Shift+Y is not redo.
    ///
    /// Mutation: drop the Shift check on Y in `put_back`, and this fails with
    /// the commit put back.
    #[test]
    fn ctrl_shift_y_is_not_redo() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        commit_translation(&mut app, 1, "10");
        undo(&mut app);

        press_with(
            &mut app,
            &[KeyCode::ControlLeft, KeyCode::ShiftLeft],
            KeyCode::KeyY,
            "Y",
        );

        assert_eq!(translation(&app, middle).y, 0.0, "Ctrl+Shift+Y redid");
    }
}
