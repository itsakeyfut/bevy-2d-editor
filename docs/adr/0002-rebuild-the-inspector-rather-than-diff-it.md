---
status: "accepted"
date: 2026-09-23
decision-makers: the project's owner, during `/spec` on #42
---

# Rebuild the inspector's panel rather than reconcile it, and walk the entity from an ordinary system

## Context and Problem Statement

The inspector is the first UI in this workspace whose content is not fixed when
it is spawned. Everything before it, the five regions in
`crates/editor/src/lib.rs`, is written once at startup and never changes; this
panel changes every time the selection does, and its length changes with it.

Two shapes answer that, and they differ in what they keep: a rebuild keeps
nothing and a reconcile keeps a row per component. The same change also has to
choose how the entity is walked, because Bevy offers the walk in two places and
one of them makes the system exclusive.

What is on screen is [`docs/specs/ui.md` §6](../specs/ui.md) and is not repeated
here.

The panels that come after this inherit whichever is chosen: the level editor's
outline, the database editor's table
([database-editor.md §1](../specs/database-editor.md)) and the manuscript UI
([scenario-editor.md §2](../specs/scenario-editor.md)) are all lists whose
contents move.

## Decision Drivers

* **Row 4**, the editor showing something wrong where the user can see it, is
  where a reconcile's bug lands: a row that was not updated is a stale name
  beside fresh ones.
* **Row 5**, the viewport not answering, is where a rebuild's bug lands: no
  comparison, or a comparison that is always false, and the panel is destroyed
  and rebuilt every frame.
* **#44 attaches an input field to a row.** Whatever is chosen has to leave a
  row alone while nothing about it has changed, or the field is destroyed under
  the cursor.
* Nothing here is large. Fifteen rows on a placeholder, measured.

## Considered Options

* Rebuild the whole panel when the computed content differs from what is shown
* Reconcile row by row against the existing children
* Rebuild every frame, with no comparison

## Decision Outcome

Chosen option: **rebuild when the computed content differs**, compared as one
value.

`crates/editor/src/inspector.rs` computes a `Shape` from the selection each
frame, compares it against the `Shown` resource, and despawns and respawns the
region's children only when the two differ. There is no per-row state, so there
is nothing that can fall out of step with the model; the whole question a
reconcile answers does not arise.

Comparing the finished value, rather than watching `Selection` for changes, is
part of the choice. Change detection on the selection alone would not see a
component added to an entity that is already selected, and watching archetypes
as well would be a second mechanism to keep in step with the first.

**The walk is done from an ordinary system.** `World::inspect_entity` is the
engine's own and returns exactly what this needs, `Err` for a despawned entity
included, but it takes `&World` and would make this the only exclusive system in
the editor. `EntityRef::archetype().components()` with `&Components` beside it
is the same walk in a system the schedule can run in parallel with others. This
is where "use what Bevy provides"
([architecture.md §5](../specs/architecture.md)) meets a cost: the engine
provides both, and the cheaper-looking one is the one that serialises the
schedule.

### Confirmation

`the_inspector_does_not_rebuild_what_has_not_changed` in
`crates/editor/src/inspector.rs`. It reads the entity ids under the panel,
updates twice with nothing changed, and asserts the ids are the same: a rebuild
despawns them, so surviving ids are a panel that was left alone.

**Mutation: delete the `shown.0 == wanted` guard in `show`.** Applied, and that
test failed while the other ten in the module passed. Measured.

The opposite failure, a panel that never rebuilds, is held by
`choosing_a_different_entity_changes_what_is_shown`. Mutation: return early
whenever `Shown` already holds something. Applied, and six tests failed
including that one.

**Nothing holds the exclusive-versus-ordinary half of this decision**, and
nothing can: both spellings produce the same panel. What would reverse it is a
reviewer deciding the engine's own call reads better than reproducing it, which
is what this record exists to answer.

### Consequences

* Good, because there is no reconcile to get wrong, and therefore no stale row.
* Good, because the panel is left entirely alone while nothing has changed,
  which is what #44's input field needs.
* Bad, because every change rebuilds every row, so a panel of a thousand rows
  would rebuild a thousand rows to change one. Nothing is near that.
* Bad, because per-row state that survives a rebuild has nowhere to live. A
  half-typed value in #44's field is the first thing that will want it, and the
  answer then is to hold it outside the row rather than to start reconciling.
* What would reverse this: a panel where a rebuild is visible, either as a
  dropped frame or as something the user was interacting with disappearing.
  Scrolling and collapsing, both deferred in
  [`ui.md` §6](../specs/ui.md), are the likely first sign.

## Pros and Cons of the Options

### Rebuild when the content differs

* Good, because no state is retained, so nothing can disagree with the model.
* Good, because the comparison is one `PartialEq` over a value, which catches a
  component added to an already-selected entity without a second mechanism.
* Bad, because the cost is proportional to the whole panel rather than to what
  changed.

### Reconcile row by row

* Good, because the cost is proportional to the change, which matters when the
  list is long.
* Good, because a row, and anything attached to it, survives a change elsewhere
  in the list.
* Bad, because it adds state whose bug is a stale row: row 4, arriving quietly,
  in a panel whose whole job is to say what is true.
* Bad, because it is more code than the thing it optimises is worth at fifteen
  rows.

### Rebuild every frame, no comparison

* Good, because it is the shortest, and it cannot show anything stale.
* Bad, because it hands #44 a panel destroyed under the cursor every frame, and
  because the cost is paid whether or not anything changed: row 5.

## More Information

* The code: `crates/editor/src/inspector.rs`, `show`.
* What is on screen, and why: [`docs/specs/ui.md` §6](../specs/ui.md).
* The same reasoning one level up, for the selection outline drawn every frame
  rather than kept: [`docs/specs/ui.md` §5](../specs/ui.md).
* Issue #42, where the options were put and chosen.
