//! The handles drawn on what is selected: today, the rotate knob.
//!
//! What the knob is, where it sits, what a press on it does and what was
//! turned down for it are settled in
//! [`docs/specs/ui.md` §12](../../../docs/specs/ui.md). The press is taken in
//! `selection.rs`, because §4 is one gesture table, and the turn is done in
//! `moving.rs`, because it is a drag like a move.

use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::{RenderLayers, VisibilitySystems};
use bevy::gizmos::AppGizmoBuilder;
use bevy::gizmos::config::{GizmoConfig, GizmoConfigGroup};
use bevy::picking::pointer::PointerLocation;
use bevy::prelude::*;

use crate::Selection;
use crate::moving::{Grip, Stroke};
use crate::outline::{OUTLINE, SELECTION_LAYER};
use crate::selection::world_bounds;
use crate::viewport::ViewportCamera;

/// How far the knob stands above the frame, in logical pixels.
///
/// Chosen rather than measured, as are the two below.
const STALK: f32 = 24.0;

/// The knob's radius as drawn, in logical pixels.
const KNOB_RADIUS: f32 = 6.0;

/// How close a press has to be to the knob's centre to take it, in logical
/// pixels. Larger than the knob, so that it can be hit without aiming.
const KNOB_REACH: f32 = 9.0;

/// Where the rotate knob was drawn last, so that a press is tested against
/// what the user saw.
///
/// Written in `PostUpdate` and read by the next frame's press in `PreUpdate`,
/// so a press is tested against **the knob drawn in the frame before it**,
/// which is the one on the screen when the button went down.
#[derive(Resource, Default)]
pub(crate) struct Knob(Option<Placed>);

/// Where the knob is, in the two sets of units it is read in.
struct Placed {
    /// The knob's centre, in the coordinates the viewport's pointer is read in.
    at: Vec2,
    /// The centre of the selection's frame, in world units.
    centre: Vec2,
    /// Where the stalk leaves the frame, in world units.
    base: Vec2,
    /// The knob's centre, in world units.
    tip: Vec2,
}

impl Knob {
    /// What a press at `location` takes hold of, when it is on the knob.
    ///
    /// Mutation: compare against ten times `KNOB_REACH`, and
    /// `a_press_beside_the_knob_is_not_on_it` fails. Mutation: a `KNOB_REACH`
    /// of one, and `a_press_off_the_knobs_centre_within_its_reach_takes_it`
    /// fails.
    pub(crate) fn under(&self, location: &PointerLocation) -> Option<Grip> {
        let (Some(placed), Some(location)) = (&self.0, &location.location) else {
            return None;
        };
        (placed.at.distance(location.position) <= KNOB_REACH).then_some(Grip::Knob {
            centre: placed.centre,
            base: placed.base,
            tip: placed.tip,
        })
    }
}

/// The gizmo group the handles are drawn in.
///
/// Its own group rather than `outline::SelectionGizmos`, on the same layer,
/// for the reason `selection::BandGizmos` gives: the outline's tests read
/// whether that group has a handle at all to mean "nothing is drawn".
#[derive(Default, Reflect, GizmoConfigGroup)]
#[reflect(Default)]
pub(crate) struct HandleGizmos;

/// The handles on what is selected.
///
/// Mutation: leave its row out of the editor's member table, and
/// `the_group_carries_the_members_the_table_names` fails.
pub struct HandlesPlugin;

impl Plugin for HandlesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Knob>()
            .insert_gizmo_config(
                HandleGizmos,
                GizmoConfig {
                    render_layers: RenderLayers::layer(SELECTION_LAYER),
                    ..default()
                },
            )
            .add_systems(
                PostUpdate,
                place_knob
                    .after(VisibilitySystems::CalculateBounds)
                    .after(crate::moving::follow),
            );
    }
}

/// Place the knob above what is selected, draw it, and remember where.
///
/// After [`VisibilitySystems::CalculateBounds`], for the reason
/// `outline::outline` gives: the bounds and the `GlobalTransform` the frame is
/// built from are both written in `PostUpdate`. And after `moving::follow`,
/// which writes the angle a turn in progress has reached; unordered, the
/// knob could be drawn a frame behind the pointer depending on which ran
/// first. **No test holds this order**:
/// removing it leaves every handle test green, measured, because each reads
/// the knob after a frame in which nothing moves.
///
/// **The stalk is measured on the viewport, not in the world.** The top of the
/// frame goes to the viewport through the camera, the stalk is added there,
/// and the knob comes back into the world the same way, so it is the same
/// size on screen at every zoom. Both directions are the camera's own pair,
/// the one `selection::pointer_world` reads a press through, so no scale
/// factor appears here and RK-005 has nothing to bite.
///
/// **While a turn is held**, the knob is drawn where the turn has carried it,
/// about the pivot, rather than above the frame, whose top moves as the
/// corners turn; and [`Knob`] is left as it was, since nothing reads it until
/// the next press.
///
/// Mutation: return before drawing, and `selecting_an_entity_draws_a_knob_above_it`
/// fails. Mutation: build the frame from every entity with bounds rather than
/// from the selection, and `nothing_selected_draws_no_knob` fails. Mutation: add `STALK` to the
/// frame's top in world units, and
/// `the_knob_is_the_same_distance_on_screen_at_every_zoom` fails. Mutation:
/// build the frame from the `Aabb` untransformed, and
/// `the_knob_is_above_a_turned_and_scaled_entitys_frame` fails.
fn place_knob(
    selection: Res<Selection>,
    bounds: Query<(&GlobalTransform, &Aabb)>,
    cameras: Query<(&Camera, &GlobalTransform), With<ViewportCamera>>,
    stroke: Res<Stroke>,
    mut knob: ResMut<Knob>,
    mut gizmos: Gizmos<HandleGizmos>,
) {
    let Ok((camera, eye)) = cameras.single() else {
        knob.0 = None;
        return;
    };
    if let Some((Grip::Knob { base, tip, .. }, pivot, angle)) = stroke.turned() {
        let round = Rot2::radians(angle);
        draw(
            &mut gizmos,
            camera,
            eye,
            pivot + round * (base - pivot),
            pivot + round * (tip - pivot),
        );
        return;
    }

    knob.0 = frame(&selection, &bounds).and_then(|frame| place(camera, eye, frame));
    if let Some(placed) = &knob.0 {
        draw(&mut gizmos, camera, eye, placed.base, placed.tip);
    }
}

/// The axis-aligned rectangle covering everything selected, or `None` when
/// nothing selected has bounds.
///
/// The bounds are `selection::world_bounds`, which the box drag tests against,
/// so the frame a knob sits on is the frame a box would find.
fn frame(selection: &Selection, bounds: &Query<(&GlobalTransform, &Aabb)>) -> Option<Rect> {
    selection
        .entities()
        .iter()
        .filter_map(|entity| bounds.get(*entity).ok())
        .map(|(at, aabb)| world_bounds(at, aabb))
        .reduce(|frame, bounds| frame.union(bounds))
}

/// Where the knob goes above `frame`.
fn place(camera: &Camera, eye: &GlobalTransform, frame: Rect) -> Option<Placed> {
    let base = Vec2::new(frame.center().x, frame.max.y);
    // The viewport's `y` grows downwards, so up the screen is minus.
    let at = camera.world_to_viewport(eye, base.extend(0.0)).ok()? - Vec2::new(0.0, STALK);
    let tip = camera.viewport_to_world_2d(eye, at).ok()?;
    Some(Placed {
        at,
        centre: frame.center(),
        base,
        tip,
    })
}

/// Draw the stalk from `base` and the knob at `tip`.
///
/// The radius is `KNOB_RADIUS` on the viewport, taken back into the world
/// beside the knob, for the reason [`place_knob`] measures the stalk there.
fn draw(
    gizmos: &mut Gizmos<HandleGizmos>,
    camera: &Camera,
    eye: &GlobalTransform,
    base: Vec2,
    tip: Vec2,
) {
    gizmos.line_2d(base, tip, OUTLINE);
    let Ok(at) = camera.world_to_viewport(eye, tip.extend(0.0)) else {
        return;
    };
    let Ok(edge) = camera.viewport_to_world_2d(eye, at + Vec2::new(KNOB_RADIUS, 0.0)) else {
        return;
    };
    gizmos.circle_2d(tip, tip.distance(edge), OUTLINE);
}

/// Where the knob was drawn, in world units, read back from the drawing.
///
/// The knob is the only strip the group draws, since the stalk is a line, so
/// the one rectangle `drawn::rectangles_of` finds is the extent of the
/// circle's strip. That is the circle's square, centred on the knob and a
/// diameter across, because `circle_2d` in `bevy_gizmos` 0.20.1 places a
/// vertex at every multiple of a full turn over its 32 segments, which puts
/// four of them on the axes. Read from what was drawn rather than worked out
/// the way [`place`] works it out, so that a test agrees with the screen and
/// not with the code.
#[cfg(test)]
pub(crate) fn drawn_knob(app: &App) -> Option<Rect> {
    let drawn = crate::drawn::rectangles_of::<HandleGizmos>(app);
    assert!(drawn.len() <= 1, "more than one knob drawn: {drawn:?}");
    drawn.first().copied()
}

/// Where the stalk was drawn, from the frame to the knob, in world units.
#[cfg(test)]
fn drawn_stalk(app: &App) -> Option<(Vec2, Vec2)> {
    let drawn = crate::drawn::lines_of::<HandleGizmos>(app);
    assert!(drawn.len() <= 1, "more than one stalk drawn: {drawn:?}");
    drawn.first().copied()
}

#[cfg(test)]
mod tests {
    use super::{drawn_knob, drawn_stalk};
    use crate::pointer::{
        click_at, drag_through, in_window, press_and_hold, press_then_release, round,
    };
    use crate::selection::{Selectable, Selection};
    use crate::viewport::ViewportCamera;
    use crate::{editor, headless};
    use bevy::picking::pointer::PointerButton;
    use bevy::prelude::*;
    use core::f32::consts::{FRAC_PI_4, SQRT_2};

    /// Where the left placeholder is, in world units. Repeated rather than
    /// read from `PLACEHOLDERS`, for the reason `in_window` gives.
    const LEFT: Vec2 = Vec2::new(-200.0, 0.0);
    /// Where the middle placeholder is, in world units.
    const MIDDLE: Vec2 = Vec2::ZERO;
    /// Where the knob stands above the middle placeholder at the default
    /// zoom: its top edge is 32 up, and the stalk is 24 pixels, which is 24
    /// world units at one unit a pixel.
    const MIDDLE_KNOB: Vec2 = Vec2::new(0.0, 56.0);

    /// The editor, one frame in, which is when the placeholders exist.
    fn knob_editor() -> App {
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

    /// What is selected.
    fn selected(app: &App) -> Vec<Entity> {
        app.world().resource::<Selection>().entities().to_vec()
    }

    /// An entity's transform.
    fn transform(app: &App, entity: Entity) -> Transform {
        *app.world()
            .get::<Transform>(entity)
            .expect("it has a transform")
    }

    /// Whether two points are the same to within the pointer's rounding,
    /// which `moving::tests::assert_near` measures.
    fn assert_near(at: Vec2, wanted: Vec2) {
        assert!(at.abs_diff_eq(wanted, 1e-3), "{at} is not {wanted}");
    }

    /// Set the viewport camera's zoom directly, since the zoom is not what is
    /// under test.
    fn zoom_to(app: &mut App, scale: f32) {
        let world = app.world_mut();
        let mut projection = world
            .query_filtered::<&mut Projection, With<ViewportCamera>>()
            .single_mut(world)
            .expect("one viewport camera");
        let Projection::Orthographic(orthographic) = &mut *projection else {
            panic!("the viewport camera is orthographic");
        };
        orthographic.scale = scale;
        app.update();
    }

    /// Selecting an entity draws a knob above the middle of its top edge.
    ///
    /// Mutation: return in `place_knob` before drawing, and this fails.
    #[test]
    fn selecting_an_entity_draws_a_knob_above_it() {
        let mut app = knob_editor();
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);

        let knob = drawn_knob(&app).expect("a knob is drawn");

        assert_near(knob.center(), MIDDLE_KNOB);
        assert_near(knob.size(), Vec2::splat(12.0));
    }

    /// With nothing selected there is no knob.
    ///
    /// Mutation: build the frame in `frame` from every entity with bounds
    /// rather than from the selection, and this fails.
    #[test]
    fn nothing_selected_draws_no_knob() {
        let app = knob_editor();

        assert_eq!(drawn_knob(&app), None);
    }

    /// Zoomed out, the knob stands the same number of pixels above the frame
    /// and is the same size on screen, which is more of the world.
    ///
    /// At a scale of two, the middle placeholder's top edge is 16 pixels above
    /// the viewport's middle, and 24 pixels more is 80 world units.
    ///
    /// Mutation: add `STALK` to the frame's top in world units in `place`, and
    /// this fails.
    #[test]
    fn the_knob_is_the_same_distance_on_screen_at_every_zoom() {
        let mut app = knob_editor();
        zoom_to(&mut app, 2.0);
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);

        let knob = drawn_knob(&app).expect("a knob is drawn");

        assert_near(knob.center(), Vec2::new(0.0, 80.0));
        assert_near(knob.size(), Vec2::splat(24.0));
    }

    /// The knob stands above the frame a turned and scaled entity actually
    /// covers.
    ///
    /// RK-006: every placeholder is unturned and unscaled, so this spawns the
    /// awkward one. A 64 pixel square scaled by two and turned an eighth of a
    /// turn reaches `64 * sqrt(2)` above its origin.
    ///
    /// Mutation: build the frame from the `Aabb` untransformed, and this
    /// fails.
    #[test]
    fn the_knob_is_above_a_turned_and_scaled_entitys_frame() {
        let mut app = knob_editor();
        let at = Vec2::new(0.0, -150.0);
        app.world_mut().spawn((
            Sprite::from_color(Color::WHITE, Vec2::splat(64.0)),
            Transform::from_translation(at.extend(0.0))
                .with_rotation(Quat::from_rotation_z(FRAC_PI_4))
                .with_scale(Vec3::splat(2.0)),
            Selectable,
        ));
        app.update();
        click_at(&mut app, in_window(at), PointerButton::Primary);

        let knob = drawn_knob(&app).expect("a knob is drawn");

        assert_near(knob.center(), Vec2::new(0.0, at.y + 64.0 * SQRT_2 + 24.0));
    }

    /// While a turn is held, the knob is drawn under the pointer rather than
    /// above the frame, and its stalk turns with it, from where it left the
    /// frame turned by the same angle.
    ///
    /// Mutation: drop the branch in `place_knob` that draws a turn in
    /// progress, and this fails. Mutation: draw the stalk from where it left
    /// the frame unturned, and this fails on the stalk.
    #[test]
    fn the_knob_and_its_stalk_turn_under_the_pointer() {
        let mut app = knob_editor();
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);
        let round = Rot2::degrees(60.0);
        let to = MIDDLE + round * MIDDLE_KNOB;

        press_and_hold(&mut app, in_window(MIDDLE_KNOB), in_window(to));

        assert_near(drawn_knob(&app).expect("a knob is drawn").center(), to);
        let (base, tip) = drawn_stalk(&app).expect("a stalk is drawn");
        assert_near(base, MIDDLE + round * Vec2::new(0.0, 32.0));
        assert_near(tip, to);
    }

    /// Clicking the knob leaves the selection as it was and turns nothing.
    ///
    /// RK-008: the release lands on empty space, where the click that
    /// accompanies it would clear the selection.
    ///
    /// Mutation: drop the `pressed.handle` check in `selection::select`, and
    /// this fails.
    #[test]
    fn clicking_the_knob_keeps_the_selection() {
        let mut app = knob_editor();
        let middle = entity_at(&mut app, MIDDLE);
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);

        click_at(&mut app, in_window(MIDDLE_KNOB), PointerButton::Primary);

        assert_eq!(selected(&app), [middle]);
        assert_eq!(transform(&app, middle).rotation, Quat::IDENTITY);
    }

    /// A drag from the knob is a turn and not a box, though it began where
    /// nothing is.
    ///
    /// RK-008: it ends over the left placeholder, with a box that would have
    /// covered both, so the box's own write is what would change the
    /// selection; the click that comes with the release is held back by the
    /// turn either way.
    ///
    /// Mutation: drop the `pressed.handle` check in `selection::begin`, and this
    /// fails.
    #[test]
    fn a_drag_from_the_knob_is_not_a_box() {
        let mut app = knob_editor();
        let middle = entity_at(&mut app, MIDDLE);
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);

        press_then_release(&mut app, in_window(MIDDLE_KNOB), in_window(LEFT));

        assert_eq!(selected(&app), [middle]);
    }

    /// The knob takes a press over an entity standing under it: what is
    /// selected turns, and the entity under the knob neither moves nor is
    /// selected.
    ///
    /// The entity above spans 38 to 102 up, so the knob at 56 is inside it and
    /// the hover map names it on the press. At 90, where this first put it,
    /// its lower edge was 58 and the test guarded nothing: measured by the
    /// mutation below leaving it green.
    ///
    /// Mutation: ask about the entity before the knob in `selection::grab`,
    /// and this fails.
    #[test]
    fn the_knob_takes_the_press_over_an_entity_under_it() {
        let mut app = knob_editor();
        let middle = entity_at(&mut app, MIDDLE);
        let above_at = Vec3::new(0.0, 70.0, 0.0);
        let above = app
            .world_mut()
            .spawn((
                Sprite::from_color(Color::WHITE, Vec2::splat(64.0)),
                Transform::from_translation(above_at),
                Selectable,
            ))
            .id();
        app.update();
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);

        drag_through(&mut app, &round(MIDDLE, MIDDLE_KNOB, 45.0, 3));

        assert_eq!(selected(&app), [middle]);
        assert_ne!(transform(&app, middle).rotation, Quat::IDENTITY);
        assert_eq!(transform(&app, above).translation, above_at);
    }

    /// A press inside the knob's reach but off its centre takes it: the knob
    /// can be hit without aiming at its middle.
    ///
    /// Eight pixels beside the knob's centre, where the knob's reach is nine
    /// and its radius six, so the press is outside the circle as drawn.
    ///
    /// Mutation: a `KNOB_REACH` of one, and this fails.
    #[test]
    fn a_press_off_the_knobs_centre_within_its_reach_takes_it() {
        let mut app = knob_editor();
        let middle = entity_at(&mut app, MIDDLE);
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);
        let beside = MIDDLE_KNOB + Vec2::new(8.0, 0.0);

        drag_through(&mut app, &round(MIDDLE, beside, 45.0, 3));

        assert_eq!(selected(&app), [middle]);
        assert_ne!(transform(&app, middle).rotation, Quat::IDENTITY);
    }

    /// A press just outside the knob's reach is not on it: dragged, it is a
    /// box, and nothing turns.
    ///
    /// Fifteen pixels above the knob's centre, where the knob's reach is
    /// nine.
    ///
    /// Mutation: compare against ten times `KNOB_REACH` in `Knob::under`, and
    /// this fails.
    #[test]
    fn a_press_beside_the_knob_is_not_on_it() {
        let mut app = knob_editor();
        let middle = entity_at(&mut app, MIDDLE);
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);
        let beside = MIDDLE_KNOB + Vec2::new(0.0, 15.0);

        drag_through(&mut app, &round(MIDDLE, beside, 45.0, 3));

        assert_eq!(transform(&app, middle).rotation, Quat::IDENTITY);
    }
}
