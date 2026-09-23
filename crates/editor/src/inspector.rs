//! The inspector: what the active entity is, and what it is made of.
//!
//! What it shows, what it calls an entity, and in what order are settled in
//! [`docs/specs/ui.md` §6](../../../docs/specs/ui.md). That the panel is
//! rebuilt rather than reconciled is
//! [ADR-0002](../../../docs/adr/0002-rebuild-the-inspector-rather-than-diff-it.md).

use bevy::ecs::component::{ComponentInfo, Components};
use bevy::feathers::containers::{pane_body, pane_header};
use bevy::feathers::display::{label, label_dim};
use bevy::prelude::*;

use crate::{Region, Selection};

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
    /// **Nothing draws it, and it is not redundant.** Without it two entities
    /// with the same `Name` and the same components compare equal, so moving
    /// the selection between them leaves the panel alone. Measured before this
    /// field was here: selecting one of two identically named placeholders
    /// after the other rebuilt nothing. What is on screen is the same either
    /// way today, because this panel draws no values; the moment #43 puts a
    /// value in a row, the panel would be showing the other entity's.
    of: Entity,
    /// What the header calls the entity.
    title: String,
    /// One per component, in the order they are drawn.
    rows: Vec<Row>,
}

/// One component's row.
///
/// **The order of the two variants is the order they sort in**, which is what
/// puts every named component above every unregistered one: a derived `Ord` on
/// an enum ranks by declaration first and by the fields second, so `Named`
/// against `Named` compares the names. Swapping these two lines changes what is
/// on screen, which is why they are not in the other order by accident.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Row {
    /// A component the type registry names.
    Named(String),
    /// A component with no registration, so with no name available.
    Unregistered,
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
/// * **The type registry is the only source of a component's name.**
///   `ComponentInfo::name()` returns a `DebugName`, which carries nothing
///   unless `bevy_utils/debug` is on, and this build does not turn it on:
///   every component answers `"<Enable the debug feature to see the name>"`.
///   So the walk goes `ComponentId` -> `ComponentInfo::type_id` ->
///   `TypeRegistry::get` -> `type_path_table().short_path()`.
/// * **A component nobody registered has no name at all.** There is no
///   fallback to drop to, which is why [`UNREGISTERED`] exists. Of the 15
///   components a placeholder carries, 13 are named and 2 are not: `Selectable`
///   and anything else this workspace attaches without deriving `Reflect`.
/// * **`reflect_auto_register` is on**, through Bevy's `default_app`. A
///   component that derives `Reflect` is registered without anybody calling
///   `register_type`, so phase 3's user-defined components arrive here on their
///   own.
/// * **`World::inspect_entity` is the engine's own walk** and returns exactly
///   this, `Err` for an entity that is gone included. It takes `&World`, which
///   would make this the only exclusive system in the editor;
///   [`EntityRef::archetype`] plus `&Components` is the same walk in a system
///   the schedule can run beside others. ADR-0002 carries that choice.
///
/// # Why the resource is read and written through two different paths
///
/// [`Shown`] is a `Res` here and goes back through `Commands`, **not a
/// `ResMut`**. `Query<EntityRef>` claims read access to everything, resources
/// included, so a `ResMut` beside it is a conflict the engine panics on at
/// first run:
///
/// ```text
/// error[B0002]: ResMut<<Enable the debug feature to see the name>> in system
/// <Enable the debug feature to see the name> conflicts with a previous system
/// parameter.
/// ```
///
/// Measured. Note that the message can name neither the resource nor the
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
/// `the_header_uses_the_entitys_name_when_it_has_one` fails. Mutation: drop the
/// `of` field from [`Shape`], and
/// `the_panel_follows_the_selection_to_a_look_alike` fails. Each was applied and
/// the named test watched to fail.
fn show(
    selection: Res<Selection>,
    entities: Query<EntityRef>,
    components: &Components,
    registry: Res<AppTypeRegistry>,
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

    let registry = registry.read();
    // The last element is the most recently chosen, which is the one Unity
    // calls active. `Selection` in `selection.rs` is where that condition is
    // written down; this is its first reader.
    let wanted = selection.entities().last().and_then(|entity| {
        // `Err` rather than a panic when the entity is gone. **No test holds
        // this, because nothing can currently reach it**: `forget_what_is_gone`
        // in `selection.rs` observes `Remove<Selectable>`, observers run at the
        // despawn rather than a frame later, and `Selection` only ever names
        // something that was `Selectable`. Measured: replacing this with
        // `.expect()` leaves all eleven tests in this module green. It stays
        // because `Query::get` returns a `Result` and taking the safe arm of
        // one costs nothing, and because the entity that reaches here first
        // will be one some later gesture put in the selection without that
        // observer's knowledge. Saying so beats letting a green suite imply a
        // guard, which is RK-005's rule.
        let found = entities.get(*entity).ok()?;
        let mut rows: Vec<Row> = found
            .archetype()
            .components()
            .iter()
            .map(|id| {
                components
                    .get_info(*id)
                    .and_then(ComponentInfo::type_id)
                    .and_then(|of| registry.get(of))
                    .map_or(Row::Unregistered, |registration| {
                        Row::Named(
                            registration
                                .type_info()
                                .type_path_table()
                                .short_path()
                                .to_owned(),
                        )
                    })
            })
            .collect();
        rows.sort();
        let title = found.get::<Name>().map_or_else(
            || format!("Entity {entity}"),
            |name| name.as_str().to_owned(),
        );
        Some(Shape {
            of: *entity,
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
                Row::Named(name) => {
                    commands
                        .spawn_scene(label(name.clone()))
                        .insert(ChildOf(body));
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

    /// The component rows, without the header.
    fn rows(app: &mut App) -> Vec<String> {
        on_screen(app).into_iter().skip(1).collect()
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

        assert_eq!(title(&mut app), Some(format!("Entity {left}")));
        assert_ne!(
            title(&mut app),
            Some(format!("Entity {middle}")),
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
    #[test]
    fn the_rows_are_in_the_order_the_names_sort_in() {
        let mut app = inspector_editor();
        select_the_middle(&mut app);

        let listed = rows(&mut app);
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

    /// The panel follows the selection to an entity that looks the same.
    ///
    /// Every other test here distinguishes two entities by what is on screen,
    /// which is why none of them reaches this: the placeholders have different
    /// ids, so their headers differ, so any comparison at all rebuilds. Here
    /// they are given one name between them, and the header and the rows then
    /// match to the character.
    ///
    /// **What it costs to get wrong is #43's, not this issue's.** The two
    /// panels are identical today, so nothing is visibly wrong; the moment a
    /// row carries a value, a panel that did not rebuild is one showing the
    /// value of an entity the user is no longer looking at.
    ///
    /// Mutation: drop the `of` field from `Shape`, and this fails. Every other
    /// test in this module passes under it, measured, which is the whole reason
    /// this one exists.
    #[test]
    fn the_panel_follows_the_selection_to_a_look_alike() {
        let mut app = inspector_editor();
        let everything: Vec<Entity> = app
            .world_mut()
            .query_filtered::<Entity, With<crate::Selectable>>()
            .iter(app.world())
            .collect();
        assert!(
            everything.len() > 1,
            "there is only one thing to select, so nothing can look like another"
        );
        for entity in everything {
            app.world_mut()
                .entity_mut(entity)
                .insert(Name::new("Player"));
        }
        app.update();

        let middle = select_the_middle(&mut app);
        let first = under_the_panel(&mut app);
        let shown_first = on_screen(&mut app);
        assert!(!first.is_empty(), "nothing was built to compare");

        click_at(
            &mut app,
            in_window(Vec2::new(-200.0, 0.0)),
            PointerButton::Primary,
        );
        let left = app.world().resource::<Selection>().entities()[0];

        assert_ne!(left, middle, "the second click chose the same entity");
        assert_eq!(
            on_screen(&mut app),
            shown_first,
            "the two entities do not look the same, so this tests nothing"
        );
        assert_ne!(
            under_the_panel(&mut app),
            first,
            "the panel stayed on the entity that is no longer selected"
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
}
