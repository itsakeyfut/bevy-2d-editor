//! Moving what is selected by dragging it, and recording the move.
//!
//! What turns a press into a move, and what moves, is decided in
//! `selection.rs`, because the click, the box and the move are one gesture
//! table: [`docs/specs/ui.md` §4](../../../docs/specs/ui.md). What dragging
//! does, and what was turned down for it, is §11 of the same file.

use bevy::input_focus::InputFocusSystems;
use bevy::picking::events::{DragEnd, Pointer, Press};
use bevy::picking::pointer::{PointerButton, PointerLocation};
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy::ui::widget::ViewportNode;

use crate::Region;
use crate::history::{EditorCommand, History, put_back, take_back};
use crate::selection::{pointer_world, record_choice};
use crate::viewport::ViewportCamera;

/// The move in progress, if there is one.
#[derive(Resource, Default)]
pub(crate) struct Stroke(Option<Held>);

/// A move that has begun and has not been recorded yet.
struct Held {
    /// What moves.
    group: Vec<Entity>,
    /// Where the press was, in world units.
    anchor: Vec2,
    /// Where each entity was when the move began.
    ///
    /// `None` until [`follow`] has run once, **which is after the frame's
    /// focus changes**. The press that begins a move takes the focus out of a
    /// number box, which commits when `FocusLost` reaches it in `PostUpdate`;
    /// read any earlier, in the frame where the press and the first move land
    /// together, this would hold the number from before that commit, and
    /// undoing the move would write it over the commit.
    from: Option<Vec<(Entity, Vec3)>>,
    /// Whether the button has been let go.
    ended: bool,
}

/// Moving what is selected, and recording it.
///
/// Mutation: leave its row out of the editor's member table, and
/// `the_group_carries_the_members_the_table_names` fails.
pub struct MovePlugin;

impl Plugin for MovePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Stroke>()
            .add_systems(
                PostUpdate,
                follow
                    .after(InputFocusSystems::FocusChangeEvents)
                    .after(record_choice)
                    .before(take_back)
                    .before(put_back)
                    .before(TransformSystems::Propagate),
            )
            .add_observer(attach);
    }
}

/// Listen for the press and the release on the viewport, as soon as the
/// region exists.
///
/// The shape `selection::attach` uses, and for its reason. A second set of
/// observers on the same region rather than lines in `selection.rs`'s, because
/// what the press and the release mean for a move is this module's, and the
/// gesture's own observers already carry as many parameters as clippy allows.
fn attach(add: On<Add, Region>, regions: Query<&Region>, mut commands: Commands) {
    if regions.get(add.entity) != Ok(&Region::Viewport) {
        return;
    }
    commands
        .entity(add.entity)
        .observe(record_held)
        .observe(let_go);
}

/// Record a move that is still held when the left button goes down again.
///
/// **This is the way out for a drag whose release never arrives**, which a
/// window losing focus mid-drag produces: `Pressed` in `selection.rs` records
/// that for the box. Queued rather than done here, so that it lands before the
/// `start` a drag in the same frame queues after it.
///
/// Mutation: return before queueing, and
/// `a_drag_whose_release_is_lost_is_recorded_at_the_next_press` fails.
fn record_held(press: On<Pointer<Press>>, mut commands: Commands) {
    if press.event().button == PointerButton::Primary {
        commands.queue(finish_now);
    }
}

/// Say that the left button was let go. [`follow`] records the move in the
/// same frame.
///
/// Queued, so that it lands after the `start` of a drag that begins and ends
/// in one frame.
fn let_go(end: On<Pointer<DragEnd>>, mut commands: Commands) {
    if end.event().button == PointerButton::Primary {
        commands.queue(|world: &mut World| {
            if let Some(held) = world.resource_mut::<Stroke>().0.as_mut() {
                held.ended = true;
            }
        });
    }
}

/// Begin a move of `group`, from a press at `anchor`.
///
/// Nothing is held when this runs: the press that begins this move has
/// already recorded any earlier one, through [`record_held`].
pub(crate) fn start(world: &mut World, group: Vec<Entity>, anchor: Vec2) {
    world.resource_mut::<Stroke>().0 = Some(Held {
        group,
        anchor,
        from: None,
        ended: false,
    });
}

/// Record a move that is still held, where it is now.
fn finish_now(world: &mut World) {
    let Some(held) = world.resource_mut::<Stroke>().0.take() else {
        return;
    };
    if let Some(from) = held.from {
        record(world, from);
    }
}

/// Whether a move is in progress.
///
/// Undo and redo do nothing while one is: an older position written now would
/// be written over by the next frame of the drag, and the key would look
/// broken. [`docs/specs/ui.md` §11](../../../docs/specs/ui.md).
pub(crate) fn in_progress(world: &World) -> bool {
    world.resource::<Stroke>().0.is_some()
}

/// Move the group with the pointer, and record the move when the button has
/// been let go.
///
/// Where it sits in `PostUpdate` is the whole of its correctness, and
/// [`MovePlugin`] says it once:
///
/// * after the focus changes, so that [`Held::from`] is read after the commit
///   the press caused
/// * after `record_choice`, so that selecting an entity by dragging it is the
///   entry below the move
/// * before the undo and redo keys, so that letting go and Ctrl+Z in one frame
///   take back the move
/// * before propagation, so that the outline and picking read this frame's
///   position
///
/// **The delta is added to `translation` as it is**, a world delta written as
/// a local one. That holds while nothing in the editor has a parent, and a
/// hierarchy is what breaks it.
///
/// **A translation is only written when it changes**, so that holding the
/// button still does not mark every entity changed every frame.
///
/// Mutation: return before writing, and
/// `dragging_a_selected_entity_moves_it_by_what_the_pointer_moved` fails.
/// Mutation: read [`Held::from`] in `selection::grab` rather than here, and
/// `a_number_the_press_commits_is_under_the_move_in_the_history` fails.
/// Mutation: record on every frame rather than when [`Held::ended`], and
/// `one_ctrl_z_after_a_drag_puts_every_entity_back_bit_for_bit` fails.
fn follow(
    mut stroke: ResMut<Stroke>,
    viewport: Query<&PointerLocation, With<ViewportNode>>,
    cameras: Query<(&Camera, &GlobalTransform), With<ViewportCamera>>,
    mut transforms: Query<&mut Transform>,
    mut commands: Commands,
) {
    let Some(held) = stroke.0.as_mut() else {
        return;
    };
    let from = held.from.get_or_insert_with(|| {
        held.group
            .iter()
            .filter_map(|entity| Some((*entity, transforms.get(*entity).ok()?.translation)))
            .collect()
    });

    let now = viewport
        .single()
        .ok()
        .and_then(|location| pointer_world(location, &cameras));
    if let Some(now) = now {
        let delta = (now - held.anchor).extend(0.0);
        for (entity, at) in from.iter() {
            let Ok(mut transform) = transforms.get_mut(*entity) else {
                continue;
            };
            let to = *at + delta;
            if transform.translation != to {
                transform.translation = to;
            }
        }
    }

    if held.ended
        && let Some(from) = stroke.0.take().and_then(|held| held.from)
    {
        commands.queue(move |world: &mut World| record(world, from));
    }
}

/// Put a finished move in the history, if it moved anything.
///
/// **Through [`History::record`], whose `execute` writes where the entities
/// already are**: the drag wrote them there frame by frame, so the first
/// `execute` changes nothing, and `History` keeps one way in.
/// [`docs/specs/ui.md` §11](../../../docs/specs/ui.md) has what was turned down
/// for it.
///
/// **A move that leaves every entity's bits where they were is not an entry**,
/// for the reason §8 gives for a commit that changes nothing. An entity gone
/// since the move began is left out.
///
/// Mutation: drop the bits comparison, and
/// `a_drag_that_comes_back_to_where_it_started_records_nothing` fails.
fn record(world: &mut World, from: Vec<(Entity, Vec3)>) {
    let moved: Vec<Moved> = from
        .into_iter()
        .filter_map(|(entity, from)| {
            let to = world.get::<Transform>(entity)?.translation;
            Some(Moved { entity, from, to })
        })
        .collect();
    if moved.iter().all(|moved| bits(moved.from) == bits(moved.to)) {
        return;
    }
    History::record(world, MoveEntities(moved));
}

/// A vector's bits, so that two positions compare as the same number rather
/// than as two floats that are merely equal.
fn bits(at: Vec3) -> [u32; 3] {
    at.to_array().map(f32::to_bits)
}

/// One entity's part of a move.
struct Moved {
    entity: Entity,
    /// What undo writes back.
    from: Vec3,
    /// What redo writes.
    to: Vec3,
}

/// A move of everything that was dragged together.
///
/// The command of [`docs/specs/data-model.md` §1](../../../docs/specs/data-model.md),
/// writing the component directly because there is no Editor Model yet.
///
/// Mutation: write `from` in `execute`, and
/// `redo_after_undoing_a_drag_moves_them_again` fails.
struct MoveEntities(Vec<Moved>);

impl EditorCommand for MoveEntities {
    fn execute(&mut self, world: &mut World) {
        for moved in &self.0 {
            put(world, moved.entity, moved.to);
        }
    }

    fn undo(&mut self, world: &mut World) {
        for moved in &self.0 {
            put(world, moved.entity, moved.from);
        }
    }
}

/// Write a translation, and nothing when the entity is gone, as `write_leaf`
/// in `inspector.rs` does and for the reason it gives.
fn put(world: &mut World, entity: Entity, at: Vec3) {
    if let Some(mut transform) = world.get_mut::<Transform>(entity) {
        transform.translation = at;
    }
}

#[cfg(test)]
mod tests {
    use crate::pointer::{
        click_at, hold_key, hold_letter, in_window, press_and_hold, press_then_release,
        release_key, release_letter, write_input,
    };
    use crate::selection::{Selectable, Selection};
    use crate::viewport::ViewportCamera;
    use crate::{editor, headless};
    use bevy::picking::pointer::{PointerAction, PointerButton};
    use bevy::prelude::*;
    use core::f32::consts::FRAC_PI_4;

    /// Where the left placeholder is, in world units. Repeated rather than
    /// read from `PLACEHOLDERS`, for the reason `in_window` gives.
    const LEFT: Vec2 = Vec2::new(-200.0, 0.0);
    /// Where the middle placeholder is, in world units.
    const MIDDLE: Vec2 = Vec2::ZERO;

    /// The editor, one frame in, which is when the placeholders exist.
    fn moving_editor() -> App {
        let mut app = editor(headless());
        app.update();
        app
    }

    /// The selectable entity whose origin is at `at`.
    fn entity_at(app: &mut App, at: Vec2) -> Entity {
        app.world_mut()
            .query_filtered::<(Entity, &Transform), With<Selectable>>()
            .iter(app.world())
            .find(|(_, transform)| transform.translation.truncate() == at)
            .map(|(entity, _)| entity)
            .expect("something selectable is there")
    }

    /// Where an entity is.
    fn translation(app: &App, entity: Entity) -> Vec3 {
        app.world()
            .get::<Transform>(entity)
            .expect("it has a transform")
            .translation
    }

    /// Whether a dragged entity is where the pointer put it.
    ///
    /// Near rather than equal: the pointer's position goes through the
    /// camera's inverse, which leaves the last bit or two of a world position
    /// in the tens off, measured at `50.000015` for `50`. What these tests hold
    /// is where a drag goes; bit for bit is the undo's claim, and it is
    /// compared by its bits.
    fn assert_near(at: Vec3, wanted: Vec3) {
        assert!(at.abs_diff_eq(wanted, 1e-3), "{at} is not {wanted}");
    }

    /// A position's bits, so that "bit for bit" is what is compared.
    fn bits(at: Vec3) -> [u32; 3] {
        at.to_array().map(f32::to_bits)
    }

    /// What is selected.
    fn selected(app: &App) -> Vec<Entity> {
        app.world().resource::<Selection>().entities().to_vec()
    }

    /// Click with the modifier held.
    fn modifier_click_at(app: &mut App, position: Vec2) {
        hold_key(app, KeyCode::ControlLeft);
        click_at(app, position, PointerButton::Primary);
        release_key(app, KeyCode::ControlLeft);
    }

    /// Ctrl and a letter, pressed and let go.
    fn ctrl_letter(app: &mut App, key: KeyCode, letter: &str) {
        hold_key(app, KeyCode::ControlLeft);
        hold_letter(app, key, letter);
        app.update();
        release_letter(app, key, letter);
        release_key(app, KeyCode::ControlLeft);
        app.update();
        app.update();
    }

    /// Ctrl+Z.
    fn undo(app: &mut App) {
        ctrl_letter(app, KeyCode::KeyZ, "z");
    }

    /// Ctrl+Y.
    fn redo(app: &mut App) {
        ctrl_letter(app, KeyCode::KeyY, "y");
    }

    /// A 64 by 64 selectable sprite, spawned now.
    fn sprite(app: &mut App, transform: Transform) -> Entity {
        let entity = app
            .world_mut()
            .spawn((
                Sprite::from_color(Color::WHITE, Vec2::splat(64.0)),
                transform,
                Selectable,
            ))
            .id();
        app.update();
        entity
    }

    /// Dragging a selected entity moves it by what the pointer moved.
    ///
    /// Mutation: return in `follow` before writing, and this fails.
    #[test]
    fn dragging_a_selected_entity_moves_it_by_what_the_pointer_moved() {
        let mut app = moving_editor();
        let middle = entity_at(&mut app, MIDDLE);
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);

        press_then_release(
            &mut app,
            in_window(MIDDLE),
            in_window(Vec2::new(50.0, 30.0)),
        );

        assert_near(translation(&app, middle), Vec3::new(50.0, 30.0, 0.0));
    }

    /// Zoomed out, the entity moves by the world distance under the pointer
    /// and not by the pixels the pointer crossed.
    ///
    /// The zoom is set on the projection rather than scrolled to, because it
    /// is not what is under test; the drag is the gesture.
    ///
    /// Mutation: take the delta from `Drag::distance`, in pixels, in place of
    /// `pointer_world`, and this fails.
    #[test]
    fn a_drag_moves_the_same_world_distance_when_zoomed() {
        let mut app = moving_editor();
        let middle = entity_at(&mut app, MIDDLE);
        {
            let world = app.world_mut();
            let mut projection = world
                .query_filtered::<&mut Projection, With<ViewportCamera>>()
                .single_mut(world)
                .expect("one viewport camera");
            let Projection::Orthographic(orthographic) = &mut *projection else {
                panic!("the viewport camera is orthographic");
            };
            orthographic.scale = 2.0;
        }
        app.update();
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);

        // Fifty pixels to the right of the viewport's middle, which at a scale
        // of two is a hundred world units.
        press_then_release(&mut app, in_window(MIDDLE), in_window(Vec2::new(50.0, 0.0)));

        assert_near(translation(&app, middle), Vec3::new(100.0, 0.0, 0.0));
    }

    /// A turned and scaled entity moves by what the pointer moved, in the
    /// world, and not along its own axes.
    ///
    /// RK-006: every placeholder is unturned and unscaled, so this spawns the
    /// awkward one.
    ///
    /// Mutation: put the delta through the entity's rotation and scale, and
    /// this fails.
    #[test]
    fn a_turned_and_scaled_entity_moves_by_what_the_pointer_moved() {
        let mut app = moving_editor();
        let at = Vec3::new(0.0, -150.0, 0.0);
        let turned = sprite(
            &mut app,
            Transform::from_translation(at)
                .with_rotation(Quat::from_rotation_z(FRAC_PI_4))
                .with_scale(Vec3::splat(2.0)),
        );
        click_at(&mut app, in_window(at.truncate()), PointerButton::Primary);
        assert_eq!(selected(&app), [turned], "the click selected it");

        press_then_release(
            &mut app,
            in_window(at.truncate()),
            in_window(Vec2::new(40.0, -130.0)),
        );

        assert_near(translation(&app, turned), Vec3::new(40.0, -130.0, 0.0));
    }

    /// Dragging one of several selected moves all of them, and the release
    /// does not collapse the selection to the one under the pointer.
    ///
    /// RK-007 and RK-008: two entities, and a release that ends over the one
    /// pressed, where the click that accompanies it would act.
    ///
    /// Mutation: move only the entity pressed, and this fails. Mutation: drop
    /// the `pressed.moving` check in `selection::select`, and this fails with
    /// the selection collapsed to the middle placeholder.
    #[test]
    fn dragging_one_of_several_selected_moves_them_all_and_keeps_the_selection() {
        let mut app = moving_editor();
        let middle = entity_at(&mut app, MIDDLE);
        let left = entity_at(&mut app, LEFT);
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);
        modifier_click_at(&mut app, in_window(LEFT));

        press_then_release(
            &mut app,
            in_window(MIDDLE),
            in_window(Vec2::new(20.0, -40.0)),
        );

        assert_near(translation(&app, middle), Vec3::new(20.0, -40.0, 0.0));
        assert_near(translation(&app, left), Vec3::new(-180.0, -40.0, 0.0));
        assert_eq!(selected(&app), [middle, left]);
    }

    /// One Ctrl+Z after a drag puts every entity back, bit for bit, and the
    /// next one reaches what came before the drag.
    ///
    /// The entities start at positions that are not round, so a restore that
    /// went through a rounding would show. The second Ctrl+Z is what makes
    /// "one entry" checkable: a drag recorded as several entries leaves the
    /// second one taking back another piece of the same drag.
    ///
    /// Mutation: record on every frame in `follow`, rather than when the
    /// button is let go, and this fails.
    #[test]
    fn one_ctrl_z_after_a_drag_puts_every_entity_back_bit_for_bit() {
        let mut app = moving_editor();
        let a_at = Vec3::new(0.1, -150.3, 0.7);
        let b_at = Vec3::new(120.9, -149.7, 0.3);
        let a = sprite(&mut app, Transform::from_translation(a_at));
        let b = sprite(&mut app, Transform::from_translation(b_at));
        click_at(&mut app, in_window(a_at.truncate()), PointerButton::Primary);
        modifier_click_at(&mut app, in_window(b_at.truncate()));
        assert_eq!(selected(&app), [a, b]);

        press_then_release(
            &mut app,
            in_window(a_at.truncate()),
            in_window(Vec2::new(60.0, -100.0)),
        );
        assert_ne!(translation(&app, a), a_at, "the drag moved nothing");

        undo(&mut app);
        assert_eq!(bits(translation(&app, a)), bits(a_at));
        assert_eq!(bits(translation(&app, b)), bits(b_at));

        undo(&mut app);
        assert_eq!(
            selected(&app),
            [a],
            "the second Ctrl+Z did not reach the modifier click"
        );
    }

    /// Redo after undoing a drag moves the entities again.
    ///
    /// Mutation: write `from` in `MoveEntities::execute`, and this fails.
    #[test]
    fn redo_after_undoing_a_drag_moves_them_again() {
        let mut app = moving_editor();
        let middle = entity_at(&mut app, MIDDLE);
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);
        press_then_release(
            &mut app,
            in_window(MIDDLE),
            in_window(Vec2::new(50.0, 30.0)),
        );

        undo(&mut app);
        assert_eq!(translation(&app, middle), Vec3::ZERO);
        redo(&mut app);

        assert_near(translation(&app, middle), Vec3::new(50.0, 30.0, 0.0));
    }

    /// A drag that comes back to where it started is not an entry.
    ///
    /// Mutation: drop the bits comparison in `record`, and this fails, because
    /// the Ctrl+Z is spent on the move that did nothing and the selection
    /// stays.
    #[test]
    fn a_drag_that_comes_back_to_where_it_started_records_nothing() {
        let mut app = moving_editor();
        let middle = entity_at(&mut app, MIDDLE);
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);

        press_and_hold(&mut app, in_window(MIDDLE), in_window(Vec2::new(50.0, 0.0)));
        for action in [
            PointerAction::Move { delta: Vec2::ONE },
            PointerAction::Release(PointerButton::Primary),
        ] {
            write_input(&mut app, in_window(MIDDLE), action);
            app.update();
            app.update();
        }
        assert_eq!(translation(&app, middle), Vec3::ZERO);

        undo(&mut app);

        assert!(
            selected(&app).is_empty(),
            "Ctrl+Z took back something other than the click"
        );
    }

    /// A press on an entity that moves less than the threshold is a click,
    /// and moves nothing.
    ///
    /// Mutation: a `MOVE_THRESHOLD` of zero, and this fails.
    #[test]
    fn a_press_that_moves_less_than_the_threshold_is_a_click() {
        let mut app = moving_editor();
        let middle = entity_at(&mut app, MIDDLE);

        press_then_release(&mut app, in_window(MIDDLE), in_window(Vec2::new(3.0, 0.0)));

        assert_eq!(selected(&app), [middle]);
        assert_eq!(translation(&app, middle), Vec3::ZERO);
    }

    /// Dragging an entity that is not selected selects it and moves it, and
    /// two Ctrl+Z take back the move and then the selection.
    ///
    /// Mutation: move what was selected before, not the entity pressed, in
    /// `selection::grab`, and this fails.
    #[test]
    fn dragging_an_unselected_entity_selects_it_and_moves_it() {
        let mut app = moving_editor();
        let middle = entity_at(&mut app, MIDDLE);
        let left = entity_at(&mut app, LEFT);
        click_at(&mut app, in_window(LEFT), PointerButton::Primary);

        press_then_release(&mut app, in_window(MIDDLE), in_window(Vec2::new(0.0, 50.0)));
        assert_eq!(selected(&app), [middle]);
        assert_near(translation(&app, middle), Vec3::new(0.0, 50.0, 0.0));
        assert_eq!(translation(&app, left), LEFT.extend(0.0));

        undo(&mut app);
        assert_eq!(translation(&app, middle), Vec3::ZERO);
        assert_eq!(selected(&app), [middle]);

        undo(&mut app);
        assert_eq!(selected(&app), [left]);
    }

    /// With the modifier held, dragging an entity that is not selected adds
    /// it and moves everything selected.
    ///
    /// Mutation: ignore `Pressed::additive` in `selection::grab`, and this
    /// fails.
    #[test]
    fn a_modifier_drag_on_an_unselected_entity_adds_it_and_moves_everything() {
        let mut app = moving_editor();
        let middle = entity_at(&mut app, MIDDLE);
        let left = entity_at(&mut app, LEFT);
        click_at(&mut app, in_window(LEFT), PointerButton::Primary);

        hold_key(&mut app, KeyCode::ControlLeft);
        press_then_release(&mut app, in_window(MIDDLE), in_window(Vec2::new(0.0, 50.0)));
        release_key(&mut app, KeyCode::ControlLeft);

        assert_eq!(selected(&app), [left, middle]);
        assert_near(translation(&app, middle), Vec3::new(0.0, 50.0, 0.0));
        assert_near(translation(&app, left), Vec3::new(-200.0, 50.0, 0.0));
    }

    /// Ctrl+Z during a drag does nothing.
    ///
    /// What shows it is the history afterwards. An undo in the middle of a
    /// drag writes an older position that the next frame of the drag writes
    /// over, so the drag ends where it would have; but the entry it took back
    /// is then dropped by the drag's own entry, and two Ctrl+Z no longer reach
    /// the first move.
    ///
    /// Mutation: drop the `moving::in_progress` check in `history::take_back`,
    /// and this fails.
    #[test]
    fn ctrl_z_during_a_drag_does_nothing() {
        let mut app = moving_editor();
        let middle = entity_at(&mut app, MIDDLE);
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);
        press_then_release(&mut app, in_window(MIDDLE), in_window(Vec2::new(30.0, 0.0)));

        let to = in_window(Vec2::new(60.0, 0.0));
        press_and_hold(&mut app, in_window(Vec2::new(30.0, 0.0)), to);
        undo(&mut app);
        write_input(&mut app, to, PointerAction::Release(PointerButton::Primary));
        app.update();
        app.update();
        assert_near(translation(&app, middle), Vec3::new(60.0, 0.0, 0.0));

        undo(&mut app);
        undo(&mut app);

        assert_eq!(translation(&app, middle), Vec3::ZERO);
    }

    /// A drag whose release never arrives is recorded at the next press.
    ///
    /// `PointerAction::Cancel` clears `bevy_picking`'s state for the pointer
    /// with no `DragEnd`, which is the state a window losing focus mid-drag
    /// leaves.
    ///
    /// Mutation: return in `record_held` before queueing, and this fails.
    #[test]
    fn a_drag_whose_release_is_lost_is_recorded_at_the_next_press() {
        let mut app = moving_editor();
        let middle = entity_at(&mut app, MIDDLE);
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);
        let to = in_window(Vec2::new(40.0, 0.0));
        press_and_hold(&mut app, in_window(MIDDLE), to);
        for action in [
            PointerAction::Cancel,
            PointerAction::Press(PointerButton::Primary),
            PointerAction::Release(PointerButton::Primary),
        ] {
            write_input(&mut app, to, action);
            app.update();
            app.update();
        }
        assert_near(translation(&app, middle), Vec3::new(40.0, 0.0, 0.0));

        undo(&mut app);

        assert_eq!(translation(&app, middle), Vec3::ZERO);
    }

    /// Dragging with the middle button moves nothing: that button drags the
    /// view.
    ///
    /// The click first is what makes it reachable: `Pressed` is only written
    /// by the left button, so without an earlier left press on the entity the
    /// check has nothing to stop.
    ///
    /// Mutation: drop the button check in `selection::grab`, and this fails.
    #[test]
    fn a_middle_button_drag_moves_nothing() {
        let mut app = moving_editor();
        let middle = entity_at(&mut app, MIDDLE);
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);

        for (position, action) in [
            (MIDDLE, PointerAction::Press(PointerButton::Middle)),
            (
                Vec2::new(50.0, 0.0),
                PointerAction::Move { delta: Vec2::ONE },
            ),
            (
                Vec2::new(50.0, 0.0),
                PointerAction::Release(PointerButton::Middle),
            ),
        ] {
            write_input(&mut app, in_window(position), action);
            app.update();
            app.update();
        }

        assert_eq!(translation(&app, middle), Vec3::ZERO);
    }
}
