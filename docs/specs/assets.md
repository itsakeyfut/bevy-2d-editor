# Assets

Working with Aseprite, and the asset processing pipeline.

---

## 1. `.aseprite` files are read directly

### Decision

`.aseprite` files are **read as they are.** Nobody is asked to export a PNG by
hand.

* **The editor and the runtime go through the same code.** The `runtime` crate
  carries Aseprite support and `editor` reaches it through `runtime`, the same
  arrangement as constraint 4 in [level-editor.md §2](./level-editor.md).
* It is implemented with **`bevy_aseprite_ultra` behind a default-on feature**,
  the same pattern as [level-editor.md §3](./level-editor.md).

```toml
# runtime/Cargo.toml
[features]
default = ["avian2d", "aseprite"]
aseprite = ["dep:bevy_aseprite_ultra"]
```

### Why this needed deciding

[concepts.md §3](../concepts.md) and [concepts.md §4](../concepts.md) build on
Aseprite, but **Bevy cannot read `.aseprite`**. The arrow in
[concepts.md §4](../concepts.md) from Aseprite to assets to the level editor had
nothing behind it.

As in [level-editor.md §2](./level-editor.md) and
[level-editor.md §3](./level-editor.md), this splits in two:

* **Editor side**: showing sprites in the asset browser and the viewport. That
  is our own dependency and touches no public contract.
* **Runtime side**: the user's game loading a sprite. That is the question.

### The ecosystem, as of September 2026

| Crate | Latest | Released | State |
| --- | --- | --- | --- |
| `bevy_aseprite_ultra` | 0.9.0 | 2026-06-20 | **alive**, on bevy ^0.19, 33k downloads |
| `aseprite-loader` | 0.4.2 | 2026-02-19 | **alive**, a pure parser with no Bevy dependency, 28k downloads |
| `asefile` | 0.3.8 | 2024-03-14 | stalled |
| `bevy_ase` | — | 2023-09-02 | stalled |
| `bevy_asepritesheet` | 0.6.0 | 2024-02-23 | stalled |

**There is effectively one living option.** `bevy_aseprite_ultra` is built on
`aseprite-loader`.

### Rejected options

**A. Require a PNG export**

No dependency and no implementation, but the arrow in
[concepts.md §4](../concepts.md) becomes manual work. Worse, **frame timings,
tags, slices and pivots are lost**, so they end up managed in a separate JSON
file or retyped in the editor.

As [level-editor.md §3](./level-editor.md) established, **not forcing a
dependency is no excuse for not working out of the box.** This fails the first
principle in [concepts.md §9](../concepts.md).

**C. Bake a PNG atlas and metadata at build time, the way Unity does**

**Not now. Not rejected either: it is the long-term goal, below.**

`.aseprite` becomes a source asset and the editor produces the artifact. It is
the cleanest answer and costs the runtime nothing, but **it needs an import
pipeline**, and it adds a build step that slows iteration. Not for the MVP.

**B can become C later.** C is additive, a bake step on top, and it breaks no
public contract. Doing C first would be the scope inflation
[concepts.md §8](../concepts.md) warns about.

### Long term: the asset processing pipeline

**The destination is C.**

This is ground Jackdaw has left open. Its
`book/src/developer-guide/open-challenges.md` says:

> ## Asset processing pipeline
>
> Asset processing happens only at editor runtime. To pre-process
> textures or bake meshes for a CI build, **you have to start the
> editor headlessly.**
>
> What is missing is a `process` step and the asset-processing pipeline
> behind it.

The two shapes it names:

1. split the user's game into a library plus several binaries (run, process) and
   drive processing from the project's own binary, which is invasive for the
   project template
2. add a `process` subcommand beside `build` in the CLI, which is less invasive
   but puts more code in the editor

> Where to dig in: pick one shape and prototype it against a small game.
> **We'd like to see the workflow before locking in the design.**

**This project intends to solve that ground itself.**

Being 2D-only and Bevy-only puts it in a better position than a general 3D
editor:

* there is less to process: `.aseprite` to a PNG atlas plus metadata, and tile
  data to a binary form. No mesh baking, no navmesh generation.
* by [runtime-and-play.md §1](./runtime-and-play.md), **the user's game is
  already built and launched as its own process.** The problem Jackdaw describes
  as needing a headless editor has a different shape here.
* by [data-model.md §4](./data-model.md), **tile data is already a file of its
  own.** The habit of separating a source asset from a produced one is already
  in place.

The move happens after phase 3, once play
([runtime-and-play.md §2](./runtime-and-play.md)) and the build pipeline work.
Establishing the experience of reading `.aseprite` directly, hot reload
included, first is what makes it possible to set **keeping that experience
through the pipeline** as a design requirement.

### Where it lives

The same arrangement as constraint 4 in
[level-editor.md §2](./level-editor.md), the editor and the runtime sharing one
implementation.

```text
runtime/  ← carries Aseprite support behind a feature
   ↑
editor/   ← reaches the same code through runtime
```

Better than the editor using `aseprite-loader` directly: **going through one
implementation is what guarantees the two show the same thing.**

### Accepted risk

* **One living crate means depending on effectively one author.** Keeping the
  seam at a feature boundary means that if it stops, the options are to (a) fork
  it, (b) write it here on top of `aseprite-loader`, or (c) move to C.
  **The feature boundary is the insurance.**
* `bevy_aseprite_ultra` will need version chasing, the same as avian2d in
  [level-editor.md §3](./level-editor.md).
