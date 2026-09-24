---
status: "accepted"
date: 2026-09-24
decision-makers: the project's owner, during `/spec` on #44
---

# The inspector panel belongs to the user while focus is inside it, so `show` does not rebuild it

## Context and Problem Statement

[ADR-0002](./0002-rebuild-the-inspector-rather-than-diff-it.md) rebuilds the
inspector's children whenever the computed `Shape` differs from what is shown,
and it wrote down that #44 would be the first thing to want a row left alone:
"per-row state that survives a rebuild has nowhere to live. A half-typed value
in #44's field is the first thing that will want it, and the answer then is to
hold it outside the row rather than to start reconciling."

#44 arrived and the problem is worse than a half-typed value. `Shape` carries
the values, so **committing an edit changes the component, which changes
`Shape`, which despawns the box that was just typed into.** The same thing
happens without any edit at all: hovering the selected sprite changes its
`PickingInteraction`, which is one of the fourteen components the panel lists.

What the panel shows is [`docs/specs/ui.md` §6](../specs/ui.md) and what it lets
the user edit is [§7](../specs/ui.md); neither is repeated here.

## Decision Drivers

* **Row 4**, something wrong on screen the user can see, is where a stale line
  lands: a `GlobalTransform` that has not caught up with a `Transform` the user
  just committed.
* **Row 4 again, and heavier**, is where the box disappearing lands. An input
  that vanishes mid-edit takes the keystrokes with it, and the user has no way
  to tell what became of them.
* **Row 5** is what a per-frame rebuild would be, and ADR-0002 already ruled it
  out on those grounds.
* ADR-0002's own reversal condition is "something the user was interacting with
  disappearing". This is that condition being met, which is why this is decided
  here rather than left to the implementation.

## Considered Options

* Do not rebuild while `InputFocus` names an entity under the inspector region
* Take the values out of `Shape` and push them into the widgets with
  `UpdateNumberInput`
* Rebuild as ADR-0002 does, and accept that the box goes away on every commit

## Decision Outcome

Chosen option: **do not rebuild while focus is inside the panel.**

`show` asks `bevy_input_focus`' own `IsFocused::is_focus_within` whether the
focus is inside the inspector region, and returns before comparing anything and
without writing `Shown` when it is. The engine's call is the same walk up
`ChildOf` that a hand-written one would be, and
[architecture.md §5](../specs/architecture.md) asks for the engine's where there
is one.

The rule is one sentence: **while the user is in the panel, the panel is theirs.**
It is the smallest thing that answers ADR-0002's open cost, and it holds the
half-typed value in the only place that can hold it without a reconcile, which
is the widget the user is typing into.

**And for one frame after the focus leaves, which is not an extra and was found
by review.** A box commits when it loses focus, and the losing and the
committing are in different places in the frame: the engine clears `InputFocus`
in `PreUpdate`, `show` runs in `Update`, and `FocusLost` reaches the widget in
`PostUpdate`. With the rule as first written, clicking away from a box rebuilt
the panel in between, so `bevy_feathers` found a despawned entity, emitted
nothing, and **the number the user had typed was discarded in silence.** The
`FocusWasInside` resource holds the panel for that one frame, so the box
outlives its own commit. Row 4: the box snaps back to the old number and the
user can see that nothing took, which is a poor way to find out.

Leaving `Shown` unwritten is part of the choice. Writing it would record a
picture that was never drawn, and the panel would then compare equal to
something that is not on screen: the failure ADR-0002's `into` field exists to
prevent, in a new place.

### Confirmation

`a_focused_field_survives_the_edit_it_commits` in
`crates/editor/src/inspector.rs`. It focuses a number input, commits a value,
and asserts that the same entity is still there afterwards.

**Mutation: delete the focus guard at the top of `show`.** The commit changes
`Transform`, `Shape` differs, and the panel is despawned, so the entity the test
recorded is gone.

**Mutation: hold only while the focus is inside, dropping `FocusWasInside`.**
Then `committing_a_field_writes_it_to_the_component` fails, because the gesture
its helper performs is the one that loses the value: typing, then clicking
somewhere else. That helper used to clear `InputFocus` by hand between updates,
which is the one arrangement where the panel has nothing to redraw and the box
survives by luck, and the suite was green while the gesture was broken.

The opposite failure, a panel that stops rebuilding altogether, is held by
`the_panel_is_frozen_while_a_box_has_focus_and_catches_up_after`. **Mutation:
return whenever anything is already shown**, rather than only while the focus is
in the panel or was last frame.

### Consequences

* Good, because nothing is held outside the row. ADR-0002 expected a side table
  of half-typed values and there is none; the widget holds its own buffer, which
  is what a text input is.
* Good, because it costs one `Res<InputFocus>` and one ancestor walk, and no
  second mechanism to keep in step with the first.
* Bad, because every other line on the panel is stale for as long as one box
  holds focus. `GlobalTransform` and `Aabb` are the visible ones.
* Bad, because the guard is about **the region**, not about the row. Focus in
  any box freezes the whole panel, including the components nobody is editing.
  Per-row would need the reconcile ADR-0002 turned down.
* Bad, because the panel is one frame out of date after the focus leaves.
  Nobody can see one frame, and the alternative was the commit not happening.
* What would reverse this: a panel where the stale lines matter more than the
  box surviving. A gizmo dragged in the viewport while a box holds focus would
  be it, because then the numbers on screen contradict what the user is
  watching move.

## Pros and Cons of the Options

### Do not rebuild while focus is inside the panel

* Good, because the box survives the edit it causes, which is the whole point.
* Good, because it is a guard on an existing early return rather than a new
  shape.
* Bad, because the freeze is panel-wide and the staleness is visible.

### Take the values out of `Shape` and push them in with `UpdateNumberInput`

* Good, because it is the two-way synchronization `bevy_feathers` documents, and
  `number_input_on_update` already declines to overwrite a focused widget.
* Good, because the panel would stop rebuilding when a value moves, which is
  most of the rebuilds there are.
* Bad, because the panel is mostly not numbers. Measured on one placeholder: 22
  value lines, of which 9 would be boxes. Every other line needs its own way to
  be updated in place, and that is the row-by-row reconcile ADR-0002 rejected,
  arriving through the back door.
* Bad, because pushing a value in emits a `ValueChange` back out. Measured with
  a throwaway probe under `headless()`: `UpdateNumberInput` with `12.5` on a
  freshly spawned input left the buffer reading `12.5` and an observer hearing
  `12.5`. It is filtered by `is_final`, but it is a loop that exists.

### Rebuild as ADR-0002 does, and accept the box going away

* Good, because it is no code at all.
* Bad, because every commit destroys the box that caused it, focus lands on a
  despawned entity, and a second value cannot be typed without clicking again.
* Bad, because moving the pointer over the viewport during an edit does the same
  thing, through `PickingInteraction`. The user would have no way to connect the
  cause to the effect.

## More Information

* The code: `crates/editor/src/inspector.rs`, `show`.
* What can be edited, and what that rejected: [`ui.md` §7](../specs/ui.md).
* The decision this refines, and the cost it left open:
  [ADR-0002](./0002-rebuild-the-inspector-rather-than-diff-it.md).
* Issue #44, where the options were put and chosen.
