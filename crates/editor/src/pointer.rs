//! Driving the window's pointer and keyboard from a test.
//!
//! One copy rather than one per test module. Two modules ask the same thing of
//! the editor, that a click at a place in the window reaches what is in the
//! world, and a helper that drifted between them would be two different claims
//! wearing one name.
//!
//! The keyboard half is named `hold_key` and `release_key` rather than `hold`
//! and `let_go`, because holding and letting go are what the mouse button does
//! too: a rubber band drags with the button held, and that helper wants the
//! shorter name.
//!
//! The inputs are written as [`PointerInput`] for the window's own pointer,
//! which is one layer earlier than triggering a `Pointer` event: the chain
//! under test runs from the window through `viewport_picking` to
//! `sprite_picking`, and triggering the last event in it would assume the thing
//! being claimed.

use bevy::camera::NormalizedRenderTarget;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
use bevy::input::mouse::MouseScrollUnit;
use bevy::input::touch::TouchPhase;
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

/// Turn the wheel by `notches` with the pointer at a window position.
///
/// Written as a [`PointerInput`] like the rest of this module, so the chain
/// under test is the real one: the window, then UI picking deciding which node
/// is under the pointer, then `Pointer<Scroll>` bubbling up from it.
pub(crate) fn scroll_at(app: &mut App, position: Vec2, notches: f32) {
    write_input(app, position, PointerAction::Move { delta: Vec2::ONE });
    app.update();
    write_input(
        app,
        position,
        PointerAction::Scroll {
            unit: MouseScrollUnit::Line,
            x: 0.0,
            y: notches,
            phase: TouchPhase::Moved,
        },
    );
    app.update();
    app.update();
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

/// The window an input is addressed to.
///
/// The pointer's [`Location`] needs it. The keyboard's message carries it and
/// nothing reads it, which [`write_key`] says more about.
fn primary_window(app: &mut App) -> Entity {
    app.world_mut()
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(app.world())
        .expect("there is a primary window")
}

/// Hold a key down, and leave it down.
///
/// Written as a [`KeyboardInput`] rather than by pressing `ButtonInput`
/// directly, for the reason the module says about the pointer: the chain under
/// test starts one layer earlier, and `keyboard_input_system` is part of it.
///
/// It stays down across the frames a gesture takes. Measured: that system
/// clears only what was just pressed and just released, so one message holds
/// for the six updates [`click_at`] runs.
pub(crate) fn hold_key(app: &mut App, key: KeyCode) {
    write_key(app, key, ButtonState::Pressed);
}

/// Let a key back up.
pub(crate) fn release_key(app: &mut App, key: KeyCode) {
    write_key(app, key, ButtonState::Released);
}

/// Write one keyboard input for the primary window.
///
/// `logical_key` is left unidentified rather than given what a layout would
/// produce, because nothing here reads `ButtonInput<Key>`; saying so is cheaper
/// than a table of layouts nothing consults. `window` is required by the
/// message and read by nobody on this path: `keyboard_input_system` takes
/// `key_code`, `logical_key` and `state` and ignores the rest, so a key cannot
/// be aimed at one window rather than another.
///
/// It runs no frame of its own, which is [`write_input`]'s contract too. A key
/// takes effect in the next update, and a caller that needs it down before it
/// reads anything runs one; a trailing [`release_key`] before an assertion does
/// not, and does not need to. An `app.update()` was here and nothing in the
/// workspace failed without it.
fn write_key(app: &mut App, key: KeyCode, state: ButtonState) {
    let window = primary_window(app);
    app.world_mut().write_message(KeyboardInput {
        key_code: key,
        logical_key: Key::Unidentified(NativeKey::Unidentified),
        state,
        text: None,
        repeat: false,
        window,
    });
}

/// Write one pointer input for the window's own pointer.
pub(crate) fn write_input(app: &mut App, position: Vec2, action: PointerAction) {
    let window = primary_window(app);
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
