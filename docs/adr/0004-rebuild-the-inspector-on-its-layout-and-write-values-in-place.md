---
status: "accepted"
date: 2026-11-14
decision-makers: the project's owner, during `/spec` on #88
---

# Rebuild the inspector's panel when its layout changes, and write a changed value into the row already showing it

## Context and Problem Statement

[ADR-0002](./0002-rebuild-the-inspector-rather-than-diff-it.md) rebuilt the
whole panel whenever what it computed differed from what it showed, values
included, and named its own reversal: "a panel where a rebuild is visible,
either as a dropped frame or as something the user was interacting with
disappearing". A drag in the viewport is that. The dragged entity's
`Transform`, `GlobalTransform` and `Aabb` change on every frame of it, so every
frame rebuilt every row, and in a development build that was the whole frame:
120 to 160 ms, against 16.7 ms with `InspectorPlugin` left out of the editor,
measured on #86. Optimizing the dependencies
([dev-environment.md §4](../specs/dev-environment.md)) made the rebuild fit in a
frame; it did not make it unnecessary, and a panel with more rows brings it
back.

ADR-0002 decided two things. The second, walking the entity with
`World::inspect_entity`, the engine's own call, is not touched and is not
repeated here.

## Decision Drivers

* **Row 5**, the viewport not answering, is where the rebuild landed: the
  frame a drag spends rebuilding the panel is a frame the viewport does not
  draw.
* **Row 4**, a stale value beside fresh ones, is where writing in place can
  land if a changed value is missed. That is the failure ADR-0002 avoided by
  keeping no per-row state, so whatever is chosen has to keep a row's identity
  derivable from what is drawn rather than remembered beside it.
* The editor already writes values into boxes without a rebuild, after an undo:
  `show_values_in_place` in `crates/editor/src/inspector.rs`.
* [ADR-0003](./0003-the-inspector-panel-belongs-to-the-user-while-focus-is-in-it.md)
  freezes the panel while focus is inside it. Nothing here changes that: while
  the user is in a box, nothing is written to the panel at all.

## Considered Options

* Compare the layout to decide a rebuild, and write changed values into the
  rows that show them
* Write only the number boxes in place, and keep rebuilding for a text value
* Reconcile row by row, as ADR-0002 rejected

## Decision Outcome

Chosen option: **compare the layout, write the values in place.**

`show` still computes the panel's whole content as one value each frame and
compares it with what is shown. When the two differ only in values, with the
same entity, region, title, components, field names, field kinds and leaf names,
the panel is left standing and each changed value is written into the row
already showing it: a number box through `NumberInputValue`, as an undo does,
and a text value by replacing the text of the entity that draws it. Any other
difference rebuilds the panel as before.

A row's text value is found from a marker its row carries, naming its position
among the panel's text values, which is fixed by the layout that was just
compared equal. Nothing is held beside the row that the world could
contradict.

Writing only the boxes in place was not enough: `GlobalTransform` and the
other values without a box are text, they change on every frame of a drag, and
a rebuild for them is the rebuild this record exists to remove.

### Confirmation

In `crates/editor/src/inspector.rs`, each measured by applying the mutation
and watching the named test fail:

* `a_drag_leaves_the_panel_standing_and_its_values_follow` drags the selected
  placeholder over several moves and holds the ids under the panel unchanged,
  the translation box and the `GlobalTransform` line showing the moved `x`.
  **Mutation: rebuild on any difference again**, and it fails on the ids.
  Mutation: skip the text lines in `write_values_in_place`, and it fails on
  `GlobalTransform`. Mutation: skip the boxes, and it fails with five others.
* `a_component_added_to_the_selected_entity_rebuilds_the_panel` inserts a
  component on the selected entity and finds its row. **Mutation: make
  `Shape::same_layout` ignore the rows**, and it fails with nine others.
* `renaming_the_selected_entity_changes_the_header` renames the selected
  entity, which changes only a value and the title. **Mutation: make
  `Shape::same_layout` ignore the title**, and it fails.
* Writing a box in place has to give the box what a rebuilt one would show,
  and two existing tests found where a value alone does not.
  `ctrl_z_in_a_box_left_empty_by_a_long_value_reaches_the_history` fails when
  a box gets only its `NumberInputValue`: a value longer than the box holds
  left the old text in it, which letting go would write back.
  `a_scale_the_engine_could_not_read_back_is_never_written` fails when a box is
  compared by its value alone: text a commit refused stayed in it after the
  focus left.
* `the_inspector_does_not_rebuild_what_has_not_changed`,
  `choosing_a_different_entity_changes_what_is_shown` and
  `a_panel_holding_a_nan_is_not_rebuilt_every_frame` still hold what they held
  for ADR-0002.

**Comparing the entity in `same_layout` is held by no test**, measured: another
entity also has another title, so ignoring `of` leaves the suite green. It is
kept for the reason `Shape::of` gives in the code. Nor is anything compared
inside a row, measured the same way: the component, the field names and kinds,
and the leaves can each be ignored with the suite green, because a field's
kind and leaves come from the component's type rather than from a value.
`Shape::same_layout` says why each is still compared.

### Consequences

* Good, because a drag no longer rebuilds the panel, which is the whole of the
  slowdown #86 measured.
* Good, because a value written in place reads the same `Shape` a rebuild
  would draw, so the two paths cannot disagree about what a value says.
* Bad, because there are now two paths to the screen, a rebuild and a write,
  and a value kind added later needs both.
* Bad, because a row's text value is found by position, and that holds only
  while a layout compared equal means the rows were spawned in the same order.
* What would reverse this: a value written in place that the user can see is
  stale, or a layout change that compares equal.

## Pros and Cons of the Options

### Compare the layout, write values in place

* Good, because no rebuild happens while only values move.
* Bad, because the text values need a way to be found again.

### Boxes in place, rebuild for text

* Good, because it is only `show_values_in_place`, which exists.
* Bad, because a drag changes `GlobalTransform`'s text on every frame, so it
  still rebuilds on every frame.

### Reconcile row by row

* Good, because the cost follows what changed, layout included.
* Bad, because it is the per-row state ADR-0002 turned down, whose bug is a
  stale row, for layout changes that happen only on a click.
