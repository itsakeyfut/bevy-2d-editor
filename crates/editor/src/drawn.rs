//! Reading back what a gizmo group drew, from a test.
//!
//! One copy rather than one per test module, for the reason `pointer.rs` gives
//! about the pointer: two modules ask the same thing of the same storage, and a
//! parser that drifted between them would be two different claims wearing one
//! name. The outline asks it of `outline::SelectionGizmos` and the box drag of
//! `selection::BandGizmos`, so the group is the parameter.

use bevy::gizmos::config::GizmoConfigGroup;
use bevy::gizmos::{GizmoAsset, GizmoHandles};
use bevy::prelude::*;
use core::any::TypeId;

/// Every rectangle a group drew, one per strip.
///
/// `rect_2d` emits a line loop, which arrives as a repeated vertex and a `NaN`
/// separator, so a strip is a run of finite positions and the count of runs is
/// the count of rectangles. Reading the runs apart rather than together is what
/// lets a test tell two rectangles from one: their extents taken together are a
/// single rectangle covering both.
pub(crate) fn rectangles_of<T: GizmoConfigGroup>(app: &App) -> Vec<Rect> {
    let Some(handle) = app
        .world()
        .resource::<GizmoHandles>()
        .handles()
        .get(&TypeId::of::<T>())
        .cloned()
        .flatten()
    else {
        return Vec::new();
    };
    let mut rectangles = Vec::new();
    let mut strip: Vec<Vec2> = Vec::new();
    let mut close = |strip: &mut Vec<Vec2>| {
        if let (Some(min), Some(max)) = (
            strip.iter().copied().reduce(Vec2::min),
            strip.iter().copied().reduce(Vec2::max),
        ) {
            rectangles.push(Rect::from_corners(min, max));
        }
        strip.clear();
    };
    for position in &app
        .world()
        .resource::<Assets<GizmoAsset>>()
        .get(&handle)
        .expect("the group's handle names an asset")
        .strip_positions
    {
        if position.is_finite() {
            strip.push(position.truncate());
        } else {
            close(&mut strip);
        }
    }
    close(&mut strip);
    rectangles
}

/// What a group's drawing covers, or `None` when it drew nothing.
///
/// `update_gizmo_meshes` puts a group's handle back to `None` in a frame where
/// nothing reached its storage, so "nothing is drawn" is a state that can be
/// read rather than an absence that has to be inferred.
pub(crate) fn covered_by<T: GizmoConfigGroup>(app: &App) -> Option<Rect> {
    app.world()
        .resource::<GizmoHandles>()
        .handles()
        .get(&TypeId::of::<T>())
        .cloned()
        .flatten()?;
    let drawn = rectangles_of::<T>(app);
    assert!(
        !drawn.is_empty(),
        "a handle exists with nothing drawn in it"
    );
    Some(
        drawn
            .into_iter()
            .reduce(|covered, rectangle| covered.union(rectangle))
            .expect("something was drawn"),
    )
}
