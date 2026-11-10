//! Moving and turning what is selected by dragging it, and recording it.
//!
//! What turns a press into a drag, and what it takes hold of, is decided in
//! `selection.rs`, because the click, the box and the drag are one gesture
//! table: [`docs/specs/ui.md` §4](../../../docs/specs/ui.md). What dragging
//! the body does, and what was turned down for it, is §11 of the same file;
//! what dragging the rotate knob does is §12.

use bevy::input_focus::InputFocusSystems;
use bevy::math::ops;
use bevy::picking::events::{PointerDragEnd, PointerPress};
use bevy::picking::pointer::{PointerButton, PointerLocation};
use bevy::prelude::*;
use bevy::ui::widget::ViewportNode;

use crate::Region;
use crate::history::{EditorCommand, History, put_back, take_back};
use crate::selection::{pointer_world, record_choice};
use crate::viewport::ViewportCamera;

/// The drag in progress, if there is one.
#[derive(Resource, Default)]
pub(crate) struct Stroke(Option<Held>);

impl Stroke {
    /// The pivot and the angle turned so far, while the rotate knob is held
    /// and [`follow`] has run at least once.
    ///
    /// For the rotate knob alone, which is what it is named for: a handle
    /// that does something else, as scale will, carries its own read-back
    /// rather than stretching this one.
    pub(crate) fn turned(&self) -> Option<(Grip, Vec2, f32)> {
        let held = self.0.as_ref()?;
        let (pivot, angle) = held.turned?;
        Some((held.grip, pivot, angle))
    }
}

/// What the press took hold of.
#[derive(Clone, Copy)]
pub(crate) enum Grip {
    /// The entity's body: the group moves with the pointer.
    Body,
    /// The rotate knob: the group turns about the view axis.
    Knob {
        /// The centre of the selection's frame at the press, which is the
        /// pivot when more than one entity turns.
        centre: Vec2,
        /// Where the stalk left the frame at the press, in world units.
        base: Vec2,
        /// Where the knob was at the press, in world units.
        tip: Vec2,
    },
}

/// A drag that has begun and has not been recorded yet.
struct Held {
    /// What moves or turns.
    group: Vec<Entity>,
    /// What the press took hold of.
    grip: Grip,
    /// Where the press was, in world units.
    anchor: Vec2,
    /// The pivot and the angle turned so far, written by [`follow`] while
    /// the knob is held, so that `handles::place_knob` can keep the knob
    /// under the pointer.
    turned: Option<(Vec2, f32)>,
    /// Each entity's transform when the drag began.
    ///
    /// `None` until [`follow`] has run once, **which is after the frame's
    /// focus changes**. The press that begins a move takes the focus out of a
    /// number box, which commits when `FocusLost` reaches it in `PostUpdate`;
    /// read any earlier, in the frame where the press and the first move land
    /// together, this would hold the number from before that commit, and
    /// undoing the move would write it over the commit.
    from: Option<Vec<(Entity, Transform)>>,
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
                    .before(put_back),
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
fn attach(add: On<Add<Region>>, regions: Query<&Region>, mut commands: Commands) {
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
fn record_held(press: On<PointerPress>, mut commands: Commands) {
    if press.button == PointerButton::Primary {
        commands.queue(finish_now);
    }
}

/// Say that the left button was let go. [`follow`] records the move in the
/// same frame.
///
/// Queued, so that it lands after the `start` of a drag that begins and ends
/// in one frame.
///
/// **The button check is one line no test holds**, as the same check in
/// `selection::finish` is not: reaching it takes a second button let go
/// during a left drag. Without it, letting go of the middle button ends the
/// move there, and the rest of the drag moves nothing.
fn let_go(end: On<PointerDragEnd>, mut commands: Commands) {
    if end.button == PointerButton::Primary {
        commands.queue(|world: &mut World| {
            if let Some(held) = world.resource_mut::<Stroke>().0.as_mut() {
                held.ended = true;
            }
        });
    }
}

/// Begin a drag of `group`, holding `grip`, from a press at `anchor`.
///
/// Nothing is held when this runs: the press that begins this drag has
/// already recorded any earlier one, through [`record_held`].
pub(crate) fn start(world: &mut World, group: Vec<Entity>, anchor: Vec2, grip: Grip) {
    world.resource_mut::<Stroke>().0 = Some(Held {
        group,
        grip,
        anchor,
        turned: None,
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

/// Move or turn the group with the pointer, and record it when the button
/// has been let go.
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
///
/// **It is not before transform propagation, and cannot be.** `bevy_ui_widgets`'
/// `MenuFocusSystem` runs after visibility propagation and before
/// `InputFocusSystems::FocusChangeEvents`, which puts the focus changes after
/// `TransformSystems::Propagate`, so being after the one and before the other
/// is a cycle the schedule refuses to build. The one kept is the one whose loss is worse:
/// read before the commit a press caused, [`Held::from`] would hold the number
/// from before it, and undoing the move would write it over the commit, which
/// saves. The one dropped costs a frame: the outline and the knob trail a
/// moving entity by one, and picking reads last frame's position.
///
/// **Anything that reads [`Stroke`] is ordered after this**, or it reads the
/// last frame's drag; `handles::place_knob` is the one there is.
///
/// **The delta is added to `translation` as it is**, a world delta written as
/// a local one, and the pivot of a turn is read from `translation` in the
/// same way. That holds while nothing in the editor has a parent, and a
/// hierarchy is what breaks it.
///
/// **A transform is only written when it changes**, so that holding the
/// button still does not mark every entity changed every frame.
///
/// Mutation: return before writing, and
/// `dragging_a_selected_entity_moves_it_by_what_the_pointer_moved` fails.
/// Mutation: fill [`Held::from`] in [`start`], which runs in `PreUpdate`,
/// rather than here, and
/// `a_number_the_press_commits_is_under_the_move_in_the_history` fails.
/// Mutation: record on every frame rather than when [`Held::ended`], and
/// `one_ctrl_z_after_a_drag_puts_every_entity_back_bit_for_bit` fails.
/// Mutation: drop `.before(take_back)` in [`MovePlugin`], and
/// `letting_go_and_ctrl_z_in_one_frame_take_back_the_move` fails. Mutation:
/// drop `.after(record_choice)`, and
/// `a_grab_and_a_release_in_one_frame_record_the_selection_below_the_move`
/// fails.
///
/// **One of its orderings no test holds**, measured by removing it with the
/// whole suite green. Without `.after(FocusChangeEvents)` the system is
/// unordered against the commit rather than before it, and the schedule
/// happens to put it after, so the test of that path passes on the order the
/// schedule picks; `docs/specs/ui.md` §10 accepts the same of
/// `record_choice`. The write only when a
/// transform changes is unguarded too: what it saves is the inspector
/// rebuilding on every frame the button is held still, which no test counts.
pub(crate) fn follow(
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
            .filter_map(|entity| Some((*entity, *transforms.get(*entity).ok()?)))
            .collect()
    });

    let now = viewport
        .single()
        .ok()
        .and_then(|location| pointer_world(location, &cameras));
    if let Some(now) = now {
        let place: Box<dyn Fn(&Transform) -> Transform> = match held.grip {
            Grip::Body => {
                let delta = (now - held.anchor).extend(0.0);
                Box::new(move |at| at.with_translation(at.translation + delta))
            }
            Grip::Knob { centre, .. } => {
                let pivot = pivot(from, centre);
                let angle = angle_between(held.anchor - pivot, now - pivot);
                held.turned = Some((pivot, angle));
                Box::new(turn(pivot, angle, from.len() > 1))
            }
        };
        for (entity, at) in from.iter() {
            let Ok(mut transform) = transforms.get_mut(*entity) else {
                continue;
            };
            let to = place(at);
            if *transform != to {
                *transform = to;
            }
        }
    }

    if held.ended
        && let Some(from) = stroke.0.take().and_then(|held| held.from)
    {
        commands.queue(move |world: &mut World| record(world, from));
    }
}

/// What a turn is about: the entity's own origin when it turns alone, and the
/// centre of the selection's frame when several turn together.
/// [`docs/specs/ui.md` §12](../../../docs/specs/ui.md).
///
/// Mutation: always the centre, and
/// `dragging_the_knob_turns_one_entity_about_its_origin` fails, because that
/// test's sprite is anchored away from its middle. Mutation: always the first
/// entity's origin, and
/// `turning_several_turns_them_about_the_centre_of_their_frame` fails.
fn pivot(from: &[(Entity, Transform)], centre: Vec2) -> Vec2 {
    match from {
        [(_, alone)] => alone.translation.truncate(),
        _ => centre,
    }
}

/// The signed angle from `from` to `to`, anticlockwise positive.
///
/// `atan2` of the cross and the dot rather than `Vec2::angle_to`, so that a
/// pointer exactly on the pivot, which is a zero vector, gives `0` rather
/// than anything a division by a zero length could give. A `NaN` written into
/// a rotation would be row 6: it saves, and nothing draws.
fn angle_between(from: Vec2, to: Vec2) -> f32 {
    ops::atan2(from.perp_dot(to), from.dot(to))
}

/// A turn by `angle` about `pivot`, as a function from where an entity was to
/// where it goes.
///
/// **The rotation is composed rather than replaced**, and normalized, which
/// [`docs/specs/ui.md` §12](../../../docs/specs/ui.md) decided: a tilt the
/// inspector gave in x or y is kept, and what is written is a unit rotation.
///
/// **Only a group's positions turn.** One entity turns about its own origin,
/// so turning its position would put it back where it was, with one
/// exception: `pivot + 0.0` is `+0.0` where the position said `-0.0`. That is
/// a write the user did not ask for, so its `translation` is not touched.
///
/// **A turn by nothing writes nothing.** A pointer let go exactly where it was
/// pressed gives an angle of exactly zero, and `pivot + (at - pivot)` does not
/// always come back to `at` in floats, nor does normalizing a rotation that
/// was not quite unit; either would be a change of a few bits the user cannot
/// see, recorded as an entry and saved.
///
/// **A rotation of all zeros turns as no rotation**, which is how the engine
/// draws it and what the inspector's raw numbers can leave behind. Normalizing
/// it divides by a zero length, and the `NaN` that gives would be row 6.
///
/// Mutation: `Quat::from_rotation_x` in place of `from_rotation_z`, and
/// `dragging_the_knob_turns_one_entity_about_its_origin` fails. Mutation: drop
/// `normalize`, and `a_turn_writes_a_unit_rotation` fails. Mutation: turn the
/// positions whatever `together` says, and
/// `dragging_the_knob_turns_one_entity_about_its_origin` fails on the sign of
/// a zero. Mutation: leave the positions alone whatever `together` says, and
/// `turning_several_turns_them_about_the_centre_of_their_frame` fails.
/// Mutation: drop the return for an angle of zero, and
/// `a_turn_by_nothing_changes_no_bit` fails. Mutation: write the normalized
/// product whatever it is, and `turning_a_rotation_of_zeros_writes_the_turn`
/// fails.
fn turn(pivot: Vec2, angle: f32, together: bool) -> impl Fn(&Transform) -> Transform {
    let about_z = Quat::from_rotation_z(angle);
    let round = Rot2::radians(angle);
    move |at| {
        let mut to = *at;
        if angle == 0.0 {
            return to;
        }
        let turned = (about_z * at.rotation).normalize();
        to.rotation = if turned.is_finite() { turned } else { about_z };
        if together {
            let xy = pivot + round * (at.translation.truncate() - pivot);
            to.translation = xy.extend(at.translation.z);
        }
        to
    }
}

/// Put a finished drag in the history, if it changed anything.
///
/// **Through [`History::record`], whose `execute` writes where the entities
/// already are**: the drag wrote them there frame by frame, so the first
/// `execute` changes nothing, and `History` keeps one way in.
/// [`docs/specs/ui.md` §11](../../../docs/specs/ui.md) has what was turned down
/// for it.
///
/// **A drag that leaves every entity's bits where they were is not an entry**,
/// for the reason §8 gives for a commit that changes nothing. An entity gone
/// since the drag began is left out.
///
/// Mutation: drop the bits comparison, and
/// `a_drag_that_comes_back_to_where_it_started_records_nothing` fails.
fn record(world: &mut World, from: Vec<(Entity, Transform)>) {
    let moved: Vec<Transformed> = from
        .into_iter()
        .filter_map(|(entity, from)| {
            let to = *world.get::<Transform>(entity)?;
            Some(Transformed { entity, from, to })
        })
        .collect();
    if moved
        .iter()
        .all(|moved| bits(&moved.from) == bits(&moved.to))
    {
        return;
    }
    History::record(world, TransformEntities(moved));
}

/// A transform's bits, so that two compare as the same numbers rather than
/// as floats that are merely equal.
fn bits(at: &Transform) -> Vec<u32> {
    at.translation
        .to_array()
        .into_iter()
        .chain(at.rotation.to_array())
        .chain(at.scale.to_array())
        .map(f32::to_bits)
        .collect()
}

/// One entity's part of a drag.
struct Transformed {
    entity: Entity,
    /// What undo writes back.
    from: Transform,
    /// What redo writes.
    to: Transform,
}

/// Everything that was dragged together, and where each one was before.
///
/// The command of [`docs/specs/data-model.md` §1](../../../docs/specs/data-model.md),
/// writing the component directly because there is no Editor Model yet.
///
/// **It writes the whole `Transform`**, so that a move and a turn are one
/// command, and a scale can be. That is safe because the history is one line:
/// anything that changed another field of the transform since this was
/// recorded is a later entry, and is taken back before this one is.
///
/// Mutation: write `from` in `execute`, and
/// `redo_after_undoing_a_drag_moves_them_again` fails. Mutation: write only
/// the rotation in `undo`, and
/// `one_ctrl_z_after_a_turn_puts_every_rotation_and_position_back_bit_for_bit`
/// fails.
struct TransformEntities(Vec<Transformed>);

impl EditorCommand for TransformEntities {
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

/// Write a transform, and nothing when the entity is gone, as `write_leaf`
/// in `inspector.rs` does and for the reason it gives.
fn put(world: &mut World, entity: Entity, at: Transform) {
    if let Some(mut transform) = world.get_mut::<Transform>(entity) {
        *transform = at;
    }
}

#[cfg(test)]
mod tests {
    use crate::pointer::{
        click_at, drag_through, hold_key, hold_letter, in_window, press_and_hold,
        press_then_release, release_key, release_letter, round, write_input,
    };
    use crate::selection::{Selectable, Selection};
    use crate::viewport::ViewportCamera;
    use crate::{editor, headless};
    use bevy::picking::pointer::{PointerAction, PointerButton};
    use bevy::prelude::*;
    use bevy::sprite::Anchor;
    use core::f32::consts::{FRAC_PI_2, FRAC_PI_4};

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
    /// Mutation: divide the delta in `follow` by the zoom, which is what a
    /// delta in pixels is, and this fails.
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
    /// Every placeholder is unturned and unscaled, which leaves transform
    /// maths unguarded, so this spawns the awkward one.
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
    /// Two entities, because one guards no plural, and a release that ends
    /// over the one pressed, where the click that accompanies the drag end
    /// would act.
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
    /// Mutation: write `from` in `TransformEntities::execute`, and this fails.
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

    /// Redo during a drag does nothing.
    ///
    /// The same shape as the undo: a redo in the middle of a drag writes a
    /// position the next frame writes over, and the entry it put back sits
    /// between the drag and what came before it, where the second Ctrl+Z
    /// spends itself on it rather than reaching the click.
    ///
    /// Mutation: drop the `moving::in_progress` check in `history::put_back`,
    /// and this fails.
    #[test]
    fn redo_during_a_drag_does_nothing() {
        let mut app = moving_editor();
        let middle = entity_at(&mut app, MIDDLE);
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);
        press_then_release(&mut app, in_window(MIDDLE), in_window(Vec2::new(30.0, 0.0)));
        undo(&mut app);
        assert_eq!(translation(&app, middle), Vec3::ZERO);

        let to = in_window(Vec2::new(60.0, 0.0));
        press_and_hold(&mut app, in_window(MIDDLE), to);
        redo(&mut app);
        write_input(&mut app, to, PointerAction::Release(PointerButton::Primary));
        app.update();
        app.update();
        assert_near(translation(&app, middle), Vec3::new(60.0, 0.0, 0.0));

        undo(&mut app);
        undo(&mut app);

        assert!(
            selected(&app).is_empty(),
            "the second Ctrl+Z did not reach the click"
        );
    }

    /// A drag of many small moves follows the pointer, and undo puts it back
    /// where it began.
    ///
    /// A real drag is dozens of moves, and every other test here makes one.
    /// With one, a grab that begins the move again on each move cannot be
    /// told apart from one that begins it once.
    ///
    /// Mutation: drop the `pressed.moving` return in `selection::grab`, and
    /// this fails, the entity running ahead of the pointer and the undo
    /// landing short of where it began.
    #[test]
    fn a_drag_of_many_moves_follows_the_pointer_and_undoes_to_where_it_began() {
        let mut app = moving_editor();
        let middle = entity_at(&mut app, MIDDLE);
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);

        write_input(
            &mut app,
            in_window(MIDDLE),
            PointerAction::Move { delta: Vec2::ONE },
        );
        app.update();
        write_input(
            &mut app,
            in_window(MIDDLE),
            PointerAction::Press(PointerButton::Primary),
        );
        app.update();
        for x in [10.0, 20.0, 30.0] {
            write_input(
                &mut app,
                in_window(Vec2::new(x, 0.0)),
                PointerAction::Move { delta: Vec2::ONE },
            );
            app.update();
            app.update();
        }
        write_input(
            &mut app,
            in_window(Vec2::new(30.0, 0.0)),
            PointerAction::Release(PointerButton::Primary),
        );
        app.update();
        app.update();
        assert_near(translation(&app, middle), Vec3::new(30.0, 0.0, 0.0));

        undo(&mut app);

        assert_eq!(translation(&app, middle), Vec3::ZERO);
    }

    /// Letting go and Ctrl+Z in one frame take back the move.
    ///
    /// The move is recorded when the button is let go, and the key is read
    /// later in the same frame, which is the order the user did them in.
    ///
    /// Mutation: drop `.before(take_back)` in `MovePlugin`, and this fails,
    /// the undo taking back the click and the move landing after it.
    #[test]
    fn letting_go_and_ctrl_z_in_one_frame_take_back_the_move() {
        let mut app = moving_editor();
        let middle = entity_at(&mut app, MIDDLE);
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);
        let to = in_window(Vec2::new(30.0, 0.0));
        press_and_hold(&mut app, in_window(MIDDLE), to);

        write_input(&mut app, to, PointerAction::Release(PointerButton::Primary));
        hold_key(&mut app, KeyCode::ControlLeft);
        hold_letter(&mut app, KeyCode::KeyZ, "z");
        app.update();
        release_letter(&mut app, KeyCode::KeyZ, "z");
        release_key(&mut app, KeyCode::ControlLeft);
        app.update();
        app.update();

        assert_eq!(translation(&app, middle), Vec3::ZERO);
        assert_eq!(selected(&app), [middle]);
    }

    /// A grab and a release in one frame record the selection below the
    /// move, so undo takes back the move first.
    ///
    /// The press is a frame earlier, so the grab and the release are the
    /// only two things in the batch: pressed in the same batch, the press is
    /// read where the move ended and the move is nothing
    /// (`docs/specs/ui.md` §11, "Accepted risk").
    ///
    /// Mutation: drop `.after(record_choice)` in `MovePlugin`, and this fails.
    #[test]
    fn a_grab_and_a_release_in_one_frame_record_the_selection_below_the_move() {
        let mut app = moving_editor();
        let middle = entity_at(&mut app, MIDDLE);
        let left = entity_at(&mut app, LEFT);
        click_at(&mut app, in_window(LEFT), PointerButton::Primary);

        write_input(
            &mut app,
            in_window(MIDDLE),
            PointerAction::Move { delta: Vec2::ONE },
        );
        app.update();
        write_input(
            &mut app,
            in_window(MIDDLE),
            PointerAction::Press(PointerButton::Primary),
        );
        app.update();
        let to = in_window(Vec2::new(20.0, 0.0));
        write_input(&mut app, to, PointerAction::Move { delta: Vec2::ONE });
        write_input(&mut app, to, PointerAction::Release(PointerButton::Primary));
        app.update();
        app.update();
        assert_eq!(selected(&app), [middle]);
        assert_near(translation(&app, middle), Vec3::new(20.0, 0.0, 0.0));

        undo(&mut app);

        assert_eq!(translation(&app, middle), Vec3::ZERO);
        assert_eq!(
            selected(&app),
            [middle],
            "the first Ctrl+Z took back the selection"
        );
        undo(&mut app);
        assert_eq!(selected(&app), [left]);
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

    /// A rotation's bits, so that "bit for bit" is what is compared.
    fn rotation_bits(rotation: Quat) -> [u32; 4] {
        rotation.to_array().map(f32::to_bits)
    }

    /// An entity's rotation.
    fn rotation(app: &App, entity: Entity) -> Quat {
        app.world()
            .get::<Transform>(entity)
            .expect("it has a transform")
            .rotation
    }

    /// Where the knob is drawn now, in world units.
    fn knob(app: &App) -> Vec2 {
        crate::handles::drawn_knob(app)
            .expect("a knob is drawn")
            .center()
    }

    /// Whether a rotation is the one wanted, to within what the pointer's
    /// rounding leaves in an angle.
    fn assert_turned(rotation: Quat, wanted: Quat) {
        assert!(
            rotation.abs_diff_eq(wanted, 1e-4),
            "{rotation} is not {wanted}"
        );
    }

    /// Dragging the knob round turns a lone entity about its own origin: the
    /// rotation follows the pointer's angle, is about Z, and the position is
    /// left exactly as it was.
    ///
    /// The sprite is anchored at its bottom-left corner, so its origin is not
    /// the middle of its frame and a pivot at the frame's centre gives a
    /// different angle. Its `x` is `-0.0`, which turning the position about
    /// the origin would make `+0.0`. The drag is several moves, because one
    /// move cannot tell a drag that begins once from one that begins on every
    /// move.
    ///
    /// Mutation: `Quat::from_rotation_x` in `moving::turn`, and this fails.
    /// Mutation: always the frame's centre in `moving::pivot`, and this fails.
    /// Mutation: turn the positions whatever `together` says in
    /// `moving::turn`, and this fails. Mutation: start a move rather than a
    /// turn from the knob in `selection::grab`, and this fails.
    #[test]
    fn dragging_the_knob_turns_one_entity_about_its_origin() {
        let mut app = moving_editor();
        let at = Vec3::new(-0.0, -150.0, 0.0);
        let anchored = app
            .world_mut()
            .spawn((
                Sprite::from_color(Color::WHITE, Vec2::splat(64.0)),
                Anchor::BOTTOM_LEFT,
                Transform::from_translation(at),
                Selectable,
            ))
            .id();
        app.update();
        click_at(
            &mut app,
            in_window(Vec2::new(32.0, -118.0)),
            PointerButton::Primary,
        );
        assert_eq!(selected(&app), [anchored]);

        let path = round(at.truncate(), knob(&app), 45.0, 4);
        drag_through(&mut app, &path);

        let turned = rotation(&app, anchored);
        assert_turned(turned, Quat::from_rotation_z(FRAC_PI_4));
        assert_eq!((turned.x, turned.y), (0.0, 0.0), "not a turn about Z");
        assert_eq!(bits(translation(&app, anchored)), bits(at));
    }

    /// What a turn writes is a unit rotation, even from one that was not
    /// quite unit before.
    ///
    /// The starting rotation is a turn about Z whose length is 1.01, which
    /// the inspector's four raw numbers can leave behind.
    ///
    /// Mutation: drop `normalize` in `moving::turn`, and this fails.
    #[test]
    fn a_turn_writes_a_unit_rotation() {
        let mut app = moving_editor();
        let at = Vec3::new(0.0, -150.0, 0.0);
        let entity = sprite(
            &mut app,
            Transform::from_translation(at).with_rotation(Quat::from_xyzw(0.0, 0.0, 0.0, 1.01)),
        );
        click_at(&mut app, in_window(at.truncate()), PointerButton::Primary);
        assert_eq!(selected(&app), [entity]);

        let path = round(at.truncate(), knob(&app), 30.0, 3);
        drag_through(&mut app, &path);

        let turned = rotation(&app, entity);
        assert_ne!(turned.w, 1.01, "the drag turned nothing");
        assert!(turned.is_normalized(), "{turned} is not a unit rotation");
    }

    /// Several selected turn as one body about the centre of their frame:
    /// each one's rotation, and its position about that centre.
    ///
    /// The middle and left placeholders span `-232` to `32` across, so the
    /// frame's centre is at `-100`; a quarter turn carries the middle one a
    /// hundred above it and the left one a hundred below. Two entities, and a
    /// drag of several moves, because one of either guards no plural.
    ///
    /// Mutation: always the first entity's origin in `moving::pivot`, and this
    /// fails. Mutation: leave the positions alone whatever `together` says in
    /// `moving::turn`, and this fails.
    #[test]
    fn turning_several_turns_them_about_the_centre_of_their_frame() {
        let mut app = moving_editor();
        let middle = entity_at(&mut app, MIDDLE);
        let left = entity_at(&mut app, LEFT);
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);
        modifier_click_at(&mut app, in_window(LEFT));
        let centre = Vec2::new(-100.0, 0.0);

        let path = round(centre, knob(&app), 90.0, 6);
        drag_through(&mut app, &path);

        assert_near(translation(&app, middle), Vec3::new(-100.0, 100.0, 0.0));
        assert_near(translation(&app, left), Vec3::new(-100.0, -100.0, 0.0));
        assert_turned(rotation(&app, middle), Quat::from_rotation_z(FRAC_PI_2));
        assert_turned(rotation(&app, left), Quat::from_rotation_z(FRAC_PI_2));
    }

    /// One Ctrl+Z after a turn puts every entity's rotation and position back,
    /// bit for bit, and the next one reaches what came before the turn.
    ///
    /// Two entities, already turned and at positions that are not round, so a
    /// restore that went through a rounding, or that put back only the
    /// rotations of a group whose positions also turned, would show.
    ///
    /// Mutation: record on every frame in `moving::follow`, and this fails.
    /// Mutation: write only the rotation in `TransformEntities::undo`, and
    /// this fails.
    #[test]
    fn one_ctrl_z_after_a_turn_puts_every_rotation_and_position_back_bit_for_bit() {
        let mut app = moving_editor();
        let a_at = Transform::from_xyz(0.1, -150.3, 0.7).with_rotation(Quat::from_rotation_z(0.3));
        let b_at =
            Transform::from_xyz(120.9, -149.7, 0.3).with_rotation(Quat::from_rotation_z(-0.2));
        let a = sprite(&mut app, a_at);
        let b = sprite(&mut app, b_at);
        click_at(
            &mut app,
            in_window(a_at.translation.truncate()),
            PointerButton::Primary,
        );
        modifier_click_at(&mut app, in_window(b_at.translation.truncate()));
        assert_eq!(selected(&app), [a, b]);
        let from = knob(&app);

        drag_through(&mut app, &round(from - Vec2::new(0.0, 80.0), from, 60.0, 5));
        assert_ne!(
            rotation_bits(rotation(&app, a)),
            rotation_bits(a_at.rotation),
            "the drag turned nothing"
        );

        undo(&mut app);
        for (entity, at) in [(a, a_at), (b, b_at)] {
            assert_eq!(bits(translation(&app, entity)), bits(at.translation));
            assert_eq!(
                rotation_bits(rotation(&app, entity)),
                rotation_bits(at.rotation)
            );
        }

        undo(&mut app);
        assert_eq!(
            selected(&app),
            [a],
            "the second Ctrl+Z did not reach the modifier click"
        );
    }

    /// Turning several keeps each one's depth and composes the turn onto the
    /// rotation each already had, a tilt included.
    ///
    /// Every placeholder sits at `z == 0` unturned, which leaves this path
    /// unexercised, so this spawns two
    /// at different depths, already turned, one of them tilted about X, which
    /// the inspector's raw numbers can leave behind. The angle turned is read
    /// back from the one with no tilt, since the pointer's path is not round
    /// the real pivot; what is held is the composition, not the angle.
    ///
    /// Mutation: `xy.extend(0.0)` in `moving::turn`, and this fails, which is
    /// row 6: a group turned is quietly put in a different draw order.
    /// Mutation: `about_z` in place of `about_z * at.rotation`, and this
    /// fails. Mutation: `at.rotation * about_z`, and this fails on the tilt.
    #[test]
    fn turning_several_keeps_each_depth_and_composes_onto_each_rotation() {
        let mut app = moving_editor();
        let tilted = Quat::from_rotation_z(0.3) * Quat::from_rotation_x(0.2);
        let a_at = Transform::from_xyz(0.0, -150.0, 0.7).with_rotation(tilted);
        let b_at =
            Transform::from_xyz(120.0, -150.0, 0.3).with_rotation(Quat::from_rotation_z(-0.2));
        let a = sprite(&mut app, a_at);
        let b = sprite(&mut app, b_at);
        click_at(
            &mut app,
            in_window(a_at.translation.truncate()),
            PointerButton::Primary,
        );
        modifier_click_at(&mut app, in_window(b_at.translation.truncate()));
        assert_eq!(selected(&app), [a, b]);
        let from = knob(&app);

        drag_through(&mut app, &round(from - Vec2::new(0.0, 80.0), from, 40.0, 4));

        let turned = rotation(&app, b).to_euler(EulerRot::ZYX).0 - (-0.2);
        assert!(turned.abs() > 0.1, "the drag turned nothing");
        assert_turned(rotation(&app, a), Quat::from_rotation_z(turned) * tilted);
        for (entity, at) in [(a, a_at), (b, b_at)] {
            assert_eq!(
                translation(&app, entity).z.to_bits(),
                at.translation.z.to_bits(),
                "a turn changed a depth"
            );
        }
    }

    /// One Ctrl+Z after turning one entity puts its rotation back, bit for bit.
    ///
    /// A lone entity's position does not change when it turns, so this is the
    /// case where only the rotation says the drag did anything.
    ///
    /// Mutation: compare only the translation in `moving::bits`, and this
    /// fails: the turn is not recorded, and the Ctrl+Z takes back the click
    /// instead.
    #[test]
    fn one_ctrl_z_after_turning_one_entity_puts_its_rotation_back_bit_for_bit() {
        let mut app = moving_editor();
        let at = Transform::from_xyz(0.1, -150.3, 0.7).with_rotation(Quat::from_rotation_z(0.3));
        let entity = sprite(&mut app, at);
        click_at(
            &mut app,
            in_window(at.translation.truncate()),
            PointerButton::Primary,
        );
        let path = round(at.translation.truncate(), knob(&app), 50.0, 4);

        drag_through(&mut app, &path);
        assert_ne!(
            rotation_bits(rotation(&app, entity)),
            rotation_bits(at.rotation),
            "the drag turned nothing"
        );

        undo(&mut app);
        assert_eq!(
            rotation_bits(rotation(&app, entity)),
            rotation_bits(at.rotation)
        );
        assert_eq!(selected(&app), [entity], "the Ctrl+Z took back the click");
    }

    /// Letting go with the pointer exactly on the pivot leaves a rotation, not
    /// a `NaN`.
    ///
    /// The middle placeholder turns about its origin, the world's origin, and
    /// the viewport's middle is where the pointer reads as exactly that.
    ///
    /// Mutation: `from.angle_to(to)` in `moving::angle_between`, and this
    /// fails: the angle of a zero vector is `0 / 0`.
    #[test]
    fn letting_go_on_the_pivot_leaves_a_rotation() {
        let mut app = moving_editor();
        let middle = entity_at(&mut app, MIDDLE);
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);
        let mut path = round(MIDDLE, knob(&app), 45.0, 3);
        path.push(in_window(MIDDLE));

        drag_through(&mut app, &path);

        let turned = rotation(&app, middle);
        assert!(turned.is_finite(), "{turned} is not a rotation");
    }

    /// Turning an entity whose rotation is all zeros writes a rotation, the
    /// turn itself, rather than a `NaN`.
    ///
    /// The inspector's four raw numbers can be typed to `0, 0, 0, 0`, and the
    /// engine draws that as no rotation at all, so the turn starts from what
    /// was on the screen.
    ///
    /// Mutation: write the normalized product whatever it is in
    /// `moving::turn`, and this fails: normalizing a zero length is `0 / 0`.
    #[test]
    fn turning_a_rotation_of_zeros_writes_the_turn() {
        let mut app = moving_editor();
        let at = Transform::from_xyz(0.0, -150.0, 0.0)
            .with_rotation(Quat::from_xyzw(0.0, 0.0, 0.0, 0.0));
        let entity = sprite(&mut app, at);
        click_at(
            &mut app,
            in_window(at.translation.truncate()),
            PointerButton::Primary,
        );
        let path = round(at.translation.truncate(), knob(&app), 45.0, 3);

        drag_through(&mut app, &path);

        assert_turned(rotation(&app, entity), Quat::from_rotation_z(FRAC_PI_4));
    }

    /// A turn by nothing writes nothing: a pointer let go exactly where it was
    /// pressed leaves every bit where it was, a group's positions and a
    /// rotation that was not quite unit included.
    ///
    /// Read off `turn` itself rather than through a drag, because whether
    /// a group's `pivot + (at - pivot)` comes back to `at` depends on the
    /// pivot, and a drag cannot choose one that is known not to: measured, a
    /// drag round the frame of two entities and back came back exact, while
    /// these three pivots do not. `record` comparing bits is what keeps such a
    /// drag out of the history, and it needs this to have written nothing.
    ///
    /// Mutation: drop the early return for an angle of zero in
    /// `moving::turn`, and this fails.
    #[test]
    fn a_turn_by_nothing_changes_no_bit() {
        let at = Transform::from_xyz(0.1, -150.3, 0.7)
            .with_rotation(Quat::from_xyzw(0.0, 0.0, 0.151, 0.999));
        for pivot in [
            Vec2::new(60.5, -150.0),
            Vec2::new(60.512_344, -150.01),
            Vec2::new(33.3, 7.7),
        ] {
            for together in [true, false] {
                let to = super::turn(pivot, 0.0, together)(&at);
                assert_eq!(
                    super::bits(&to),
                    super::bits(&at),
                    "about {pivot}, together: {together}"
                );
            }
        }
    }
}
