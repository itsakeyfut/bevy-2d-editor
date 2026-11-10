//! The handles drawn on what is selected: the rotate knob and the scale
//! handles.
//!
//! What the knob is, where it sits, what a press on it does and what was
//! turned down for it are settled in
//! [`docs/specs/ui.md` §12](../../../docs/specs/ui.md); the same for the scale
//! handles is §13. The press is taken in `selection.rs`, because §4 is one
//! gesture table, and the turn and the scale are done in `moving.rs`, because
//! they are drags like a move.

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
/// Chosen rather than measured, as are the three below.
const STALK: f32 = 24.0;

/// The knob's radius as drawn, in logical pixels.
const KNOB_RADIUS: f32 = 6.0;

/// A scale handle's side as drawn, in logical pixels.
const HANDLE_SIZE: f32 = 8.0;

/// How close a press has to be to a handle's centre to take it, in logical
/// pixels. Larger than the knob and the squares, so that they can be hit
/// without aiming. One reach for every handle.
const REACH: f32 = 9.0;

/// Where the handles were drawn last, so that a press is tested against what
/// the user saw.
///
/// Written in `PostUpdate` and read by the next frame's press in `PreUpdate`,
/// so a press is tested against **the handles drawn in the frame before it**,
/// which are the ones on the screen when the button went down.
#[derive(Resource, Default)]
pub(crate) struct Handles {
    /// The rotate knob.
    knob: Option<Placed>,
    /// The selection's frame and the scale handles on it.
    frame: Option<Framed>,
}

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

/// The selection's frame on the viewport, and the eight scale handles on it.
struct Framed {
    /// The frame, in the coordinates the viewport's pointer is read in.
    viewport: Rect,
    /// The handles, corners first, so that a tie goes to a corner.
    handles: Vec<ScaleHandle>,
}

/// One scale handle, in the two sets of units it is read in.
struct ScaleHandle {
    /// Its centre, in the coordinates the viewport's pointer is read in.
    at: Vec2,
    /// Its centre, in world units.
    handle: Vec2,
    /// The side opposite it, which holds still, in world units. For an
    /// edge, it is level with the handle along the edge, so that axis has
    /// nothing to scale by.
    pivot: Vec2,
    /// On each axis, one logical pixel over the frame's size on screen, and
    /// never more than one, which is the least a factor can be.
    floor: Vec2,
}

/// Where a handle is on the frame, as a multiple of its half size from the
/// centre: the four corners, then the middles of the four edges.
const AROUND: [Vec2; 8] = [
    Vec2::new(-1.0, -1.0),
    Vec2::new(1.0, -1.0),
    Vec2::new(1.0, 1.0),
    Vec2::new(-1.0, 1.0),
    Vec2::new(-1.0, 0.0),
    Vec2::new(1.0, 0.0),
    Vec2::new(0.0, -1.0),
    Vec2::new(0.0, 1.0),
];

impl Handles {
    /// What a press at `location` takes hold of, when it is on a handle.
    ///
    /// The knob first, though the order is not observable and no test holds
    /// it: the knob stands `STALK` pixels above the frame and the nearest
    /// scale handle is on it, so their reaches, `REACH` each, never meet.
    pub(crate) fn under(&self, location: &PointerLocation) -> Option<Grip> {
        let at = location.location.as_ref()?.position;
        self.knob_under(at).or_else(|| self.scale_under(at))
    }

    /// The knob, when `at` is on it.
    ///
    /// Mutation: compare against ten times `REACH`, and
    /// `a_press_beside_the_knob_is_not_on_it` fails. Mutation: a `REACH` of
    /// one, and `a_press_off_the_knobs_centre_within_its_reach_takes_it` fails.
    fn knob_under(&self, at: Vec2) -> Option<Grip> {
        let placed = self.knob.as_ref()?;
        (placed.at.distance(at) <= REACH).then_some(Grip::Knob {
            centre: placed.centre,
            base: placed.base,
            tip: placed.tip,
        })
    }

    /// The nearest scale handle within reach of `at`, **unless `at` is inside
    /// the frame**, where a press is the body's or the box's:
    /// [`docs/specs/ui.md` §13](../../../docs/specs/ui.md). On the frame's
    /// edge counts as outside.
    ///
    /// Mutation: drop the inside check, and
    /// `a_press_inside_the_frame_near_a_corner_moves_the_body` fails.
    /// Mutation: a `REACH` of one, and
    /// `a_press_just_outside_a_corner_takes_it` fails.
    fn scale_under(&self, at: Vec2) -> Option<Grip> {
        let framed = self.frame.as_ref()?;
        let inside = framed.viewport.min.cmplt(at).all() && at.cmplt(framed.viewport.max).all();
        if inside {
            return None;
        }
        framed
            .handles
            .iter()
            .map(|handle| (handle, handle.at.distance(at)))
            .filter(|(_, distance)| *distance <= REACH)
            .min_by(|(_, a), (_, b)| a.total_cmp(b))
            .map(|(handle, _)| Grip::Scale {
                handle: handle.handle,
                pivot: handle.pivot,
                floor: handle.floor,
            })
    }
}

/// The gizmo group the rotate knob is drawn in.
///
/// Its own group rather than `outline::SelectionGizmos`, on the same layer,
/// for the reason `selection::BandGizmos` gives: the outline's tests read
/// whether that group has a handle at all to mean "nothing is drawn".
#[derive(Default, Reflect, GizmoConfigGroup)]
#[reflect(Default)]
pub(crate) struct KnobGizmos;

/// The gizmo group the scale handles are drawn in.
///
/// Its own group rather than [`KnobGizmos`], because `drawn_knob` reads the
/// one rectangle that group draws as the knob, and eight squares beside it
/// would leave it nothing to read.
#[derive(Default, Reflect, GizmoConfigGroup)]
#[reflect(Default)]
pub(crate) struct ScaleGizmos;

/// The handles on what is selected.
///
/// Mutation: leave its row out of the editor's member table, and
/// `the_group_carries_the_members_the_table_names` fails.
pub struct HandlesPlugin;

impl Plugin for HandlesPlugin {
    fn build(&self, app: &mut App) {
        let on_the_selection_layer = || GizmoConfig {
            render_layers: RenderLayers::layer(SELECTION_LAYER),
            ..default()
        };
        app.init_resource::<Handles>()
            .insert_gizmo_config(KnobGizmos, on_the_selection_layer())
            .insert_gizmo_config(ScaleGizmos, on_the_selection_layer())
            .add_systems(
                PostUpdate,
                place_handles
                    .after(VisibilitySystems::CalculateBounds)
                    .after(crate::moving::follow),
            );
    }
}

/// Place the knob above what is selected and the scale handles on its frame,
/// draw them, and remember where.
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
/// factor appears here, and a headless test stuck at a scale factor of one
/// has nothing to miss. The squares are sized the same way.
///
/// **While a turn is held**, the knob is drawn where the turn has carried it,
/// about the pivot, rather than above the frame, whose top moves as the
/// corners turn; and the knob's place is left as it was, since nothing reads
/// it until the next press. The scale handles are drawn on the frame
/// whatever is held, and so trail a moving entity by a frame as the outline
/// does: [`docs/specs/ui.md` §13](../../../docs/specs/ui.md).
///
/// Mutation: return before drawing, and `selecting_an_entity_draws_a_knob_above_it`
/// fails. Mutation: build the frame from every entity with bounds rather than
/// from the selection, and `nothing_selected_draws_no_knob` fails. Mutation: add `STALK` to the
/// frame's top in world units, and
/// `the_knob_is_the_same_distance_on_screen_at_every_zoom` fails. Mutation:
/// build the frame from the `Aabb` untransformed, and
/// `the_knob_is_above_a_turned_and_scaled_entitys_frame` fails.
fn place_handles(
    selection: Res<Selection>,
    bounds: Query<(&GlobalTransform, &Aabb)>,
    cameras: Query<(&Camera, &GlobalTransform), With<ViewportCamera>>,
    stroke: Res<Stroke>,
    mut handles: ResMut<Handles>,
    mut gizmos: (Gizmos<KnobGizmos>, Gizmos<ScaleGizmos>),
) {
    let Ok((camera, eye)) = cameras.single() else {
        *handles = Handles::default();
        return;
    };
    let frame = frame(&selection, &bounds);

    handles.frame = frame.and_then(|frame| framed(camera, eye, frame));
    if let Some(framed) = &handles.frame {
        for handle in &framed.handles {
            draw_square(&mut gizmos.1, camera, eye, handle.handle);
        }
    }

    if let Some((Grip::Knob { base, tip, .. }, pivot, angle)) = stroke.turned() {
        let round = Rot2::radians(angle);
        draw(
            &mut gizmos.0,
            camera,
            eye,
            pivot + round * (base - pivot),
            pivot + round * (tip - pivot),
        );
        return;
    }

    handles.knob = frame.and_then(|frame| place(camera, eye, frame));
    if let Some(placed) = &handles.knob {
        draw(&mut gizmos.0, camera, eye, placed.base, placed.tip);
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

/// The scale handles on `frame`.
///
/// Each point goes to the viewport through the camera on its own, so the
/// viewport's downward `y` is never turned over by hand, and the floor is
/// one logical pixel over the frame's size there, for the reason
/// [`place_handles`] measures the stalk on the viewport.
///
/// **The floor is never more than one.** A frame already thinner than a
/// pixel, a tile at the furthest zoom or a line, would otherwise have a floor
/// above one, and a drag let go where it was pressed would grow it and record
/// that: [`docs/specs/ui.md` §13](../../../docs/specs/ui.md). Capped, it stops
/// a shrink and never causes a growth.
///
/// Mutation: drop the cap, and
/// `a_frame_thinner_than_a_pixel_is_not_grown_by_the_floor` fails.
/// Mutation: put an edge's pivot on a corner, `centre - half * side.signum()`,
/// and
/// `dragging_an_edge_scales_only_the_axis_across_it` fails. Mutation: put the
/// pivot at the frame's centre, and
/// `dragging_a_corner_scales_both_axes_about_the_opposite_corner` fails.
fn framed(camera: &Camera, eye: &GlobalTransform, frame: Rect) -> Option<Framed> {
    let to_viewport = |at: Vec2| camera.world_to_viewport(eye, at.extend(0.0)).ok();
    let viewport = Rect::from_corners(to_viewport(frame.min)?, to_viewport(frame.max)?);
    let floor = (Vec2::ONE / viewport.size()).min(Vec2::ONE);
    let (centre, half) = (frame.center(), frame.half_size());
    let handles = AROUND
        .iter()
        .map(|side| {
            let handle = centre + half * *side;
            Some(ScaleHandle {
                at: to_viewport(handle)?,
                handle,
                pivot: centre - half * *side,
                floor,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Framed { viewport, handles })
}

/// The side on the viewport of `size` logical pixels, as a length in the
/// world beside `at`.
fn world_across(camera: &Camera, eye: &GlobalTransform, at: Vec2, size: f32) -> Option<f32> {
    let on_screen = camera.world_to_viewport(eye, at.extend(0.0)).ok()?;
    let edge = camera
        .viewport_to_world_2d(eye, on_screen + Vec2::new(size, 0.0))
        .ok()?;
    Some(at.distance(edge))
}

/// Draw the stalk from `base` and the knob at `tip`.
///
/// The radius is `KNOB_RADIUS` on the viewport, taken back into the world
/// beside the knob, for the reason [`place_handles`] measures the stalk there.
fn draw(
    gizmos: &mut Gizmos<KnobGizmos>,
    camera: &Camera,
    eye: &GlobalTransform,
    base: Vec2,
    tip: Vec2,
) {
    gizmos.line_2d(base, tip, OUTLINE);
    if let Some(radius) = world_across(camera, eye, tip, KNOB_RADIUS) {
        gizmos.circle_2d(tip, radius, OUTLINE);
    }
}

/// Draw one scale handle, a square `HANDLE_SIZE` across on the viewport.
///
/// Mutation: return before drawing, and
/// `selecting_an_entity_draws_eight_scale_handles_on_its_frame` fails.
/// Mutation: use `HANDLE_SIZE` as world units, and
/// `the_scale_handles_are_the_same_size_on_screen_at_every_zoom` fails.
fn draw_square(gizmos: &mut Gizmos<ScaleGizmos>, camera: &Camera, eye: &GlobalTransform, at: Vec2) {
    if let Some(side) = world_across(camera, eye, at, HANDLE_SIZE) {
        gizmos.rect_2d(Isometry2d::from_translation(at), Vec2::splat(side), OUTLINE);
    }
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
    let drawn = crate::drawn::rectangles_of::<KnobGizmos>(app);
    assert!(drawn.len() <= 1, "more than one knob drawn: {drawn:?}");
    drawn.first().copied()
}

/// Where the stalk was drawn, from the frame to the knob, in world units.
#[cfg(test)]
fn drawn_stalk(app: &App) -> Option<(Vec2, Vec2)> {
    let drawn = crate::drawn::lines_of::<KnobGizmos>(app);
    assert!(drawn.len() <= 1, "more than one stalk drawn: {drawn:?}");
    drawn.first().copied()
}

/// Every scale handle drawn, as the square it was drawn as, in world units.
#[cfg(test)]
fn drawn_scale_handles(app: &App) -> Vec<Rect> {
    crate::drawn::rectangles_of::<ScaleGizmos>(app)
}

#[cfg(test)]
mod tests {
    use super::{drawn_knob, drawn_scale_handles, drawn_stalk};
    use crate::pointer::{
        click_at, drag_through, in_window, line, press_and_hold, press_then_release, round,
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
    /// Mutation: return in `place_handles` before drawing, and this fails.
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
    /// Every placeholder is unturned and unscaled, which leaves transform
    /// maths unguarded, so this spawns the awkward one. A 64 pixel square
    /// scaled by two and turned an eighth of a turn reaches `64 * sqrt(2)`
    /// above its origin.
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
    /// Mutation: drop the branch in `place_handles` that draws a turn in
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
    /// One release runs two writers, the click and the drag end. Here the
    /// release lands on empty space, where the click that accompanies it would
    /// clear the selection.
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
    /// One release runs two writers, the click and the drag end. Here it ends
    /// over the left placeholder, with a box that would have
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
    /// Mutation: a `REACH` of one, and this fails.
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
    /// Mutation: compare against ten times `REACH` in `Handles::knob_under`,
    /// and this fails.
    #[test]
    fn a_press_beside_the_knob_is_not_on_it() {
        let mut app = knob_editor();
        let middle = entity_at(&mut app, MIDDLE);
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);
        let beside = MIDDLE_KNOB + Vec2::new(0.0, 15.0);

        drag_through(&mut app, &round(MIDDLE, beside, 45.0, 3));

        assert_eq!(transform(&app, middle).rotation, Quat::IDENTITY);
    }

    /// Where the scale handles should be on the middle placeholder: its four
    /// corners and the middles of its four edges.
    const MIDDLE_HANDLES: [Vec2; 8] = [
        Vec2::new(-32.0, -32.0),
        Vec2::new(32.0, -32.0),
        Vec2::new(32.0, 32.0),
        Vec2::new(-32.0, 32.0),
        Vec2::new(-32.0, 0.0),
        Vec2::new(32.0, 0.0),
        Vec2::new(0.0, -32.0),
        Vec2::new(0.0, 32.0),
    ];

    /// Every drawn square is on one of `wanted`, `side` across, and every one
    /// of `wanted` has a square.
    fn assert_squares_on(drawn: &[Rect], wanted: &[Vec2], side: f32) {
        assert_eq!(drawn.len(), wanted.len(), "drawn: {drawn:?}");
        for at in wanted {
            let square = drawn
                .iter()
                .find(|square| square.center().abs_diff_eq(*at, 1e-3))
                .unwrap_or_else(|| panic!("no square on {at}: {drawn:?}"));
            assert_near(square.size(), Vec2::splat(side));
        }
    }

    /// Selecting an entity draws a square on each corner of its frame and on
    /// the middle of each edge, 8 pixels across.
    ///
    /// Mutation: return in `handles::draw_square` before drawing, and this
    /// fails. Mutation: drop the four edges from `AROUND`, and this fails.
    #[test]
    fn selecting_an_entity_draws_eight_scale_handles_on_its_frame() {
        let mut app = knob_editor();
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);

        assert_squares_on(&drawn_scale_handles(&app), &MIDDLE_HANDLES, 8.0);
    }

    /// Zoomed out, the squares are the same size on screen, which is more of
    /// the world, and still on the frame's corners and edges.
    ///
    /// Mutation: take `HANDLE_SIZE` as world units in `handles::draw_square`,
    /// and this fails.
    #[test]
    fn the_scale_handles_are_the_same_size_on_screen_at_every_zoom() {
        let mut app = knob_editor();
        zoom_to(&mut app, 2.0);
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);

        assert_squares_on(&drawn_scale_handles(&app), &MIDDLE_HANDLES, 16.0);
    }

    /// A press just outside a corner, within its reach, takes it: dragged, it
    /// scales.
    ///
    /// Six pixels out from the corner on each axis, about eight and a half
    /// from its centre, where the reach is nine.
    ///
    /// Mutation: a `REACH` of one, and this fails.
    #[test]
    fn a_press_just_outside_a_corner_takes_it() {
        let mut app = knob_editor();
        let middle = entity_at(&mut app, MIDDLE);
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);

        drag_through(
            &mut app,
            &line(Vec2::new(38.0, 38.0), Vec2::new(70.0, 70.0), 4),
        );

        assert_ne!(transform(&app, middle).scale, Vec3::ONE);
    }

    /// A press inside the frame, within a corner's reach, is the body's: it
    /// moves what is selected and scales nothing.
    ///
    /// Mutation: drop the inside check in `Handles::scale_under`, and this
    /// fails.
    #[test]
    fn a_press_inside_the_frame_near_a_corner_moves_the_body() {
        let mut app = knob_editor();
        let middle = entity_at(&mut app, MIDDLE);
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);

        drag_through(
            &mut app,
            &line(Vec2::new(28.0, 28.0), Vec2::new(50.0, 50.0), 4),
        );

        assert_eq!(transform(&app, middle).scale, Vec3::ONE);
        assert_near(
            transform(&app, middle).translation.truncate(),
            Vec2::new(22.0, 22.0),
        );
    }

    /// Clicking a scale handle leaves the selection as it was and scales
    /// nothing.
    ///
    /// Five pixels right of the right edge's handle, within its reach and
    /// over nothing, so the click that comes with the release would clear
    /// the selection if the handle did not hold it back.
    ///
    /// Mutation: drop the `pressed.handle` check in `selection::select`, and
    /// this fails.
    #[test]
    fn clicking_a_scale_handle_keeps_the_selection() {
        let mut app = knob_editor();
        let middle = entity_at(&mut app, MIDDLE);
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);

        click_at(
            &mut app,
            in_window(Vec2::new(37.0, 0.0)),
            PointerButton::Primary,
        );

        assert_eq!(selected(&app), [middle]);
        assert_eq!(transform(&app, middle).scale, Vec3::ONE);
    }

    /// Zoomed out, the floor is still one pixel on screen: the middle
    /// placeholder is 32 pixels across at a camera scale of two, so its
    /// right edge dragged past its left stops at a scale of `1 / 32`.
    ///
    /// Mutation: take the floor from the frame's size in world units in
    /// `handles::framed`, and this fails reading `1 / 64`.
    #[test]
    fn the_floor_is_one_pixel_on_screen_at_every_zoom() {
        let mut app = knob_editor();
        let middle = entity_at(&mut app, MIDDLE);
        zoom_to(&mut app, 2.0);
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);

        // The world at half size, since `in_window` assumes a scale of one.
        let path: Vec<Vec2> = (0_u8..=6)
            .map(|step: u8| {
                let world =
                    Vec2::new(32.0, 0.0).lerp(Vec2::new(-100.0, 0.0), f32::from(step) / 6.0);
                in_window(world / 2.0)
            })
            .collect();
        drag_through(&mut app, &path);

        assert_near(
            transform(&app, middle).scale.truncate(),
            Vec2::new(1.0 / 32.0, 1.0),
        );
    }

    /// Where two handles are within reach, the nearer one takes the press.
    ///
    /// On an 8 pixel square, a press 3 pixels right of the right edge's
    /// handle is 5 from the bottom right corner's, which comes first in
    /// `AROUND`. Dragged up and to the right, the edge leaves `y` alone and
    /// the corner would not.
    ///
    /// Mutation: take the first handle within reach rather than the nearest
    /// in `Handles::scale_under`, and this fails.
    #[test]
    fn the_nearest_handle_within_reach_takes_the_press() {
        let mut app = knob_editor();
        let at = Vec2::new(0.0, -150.0);
        let small = app
            .world_mut()
            .spawn((
                Sprite::from_color(Color::WHITE, Vec2::splat(8.0)),
                Transform::from_translation(at.extend(0.0)),
                Selectable,
            ))
            .id();
        app.update();
        click_at(&mut app, in_window(at), PointerButton::Primary);

        drag_through(
            &mut app,
            &line(Vec2::new(7.0, -150.0), Vec2::new(20.0, -140.0), 4),
        );

        let scale = transform(&app, small).scale;
        assert_ne!(scale.x, 1.0, "nothing scaled");
        assert_eq!(scale.y, 1.0);
    }
}
