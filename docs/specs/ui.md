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
| Left button, dragged from empty space | Draws a box; what it touches becomes the selection |
| Ctrl or Cmd with the left button, dragged from empty space | Adds what the box touches to what is already selected |
| Left button, dragged from an entity | Nothing yet, and kept for moving what is selected |
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

**A box takes what it touches, not what it encloses.** An entity the box
overlaps by any area is selected, whether or not it fits inside. Unity's Scene
view, Blender and Godot all read it that way and Unity's own Hierarchy reads it
the other, so this is a decision rather than a fact to look up. What it buys is
that something larger than the viewport can be selected at all, which is the
background sprite in every level.

**The box stops at the edge of what the viewport shows.** A drag that carries on
over the panels keeps growing until the box reaches the edge of the visible
world, and no further. Bevy keeps forwarding the pointer while the viewport is
being dragged and extrapolates past the region's edge, so without this an entity
that is off screen joins the selection, and the delete or the transform that
comes next takes it with no outline anywhere to say so.

**The release that ends a box drag also arrives as a click, and nothing
suppresses it.** Bevy emits the click before the end of the drag, and the box
writes the selection after it, so the only thing the click can do during a box,
which is clear the selection, is overwritten in the same frame. That is a
property of the order rather than of the editor, so the order is what is held,
by `a_release_that_ended_a_band_clicks_before_it_ends_the_drag`.

**A box is to rank what it covers, front to back, so that the last one in it is
the one in front.** It was deferred, with
the condition that it became answerable once an edit could be committed through
the inspector, and [§7](#7-what-the-inspector-can-edit) is that edit. The rule
is the one a click already follows: `select` takes the nearest of the entities
under the pointer, and a box is to take the entities it covers in the same
order, so that the entity the inspector calls active is the front-most of the
group rather than whichever one the world iterated first. Ties keep the world's
order, which is arbitrary and is said to be arbitrary rather than relied on.

**It is built, by `finish` in `crates/editor/src/selection.rs`**, which sorts
what a box adds by the world `z` of each entity, since a rectangle test has no
hit depth to read. Issue #47 was the change. The ranking, the tie and the
boundary between gestures are each held by a test named in that function's doc
comment.

The left button is still what every tool that paints will want. What is decided
here is what it does when no tool has been chosen, which is the state the editor
is in and will stay in until phase 2. A box drag over empty space is the part of
that a painting tool will have to take back, and it is the only part.

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
for the misclick above, and the rubber band turned out to need nothing from it.

**A box that selects only what it encloses**, which is Unity's Hierarchy and
Tiled's. It never takes something the user did not surround, and it makes
anything bigger than the viewport unselectable by box.

**A box that reaches past the edge of the viewport**, which is what the pointer
reports if nothing stops it. Two lines shorter, and it selects what the user
cannot see.

**A flag that suppresses the click a box drag ends with.** Easier to read than
an argument about ordering, and impossible to hold: deleting it leaves every
test green, because the box's own write always follows that click.

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

**While a box drag is happening, the viewport draws the box**: an outline in the
same colour, on the same render layer, in a gizmo group of its own, with nothing
filled in. It is gone the moment the button is let go.

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

**A translucent fill inside the box drag**, which is Unity's. A second draw and
a blend mode for a rectangle that exists for the length of a gesture.

**A colour of its own for the box drag.** One more constant for something that
is only ever on screen while the button is held, beside the only other thing
drawn there.

**The box drag in the outline's gizmo group.** One group rather than two, and it
takes away how the outline's tests tell "nothing is drawn" from "something is":
they read whether that group has a handle at all.

### Accepted risk

**The colour is a constant in the code, not a theme token.** Feathers' tokens
carry colour only (§3) and there is none for an editor overlay drawn in the
world. It was chosen to read against the window's dark background and against
the three placeholders, which is not artwork. If it disappears over real art,
it is one value and not a design.

---

## 6. What the inspector shows

### Decision

**Every component the entity has**, listed by its short type name, sorted by
that name, under a header naming the entity, and **each one opened into the
values it carries**.

| | |
| --- | --- |
| Which components | **all of them**, engine internals included. No filter, no denylist |
| The name | the value's own `short_type_path()`. `Transform`, not `bevy_transform::components::transform::Transform` |
| A component with no reflection | **a row saying `<no reflection>`**, dimmed, rather than nothing |
| What a component opens into | a named-field struct: **one line per field**, the field's name and its value, in the order the type declares them. Any other shape: **one line carrying the whole value**. A struct with no fields: nothing |
| What a value reads as | its `Debug`, through reflection. `Vec3(0.0, 0.0, 0.0)`, `Inherited`, `Anchor(Vec2(0.0, 0.0))` |
| How deep it goes | **one level, and a second one only into numbers.** A field whose own value is a named-field struct of nothing but `f32` opens into those leaves, drawn as one line carrying a box each; anything else stops at one level. So `translation` is a line of three boxes and `color` is a line reading `Srgba(...)`. [§7](#7-what-the-inspector-can-edit) is what the second level is for |
| The header | the entity's `Name` if it has one, otherwise `Entity 356v0` |
| More than one entity selected | the header adds the count: `Player (1 of 3 selected)` |
| The order | the component names, ascending. Every unregistered row goes after every named one. **Field lines are not sorted**: `translation, rotation, scale` is what `Transform` means |
| Nothing selected | an empty pane |
| Longer than the pane | **the pane scrolls**, with the wheel, and clips across its width |

**The value is read with `World::get_reflect`**, which needs only
`ReflectFromPtr`. `#[derive(Reflect)]` inserts that unconditionally, where
`ReflectComponent` needs `#[reflect(Component)]` as well, so reading through
`ReflectComponent` would see fewer components than the engine's own call does.
The name comes off the same value, so the two cannot disagree about which type
they came from, and a component whose value cannot be read is exactly the one
that gets the `<no reflection>` row.

**Opening only named-field structs is not a special case**: measured on one
clicked placeholder, 6 of its 13 readable components are named-field structs and
7 are tuple structs or enums. The single line those seven get is the ordinary
path, not the exception.

**A value is its `Debug` text because the alternative is a table of type
names.** `Debug` through `PartialReflect` is defined for every shape reflection
can take, so there is no arm that panics and no type named in the editor's code,
which is the property this section chose the component list for. What it costs
is a line like
`Srgba(bevy_color::srgba::Srgba { red: 0.45, green: 0.68, blue: 0.85, alpha: 1.0 })`,
which is row 4 of `CLAUDE.md`'s list: wrong-looking, on screen, where the user
can point at it.

**The header says how many are selected because the panel now carries values.**
[§4](#4-what-the-mouse-does-in-the-viewport)'s box drag puts several entities in
the selection at once and `crates/editor/src/selection.rs` ranked none of them
when this was written, so the entity the panel was about was an arbitrary
member of that group. While the
panel held names only, that was cosmetic. With values in it, a header naming one
entity and saying nothing else reads as a claim about the only thing selected,
and the issue after this one lets that value be edited. Saying `1 of 3` does not
choose better; it makes the choice visible, which is the difference between row
4 and row 6. Whether the box drag should rank what it covers was the half of this
left open then; §4 has since decided it and the code ranks front to back.

**The name is no longer the type registry's**, and that is a measurement rather
than a preference. `ComponentInfo::name()` returns a `DebugName`, which
carries nothing unless `bevy_utils/debug` is on; the feature line §3 settles
does not reach it, so every component answers
`<Enable the debug feature to see the name>`. There is no fallback under the
registry, which is why the unregistered row says what it says instead of
guessing.

A component that derives `Reflect` is registered without anybody asking:
`reflect_auto_register` is on through Bevy's `default_app`. So the user-defined
components phase 3 brings ([roadmap.md §3](./roadmap.md)) arrive here on their
own, and the unregistered row is for what this workspace attaches without the
derive.

### Rationale

**Listing everything is the option whose failure the user can see.** Measured on
one placeholder that has been clicked: 14 components, of which `Transform` and
`Sprite` are the two anybody came for and `TransformTreeChanged`,
`ViewVisibility` and `SyncToRenderWorld` are among the twelve they did not. That is noisy, and
noise is row 4 of `CLAUDE.md`'s failure list: wrong on screen, and visible.
Hiding components is row 4 as an absence, which is the same row and cannot be
seen at all. Between two failures on one row, the one the user can point at
wins.

It also declines to decide what "less" means. The thing that could decide it
properly is the reflection schema phase 3 extracts, which is what lets the
editor tell a component the user defined from one the engine attached. That
deferral has a trigger, in
[open-questions.md §1](./open-questions.md).

**The header names the entity the way Unity names a GameObject**, which is §2
applied. Bevy's `Name` is optional and nothing in the world carries one yet, so
the fallback is what is actually on screen; the `Name` branch is written now
because the first named entity is phase 2's and coming back for it is how it
gets forgotten.

**Sorting is what makes the panel the same twice.** The archetype hands its
components back in type registration order, which is plugin build order:
measured, `GlobalTransform` comes before `Transform`. That means nothing to a
reader and it moves when a plugin is added.

### Rejected options

**Only the components the registry names.** Thirteen rows rather than fourteen.
It differs from the chosen option by exactly the one row that says something is
there and unnamed, so it buys tidiness by deleting the honest part.

**A hand-written list of components to hide.** The Unity-shaped picture, three
rows. It is a list nobody can check against anything, written before the schema
that would justify it, and rebuilt in phase 3. It also puts component type names
in the editor's code, which is the thing an inspector driven by reflection is
supposed not to have.

**`Transform` pinned to the top, the rest alphabetical.** Unity's own layout,
and the most tempting option here. It needs a named type in the code for exactly
the reason above.

**The archetype's order, unsorted.** Shortest, meaningless to a reader, and not
stable against adding a plugin.

**`Unnamed` in the header when there is no `Name`.** Tidier than an id, and
selecting one unnamed entity after another leaves the header unchanged, so the
panel stops saying that the selection moved.

**Descending to the leaves**, so that `translation` opens into `x`, `y` and `z`.
The closest to Unity, and what editing a number will eventually want. It means
deciding now what bounds the recursion, what an enum, a map and a list do, and
what `GlobalTransform`'s `Affine3A` looks like several levels down. A recursion
whose stop condition is wrong is row 5, the viewport not answering, rather than
row 4.

**A narrow form of this was taken in [§7](#7-what-the-inspector-can-edit), and
the reason it was turned down here is the reason for its bound.** That second
level is not a recursion: it opens a named-field struct whose fields are *all*
`f32` and stops there, so there is no stop condition to get wrong and no enum,
map or list to decide about. What editing a number wanted arrived; the general
descent this paragraph rejected is still rejected.

**Opening only named-field structs and saying `<shape not shown>` for the rest.**
The least code. Measured, it would be 7 of the 13 readable rows on a
placeholder, `Visibility`, `Anchor` and `GlobalTransform` among them. It is the
rejected "only the components the registry names" above in a new place: it buys
tidiness by deleting the honest part, and its failure is an absence nobody can
see.

**A table of known types**, `f32` printed as `0`, `bool` as a tick. Tidier than
`Debug` for the types it knows and silent for the ones it does not, and it is
the thing an inspector driven by reflection is supposed not to have.

**An empty panel reading `3 selected` when several are.** It removes the chance
of showing a value from an entity the user was not looking at by removing the
values. Unity edits several objects in its inspector rather than blanking it,
and §2 says Unity wins where the two disagree.

**Ranking what a box drag covers**, so that "the last one chosen" means
something inside a boxed group and the header can name it honestly. That is a
change to what a selection is, in §4, §5 and `crates/editor/src/selection.rs`,
and none of the inspector's criteria need it. It stays open, with its trigger,
in [open-questions.md §1](./open-questions.md).

**Turning on Bevy's `debug` feature**, which makes `ComponentInfo::name()` work
and gives every component a name, registered or not. It costs an amendment to
§3's feature line, which `xtask`'s
`every_manifest_asks_for_the_features_the_specification_settled` holds, for a
fallback that only ever names types this repository owns. It comes back if the
missing names start costing time: the engine's own conflict diagnostics are
unreadable without it, which is already visible in
`crates/editor/src/inspector.rs`.

### Accepted risk

**The panel is noisy, and will stay noisy until phase 3.** Twelve of the
fourteen rows on a placeholder are the engine's, one is the editor's own
`Selectable` and one is the `Sprite` the user put there. Nobody can edit any of them yet, so what
it costs today is reading past them; what it would cost to fix today is a list
of type names that phase 3 deletes.

**Collapsing is not here.** Fourteen component names fit a 512-pixel pane; the
22 value lines under them do not. **Measured before the pane was bounded**, in
the default 1280 by 720 window: the row of panes grew to 882 pixels, the menu
bar and the bottom panel were left one pixel each where §1 asks for 28 and 180,
and the viewport moved out from under the pointer, which is row 4 and row 5 at
once.

**Scrolling was deferred here, and the deferral is withdrawn.** This section
said scrolling arrives "when something does not fit, rather than now". Running
the editor showed what that cost: in the window the editor opens at, the clip
falls inside `Sprite`, so `Transform`, the one component anybody opens the
inspector for, is below the fold and unreachable. That is not row 4 with a
visible cost, it is the panel failing at its job while looking finished. The
pane now scrolls with the wheel, through `bevy_ui_widgets::ScrollArea` and a
`ScrollPosition`, both of which arrive with `DefaultPlugins`; neither is a
mechanism this project wrote.

**The scroll position lives on the pane, not on the panel drawn into it**,
because the panel is despawned and respawned whenever anything in it changes,
hovering a sprite included. It goes back to the top when the selection moves to
**another entity**, and only then: a position into one entity's list means
nothing in another's, while a rebuild of the same entity is the ordinary case
and must not throw the reader back to the top.

**There is no scrollbar.** The wheel scrolls, and nothing on screen says the
panel is longer than the pane. `bevy_feathers` ships `FeathersScrollbar`; what
keeps it out is that `show` despawns the pane's children on every rebuild, so a
scrollbar beside the panel would have to survive that. That is a change to how
the panel is built rather than a widget to add, and it is the next thing this
pane wants.

**A value that is a `Reflect` type with no `Debug` registered reads as its whole
type path.** `Anchor(Vec2(0.0, 0.0))` is short because `bevy_reflect` was told
to use the real `Debug`; a component a user writes without `#[reflect(Debug)]`
reads `my_game::enemies::Weight(2.5)` instead. It is what a phase 3 component
looks like here until it says otherwise, and it is one line of noise rather than
a wrong value.

**Every value is formatted every frame**, because
[ADR-0002](../adr/0002-rebuild-the-inspector-rather-than-diff-it.md) compares
the finished picture and the picture now holds the text. That is 22 short
strings a frame for one placeholder. A component carrying a large array would
make it row 5, and phase 2's tile data is the first thing that could; the answer
then is to bound what a row formats rather than to start watching for changes.

**After a box drag the header named an arbitrary one of what the box
covered, until issue #47.** This paragraph recorded that §4's box adds several entities in one
gesture while `crates/editor/src/selection.rs` ranks none of them, so "the last
one chosen" has no answer inside a boxed group, and it named the condition under
which that would have to be settled: a value being written back.
[§7](#7-what-the-inspector-can-edit) is that value, so §4 has now decided the
answer, front to back, and issue #47 built it: the header now names the
front-most of a boxed group. Before that, an edit committed through the
inspector could land on an entity the user did not single out, which is what
the count in the header made visible rather than fixed.

---

## 7. What the inspector can edit

### Decision

**A number, typed into the box beside its name, and committed.** Nothing else on
the panel is editable.

| | |
| --- | --- |
| What is editable | every `f32` leaf the second level of [§6](#6-what-the-inspector-shows) opens. On a placeholder that is `Transform`'s nine numbers and nothing else |
| What is not | every other value. It stays the `Debug` text §6 draws |
| The box | `bevy_feathers`' `FeathersNumberInput`, which already refuses every character that is not a digit or one of `.-+eE` |
| The name beside it | the leaf's own name through reflection, `x`, `y`, `z`, `w`, dimmed, drawn by this project rather than by the widget |
| What commits | **Enter, or the box losing focus.** Typing moves nothing |
| A value that does not parse | **nothing is written.** The box keeps the typed text until it loses focus, and the real value comes back with the rebuild that follows |
| Where the write lands | one `f32` leaf, of one field, of one component, on the active entity, through `World::get_reflect_mut` |
| While a box has focus | **the panel is not rebuilt**, so the box survives the value it just changed, and for one frame after the focus leaves, so that it survives its own commit. [ADR-0003](../adr/0003-the-inspector-panel-belongs-to-the-user-while-focus-is-in-it.md) |
| More than one entity selected | only the active entity, which is the one the header names and, since §4, the front-most of the group |
| Taking it back | **not yet.** Undo is later in the phase ([roadmap.md §3](./roadmap.md)); typing the old number back is what there is |

### Rationale

**Committing on Enter, and not on every keystroke, is the whole shape of this.**
`FeathersNumberInput` emits a value on each keystroke as well, marked
`is_final: false`, and taking those would move the sprite through `-`, `-1`,
`-12` on the way to `-120`. With no undo yet, each of those is a change to the
user's work that nothing can take back, and it is also a `GlobalTransform`
recomputed per character. Enter and focus loss are what Unity commits on.

**The write is the mirror of the read, and has to see the same components.**
§6 records that the panel reads through `World::get_reflect`, which needs only
`ReflectFromPtr`, because `ReflectComponent` would see fewer components than the
engine's own call does. `World::get_reflect_mut` is that call with `&mut World`,
so the set that can be written is exactly the set that is shown.

**The value cannot be silently wrong, because nothing writes a value it could
not parse.** The widget filters the characters, and when what is left still does
not parse it emits nothing at all. There is no `unwrap_or_default` anywhere on
this path, which is the shape that would put a zero over somebody's number:
row 6, and the only row on `CLAUDE.md`'s list that destroys something.

**What is on screen is one box per number, not one box per line.** `translation`
stays one line, carrying three boxes, which is how Unity draws a vector and is
what `FeathersNumberInputProps` is shaped for. Three separate lines would say
that `x` is a sibling of `rotation`.

### Rejected options

**Writing on every keystroke.** It is what the widget offers first, and with
two-way sync it is what feathers documents. Without undo it makes every
intermediate state a permanent edit, and it is the option that would need undo
built first.

**The widget's own axis label**, `FeathersNumberInputProps::label_text`. It is
an `Option<&'static str>` and a reflected field name is a `String`, so using it
means a table mapping four literals to four names and a second path for every
other leaf. One label drawn here is one path, and costs a strip of colour the
widget would have drawn beside it.

**Editing the whole `Vec3` as one text field**, leaving §6 untouched. No second
level, no widget, and a parser written here. It also means a typo in one
component rewrites all three, which is the difference between an edit the user
can see and an edit they cannot.

**Writing through `ReflectComponent`.** The issue that asked for this named it.
It is the wrong half of the pair §6 already chose between.

**Writing through an `EditorCommand` into an Editor Model**, which is the shape
[data-model.md §1](./data-model.md) describes. It is where this goes, and there
is no Editor Model yet. §1 records the divergence and the condition that ends
it.

**Blanking the panel, or refusing to write, when several entities are
selected.** §6 already rejected blanking on the grounds that Unity edits several
objects rather than showing nothing, and refusing to write is the same absence
one step later. §4 now ranks a boxed group instead, which makes the entity the
write lands on the one the user can see named.

**A drag on the label to scrub the value**, which Unity has. More input
handling, on a gesture the viewport also claims, for something the keyboard
already does. It comes back when somebody asks for it.

### Accepted risk

**An edit cannot be taken back.** Undo is later in the same phase. What this
costs is retyping a number that is on screen the whole time: row 4. What would
make it row 6 is a write the user does not see happening, and the only write
there is happens in a box they are looking at, on the entity the header names.

**A value something else recomputes can be typed into.** `Aabb`'s centre is a
struct of `f32` like any other, so the panel lets it be edited; the edit lands,
the next frame overwrites it, and the box snaps back when focus leaves. That is
§6's noise seen from the writing side, and §6's answer applies unchanged: a
failure the user can point at beats a list of type names that phase 3 deletes.

**Rotation is edited as four raw numbers.** `Quat` reflects as `x, y, z, w`, so
a sprite can be put into a state that is not a rotation. Unity shows Euler
angles and converts. Doing that here means naming `Quat` in the editor's code,
which §6 spent its whole component list avoiding, and it carries its own
question about which three angles. It comes back when somebody turns a sprite
from the inspector and finds out.

**While a box has focus, every other line on the panel is stale.** Committing a
translation does not redraw `GlobalTransform` until focus leaves, because the
panel is not rebuilt while the user is in it. Row 4, bounded by how long one box
holds focus, and the alternative was the box disappearing under the cursor.
[ADR-0003](../adr/0003-the-inspector-panel-belongs-to-the-user-while-focus-is-in-it.md)
is where that was weighed.

**A write that cannot land leaves the box and the component disagreeing** until
focus leaves. The way to reach it is the component being removed between the
panel being drawn and Enter being pressed, which nothing in the editor does yet.
Nothing is written and nothing is said, which is row 4 for as long as the box
holds focus.

**Letting go of a box writes what is in it, whether or not it was typed into.**
`bevy_feathers` emits on focus loss regardless, so clicking into a box and
clicking out again writes the number that was already there back to the
component. That is a no-op unless the value moved while the box held focus, and
while it does the panel is frozen, so the only thing that could move it is
something other than the inspector. Nothing in the editor is that today; the
gizmos and the undo of [roadmap.md §3](./roadmap.md) are the first that would
be, and what it would look like then is the user's own older number quietly
coming back. Row 6 in miniature, reachable only once a second writer exists,
and the thing that would close it is a box that knows whether it was edited.
