//! The inspector: what the active entity is, and what it is made of.
//!
//! What it shows, what it calls an entity, and in what order are settled in
//! [`docs/specs/ui.md` §6](../../../docs/specs/ui.md). That the panel is
//! rebuilt rather than reconciled is
//! [ADR-0002](../../../docs/adr/0002-rebuild-the-inspector-rather-than-diff-it.md).

use b2d_editor_ui::field_row;
use bevy::feathers::containers::{pane_body, pane_header};
use bevy::feathers::display::{label, label_dim};
use bevy::prelude::*;
use bevy::reflect::ReflectRef;

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
    /// **Nothing draws it, and nothing reaches it any more.** It was put here
    /// because two entities with the same `Name` and the same components
    /// compared equal, so moving the selection between them left the panel
    /// alone; a test built that pair out of two identically named placeholders.
    ///
    /// Showing the values took that pair away. Every component's value is on
    /// screen now, so two entities that sit in different places differ in
    /// `Transform`, `GlobalTransform` and `Aabb`, and two entities that sit in
    /// the same place cannot both be clicked: `select` in `selection.rs` takes
    /// the nearest, which is one of them and always the same one. **Measured**:
    /// with the look-alike test gone, replacing this with
    /// `Entity::PLACEHOLDER` leaves all 85 tests green.
    ///
    /// It stays because what took the pair away is what
    /// [`docs/specs/ui.md` §6](../../../docs/specs/ui.md) defers: filtering
    /// which components are listed, and collapsing a component so its values
    /// are not drawn. Either one brings look-alikes back, and then this field
    /// is the difference between a panel that follows the selection and one
    /// that silently does not. Saying it is unguarded beats letting a green
    /// suite imply otherwise.
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
/// **The order of the two variants is the order they sort in**, which is what
/// puts every named component above every unregistered one: a derived `Ord` on
/// an enum ranks by declaration first and by the fields second, so `Named`
/// against `Named` compares the names. Swapping these two lines changes what is
/// on screen, which is why they are not in the other order by accident.
///
/// **`name` stays the first field of `Named`** for the same reason: the derive
/// ranks by declaration order, so `rows.sort()` sorts by name and not by what
/// the component happens to contain.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Row {
    /// A component whose value could be read, and the lines under it.
    Named {
        /// What the panel calls it.
        name: String,
        /// One per line under the name, in the order they are drawn.
        fields: Vec<Field>,
    },
    /// A component with no reflection, so with neither a name nor a value.
    Unregistered,
}

/// One line under a component.
///
/// **The values are part of what the panel is compared on**, which is what
/// makes a component that changed in the world redraw: [`Shown`] holds the
/// finished picture, and a picture that carried only the names would be equal
/// to itself while the numbers moved.
///
/// Mutation: write `PartialEq` by hand so that it compares `name` and not
/// `value`, and `a_field_row_follows_the_component_it_reads` fails.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Field {
    /// The field's name, or `None` when the component is not a named struct
    /// and the line carries the whole value.
    name: Option<String>,
    /// What the value reads as.
    value: String,
}

/// The inspector, showing what is selected.
///
/// Mutation: leave its row out of the editor's member table, and
/// `the_group_carries_the_members_the_table_names` fails.
pub struct InspectorPlugin;

impl Plugin for InspectorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Shown>().add_systems(Update, show);
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
/// the `rows.sort()`, and `the_rows_are_in_the_order_the_names_sort_in` fails.
/// Mutation: return `Row::Named` for the unregistered arm, and
/// `a_component_with_no_reflection_appears_as_unregistered` fails. Mutation:
/// despawn the children only when there is something to put back, and
/// `the_inspector_is_empty_when_nothing_is_selected` fails. Mutation: format the
/// entity without asking for its `Name`, and
/// `the_header_uses_the_entitys_name_when_it_has_one` fails. Mutation: give a
/// field row an empty value in [`fields_of`], and
/// `a_field_row_reads_the_entitys_own_value` and
/// `a_field_row_follows_the_component_it_reads` both fail. Mutation: write to
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
                info.type_id()
                    .and_then(|of| world.get_reflect(*entity, of).ok())
                    .map_or(Row::Unregistered, |value| Row::Named {
                        name: value.reflect_short_type_path().to_owned(),
                        fields: fields_of(value.as_partial_reflect()),
                    })
            })
            .collect();
        rows.sort();
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
                Row::Named { name, fields } => {
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
                        // widget's, so it is on a node here and not inside
                        // `field_row`, which stays a plain two-string row.
                        let line = commands
                            .spawn_scene(bsn! {
                                Node { padding: {UiRect::left(FIELD_INDENT)} }
                            })
                            .insert(ChildOf(group))
                            .id();
                        match &field.name {
                            Some(name) => {
                                commands
                                    .spawn_scene(field_row(name.clone(), field.value.clone()))
                                    .insert(ChildOf(line));
                            }
                            None => {
                                commands
                                    .spawn_scene(label_dim(field.value.clone()))
                                    .insert(ChildOf(line));
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
            .map(|index| Field {
                name: shape.name_at(index).map(ToOwned::to_owned),
                value: shape
                    .field_at(index)
                    .map_or_else(String::new, |field| format!("{field:?}")),
            })
            .collect(),
        _ => vec![Field {
            name: None,
            value: format!("{value:?}"),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::UNREGISTERED;
    use crate::pointer::{click_at, hold_key, in_window, release_key};
    use crate::{Region, Selection, editor, headless};
    use bevy::picking::pointer::PointerButton;
    use bevy::prelude::*;

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
    /// Mutation: drop the `rows.sort()` in `show`, and this fails. Mutation:
    /// swap the two variants of `Row`, and the second half fails.
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
    /// Mutation: give a field row an empty value in `fields_of`, and this
    /// fails. There is no default path to mutate into: the value comes off the
    /// entity by construction, which is row 1 rather than row 2.
    #[test]
    fn a_field_row_reads_the_entitys_own_value() {
        let mut app = inspector_editor();
        let middle = select_the_middle(&mut app);
        app.world_mut()
            .entity_mut(middle)
            .insert(Transform::from_xyz(12.5, -7.25, 3.0));
        app.update();

        let lines = lines_under(&mut app, "Transform");

        assert_eq!(
            lines.first().map(Vec::as_slice),
            Some(
                [
                    "translation".to_owned(),
                    "Vec3(12.5, -7.25, 3.0)".to_owned()
                ]
                .as_slice()
            ),
            "the translation row is not the entity's own: {lines:?}"
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
        let before = lines_under(&mut app, "Transform");

        app.world_mut()
            .entity_mut(middle)
            .insert(Transform::from_xyz(2.0, 0.0, 0.0));
        app.update();
        let after = lines_under(&mut app, "Transform");

        assert_eq!(
            before.first().map(Vec::as_slice),
            Some(["translation".to_owned(), "Vec3(1.0, 0.0, 0.0)".to_owned()].as_slice()),
            "the fixture never showed the first value: {before:?}"
        );
        assert_eq!(
            after.first().map(Vec::as_slice),
            Some(["translation".to_owned(), "Vec3(2.0, 0.0, 0.0)".to_owned()].as_slice()),
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
}
