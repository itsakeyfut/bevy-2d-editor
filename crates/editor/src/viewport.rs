//! The 2D viewport: a camera rendering into the middle region of
//! [`docs/specs/ui.md` §1](../../../docs/specs/ui.md).
//!
//! What the viewport shows is a Bevy camera rendering a real world rather than
//! a picture the editor composes itself, which is `docs/specs/architecture.md`
//! §5 applied to the surface every later editing tool attaches to. The
//! navigation bindings are `docs/specs/ui.md` §4.

use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::image::{Image, ToExtents};
use bevy::input::mouse::MouseScrollUnit;
use bevy::picking::events::{Drag, Pointer, Scroll};
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use bevy::render::render_resource::{TextureDimension, TextureFormat, TextureUsages};
use bevy::ui::widget::ViewportNode;
use bevy::ui::{ComputedNode, UiGlobalTransform};

use crate::Region;

/// The camera whose view fills [`Region::Viewport`].
///
/// A second camera beside the one the panels are drawn to, rather than the
/// same one. The two want different render targets: the panels are drawn to
/// the window, and this one is drawn to an image that a UI node holds.
#[derive(Component)]
pub struct ViewportCamera;

/// How much one notch of the wheel multiplies the visible area by.
const ZOOM_PER_NOTCH: f32 = 1.2;

/// How many pixels of a trackpad's scroll count as one notch of a wheel.
///
/// `MouseScrollUnit::Pixel` arrives from touchpads and from platforms that
/// report smooth scrolling, and a pixel treated as a notch would zoom by a
/// factor of twenty in one gesture.
const PIXELS_PER_NOTCH: f32 = 16.0;

/// The closest the camera zooms in, in world units per pixel.
const ZOOM_MIN: f32 = 1.0 / 32.0;

/// The furthest the camera zooms out, in world units per pixel.
///
/// The range is clamped rather than left open because an unclamped scale
/// reaches zero, and a projection with no width is a camera that shows nothing
/// and that scrolling back does not recover.
const ZOOM_MAX: f32 = 32.0;

/// The colour of the placeholder in the world.
///
/// Warm, so that it reads against the window's own dark background. It is here
/// to be something the viewport can show and something a person can watch move
/// while panning; the first change that puts real content in the world deletes
/// it.
const PLACEHOLDER: Color = Color::srgb(0.85, 0.62, 0.25);

/// How big the placeholder is, in world units.
const PLACEHOLDER_SIZE: f32 = 64.0;

/// The 2D viewport, and the navigation `docs/specs/ui.md` §4 decided.
///
/// Mutation: leave its row out of the editor's member table, and
/// `the_group_carries_the_members_the_table_names` fails.
pub struct ViewportPlugin;

impl Plugin for ViewportPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(attach);
    }
}

/// Put a camera behind the viewport region as soon as the region exists.
///
/// An observer rather than a `Startup` system ordered after the panels: the
/// region is spawned by another plugin, and ordering against it would mean
/// cutting a public `SystemSet` into `PanelsPlugin` for this plugin's benefit.
///
/// **It runs once because the region is spawned once.** Nothing respawns the
/// layout: `spawn_regions` is a `Startup` system and its scenes are written
/// inline rather than loaded, so there is no reload that would bring a second
/// `Add`. If one ever arrives, this spawns a second camera and a second
/// placeholder and despawns neither, and `pan` and `zoom` both ask for a single
/// camera and would quietly stop answering. Whatever brings that day, docking
/// most likely, is what has to make this replace rather than add;
/// `docs/specs/open-questions.md` §1 is where docking is deferred, with its
/// trigger.
///
/// Mutation: drop the [`ViewportNode`] insert, and
/// `the_viewport_camera_renders_to_the_region_and_not_the_window` fails.
fn attach(
    add: On<Add, Region>,
    regions: Query<&Region>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
) {
    if regions.get(add.entity) != Ok(&Region::Viewport) {
        return;
    }

    let mut image = Image::new_fill(
        // One pixel, because `update_viewport_render_target_size` sizes this
        // from the region on the first layout pass. A window's worth of pixels
        // written here would be wrong in every window and corrected a frame
        // later.
        UVec2::ONE.to_extents(),
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Bgra8UnormSrgb,
        RenderAssetUsages::default(),
    );
    // `Image::new_fill` asks for TEXTURE_BINDING, COPY_DST and COPY_SRC, and a
    // camera cannot draw into a texture that is not an attachment. A headless
    // test has no adapter to complain, so the usage is asserted by name in
    // `the_render_target_can_be_drawn_into` rather than left to a person.
    image.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT;

    let target = images.add(image);
    let camera = commands
        .spawn((Camera2d, ViewportCamera, RenderTarget::Image(target.into())))
        .id();

    commands.spawn(Sprite::from_color(
        PLACEHOLDER,
        Vec2::splat(PLACEHOLDER_SIZE),
    ));

    commands
        .entity(add.entity)
        .insert(ViewportNode::new(camera))
        .observe(pan)
        .observe(zoom);
}

/// How much of the world the viewport is showing, in world units.
///
/// Worked out from the region and the zoom rather than read from
/// `OrthographicProjection::area`, **which is last frame's**. `area` is an
/// output of Bevy's `camera_system`, which runs once per frame in `PostUpdate`;
/// an observer that has already changed `scale` earlier in the same frame is
/// looking at the extent from before it did. Several `Pointer<Scroll>` in one
/// frame is the ordinary case rather than a corner: `bevy_picking` turns every
/// buffered wheel event into one, and one flick of a wheel is several.
/// `a_burst_of_scrolling_in_one_frame_holds_the_cursor_still` is that claim.
///
/// `Camera2d` projects with `ScalingMode::WindowSize`, where
/// `OrthographicProjection::update` writes `area` as the viewport's own size
/// times `scale`. The viewport here is the render target, which
/// `update_viewport_render_target_size` holds equal to the region, so the
/// region's size times `scale` is the same number a frame earlier.
fn visible(node: &ComputedNode, projection: &OrthographicProjection) -> Vec2 {
    node.size() * projection.scale
}

/// How many world units one logical pixel of the region covers.
///
/// Both sides of the division are logical pixels, so this is right whatever the
/// window's scale factor is, and [`visible`] is current within the frame, so it
/// is right at every zoom and in the frame a zoom happens.
///
/// **`inverse_scale_factor` is the one thing here no test holds.** Dropping it
/// leaves every test green, because a headless app is stuck at a scale factor
/// of one: `Window::resolution.set_scale_factor_override` does not move
/// `ComputedNode::inverse_scale_factor` without winit, measured. What it costs
/// is panning at half speed on a display at 200%, which is invisible on a
/// machine at 100%. RK-005 in the knowledge bank carries that.
fn world_per_pixel(node: &ComputedNode, projection: &OrthographicProjection) -> Vec2 {
    visible(node, projection) / (node.size() * node.inverse_scale_factor())
}

/// Drag the world under the pointer, with the middle button.
///
/// `docs/specs/ui.md` §4 settles the button and what the other ones are being
/// kept for.
///
/// Mutation: drop the button check, and `a_drag_with_any_other_button_does_not_pan`
/// fails. Mutation: move the world instead of the camera, and
/// `a_pan_moves_the_camera_and_leaves_the_world_where_it_was` fails.
fn pan(
    drag: On<Pointer<Drag>>,
    regions: Query<&ComputedNode>,
    mut cameras: Query<(&mut Transform, &Projection), With<ViewportCamera>>,
) {
    if drag.event().button != PointerButton::Middle {
        return;
    }
    let Ok(node) = regions.get(drag.event().entity) else {
        return;
    };
    let Ok((mut transform, projection)) = cameras.single_mut() else {
        return;
    };
    let Projection::Orthographic(orthographic) = projection else {
        return;
    };

    let delta = drag.event().delta * world_per_pixel(node, orthographic);
    // Dragging right moves the world right, which moves the camera left; y is
    // the flip between screen space and world space.
    transform.translation.x -= delta.x;
    transform.translation.y += delta.y;
}

/// Zoom on the wheel, holding the world point under the cursor still.
///
/// `docs/specs/ui.md` §4 settles both halves of that.
///
/// Mutation: drop the translation, and
/// `a_zoom_holds_the_world_point_under_the_cursor_still` fails. Mutation: take
/// the ratio before clamping, and `zoom_stops_at_the_ends_of_its_range` fails.
/// Mutation: read `orthographic.area.size()` instead of [`visible`], and
/// `a_burst_of_scrolling_in_one_frame_holds_the_cursor_still` fails.
fn zoom(
    scroll: On<Pointer<Scroll>>,
    regions: Query<(&ComputedNode, &UiGlobalTransform)>,
    mut cameras: Query<(&mut Transform, &mut Projection), With<ViewportCamera>>,
) {
    let notches = match scroll.event().unit {
        MouseScrollUnit::Line => scroll.event().y,
        MouseScrollUnit::Pixel => scroll.event().y / PIXELS_PER_NOTCH,
    };
    if notches == 0.0 {
        return;
    }
    let Ok((node, ui_transform)) = regions.get(scroll.event().entity) else {
        return;
    };
    let Ok((mut transform, mut projection)) = cameras.single_mut() else {
        return;
    };
    let Projection::Orthographic(orthographic) = &mut *projection else {
        return;
    };

    // Clamped before the ratio is taken, so that at either end the camera stops
    // rather than carrying on drifting sideways.
    let scale = (orthographic.scale * ZOOM_PER_NOTCH.powf(-notches)).clamp(ZOOM_MIN, ZOOM_MAX);
    let ratio = scale / orthographic.scale;

    // Where the cursor is in the region, from its centre, in halves. The same
    // normalisation `bevy_ui`'s own `viewport_picking` does, with y flipped
    // into world space.
    let logical = node.size() * node.inverse_scale_factor();
    let top_left = (ui_transform.translation - node.size() * 0.5) * node.inverse_scale_factor();
    let from_centre =
        (scroll.event().pointer_location.position - top_left) / logical - Vec2::splat(0.5);
    let from_centre = Vec2::new(from_centre.x, -from_centre.y);

    // Hold the world point under the cursor still. Arithmetic rather than a
    // round trip through the camera: the camera's own view of itself is a
    // frame old, which `visible` is about, and so is `Camera::viewport_to_world_2d`.
    transform.translation +=
        (from_centre * visible(node, orthographic) * (1.0 - ratio)).extend(0.0);
    orthographic.scale = scale;
}

#[cfg(test)]
mod tests {
    use super::{PIXELS_PER_NOTCH, PLACEHOLDER_SIZE, ViewportCamera, ZOOM_MAX, ZOOM_MIN};
    use crate::{Region, editor, headless};
    use bevy::camera::{NormalizedRenderTarget, RenderTarget};
    use bevy::input::mouse::MouseScrollUnit;
    use bevy::picking::backend::HitData;
    use bevy::picking::events::{Drag, Pointer, Scroll};
    use bevy::picking::pointer::{Location, PointerButton, PointerId};
    use bevy::prelude::*;
    use bevy::render::render_resource::TextureUsages;
    use bevy::ui::{ComputedNode, px};
    use bevy::window::{PrimaryWindow, WindowRef};

    /// How many frames the viewport takes to settle from nothing.
    ///
    /// Two, and the second one is not padding. In the first frame the layout
    /// is measured and `update_viewport_render_target_size` resizes the target
    /// from it, which emits `AssetEvent::Modified`; `camera_system` runs
    /// `before(AssetEventSystems)`, so it reads that event in the frame after,
    /// and only then does the projection describe the region rather than the
    /// one pixel the target was created with. One update here, measured,
    /// leaves a pan moving the camera by 1/740th of what it should.
    ///
    /// What that costs the editor is one frame at startup, before a window is
    /// on screen to drag in.
    const UPDATES: usize = 2;

    /// The editor, run until the viewport has settled.
    fn viewport_editor() -> App {
        let mut app = editor(headless());
        for _ in 0..UPDATES {
            app.update();
        }
        app
    }

    /// The entity carrying a region.
    fn region_of(app: &mut App, wanted: Region) -> Entity {
        app.world_mut()
            .query::<(Entity, &Region)>()
            .iter(app.world())
            .find(|(_, region)| **region == wanted)
            .map(|(entity, _)| entity)
            .expect("every region is on screen")
    }

    /// What the viewport camera renders into, and how big it currently is.
    fn target_size(app: &mut App) -> UVec2 {
        let handle = app
            .world_mut()
            .query_filtered::<&RenderTarget, With<ViewportCamera>>()
            .single(app.world())
            .expect("there is one viewport camera")
            .as_image()
            .expect("the viewport camera renders into an image")
            .clone();
        let size = app
            .world()
            .resource::<Assets<Image>>()
            .get(&handle)
            .expect("the render target is an asset")
            .texture_descriptor
            .size;
        UVec2::new(size.width, size.height)
    }

    /// A pointer position in the primary window, in logical pixels.
    fn at(app: &mut App, position: Vec2) -> Location {
        let window = app
            .world_mut()
            .query_filtered::<Entity, With<PrimaryWindow>>()
            .single(app.world())
            .expect("there is a primary window");
        Location {
            position,
            target: NormalizedRenderTarget::Window(
                WindowRef::Primary
                    .normalize(Some(window))
                    .expect("the primary window normalises"),
            ),
        }
    }

    /// Drag across the viewport with a button, from wherever the pointer is.
    fn drag(app: &mut App, button: PointerButton, delta: Vec2) {
        let viewport = region_of(app, Region::Viewport);
        let location = at(app, Vec2::new(600.0, 300.0));
        app.world_mut().trigger(Pointer::new(
            PointerId::Mouse,
            location,
            Drag {
                button,
                distance: delta,
                delta,
            },
            viewport,
        ));
        app.update();
    }

    /// Turn the wheel by `notches` with the pointer at a window position.
    fn scroll(app: &mut App, position: Vec2, notches: f32) {
        scroll_in(app, position, MouseScrollUnit::Line, notches);
    }

    /// Turn the wheel, or push a trackpad, by `amount` of a unit.
    fn scroll_in(app: &mut App, position: Vec2, unit: MouseScrollUnit, amount: f32) {
        let viewport = region_of(app, Region::Viewport);
        let location = at(app, position);
        app.world_mut().trigger(Pointer::new(
            PointerId::Mouse,
            location.clone(),
            Scroll {
                unit,
                x: 0.0,
                y: amount,
                hit: HitData::new(viewport, 0.0, None, None),
                phase: bevy::input::touch::TouchPhase::Moved,
            },
            viewport,
        ));
        app.update();
    }

    /// Turn the wheel `count` times before the frame ends.
    ///
    /// The updateless part is the point: `scroll_in` runs one, and a wheel
    /// delivers several between two frames.
    fn scroll_burst(app: &mut App, position: Vec2, count: usize) {
        let viewport = region_of(app, Region::Viewport);
        let location = at(app, position);
        for _ in 0..count {
            app.world_mut().trigger(Pointer::new(
                PointerId::Mouse,
                location.clone(),
                Scroll {
                    unit: MouseScrollUnit::Line,
                    x: 0.0,
                    y: 1.0,
                    hit: HitData::new(viewport, 0.0, None, None),
                    phase: bevy::input::touch::TouchPhase::Moved,
                },
                viewport,
            ));
        }
        app.update();
    }

    /// Where the camera is, and what it is showing.
    fn camera(app: &mut App) -> (Vec3, f32) {
        let (transform, projection) = app
            .world_mut()
            .query_filtered::<(&Transform, &Projection), With<ViewportCamera>>()
            .single(app.world())
            .expect("there is one viewport camera");
        let Projection::Orthographic(orthographic) = projection else {
            panic!("the viewport camera is not orthographic");
        };
        (transform.translation, orthographic.scale)
    }

    /// The viewport camera renders to the region, and not to the whole window.
    ///
    /// The size is the claim. 740 is 1280 less the asset browser and the
    /// inspector, and 512 is 720 less the menu bar and the bottom panel, so a
    /// camera that had been given the window would measure 1280 by 720 and fail
    /// here.
    ///
    /// Mutation: drop the `ViewportNode` insert in `attach`, and the target
    /// stays at the one pixel it was created with. Mutation: give the camera
    /// `RenderTarget::Window`, and there is no image to measure.
    #[test]
    fn the_viewport_camera_renders_to_the_region_and_not_the_window() {
        let mut app = viewport_editor();
        let viewport = region_of(&mut app, Region::Viewport);
        let region = app
            .world()
            .entity(viewport)
            .get::<ComputedNode>()
            .expect("the viewport region is laid out")
            .size;

        assert_eq!(region, Vec2::new(740.0, 512.0), "the region moved");
        assert_eq!(
            target_size(&mut app),
            UVec2::new(740, 512),
            "the camera is not drawing the region"
        );
    }

    /// The render target follows the region when the region is resized.
    ///
    /// This is the half of "resizing the region changes what is visible rather
    /// than stretching it" that a test can hold: the target is resized rather
    /// than sampled differently, and `Camera2d` defaults to
    /// `ScalingMode::WindowSize`, so a larger target is more world and not a
    /// larger picture of the same world.
    ///
    /// Driven by narrowing the inspector rather than by resizing the window,
    /// which needs winit: what moves the layout is a `Node` changing.
    ///
    /// Mutation: drop the `ViewportNode` insert in `attach`, and the target
    /// stops following anything, which fails this and the test above it. There
    /// is no line here that sizes the target from the window: `bevy_ui`'s
    /// `update_viewport_render_target_size` does the sizing and this code only
    /// says which node, which is the whole reason the region is what it
    /// follows.
    #[test]
    fn the_render_target_follows_the_region_when_the_region_is_resized() {
        let mut app = viewport_editor();
        let inspector = region_of(&mut app, Region::Inspector);
        app.world_mut()
            .entity_mut(inspector)
            .get_mut::<Node>()
            .expect("the inspector is a node")
            .width = px(100);
        app.update();

        assert_eq!(
            target_size(&mut app),
            UVec2::new(940, 512),
            "the render target did not follow the region"
        );
    }

    /// The render target can be drawn into.
    ///
    /// `Image::new_fill` asks for `TEXTURE_BINDING`, `COPY_DST` and `COPY_SRC`,
    /// and a camera needs `RENDER_ATTACHMENT`. Nothing else here catches it: the
    /// headless platform has no adapter to refuse the texture, so without this
    /// the editor would come up with an empty viewport and a green suite.
    ///
    /// Mutation: drop the `|= TextureUsages::RENDER_ATTACHMENT` line, and this
    /// fails.
    #[test]
    fn the_render_target_can_be_drawn_into() {
        let mut app = viewport_editor();
        let handle = app
            .world_mut()
            .query_filtered::<&RenderTarget, With<ViewportCamera>>()
            .single(app.world())
            .expect("there is one viewport camera")
            .as_image()
            .expect("the viewport camera renders into an image")
            .clone();
        let usage = app
            .world()
            .resource::<Assets<Image>>()
            .get(&handle)
            .expect("the render target is an asset")
            .texture_descriptor
            .usage;

        assert!(
            usage.contains(TextureUsages::RENDER_ATTACHMENT),
            "the camera cannot draw into its own target: {usage:?}"
        );
    }

    /// A pan moves the camera and leaves the world where it was.
    ///
    /// The second half is the claim, and it is asserted over a different set
    /// from the one `pan` writes to: every `GlobalTransform` in the world
    /// except the camera's, before and after. A test that read the camera back
    /// after moving it would agree with itself, which is the shape RK-001 is
    /// about and which this issue's body named in advance.
    ///
    /// Mutation: move the placeholder instead of the camera, and this fails.
    #[test]
    fn a_pan_moves_the_camera_and_leaves_the_world_where_it_was() {
        let mut app = viewport_editor();
        let viewport_camera = app
            .world_mut()
            .query_filtered::<Entity, With<ViewportCamera>>()
            .single(app.world())
            .expect("there is one viewport camera");
        let world_of = |app: &mut App| -> Vec<(Entity, GlobalTransform)> {
            let mut found: Vec<(Entity, GlobalTransform)> = app
                .world_mut()
                .query::<(Entity, &GlobalTransform)>()
                .iter(app.world())
                .filter(|(entity, _)| *entity != viewport_camera)
                .map(|(entity, transform)| (entity, *transform))
                .collect();
            found.sort_by_key(|(entity, _)| *entity);
            found
        };

        let before = world_of(&mut app);
        drag(&mut app, PointerButton::Middle, Vec2::new(10.0, -4.0));
        let after = world_of(&mut app);

        assert_ne!(
            camera(&mut app).0,
            Vec3::ZERO,
            "the pan did not reach the camera"
        );
        assert!(
            !before.is_empty(),
            "there is nothing in the world to hold still"
        );
        assert_eq!(before, after, "the pan moved the world");
    }

    /// A pan moves the camera by what the pointer crossed.
    ///
    /// At the default zoom the region is 740 by 512 logical pixels and shows
    /// 740 by 512 world units, so one pixel is one unit. Zoomed out by a
    /// factor of two it is two, which is the part that fails if the delta is
    /// divided by the window's scale factor instead of by the region.
    ///
    /// Mutation: drop either sign, or read `scale` instead of `area`, and this
    /// fails.
    #[test]
    fn a_pan_moves_the_camera_by_what_the_pointer_crossed() {
        let mut app = viewport_editor();
        drag(&mut app, PointerButton::Middle, Vec2::new(10.0, -4.0));
        assert_eq!(
            camera(&mut app).0,
            Vec3::new(-10.0, -4.0, 0.0),
            "one logical pixel is not one world unit at the default zoom"
        );

        let mut app = viewport_editor();
        let mut projection = app
            .world_mut()
            .query_filtered::<&mut Projection, With<ViewportCamera>>()
            .single_mut(app.world_mut())
            .expect("there is one viewport camera");
        if let Projection::Orthographic(orthographic) = &mut *projection {
            orthographic.scale = 2.0;
        }
        app.update();
        drag(&mut app, PointerButton::Middle, Vec2::new(10.0, -4.0));
        assert_eq!(
            camera(&mut app).0,
            Vec3::new(-20.0, -8.0, 0.0),
            "the delta did not follow the zoom"
        );
    }

    /// A drag with any other button does not pan.
    ///
    /// The left button is what selection and every painting tool will want, so
    /// a viewport that panned on it would have to take it back later.
    ///
    /// Mutation: drop the `PointerButton::Middle` check, and this fails.
    #[test]
    fn a_drag_with_any_other_button_does_not_pan() {
        let mut app = viewport_editor();
        drag(&mut app, PointerButton::Primary, Vec2::new(10.0, -4.0));
        assert_eq!(camera(&mut app).0, Vec3::ZERO, "the left button panned");

        drag(&mut app, PointerButton::Secondary, Vec2::new(10.0, -4.0));
        assert_eq!(camera(&mut app).0, Vec3::ZERO, "the right button panned");
    }

    /// A zoom holds the world point under the cursor still.
    ///
    /// Asserted through `Camera::viewport_to_world_2d`, which is the engine's
    /// own inverse of the projection and not the arithmetic `zoom` does. The
    /// cursor is deliberately off centre: a zoom anchored on the viewport's
    /// centre passes this at the centre and fails here.
    ///
    /// Mutation: drop the translation from `zoom`, and this fails.
    #[test]
    fn a_zoom_holds_the_world_point_under_the_cursor_still() {
        let mut app = viewport_editor();
        // The viewport starts 240 into the window and 28 down, so this is 60
        // and 70 into the region, well off its centre.
        let cursor = Vec2::new(300.0, 98.0);
        let in_region = Vec2::new(60.0, 70.0);

        let under_cursor = |app: &mut App| {
            let (camera, transform) = app
                .world_mut()
                .query_filtered::<(&Camera, &GlobalTransform), With<ViewportCamera>>()
                .single(app.world())
                .expect("there is one viewport camera");
            camera
                .viewport_to_world_2d(transform, in_region)
                .expect("the cursor is inside the viewport")
        };

        let before = under_cursor(&mut app);
        scroll(&mut app, cursor, 3.0);
        let after = under_cursor(&mut app);

        assert_ne!(camera(&mut app).1, 1.0, "the wheel did not zoom");
        assert!(
            (before - after).length() < 0.5,
            "the world moved out from under the cursor: {before:?} became {after:?}"
        );
    }

    /// A trackpad's pixels are not a wheel's notches.
    ///
    /// `MouseScrollUnit::Pixel` arrives from touchpads and from platforms that
    /// report smooth scrolling, and a pixel read as a notch is a factor of
    /// twenty in one gesture rather than the factor of 1.2 a notch means. The
    /// claim is that the two units agree: `PIXELS_PER_NOTCH` pixels leave the
    /// camera where one notch does.
    ///
    /// Mutation: drop the `/ PIXELS_PER_NOTCH`, and this fails. Nothing else
    /// here sends a pixel, so nothing else catches it.
    #[test]
    fn a_trackpads_pixels_are_not_a_wheels_notches() {
        let cursor = Vec2::new(600.0, 300.0);

        let mut wheel = viewport_editor();
        scroll_in(&mut wheel, cursor, MouseScrollUnit::Line, 1.0);

        let mut trackpad = viewport_editor();
        scroll_in(
            &mut trackpad,
            cursor,
            MouseScrollUnit::Pixel,
            PIXELS_PER_NOTCH,
        );

        assert_eq!(
            camera(&mut trackpad),
            camera(&mut wheel),
            "a pixel and a notch are being read as the same amount"
        );
    }

    /// A burst of scrolling in one frame holds the cursor still too.
    ///
    /// The other zoom test turns the wheel once per frame, and that is not how
    /// a wheel is turned. `bevy_picking` makes one `Pointer<Scroll>` out of
    /// every buffered wheel event, so one flick arrives as several in the same
    /// frame, and `OrthographicProjection::area` does not move until Bevy's
    /// camera system runs at the end of it. Reading `area` in the second event
    /// of a frame is reading the extent from before the first one: measured at
    /// 10 world units of drift for two events, and 205 for eight, across a
    /// region 740 units wide.
    ///
    /// Mutation: put `orthographic.area.size()` back in `zoom`'s last
    /// arithmetic, and this fails while every other test stays green.
    #[test]
    fn a_burst_of_scrolling_in_one_frame_holds_the_cursor_still() {
        let mut app = viewport_editor();
        let cursor = Vec2::new(300.0, 98.0);
        let in_region = Vec2::new(60.0, 70.0);
        let under_cursor = |app: &mut App| {
            let (camera, transform) = app
                .world_mut()
                .query_filtered::<(&Camera, &GlobalTransform), With<ViewportCamera>>()
                .single(app.world())
                .expect("there is one viewport camera");
            camera
                .viewport_to_world_2d(transform, in_region)
                .expect("the cursor is inside the viewport")
        };

        let before = under_cursor(&mut app);
        scroll_burst(&mut app, cursor, 8);
        let after = under_cursor(&mut app);

        assert!(
            camera(&mut app).1 < 0.5,
            "eight notches did not zoom: {}",
            camera(&mut app).1
        );
        assert!(
            (before - after).length() < 0.5,
            "the world slid out from under the cursor: {before:?} became {after:?}"
        );
    }

    /// Zoom stops at the ends of its range.
    ///
    /// A scale that reaches zero is a projection with no width, which shows
    /// nothing and which scrolling back does not recover: row 4 of
    /// `CLAUDE.md`'s list with no way out.
    ///
    /// Mutation: remove the clamp, or take the ratio before clamping, and this
    /// fails.
    #[test]
    fn zoom_stops_at_the_ends_of_its_range() {
        let mut app = viewport_editor();
        let cursor = Vec2::new(600.0, 300.0);

        for _ in 0..100 {
            scroll(&mut app, cursor, 1.0);
        }
        assert_eq!(camera(&mut app).1, ZOOM_MIN, "zooming in did not stop");

        for _ in 0..200 {
            scroll(&mut app, cursor, -1.0);
        }
        assert_eq!(camera(&mut app).1, ZOOM_MAX, "zooming out did not stop");
    }

    /// There is something in the world for the viewport to show.
    ///
    /// The first acceptance criterion needs something placed in the world, and
    /// panning needs something a person can watch move. This is the placeholder
    /// the first real content replaces.
    ///
    /// Mutation: stop spawning the sprite in `attach`, and this fails.
    #[test]
    fn there_is_something_in_the_world_for_the_viewport_to_show() {
        let mut app = viewport_editor();
        let sprites: Vec<Vec2> = app
            .world_mut()
            .query::<&Sprite>()
            .iter(app.world())
            .filter_map(|sprite| sprite.custom_size)
            .collect();
        assert_eq!(sprites, [Vec2::splat(PLACEHOLDER_SIZE)]);
    }
}
