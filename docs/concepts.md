# Concepts

What this editor is for, and what it will not become.
The specifications are in [specs/](./specs/).

---

## 1. Overview

A game authoring tool for Bevy, and for 2D.

The goal is not a level editor. It is

> **an environment for making 2D games in Bevy that a beginner can actually
> use.**

The idea, in one line:

> **Unity-like usability, Bevy-native architecture.**

A developer using Bevy should be able to edit

* 2D levels
* tilemaps
* sprites
* entities
* events and triggers
* dialogue
* scenarios
* scene transitions

visually, without first learning a lot of editor concepts, and run the result as
a Bevy game.

---

## 2. Goals

### What this is

* for Bevy
* for 2D
* written in Rust
* tightly integrated with Bevy
* usable by a beginner without preparation
* something that drops into a Bevy project naturally
* producing data a Bevy runtime uses as it is
* able to join levels and scenarios together
* extensible through plugins

### The games it is for

Not one genre. **2D games generally**, along three lines.

#### Action and platformers

**Phase 2's targets, tile-based on a square grid:**

* tile-based platformers, Celeste or Shovel Knight shaped
* tile-based metroidvanias
* top-down action on a square grid
* puzzle and action-puzzle games

**Phase 9 and later:**

* **Hollow Knight**, built from hand-painted art
  ([level-editor.md §5](./specs/level-editor.md))
* **Dead Cells**, procedural generation from room templates
* **Hades**, room-based and isometric, which would mean revisiting
  [level-editor.md §2](./specs/level-editor.md)

What they need:

* tilemaps
* colliders
* entities
* enemies
* NPCs
* triggers
* a camera
* spawn points
* scene transitions
* animation
* events
* room templates and connection points, for procedural generation

#### RPGs

For example:

* **Final Fantasy 5**
* **Dragon Quest 7**

What they need:

* tilemaps, for towns, dungeons and a world map
* **a database**: items, weapons, armour, spells, abilities, monsters, enemy
  groups, jobs, growth curves, states, shops, encounter tables
* **battle scenes**: background, enemy and party placement, command UI layout,
  staging
* menu UI layout
* a party, and stats
* **conditional dialogue**, on numbers as well as flags
* a great deal of scenario text

**An RPG is a database with a map editor attached**, not the other way round.

#### Scenario games, adventure games, visual novels

For example:

* **AIR**
* **CLANNAD**
* investigation and deduction adventures
* adventure games and visual novels generally

What they need:

* **a dedicated UI for writing scenarios**
  ([scenario-editor.md §2](./specs/scenario-editor.md))
* dialogue
* characters
* backgrounds
* choices
* branching, rejoining, returning to a choice
* flags and numeric variables
* events
* music
* sound effects
* scene transitions
* **managing a lot of text, and seeing the route structure**
* **a deduction mechanic**: collecting keywords or evidence and combining them,
  which the database expresses

A long adventure game's text is beyond what a node graph can hold. The prose is
the primary thing, and the graph is generated from it
([scenario-editor.md §2](./specs/scenario-editor.md)).

### What each target game needs

| Game | Level | Scenario | Database | Battle | Procedural |
| --- | :---: | :---: | :---: | :---: | :---: |
| Celeste, Shovel Knight | yes | — | — | — | — |
| a metroidvania | yes | yes | some | — | — |
| Hollow Knight | yes | yes | some | — | — |
| Dead Cells | yes | — | yes | — | yes |
| Hades | yes | yes | yes | — | yes |
| Final Fantasy 5 | yes | yes | yes | yes | — |
| Dragon Quest 7 | yes | yes | yes | yes | — |
| AIR, CLANNAD | some | yes | — | — | — |
| an investigation adventure | some | yes | yes | — | — |

**Six of the nine need a database.** After levels, it is the most important
thing to edit.

---

## 3. What this is not

This is not an attempt at a general-purpose game engine to replace Unity, Unreal
or Godot.

Bevy is the engine. What is built on top of it is

> **a 2D game authoring environment for Bevy.**

### Where it sits

```text
Aseprite
    ↓
sprites and pixel art

Bevy 2D Editor
    ↓
authoring levels and scenarios

Bevy
    ↓
the game runtime
```

It does not replace Aseprite.

It does not replace Blender.

It does not replace the engine.

Each keeps its own job.

---

## 4. The division of labour with Aseprite

Aseprite is the tool for sprites and pixel art.

```text
Aseprite
├── sprites
├── animation
├── tilesets
└── pixel art
```

The level editor assembles what it produces into a world.

```text
Aseprite
    ↓
.aseprite files          ← read directly by the editor (assets.md §1)
    ↓
Level Editor
    ↓
the game world
```

`.aseprite` files do not have to be exported to PNG by hand. Animation tags,
frame timings, slices and pivots are used as they are
([assets.md §1](./specs/assets.md)).

It is fine for a specialist tool like Aseprite to require learning.

The level editor holds itself to something else:

> **somebody who has just started making games can begin without studying the
> editor first.**

---

## 5. The relationship to Jackdaw

Jackdaw is the main technical reference.

Jackdaw:

```text
Bevy
  ↓
a general-purpose 3D editor
  ├── hierarchy
  ├── inspector
  ├── viewport
  ├── gizmos
  ├── BSN
  ├── reflection
  ├── undo / redo
  ├── geometry
  ├── terrain
  └── extensions
```

This project:

```text
Bevy
  ↓
a 2D game editor with a narrower job
  ├── level editor
  ├── scenario editor
  ├── tilemap
  ├── sprites
  ├── triggers
  ├── dialogue
  └── story
```

Copying Jackdaw is not the aim.

What is taken from it:

* the crate structure
* separating the editor from the runtime
* reflection
* BSN
* the plugin architecture
* the UI architecture
* undo and redo
* the remote protocol
* how extensions are arranged

---

## 6. The relationship to Bevy's official editor

### Where this stands, as of September 2026

**There is no official editor implementation.**

`bevyengine/bevy_editor_prototypes` was **archived on 2026-04-16 and is
read-only.** The vision, architecture, roadmap and Figma design it held are all
archived work.

The archive notice points two ways:

1. **Bevy itself**, where the foundations continue: BSN, UI, camera controllers
2. **Jackdaw**, named as the place to experiment with a working editor

### Decision

**Archived and prototype work is excluded as a reference for this project.**

Excluded:

* the `bevy_editor_prototypes` repository
* its vision, architecture and roadmap documents
* the official editor's Figma design

Usable:

* **the foundations that landed in Bevy itself**: BSN, reflection, the asset
  system, gizmos, Bevy UI, `bevy_feathers`
* **Jackdaw**, which is an implementation that actually runs
  ([ui.md §2](./specs/ui.md))

### The approach

Not "assume the official editor will develop and follow it". There is nothing
there to follow.

Instead, alignment comes from **living on the foundations Bevy itself
provides.** As long as the same foundations are used, data and assets stay
compatible with whatever official editor eventually appears.

```text
Bevy's own foundations
(BSN / reflection / assets / Bevy UI)
        ↓
a 2D-specific workflow
        ↓
authoring levels and scenarios
```

Competing with it is still not the point.

---

## 7. The language

The implementation language is **Rust, and only Rust.**

No second language in the editor itself.

Because:

1. Bevy is Rust
2. Bevy plugins can be used directly
3. ECS, reflection and BSN can be used directly
4. Bevy UI can be used directly
5. the asset system can be used directly
6. integrating with the remote protocol is straightforward
7. no additional FFI boundaries

The aim is

> **a Bevy-native editor that gets as much out of Bevy as Rust allows.**

---

## 8. What is in scope, and what is not

### The test

> **Is this something the editor has to carry, for somebody to finish a 2D game
> in Bevy?**

**The size of the scope is not the test.** Whatever a 2D authoring environment
needs is carried. The reason for not carrying something must be
**"it is not this project's responsibility"**, never "it is large".

### Carried

* **levels**: tilemaps, entity placement, colliders, triggers
* **scenarios**: dialogue, branching, flags, direction
* **a database**: items, monsters, skills, shops, encounter tables and the rest,
  as tables
* **battle scenes**: the layout and staging of a battle screen
* **metadata for procedural generation**: room templates, connection points,
  constraints
* **a dedicated UI for writing scenarios**, and its data format
  ([scenario-editor.md §2](./specs/scenario-editor.md))

### Not carried, permanently

| | Why |
| --- | --- |
| a 3D editor, 3D modelling | 2D only (§1) |
| replacing Blender | somebody else's job (§3) |
| replacing Aseprite | somebody else's job (§4) |
| Unity, Unreal or Godot compatibility | Bevy only (§1) |
| multi-engine support | the same |
| a general-purpose game engine | Bevy is not being replaced (§3) |
| a full IDE | editing, completing and debugging Rust belongs to an existing IDE ([dev-environment.md §1](./specs/dev-environment.md)) |
| **general-purpose visual scripting** | scenarios are written in a dedicated UI; the graph is a generated, read-only overview rather than an editing surface ([scenario-editor.md §2](./specs/scenario-editor.md)) |

### The line that matters most

> **A scenario describes progression over time. It does not describe the game's
> behaviour.**

```text
what a scenario says     who speaks, which background, which music, branches, flags
                         → an order of things across time

what Rust says           player movement, combat maths, enemy AI, physics
                         → what happens in one frame
```

The test: **does this complete in one frame, or does it span time?**

The first is Rust's; the second is the scenario's. This line follows from
principle 5 in §9, not hiding Bevy, and from
[runtime-and-play.md §1](./specs/runtime-and-play.md), game code living in the
user's own binary.

**Loops and conditions in a scenario do not make it general-purpose scripting,
as long as this line holds.**

(The earlier [scenario-editor.md §3](./specs/scenario-editor.md) made "a
scenario is a DAG with no loops" a permanent constraint. It does not hold:
returning to a choice and talking to an NPC again are basic to adventure games
and RPGs, and both are loops. It was withdrawn in
[scenario-editor.md §4](./specs/scenario-editor.md) and replaced by this line.)

### What holds the scope in check

Having accepted a wider scope, something else has to do the holding.

1. **Do not skip a phase** ([roadmap.md §3](./specs/roadmap.md)). The database
   editor does not get built before the level editor works.
2. **Do not cross the line above.** Anything that pulls per-frame behaviour into
   a scenario is a reason to stop.
3. **Keep the public contract thin**
   ([level-editor.md §2](./specs/level-editor.md),
   [crates.md §3](./specs/crates.md),
   [level-editor.md §3](./specs/level-editor.md)). Features may grow; what a
   user's game has to depend on does not.

### What this project is

> **A Bevy-native 2D game authoring environment.**

The word "focused" is deliberately dropped. The target is narrowed to 2D and to
Bevy, but **within 2D game development nothing is narrowed.**

---

## 9. UX principles

The most important design goal is not how many features there are.

It is whether the thing can be used.

### Principle 1: beginners first

A user must be able to do this without first learning a complicated editor
architecture.

```text
create a project
    ↓
open the editor
    ↓
create a level
    ↓
place some tiles
    ↓
place the player
    ↓
press play
```

### Principle 2: convention over configuration

Common tasks have obvious defaults.

### Principle 3: visual feedback

Every edit shows its result on screen immediately.

### Principle 4: Bevy-native

The editor reflects Bevy's own concepts rather than inventing an unrelated
object model.

### Principle 5: do not hide Bevy

An advanced user still reaches the actual components, resources and ECS data.

---

## 10. The long view

The destination is not a level editor.

It is

> **a Bevy-native 2D game authoring environment for building levels, stories and
> interactive worlds.**

In outline:

```text
                         Bevy
                           │
                    ┌──────┴──────┐
                    │ Editor Core │
                    └──────┬──────┘
                           │
          ┌────────────────┼────────────────┐
          │                │                │
          ▼                ▼                ▼
    Level Editor     Scenario Editor   future editors
          │                │
          │                │
          └────────┬───────┘
                   │
             Event / Trigger
                   │
                   ▼
             Bevy Runtime
```

Everything from

```text
Hollow Knight shaped
2D action and metroidvanias
```

to

```text
AIR and CLANNAD shaped
scenario games, visual novels, adventures
```

built **on one Bevy-native foundation.**

---

## 11. What this project is, in short

### The concept

> **A Bevy-native 2D game editor.**

### The idea

> **Unity-like usability, Bevy-native architecture.**

### The scope

> **2D only, Bevy only.**

### The editors

```text
Level Editor
Scenario Editor
```

### The runtime

```text
Bevy ECS
```

### The language

```text
Rust
```

### Asset creation

```text
Aseprite
Blender
other external tools
```

### What the editor does

```text
assemble
author
connect
preview
debug
```

### What the runtime does

```text
execute
render
simulate
manage the ECS
```
