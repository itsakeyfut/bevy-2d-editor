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

The panels the editor needs. **The names in this drawing are the names the
code uses**: `Region` in `b2d_editor` has one variant per region here, and the
bottom one is a panel with room for what this drawing puts in it rather than a
status strip. An earlier implementation called it a status bar and gave it 22
pixels, which a scenario graph does not fit in.

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

---

## 4. What the mouse does in the viewport

### Decision

| Gesture | What it does |
| --- | --- |
| Middle button, dragged | Drags the view |
| Wheel | Zooms, on the cursor |
| Left button, released over an entity | Selects it, and nothing else stays selected |
| Left button, released over empty space | Clears the selection |
| Ctrl or Cmd with the left button, over an entity | Adds it to the selection, or takes it out if it was already in it |
| Ctrl or Cmd with the left button, over empty space | Nothing; the selection stays as it was |
| Right button | Nothing yet, and kept for a context menu |

Zoom is clamped, from 1/32 to 32 world units per pixel.

**Selection happens when the button is released over the same thing it was
pressed on.** A press that was a mistake is taken back by moving off the entity
before letting go, which is the ordinary way out of a misclick everywhere else.
Letting go somewhere else leaves the selection exactly as it was, rather than
selecting what the pointer happened to end up over or clearing what was already
chosen.

**What the gesture means is fixed when the button goes down**, and that includes
the modifier. Letting the key go a moment before the button still adds, because
the alternative is that an intended add becomes a replace and a selection
somebody spent a minute picking out is gone.

**Control and Command are both taken, on every platform**, rather than one
chosen per operating system. Unity binds this to Ctrl on Windows and to Command
on macOS, and macOS turns Ctrl and the left button into a secondary click before
the editor ever sees it, so a Mac user reaches for Command in any case.

One consequence, stated here rather than left to be discovered: **a plain click
on one of several selected entities collapses the selection to that one.** Unity
does the same. The gesture that will want to keep the group is dragging it,
which belongs to the parent that moves what is selected.

The left button is still what every tool that paints will want. What is decided
here is what it does when no tool has been chosen, which is the state the editor
is in and will stay in until phase 2.

### Rationale

§2 makes Unity the design target, and this is Unity's Scene view. Keeping
panning off the left button matters more here than it looks: the level editor in
[level-editor.md](./level-editor.md) is a sequence of tools that all want a drag
with the left button, and a viewport that took it for panning would have to hand
it back.

Releasing rather than pressing is not Unity's, which selects on the press. It is
taken because the cost of the difference is small and what it buys is a misclick
the user can still escape from.

**Bevy does not give that escape for free**, and an earlier draft of this section
said it did. Its click event does fire only when the press and the release share
a target, but the target of a click in the viewport is the viewport's own UI
node, which is the same node everywhere inside it; what is in the world is
reached through a second pointer that the node carries. So the engine's
guarantee says nothing about which entity is selected, and the editor keeps what
the press was over and compares it itself. Measured before it did: pressing one
entity and letting go over another selected the second, and pressing one and
letting go over empty space cleared a selection that was already there.

Zooming on the cursor rather than on the centre is Unity again, and it is the
difference between reaching a corner of a large level in one gesture and
reaching it by alternating pans and zooms.

The clamp is not a preference. An unclamped scale reaches zero, and a projection
with no width shows nothing and does not come back when the wheel is turned the
other way.

### Rejected options

**Space and the left button, as a second way to pan.** Also Unity, and also
Photoshop. It is the escape route for a mouse with no middle button, and it
carries modifier state and its own tests for a case nobody here has hit. It goes
in when somebody has that mouse.

**The right button to pan**, which is Godot's and Aseprite's. It takes the
button a context menu will want, and §2 says Unity wins where the two disagree.

**Zooming on the viewport's centre.** Two lines rather than four, and it makes
the viewport tiring to use at exactly the moment a level gets big enough to
need it.

**Shift as the key that adds and removes.** One key, nothing to say about
Command, and it is what a hand reaches for. Unity's Shift adds and never
removes, though: somebody who has picked out six things and shift-clicks one of
them expects it to stay, and it would go. That is the same loss as clearing on a
missed click, said in a different gesture.

**Unity's full pair**, Shift for adding only beside Ctrl/Cmd for adding and
removing. It is the most faithful reading of Unity and it is more than is needed:
a second rule, with its own tests, for a gesture nobody here has reached for.
Shift-as-add-only can be put in later without unpicking this, because it is a
rule beside this one rather than a change to it.

**Choosing the key by operating system**, Command on macOS and Control
elsewhere. Literal, and it buys a branch that cannot run on the machine the
tests run on. Taking both costs one line and is held by a named test.

**Selecting on the press**, which is Unity's and which composes more easily with
a rubber band: a press on empty space clears, and the drag that follows builds a
new selection, with no question about what the release meant. It was turned down
for the misclick above. What it costs is written where it lands, on the issue
for the rubber band.

### Accepted risk

A trackpad reports scrolling in pixels rather than in notches, and how many
pixels make a notch is a constant here rather than a measurement. Sixteen is
`PIXELS_PER_NOTCH` in `crates/editor/src/viewport.rs`; if it turns out to feel
wrong on a trackpad, it is one number and not a design.

Taking Command everywhere means that on Windows and Linux the Super key and the
left button add to the selection too. The window manager takes that combination
in most configurations, nothing else in the editor uses Super, and if it ever
gets in somebody's way it is one line.

---

## 5. What a selection looks like in the viewport

### Decision

**A rectangle on the entity's bounds**, drawn every frame from what is
selected. Every selected entity gets the same one; nothing distinguishes the
last one chosen.

**Only the viewport's camera draws it.** The outline is a gizmo group of its
own, on a render layer the viewport camera adds and the camera the panels are
drawn to does not.

The corners stay empty. They are where a transform gizmo puts its handles, and
a mark that looks grabbable and is not is worse than no mark.

### Rationale

§2 makes Unity the design target, and this is Unity's 2D Scene view: a thin
outline hugging the sprite. A beginner reads a box around a thing as "this is
the thing I picked" without being told.

The bounds are the `Aabb` that `bevy_sprite`'s `calculate_bounds_2d` already
computes, rather than a size read back out of `Sprite`. That is
[architecture.md §5](./architecture.md) applied: the outline is then right for
anything the world draws, and correct when a sprite is given a custom size or a
sub-rectangle, without this knowing how any of that works.

**Drawing it each frame rather than keeping something that represents it** is
what makes two of this decision's properties free. It follows the entity at
every zoom and after a pan because it is computed from the entity's transform
in the frame it is drawn, and it writes nothing to what is selected, so
selecting is not an edit to the document. Both are the things that would
otherwise have to be kept in step.

The render layer is what makes "inside the viewport" true by construction. The
alternative is to rely on the opaque panes covering whatever the window's camera
draws in world space, which is a claim about draw order between the 2D pass and
the UI pass that nobody here has measured.

### Rejected options

**Corner marks, or a box with handles at the corners.** That is a gizmo, and it
is a separate piece of work. Drawing something that looks like a handle before
one can be dragged teaches the wrong thing.

**A light line over a dark one**, so the outline reads over any artwork. It is
two draws and an offset for a contrast problem that has not appeared yet. It
comes back if the outline turns out to disappear over light art.

**A distinct colour for the last entity chosen.** Unity has an active object,
and `Selection` is an ordered list for exactly that reason. Nothing reads the
last one yet: the inspector is the thing that will, and it is a later chunk of
this milestone. Deciding what "active" looks like before anything acts on it is
deciding without the premise.

**An outline sprite as a child of the selected entity.** It writes `Children` to
the entity that was selected, which makes selecting an edit to the user's
document. That is the trap the selection chunk was written to avoid.

**A retained `Gizmo` entity per selected entity.** A second set of entities to
spawn, despawn and reconcile against a selection that changes on every click,
for a picture that has to be recomputed anyway when the entity moves.

### Accepted risk

**The colour is a constant in the code, not a theme token.** Feathers' tokens
carry colour only (§3) and there is none for an editor overlay drawn in the
world. It was chosen to read against the window's dark background and against
the three placeholders, which is not artwork. If it disappears over real art,
it is one value and not a design.
