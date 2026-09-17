# Roadmap

The MVPs, and phases 0 through 10.

---

## 1. The first MVP

Not levels and scenarios and animation all at once.

The first MVP is the level editor, and nothing else.

```text
open a Bevy project
      ↓
recognise its assets
      ↓
show a sprite
      ↓
show a tileset
      ↓
paint a tilemap
      ↓
place a sprite
      ↓
place an entity
      ↓
edit a transform
      ↓
layers
      ↓
inspector
      ↓
save and load
      ↓
undo and redo
      ↓
preview (runtime-and-play.md §2)
```

Preview here means replaying the level inside the editor. Real play, with the
game's own code, is phase 3
([runtime-and-play.md §2](./runtime-and-play.md)).

---

## 2. The scenario editor's MVP

The scenario editor comes after the level editor's basics stand up.

```text
create a scenario
      ↓
add dialogue
      ↓
add a character
      ↓
add a background
      ↓
add a choice
      ↓
create a branch
      ↓
save
      ↓
play the scenario
```

Then:

* flags and numeric variables
* events
* music
* sound effects
* level transitions
* integration with triggers

**An import path, from plain text and Word, and search and replace across every
scenario, are part of the MVP.** They are not things to add later: without them
the tool does not get used
([scenario-editor.md §2.7](./scenario-editor.md)).

---

## 3. The phases

### Dependencies

The order is not a preference. **It is forced by dependencies.**

```text
Phase 1  Editor Core
   │
   ├──────────────────┐
   ▼                  ▼
Phase 2            Phase 3
Level editor       Runtime and play
                      │
                      │  ★ schema extraction starts working here
                      │    user-defined types are usable from now on
                      ▼
                   Phase 4
                   Database editor
                      │
                      ▼
                   Phase 5
                   Scenario editor (the manuscript UI)
                      │
                      ▼
                   Phase 6
                   Integration and saving
                      │
          ┌───────────┼───────────┐
          ▼           ▼           ▼
       Phase 7     Phase 8     Phase 9
       Battle      Procedural  Presentation
```

**Two of these dependencies matter.**

1. **The database editor depends on phase 3.** Working with types the user
   defined in Rust requires building their project and extracting its schema
   ([runtime-and-play.md §1](./runtime-and-play.md)).
2. **The scenario editor depends on phase 4.** The direction palette uses the
   same reflection-driven mechanism as the database editor
   ([scenario-editor.md §2.4](./scenario-editor.md)). Establish it on the
   smaller of the two first.

---

### Phase 0: research, complete

The results are in [decisions.md](./decisions.md) and
[scenario-editor.md §2](./scenario-editor.md). What remains to look at is in §4.

---

### Phase 1: Editor Core

* a Bevy application
* a window
* a viewport
* the asset browser
* selection
* **the inspector, for built-in types only**; user-defined components arrive in
  phase 3
* transforms
* gizmos
* undo and redo
* save and load
* **preview and stop**, replaying inside the editor with no game code
  ([runtime-and-play.md §2](./runtime-and-play.md))

---

### Phase 2: the level editor

* tilesets
* tilemaps
* placing sprites
* placing entities
* layers
* colliders: `ColliderShape`, **defined with the polygon variant included**
  ([level-editor.md §5](./level-editor.md))
* triggers
* spawn points
* camera
* **sprite animation playback and tag selection**
  ([level-editor.md §4](./level-editor.md))
* level serialization
* settling the tile data file format ([data-model.md §4](./data-model.md))

---

### Phase 3: runtime and play

```text
Editor
  ↓
save
  ↓
level data
  ↓
Runtime
```

* a `cargo build` pipeline over the user's project
* **extracting the reflection schema from what it built** ← everything after
  this depends on it
* spawning a child process and managing its lifetime
* IPC between the editor and the runtime
* **play and stop**, running the game's own code
  ([runtime-and-play.md §2](./runtime-and-play.md))
* **the inspector working with user-defined components**
* the avian2d adapter ([level-editor.md §3](./level-editor.md))
* hot reload
* runtime synchronization

---

### Phase 4: the database editor

* the table view, which belongs in `editor_ui` ([crates.md §3](./crates.md))
* reading user-defined types from the schema extracted in phase 3
* saving one row per `.bsn` file ([database-editor.md §1](./database-editor.md))
* adding, duplicating, deleting and reordering rows
* scaffolding templates for RPGs, roguelites and adventure games
* referencing a database row from a level

**A database is needed by six of the nine target games in
[concepts.md §2](../concepts.md). It matters second only to levels.**

---

### Phase 5: the scenario editor and its manuscript UI

The whole design is in
**[scenario-editor.md §2](./scenario-editor.md)**.

* **the manuscript UI**, where the prose is edited
* switching presentation between a Japanese manuscript grid and standard
  manuscript format ([scenario-editor.md §2.6](./scenario-editor.md))
* **showing the line-length limit** ([scenario-editor.md §2.5](./scenario-editor.md))
* the direction track, attaching items to lines and paragraphs
* **the direction palette generated from reflection**
  ([scenario-editor.md §2.4](./scenario-editor.md))
* **a live preview of the cursor's position**
  ([scenario-editor.md §2.5](./scenario-editor.md))
* characters, backgrounds, music, sound effects
* choices and branches
* **an import path** from text and Word, required by the MVP
  ([scenario-editor.md §2.7](./scenario-editor.md))
* **search and replace across every scenario**, with regular expressions,
  required by the MVP ([scenario-editor.md §2.7](./scenario-editor.md))
* the text view, read-only
* the graph view, a read-only overview
* the id map, for save compatibility and localization
  ([scenario-editor.md §2](./scenario-editor.md))

---

### Phase 6: integration and saving

```text
a trigger in a level
      ↓
a scenario
      ↓
flags and numeric variables
      ↓
an event back in the level
```

* trigger to scenario
* scenario to level
* scene transitions
* global flags and numeric variables
* conditions, on flags and numbers
  ([scenario-editor.md §4](./scenario-editor.md))
* the event system
* game state
* **save and load**, with the id map carrying save compatibility
  ([scenario-editor.md §2](./scenario-editor.md))
* wiring localization through

---

### Phase 7: the battle editor

* the battle screen's layout: background, enemy placement, party placement
* the command UI's layout
* battle staging
* working with the database: monsters, enemy groups, skills

**Undecided**: whether this is its own editor or a scene kind of the level
editor. **The trigger condition is in
[open-questions.md §1](./open-questions.md).** The judgement comes after the
database exists in phase 4.

---

### Phase 8: procedural generation

* room templates
* connection points, and which way they face
* room kind tags: combat, treasure, boss
* generation constraints
* assembling them at runtime

For Dead Cells and Hades shaped games.

**The data model is undecided. The trigger condition is in
[open-questions.md §1](./open-questions.md).** The judgement comes after several
real levels have been built in phase 2.

---

### Phase 9: presentation

* **a hand-painted art workflow**, for Hollow Knight shaped games
  ([level-editor.md §5](./level-editor.md))
  * backgrounds built from freely placed sprites
  * layers and parallax
  * a polygon collider editing UI

---

### Phase 10: advanced authoring

* **the asset processing pipeline**, baking `.aseprite` into atlases and the
  rest ([assets.md §1](./assets.md))
* an animation editor
* a timeline
* a cutscene editor
* menu UI layout, if it is taken on at all
  ([open-questions.md §1](./open-questions.md))
* dialogue preview
* prefabs and entity templates
* debugging tools
* two-way text editing for scenarios
  ([scenario-editor.md §2.9](./scenario-editor.md))

---

### A note on length

This roadmap is long because [concepts.md §8](../concepts.md) accepted a wider
scope. **What holds it in check is the three rules there.**

1. Do not skip a phase.
2. Do not cross the line in [concepts.md §8](../concepts.md): per-frame
   behaviour is Rust's, progression over time is the scenario's.
3. Keep the public contract thin
   ([level-editor.md §2](./level-editor.md), [crates.md §3](./crates.md),
   [level-editor.md §3](./level-editor.md)).

---

## 4. What to do next

### Done (see [decisions.md](./decisions.md))

| Question | Answer |
| --- | --- |
| Jackdaw's crate structure | [crates.md §2](./crates.md), [crates.md §3](./crates.md) |
| BSN | [data-model.md §2](./data-model.md), [data-model.md §4](./data-model.md) |
| Bevy Feathers and the UI stack | [ui.md §3](./ui.md) |
| Existing Bevy 2D tilemap crates | [level-editor.md §2](./level-editor.md) |
| Aseprite integration | [assets.md §1](./assets.md) |
| Physics | [level-editor.md §3](./level-editor.md) |
| What the editor plugin API is | [architecture.md §7](./architecture.md), a compile-time `PluginGroup` |
| The Editor Model and undo | [data-model.md §5](./data-model.md) |

### Left before phase 1 starts

Nothing. What is in [decisions.md](./decisions.md) is enough to start
implementing.

### Alongside phase 1

1. Confirm Bevy 0.19's reflection in practice, while building the inspector.
2. Confirm Bevy's asset system in practice, while building the asset browser.
3. Confirm Bevy's gizmos in practice, while building transform editing.
4. Define the level data model, in the `data` crate; the approach is
   [data-model.md §5](./data-model.md).

### From phase 3

5. Bevy's remote protocol, as a candidate for the editor-to-runtime transport
   ([open-questions.md §1](./open-questions.md)).
6. Bevy 0.20's official `.bsn` loader ([data-model.md §4](./data-model.md)).

### From phase 4

7. What the database templates contain, and how scaffolding works
   ([database-editor.md §1](./database-editor.md)).
8. The table view's UI design.

### From phase 5

9. The manuscript UI in detail
   ([scenario-editor.md §2.10](./scenario-editor.md)).
10. The scenario data format
    ([scenario-editor.md §2.8](./scenario-editor.md)).
11. How the id map works, for save compatibility and localization
    ([scenario-editor.md §2](./scenario-editor.md)).
12. How far the Word import goes.
