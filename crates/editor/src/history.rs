//! The history of what the user changed, and the key that takes it back.
//!
//! The command and the one history are
//! [`docs/specs/data-model.md` §1](../../../docs/specs/data-model.md); what
//! Ctrl+Z takes back, wherever the focus is, is
//! [`docs/specs/ui.md` §8](../../../docs/specs/ui.md).

use bevy::input::keyboard::Key;
use bevy::input_focus::InputFocusSystems;
use bevy::prelude::*;

use crate::inspector;

/// One change to the world that can be taken back.
///
/// **There is no `description`**, which `docs/specs/data-model.md` §1 says
/// why: nothing reads one yet.
pub trait EditorCommand: Send + Sync + 'static {
    /// Make the change.
    fn execute(&mut self, world: &mut World);
    /// Take it back, leaving the world as [`Self::execute`] found it.
    fn undo(&mut self, world: &mut World);
}

/// Every change the user has made, oldest first.
///
/// **The field is private and [`History::record`] is the only way in.** What
/// the compiler holds is that nothing enters the history except through
/// `record`. **It does not hold that nothing changes the world except
/// through it**: a system can write a component or a resource without ever
/// touching this type, and that compiles. So "one history" is still a rule a
/// writer has to follow, and there is no single search that finds every
/// writer that does not. A reflected write goes through
/// `World::get_reflect_mut`; the selection (issue #54) is a resource written
/// in `selection.rs`; a deletion (issue #55) is a despawn. The inspector's
/// commit is the first writer through here, and the gizmos, the selection and
/// the deletion are each the next.
#[derive(Resource, Default)]
pub struct History(Vec<Box<dyn EditorCommand>>);

impl History {
    /// Make a change and remember it.
    pub fn record(world: &mut World, mut command: impl EditorCommand) {
        command.execute(world);
        world.resource_mut::<History>().0.push(Box::new(command));
    }

    /// Take the last change back, and say whether there was one.
    ///
    /// Mutation: take `remove(0)` in place of `pop()`, and
    /// `two_commits_come_back_in_reverse_order` fails. Mutation: return
    /// before calling `undo`, and
    /// `undoing_a_commit_puts_the_component_back_bit_for_bit` fails.
    fn undo(world: &mut World) -> bool {
        let Some(mut command) = world.resource_mut::<History>().0.pop() else {
            return false;
        };
        command.undo(world);
        true
    }
}

/// The history, and Ctrl+Z.
///
/// Mutation: leave its row out of the editor's member table, and
/// `the_group_carries_the_members_the_table_names` fails.
pub struct HistoryPlugin;

impl Plugin for HistoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<History>().add_systems(
            PostUpdate,
            take_back.after(InputFocusSystems::FocusChangeEvents),
        );
    }
}

/// Take one step back when Ctrl+Z is pressed.
///
/// # After the focus changes, not in `Update`
///
/// Letting go of a box and pressing Ctrl+Z can land in one frame. The engine
/// clears the focus in `PreUpdate` and the box commits in `PostUpdate`, when
/// `FocusLost` reaches it, so an undo that ran in `Update` would take back the
/// entry before that commit and then watch the box write it again. Ordered
/// after `FocusChangeEvents`, the commit is recorded first and this takes it
/// back, which is the order the user did them in.
///
/// Mutation: add this to `Update` instead, and
/// `letting_go_of_a_box_and_pressing_ctrl_z_in_one_frame_takes_back_what_letting_go_committed`
/// fails.
///
/// # Which keys
///
/// Ctrl or Cmd, on every platform, for the reason `docs/specs/ui.md` §4 takes
/// both for the selection; and **not Shift**, which is redo's. Z is read by
/// the letter the key produces rather than by where it sits, because that is
/// how `bevy_ui_widgets` reads Ctrl+A, C, X and V inside the box, and one
/// window should not have two rules.
///
/// Mutation: drop the `Super` keys, and `cmd_z_is_undo_as_ctrl_z_is` fails.
/// Mutation: drop the Shift check, and `ctrl_shift_z_is_not_undo` fails.
/// Mutation: read `KeyCode::KeyZ` in place of the letter, and
/// `undo_is_the_key_that_says_z_wherever_it_is` fails.
///
/// # One step, wherever the focus is
///
/// What was typed in a focused box is taken back before the history is, and
/// after the history is, the boxes are given their new values in place. The
/// inspector is called by name rather than through an event: there is one
/// caller and one callee, and a pair of observers would add a question about
/// which runs first for nothing. The next panel with boxes in it is added
/// here.
fn take_back(
    letters: Res<ButtonInput<Key>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
) {
    let z = letters
        .get_just_pressed()
        .any(|key| matches!(key, Key::Character(letter) if letter.eq_ignore_ascii_case("z")));
    let command = keys.any_pressed([
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ]);
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    if !(z && command) || shift {
        return;
    }
    commands.queue(|world: &mut World| {
        if inspector::take_back_typing(world) {
            return;
        }
        if History::undo(world) {
            inspector::show_values_in_place(world);
        }
    });
}
