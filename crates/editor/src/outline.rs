//! The outline that says which entities are selected.
//!
//! What it looks like, which camera draws it and why every selected entity
//! looks the same are settled in
//! [`docs/specs/ui.md` §5](../../../docs/specs/ui.md).

use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::{RenderLayers, VisibilitySystems};
use bevy::gizmos::AppGizmoBuilder;
use bevy::gizmos::config::{GizmoConfig, GizmoConfigGroup};
use bevy::prelude::*;

use crate::Selection;

/// The render layer the outline is drawn on.
///
/// `viewport::attach` adds it to the viewport's camera and nothing adds it to
/// the camera the panels are drawn to, which is what keeps the outline inside
/// the viewport region. The alternative was to rely on the panes being opaque
/// and the UI pass drawing after the 2D pass, which is a claim about draw order
/// nobody here has measured.
pub(crate) const SELECTION_LAYER: usize = 1;

/// The colour of the outline.
///
/// A constant rather than a Feathers token: its themes carry colour only and
/// have none for an editor overlay drawn in the world. `docs/specs/ui.md` §5
/// accepts that, and what to do if it disappears over real artwork.
const OUTLINE: Color = Color::srgb(1.0, 0.61, 0.16);

/// The gizmo group the outline is drawn in.
///
/// Its own group rather than the default one, so that the layer above applies
/// to this and not to whatever else draws a gizmo later.
#[derive(Default, Reflect, GizmoConfigGroup)]
#[reflect(Default)]
pub(crate) struct SelectionGizmos;

/// The outline that says what is selected.
///
/// Mutation: leave its row out of the editor's member table, and
/// `the_group_carries_the_members_the_table_names` fails.
pub struct SelectionOutlinePlugin;

impl Plugin for SelectionOutlinePlugin {
    fn build(&self, app: &mut App) {
        app.insert_gizmo_config(
            SelectionGizmos,
            GizmoConfig {
                render_layers: RenderLayers::layer(SELECTION_LAYER),
                ..default()
            },
        )
        .add_systems(
            PostUpdate,
            outline.after(VisibilitySystems::CalculateBounds),
        );
    }
}

/// Draw a rectangle on the bounds of everything that is selected.
///
/// **Drawn again every frame rather than kept.** Nothing is spawned, so there
/// is nothing to reconcile against a selection that changes on every click, and
/// the rectangle is computed from the entity's transform in the frame it is
/// drawn, so it is where the entity is at every zoom and after a pan without
/// anything being kept in step. It also means selecting writes nothing to what
/// is selected: this system has mutable access to no part of the world.
///
/// The bounds are the [`Aabb`] that `bevy_sprite`'s `calculate_bounds_2d`
/// already computes rather than a size read back out of `Sprite`, which is
/// `docs/specs/architecture.md` §5: the outline is then right for anything the
/// world draws, and right when a sprite is given a custom size or a
/// sub-rectangle, without this knowing how either works.
///
/// **A selected entity with no `Aabb` gets no outline and no diagnostic.**
/// Nothing is in that state: everything selectable is a sprite, and
/// `Selectable` requires `Pickable` rather than `Aabb`. If one ever arrives the
/// failure is a thing the user can see, which is row 4 of `CLAUDE.md`'s list,
/// rather than an error path written for a case that cannot happen.
///
/// **The schedule is not a detail.** Drawing in `PostUpdate` after
/// [`VisibilitySystems::CalculateBounds`] is what keeps the outline from
/// trailing what it is drawn on by a frame: both the bounds it is sized by and
/// the `GlobalTransform` it is placed by are written in `PostUpdate`, so a
/// system that drew before them would draw where the entity was last frame.
///
/// One ordering carries both, because Bevy configures that set
/// `.after(TransformSystems::Propagate)` itself. Naming `Propagate` here as
/// well reads as a second constraint and is not one: removing it changes
/// nothing and no test notices, measured. If the engine ever unpicks that,
/// `the_outline_follows_the_entity_in_the_frame_it_moves` is what says so.
///
/// Mutation: return before drawing, and `selecting_an_entity_outlines_it`
/// fails. Mutation: draw the half extents rather than twice them, and
/// `the_outline_is_on_the_entitys_bounds` fails. Mutation: draw for every
/// entity that has bounds rather than for the selection, and
/// `clearing_the_selection_takes_the_outline_away` fails. Mutation: drop the
/// `after(VisibilitySystems::CalculateBounds)` above, and
/// `the_outline_follows_the_entity_in_the_frame_it_moves` fails.
fn outline(
    selection: Res<Selection>,
    bounds: Query<(&GlobalTransform, &Aabb)>,
    mut gizmos: Gizmos<SelectionGizmos>,
) {
    for entity in selection.entities() {
        let Ok((at, aabb)) = bounds.get(*entity) else {
            continue;
        };
        let centre = at.transform_point(Vec3::from(aabb.center)).truncate();
        let size = aabb.half_extents.xy() * 2.0 * at.scale().truncate();
        let angle = at.rotation().to_euler(EulerRot::ZYX).0;
        gizmos.rect_2d(Isometry2d::new(centre, Rot2::radians(angle)), size, OUTLINE);
    }
}

#[cfg(test)]
mod tests {
    use super::{SELECTION_LAYER, SelectionGizmos};
    use crate::pointer::{click_at, in_window};
    use crate::viewport::ViewportCamera;
    use crate::{Selectable, editor, headless};
    use bevy::camera::visibility::RenderLayers;
    use bevy::gizmos::config::GizmoConfigStore;
    use bevy::gizmos::{GizmoAsset, GizmoHandles};
    use bevy::picking::pointer::{PointerAction, PointerButton};
    use bevy::prelude::*;
    use bevy::sprite::Anchor;
    use core::any::TypeId;
    use core::f32::consts::FRAC_PI_4;

    /// The editor, run until a test can look at the world.
    ///
    /// The same one frame `selection::tests` settles for, and for the same
    /// reason: before it there are no regions, so there is nothing to click.
    fn outline_editor() -> App {
        let mut app = editor(headless());
        app.update();
        app
    }

    /// What the outline covers, or `None` when nothing was drawn.
    ///
    /// `update_gizmo_meshes` puts the group's handle back to `None` in a frame
    /// where nothing reached its storage, so "nothing is drawn" is a state that
    /// can be read rather than an absence that has to be inferred.
    ///
    /// `rect_2d` emits a line loop, which arrives as a repeated vertex and a
    /// `NaN` separator, so the extent of the finite positions is what is
    /// compared rather than the list as it comes.
    fn outlined(app: &App) -> Option<Rect> {
        let handle = app
            .world()
            .resource::<GizmoHandles>()
            .handles()
            .get(&TypeId::of::<SelectionGizmos>())
            .cloned()
            .flatten()?;
        let drawn = app
            .world()
            .resource::<Assets<GizmoAsset>>()
            .get(&handle)
            .expect("the group's handle names an asset")
            .strip_positions
            .iter()
            .filter(|position| position.is_finite())
            .map(|position| position.truncate())
            .collect::<Vec<Vec2>>();
        assert!(
            !drawn.is_empty(),
            "a handle exists with nothing drawn in it"
        );
        Some(Rect::from_corners(
            drawn
                .iter()
                .copied()
                .reduce(Vec2::min)
                .expect("something was drawn"),
            drawn
                .iter()
                .copied()
                .reduce(Vec2::max)
                .expect("something was drawn"),
        ))
    }

    /// Where an entity is, worked out from its sprite rather than from its
    /// `Aabb`.
    ///
    /// Deliberately not the way `outline` works it out. A test that computed
    /// the rectangle from the same components in the same order would agree
    /// with the code whatever either of them meant.
    fn rect_of(app: &App, entity: Entity) -> Rect {
        let at = app
            .world()
            .entity(entity)
            .get::<Transform>()
            .expect("a placeholder has a transform")
            .translation
            .truncate();
        let size = app
            .world()
            .entity(entity)
            .get::<Sprite>()
            .expect("a placeholder has a sprite")
            .custom_size
            .expect("a placeholder's sprite has a size of its own");
        Rect::from_center_size(at, size)
    }

    /// Two rectangles are the same to within a rotation's arithmetic.
    ///
    /// A rectangle turned by an eighth of a turn has irrational corners, so
    /// comparing one for equality is comparing two roundings.
    fn assert_close(left: Option<Rect>, right: Rect) {
        let left = left.expect("nothing was drawn to compare");
        let apart = (left.min - right.min)
            .abs()
            .max((left.max - right.max).abs())
            .max_element();
        assert!(apart < 1e-3, "{left:?} is not {right:?}");
    }

    /// Select the middle placeholder, and say which entity it is.
    fn select_the_middle(app: &mut App) -> Entity {
        click_at(app, in_window(Vec2::ZERO), PointerButton::Primary);
        let selection = app.world().resource::<crate::Selection>().entities();
        assert_eq!(selection.len(), 1, "the click selected nothing");
        selection[0]
    }

    /// Selecting an entity outlines it.
    ///
    /// The first half of the first acceptance criterion: before the click the
    /// viewport draws nothing of the sort, and after it there is something.
    ///
    /// Mutation: return from `outline` before drawing, and this fails.
    #[test]
    fn selecting_an_entity_outlines_it() {
        let mut app = outline_editor();
        assert_eq!(
            outlined(&app),
            None,
            "something was drawn with no selection"
        );

        select_the_middle(&mut app);

        assert!(
            outlined(&app).is_some(),
            "the selected entity has no outline"
        );
    }

    /// The outline is on the entity's bounds.
    ///
    /// What "it is where the entity is" means, compared against a rectangle the
    /// test works out from the entity's own sprite.
    ///
    /// Mutation: draw the half extents rather than twice them in `outline`, and
    /// this fails.
    #[test]
    fn the_outline_is_on_the_entitys_bounds() {
        let mut app = outline_editor();
        let middle = select_the_middle(&mut app);

        assert_eq!(outlined(&app), Some(rect_of(&app, middle)));
    }

    /// Clearing the selection takes the outline away.
    ///
    /// The other half of the first criterion. The position is inside the
    /// viewport region and away from every placeholder.
    ///
    /// Mutation: draw for every entity that has bounds rather than for what is
    /// in `Selection`, and this fails.
    #[test]
    fn clearing_the_selection_takes_the_outline_away() {
        let mut app = outline_editor();
        select_the_middle(&mut app);
        assert!(outlined(&app).is_some(), "nothing was drawn to take away");

        click_at(
            &mut app,
            in_window(Vec2::new(0.0, -180.0)),
            PointerButton::Primary,
        );

        assert_eq!(outlined(&app), None, "the outline outlived the selection");
    }

    /// The outline follows the entity in the frame it moves.
    ///
    /// Not a frame later. `GlobalTransform` is written by
    /// `TransformSystems::Propagate`, which runs in the same `PostUpdate`, so a
    /// system that drew before it would read where the entity was last frame,
    /// and an outline that trails what it is drawn on is exactly what a person
    /// dragging something sees.
    ///
    /// Mutation: drop the `after(VisibilitySystems::CalculateBounds)` in the
    /// plugin, and this fails.
    #[test]
    fn the_outline_follows_the_entity_in_the_frame_it_moves() {
        let mut app = outline_editor();
        let middle = select_the_middle(&mut app);

        app.world_mut()
            .entity_mut(middle)
            .get_mut::<Transform>()
            .expect("a placeholder has a transform")
            .translation += Vec3::new(120.0, 45.0, 0.0);
        app.update();

        assert_eq!(outlined(&app), Some(rect_of(&app, middle)));
    }

    /// The outline is on the bounds of an entity that is turned, scaled and
    /// anchored away from its centre.
    ///
    /// Three of the four terms in the placement are otherwise unguarded, and
    /// were: everything in the world is an unrotated, unscaled, centre-anchored
    /// sprite, so removing the rotation, the scale or the `Aabb`'s own centre
    /// from `outline` left all eight of the tests around this green, measured.
    /// This is the same shape as RK-005 in the knowledge bank, where a fixture
    /// that cannot reach a path makes the tests around it look like cover.
    ///
    /// The expected rectangle is built by turning the sprite's four corners,
    /// which is not how `outline` builds it: it places one point and hands the
    /// size and the angle to `rect_2d`.
    ///
    /// Mutation: drop `at.scale()`, or use `Rot2::IDENTITY`, or place the
    /// rectangle on `at.translation()` rather than on the `Aabb`'s centre, and
    /// this fails. Each on its own.
    #[test]
    fn the_outline_is_on_the_bounds_of_a_turned_and_scaled_entity() {
        let mut app = outline_editor();
        let size = Vec2::new(48.0, 24.0);
        let turned = Transform::from_xyz(0.0, 0.0, 1.0)
            .with_rotation(Quat::from_rotation_z(FRAC_PI_4))
            .with_scale(Vec3::new(2.0, 3.0, 1.0));
        let awkward = app
            .world_mut()
            .spawn((
                Sprite::from_color(Color::WHITE, size),
                Anchor::TOP_LEFT,
                turned,
                Selectable,
            ))
            .id();
        app.update();

        // The sprite sits where its anchor puts it, so the middle of it is not
        // where the entity is. Clicking the middle is what reaches it.
        let middle_of_it = turned.transform_point((-Anchor::TOP_LEFT.as_vec() * size).extend(0.0));
        click_at(
            &mut app,
            in_window(middle_of_it.truncate()),
            PointerButton::Primary,
        );
        assert_eq!(
            app.world().resource::<crate::Selection>().entities(),
            [awkward],
            "the click did not reach the sprite under test"
        );

        let half = size * 0.5;
        let corners = [
            Vec2::new(-half.x, -half.y),
            Vec2::new(half.x, -half.y),
            Vec2::new(half.x, half.y),
            Vec2::new(-half.x, half.y),
        ]
        .map(|corner| {
            turned
                .transform_point((corner - Anchor::TOP_LEFT.as_vec() * size).extend(0.0))
                .truncate()
        });
        let expected = Rect::from_corners(
            corners.into_iter().reduce(Vec2::min).expect("four corners"),
            corners.into_iter().reduce(Vec2::max).expect("four corners"),
        );

        assert_close(outlined(&app), expected);
    }

    /// Zooming does not change what the outline covers.
    ///
    /// The outline is a rectangle in the world, so how much of the world fits
    /// in the region is none of its business. What this rules out is an outline
    /// sized in pixels, which would hug the sprite at one zoom and float around
    /// it at every other.
    ///
    /// The projection is set directly rather than scrolled: what the wheel is
    /// bound to is `viewport`'s, and
    /// `viewport::tests::a_zoom_holds_the_world_point_under_the_cursor_still`
    /// holds it.
    ///
    /// Mutation: multiply the size by the viewport camera's projection scale in
    /// `outline`, and this fails while
    /// `panning_does_not_move_the_outline_off_the_entity` passes.
    #[test]
    fn zooming_does_not_change_what_the_outline_covers() {
        let mut app = outline_editor();
        let middle = select_the_middle(&mut app);
        let before = outlined(&app);

        let mut cameras = app
            .world_mut()
            .query_filtered::<&mut Projection, With<ViewportCamera>>();
        let mut projection = cameras
            .single_mut(app.world_mut())
            .expect("there is one viewport camera");
        let Projection::Orthographic(orthographic) = &mut *projection else {
            panic!("the viewport camera projects orthographically");
        };
        orthographic.scale = 4.0;
        app.update();

        assert_eq!(outlined(&app), before, "zooming resized the outline");
        assert_eq!(outlined(&app), Some(rect_of(&app, middle)));
    }

    /// Panning does not move the outline off the entity.
    ///
    /// The rest of the second criterion: the outline is in the world rather
    /// than on the screen, so moving the camera leaves it where the entity is.
    ///
    /// Mutation: draw at positions taken relative to the camera in `outline`,
    /// and this fails.
    #[test]
    fn panning_does_not_move_the_outline_off_the_entity() {
        let mut app = outline_editor();
        let middle = select_the_middle(&mut app);
        let before = outlined(&app);

        // The middle button pans, which `docs/specs/ui.md` §4 decides.
        let from = in_window(Vec2::ZERO);
        crate::pointer::write_input(&mut app, from, PointerAction::Press(PointerButton::Middle));
        crate::pointer::write_input(
            &mut app,
            from + Vec2::new(90.0, 40.0),
            PointerAction::Move {
                delta: Vec2::new(90.0, 40.0),
            },
        );
        crate::pointer::write_input(
            &mut app,
            from + Vec2::new(90.0, 40.0),
            PointerAction::Release(PointerButton::Middle),
        );
        app.update();
        app.update();

        let camera = app
            .world_mut()
            .query_filtered::<&Transform, With<ViewportCamera>>()
            .single(app.world())
            .expect("there is one viewport camera")
            .translation;
        assert_ne!(camera, Vec3::ZERO, "the drag did not pan the camera");
        assert_eq!(outlined(&app), before, "panning moved the outline");
        assert_eq!(outlined(&app), Some(rect_of(&app, middle)));
    }

    /// Outlining a selection leaves the world where it was.
    ///
    /// The third acceptance criterion, asserted over a set the drawing does not
    /// write to: every `GlobalTransform` in the world, before and after. It is
    /// the shape `selection::tests::selecting_changes_the_selection_and_leaves_the_world_where_it_was`
    /// holds for the click.
    ///
    /// Mutation: write anything to the entity that is selected, from `outline`
    /// or from the plugin, and this fails.
    #[test]
    fn outlining_a_selection_leaves_the_world_where_it_was() {
        let mut app = outline_editor();
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
        select_the_middle(&mut app);
        assert!(outlined(&app).is_some(), "nothing was drawn to hold");

        let before = world_of(&mut app);
        app.update();
        let after = world_of(&mut app);

        assert!(!before.is_empty(), "there is nothing in the world to hold");
        assert_eq!(before, after, "drawing the outline moved the world");
    }

    /// The outline is drawn only for the viewport's camera.
    ///
    /// The fourth acceptance criterion. Both halves are needed: the group is on
    /// a layer, and the viewport's camera is the only one that has it. Either
    /// one alone puts the outline in front of the panels.
    ///
    /// Mutation: drop the `RenderLayers` from the viewport's camera in
    /// `viewport::attach`, and this fails. Mutation: drop `render_layers` from
    /// the group's config, and this fails too, because the group is then on the
    /// layer every other camera is on.
    #[test]
    fn the_outline_is_drawn_only_for_the_viewports_camera() {
        let app = outline_editor();
        let drawn_on = app
            .world()
            .resource::<GizmoConfigStore>()
            .config::<SelectionGizmos>()
            .0
            .render_layers
            .clone();
        assert!(
            drawn_on.intersects(&RenderLayers::layer(SELECTION_LAYER)),
            "the group is not on the layer the viewport camera adds"
        );

        let mut app = app;
        let cameras: Vec<(bool, RenderLayers)> = app
            .world_mut()
            .query::<(&Camera, Option<&ViewportCamera>, Option<&RenderLayers>)>()
            .iter(app.world())
            .map(|(_, viewport, layers)| (viewport.is_some(), layers.cloned().unwrap_or_default()))
            .collect();
        assert_eq!(cameras.len(), 2, "the editor has the two cameras it had");
        for (is_viewport, layers) in cameras {
            assert_eq!(
                layers.intersects(&drawn_on),
                is_viewport,
                "the wrong camera draws the outline"
            );
        }
    }
}
