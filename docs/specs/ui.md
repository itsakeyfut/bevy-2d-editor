# UI

Choosing the UI stack, and what the design aims at.

---

## 1. UI

The candidates were:

* Bevy UI
* Bevy Feathers
* egui / bevy_egui

Jackdaw's UI architecture is the reference for how to build one.
**§3 settles the stack: Bevy UI + `bevy_feathers`.**

The panels the editor needs:

```text
┌─────────────────────────────────────────┐
│ menu / toolbar                          │
├──────────┬──────────────────┬───────────┤
│ asset    │                  │ inspector │
│ browser  │     viewport     │           │
│          │                  │           │
├──────────┴──────────────────┴───────────┤
│ timeline / scenario graph / console      │
└─────────────────────────────────────────┘
```

---

## 2. Unity is the design target, Jackdaw the implementation reference

### Decision

**The design target for the UI is Unity.**

**Jackdaw is the reference for how to build a Unity-like editor UI in Rust. It
is not the design to copy.**

The two are not the same thing.

```text
        Unity
          │  what to build
          │  UX / layout / feel / vocabulary
          ▼
   Bevy 2D Editor
          ▲
          │  how to build it
          │  widgets in Bevy UI / docking / design tokens
        Jackdaw
```

### Rationale

[concepts.md §1](../concepts.md) states the idea as **Unity-like usability,
Bevy-native architecture**, and the first principle in
[concepts.md §9](../concepts.md) is that a beginner can start without studying
the editor first. Unity is what meets that bar. Jackdaw is not.

What to take from Jackdaw:

* how editor widgets are actually assembled in Bevy UI
* docking, workspaces, and persisting a layout
* consistent styling through design tokens
* an inspector driven by reflection

What not to take from Jackdaw:

* UI specific to a 3D editor: brush editing, terrain sculpting, 3D viewport
  gizmo manipulation
* its own vocabulary and interaction model
* Blender-shaped, expert-first UI such as radial and pie menus

### Note

Jackdaw's UI was refactored towards the Figma design left behind by the archived
official editor prototype. Per [concepts.md §6](../concepts.md), that design is
excluded as a reference here.

So **Jackdaw's appearance is not followed**. What is taken from it is technique;
colour, layout and vocabulary are decided against Unity. Where the two disagree,
Unity wins.

---

## 3. The UI stack is Bevy UI + `bevy_feathers`

### Decision

**Bevy UI + `bevy_feathers`**, the same stack Jackdaw uses.

```toml
bevy = { version = "0.19", default-features = false, features = [
    "2d",
    "ui",
    "bevy_feathers",
] }
```

egui and bevy_egui are not used.

The defaults are not taken. Bevy's `default` is `["2d", "3d", "ui", "audio"]`,
and dropping the two this editor does not need costs `bevy_audio` and
`bevy_gltf` and nothing else: gizmos, picking, sprites, text and Feathers all
arrive through `2d` and `ui`. Measured cold on one machine, that is 511 crates
against 552 and 3m18s against 4m20s, on every leg of a three-platform matrix on
every push. `bevy_audio` comes back in the change that first needs it, which is
what makes it a dependency with a caller rather than one kept in case.

### Rejected options

**B. Use Bevy UI directly and write the widgets by hand**

This removes every experimental dependency, but it means **writing something on
the order of 17,000 lines of UI foundation before a single tile is painted**
(the equivalent of Jackdaw's `jackdaw_feathers`). The MVP in
[roadmap.md §1](./roadmap.md) is the level editor, and building a UI framework
first inverts that order.

Bevy UI itself is also still moving, so "writing it ourselves removes the
breaking changes" is worth less than it sounds.

**C. egui / bevy_egui**

Mature and immediately productive, but it leaves the Bevy-native architecture
[concepts.md §1](../concepts.md) asks for. Nothing from Bevy UI, Feathers or BSN
carries over, almost all of Jackdaw's implementation reference stops applying,
and matching Unity's appearance gets harder.

Choosing egui is also **not reversible**: it has to be decided at the start.
Turning it down explicitly is the point of writing this.

### Accepted risk

`bevy_feathers` has landed in Bevy itself, but **Bevy marks it experimental**.
From the 0.19.1 documentation:

> this crate is still experimental and unfinished! It will change in
> breaking ways, and there will be both bugs and limitations.

Every Bevy upgrade will carry work to absorb breaking changes here. Since
[decisions.md §1](./decisions.md) already commits to tracking Bevy's current
development rather than pinning to a release, that cost is incremental rather
than new.

### Notes

* `bevy_feathers` is not excluded by [concepts.md §6](../concepts.md). Unlike the
  archived prototypes in `bevy_editor_prototypes`, it is under active
  development inside Bevy.
* If Feathers becomes unworkable, the parts that matter can be dropped down to
  hand-written widgets. The reverse, moving hand-written widgets onto Feathers,
  cannot be done. That asymmetry is part of why this is the choice.
* **The design tokens are not used as they come.** Per §2 the design target is
  Unity, so Feathers' tokens are given Unity-shaped values. What Feathers
  supplies is widget structure and behaviour, not appearance.

  **Colour is the only token kind there is.** `ThemeProps` in 0.19.1 carries a
  map of tokens to colours and says "Other style property types to be added
  later"; spacing is `const` values in `constants.rs` and typography is a font
  asset with an `InheritableFont` component. An earlier draft of this section
  said colour, spacing and typography were all tokens, which claimed two kinds
  the engine does not have.

  **The keys stay Feathers'.** Its widgets read `feathers.*` constants
  directly, and a token the theme lacks logs a warning and draws an error
  colour, so renaming them would be wrong everywhere Feathers draws. What is
  replaced is the values.
