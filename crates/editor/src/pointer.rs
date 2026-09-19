//! Driving the window's pointer from a test.
//!
//! One copy rather than one per test module. Two modules ask the same thing of
//! the editor, that a click at a place in the window reaches what is in the
//! world, and a helper that drifted between them would be two different claims
//! wearing one name.
//!
//! The inputs are written as [`PointerInput`] for the window's own pointer,
//! which is one layer earlier than triggering a `Pointer` event: the chain
//! under test runs from the window through `viewport_picking` to
//! `sprite_picking`, and triggering the last event in it would assume the thing
//! being claimed.

use bevy::camera::NormalizedRenderTarget;
use bevy::picking::pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput};
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowRef};

/// Where a world position sits in the window, in logical pixels.
///
/// The viewport region starts 240 across and 28 down and measures 740 by 512,
/// and its camera shows one world unit per pixel centred on the origin, so its
/// middle is the world's. Spelled out here rather than read from the code that
/// lays it out, because a test that computes a position the same way the editor
/// does agrees with the editor rather than with the drawing in
/// `docs/specs/ui.md` §1.
pub(crate) fn in_window(world: Vec2) -> Vec2 {
    Vec2::new(240.0 + 370.0 + world.x, 28.0 + 256.0 - world.y)
}

/// Press and release a button at a window position.
pub(crate) fn click_at(app: &mut App, position: Vec2, button: PointerButton) {
    for action in [
        PointerAction::Move { delta: Vec2::ONE },
        PointerAction::Press(button),
        PointerAction::Release(button),
    ] {
        write_input(app, position, action);
        app.update();
        app.update();
    }
}

/// Write one pointer input for the window's own pointer.
pub(crate) fn write_input(app: &mut App, position: Vec2, action: PointerAction) {
    let window = app
        .world_mut()
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(app.world())
        .expect("there is a primary window");
    let location = Location {
        position,
        target: NormalizedRenderTarget::Window(
            WindowRef::Primary
                .normalize(Some(window))
                .expect("the primary window normalises"),
        ),
    };
    app.world_mut()
        .write_message(PointerInput::new(PointerId::Mouse, location, action));
}
