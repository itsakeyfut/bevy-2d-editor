# Architecture

The editor's structure, Editor Core, the plugin composition, and how it sits on
Bevy.

---

## 1. Architecture

The shape of it:

```text
                         Bevy 2D Game Editor
                                  │
                           ┌──────┴──────┐
                           │ Editor Core │
                           └──────┬──────┘
                                  │
      ┌──────────┬────────────────┼────────────────┬──────────┐
      ▼          ▼                ▼                ▼          ▼
   Level      Scenario         Database         Battle     future
   Editor      Editor           Editor          Editor     plugins
      │          │                │                │
      └──────────┴────────┬───────┴────────────────┘
                          │
                    Event / Trigger
                          │
                    Bevy Integration
                          │
                          ▼
                     Bevy Runtime
```

### What each editor is responsible for

| Editor | Edits | Produces |
| --- | --- | --- |
| **Level editor** | the space a game world occupies | `.bsn` plus tile data |
| **Scenario editor** | progression over time | scenario documents ([scenario-editor.md §2](./scenario-editor.md)) |
| **Database editor** | table-shaped game data | `.bsn` asset files |
| **Battle editor** | the layout and staging of a battle screen | undecided ([open-questions.md §1](./open-questions.md)) |

The four are divided **by the nature of what they edit**.

```text
Level      space     where things are
Scenario   time      what happens, in what order
Database   numbers   what kinds of thing exist
Battle     screen    how a fight is arranged
```

### Inside the scenario editor

The scenario editor has one editing surface and two views onto it.

```text
Scenario Editor
├── manuscript UI  ← the only place editing happens
├── text view      ← read-only
└── graph view     ← read-only, an overview of the route structure
```

**Scenarios are not written in a script language.** What a writer touches is a
manuscript UI, and there is no syntax to learn. Direction is attached to lines
and paragraphs as items chosen in the UI
([scenario-editor.md §2](./scenario-editor.md)).

The text and graph views are read-only. Editing always happens in the manuscript
UI.

### Where Editor Core sits

Editor Core knows nothing about any particular kind of document (§2). All four
editors are composed as a **compile-time `PluginGroup`** (§7).

```rust
App::new()
    .add_plugins(DefaultPlugins)
    .add_plugins(EditorCorePlugins)
    .add_plugins(LevelEditorPlugin)
    .add_plugins(ScenarioEditorPlugin)
    .add_plugins(DatabaseEditorPlugin)
    .add_plugins(BattleEditorPlugin)
    .run()
```

---

## 2. Editor Core

Editor Core carries what every editor needs, with nothing specific to a genre.

### Core features

* project management
* asset browser
* scene management
* selecting entities
* editing transforms
* inspector
* hierarchy
* gizmos
* undo and redo
* copy and paste
* delete
* save and load
* editor commands
* run and stop
* runtime synchronization
* hot reload
* input handling
* editor panels
* the plugin system

**Editor Core holds no specification for the level editor or the scenario
editor.**

---

## 3. The plugin architecture

Editor features are added as plugins.

```text
Editor Core
    │
    ├── LevelEditorPlugin
    ├── ScenarioEditorPlugin
    ├── DatabaseEditorPlugin
    ├── BattleEditorPlugin
    └── future plugins
```

Through phase 2 the focus is **the level editor**. Scenario, database and battle
follow in that order ([roadmap.md §3](./roadmap.md)).

Plugins are composed as a **compile-time `PluginGroup`**. Dynamic loading with
dylibs is not used (§7).

---

## 4. Editor plugins and game plugins are separate

The two are kept apart.

### On the editor side

```text
LevelEditorPlugin
ScenarioEditorPlugin
```

### On the runtime side

```text
LevelRuntimePlugin
ScenarioRuntimePlugin
DialogueRuntimePlugin
TriggerRuntimePlugin
```

What the scenario editor produces reaches the game this way:

```text
ScenarioEditorPlugin
        │
        ▼
scenario.bsn
        │
        ▼
ScenarioRuntimePlugin
        │
        ▼
Bevy ECS
```

**A running game never needs the editor.**

---

## 5. Living on Bevy

Use what the engine provides, wherever it provides it.

What that covers:

* ECS
* reflection
* the asset system
* scenes
* BSN
* gizmos
* Bevy UI
* Bevy Feathers
* input
* rendering
* sprites
* texture atlases
* hot reload
* the remote protocol

The rule:

> Before building a mechanism, find out whether the engine has one.

---

## 6. The core architectural principle

Keep this separation clear:

```text
user action
     ↓
Editor Command                     ← takes &mut World (data-model.md §5)
     ↓
Editor Model                       ← a typed shell holding AST fragments (data-model.md §5)
     ↓
a serialized representation        ← BSN plus tile data files (data-model.md §4)
     ↓
Bevy integration
     ↓
Runtime ECS
```

**The editor UI must not become the source of truth.**

Level and scenario data stays serializable, inspectable and version-controllable.

Version-controllable here means **the output is deterministic**. Preserving
hand-written formatting or comments is not required
([data-model.md §5](./data-model.md)).

---

## 7. No dylib

### Decision

Dynamic loading of editor extensions, through dylibs and `dlopen`, is **not
used**.

The one exception is the `bevy/dynamic_linking` feature, used only to cut link
time in development builds and turned off for release.

### Rationale

**One: Jackdaw did not start with it, and has since retreated halfway.**

```text
2026-02-02  first commit, no dylib
     │      about two and a half months of ordinary static linking
     │      viewport, hierarchy, inspector, brushes,
     │      undo/redo and remote debugging all landed in it
     ▼
2026-04-19  "initial PIE support with rustc wrapper and dylib loading"
     │      four months of firefighting follow
     ▼
2026-08-08  #320 "Isolate game bevy"
            → games went back to an ordinary cargo build
```

Dylibs arrived to run the user's game inside the editor's process. That use was
withdrawn four months later and replaced with process separation. What still
uses dylibs is only the extension mechanism.

**Two: it is permanent technical debt on Windows.**

From the top of Jackdaw's
`book/src/developer-guide/open-challenges.md`:

> On Windows, a PE export table addresses its entries with a 16-bit ordinal, so 65,535 is the ceiling and no linker escapes it. ... the hottest table is `bevy_dylib` (~46k exports in the workspace debug profile) ... The release job measures every table and fails above 60,000.
>
> What remains is headroom: **Bevy's export surface grows with the engine.**

A Windows DLL addresses exported symbols with a 16-bit ordinal, so 65,535 is the
ceiling. Bevy as a dylib takes about 46,000 of them today, and every Bevy
release moves that closer. It also forces LTO off on Windows and requires
`CARGO_INCREMENTAL=0`.

The main development environment here is Windows
([dev-environment.md §1](./dev-environment.md)). There is no reason to inherit
that.

**Three: it avoids the TypeId problem.**

Bevy's ECS and reflection identify types by `TypeId`, which is generated per
compilation unit. If the editor and an extension dylib each compile their own
Bevy, the same type is two different types.

```text
editor     TypeId::of::<Transform>()  =  0xAAAA
extension  TypeId::of::<Transform>()  =  0xBBBB   ← they do not match
```

Jackdaw solves this with an SDK facade dylib and **a rustc wrapper of its own**
that rewrites `--extern bevy=`. It is the heaviest machinery in that repository,
and not something to carry from the start.

**Four: it removes the need for nightly.**

Nightly is what Jackdaw's dylib machinery requires. Without it, stable should be
enough.

### Consequence

The plugin architecture in §3 is realised as a **compile-time Bevy
`PluginGroup`**.

```rust
App::new()
    .add_plugins(DefaultPlugins)
    .add_plugins(EditorCorePlugins)
    .add_plugins(LevelEditorPlugin)
    .add_plugins(ScenarioEditorPlugin)
    .run()
```

That achieves what the plugin architecture is for: loosely coupled features and
room to add more. Dynamic loading gets reconsidered when third parties start
distributing plugins as binaries, and not before.
