---
status: "accepted"
date: 2026-09-18
decision-makers: itsakeyfut
---

# Compose the editor from a table a test can read, not from Bevy's `plugin_group!`

## Context and Problem Statement

[architecture.md §3](../specs/architecture.md) decides that editor features are
added as plugins and composed as a compile-time `PluginGroup`. It does not say
how that group is written, and that only becomes a question with the engine in
front of you: Bevy ships a macro for exactly this, and whether to take it turns
on what its output can be asked afterwards.

The group is the interface every later phase writes into. `LevelEditorPlugin`,
`ScenarioEditorPlugin` and the rest each arrive as a line in it, so the shape
chosen here is the shape they inherit.

## Decision Drivers

* [architecture.md §5](../specs/architecture.md) says to find out whether the
  engine has a mechanism before building one. Bevy has one, so anything else is
  a deviation that has to pay for itself.
* A member that quietly leaves the group, or quietly joins it, must fail a named
  test. This is the shape the knowledge bank's RK-001 records, and it has
  recurred here rather than being a hazard read about somewhere else. How many
  times is not a number this record should assert: the entry's own headline and
  the incidents it narrates do not agree, and reconciling that belongs to the
  entry.
* Bevy 0.19.1's `PluginGroupBuilder` cannot be enumerated. `order` and `plugins`
  are private, there is no iterator, and `contains::<T>()` answers about one
  named type at a time. `App` is the same: `plugin_registry` is `pub(crate)` and
  the public readers all need the type spelled out.
* Failure lands on row 1 or row 2 of `CLAUDE.md`'s list either way: the editor
  is composed at compile time, so a mistake here does not compile or fails a
  test. Nothing about this choice can reach a saved document.

## Considered Options

* a `const` table of members, folded into a `PluginGroupBuilder`
* Bevy's `plugin_group!` macro

## Decision Outcome

Chosen option: the table, because the group has to be readable as a **set** and
`plugin_group!` produces one that is only answerable as a series of questions
about types the asker already knows.

With the macro, a test can say `EditorPlugins.build().contains::<PanelsPlugin>()`
for each plugin it happens to name. Nothing can say that those are all of them,
and nothing notices a member arriving with no test beside it. The table is read
by `EditorPlugins::build()` and by the tests, so both of those become ordinary
assertions on a value.

### Confirmation

`the_group_carries_the_members_the_table_names` in `crates/editor/src/lib.rs`
asserts the table's names against a literal spelled out in the test. Mutation:
add a row to `MEMBERS` and change nothing else, and it fails.

That the table is what composes the group, rather than a list kept beside one,
is held by `a_member_is_a_row_and_nothing_else`. Mutation: fold `&MEMBERS` in
`compose` instead of the argument it was handed, and it fails.

What is **not** guarded: that `editor` adds `EditorPlugins` at all. While the
table is empty the group registers nothing, and Bevy records a group's plugins
rather than the group, so there is no observable to assert against. The first
member makes it assertable, and this section is to be updated in that change.

### Consequences

* Good, because the group's membership is a value, so the tests read it instead
  of restating it, and a row that leaves or joins is a named failure.
* Good, because a row's name and its plugin come out of one token through
  `member!`, so the column a test reads cannot drift from the plugin it names.
* Bad, because it is a second mechanism standing beside one the engine already
  has, which is the thing [architecture.md §5](../specs/architecture.md) exists
  to discourage. It is about fifteen lines, and it is fifteen lines to keep in
  step with `PluginGroupBuilder` across Bevy upgrades.
* Bad, because `plugin_group!` generates a documented member list and the table
  does not. The doc comment on `MEMBERS` is what stands in for it.
* What would reverse this: Bevy exposing the group's membership, by an iterator
  over `PluginGroupBuilder` or by a public reader on `App`'s plugin registry. At
  that point `plugin_group!` answers the set question too and the table is a
  deviation with nothing left to buy.

## Pros and Cons of the Options

### a `const` table of members

* Good, because the set can be asserted whole: names, order and count.
* Good, because `compose` takes the table as an argument, so the columns can be
  exercised while the table is still empty. A table whose rows are all empty
  otherwise exercises none of its columns, which is RK-001's newest instance.
* Bad, because it is ours to maintain, and because `stringify!` is a weaker name
  than `core::any::type_name`, which is not a `const fn` and so cannot be a
  column of a `const` table.

### Bevy's `plugin_group!`

* Good, because it is the engine's own mechanism, generates documentation of the
  member list, and is what a reader arriving from Bevy already knows.
* Good, because it supports feature-gated and nested members, which this will
  eventually want.
* Bad, because the group it produces cannot be read as a set, so the first
  acceptance criterion of #21 cannot be met with it.
* Bad, because every member must implement `Default`, which is a constraint on
  plugins that have not been written yet.

## More Information

* [architecture.md §3](../specs/architecture.md), the decision this serves, and
  [§5](../specs/architecture.md) and [§7](../specs/architecture.md), which say
  to prefer the engine's mechanisms and rule out dynamic loading.
* `crates/editor/src/lib.rs`: `Member`, `member!`, `MEMBERS`, `compose` and
  `EditorPlugins`.
* Bevy 0.19.1, `bevy_app/src/plugin_group.rs`: the macro, and the private
  `order` and `plugins` fields that this record turns on.
