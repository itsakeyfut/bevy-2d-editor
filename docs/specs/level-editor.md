# Level editor

Editing the space a game world occupies: tilemaps, colliders, sprite animation.

---

## 1. Level editor

The editor for building a 2D game's world.

### MVP

* tilemap
* tileset
* painting tiles
* placing sprites
* placing entities
* transforms
* layers
* entity hierarchy
* inspector
* colliders
* triggers
* spawn points
* camera
* save and load
* undo and redo
* preview

### The workflow it is for

```text
create a project
    ↓
bring in assets
    ↓
create a level
    ↓
paint the tilemap
    ↓
place sprites
    ↓
place entities
    ↓
place colliders and triggers
    ↓
save
    ↓
run it
```

---

## 2. Tilemaps ride on Bevy's own `TilemapChunk`

### Decision

**`TilemapChunk`**, from Bevy's own `bevy_sprite_render`, is the rendering
foundation. No third-party tilemap crate.

Chunk management, layers and tile picking are written here.

**The editor and the runtime use the same renderer.** That is what guarantees
what you see is what you get (principle 3 in
[concepts.md §9](../concepts.md)).

### The two halves of the question

This splits in two.

1. **Editor side**: what draws tiles in the viewport.
2. **Runtime side**: what draws tiles in the user's game.

By [runtime-and-play.md §1](./runtime-and-play.md) the game is the user's own
Bevy binary in its own process, so **the runtime side's choice becomes a public
contract forced into every user's `Cargo.toml`.**

Also, **the tile data model is ours whichever option is taken.** By
[architecture.md §6](./architecture.md), the editor UI is not the source of
truth, and by [data-model.md §1](./data-model.md) undo is command-based, so the
truth about tile placement lives in the Editor Model. What an existing crate
could supply is **only the rendering.**

### What Bevy provides, as of 0.19.1

```text
TilemapChunk              one chunk is one mesh
TilemapChunkTileData      the tiles in a chunk; None is an empty tile
TileData                  one tile
TileOrientation           every combination of mirroring and 90-degree rotation
TilemapChunkMaterial      the material
TilemapChunkMeshCache     a mesh cache, keyed by chunk size
TilemapChunkPlugin
PackedTileData
```

The documentation calls this **"a building block"**, designed as the foundation
third-party tilemap crates sit on. What this project intends to do is exactly
that: build a 2D authoring layer on top of it.

`TileOrientation` already carries mirroring and rotation, which covers most of
what a 2D editor needs to express.

### Rejected options

**B. `bevy_ecs_tilemap` (0.19.0, 2026-07-04, alive and the de facto standard)**

Layers, sparse maps, GPU animation, isometric and hex grids, all present from
the start, with by far the most mileage. Still not taken, because:

* **it forces a third-party dependency on every user's game.** For the same
  reason [crates.md §2](./crates.md) insists the editor and runtime split is
  needed on day one, putting somebody else's crate into the public contract is
  the decision to be most careful about
* **most of what it offers cannot be used here.** Its value is the pairing of a
  data model around `TileStorage` with a renderer, and the data model has to be
  ours. Wanting only the renderer means **taking on one entity per tile as
  well**, and then maintaining two models
* **one entity per tile is heavy.** At Hollow Knight scale, tens to hundreds of
  thousands of tiles, it costs
* now that `TilemapChunk` is in Bevy itself, this crate's own future position is
  unclear

**C. Write the renderer too**

With `TilemapChunk` in Bevy, that is reinventing a wheel.

### Accepted risk

* **Chunk management, layers and tile picking are ours to write**, an estimated
  one to three thousand lines.
* **Isometric and hex grids are out.** `TilemapChunk` assumes a square grid.
  Everything [concepts.md §2](../concepts.md) targets, Hollow Knight-shaped
  action, metroidvanias, platformers, top-down action and adventure games, works
  on a square grid, but **taking isometric on later means revisiting this.**
* `TilemapChunk` is newer than `bevy_ecs_tilemap` and has seen less use.

### LDtk and Tiled

A separate question. `bevy_ecs_tiled` and `bevy_ecs_ldtk` exist to load maps
made in LDtk or Tiled, and **this project is the authoring tool**, so neither is
a foundation.

Importing from LDtk may be worth adding later. It is not part of the phase 2
decision.

### The ecosystem, as of September 2026

| Crate | Latest | Released | State |
| --- | --- | --- | --- |
| Bevy's `TilemapChunk` | Bevy 0.19.1 | 2026-08-13 | ships in `bevy_sprite_render` |
| `bevy_ecs_tilemap` | 0.19.0 | 2026-07-04 | alive, the de facto standard, 277k downloads |
| `bevy_ecs_tiled` | 0.13.4 | 2026-08-24 | alive, built on `bevy_ecs_tilemap` |
| `bevy_ecs_ldtk` | 0.15.0 | 2026-07-05 | alive, the same |
| `bevy_fast_tilemap` | 0.8.1 | 2024-10-27 | stalled |
| `bevy_sparse_tilemap` | 0.4.0 | 2024-12-10 | stalled |

---

## 3. Colliders: a physics-neutral shape, avian2d behind a default-on feature

### Decision

**The shape type carries no physics dependency** and lives in the `data` crate.

```rust
// in the data crate
enum ColliderShape {
    Rect    { half: Vec2 },
    Circle  { radius: f32 },
    Polygon { points: Vec<Vec2> },
    TilemapAuto { layer: LayerId },   // generated from a tile layer
}
```

**The avian2d adapter is a feature of `runtime`, on by default.**

```toml
# runtime/Cargo.toml
[features]
default = ["avian2d"]        # works with no configuration
avian2d = ["dep:avian2d"]
```

The editor edits, saves and draws gizmos for shapes. Turning a `ColliderShape`
into an `avian2d::Collider` is the adapter's job.

### Why this needed deciding

Colliders appear as a feature in [concepts.md §2](../concepts.md), in §1's MVP
and in phase 2 of [roadmap.md §3](./roadmap.md), but **nothing said how they
would be implemented.**

It is the same problem as §2, in the same shape:

> it forces a third-party dependency on every user's game. Putting somebody
> else's crate into the public contract is the decision to be most careful about

A physics engine is more invasive than a tilemap. Adopting one forces every
user's game onto it, and a project already using a different one could not use
this editor at all.

And **Bevy has no built-in physics.** There is no `TilemapChunk`-shaped escape.

**Jackdaw is no help here.** It adopted `avian3d` and has a
`jackdaw_avian_integration` crate, which is a decision that contradicts the
criterion §2 used.

### Rejected options

**A. Hold shapes only, and name no physics engine**

Forces no dependency and agrees completely with §2's criterion, but **colliders
do not work until the user writes the conversion themselves.** Placing a
collider and falling through it fails principle 1 of
[concepts.md §9](../concepts.md).

§2's criterion, not forcing a dependency, is worth keeping, but it is
**no excuse for not working out of the box.**

**B. Put avian2d directly into `data` and `runtime`, as Jackdaw does**

Works immediately and is kind to beginners, but the shape type is then tied to
`avian2d::Collider`, so **swapping the physics engine later breaks the public
contract.**

What is decided here keeps B's experience and removes only that weakness. The
amount of avian2d conversion code is the same either way. What differs is
**where it lives, and whether the shape type is neutral.**

### Why this was chosen

* **The experience is B's.** Add `runtime` and colliders work. No configuration.
* **It can be swapped.** `default-features = false` and an adapter of your own.
* **A physics engine written here later can be moved to.** Add something like
  `runtime_myphysics`, change the default, and no existing game breaks.
* Jackdaw uses the same pattern, with `multiplayer` and `camera_rig` as
  default-on optional features. It is also the mechanism
  [crates.md §3](./crates.md) uses to split `data`.

### On writing a physics engine here

The owner of this project intends to write one eventually.

That touches "no general-purpose game engine" in
[concepts.md §8](../concepts.md), so **it is not built into the editor. It is a
separate project, reached through an adapter.** Which is another reason to keep
the shape type neutral.

### Doing it in stages

None of this has to be built at once.

* **Phase 2**: the shape data (`ColliderShape`), gizmos, editing, saving.
  Physics-neutral geometry can live in the `core` crate, which has no Bevy
  dependency.
* **Phase 3**: the avian2d adapter.

As long as the shape type stays physics-neutral, adding the adapter later breaks
no public contract.

### Accepted risk

* **avian2d has to be kept up with**, and unlike Bevy itself that is not in our
  control. The feature boundary softens it: a period where it cannot be followed
  is survivable with `default-features = false`.
* The adapter has a maintenance cost.

---

## 4. Sprite animation: playback and tag selection in phase 2, editing in phase 10

### Decision

Animation splits into three, and **(1) and (2) go into phase 2.**

| | What | When |
| --- | --- | --- |
| (1) | playback in the viewport | **phase 2** |
| (2) | choosing a tag in the inspector | **phase 2** |
| (3) | editing animations, state machines | phase 10 |

```rust
// in the data crate; the same thinking as the neutral shape type in §3
struct SpriteAnimation {
    source:  AssetPath,      // "enemies/slime.aseprite"
    tag:     Option<String>, // "walk"
    playing: bool,           // whether it plays in the editor
}
```

**Playback in the viewport is a toggle.**

* **while editing**: still, on the first frame
* **in preview** ([runtime-and-play.md §2](./runtime-and-play.md)): playing

Playing constantly gets in the way of placing things. Unity treats it the same
way.

### Why this needed deciding

[concepts.md §2](../concepts.md) lists animation among what an action game
needs, and [concepts.md §4](../concepts.md) says Aseprite is where animation is
made. But **nothing about the editor's side appeared before phase 10's animation
editor.**

An enemy sitting in the viewport as a still image does not meet principle 3 of
[concepts.md §9](../concepts.md), that every edit shows its result immediately.

### [assets.md §1](./assets.md) already solves half of this

Because `.aseprite` files are read directly, **tags, frame timings and pivots
are already in hand.** `bevy_aseprite_ultra` carries playback itself: an
`AseAnimation` component, tag selection, and a completion event.

**No animation system has to be written.** What is left is how the editor uses
it.

### Rejected options

**A. Only (1) in phase 2**

Play constantly in the viewport, looping the default or first tag. The smallest
implementation, but "I want to see the walk cycle and it is playing idle" makes
it close to useless.

**C. All three at once**

A state machine is as large a thing as the scenario editor, and does not fit in
phase 2.

### Why this was chosen

Because Aseprite carries the tags, the editor's job is **listing the tag names
and letting one be chosen.**

**Making the animation is Aseprite's responsibility**
([concepts.md §4](../concepts.md)); the editor selects and places. That is the
division of labour §4 of concepts already describes.

### Relationship to [runtime-and-play.md §2](./runtime-and-play.md)

Preview was defined as tiles, sprites, camera and animation all moving, but
**nothing said what the animation part was.** This section is that.

---

## 5. Target genres, in stages

### Decision

**For now, the target is tile-based games on a square grid.**

**Hollow Knight-shaped games, built from hand-painted art, are a phase 9
question.** They are not dropped from the goal.

### Why this needed deciding

[concepts.md §2](../concepts.md) named "Hollow Knight-shaped 2D action" first
among its targets, while the MVP in [roadmap.md §1](./roadmap.md) is built
around painting tilemaps.

But **Hollow Knight is not a tilemap game.** Its backgrounds are hand-painted
art placed by hand, not a grid, and its collision is polygon colliders.

The "top-down action" also named in [concepts.md §2](../concepts.md) is usually
isometric, which §2 put out of scope.

**The sign on the door did not match what was inside.** Not a decision to
reverse: a question of what to promise.

### Rejected options

**A. Rewrite the target list to match what is buildable, and stop there**

Accurate, but **part of why this project exists disappears from the documents.**
Hollow Knight-shaped games are worth keeping as a long-term goal.

**B. Keep the targets and add the means to phase 2**

Aiming at Hollow Knight seriously takes freely placed sprite backgrounds and
hand-edited polygon colliders on top of tilemaps. Phase 2 inflates, against
[concepts.md §8](../concepts.md).

### Why this was chosen

Taking both means **describing what can be built now accurately, while keeping
the destination.**

It is the same treatment §2 gives isometric: recorded as something that would
require revisiting the decision.

### What a hand-painted workflow would need

Recorded now, as the starting point for phase 9.

* **backgrounds built from freely placed sprites**: layers, parallax, and
  managing a lot of placements. Different work from "placing sprites" in
  [roadmap.md §1](./roadmap.md)
* **hand-edited polygon colliders**: the type already exists as
  `ColliderShape::Polygon` in §3. What is missing is the editing UI and gizmos
* atlas management for large art, which connects to the long-term asset pipeline
  in [assets.md §1](./assets.md)

At the level of types, §3 has already opened the way.
**When building the tile-based version in phase 2, define `ColliderShape` with
the polygon variant included.** That makes phase 9 additive.
