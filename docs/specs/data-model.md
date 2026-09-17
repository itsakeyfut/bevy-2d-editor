# Data model and serialization

The Editor Model, BSN, reflection, and what gets written to disk.

---

## 1. Data architecture

The editor does not reach into the game's ECS and change it arbitrarily.

The shape:

```text
user action
     ↓
Editor Command
     ↓
Editor Model (LevelDocument / ScenarioDocument)
     ↓
serialization (BSN plus tile data files)
     ↓
synchronization with Bevy
     ↓
Runtime ECS
```

What the Editor Model actually is gets defined in **§5**. In short:

* a level's domain structure, its metadata, tile layers and entity list, is
  **typed**
* only the values of user-defined components stay as **BSN AST fragments**,
  because their types are not known at compile time here

Undo and redo are built around the same command.

```rust
trait EditorCommand: Send + Sync + 'static {
    fn execute(&mut self, world: &mut World);
    fn undo(&mut self, world: &mut World);
    fn description(&self) -> &str;
}
```

Because a command takes `&mut World`, one command can touch both the Editor
Model and the tile data. **There is one history, not two.**

---

## 2. BSN and scenes

### Where this stands, as of September 2026

Bevy 0.19 shipped the next-generation scene system, but **only the code-facing
half of it.**

**Shipped**

* the `bsn!` macro, for defining a scene in Rust
* the `Scene` trait, templates and patches, scene composition

**Not shipped**

* the `.bsn` file format
* an asset loader for it

From the release notes:

> while Bevy 0.19 technically supports scene assets, **we aren't yet shipping
> a first-party `.bsn` asset loader.** This release focuses on the code-driven
> workflow, and we plan to roll out the asset driven workflow in a future release.

Documentation still referred to a `.bsn` asset format, which is why issue #24309
was opened asking for warnings or removals; it was fixed in PR #24371 and
closed.

### What arrives in 0.20

PR **bevyengine/bevy#23576, "Dynamic BSN", is merged**, with milestone **0.20**.
It is a runtime asset loader for `.bsn`, with its own lexer, parser and AST,
producing `ScenePatch` assets. It supports entity references through `#Name`,
inheritance through base includes, and hot reload.

Jackdaw's `jackdaw_bsn` tracks that upstream work explicitly:

> The grammar rules track the dynamic-BSN work in bevyengine/bevy#23576.

### What this project does

BSN is adopted. **But the editor's parser, AST and emitter are written here now,
with Jackdaw as the reference, rather than waiting for 0.20.**

The reasoning is in **§4**. In short:

* the official loader is **read-only**, and does not give the editor the round
  trip it needs: `.bsn` to an editable AST and back
* so the editor side needs its own; the runtime side can likely move onto the
  official loader
* tile data is not written into BSN at all. It is a separate asset, referenced
  by path (§4)

Still being tracked:

* entity serialization
* component serialization
* reflection
* asset references
* editing a scene
* default values
* runtime synchronization

---

## 3. Reflection

Reflection is one of the editor's central technologies.

What it is for:

```text
Component
    ↓
Reflection
    ↓
Inspector
```

A component defined in the user's game is recognised and edited by the editor.

The shape of it:

```rust
#[derive(Component, Reflect)]
struct Enemy {
    health: f32,
    speed: f32,
}
```

becomes, in the inspector:

```text
Enemy
├── Health: 100
└── Speed: 3.5
```

This is what makes the editor work with components it has never heard of.

---

## 4. The data format is BSN, with the editor's parser written here

### Decision

**The data format for scenes and levels is BSN, as text.**

The editor's parser, AST and emitter are **written here, with Jackdaw as the
reference.** No serialization format of our own, such as RON.

**Tile data is not written into BSN.** It is split into its own asset and
referenced by path.

### Where BSN stands

By §2, as of September 2026 Bevy 0.19 ships only the `bsn!` macro, and
**neither the `.bsn` file format nor a loader for it exists.**

Meanwhile PR **bevyengine/bevy#23576, "Dynamic BSN", is merged** with milestone
**0.20**, bringing a runtime asset loader.

Jackdaw's `jackdaw_bsn` tracks that work explicitly:

> The grammar rules track the dynamic-BSN work in bevyengine/bevy#23576.

So using Jackdaw as a reference is more than having an example to read: **it is
how this project rides the upstream grammar.**

### Why not wait for the official loader

**It is read-only, which is not what an editor needs.**

```text
runtime side   .bsn → ScenePatch → spawn        ← 0.20's loader is enough
editor side    .bsn → an editable AST → .bsn    ← has to be written here
```

Jackdaw states the same reason in `jackdaw_bsn/src/lib.rs`:

> The parser builds the editor document (`SceneBsnAst`) **directly from `.bsn`
> source text; there is no separate parse-time representation.**

An editor needs the round trip: parse, edit, write back. What the official
loader produces is a `ScenePatch` to spawn from, not a document to edit.

That division lines up with the editor and runtime split in
[runtime-and-play.md §1](./runtime-and-play.md).
**The `runtime` crate will likely not need a BSN loader of its own.**

That does not make the whole public contract thin. By
[level-editor.md §2](./level-editor.md), `runtime` keeps at least:

* chunk management and layers, the layer written on top of `TilemapChunk`,
  estimated at one to three thousand lines, because the game draws levels too
* a loader for the tile data file, since the format is ours
* runtime components and systems for levels, triggers, spawn points and the rest

**None of that goes away** when the BSN loader is replaced by an official one.

### The grammar

```text
#Root
bevy_transform::components::transform::Transform
bevy_camera::visibility::Visibility::Visible
bevy_ecs::hierarchy::Children [
    #"Main Camera"
    bevy_camera::components::Camera3d
    bevy_transform::components::transform::Transform {
        translation: glam::Vec3 { x: 0.0, y: 6.0, z: 12.0 },
    }
]
```

* `#Name` is an entity's name, its `Name` component
* a bare type path is a component at its default value
* `Type { field: value }` sets fields; anything omitted keeps its default
* `Type::Variant` is an enum, `Type(value)` a tuple struct
* `Children [ .. ]` nests child entities

A component's key is its full type path, the same string the inspector shows as
"type path". Values are whatever Bevy's reflection produces for that type.

### Scope of the implementation

Jackdaw's `jackdaw_bsn` measures **9,289 lines of source and 3,604 of tests**.
A clear part of it is not needed here.

| Module | Lines | Verdict |
| --- | ---: | --- |
| `document/` | 1,601 | **needed**, the editable AST |
| `parse/` | 561 | **needed**, the parser |
| `apply.rs` | 2,972 | **partly needed**; the 3D, material and animation specifics drop out |
| `emitter.rs` | 515 | **needed**, writing text back |
| `writer.rs` | 423 | **needed**, World to AST |
| `loader.rs` | 88 | **needed** |
| `catalog.rs` | 906 | **not needed**, Jackdaw's legacy `@Name` migration |
| `retired.rs` | 122 | **not needed**, Jackdaw-specific legacy handling |
| `binary.rs` | 480 | later, the `.bsb` binary form |
| `delta.rs` | 351 | later, phase 3 |
| `sync.rs` | 338 | later, phase 3 |
| `header.rs` | 517 | partly later |

**The realistic target for phases 1 and 2 is three to four thousand lines.**

### Tile data

BSN is a list of components per entity, so writing a 100 by 100 tile layer
directly puts ten thousand array elements into text.

Three options were weighed:

* **(a) write it as a `Vec<u32>`.** Direct, but tens of kilobytes for 100 by
  100, and `git diff` stops meaning anything
* **(b) hold it as a component whose value is a Base64 string.** Small, but
  unreadable, against "stays inspectable" in
  [architecture.md §6](./architecture.md)
* **(c) split the tile layer into its own file and reference it by path from
  BSN** ← **chosen**

Why (c):

* it draws a clear line: BSN holds the level's structure and entity placement,
  and the tile data is a separate asset
* it is the same arrangement Jackdaw uses for materials and animation graphs, so
  the reference implementation applies directly
* it leaves room to make only the tile data binary later. That matches Jackdaw's
  reason for building its `.bsb` binary twin: a file too large for anyone to
  diff
* the structural part stays small, so `git diff` keeps working
  ([architecture.md §6](./architecture.md))

```text
assets/levels/forest.bsn        ← structure: entities, triggers, camera
assets/levels/forest.tiles      ← the tile data, referenced by path from the BSN
```

### Accepted risk

* **The upstream grammar can still move.** #23576 is merged but 0.20 is not
  released, so details can change. Jackdaw carries the same risk and tracks it,
  so following Jackdaw absorbs most of it.
* **When 0.20 lands, the runtime side has to move onto the official loader.**
  That is work in the direction of less code here, not debt.
* The tile data format itself still needs defining. That is a phase 2 question.

---

## 5. The Editor Model: a typed shell holding AST fragments

### Decision

The Editor Model is a **typed `LevelDocument` and `ScenarioDocument`**. The BSN
AST is not used as the Editor Model directly, which is where this departs from
Jackdaw.

**Only the values of user-defined components stay as BSN AST fragments.**

```rust
// in the data crate
struct LevelDocument {
    meta:     LevelMeta,               // typed
    layers:   Vec<TileLayer>,          // typed
    entities: Vec<EntityNode>,
}

struct EntityNode {
    name:       String,                // typed
    transform:  Transform,             // typed
    components: Vec<BsnPatch>,         // ← the only untyped part
}
```

Loading projects the BSN AST into a `LevelDocument`; saving writes it back.

### The contradiction this resolves

§1 and [architecture.md §6](./architecture.md) describe a flow of
**Editor Command, Editor Model, serialized data**, in which the model and the
serialization format are different things.

The Jackdaw approach adopted in §4 denies that premise:

> The parser builds the editor document (`SceneBsnAst`) directly from `.bsn`
> source text; **there is no separate parse-time representation.**

In Jackdaw, **the BSN AST is the Editor Model**; there is no intermediate typed
model. And since §4 (c) moved tile data outside BSN, the result would have been
two models side by side: an AST, and a typed model for tiles.

This section removes that.

### The part that has to stay dynamic

§3 requires that user-defined components be editable in the inspector. A user's
`Enemy { health: f32 }` has no type known when this project compiles, so it can
only be held as a dynamic reflected value.

There is therefore no "make it all typed" option. **The choice is only how far
the dynamic part extends.**

### Rejected options

**A. The AST alone, as Jackdaw does it**

The BSN AST is the Editor Model, and level structure, entities and components
are all untyped nodes.

* Jackdaw's `document/` (1,601 lines) and `parse/` (561) transfer almost
  unchanged
* comments and formatting in the original file survive

Why it is still not taken:

* **It disagrees with [level-editor.md §2](./level-editor.md) and §4(c).** Both
  were decided on the grounds of not subordinating our domain structure to a
  general-purpose format. Having already moved tile data out as typed data,
  there is no reason to leave level structure untyped.
* **A level here is not the same kind of thing as a Jackdaw scene.** Jackdaw is
  a general 3D scene editor, and a scene has no structure beyond entities and
  components; an AST is exactly the right model for that. A level here has a
  fixed shape: tile layers, triggers, spawn points, a camera.
* "Get this level's tile layers" becomes a walk over an untyped tree: not
  `level.layers` but "find the child nodes whose type path is `TileLayerRef`".
  **Mistakes stop being compile errors.**
* The `data` crate from [crates.md §2](./crates.md) would be almost empty, which
  would mean revisiting the five-crate layout.

**B. A fully typed model**

Ruled out by the premise above. Because user-defined component types are not
known statically, it collapses into the hybrid anyway.

### What this means for undo

The original concern, that two models would break undo, **was wrong.**

Jackdaw's `EditorCommand` takes `&mut World`.

```rust
trait EditorCommand: Send + Sync + 'static {
    fn execute(&mut self, world: &mut World);
    fn undo(&mut self, world: &mut World);
    fn description(&self) -> &str;
}
```

With the Editor Model and the tile data both as resources in the World, one
command reaches both, and **one history is enough.**

This decision also lets a command operate on `level.layers[i]` directly, which
is what most of the level editor's code will be written against.

### Accepted risk

* **The projection layer is ours to write.** Converting between the BSN AST and
  `LevelDocument` in both directions, estimated at five hundred to a thousand
  lines. Jackdaw has no equivalent.
* **Comments and formatting are lost on save**, because the output comes from
  the types rather than from the AST. What
  [architecture.md §6](./architecture.md) asks for is serializable, inspectable
  and version-controllable, not preserved hand editing, and
  **`git diff` works as long as the output is deterministic**. As Jackdaw does,
  the leading comment on a file, saying the editor generated it, is preserved.
