//! What the editor is currently acting on, and the click that sets it.
//!
//! The inspector shows it, transform editing moves it, gizmos draw on it and
//! delete acts on it, so this is one answer rather than one per panel that
//! wants to know. The button it is bound to is
//! [`docs/specs/ui.md` §4](../../../docs/specs/ui.md).

use bevy::ecs::lifecycle::Remove;
use bevy::picking::Pickable;
use bevy::picking::events::{Click, Pointer, Press};
use bevy::picking::hover::HoverMap;
use bevy::picking::pointer::{PointerButton, PointerId};
use bevy::prelude::*;
use bevy::ui::widget::ViewportNode;

use crate::Region;

/// What the editor is currently acting on.
///
/// A list rather than one entity, and ordered rather than a set. Ordered
/// because the inspector will want the last thing chosen, which a set cannot
/// answer; a list because adding to the selection is then an addition to this
/// module rather than a rewrite of everything that reads it.
///
/// **The field is private and there is no setter.** That is what makes "one
/// place decides what is selected" a thing the compiler holds rather than a
/// rule somebody has to remember: nothing outside this module can write it.
#[derive(Resource, Default)]
pub struct Selection(Vec<Entity>);

impl Selection {
    /// What is selected, in the order it was chosen.
    pub fn entities(&self) -> &[Entity] {
        &self.0
    }
}

/// What the viewport's pointer was over when the button went down.
///
/// `Pointer<Click>` guarantees that the press and the release shared a target,
/// and the target here is the viewport's UI node rather than anything in the
/// world: the world is reached through a second pointer, so the engine's
/// guarantee says nothing about which entity ends up selected. Without this,
/// pressing one entity and letting go over another selected the second, and
/// pressing one and letting go over empty space cleared the selection that was
/// already there. Both measured.
///
/// `None` means the press was over empty space, which is a real answer rather
/// than a missing one: pressing and releasing over nothing is what clears the
/// selection.
#[derive(Resource, Default)]
struct PressedOver(Option<Entity>);

/// An entity the user can select by clicking it.
///
/// `Pickable` is required rather than left to whoever spawns one.
/// `SpritePickingSettings` says of itself that "regardless of this setting,
/// only sprites marked with `Pickable` will be considered", so a `Selectable`
/// without one is a thing that cannot be selected, which would read as a defect
/// in this file rather than as a missing component over there.
///
/// It also names the set a later rubber band has to hit-test against, and
/// leaves room for the thing that is pickable and not selectable, which is a
/// gizmo handle.
#[derive(Component, Default)]
#[require(Pickable)]
pub struct Selectable;

/// Selection, and the left button that sets it.
///
/// Mutation: leave its row out of the editor's member table, and
/// `the_group_carries_the_members_the_table_names` fails.
pub struct SelectionPlugin;

impl Plugin for SelectionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Selection>()
            .init_resource::<PressedOver>()
            .add_observer(attach)
            .add_observer(forget_what_is_gone);
    }
}

/// Listen for clicks on the viewport as soon as the region exists.
///
/// The same shape `viewport::attach` uses, and for the same reason: the region
/// belongs to another plugin, and ordering a `Startup` system after it would
/// mean cutting a public `SystemSet` into `PanelsPlugin` for this plugin's
/// benefit.
fn attach(add: On<Add, Region>, regions: Query<&Region>, mut commands: Commands) {
    if regions.get(add.entity) != Ok(&Region::Viewport) {
        return;
    }
    commands
        .entity(add.entity)
        .observe(remember)
        .observe(select);
}

/// Set the selection to what the left button was released over.
///
/// Released rather than pressed: `Pointer<Click>` fires only when the press and
/// the release share a target, so a press that was a mistake is taken back by
/// moving off before letting go. `docs/specs/ui.md` §4 records that.
///
/// **What was clicked is read out of the hover map rather than from a second
/// observer.** One physical click moves two pointers: the window's, which lands
/// on the viewport's UI node, and the viewport's own, which `viewport_picking`
/// drives in camera coordinates and which lands on what is in the world. Both
/// arrive in the same frame, in the order a `HashMap` over pointers iterates,
/// so an observer on the entity racing an observer on the region would be
/// correct or wrong by hash order. Measured.
///
/// Mutation: return before writing the selection, and `clicking_an_entity_selects_it`
/// fails. Mutation: clear on a hit and select on a miss, and
/// `clicking_empty_space_clears_the_selection` fails. Mutation: take the
/// furthest rather than the nearest, and `the_nearer_of_two_under_the_pointer_is_the_one_selected`
/// fails. Mutation: drop the comparison with [`PressedOver`], and
/// `letting_go_somewhere_else_takes_the_press_back` fails.
fn select(
    click: On<Pointer<Click>>,
    viewport: Query<&PointerId, With<ViewportNode>>,
    selectable: Query<(), With<Selectable>>,
    hover: Res<HoverMap>,
    pressed: Res<PressedOver>,
    mut selection: ResMut<Selection>,
) {
    if click.event().button != PointerButton::Primary {
        return;
    }
    // The pointer is read from the entity that carries `ViewportNode`, not from
    // the event's target: `Pointer` events propagate to ancestors, so an
    // observer on the region will one day see a click on a child of it. The
    // viewport is the only entity with both a `PointerId` and a `ViewportNode`.
    let Ok(pointer) = viewport.single() else {
        return;
    };

    let hit = under(&hover, pointer, &selectable);

    // The gesture has to end where it began. `docs/specs/ui.md` §4 asks that a
    // press somebody did not mean is taken back by moving off before letting
    // go, and this is the whole of it: the engine's own same-target guarantee
    // is about the viewport's node, which never changes during a drag across
    // it.
    if hit != pressed.0 {
        return;
    }

    selection.0.clear();
    selection.0.extend(hit);
}

/// Remember what the press was over, so that the release can be compared to it.
///
/// Mutation: record `None` regardless, and `clicking_an_entity_selects_it`
/// fails, because every press would then look like a press on empty space.
fn remember(
    press: On<Pointer<Press>>,
    viewport: Query<&PointerId, With<ViewportNode>>,
    selectable: Query<(), With<Selectable>>,
    hover: Res<HoverMap>,
    mut pressed: ResMut<PressedOver>,
) {
    if press.event().button != PointerButton::Primary {
        return;
    }
    let Ok(pointer) = viewport.single() else {
        return;
    };
    pressed.0 = under(&hover, pointer, &selectable);
}

/// The nearest selectable entity the viewport's pointer is over.
///
/// `Pickable::default` blocks what is under it, so the map usually holds one; a
/// `Pickable` that does not block puts what is behind it in the map too, which
/// is what makes the tie-break reachable and testable. `bevy_picking`'s
/// `build_hover_map` is where that branch is.
///
/// One function rather than two copies, because the press and the release ask
/// the same question and an answer that drifted between them would be a
/// misclick escape that worked in one direction.
fn under(
    hover: &HoverMap,
    pointer: &PointerId,
    selectable: &Query<(), With<Selectable>>,
) -> Option<Entity> {
    hover
        .get(pointer)
        .into_iter()
        .flatten()
        .filter(|(entity, _)| selectable.contains(**entity))
        .min_by(|a, b| a.1.depth.total_cmp(&b.1.depth))
        .map(|(entity, _)| *entity)
}

/// Drop an entity from the selection when it stops being selectable.
///
/// An `Entity` held past the despawn of what it named is not an error in Rust
/// and not a panic in Bevy: it is a query that matches nothing, until the id is
/// reused and it matches something else instead.
///
/// `Remove` covers both ways it can happen. Measured: despawning an entity
/// fires `Despawn` and `Remove`, and taking the component off fires `Remove`,
/// so observing the second alone needs no observer on the first.
///
/// Mutation: drop this observer, and `a_despawned_entity_does_not_stay_selected`
/// fails. Mutation: listen on `Despawn` instead, which is the plausible
/// narrowing because the prose around this talks about despawning, and
/// `something_that_stops_being_selectable_stops_being_selected` fails while the
/// despawn test stays green.
fn forget_what_is_gone(remove: On<Remove, Selectable>, mut selection: ResMut<Selection>) {
    selection.0.retain(|entity| *entity != remove.entity);
}

#[cfg(test)]
mod tests {
    use super::{Selectable, Selection};
    use crate::pointer::{click_at, in_window, write_input};
    use crate::viewport::PLACEHOLDERS;
    use crate::{editor, headless};
    use bevy::picking::Pickable;
    use bevy::picking::pointer::{PointerAction, PointerButton};
    use bevy::prelude::*;

    /// How many frames the editor needs before a test can look at the world.
    ///
    /// One, and measured rather than guessed: at zero the `Startup` system has
    /// not spawned the regions, so there is no viewport, no observer on it and
    /// no placeholder to name, and three tests fail. At one there is.
    ///
    /// The frames the pointer itself needs are not here. `click_at` runs two
    /// updates for each of the three inputs it writes, so the render target is
    /// sized and both picking backends have run long before anything is
    /// asserted. An earlier version of this was three, with a doc comment
    /// explaining frames that `click_at` was already providing, and nothing
    /// failed when it was one.
    const SETTLE: usize = 1;

    /// The editor, run until a test can look at the world.
    fn selection_editor() -> App {
        let mut app = editor(headless());
        for _ in 0..SETTLE {
            app.update();
        }
        app
    }

    /// Press at one place and let go at another, a frame apart.
    fn press_then_release(app: &mut App, press: Vec2, release: Vec2) {
        for (position, action) in [
            (press, PointerAction::Move { delta: Vec2::ONE }),
            (press, PointerAction::Press(PointerButton::Primary)),
            (release, PointerAction::Move { delta: Vec2::ONE }),
            (release, PointerAction::Release(PointerButton::Primary)),
        ] {
            write_input(app, position, action);
            app.update();
            app.update();
        }
    }

    /// Press, then move and let go in one frame, with no update between them.
    ///
    /// A gesture whose input batches into a single frame, which is what a long
    /// frame does: a large level, or a paint stroke later on.
    fn press_then_leave_in_one_frame(app: &mut App, press: Vec2, release: Vec2) {
        write_input(app, press, PointerAction::Move { delta: Vec2::ONE });
        write_input(app, press, PointerAction::Press(PointerButton::Primary));
        app.update();
        write_input(app, release, PointerAction::Move { delta: Vec2::ONE });
        write_input(app, release, PointerAction::Release(PointerButton::Primary));
        app.update();
        app.update();
    }

    /// What is selected.
    fn selected(app: &App) -> Vec<Entity> {
        app.world().resource::<Selection>().entities().to_vec()
    }

    /// The placeholder at a world x, by the order they are spawned in.
    fn placeholder(app: &mut App, index: usize) -> Entity {
        let mut found: Vec<(Entity, f32)> = app
            .world_mut()
            .query_filtered::<(Entity, &Transform), With<Selectable>>()
            .iter(app.world())
            .map(|(entity, transform)| (entity, transform.translation.x))
            .collect();
        found.sort_by(|a, b| a.1.total_cmp(&b.1));
        found[index].0
    }

    /// Clicking an entity selects it.
    ///
    /// The whole chain, driven from the window's own pointer: the UI backend
    /// hits the viewport node, `viewport_picking` re-emits on the viewport's
    /// pointer in camera coordinates, and `sprite_picking` finds the sprite.
    ///
    /// Mutation: return from `select` before writing the selection, and this
    /// fails.
    #[test]
    fn clicking_an_entity_selects_it() {
        let mut app = selection_editor();
        let middle = placeholder(&mut app, 1);

        click_at(&mut app, in_window(Vec2::ZERO), PointerButton::Primary);

        assert_eq!(selected(&app), [middle]);
    }

    /// Every placeholder can be selected.
    ///
    /// Three of them, because selecting one of one demonstrates nothing, and
    /// because a `Selectable` missing from one of the three is otherwise
    /// invisible: the test above would pass on the one it happens to click.
    ///
    /// Mutation: take `Selectable` off one of the rows in `PLACEHOLDERS`'
    /// spawning, and this fails.
    #[test]
    fn every_placeholder_can_be_selected() {
        for index in 0..PLACEHOLDERS.len() {
            let mut app = selection_editor();
            let wanted = placeholder(&mut app, index);
            let at = app
                .world()
                .entity(wanted)
                .get::<Transform>()
                .expect("a placeholder has a transform")
                .translation
                .truncate();

            click_at(&mut app, in_window(at), PointerButton::Primary);

            assert_eq!(selected(&app), [wanted], "placeholder {index} at {at:?}");
        }
    }

    /// The nearer of two under the pointer is the one selected.
    ///
    /// Two entities reach one pointer's hover map when the one in front does
    /// not block what is under it: `bevy_picking`'s `build_hover_map` stops at
    /// the first entity whose `Pickable` blocks, and carries on past one whose
    /// does not. So the tie-break is reachable without waiting for anything,
    /// and an earlier version of this file said in a comment that it could not
    /// be tested, which was true of the placeholders and not of the engine.
    ///
    /// Mutation: take the furthest instead of the nearest in `select`, and this
    /// fails.
    #[test]
    fn the_nearer_of_two_under_the_pointer_is_the_one_selected() {
        let mut app = selection_editor();
        let behind = placeholder(&mut app, 1);
        let in_front = app
            .world_mut()
            .spawn((
                Sprite::from_color(Color::WHITE, Vec2::splat(64.0)),
                Transform::from_xyz(0.0, 0.0, 1.0),
                Selectable,
                Pickable {
                    should_block_lower: false,
                    is_hoverable: true,
                },
            ))
            .id();
        app.update();

        click_at(&mut app, in_window(Vec2::ZERO), PointerButton::Primary);

        assert_ne!(
            selected(&app),
            [behind],
            "the one behind was selected, so the nearest is not what is taken"
        );
        assert_eq!(selected(&app), [in_front]);
    }

    /// Clicking empty space clears the selection.
    ///
    /// The position is inside the viewport region and away from every
    /// placeholder, which sit in a row on the world's x axis.
    ///
    /// Mutation: clear on a hit and select on a miss, and this fails.
    #[test]
    fn clicking_empty_space_clears_the_selection() {
        let mut app = selection_editor();
        click_at(&mut app, in_window(Vec2::ZERO), PointerButton::Primary);
        assert_eq!(selected(&app).len(), 1, "nothing was selected to clear");

        click_at(
            &mut app,
            in_window(Vec2::new(0.0, -180.0)),
            PointerButton::Primary,
        );

        assert!(selected(&app).is_empty(), "the selection was not cleared");
    }

    /// Letting go somewhere else takes the press back.
    ///
    /// This is what `docs/specs/ui.md` §4 asks of the left button: a press
    /// somebody did not mean costs nothing as long as they move off before
    /// letting go. Three gestures, and none may reach the selection.
    ///
    /// Bevy's own same-target guarantee does not give this. `Pointer<Click>`
    /// fires when the press and the release shared a target, and the target
    /// here is the viewport's UI node, which is the same node all the way
    /// across the viewport. Before the press was remembered, the first gesture
    /// selected the middle placeholder and the second cleared a selection that
    /// was already there, both measured.
    ///
    /// Mutation: drop the comparison with `PressedOver` in `select`, and this
    /// fails.
    #[test]
    fn letting_go_somewhere_else_takes_the_press_back() {
        // One: press one entity, let go over another.
        let mut app = selection_editor();
        press_then_release(
            &mut app,
            in_window(Vec2::new(-200.0, 0.0)),
            in_window(Vec2::ZERO),
        );
        assert!(
            selected(&app).is_empty(),
            "letting go over a different entity selected it"
        );

        // Two: with something already selected, press another and let go over
        // empty space.
        let mut app = selection_editor();
        click_at(
            &mut app,
            in_window(Vec2::new(-200.0, 0.0)),
            PointerButton::Primary,
        );
        let chosen = selected(&app);
        assert_eq!(chosen.len(), 1, "nothing was selected to protect");
        press_then_release(
            &mut app,
            in_window(Vec2::ZERO),
            in_window(Vec2::new(0.0, -180.0)),
        );
        assert_eq!(
            selected(&app),
            chosen,
            "a press that was taken back cleared the selection anyway"
        );

        // Three: press empty space, let go over an entity.
        let mut app = selection_editor();
        press_then_release(
            &mut app,
            in_window(Vec2::new(0.0, -180.0)),
            in_window(Vec2::ZERO),
        );
        assert!(
            selected(&app).is_empty(),
            "a gesture that began on empty space selected something"
        );
    }

    /// A gesture that leaves the viewport in one frame keeps the selection.
    ///
    /// `Pointer<Click>` is dispatched from the previous frame's hover while
    /// what is under the pointer is read from this frame's, so a release
    /// arriving in the same frame as the move out of the viewport is a click on
    /// the viewport node with nothing under the viewport's own pointer. Before
    /// the press was remembered that cleared the selection, measured. An input
    /// batch is what a long frame produces, and a long frame is what a large
    /// level produces.
    ///
    /// Mutation: drop the comparison with `PressedOver` in `select`, and this
    /// fails.
    #[test]
    fn a_gesture_that_leaves_the_viewport_in_one_frame_keeps_the_selection() {
        let mut app = selection_editor();
        click_at(&mut app, in_window(Vec2::ZERO), PointerButton::Primary);
        let chosen = selected(&app);
        assert_eq!(chosen.len(), 1, "nothing was selected to protect");

        // 1150 across is inside the inspector, which starts 980 across.
        press_then_leave_in_one_frame(&mut app, in_window(Vec2::ZERO), Vec2::new(1150.0, 284.0));

        assert_eq!(
            selected(&app),
            chosen,
            "leaving the viewport in one frame cleared the selection"
        );
    }

    /// Something pickable that is not selectable is not selected.
    ///
    /// `Pickable` is what the engine needs to consider a sprite at all, and
    /// `Selectable` is what this editor means by something the user picks out.
    /// Without the difference, a gizmo handle would become selectable the day
    /// it is drawn.
    ///
    /// Mutation: drop the `With<Selectable>` filter in `select`, and this
    /// fails.
    #[test]
    fn something_pickable_that_is_not_selectable_is_not_selected() {
        let mut app = selection_editor();
        // In front of the middle placeholder, so that it is what the pointer
        // reaches first and the one below it is blocked.
        app.world_mut().spawn((
            Sprite::from_color(Color::WHITE, Vec2::splat(64.0)),
            Transform::from_xyz(0.0, 0.0, 1.0),
            Pickable::default(),
        ));
        app.update();

        click_at(&mut app, in_window(Vec2::ZERO), PointerButton::Primary);

        assert!(
            selected(&app).is_empty(),
            "something that is not selectable was selected"
        );
    }

    /// The other buttons do not select.
    ///
    /// The middle button pans and the right button is being kept for a context
    /// menu, both in `docs/specs/ui.md` §4.
    ///
    /// Mutation: drop the `PointerButton::Primary` check in `select`, and this
    /// fails.
    #[test]
    fn the_other_buttons_do_not_select() {
        for button in [PointerButton::Middle, PointerButton::Secondary] {
            let mut app = selection_editor();

            click_at(&mut app, in_window(Vec2::ZERO), button);

            assert!(selected(&app).is_empty(), "{button:?} selected something");
        }
    }

    /// A despawned entity does not stay selected.
    ///
    /// An `Entity` held past the despawn of what it named is a query that
    /// matches nothing, until the id is reused and it matches something else.
    ///
    /// Mutation: drop the `forget_what_is_gone` observer, and this fails.
    #[test]
    fn a_despawned_entity_does_not_stay_selected() {
        let mut app = selection_editor();
        click_at(&mut app, in_window(Vec2::ZERO), PointerButton::Primary);
        let selected_entity = selected(&app)[0];

        app.world_mut().entity_mut(selected_entity).despawn();
        app.update();

        assert!(
            selected(&app).is_empty(),
            "the selection still names a despawned entity"
        );
    }

    /// Something that stops being selectable stops being selected.
    ///
    /// The other half of what `forget_what_is_gone` claims. Despawning is not
    /// the only way an entity leaves the selection: taking `Selectable` off one
    /// that is still alive does it too, and that is the case `Remove` covers and
    /// `Despawn` does not.
    ///
    /// Without this, swapping the observer to `Despawn` left every test green,
    /// measured, which makes the narrowing invisible to whoever reads the
    /// surrounding prose about despawning and simplifies it.
    ///
    /// Mutation: listen on `Despawn` rather than `Remove`, and this fails while
    /// `a_despawned_entity_does_not_stay_selected` passes.
    #[test]
    fn something_that_stops_being_selectable_stops_being_selected() {
        let mut app = selection_editor();
        click_at(&mut app, in_window(Vec2::ZERO), PointerButton::Primary);
        let chosen = selected(&app)[0];

        app.world_mut().entity_mut(chosen).remove::<Selectable>();
        app.update();

        assert!(
            selected(&app).is_empty(),
            "the selection still names something that is no longer selectable"
        );
    }

    /// Selecting changes the selection and leaves the world where it was.
    ///
    /// The second half is the claim, and it is asserted over a set `select`
    /// does not write to: every `GlobalTransform` in the world, before and
    /// after. Marking a selection by writing to what is selected would be an
    /// edit the user did not ask for, and it is the shape
    /// `viewport::tests::a_pan_moves_the_camera_and_leaves_the_world_where_it_was`
    /// guards for the camera.
    ///
    /// Mutation: write anything to the entity that was selected, and this
    /// fails.
    #[test]
    fn selecting_changes_the_selection_and_leaves_the_world_where_it_was() {
        let mut app = selection_editor();
        let world_of = |app: &mut App| -> Vec<(Entity, GlobalTransform)> {
            let mut found: Vec<(Entity, GlobalTransform)> = app
                .world_mut()
                .query::<(Entity, &GlobalTransform)>()
                .iter(app.world())
                .map(|(entity, transform)| (entity, *transform))
                .collect();
            found.sort_by_key(|(entity, _)| *entity);
            found
        };

        let before = world_of(&mut app);
        click_at(&mut app, in_window(Vec2::ZERO), PointerButton::Primary);
        let after = world_of(&mut app);

        assert_eq!(selected(&app).len(), 1, "nothing was selected");
        assert!(!before.is_empty(), "there is nothing in the world to hold");
        assert_eq!(before, after, "selecting moved the world");
    }

    /// A click outside the viewport does not reach the selection.
    ///
    /// The inspector and the asset browser are panes beside the viewport, and a
    /// click on one of them is not a click in the world. `viewport_picking`
    /// only drives the viewport's pointer while the viewport is the thing being
    /// hovered, so this is the engine's behaviour rather than this file's; it
    /// is asserted because the day it stops being true, everything here is
    /// wrong and nothing else would say so.
    ///
    /// Mutation: attach to every region rather than to `Region::Viewport`, and
    /// this fails: the observer then fires for a click on a pane, and the
    /// viewport's own pointer is over nothing, so the selection is cleared.
    #[test]
    fn a_click_outside_the_viewport_does_not_reach_the_selection() {
        let mut app = selection_editor();
        click_at(&mut app, in_window(Vec2::ZERO), PointerButton::Primary);
        let chosen = selected(&app);
        assert_eq!(chosen.len(), 1, "nothing was selected to disturb");

        // 120 across is inside the asset browser, which is 240 wide.
        click_at(&mut app, Vec2::new(120.0, 284.0), PointerButton::Primary);

        assert_eq!(
            selected(&app),
            chosen,
            "a click on a pane changed the selection"
        );
    }
}
