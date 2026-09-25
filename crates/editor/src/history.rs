//! The history of what the user changed, and the keys that walk it.
//!
//! The command and the one history are
//! [`docs/specs/data-model.md` §1](../../../docs/specs/data-model.md); what
//! Ctrl+Z takes back, wherever the focus is, is
//! [`docs/specs/ui.md` §8](../../../docs/specs/ui.md), and what redo puts
//! back is §9 of the same file.

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

/// Every change the user has made, and every one undo has taken back.
///
/// **The fields are private and [`History::record`] is the only way in.** What
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
pub struct History {
    /// What undo can take back, oldest first.
    done: Vec<Box<dyn EditorCommand>>,
    /// What undo has taken back and redo can put back, the most recently
    /// undone last.
    undone: Vec<Box<dyn EditorCommand>>,
}

impl History {
    /// Make a change and remember it, and forget everything that could still
    /// be redone.
    ///
    /// **Dropping the redo side is what keeps the history one line**: commit
    /// A, undo it, commit B, and a redo that still held A would write it over
    /// B, a value the user did not choose.
    /// [`docs/specs/ui.md` §9](../../../docs/specs/ui.md) has why only an
    /// entry drops it.
    ///
    /// Mutation: leave out the `clear`, and
    /// `a_new_commit_after_an_undo_leaves_nothing_to_redo` fails.
    pub fn record(world: &mut World, mut command: impl EditorCommand) {
        command.execute(world);
        let mut history = world.resource_mut::<History>();
        history.done.push(Box::new(command));
        history.undone.clear();
    }

    /// Take the last change back, and say whether there was one.
    ///
    /// Mutation: take `remove(0)` in place of `pop()`, and
    /// `two_commits_come_back_in_reverse_order` fails. Mutation: return
    /// before calling `undo`, and
    /// `undoing_a_commit_puts_the_component_back_bit_for_bit` fails.
    /// The entry moves to the redo side whatever its `undo` did, including
    /// when its entity is gone and it wrote nothing: redo then writes
    /// nothing either. That holds because the push is unconditional, and
    /// the history cannot tell which entries wrote anything.
    ///
    /// Mutation: drop the `undone.push`, and
    /// `redo_puts_back_the_commit_undo_took_back_and_the_box_shows_it` fails.
    fn undo(world: &mut World) -> bool {
        let Some(mut command) = world.resource_mut::<History>().done.pop() else {
            return false;
        };
        command.undo(world);
        world.resource_mut::<History>().undone.push(command);
        true
    }

    /// Put back the last change undo took back, and say whether there was
    /// one.
    ///
    /// Mutation: take `remove(0)` in place of `pop()`, and
    /// `two_undos_come_back_in_order_under_two_redos` fails. Mutation: return
    /// before calling `execute`, and
    /// `redo_puts_back_the_commit_undo_took_back_and_the_box_shows_it` fails.
    /// Mutation: drop the `done.push`, and
    /// `an_undo_after_a_redo_takes_the_redone_commit_back` fails.
    fn redo(world: &mut World) -> bool {
        let Some(mut command) = world.resource_mut::<History>().undone.pop() else {
            return false;
        };
        command.execute(world);
        world.resource_mut::<History>().done.push(command);
        true
    }
}

/// The history, Ctrl+Z, and redo.
///
/// Mutation: leave its row out of the editor's member table, and
/// `the_group_carries_the_members_the_table_names` fails.
pub struct HistoryPlugin;

impl Plugin for HistoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<History>().add_systems(
            PostUpdate,
            (take_back, put_back).after(InputFocusSystems::FocusChangeEvents),
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
    let (command, shift) = command_and_shift(&keys);
    if !(just_pressed_letter(&letters, "z") && command) || shift {
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

/// Put one step back when Ctrl+Y or Ctrl+Shift+Z is pressed.
///
/// Ordered as [`take_back`] is, and for the same reason: letting go of a box
/// and pressing Ctrl+Y can land in one frame, and the commit letting go makes
/// is a new entry that drops the redo side. Read first, the redo would put
/// back an undone value and the commit would then land on top of it, leaving
/// an entry in the history the user never made.
///
/// Mutation: add this to `Update` instead, and
/// `letting_go_of_a_box_and_pressing_ctrl_y_in_one_frame_does_not_redo_under_the_commit`
/// fails.
///
/// # Which keys
///
/// Ctrl or Cmd with Y, or with Shift and Z, on every platform, read by the
/// letter as [`take_back`] reads Z.
/// [`docs/specs/ui.md` §9](../../../docs/specs/ui.md) has why both.
///
/// Mutation: stop requiring Ctrl or Cmd, and
/// `y_without_ctrl_or_cmd_is_not_redo` fails. Mutation: drop the Shift and Z
/// arm, and `ctrl_shift_z_is_redo_as_ctrl_y_is` fails. Mutation: drop the
/// Shift check on Y, and `ctrl_shift_y_is_not_redo` fails.
///
/// # Not while something is typed
///
/// Typing in the focused box holds redo back: it is the newest thing the user
/// did, and putting an older value into the box would throw it away. Nothing
/// is written and the redo side is kept.
///
/// Mutation: drop the `typing_in_focus` check, and
/// `redo_does_nothing_while_the_focused_box_holds_typing` fails.
fn put_back(
    letters: Res<ButtonInput<Key>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
) {
    let (command, shift) = command_and_shift(&keys);
    let pressed = |letter| just_pressed_letter(&letters, letter);
    if !(command && ((pressed("y") && !shift) || (pressed("z") && shift))) {
        return;
    }
    commands.queue(|world: &mut World| {
        if inspector::typing_in_focus(world).is_some() {
            return;
        }
        if History::redo(world) {
            inspector::show_values_in_place(world);
        }
    });
}

/// Whether Ctrl or Cmd is held, and whether Shift is, as undo and redo both
/// read them.
///
/// Ctrl and Cmd count alike on every platform, for the reason
/// `docs/specs/ui.md` §4 takes both for the selection.
///
/// Mutation: drop the `Super` keys, and `cmd_z_is_undo_as_ctrl_z_is` and
/// `cmd_y_is_redo_as_ctrl_y_is` fail.
fn command_and_shift(keys: &ButtonInput<KeyCode>) -> (bool, bool) {
    let command = keys.any_pressed([
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ]);
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    (command, shift)
}

/// Whether the key that produces `letter` went down this frame, wherever it
/// sits on the keyboard.
fn just_pressed_letter(letters: &ButtonInput<Key>, letter: &str) -> bool {
    letters
        .get_just_pressed()
        .any(|key| matches!(key, Key::Character(pressed) if pressed.eq_ignore_ascii_case(letter)))
}
