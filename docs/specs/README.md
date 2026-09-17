# Specifications

How each part of the editor works. What the project is for is in
[concepts.md](../concepts.md).

Each file holds **the specification and the reasoning behind it in the same
place**. How something works and why it was decided that way are not separated.

---

## The whole

| File | Holds |
| --- | --- |
| [architecture.md](./architecture.md) | the editor's structure, Editor Core, the plugin composition, integration with Bevy |
| [crates.md](./crates.md) | the workspace layout, the bar for splitting a crate, the dependency graph |
| [data-model.md](./data-model.md) | the Editor Model, BSN, reflection, serialization |
| [runtime-and-play.md](./runtime-and-play.md) | the editor and runtime process model, preview and play |
| [ui.md](./ui.md) | the UI stack and the design it aims at |

## The editors

| File | Holds |
| --- | --- |
| [level-editor.md](./level-editor.md) | editing space: tilemap, colliders, sprite animation |
| [scenario-editor.md](./scenario-editor.md) | editing progression over time: the manuscript UI |
| [database-editor.md](./database-editor.md) | editing table-shaped data: items, monsters, skills |
| [integration.md](./integration.md) | how they reach each other, through events and triggers |

## The rest

| File | Holds |
| --- | --- |
| [assets.md](./assets.md) | Aseprite, and the asset processing pipeline |
| [dev-environment.md](./dev-environment.md) | the development environment and the documentation |
| [roadmap.md](./roadmap.md) | the MVPs and phases 0 through 10 |
| [decisions.md](./decisions.md) | every decision, and where each one is written |
| [../adr/](../adr/README.md) | decisions about how the code is shaped, and the line between those and these |
| [open-questions.md](./open-questions.md) | what is still open, and the trigger condition for each deferral |

---

## Reading order

Coming to this for the first time:

```text
../concepts.md        what is being built
      ↓
architecture.md       the shape of it
      ↓
crates.md             where code goes and which way dependencies run
      ↓
roadmap.md            the order it gets built in
```

To implement a particular feature, read the file for that editor. The reasoning
behind each decision, and the options that were rejected, are there too.

---

## About section numbers

**Section numbers restart at 1 in every file.** A bare number refers to a
section of the document it appears in.

A reference that crosses files is **a link carrying the filename**, like
`[level-editor.md §2](./level-editor.md)`, because the number alone does not
identify a section.

Add a section at the end of its file, with the next number.
**Do not insert one in the middle and renumber**: every reference from another
file shifts at once.
