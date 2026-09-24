---
status: "accepted"
date: 2026-09-23
decision-makers: the project's owner, during `/spec` on #42
---

# Rebuild the inspector's panel rather than reconcile it, and walk the entity with the engine's own call

## Context and Problem Statement

The inspector is the first UI in this workspace whose content is not fixed when
it is spawned. Everything before it, the five regions in
`crates/editor/src/lib.rs`, is written once at startup and never changes; this
panel changes every time the selection does, and its length changes with it.

Two shapes answer that, and they differ in what they keep: a rebuild keeps
nothing and a reconcile keeps a row per component. The same change also has to
choose how the entity is walked, because Bevy offers the walk in two places.

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
* Nothing here is large. Fourteen rows on a placeholder, measured.

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

**The walk is `World::inspect_entity`, which is the engine's own.** It returns
`Result<impl Iterator<Item = &ComponentInfo>, EntityNotSpawnedError>`, the `Err`
covering a despawned entity, and it is exactly what this needs.
[architecture.md §5](../specs/architecture.md) says to find out whether the
engine has a mechanism before building one, and it has this one.

**The first version of this record said otherwise, and was wrong.** It claimed
that `&World` would make this "the only exclusive system in the editor" and that
`EntityRef::archetype().components()` with `&Components` beside it would let the
schedule run this in parallel where `&World` would not. A reviewer checked both
halves against `bevy_ecs` 0.19.1 and neither holds:

* Bevy reserves "exclusive system" for a system taking `&mut World`
  (`src/system/exclusive_system_param.rs`). A `&World` parameter is an ordinary
  `ReadOnlySystemParam`.
* `&World` and `Query<EntityRef>` have **the same** access footprint. `&World`'s
  `init_access` calls `filtered_access.read_all()`
  (`src/system/system_param.rs`), and `EntityRef`'s `update_component_access`
  calls `access.read_all()` (`src/query/fetch.rs`). Neither can run beside a
  system that writes anything, and neither is better off than the other.

So the reason recorded here bought nothing, and what it was hiding is that the
engine's own call is both shorter and the one
[architecture.md §5](../specs/architecture.md) asks for.
`show` went from seven parameters to four. This is kept rather than deleted
because the wrong reason is the part worth knowing: it was plausible, it was
written as if measured, and it took a reader with no stake in it to check.

### Confirmation

`the_inspector_does_not_rebuild_what_has_not_changed` in
`crates/editor/src/inspector.rs`. It reads the entity ids under the panel,
updates twice with nothing changed, and asserts the ids are the same: a rebuild
despawns them, so surviving ids are a panel that was left alone.

**Mutation: delete the `shown.0 == wanted` guard in `show`.** Applied, and that
test failed while the other eleven in the module passed. Measured.

The opposite failure, a panel that never rebuilds, is held by
`choosing_a_different_entity_changes_what_is_shown`. Mutation: return early
whenever `Shown` already holds something. Applied: **seven** of the twelve
failed, that one among them.

**Nothing holds the choice of walk**, and nothing can: both spellings produce
the same panel, and the first version of this record proves that a reason
written here can be wrong for weeks without any test noticing. What guards it is
a reader checking the claim against the engine, which is what happened.

### Consequences

* Good, because there is no reconcile to get wrong, and therefore no stale row.
* Good, because the panel is left entirely alone while nothing has changed,
  which is what #44's input field needs.
* Bad, because every change rebuilds every row, so a panel of a thousand rows
  would rebuild a thousand rows to change one. Nothing is near that.
* Bad, because per-row state that survives a rebuild has nowhere to live. A
  half-typed value in #44's field is the first thing that will want it, and the
  answer then is to hold it outside the row rather than to start reconciling.
  **That prediction was tested and was wrong about the mechanism.** #44 needed
  nothing held outside the row: the widget keeps its own buffer, and what was
  needed was for `show` to leave the panel alone while the user is in it.
  [ADR-0003](./0003-the-inspector-panel-belongs-to-the-user-while-focus-is-in-it.md)
  is that decision, and it is a condition on this one rather than a reversal of
  it.
* What would reverse this: a panel where a rebuild is visible, either as a
  dropped frame or as something the user was interacting with disappearing.
  Scrolling and collapsing, both deferred in
  [`ui.md` §6](../specs/ui.md), were the expected first sign; the one that
  actually arrived was #44's input box, and it was answered by
  [ADR-0003](./0003-the-inspector-panel-belongs-to-the-user-while-focus-is-in-it.md)
  without reversing anything here.

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
* Bad, because it is more code than the thing it optimises is worth at fourteen
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
