# Scenario editor

Editing scenarios: the manuscript UI, and why a script language was not built.

---

## 1. Scenario editor

The editor for scenario games, adventure games and visual novels.

It is a separate editor plugin from the level editor.

### The thing that matters most

> **A scenario writer is never asked to write a script.**

What a writer touches is **a manuscript UI**, and there is no syntax to learn.
Direction is attached to lines and paragraphs as items chosen in the UI.

The whole design, and the reasoning behind not building a language, is in **§2**.

### The concepts

```text
Scenario
├── dialogue
├── characters
├── backgrounds
├── choices
├── branching, rejoining, returning to a choice
├── flags and numeric variables
├── events
├── music
├── sound effects
└── scene transitions
```

### Three views

```text
manuscript UI (editing)  ──→  the data  ──→  text view (reading)
                                         ──→  graph view (overview)
```

**The manuscript UI is the only editing surface.** The text and graph views are
read-only.

### What the scenario graph is for

The graph is **a read-only overview generated from the data**, not a place to
edit.

```text
start
  │
  ▼
choice
 ┌┴─────────────┐
 ▼              ▼
route A       route B
 │              │
 └──────┬───────┘
        ▼
       end
```

A long adventure game's text is beyond what a node graph can be used to edit.
The graph is for understanding the route structure and navigating it (§4, §2).

### The boundary

**This does not become general-purpose visual scripting**
([concepts.md §8](../concepts.md)).

The boundary is not "a scenario is a DAG". Loops are needed: returning to a
choice, talking to an NPC again. The DAG constraint in §3 was withdrawn in §4.

The boundary now is the line in [concepts.md §8](../concepts.md):

> **A scenario describes progression over time. It does not describe the game's
> behaviour.**
> The test: does this complete in one frame, or does it span time?

Plus one constraint from §2: **no data structures.** Variables are scalars and
references to database rows. No arrays, dictionaries or objects.

---

## 2. Writing a scenario: the manuscript UI

### 2.1. Decision

**Scenarios are written in a dedicated UI, not a script language.**

What a writer touches is **a manuscript UI**, with no syntax to learn.

Direction, backgrounds, character sprites, animation, music, durations, is
specified from the UI as **items attached to a line or a paragraph**.

```text
manuscript UI (editing)  ──→  the data  ──→  text view (reading)
                                         ──→  graph view (overview)
```

The manuscript UI is the only editing surface. The text and graph views are
**read-only**, the same judgement §4 makes about the graph.

### 2.2. Why not a script language

The original plan was to design a script language that fixed what is wrong with
Ren'Py, KiriKiri's KAG, and NScripter. Four syntaxes were drafted: a prose form,
a screenplay form, a Markdown dialect, and one separating prose from direction.

But **the question itself was wrong.**

#### What the research actually said

The top answers in KiriKiri Z's own survey of why people do not use it:

```text
22  no usable adventure-game system     ← the most common answer
21  too much trouble to look things up
 8  you cannot make a game through a GUI
```

And from the free-text answers:

> "I could not tell which of Krkr and KAG was the development tool"

**Ren'Py, KiriKiri and NScripter all shipped a language and did not ship a
tool.** The complaints were not about language design. They were about the
absence of a tool.

This project is building an editor from the start. **There is no reason to
compete on language.**

#### Do not ask a non-programmer to write a script

Scenarios are written by writers and designers, not engineers. However far the
syntax is reduced, the cost of learning it does not reach zero.

**With a UI it does.**

### 2.3. This makes the design simpler

**§2 becomes a data format rather than a language.** The bar drops.

| | As a language | As a data format |
| --- | --- | --- |
| Approachable syntax | the whole problem | **not needed** |
| Parse error recovery and messages | required | **not needed** |
| Removing ambiguity | hard | **does not arise** |
| Static checking | has to be implemented | **prevented at input** |
| Symbols to memorise | to be minimised | **none** |

An invalid asset path is not **detected**; it is **impossible to enter**,
because it is picked from a list.

Of the eight design principles originally drafted, **everything about syntax
disappears.** Three survive, and each is strengthened by being a UI.

| Principle | After |
| --- | --- |
| An id map, for save compatibility and localization | **easier to manage from a UI** |
| Commands registered through reflection | **still true; the UI items generate themselves** |
| No data structures | unchanged |

### 2.4. Direction items are generated from reflection

When a user defines a direction command in Rust, **an item appears in the
direction palette on its own.**

```rust
// in the user's game crate
#[derive(ScenarioCommand, Reflect)]
struct ShakeScreen {
    power:    f32,
    duration: f32,
}
```

The field types decide the input UI too.

```text
f32            → a slider or a number field
AssetPath      → a file picker
bool           → a checkbox
enum           → a dropdown
a database row → a chooser over the rows
```

This is exactly the mechanism in
[database-editor.md §1](./database-editor.md), and it follows from
[data-model.md §3](./data-model.md) making reflection a central technology.

**Reflection is this project's backbone.** The inspector, the database and the
direction palette all run on one mechanism.

### 2.5. What only a manuscript UI can do

Two things a language cannot offer at all.

#### Showing the line-length limit

An adventure game's text window has a limit on **characters per line** and
**lines per screen**. Writing in a script, you find out you overflowed when you
run the game.

**The manuscript grid is the text window's width.** Overflow is visible where it
happens.

#### A live preview at the cursor

Put the cursor on a line and **the viewport shows what the player sees at that
moment**: background, character sprites, expressions and music, in the state
they are in at that line.

Principle 3 of [concepts.md §9](../concepts.md), that every edit shows its
result immediately, becomes true for scenarios too. **This is what makes the UI
worth building.**

It connects naturally to preview in
[runtime-and-play.md §2](./runtime-and-play.md).

### 2.6. Presentation: a manuscript grid, or standard manuscript format

The same data, shown in whichever standard format the locale expects.

```text
Japanese   a manuscript grid, 20 by 20 characters
English    standard manuscript format: monospaced, double spaced, indented
```

A presentation layer. The data underneath is one thing.

This meets the principle that localization is not bolted on afterwards.
**Translation work happens in the target locale's own standard format.**

### 2.7. What the MVP has to include

The following are not features to add later. **Without them the tool does not
get used.**

#### An import path

**In commercial adventure game production, writers deliver prose as Word or text
files.** Writing away from the machine, writing in another tool, bringing in an
existing draft. A tool that cannot take those does not get adopted.

* import from plain text and Word
* recognise the `name "line"` shape and split it into lines
* direction is attached afterwards, in the UI

#### Bulk editing

Writers always do this.

```text
rename a character everywhere
replace one speech pattern with another across every file
find every line containing a particular word
```

In text files, any editor does it in seconds. **A UI that cannot do this is
plainly worse than a script.**

* search and replace across every scenario
* regular expressions

### 2.8. The data format

What the UI edits gets saved.
[architecture.md §6](./architecture.md) requires that it be serializable,
inspectable and **version-controllable**.

**The guideline: a line-oriented format where one paragraph is one line.** Then
`git diff` reads, even though a UI produced the file.

The bar is lower than it was. It has to be **a format a human can read**, not
one a human writes.

Whether that is BSN or something of our own is open
([open-questions.md §1](./open-questions.md)).

### 2.9. Accepted risk

* **It cannot be written in a text editor.** That is a step back from the spirit
  of principle 5 in [concepts.md §9](../concepts.md), not hiding Bevy. The
  read-only text view softens it, and two-way editing is a later question.
  Guaranteeing that a round trip agrees is expensive, so it is not attempted at
  the start.
* **The UI is a lot of work**: the manuscript grid, the direction track, live
  preview, search and replace, import. Phase 5's estimate has to carry it.
* **Performance on a large scenario.** A long adventure game's text is a lot of
  text. An outline view and lazy loading will be needed.

### 2.10. Still open

* the data format itself, BSN or our own
* the manuscript UI in detail: the grid, how the direction track is shown
* how choices and branches are presented
* how conditions on flags and numbers are presented
* how the id map actually works
* how far the Word import goes

---

## 3. The scenario graph's boundary: a DAG with fixed node kinds (withdrawn, see §4)

> **This section was withdrawn in §4.** Neither the DAG constraint nor limiting
> conditions to flags holds. What survives is only the placement decision, that
> the node graph UI belongs in `editor_ui`.
> It is kept as a record. As a decision it is void.

### Decision

**Two lines are drawn.**

```text
line 1 (permanent)   a scenario is a DAG.
                     no loops, no functions, no jumps.

line 2 (for now)     node kinds are fixed in an enum.
                     the user cannot add one.
```

Conditions are **limited to flags**. No numeric variables, no comparisons.

The node graph UI belongs in **`editor_ui`**, following the dependency graph in
[crates.md §3](./crates.md). The node kinds are an enum in the **`data`** crate.

```rust
// in the data crate
enum ScenarioNode {
    Dialogue   { .. },
    Character  { .. },
    Background { .. },
    Choice     { .. },
    SetFlag    { .. },
    IfFlag     { .. },      // conditions are flags only
    PlayBgm    { .. },
    StartLevel { .. },
    End,
}
```

### Why this needed deciding

[concepts.md §8](../concepts.md) states plainly that no general-purpose visual
scripting system is built here. Meanwhile the scenario graph in §1 and
[integration.md §2](./integration.md) carries branching, flags, events and
conditions.

**Nothing said where the difference lay.**

Add a conditional node, add a variable, add a loop, one step at a time, and it
is general-purpose visual scripting by the time anybody notices. Keeping
[concepts.md §8](../concepts.md) requires an actual line.

### Line 1 is the real barrier

**Without loops it does not become general-purpose scripting.** Variables,
arithmetic and conditions together still get nowhere near Turing complete
without control flow.

And in fact, **no case that needs a loop comes to mind.**

* an adventure game's scenario branches and rejoins: a DAG
* the trigger integration in [integration.md §1](./integration.md) is one
  direction too: trigger, scenario, flag, level event

Line 1 captures something true about what a scenario is, so **it can be set as a
permanent constraint.**

### Line 2 is provisional

Fixing the node kinds in an enum makes **the compiler hold the line.** Adding to
the enum is an explicit decision every time.

It is there to keep the implementation small, and it can be relaxed. Unlike line
1, it is not essential.

### Rejected option

**B. Allow expressions, with numeric variables and comparisons**

**That is the doorway to general-purpose visual scripting.** Once variables,
arithmetic and conditions are all present, only loops stand between it and
Turing completeness.

Where a numeric comparison is genuinely needed, principle 5 of
[concepts.md §9](../concepts.md) applies: write a component in Rust and call it
from a trigger.
**Not trying to express everything in the editor is how
[concepts.md §8](../concepts.md) is kept.**

### What a node graph UI costs

Jackdaw has **3,460 lines** of dedicated implementation in
`jackdaw_node_graph`. Neither the five-crate layout in
[crates.md §2](./crates.md) nor the UI stack in [ui.md §3](./ui.md) accounted
for that.

**It does not need a crate.** By the test in [crates.md §3](./crates.md), a node
graph UI belongs in `editor_ui`, the generic widget layer that knows nothing
about levels or scenarios. Being a widget that does not know about levels is
itself what keeps
[architecture.md §6](./architecture.md), that the editor UI must not become the
source of truth.

Phase 5's estimate has to carry that size.

---

## 4. Withdrawing §3: the DAG constraint does not hold

### Decision

**§3 is withdrawn.** Specifically, two constraints are taken back:

* ~~line 1 (permanent): a scenario is a DAG, with no loops, functions or jumps~~
* ~~conditions are flags only; no numeric variables or comparisons~~

What replaces them is the line in [concepts.md §8](../concepts.md), between a
scenario and the game's behaviour.

### Why

**One: loops are needed.**

§3 set line 1 as a permanent constraint on the grounds that no case needing a
loop came to mind. **That ground was wrong.**

Widening the target genres made it clear that loops are the basic shape of
adventure games and RPGs.

```text
investigation adventures   pick a thing to examine → look → back to the choices
RPGs                       talk to an NPC as many times as you like
adventure games generally  see one choice, then go back for the others
```

Returning to a choice is fundamental to the genre, and a design that forbids it
does not stand.

**Two: numeric conditions are needed too.**

§3 said to write it in Rust when a numeric comparison is needed, but
**in an RPG numeric conditions are the norm, not the exception.**

```text
this choice only if you have 100 gold
this conversation only above a certain class level
this line if affinity > 3 and attempts > 10
```

Pushing all of that into Rust breaks the goal in §1, that a writer builds the
story without writing code.

### What survives

From §3, **the following still holds.**

* the node graph UI belongs in `editor_ui`, following the dependency graph in
  [crates.md §3](./crates.md)
* it is a generic widget that knows nothing about levels or scenarios, which is
  what keeps
  [architecture.md §6](./architecture.md), that the editor UI must not become
  the source of truth

**What changes is what the graph is for.** In §3 it was an editing surface; by
§2 it becomes **a read-only overview generated from the data.** Phase 5's
estimate gets lighter by exactly the editing half.

Fixing node kinds in an enum (line 2) is likewise replaced, by the data format
in §2.

### The lesson

§3 recorded a constraint as permanent and **withdrew it two sections later.**

The cause was assuming, when the constraint was written, that the target genres
were as narrow as [concepts.md §2](../concepts.md) described them. Nobody
checked that [concepts.md §2](../concepts.md) never mentions RPGs at all, and a
permanent constraint was built on top of that.

**Before writing that a constraint is permanent, state the premise it rests
on.**
