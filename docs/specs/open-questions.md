# Open questions

What is still open, and the trigger condition for each deferral.

---

## 1. Open questions

### Settled (see [decisions.md](./decisions.md))

* ~~Whether the editor and the runtime share one Bevy App or talk as separate processes~~ → **separate processes** ([runtime-and-play.md §1](./runtime-and-play.md))
* ~~Whether nightly is required~~ → **no; stable is the target** ([architecture.md §7](./architecture.md))
* ~~What the editor plugin API is~~ → **a compile-time `PluginGroup`**; dynamic loading is a later question ([architecture.md §7](./architecture.md))
* ~~Bevy UI, Bevy Feathers or egui~~ → **Bevy UI + `bevy_feathers`** ([ui.md §3](./ui.md))
* ~~How the tilemap is implemented~~ → **Bevy's own `TilemapChunk`** ([level-editor.md §2](./level-editor.md))
* ~~Whether level and scenario data is BSN or a format of our own~~ → **BSN, with the editor's parser written here** ([data-model.md §4](./data-model.md))
* ~~How the scenario graph is implemented~~ → **the manuscript UI is the only editing surface; the graph is a generated, read-only overview** ([scenario-editor.md §2](./scenario-editor.md)). The DAG constraint in [scenario-editor.md §3](./scenario-editor.md) was withdrawn in [scenario-editor.md §4](./scenario-editor.md)
* ~~Whether scenarios are written in a script language~~ → **no; a dedicated UI instead** ([scenario-editor.md §2](./scenario-editor.md))
* ~~How the database works~~ → **the user defines the types in Rust, templates scaffold them, one row per file** ([database-editor.md §1](./database-editor.md))
* ~~How this relates to Bevy's official editor~~ → **the archived prototypes are excluded as a reference; alignment comes from living on Bevy's own foundations** ([concepts.md §6](../concepts.md))

### Still open

#### Raised by widening the scope

* **The table view's UI design** for the database (it belongs in `editor_ui`; [database-editor.md §1](./database-editor.md))
* **What the data type templates contain, and how scaffolding works** ([database-editor.md §1](./database-editor.md))
* **How a level or a scenario references a database row** ([database-editor.md §1](./database-editor.md))
* **The storage format for battle scenes**
* **The architecture for numeric variables**, now that conditions are allowed ([scenario-editor.md §4](./scenario-editor.md))
* **The scenario data format** (BSN or our own; [scenario-editor.md §2.8](./scenario-editor.md))
* **The manuscript UI in detail**: the grid, and how the direction track is shown ([scenario-editor.md §2.10](./scenario-editor.md))
* **How choices, branches and conditions are presented in the UI** ([scenario-editor.md §2.10](./scenario-editor.md))
* **How the id map works**, for save compatibility and localization ([scenario-editor.md §2.10](./scenario-editor.md))
* **How far the Word import goes** ([scenario-editor.md §2.10](./scenario-editor.md))

#### Open from the start

* What the runtime plugin API is
* The data model for events and triggers
* The architecture for global flags
* How a level and a scenario reference each other
* The transport between the editor and the runtime (ipc-channel, BRP, or our own)
* The hot reload architecture
* The tile data file format (splitting it out is decided in [data-model.md §4](./data-model.md); its contents are a phase 2 question)
* The project file format
* How assets are discovered
* The play mode architecture in detail
* The animation and timeline architecture, later

Of these, **the ones deferred with a trigger condition are collected below.**

### Deferred decisions, and what makes each one answerable

#### Why a trigger condition is written down

[scenario-editor.md §3](./scenario-editor.md) recorded a constraint as permanent
and withdrew it two sections later. The cause was **deciding before the premise
had settled** ([scenario-editor.md §4](./scenario-editor.md)).

For the same reason, **some things should not be decided before their premise
settles**. But declaring something deferred, and no more, is the same as
forgetting it.

> **The condition for deferring is writing down what makes it answerable.**

Nothing goes on this list without one.

#### The list

| Decision | When | Trigger condition | What is needed to decide |
| --- | --- | --- | --- |
| **Where the battle editor lives**<br>(its own editor, or a scene kind of the level editor) | after phase 4,<br>before phase 7 | when monsters, skills and enemy groups can actually be edited in the database editor | the database's row reference model settled. How the level editor handles scene kinds, visible in the implementation |
| **The procedural generation data model**<br>(room templates, connection points, constraints) | after phase 2,<br>before phase 8 | when several levels have actually been built and the right size for a room is known from experience | whether "a room template is a small level" holds in practice. The level data model settled |
| **Isometric support**<br>(revisiting [level-editor.md §2](./level-editor.md)) | before phase 9 | when a Hades-shaped game becomes a real target, or a square grid actually blocks something | `TilemapChunk` assumes a square grid, so what Bevy does about it. The cost of doing it here |
| **Whether the editor's panels dock**<br>(they are a fixed arrangement in [ui.md §1](./ui.md)) | after phase 2 | when a second editor wants the same screen area, which is the level editor and the scenario editor both needing the middle | whether a layout is saved per project or per user, and whether a pane gains tabs. Whether `bevy_feathers` has grown docking by then: it has `pane`, `subpane` and `group` and nothing that rearranges them |
| **Whether menu UI layout is in scope** | after phase 6 | when an RPG has been built end to end and the cost of writing its menus is known | how much work it actually is in Rust. Judged against the responsibility test in [concepts.md §8](../concepts.md) |
| **Two-way text editing for scenarios**<br>([scenario-editor.md §2.9](./scenario-editor.md)) | after phase 5 | when the manuscript UI has seen real use and people ask for text editing | the cost of guaranteeing the round trip agrees. Whether the read-only view turned out to be enough |
| **The asset processing pipeline** (moving from reading `.aseprite` to baking it)<br>([assets.md §1](./assets.md)) | after phase 3 | when play and the build pipeline work | whether the experience of reading `.aseprite` directly, hot reload included, can be kept through a pipeline |
| **The tile data file format**<br>(splitting it out is decided in [data-model.md §4](./data-model.md)) | phase 2 | when the tilemap implementation starts | the real size of things: how many tiles, how many layers |
| **The transport between editor and runtime**<br>(ipc-channel, BRP, our own) | at the start of phase 3 | when the shape of the build pipeline is settled | how Jackdaw's `jackdaw_pie_protocol` does it. How mature BRP is |
| **Moving to Bevy 0.20**<br>(revisiting the version in [ui.md §3](./ui.md)) | when 0.20 leaves release candidate | when `0.20.0` is published on crates.io, rather than while it is `0.20.0-rc.1` | how much `bevy_feathers` breaks across the upgrade, which [ui.md §3](./ui.md) already accepts as a recurring cost. Whether the official `.bsn` loader is wanted before the `Scene Format` milestone starts, which is the one thing 0.20 brings that this project asked for |
| **Which of an entity's components the inspector lists**<br>(everything, in [ui.md §6](./ui.md)) | phase 3 | when the reflection schema is extracted from the user's project, which is what lets the editor tell a component the user defined from one the engine attached | whether the noise actually costs anything in use: 13 of the 15 rows on a placeholder are the engine's. Whether collapsing, rather than filtering, is the answer |
| **Whether a crate's own `default` feature contents are held against what a game carries**<br>(the other half of [crates.md §3](./crates.md)) | phase 3 | when the first `[features]` table arrives, which is the change that brings `avian2d` in | whether taking `avian2d` out of `default` should fail a test or only the `runtime` build. `SHIPPED` in `crates/editor/tests/dependency_direction.rs` holds the selection on a dependency already; this is the crate's own table |

#### How this list is used

* **When a trigger condition is met, stop and decide it there.** Not "later",
  while carrying on with the phase.
* Write the decision down where [decisions.md](./decisions.md) says it goes, with
  the options that were rejected and the risk that was accepted, in the same
  form as every other decision.
* **Record it when a trigger condition itself turns out to be wrong**, the same
  way [scenario-editor.md §4](./scenario-editor.md) records a withdrawn
  decision.
