# Decisions

Every decision in one place, with an index of where each one is written.

**The reasoning, the rejected options and the accepted risk live in the
specification file each decision belongs to.** A decision is not filed away from
the thing it governs.

What is indexed here is decisions about **what to build**. Decisions about **how
the code is shaped** are [records](../adr/README.md), and the line between the
two is drawn there.

---

## How a decision is written

Each one takes this shape:

```text
### Decision            what was decided
### Rationale           why
### Rejected options    what was considered, and why it was not taken
### Accepted risk       what this decision costs
```

**The rejected options are always kept.** When the same question comes back,
knowing that an option was already weighed and turned down is what makes the
second pass quick.

**A decision that is withdrawn is marked withdrawn, not deleted**
([scenario-editor.md §3](./scenario-editor.md) and
[scenario-editor.md §4](./scenario-editor.md)). Why it was made and why it was
wrong are both worth keeping.

Quotations are left in the language of their source.

---

## Index

| Decision | Where |
| --- | --- |
| The relationship to Bevy's official editor: archived work is excluded as a reference | [concepts.md §6](../concepts.md) |
| What is in scope and what is not | [concepts.md §8](../concepts.md) |
| No dylib | [architecture.md §7](./architecture.md) |
| The editor and the runtime are separate processes | [runtime-and-play.md §1](./runtime-and-play.md) |
| Two kinds of running: preview and play | [runtime-and-play.md §2](./runtime-and-play.md) |
| Start with the fewest crates that work | [crates.md §2](./crates.md) |
| The dependency direction between crates | [crates.md §3](./crates.md) |
| Package names carry a `b2d_` prefix; directory names do not | [crates.md §4](./crates.md) |
| Unity is the design target, Jackdaw the implementation reference | [ui.md §2](./ui.md) |
| The UI stack is Bevy UI + `bevy_feathers` | [ui.md §3](./ui.md) |
| What the mouse does in the viewport: pan, zoom on the cursor, select on release, add or remove with Ctrl/Cmd, and a box drag over empty space | [ui.md §4](./ui.md) |
| What a selection, and a box drag in progress, look like in the viewport | [ui.md §5](./ui.md) |
| What the inspector shows: every component, named through reflection and opened into the values it carries | [ui.md §6](./ui.md) |
| What the inspector can edit: an `f32` leaf, typed into a box and committed on Enter | [ui.md §7](./ui.md) |
| What Ctrl+Z takes back: the last committed number, or what is typed in the focused box | [ui.md §8](./ui.md) |
| Tilemaps ride on Bevy's own `TilemapChunk` | [level-editor.md §2](./level-editor.md) |
| Colliders: a physics-neutral shape, avian2d behind a default-on feature | [level-editor.md §3](./level-editor.md) |
| Sprite animation | [level-editor.md §4](./level-editor.md) |
| Target genres, in stages | [level-editor.md §5](./level-editor.md) |
| The data format is BSN | [data-model.md §4](./data-model.md) |
| The Editor Model: a typed shell holding AST fragments | [data-model.md §5](./data-model.md) |
| `.aseprite` files are read directly | [assets.md §1](./assets.md) |
| Documentation is mdBook | [dev-environment.md §2](./dev-environment.md) |
| CI runs the whole gate on all three platforms, as one command | [dev-environment.md §3](./dev-environment.md) |
| The database: user-defined types, scaffolded from templates | [database-editor.md §1](./database-editor.md) |
| Scenarios are written in a manuscript UI | [scenario-editor.md §2](./scenario-editor.md) |
| The scenario graph's boundary **(withdrawn)** | [scenario-editor.md §3](./scenario-editor.md) |
| The withdrawal: a DAG constraint does not hold | [scenario-editor.md §4](./scenario-editor.md) |

---

## 1. Every decision, in one table

| Item | Decision |
| --- | --- |
| Engine | Bevy |
| Bevy version | 0.19+, tracking current development |
| Language | Rust |
| Target | 2D |
| Other engines | not supported |
| Editor style | Unity-like usability |
| Architecture | Bevy-native |
| Main development environment | Windows |
| WSL2 | a secondary environment |
| Level editor | yes |
| Scenario editor | yes |
| Plugin architecture | yes, as a **compile-time `PluginGroup`** |
| Editor plugins and runtime plugins | kept separate |
| dylib and dynamic extension loading | **not used** ([architecture.md §7](./architecture.md)) |
| `bevy/dynamic_linking` | dev builds only, for link time |
| Editor and runtime process model | **separate processes, over IPC** |
| Crate layout | **start minimal, split when it is earned** |
| Package names | **`b2d_` prefix**; the directories stay as §1 has them ([crates.md §4](./crates.md)) |
| UI design target | **Unity** |
| UI implementation reference | **Jackdaw**, for how to build it in Rust |
| UI stack | **Bevy UI + `bevy_feathers`** ([ui.md §3](./ui.md)) |
| Inspector | **every component on the entity**, sorted by name, each opened into one line per field with the value as its `Debug` text ([ui.md §6](./ui.md)) |
| Editing in the inspector | **`f32` leaves only**, one box each, committed on Enter or focus loss, written to the component through a command in the history ([ui.md §7](./ui.md)) |
| Undo | **Ctrl/Cmd+Z over one history of commands**, taking back what is typed in a focused box before the history ([ui.md §8](./ui.md)) |
| Aseprite | an external authoring tool |
| Blender | an external 3D authoring tool |
| Tilemap | **Bevy's own `TilemapChunk`**, no third-party crate |
| Isometric and hex grids | out of scope; square grids only |
| LDtk and Tiled | import only, later; not a foundation |
| Jackdaw | a reference for architecture and UI implementation |
| `bevy_editor_prototypes` | **excluded**, archived 2026-04-16 ([concepts.md §6](../concepts.md)) |
| Archived and prototype work generally | **excluded as a reference** |
| Documentation | mdBook, in `book/`, following Jackdaw's shape |
| CI | **the whole gate on Linux, macOS and Windows**, through `cargo xtask all` ([dev-environment.md §3](./dev-environment.md)) |
| Build caching in CI | **none until Bevy lands**, then `Swatinem/rust-cache` in the same change ([dev-environment.md §3](./dev-environment.md)) |
| Editor Model | **a typed shell holding AST fragments for component values** ([data-model.md §5](./data-model.md)) |
| Running | **preview in phase 1, play in phase 3** ([runtime-and-play.md §2](./runtime-and-play.md)) |
| Crate dependency direction | **fixed in [crates.md §3](./crates.md)**: `editor → runtime → data → core` |
| The `data` crate | **editor-only parts behind a feature** ([crates.md §3](./crates.md)) |
| Colliders | **the shape type is physics-neutral; avian2d is a default-on feature** ([level-editor.md §3](./level-editor.md)) |
| A physics engine of our own | not built into the editor; a separate project reached through an adapter ([level-editor.md §3](./level-editor.md)) |
| Aseprite files | **read directly**, through `bevy_aseprite_ultra` behind a default-on feature ([assets.md §1](./assets.md)) |
| Asset processing pipeline | **a long-term goal**: solve here what Jackdaw left open ([assets.md §1](./assets.md)) |
| Sprite animation | **playback and tag selection in phase 2, editing in phase 10** ([level-editor.md §4](./level-editor.md)) |
| Genres for now | **tile-based, square grid** ([level-editor.md §5](./level-editor.md)) |
| Hollow Knight shaped games | **phase 9**: a hand-painted art workflow ([level-editor.md §5](./level-editor.md)) |
| Scenario graph | ~~DAG constraint~~ → **withdrawn** ([scenario-editor.md §4](./scenario-editor.md)); loops and numeric conditions are allowed |
| Writing a scenario | **a manuscript UI** ([scenario-editor.md §2](./scenario-editor.md)); no script language |
| The scenario editing surface | **the manuscript UI only**; the text and graph views are read-only ([scenario-editor.md §2](./scenario-editor.md)) |
| Direction items | **generated from reflection** ([scenario-editor.md §2](./scenario-editor.md)), the same mechanism as [database-editor.md §1](./database-editor.md) |
| Required in the scenario MVP | **an import path and bulk editing** ([scenario-editor.md §2.7](./scenario-editor.md)) |
| The node graph UI | lives in `editor_ui`; **a generated, read-only overview** ([scenario-editor.md §4](./scenario-editor.md)) |
| Database | **types defined by the user in Rust, scaffolded from templates, one row per file** ([database-editor.md §1](./database-editor.md)) |
| Scope policy | **size is not the test; responsibility is** ([concepts.md §8](../concepts.md)) |
| Deferring a decision | **a trigger condition is the price of deferring** ([open-questions.md §1](./open-questions.md)) |
| The line between scenario and Rust | **progression over time is the scenario's; per-frame behaviour is Rust's** ([concepts.md §8](../concepts.md)) |
| Data format | **BSN, as text** ([data-model.md §4](./data-model.md)) |
| The BSN parser | **written here for the editor side**, with Jackdaw as the reference |
| Tile data | **its own file**, referenced by path from BSN ([data-model.md §4](./data-model.md)) |
| The official `.bsn` loader | arriving in Bevy 0.20 (PR #23576, merged); the runtime side moves onto it later |
| Reflection | core technology |
| Asset system | core technology |
| Gizmos | core technology |
| Undo and redo | required, as a command pattern |
| Hot reload | a goal |
| Runtime synchronization | a goal |
| 3D | not supported |
| Multi-engine support | not supported |
| Zig | not used in the editor |
| Nightly | **not required; stable is the target** |
