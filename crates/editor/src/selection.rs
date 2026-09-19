//! What the editor is currently acting on, and the gestures that set it: a
//! click, and a box dragged over empty space.
//!
//! The inspector shows it, transform editing moves it, gizmos draw on it and
//! delete acts on it, so this is one answer rather than one per panel that
//! wants to know. The button it is bound to is
//! [`docs/specs/ui.md` §4](../../../docs/specs/ui.md).

use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::RenderLayers;
use bevy::ecs::lifecycle::Remove;
use bevy::gizmos::AppGizmoBuilder;
use bevy::gizmos::config::{GizmoConfig, GizmoConfigGroup};
use bevy::picking::events::{Click, DragEnd, DragStart, Pointer, Press};
use bevy::picking::hover::HoverMap;
use bevy::picking::pointer::{PointerButton, PointerId, PointerLocation};
use bevy::picking::{Pickable, PickingSystems};
use bevy::prelude::*;
use bevy::ui::widget::ViewportNode;

use crate::Region;
use crate::outline::{OUTLINE, SELECTION_LAYER};
use crate::viewport::ViewportCamera;

/// What the editor is currently acting on.
///
/// A list rather than one entity, and ordered rather than a set. Ordered
/// because the inspector will want the last thing chosen, which a set cannot
/// answer; a list because adding to the selection is then an addition to this
/// module rather than a rewrite of everything that reads it.
///
/// What the order means, now that there is more than one way in:
///
/// * something added goes on the end, so the last element is the most recently
///   chosen. That is the one Unity calls active.
/// * removing one leaves the rest in the order they were chosen in.
/// * an entity appears at most once.
/// * several added by one gesture go on the end together, in the order the
///   world iterates them. Nothing in a box drag ranks what it covers, so only
///   the boundary between gestures is meaningful, and a test that reads this
///   after one compares it sorted.
///
/// Those are conditions on this type, not a description of who writes it
/// today. `select` and `finish` below are the writers as this stands, and
/// anything that joins them either keeps the conditions or breaks the
/// inspector that reads the last element.
///
/// **The field is private and there is no setter.** That is what makes "one
/// place decides what is selected" a thing the compiler holds rather than a
/// rule somebody has to remember: a writer has to be inside this module, where
/// the conditions above are written down.
#[derive(Resource, Default)]
pub struct Selection(Vec<Entity>);

impl Selection {
    /// What is selected, in the order it was chosen.
    pub fn entities(&self) -> &[Entity] {
        &self.0
    }
}

/// What the gesture in progress means, fixed when the button went down.
///
/// `Pointer<Click>` guarantees that the press and the release shared a target,
/// and the target here is the viewport's UI node rather than anything in the
/// world: the world is reached through a second pointer, so the engine's
/// guarantee says nothing about which entity ends up selected. Without this,
/// pressing one entity and letting go over another selected the second, and
/// pressing one and letting go over empty space cleared the selection that was
/// already there. Both measured.
///
/// The modifier is here rather than read at the release for the same reason
/// the entity is: `docs/specs/ui.md` §4 fixes what a gesture means when it
/// begins and commits it when it ends. Reading the keyboard at the release
/// instead would turn letting the key go a moment early into a selection of
/// six replaced by one.
/// **The box drag lives here too**, because it is the same gesture rather than
/// a second one: [`remember`] overwrites the whole of this at every press, so a
/// box whose `DragEnd` never arrives, which is what a window losing focus
/// mid-drag produces, is cleared by the next press rather than drawn for ever.
#[derive(Resource, Default)]
struct Pressed {
    /// What the viewport's pointer was over.
    ///
    /// `None` means empty space, which is a real answer rather than a missing
    /// one: pressing and releasing over nothing is what clears the selection.
    over: Option<Entity>,
    /// Whether the modifier that adds and removes was held.
    additive: bool,
    /// Where the press was, in world units.
    ///
    /// `None` when the viewport or its camera could not be read, which is the
    /// same state as "this gesture cannot become a box".
    ///
    /// Recorded here rather than at [`begin`], **which is too late**:
    /// `Pointer<DragStart>` fires on the first move after the press, so by the
    /// time it arrives the viewport's pointer is already at the moved-to
    /// position. Measured: a press at `(-300, -180)` dragged to `(300, 180)`
    /// reads as `(300, 180)` in a `DragStart` observer.
    at: Option<Vec2>,
    /// Where a box drag began, once the press turned into one.
    band: Option<Vec2>,
}

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
/// **It declares that the keyboard is read before a press is dispatched.**
/// `remember` below reads `ButtonInput<KeyCode>` from inside an observer that
/// `bevy_picking` triggers, and the engine orders the two against nothing:
/// `bevy_picking`'s sets are chained among themselves and say nothing about
/// `InputSystems`, checked in 0.19.1. The order that holds today is the one
/// this wants, so the line changes no behaviour; what it buys is that a
/// schedule saying otherwise fails to build and names the cycle, rather than
/// reading the modifier as up and turning somebody's add into a replace. That
/// only bites when the key and the button arrive in one frame's batch, which
/// is what a long frame on a large level produces.
///
/// Mutation: leave its row out of the editor's member table, and
/// `the_group_carries_the_members_the_table_names` fails. Mutation: drop the
/// `configure_sets` call, and
/// `a_schedule_that_reads_the_keyboard_after_the_press_does_not_build` fails.
pub struct SelectionPlugin;

impl Plugin for SelectionPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            PreUpdate,
            PickingSystems::Hover.after(bevy::input::InputSystems),
        )
        .init_resource::<Selection>()
        .init_resource::<Pressed>()
        .insert_gizmo_config(
            BandGizmos,
            GizmoConfig {
                render_layers: RenderLayers::layer(SELECTION_LAYER),
                ..default()
            },
        )
        .add_systems(Update, draw_band)
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
        .observe(select)
        .observe(begin)
        .observe(finish);
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
/// **With the modifier held it adds and removes instead of replacing**, which
/// is the other half of `docs/specs/ui.md` §4's table. A click that lands on
/// nothing then leaves the selection alone: adding nothing to a selection is
/// not the same gesture as abandoning it, and the user who misses with the
/// seventh click keeps the six.
///
/// **A release that ended a box drag also arrives here, and nothing suppresses
/// it.** `bevy_picking` triggers `Click` before `DragEnd` for one release, and
/// a box begins on empty space, so all this can do during one is clear the
/// selection, which [`finish`] then overwrites in the same frame; with the
/// modifier held it does not even do that. A flag saying "this was a box" would
/// be code no mutation could make a test fail on. What is real is the order, so
/// that is what is held, by
/// `a_release_that_ended_a_band_clicks_before_it_ends_the_drag`.
///
/// Every branch here has a mutation, and the test it fails:
///
/// * return before writing the selection: `clicking_an_entity_selects_it`
/// * clear on a hit and select on a miss: `clicking_empty_space_clears_the_selection`
/// * take the furthest rather than the nearest:
///   `the_nearer_of_two_under_the_pointer_is_the_one_selected`
/// * drop the comparison with [`Pressed::over`]:
///   `letting_go_somewhere_else_takes_the_press_back`
/// * ignore [`Pressed::additive`]: `a_modifier_click_adds_to_the_selection`
/// * push what is already selected rather than removing it:
///   `a_modifier_click_on_a_selected_entity_removes_it_and_keeps_the_rest`
/// * clear on a miss whatever the modifier says:
///   `a_modifier_click_on_empty_space_keeps_the_selection`
/// * `swap_remove` in place of `remove`:
///   `the_selection_keeps_the_order_things_were_chosen_in`
fn select(
    click: On<Pointer<Click>>,
    viewport: Query<&PointerId, With<ViewportNode>>,
    selectable: Query<(), With<Selectable>>,
    hover: Res<HoverMap>,
    pressed: Res<Pressed>,
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
    if hit != pressed.over {
        return;
    }

    let Some(entity) = hit else {
        if !pressed.additive {
            selection.0.clear();
        }
        return;
    };

    if !pressed.additive {
        selection.0.clear();
        selection.0.push(entity);
        return;
    }

    // `remove` rather than `swap_remove`: the order is a claim `Selection`
    // makes, and the cheaper call is the plausible tidying that breaks it.
    match selection.0.iter().position(|selected| *selected == entity) {
        Some(index) => {
            selection.0.remove(index);
        }
        None => selection.0.push(entity),
    }
}

/// Remember what the gesture means, so that the release can act on it.
///
/// Mutation: record `None` for the entity regardless, and
/// `clicking_an_entity_selects_it` fails, because every press would then look
/// like a press on empty space. Mutation: read the keyboard in [`select`]
/// instead of here, and `the_modifier_is_read_when_the_button_goes_down` fails.
/// Mutation: record `None` for [`Pressed::at`], and
/// `a_band_over_two_entities_selects_both` fails, because no press could then
/// anchor a box.
fn remember(
    press: On<Pointer<Press>>,
    viewport: Query<(&PointerId, &PointerLocation), With<ViewportNode>>,
    selectable: Query<(), With<Selectable>>,
    cameras: Query<(&Camera, &GlobalTransform), With<ViewportCamera>>,
    hover: Res<HoverMap>,
    keys: Res<ButtonInput<KeyCode>>,
    mut pressed: ResMut<Pressed>,
) {
    if press.event().button != PointerButton::Primary {
        return;
    }
    let Ok((pointer, location)) = viewport.single() else {
        return;
    };
    *pressed = Pressed {
        over: under(&hover, pointer, &selectable),
        additive: additive(&keys),
        at: pointer_world(location, &cameras),
        band: None,
    };
}

/// Take the press for a box drag, if it began on empty space.
///
/// `Pressed::over` being `Some` is the whole of "a drag that begins on an
/// entity is not a box": that gesture belongs to moving things, and taking it
/// here would mean taking it back. It also leaves such a drag reaching
/// [`select`], so pressing an entity, wandering off and coming back still
/// selects it, which is what `docs/specs/ui.md` §4 asks for.
///
/// Mutation: drop the `pressed.over.is_some()` check, and
/// `a_drag_that_begins_on_an_entity_is_not_a_band` fails. Mutation: drop the
/// button check, and `a_drag_with_any_other_button_does_not_band` fails.
fn begin(start: On<Pointer<DragStart>>, mut pressed: ResMut<Pressed>) {
    if start.event().button != PointerButton::Primary {
        return;
    }
    if pressed.over.is_some() {
        return;
    }
    pressed.band = pressed.at;
}

/// Select what the box covers, when the button is let go.
///
/// The rectangle is built with [`Rect::from_corners`], which orders the corners
/// itself, so a box dragged up and to the left is the same box as one dragged
/// down and to the right.
///
/// **Touched, not covered**, which `docs/specs/ui.md` §4 decided.
/// `Rect::intersect` collapses a non-overlap to a zero-sized rectangle and
/// `Rect::is_empty` is `min.x >= max.x || min.y >= max.y`, so two of this
/// issue's conditions fall out of that pair rather than out of a branch written
/// for them: a box with no area touches nothing, and an entity sharing an edge
/// with the box and no area with it is not selected.
///
/// Mutation: return before writing the selection, and
/// `a_band_over_two_entities_selects_both` fails. Mutation: `Rect::contains` in
/// place of the intersection, and
/// `a_band_selects_an_entity_it_only_half_covers` fails. Mutation:
/// `Rect { min: anchor, max: now }` in place of `from_corners`, and
/// `a_band_dragged_up_and_to_the_left_selects_the_same_things` fails.
/// Mutation: take every entity rather than the ones the box touches, and
/// `a_band_that_ends_where_it_started_selects_nothing` fails. Mutation: ignore
/// [`Pressed::additive`], and
/// `a_band_with_the_modifier_adds_to_what_was_already_selected` fails.
/// Mutation: drop the `clear`, and
/// `a_band_replaces_the_selection_when_the_modifier_is_not_held` fails.
/// Mutation: read [`Pressed::band`] instead of taking it, and
/// `the_band_is_gone_once_the_button_is_let_go` fails.
fn finish(
    end: On<Pointer<DragEnd>>,
    viewport: Query<&PointerLocation, With<ViewportNode>>,
    cameras: Query<(&Camera, &GlobalTransform), With<ViewportCamera>>,
    bounds: Query<(Entity, &GlobalTransform, &Aabb), With<Selectable>>,
    mut pressed: ResMut<Pressed>,
    mut selection: ResMut<Selection>,
) {
    if end.event().button != PointerButton::Primary {
        return;
    }
    let Some(anchor) = pressed.band.take() else {
        return;
    };
    let Ok(location) = viewport.single() else {
        return;
    };
    let Some(now) = pointer_world(location, &cameras) else {
        return;
    };
    let band = Rect::from_corners(anchor, now);

    if !pressed.additive {
        selection.0.clear();
    }
    for (entity, at, aabb) in &bounds {
        if !band.intersect(world_bounds(at, aabb)).is_empty() && !selection.0.contains(&entity) {
            selection.0.push(entity);
        }
    }
}

/// Whether the modifier that adds to and removes from the selection is held.
///
/// Which keys, and why both of them on every platform, is `docs/specs/ui.md`
/// §4. What that section cannot say is the spelling: Bevy calls the Command
/// key `KeyCode::SuperLeft` and `KeyCode::SuperRight`.
///
/// Mutation: drop either `Super` key, and `either_control_or_super_is_the_modifier`
/// fails, which is what stands in for the macOS binding here.
fn additive(keys: &ButtonInput<KeyCode>) -> bool {
    keys.any_pressed([
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ])
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

/// The axis-aligned rectangle an entity occupies in the world.
///
/// The [`Aabb`] that `bevy_sprite`'s `calculate_bounds_2d` already computes,
/// put where the entity is, which is `docs/specs/architecture.md` §5 and the
/// same bounds `outline::outline` draws on. **All four corners are
/// transformed**, rather than the centre alone with the half extents left as
/// they are, so a turned or scaled entity is tested against where it actually
/// is; RK-006 in the knowledge bank is why that needs saying, and
/// `a_band_selects_a_turned_and_scaled_entity_it_touches` is why deleting it
/// fails.
///
/// The outline draws those bounds turned rather than axis-aligned, so for a
/// turned sprite the box here is very slightly generous. That is the direction
/// `docs/specs/ui.md` §4 already leans.
fn world_bounds(at: &GlobalTransform, aabb: &Aabb) -> Rect {
    let centre = Vec3::from(aabb.center);
    let half = Vec3::from(aabb.half_extents);
    let corners = [
        Vec3::new(-half.x, -half.y, 0.0),
        Vec3::new(half.x, -half.y, 0.0),
        Vec3::new(-half.x, half.y, 0.0),
        Vec3::new(half.x, half.y, 0.0),
    ]
    .map(|corner| at.transform_point(centre + corner).truncate());

    Rect::from_corners(
        corners.iter().copied().fold(corners[0], Vec2::min),
        corners.iter().copied().fold(corners[0], Vec2::max),
    )
}

/// Where the viewport's pointer is, in world units, clamped to what the
/// viewport shows.
///
/// **The clamp is a decision, not tidying.** `viewport_picking` keeps
/// forwarding the pointer while the region is being dragged, wherever it goes,
/// and the position it forwards is extrapolated past the region's edge:
/// releasing over the right-hand pane reads as world `(540, 184)` in a viewport
/// showing `740 x 512` centred on the origin, measured. Without this an entity
/// the user cannot see joins the selection, and what happens to it next is a
/// delete or a transform. `docs/specs/ui.md` §4 records it.
///
/// Both sides are the engine's own logical pixels, `logical_viewport_rect`
/// against a `PointerLocation` that `viewport_picking` wrote in the same units,
/// so nothing here multiplies by a scale factor and RK-005 has nothing to bite.
///
/// Mutation: drop the clamp, and
/// `a_band_stops_at_the_edge_of_what_the_viewport_shows` fails.
fn pointer_world(
    location: &PointerLocation,
    cameras: &Query<(&Camera, &GlobalTransform), With<ViewportCamera>>,
) -> Option<Vec2> {
    let location = location.location.as_ref()?;
    let (camera, at) = cameras.single().ok()?;
    let rect = camera.logical_viewport_rect()?;
    camera
        .viewport_to_world_2d(at, location.position.clamp(rect.min, rect.max))
        .ok()
}

/// The gizmo group the box drag is drawn in.
///
/// Its own group rather than `outline::SelectionGizmos`, on the same layer:
/// the outline's tests read whether that group has a handle at all to mean
/// "nothing is drawn", and a box sharing it would make that read something
/// else.
#[derive(Default, Reflect, GizmoConfigGroup)]
#[reflect(Default)]
pub(crate) struct BandGizmos;

/// Draw the box while the drag is happening.
///
/// In the outline's colour, for the reason `docs/specs/ui.md` §5 gives.
///
/// **It cannot see the world.** A resource, the viewport's pointer and the
/// camera, and nothing else, so the work it does per frame cannot grow with the
/// level; the one pass over every `Selectable` is [`finish`], once, at the
/// release. That is what keeps a box drag from being the gesture that stops the
/// viewport answering, and it is row 1 of `CLAUDE.md`'s list rather than row 2:
/// a query added here would be visible in the signature.
///
/// `Update` rather than `PostUpdate`: everything it reads is written in
/// `PreUpdate`, by the observers above and by `viewport_picking`, and unlike
/// the outline it needs no bounds, so it has nothing to order after.
///
/// Mutation: return before drawing, and
/// `the_band_is_drawn_between_the_corners_it_was_dragged_between` fails.
fn draw_band(
    pressed: Res<Pressed>,
    viewport: Query<&PointerLocation, With<ViewportNode>>,
    cameras: Query<(&Camera, &GlobalTransform), With<ViewportCamera>>,
    mut gizmos: Gizmos<BandGizmos>,
) {
    let Some(anchor) = pressed.band else {
        return;
    };
    let Ok(location) = viewport.single() else {
        return;
    };
    let Some(now) = pointer_world(location, &cameras) else {
        return;
    };

    let band = Rect::from_corners(anchor, now);
    gizmos.rect_2d(
        Isometry2d::from_translation(band.center()),
        band.size(),
        OUTLINE,
    );
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
/// **One entity, not the selection.** With one thing selected, `retain` and
/// `clear` are the same program, so what keeps them apart is
/// `despawning_one_of_several_leaves_the_rest_selected` and nothing else.
///
/// Mutation: drop this observer, and `a_despawned_entity_does_not_stay_selected`
/// fails. Mutation: listen on `Despawn` instead, which is the plausible
/// narrowing because the prose around this talks about despawning, and
/// `something_that_stops_being_selectable_stops_being_selected` fails while the
/// despawn test stays green. Mutation: `clear` in place of `retain`, and
/// `despawning_one_of_several_leaves_the_rest_selected` fails.
fn forget_what_is_gone(remove: On<Remove, Selectable>, mut selection: ResMut<Selection>) {
    selection.0.retain(|entity| *entity != remove.entity);
}

#[cfg(test)]
mod tests {
    use super::{BandGizmos, Selectable, Selection};
    use crate::drawn::covered_by;
    use crate::pointer::{click_at, hold_key, in_window, release_key, write_input};
    use crate::viewport::PLACEHOLDERS;
    use crate::{Region, editor, headless};
    use bevy::picking::events::{Click, DragEnd, Pointer};
    use bevy::picking::pointer::{PointerAction, PointerButton};
    use bevy::picking::{Pickable, PickingSystems};
    use bevy::prelude::*;
    use bevy::sprite::Anchor;
    use core::f32::consts::FRAC_PI_4;

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

    /// Where the left placeholder is, in world units.
    ///
    /// These are positions in the world, and every use puts them through
    /// `in_window`. They repeat what `PLACEHOLDERS` holds, for the reason
    /// `in_window` gives about the layout: a test that reads the position out
    /// of the table it is checking agrees with that table whatever it says.
    const LEFT: Vec2 = Vec2::new(-200.0, 0.0);
    /// Where the middle placeholder is, in world units.
    const MIDDLE: Vec2 = Vec2::ZERO;
    /// Where the right placeholder is, in world units.
    const RIGHT: Vec2 = Vec2::new(200.0, 0.0);

    /// Somewhere in the viewport with no placeholder under it, in world units.
    const EMPTY: Vec2 = Vec2::new(0.0, -180.0);

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
        press_and_hold(app, press, release);
        write_input(app, release, PointerAction::Release(PointerButton::Primary));
        app.update();
        app.update();
    }

    /// Press at one place, move to another, and keep the button down.
    ///
    /// The half of a box drag a test can look at while it is happening. Two
    /// moves and a press rather than one of each, because `bevy_picking` turns
    /// a press into a drag on the first move after it.
    fn press_and_hold(app: &mut App, press: Vec2, to: Vec2) {
        for (position, action) in [
            (press, PointerAction::Move { delta: Vec2::ONE }),
            (press, PointerAction::Press(PointerButton::Primary)),
            (to, PointerAction::Move { delta: Vec2::ONE }),
        ] {
            write_input(app, position, action);
            app.update();
            app.update();
        }
    }

    /// What is selected, in an order a test can compare.
    ///
    /// `Selection` says the entities one gesture added have no order among
    /// themselves, so a test over a box drag asks what is in it and not what
    /// order the world iterated in.
    fn sorted(app: &App) -> Vec<Entity> {
        let mut entities = selected(app);
        entities.sort();
        entities
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

    /// Click with the modifier held for the whole gesture.
    fn modifier_click_at(app: &mut App, position: Vec2) {
        hold_key(app, KeyCode::ControlLeft);
        click_at(app, position, PointerButton::Primary);
        release_key(app, KeyCode::ControlLeft);
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
    /// Mutation: drop the comparison with `Pressed::over` in `select`, and this
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
    /// Mutation: drop the comparison with `Pressed::over` in `select`, and this
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

    /// A modifier click adds to the selection.
    ///
    /// What was already chosen is still chosen afterwards, which is the whole
    /// of what the modifier is for.
    ///
    /// Mutation: ignore `Pressed::additive` in `select`, so that every click
    /// clears first, and this fails.
    #[test]
    fn a_modifier_click_adds_to_the_selection() {
        let mut app = selection_editor();
        let left = placeholder(&mut app, 0);
        let middle = placeholder(&mut app, 1);

        click_at(&mut app, in_window(LEFT), PointerButton::Primary);
        modifier_click_at(&mut app, in_window(MIDDLE));

        assert_eq!(selected(&app), [left, middle]);
    }

    /// A modifier click on something already selected removes it, and keeps the
    /// rest.
    ///
    /// Mutation: push in `select` without asking whether the entity is already
    /// in the selection, and this fails with the middle one in twice.
    #[test]
    fn a_modifier_click_on_a_selected_entity_removes_it_and_keeps_the_rest() {
        let mut app = selection_editor();
        let left = placeholder(&mut app, 0);

        click_at(&mut app, in_window(LEFT), PointerButton::Primary);
        modifier_click_at(&mut app, in_window(MIDDLE));
        assert_eq!(selected(&app).len(), 2, "there was nothing to take away");

        modifier_click_at(&mut app, in_window(MIDDLE));

        assert_eq!(selected(&app), [left]);
    }

    /// A modifier click on empty space keeps the selection.
    ///
    /// The one that costs a user their work: they have picked out several
    /// things, they miss with the next click, and without this everything they
    /// chose is gone. Adding nothing to a selection is not abandoning it.
    ///
    /// Mutation: clear on a miss whatever `Pressed::additive` says, and this
    /// fails.
    #[test]
    fn a_modifier_click_on_empty_space_keeps_the_selection() {
        let mut app = selection_editor();

        click_at(&mut app, in_window(LEFT), PointerButton::Primary);
        modifier_click_at(&mut app, in_window(MIDDLE));
        let chosen = selected(&app);
        assert_eq!(chosen.len(), 2, "nothing was selected to protect");

        modifier_click_at(&mut app, in_window(EMPTY));

        assert_eq!(
            selected(&app),
            chosen,
            "a missed click took the rest with it"
        );
    }

    /// A plain click collapses a selection of several to the one clicked.
    ///
    /// A click with no modifier still replaces, now that there is something
    /// wider than one entity for it to replace. Unity does the same, and the
    /// gesture that will want to keep the group is dragging it, which belongs
    /// to a later issue.
    ///
    /// Mutation: treat every click as additive, and this fails with all three
    /// selected.
    #[test]
    fn a_plain_click_collapses_a_selection_of_several_to_one() {
        let mut app = selection_editor();
        let right = placeholder(&mut app, 2);

        modifier_click_at(&mut app, in_window(LEFT));
        modifier_click_at(&mut app, in_window(MIDDLE));
        assert_eq!(selected(&app).len(), 2, "there was nothing to collapse");

        click_at(&mut app, in_window(RIGHT), PointerButton::Primary);

        assert_eq!(selected(&app), [right]);
    }

    /// The selection keeps the order things were chosen in.
    ///
    /// `Selection` says the last element is the most recently chosen, because
    /// the inspector will want it. Nothing reads that yet, so the claim is held
    /// here or nowhere. The order asserted is neither the order the
    /// placeholders are spawned in nor their order across the screen, so a
    /// selection that happens to be sorted does not pass by accident.
    ///
    /// Mutation: `swap_remove` in place of `remove` in `select`, and this fails
    /// with the last two the wrong way round. Mutation: `insert(0, entity)`
    /// rather than `push`, and the first assertion fails.
    #[test]
    fn the_selection_keeps_the_order_things_were_chosen_in() {
        let mut app = selection_editor();
        let left = placeholder(&mut app, 0);
        let middle = placeholder(&mut app, 1);
        let right = placeholder(&mut app, 2);

        modifier_click_at(&mut app, in_window(RIGHT));
        modifier_click_at(&mut app, in_window(LEFT));
        modifier_click_at(&mut app, in_window(MIDDLE));
        assert_eq!(selected(&app), [right, left, middle]);

        // The first one chosen, so that taking it out of the front is what is
        // being watched.
        modifier_click_at(&mut app, in_window(RIGHT));

        assert_eq!(selected(&app), [left, middle]);
    }

    /// Either Control or Super is the modifier.
    ///
    /// Four keys rather than one, and each asserted by name. This is what
    /// stands in for macOS on a machine that is not macOS: `docs/specs/ui.md`
    /// §4 says why both keys are taken, and all four can be pressed here
    /// whichever platform this is.
    ///
    /// Mutation: drop either `Super` key from `additive`, and this fails naming
    /// the key that went.
    #[test]
    fn either_control_or_super_is_the_modifier() {
        for key in [
            KeyCode::ControlLeft,
            KeyCode::ControlRight,
            KeyCode::SuperLeft,
            KeyCode::SuperRight,
        ] {
            let mut app = selection_editor();
            let left = placeholder(&mut app, 0);
            let middle = placeholder(&mut app, 1);

            click_at(&mut app, in_window(LEFT), PointerButton::Primary);
            hold_key(&mut app, key);
            click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);
            release_key(&mut app, key);

            assert_eq!(selected(&app), [left, middle], "{key:?} did not add");
        }
    }

    /// The modifier is read when the button goes down.
    ///
    /// `docs/specs/ui.md` §4 fixes what a gesture means when it begins, and the
    /// keyboard is part of what it means. Letting the key go before the button
    /// is the case that decides it, and reading the keyboard at the release
    /// would turn this user's add into a replace and lose what they had.
    ///
    /// Mutation: read `ButtonInput<KeyCode>` in `select` rather than storing it
    /// in `Pressed`, and this fails with only the middle one selected.
    #[test]
    fn the_modifier_is_read_when_the_button_goes_down() {
        let mut app = selection_editor();
        let left = placeholder(&mut app, 0);
        let middle = placeholder(&mut app, 1);
        click_at(&mut app, in_window(LEFT), PointerButton::Primary);

        let at = in_window(MIDDLE);
        hold_key(&mut app, KeyCode::ControlLeft);
        for action in [
            PointerAction::Move { delta: Vec2::ONE },
            PointerAction::Press(PointerButton::Primary),
        ] {
            write_input(&mut app, at, action);
            app.update();
            app.update();
        }
        release_key(&mut app, KeyCode::ControlLeft);
        write_input(&mut app, at, PointerAction::Release(PointerButton::Primary));
        app.update();
        app.update();

        assert_eq!(selected(&app), [left, middle]);
    }

    /// A key that is not the modifier replaces the selection.
    ///
    /// `either_control_or_super_is_the_modifier` says which keys do add, and a
    /// list of keys that add is not a definition until something says which
    /// ones do not. `docs/specs/ui.md` §4 turned Shift down by name, so Shift
    /// is the key a later change would plausibly add to `additive` in passing;
    /// Alt stands for every other key nobody has thought about.
    ///
    /// This is the shape `the_other_buttons_do_not_select` already has for the
    /// mouse.
    ///
    /// Mutation: add either Shift key to `additive`, and this fails naming it.
    #[test]
    fn a_key_that_is_not_the_modifier_replaces_the_selection() {
        for key in [
            KeyCode::ShiftLeft,
            KeyCode::ShiftRight,
            KeyCode::AltLeft,
            KeyCode::AltRight,
        ] {
            let mut app = selection_editor();
            let middle = placeholder(&mut app, 1);

            click_at(&mut app, in_window(LEFT), PointerButton::Primary);
            hold_key(&mut app, key);
            click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);
            release_key(&mut app, key);

            assert_eq!(
                selected(&app),
                [middle],
                "{key:?} added rather than replacing"
            );
        }
    }

    /// Despawning one of several leaves the rest selected.
    ///
    /// `forget_what_is_gone` drops the entity that went and keeps the others,
    /// and until a selection could hold more than one thing that claim had no
    /// content: with one selected, dropping it and clearing everything are the
    /// same program. Somebody who has picked out several things and deletes one
    /// of them keeps the others.
    ///
    /// Mutation: `selection.0.clear()` in place of the `retain`, and this fails
    /// while `a_despawned_entity_does_not_stay_selected` passes.
    #[test]
    fn despawning_one_of_several_leaves_the_rest_selected() {
        let mut app = selection_editor();
        let left = placeholder(&mut app, 0);
        let middle = placeholder(&mut app, 1);

        click_at(&mut app, in_window(LEFT), PointerButton::Primary);
        modifier_click_at(&mut app, in_window(MIDDLE));
        assert_eq!(selected(&app), [left, middle], "both were not selected");

        app.world_mut().entity_mut(middle).despawn();
        app.update();

        assert_eq!(selected(&app), [left]);
    }

    /// The modifier survives a key and a press arriving in one frame.
    ///
    /// A human presses the modifier first and the two are frames apart. An
    /// input batch is what a long frame produces, and a long frame is what a
    /// large level produces, which is the same argument
    /// `a_gesture_that_leaves_the_viewport_in_one_frame_keeps_the_selection`
    /// makes about the pointer.
    ///
    /// **No mutation of this repository makes this fail**, and saying so is
    /// the point of writing it down: what it watches is the engine's order
    /// between `InputSystems` and picking's dispatch, and what holds that
    /// order is the `configure_sets` on `SelectionPlugin`, guarded by the test
    /// below. This one is the canary that would notice the day the engine's
    /// own default changed underneath that line.
    #[test]
    fn the_modifier_survives_a_key_and_a_press_in_one_frame() {
        let mut app = selection_editor();
        let left = placeholder(&mut app, 0);
        let middle = placeholder(&mut app, 1);
        click_at(&mut app, in_window(LEFT), PointerButton::Primary);

        let at = in_window(MIDDLE);
        write_input(&mut app, at, PointerAction::Move { delta: Vec2::ONE });
        app.update();
        // No update between these two: the key and the button are one batch.
        hold_key(&mut app, KeyCode::ControlLeft);
        write_input(&mut app, at, PointerAction::Press(PointerButton::Primary));
        app.update();
        write_input(&mut app, at, PointerAction::Release(PointerButton::Primary));
        app.update();
        app.update();

        assert_eq!(
            selected(&app),
            [left, middle],
            "a key and a press in one frame lost the modifier"
        );
    }

    /// A schedule that reads the keyboard after the press does not build.
    ///
    /// This is what `SelectionPlugin`'s `configure_sets` is worth: with it, a
    /// plugin that puts input processing after picking is a cycle the schedule
    /// refuses and names, which is row 3 of `CLAUDE.md`'s list. Without it the
    /// same plugin builds and the editor quietly reads every modifier as up
    /// whenever the key and the button share a frame, which is row 4 and costs
    /// the user the selection they were assembling.
    ///
    /// Mutation: drop the `configure_sets` call from `SelectionPlugin`, and
    /// this fails, because the schedule then builds happily.
    #[test]
    #[should_panic(expected = "cycle")]
    fn a_schedule_that_reads_the_keyboard_after_the_press_does_not_build() {
        let mut app = editor(headless());
        app.configure_sets(
            PreUpdate,
            bevy::input::InputSystems.after(PickingSystems::Last),
        );
        app.update();
    }

    /// A box drag over two entities selects both.
    ///
    /// The box from `(-300, -100)` to `(60, 100)` covers the left placeholder
    /// and the middle one and leaves the right one alone, so it is also the
    /// claim that a box selects what it covers and not everything.
    ///
    /// Two rather than one, which is RK-007 in the knowledge bank: a loop over
    /// a selection that has only ever held one thing guards none of its
    /// plurals, and this is the first gesture that adds several at once.
    ///
    /// Mutation: return from `finish` before writing the selection, and this
    /// fails. Mutation: take the first entity the box touches rather than every
    /// one, and this fails while every other box test passes.
    #[test]
    fn a_band_over_two_entities_selects_both() {
        let mut app = selection_editor();
        let left = placeholder(&mut app, 0);
        let middle = placeholder(&mut app, 1);

        press_then_release(
            &mut app,
            in_window(Vec2::new(-300.0, -100.0)),
            in_window(Vec2::new(60.0, 100.0)),
        );

        let mut wanted = vec![left, middle];
        wanted.sort();
        assert_eq!(sorted(&app), wanted);
    }

    /// A box drag selects an entity it only half covers.
    ///
    /// `docs/specs/ui.md` §4 decided touched rather than covered. The box ends
    /// at world x `-200`, which is the middle of the left placeholder: it
    /// overlaps half of it and encloses none of it.
    ///
    /// Mutation: ask whether the box contains the entity's bounds rather than
    /// whether it intersects them, and this fails.
    #[test]
    fn a_band_selects_an_entity_it_only_half_covers() {
        let mut app = selection_editor();
        let left = placeholder(&mut app, 0);

        press_then_release(
            &mut app,
            in_window(Vec2::new(-300.0, -100.0)),
            in_window(Vec2::new(-200.0, 100.0)),
        );

        assert_eq!(selected(&app), [left]);
    }

    /// A box dragged up and to the left selects the same things.
    ///
    /// The same rectangle as `a_band_over_two_entities_selects_both`, dragged
    /// from the other corner. A sign error is invisible to a test that only
    /// ever drags one way.
    ///
    /// Mutation: build the rectangle as `Rect { min: anchor, max: now }` in
    /// place of `Rect::from_corners`, and this fails while the test that drags
    /// the other way passes.
    #[test]
    fn a_band_dragged_up_and_to_the_left_selects_the_same_things() {
        let mut app = selection_editor();
        let left = placeholder(&mut app, 0);
        let middle = placeholder(&mut app, 1);

        press_then_release(
            &mut app,
            in_window(Vec2::new(60.0, 100.0)),
            in_window(Vec2::new(-300.0, -100.0)),
        );

        let mut wanted = vec![left, middle];
        wanted.sort();
        assert_eq!(sorted(&app), wanted);
    }

    /// A box that ends where it started selects nothing.
    ///
    /// It ends the selection empty rather than leaving it alone: a box replaces
    /// the selection, and a box with no area replaces it with nothing, which is
    /// what the click on empty space it grew out of already did.
    ///
    /// The fear this guards is the opposite, a box with no area matching
    /// everything, which is what a hit test that stops consulting the rectangle
    /// produces.
    ///
    /// **Whether `finish` short-circuits a box with no area cannot be seen from
    /// here, and no test holds it.** A box begins on empty space by definition,
    /// so one that ends where it started ends on empty space too, and the click
    /// that accompanies it has already cleared the selection by the time
    /// `finish` runs. Measured: an early return for an empty box leaves every
    /// test green.
    ///
    /// Mutation: take every entity rather than the ones the box touches, and
    /// this fails.
    #[test]
    fn a_band_that_ends_where_it_started_selects_nothing() {
        let mut app = selection_editor();
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);
        assert_eq!(selected(&app).len(), 1, "nothing was selected to lose");

        press_then_release(&mut app, in_window(EMPTY), in_window(EMPTY));

        assert!(selected(&app).is_empty(), "{:?}", selected(&app));
    }

    /// A box drag with the modifier adds to what was already selected.
    ///
    /// `docs/specs/ui.md` §4's rule for the modifier, asked with a rectangle
    /// instead of a point.
    ///
    /// Mutation: ignore `Pressed::additive` in `finish`, and this fails.
    #[test]
    fn a_band_with_the_modifier_adds_to_what_was_already_selected() {
        let mut app = selection_editor();
        let left = placeholder(&mut app, 0);
        let middle = placeholder(&mut app, 1);
        click_at(&mut app, in_window(MIDDLE), PointerButton::Primary);

        hold_key(&mut app, KeyCode::ControlLeft);
        press_then_release(
            &mut app,
            in_window(Vec2::new(-300.0, -100.0)),
            in_window(Vec2::new(-150.0, 100.0)),
        );
        release_key(&mut app, KeyCode::ControlLeft);

        let mut wanted = vec![left, middle];
        wanted.sort();
        assert_eq!(sorted(&app), wanted);
    }

    /// A box drag replaces the selection when the modifier is not held.
    ///
    /// **It ends over an entity**, which is where the `clear` in `finish` is
    /// the only one. A box that ends on empty space is also a click on empty
    /// space, and that click clears the selection before `finish` runs, so a
    /// test written that way passes with the `clear` deleted. Measured.
    ///
    /// Mutation: drop the `clear` in `finish`, and this fails, because the
    /// right placeholder stays selected beside the two the box took.
    #[test]
    fn a_band_replaces_the_selection_when_the_modifier_is_not_held() {
        let mut app = selection_editor();
        let left = placeholder(&mut app, 0);
        let middle = placeholder(&mut app, 1);
        click_at(&mut app, in_window(RIGHT), PointerButton::Primary);
        assert_eq!(selected(&app).len(), 1, "nothing was selected to replace");

        press_then_release(
            &mut app,
            in_window(Vec2::new(-300.0, -100.0)),
            in_window(MIDDLE),
        );

        let mut wanted = vec![left, middle];
        wanted.sort();
        assert_eq!(sorted(&app), wanted);
    }

    /// A drag that begins on an entity is not a box.
    ///
    /// That gesture belongs to moving what is selected, which is a later chunk
    /// of this milestone, and taking it here would mean taking it back. The
    /// press is on the middle placeholder and the drag ends past the left one,
    /// so a box would have caught it.
    ///
    /// Mutation: drop the `pressed.over.is_some()` check in `begin`, and this
    /// fails.
    #[test]
    fn a_drag_that_begins_on_an_entity_is_not_a_band() {
        let mut app = selection_editor();

        press_then_release(
            &mut app,
            in_window(MIDDLE),
            in_window(Vec2::new(-300.0, -100.0)),
        );

        assert!(selected(&app).is_empty(), "{:?}", selected(&app));
    }

    /// A box drag stops at the edge of what the viewport shows.
    ///
    /// `viewport_picking` keeps forwarding the pointer while the region is
    /// being dragged, and extrapolates past the region's edge, so a release
    /// over the right-hand pane reads as a world position outside what the
    /// camera draws. The entity this spawns at world x `500` is off screen;
    /// what the box may take is what the user can see.
    ///
    /// Mutation: drop the clamp in `pointer_world`, and this fails, because the
    /// release reads as world x `540` and reaches it.
    #[test]
    fn a_band_stops_at_the_edge_of_what_the_viewport_shows() {
        let mut app = selection_editor();
        let off_screen = app
            .world_mut()
            .spawn((
                Sprite::from_color(Color::WHITE, Vec2::splat(64.0)),
                Transform::from_xyz(500.0, 0.0, 0.0),
                Selectable,
            ))
            .id();
        app.update();

        press_then_release(
            &mut app,
            in_window(Vec2::new(-300.0, -100.0)),
            Vec2::new(1150.0, 284.0),
        );

        let selection = selected(&app);
        assert!(
            !selection.contains(&off_screen),
            "the box reached an entity the viewport does not show"
        );
        assert_eq!(
            selection.len(),
            PLACEHOLDERS.len(),
            "the box took {selection:?} rather than everything on screen"
        );
    }

    /// A box drag selects a turned and scaled entity it touches.
    ///
    /// RK-006 in the knowledge bank: every `Selectable` the editor spawns is an
    /// axis-aligned square at the centre of its own sprite, so the transform in
    /// `world_bounds` is otherwise free to delete. This one is turned an eighth
    /// of a turn, twice the size, and anchored at its top left, which puts its
    /// bounds at world x `0` to `181` and y `-240` to `-60`, while the naive
    /// reading, the entity's position plus the `Aabb` as it stands, puts them
    /// at x `0` to `64` and y `-214` to `-150`. The box covers x `150` to `175`
    /// and y `-230` to `-210`, which is inside the first and outside the
    /// second.
    ///
    /// That corner of the bounds is not sprite, which is the generosity
    /// `world_bounds` documents: the box takes what the axis-aligned bounds
    /// cover, and a turned sprite's bounds cover more than it does. The press
    /// is outside the sprite itself, which is what makes this a box at all
    /// rather than a click on it.
    ///
    /// Mutation: take the `Aabb`'s centre and half extents without putting them
    /// through the `GlobalTransform`, and this fails.
    #[test]
    fn a_band_selects_a_turned_and_scaled_entity_it_touches() {
        let mut app = selection_editor();
        let turned = app
            .world_mut()
            .spawn((
                Sprite::from_color(Color::WHITE, Vec2::splat(64.0)),
                Anchor::TOP_LEFT,
                Transform::from_xyz(0.0, -150.0, 0.0)
                    .with_rotation(Quat::from_rotation_z(FRAC_PI_4))
                    .with_scale(Vec3::splat(2.0)),
                Selectable,
            ))
            .id();
        app.update();

        press_then_release(
            &mut app,
            in_window(Vec2::new(150.0, -230.0)),
            in_window(Vec2::new(175.0, -210.0)),
        );

        assert_eq!(selected(&app), [turned]);
    }

    /// A pan does not draw a box.
    ///
    /// The middle button drags the view, and `docs/specs/ui.md` §4 keeps the
    /// two apart. Something is selected first, because the anchor a box would
    /// use is recorded at the last press with the left button: without one
    /// there is nothing for the mutation below to draw.
    ///
    /// Mutation: drop the button check in `begin`, and this fails, because the
    /// pan then draws a rubber band across the viewport.
    #[test]
    fn a_pan_does_not_draw_a_box() {
        let mut app = selection_editor();
        click_at(&mut app, in_window(EMPTY), PointerButton::Primary);

        for (position, action) in [
            (in_window(EMPTY), PointerAction::Move { delta: Vec2::ONE }),
            (
                in_window(EMPTY),
                PointerAction::Press(PointerButton::Middle),
            ),
            (
                in_window(Vec2::new(100.0, 100.0)),
                PointerAction::Move { delta: Vec2::ONE },
            ),
        ] {
            write_input(&mut app, position, action);
            app.update();
            app.update();
        }

        assert_eq!(covered_by::<BandGizmos>(&app), None);
    }

    /// The release that ends a box drag clicks before it ends the drag.
    ///
    /// Nothing suppresses that click, because `finish` overwrites whatever it
    /// did in the same frame. That argument is only true while `bevy_picking`
    /// emits the two in this order, so the order is what is held here: if a
    /// later engine swaps them, this fails by name rather than the box quietly
    /// leaving the selection it just made empty.
    ///
    /// Measured in `bevy_picking` 0.19.1, whose `pointer_events` triggers
    /// `Click` and `Release` for the previously hovered entities and then the
    /// drops and `DragEnd`.
    #[test]
    fn a_release_that_ended_a_band_clicks_before_it_ends_the_drag() {
        #[derive(Resource, Default)]
        struct Order(Vec<&'static str>);

        let mut app = selection_editor();
        app.init_resource::<Order>();
        let region = app
            .world_mut()
            .query::<(Entity, &Region)>()
            .iter(app.world())
            .find(|(_, region)| **region == Region::Viewport)
            .map(|(entity, _)| entity)
            .expect("there is a viewport region");
        app.world_mut()
            .entity_mut(region)
            .observe(|_: On<Pointer<Click>>, mut order: ResMut<Order>| {
                order.0.push("Click");
            })
            .observe(|_: On<Pointer<DragEnd>>, mut order: ResMut<Order>| {
                order.0.push("DragEnd");
            });

        press_then_release(
            &mut app,
            in_window(Vec2::new(-300.0, -100.0)),
            in_window(Vec2::new(60.0, 100.0)),
        );

        assert_eq!(app.world().resource::<Order>().0, ["Click", "DragEnd"]);
    }

    /// The box is drawn between the corners it was dragged between.
    ///
    /// While the button is still down, which is the state the user spends the
    /// gesture in.
    ///
    /// Mutation: return from `draw_band` before drawing, and this fails.
    /// Mutation: draw from the pointer's position rather than from the anchor,
    /// and this fails.
    #[test]
    fn the_band_is_drawn_between_the_corners_it_was_dragged_between() {
        let mut app = selection_editor();
        let from = Vec2::new(-300.0, -100.0);
        let to = Vec2::new(60.0, 100.0);

        press_and_hold(&mut app, in_window(from), in_window(to));

        assert_eq!(
            covered_by::<BandGizmos>(&app),
            Some(Rect::from_corners(from, to))
        );
    }

    /// The box is gone once the button is let go.
    ///
    /// Mutation: read `Pressed::band` in `finish` rather than taking it, and
    /// this fails, because the box is then drawn for ever.
    #[test]
    fn the_band_is_gone_once_the_button_is_let_go() {
        let mut app = selection_editor();
        press_and_hold(
            &mut app,
            in_window(Vec2::new(-300.0, -100.0)),
            in_window(Vec2::new(60.0, 100.0)),
        );
        assert!(
            covered_by::<BandGizmos>(&app).is_some(),
            "nothing was drawn to take away"
        );

        write_input(
            &mut app,
            in_window(Vec2::new(60.0, 100.0)),
            PointerAction::Release(PointerButton::Primary),
        );
        app.update();
        app.update();

        assert_eq!(covered_by::<BandGizmos>(&app), None);
    }
}
