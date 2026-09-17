# Crates and the dependency direction

The workspace layout, the bar for splitting a crate, and the dependency graph.

---

## 1. Workspace layout

Early on, the workspace is split as little as it can be. A crate appears when a
boundary has proved itself in the implementation.

```text
project/
├── crates/
│   ├── editor/      # UI, tools, everything
│   ├── editor_ui/   # generic UI widgets
│   ├── runtime/     # the only crate a user's game depends on
│   ├── data/        # level and scenario types, and serialization
│   └── core/        # pure logic with no Bevy dependency
│
├── book/
├── examples/
├── assets/
└── Cargo.toml
```

* why it is five crates and not more: **§2**
* the dependency direction and the feature split: **§3**
* the test for whether something has earned its own crate is in §3 too:
  anything with no place in the dependency graph is not a crate yet

---

## 2. Start with the fewest crates that work

### Decision

Split **as little as possible** at the start, and split when a boundary has been
demonstrated. Neither Jackdaw's granularity (48 crates) nor Bevy's own is
copied.

### Rationale

**Bevy's reasons for splitting do not apply here.** `bevy_*` is fine-grained
because it is a published library with feature gates, so that a 2D game does not
compile PBR. An application has no such concern.

**Jackdaw is not over-split; it is split in the wrong place.** Measured:

```text
185,044 lines  ← the editor itself, src/, one crate, over 100 files
139,xxx lines  ← all 48 crates in crates/ put together
```

The largest single thing is not a crate at all. Jackdaw's own
`crate-structure.md` names breaking up `src/` as outstanding work.

**There are only four things a crate boundary buys:**

1. the compiler enforcing the dependency direction, so a cycle is an error
2. a contract about what a downstream consumer pulls in
3. incremental build time
4. being testable without a renderer

"For tidiness" is not one of them. That is what modules are for.

**What Jackdaw pays for its split:**

* a types-only crate like `jackdaw_scene_types` is a symptom: it exists to break
  a dependency cycle
* all 48 carry `version.workspace = true` at `0.19.0`, so independent releases,
  one of the things a split is supposed to buy, are worth nothing
* the orphan rule means separating types from impls breeds newtype wrappers

### The initial layout

The one exception is **splitting the editor from the runtime**. That is not
tidiness: it is the external contract for what a user's game writes in its
`Cargo.toml`, and carving it out later would break a published API. It is needed
on day one.

```text
crates/
├── editor/      # UI, tools, everything: a monolith meant to be split later
├── editor_ui/   # the equivalent of widgets + feathers + panels, as one
├── runtime/     # the only crate a user's game depends on
├── data/        # level and scenario types, and serialization
└── core/        # pure logic with no Bevy dependency: grid snapping, tile coordinates, geometry
```

**The dependency direction is fixed in §3.**

Jackdaw's three layers, `widgets`, `feathers` and `panels`, are **the right
layering and do not need to be separate crates**: `jackdaw_widgets` is 1,588
lines. Three modules inside `editor_ui` keep the same discipline.

When `editor/` grows large enough that rebuilds hurt, that is the moment to do
what Jackdaw has not: **carve up the middle.**

---

## 3. The dependency direction

### Decision

The graph is fixed:

```text
              editor
             ╱      ╲
      editor_ui    runtime
             ╲      ╱
               data
                │
               core
                │
          (no Bevy dependency)
```

```text
core      → nothing, not even Bevy
data      → core, bevy
runtime   → data, core, bevy
editor_ui → bevy, bevy_feathers        (not data, not runtime)
editor    → runtime, data, core, editor_ui, bevy
```

The physics adapter, `avian2d`, enters as a feature of `runtime`
([level-editor.md §3](./level-editor.md)). A future adapter such as
`runtime_myphysics` sits beside `runtime` the same way.

Alongside that, `data` **puts its editor-only parts behind a feature**, so a
user's game compiles only what loading needs.

### Why this is needed

§2 lists "the compiler enforcing the dependency direction" first among the
things a crate boundary buys, **and then did not define the graph.** Without it
the main purpose of splitting is not achieved.

### Why the graph is forced

**Constraint 1: `runtime` is the only crate a user's game depends on** (§2)

So `runtime` cannot depend on `editor` or on `editor_ui`. That is absolute.

**Constraint 2: `core` has no Bevy dependency** (§2)

It depends on nothing. Being testable without a renderer is its reason to exist.

**Constraint 3: `data` is used by both the editor and the runtime**

It holds `LevelDocument` ([data-model.md §5](./data-model.md)), BSN
serialization, and the tile data format.

**Constraint 4: the editor and the runtime must share the tile rendering code**
([level-editor.md §2](./level-editor.md))

Having decided that the two use the same renderer, the chunk management and
layer handling written on top of `TilemapChunk` have to be one implementation,
or what the editor shows is not what the game draws.

Constraint 4 forces **`editor → runtime`**. The other direction breaks
constraint 1. Jackdaw has the same arrangement:

> The editor depends on it (rather than the other way around) so the trait
> is reachable from a game crate that only pulls in `jackdaw_runtime`.

### `editor_ui` does not depend on `data`

This is deliberate.

`editor_ui` is the generic widget layer, split panel, tree view, inspector field
and dock, and **it knows nothing about levels or scenarios.** If it did, the UI
would know the model, and
[architecture.md §6](./architecture.md), that the editor UI must not become the
source of truth, would start to give way.

`editor` is the only place the two meet.

### The feature split in `data`

A user's game depends on `data` through `runtime`. `LevelDocument`, BSN
serialization and the tile data format all end up in their build.

Measured against the "keep the public contract thin" line in
[level-editor.md §2](./level-editor.md) and
[data-model.md §4](./data-model.md), that is not thin. **There is no way around
it:** the game has to read a level and draw it, which takes the types and a
loader.

What can be chosen is whether the editor-only parts of `data` sit behind a
feature.

**Rejected: do not split it**

One `data`, with the user's game compiling the BSN parser and emitter too.
Simpler to manage, but a game has no use for code that writes BSN.

**Chosen: split it with a feature**

Turning off the `data/editor` feature leaves the minimum needed to load. The BSN
emitter, and the editing half of the projection layer, are not needed in a game.

Why:

* it genuinely thins what the game carries
* **it makes moving the runtime onto Bevy 0.20's official `.bsn` loader easier**,
  because what would move is already separated (see
  [data-model.md §4](./data-model.md))
* the cost is managing `#[cfg(feature = ...)]`, which is lighter than another
  crate, and it agrees with §2's "split when it is earned"

### How this is used

* **A `use` against this direction is a compile error.** That is the reason the
  crates are split at all; when in doubt, follow the graph.
* Adding a crate starts by deciding where in this graph it goes. **Anything with
  no place in it is not a crate yet** (§2).
* **The graph is about `crates/`.** A package outside it, such as `xtask/`, is a
  tool the workspace builds itself with rather than part of what is built, and
  it has no place in the graph on purpose. That is not an exception to the rule
  above, it is the rule's boundary, and
  `crates/editor/tests/dependency_direction.rs` is where the boundary is drawn:
  `is_under_crates` decides which packages the graph governs.
* **What a user's game may carry is a list, not a diagram.** The table above is
  the decision; `SHIPPED` in `crates/editor/tests/dependency_direction.rs` is
  where it is enforced, for the crates a game reaches from the entry points
  that file lists: the name of each dependency, whether it is optional, and the
  weight it is pulled in with, which is its selected features and whether
  `default` is on. **Internal edges count**: `runtime -> data` is where a game's
  copy of the feature split above is decided, and a list that skipped internal
  edges could not see it turned back on. What the list holds is the **declared**
  edge; the configuration a game compiles is a different failure, and the `game`
  row of `cargo xtask gate` is what catches it, by running `cargo check` on each
  of those entry points so that `data`'s editor-only code being reachable from
  `runtime` fails here rather than in somebody's project. What neither holds is
  what a crate's own `default` feature
  contains; that is decided by the change that brings `avian2d` in, which is
  where the first feature table arrives.

---

## 4. A package name is not a directory name

### Decision

The directories keep the names §1 gives them. **The packages carry a `b2d_`
prefix.**

| Directory | Package |
| --- | --- |
| `crates/core/` | `b2d_core` |
| `crates/data/` | `b2d_data` |
| `crates/runtime/` | `b2d_runtime` |
| `crates/editor_ui/` | `b2d_editor_ui` |
| `crates/editor/` | `b2d_editor` |

### Why the directory names cannot be the package names

**A package called `core` shadows the sysroot crate of the same name**, in every
crate that depends on it. It is not a warning and it is not caught by a lint:

```text
error[E0433]: cannot find `mem` in `core`
 --> crates/data/src/lib.rs:2:11
  |
2 |     core::mem::size_of::<u32>()
  |           ^^^ could not find `mem` in `core`
```

Nothing in an empty crate says `core::`, so a workspace of five empty crates
builds green and the first person to write `core::mem` finds this instead.

### Why a prefix, and why this one

§3 makes `runtime` the only crate a user's game depends on. Publishing it means
publishing everything it reaches by path, so `b2d_runtime`, `b2d_data` and
`b2d_core` are all names this project has to hold on crates.io. `runtime`,
`data` and `core` are not names anyone can hold there.

`b2d_` rather than something longer because **it is mechanically renameable.**
This project has no name of its own the way Jackdaw does, and if it takes one,
a single regular expression over the tree moves every package, every path
dependency and every `use`. A prefix that reads as a description, such as
`bevy_2d_editor_`, is the same work and produces `bevy_2d_editor_editor_ui`
along the way.

### Consequences

* Directory `core`, package `b2d_core`, and `use b2d_core::` in the code. The
  name in a path is the package name, always.
* The binary is `b2d_editor`, from the package name, until somebody decides what
  the command should be called. Nothing is published, so that is free to change.
* §1's diagram is directory names. **Read it as directory names.**
