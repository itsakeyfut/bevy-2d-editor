# Runtime and play

How the editor and the game are arranged as processes, and the two kinds of
running.

---

## 1. The editor and the runtime are separate processes

### Decision

The editor and the game are **separate processes** talking over IPC. They do not
share one Bevy App.

### Rationale

This is where Jackdaw arrived after four months of trying the alternative. Its
`architecture.md` puts it this way:

> Play is the same artifact: the editor launches the project's own binary as a child process and talks to it over IPC. What you Play is what `cargo run` would run, and **a game crash cannot take down the editor.**

What that buys:

* what runs is what `cargo run` runs, so there is no behaviour to reconcile
* a game crash does not take the editor with it
* game code is never linked into the editor binary, so the TypeId problem cannot
  arise at all
* the user's game stays an ordinary Bevy binary

Type information comes from the built binary, which is asked to emit its
reflection schema (Jackdaw's `--jackdaw-extract-schema` does the same job).

### The first build, and what makes it bearable

A consequence of this arrangement: **showing a user's own components in the
inspector requires building their project.** Jackdaw's book is blunt about the
cost.

> Expect that first build to take around **nine minutes**: it compiles Bevy
> from source, the same as any Bevy project. Every project pays it once.
> Rebuilds after that are 1 to 4 seconds.

That collides with the first principle in [concepts.md §9](../concepts.md), that
a beginner can start without preparation. Jackdaw's answer is adopted here
unchanged.

> A new project opens immediately. ... **Placing brushes and saving scenes
> works right away, so you do not have to wait for it.**

**Stated as design requirements:**

* the editor **opens immediately**; it does not wait for a build
* **placing tiles, placing sprites and saving all work before the build
  finishes**
* the only thing that waits is the user's own components appearing in the
  inspector
* §2's **preview also works before the build**, which is where this project goes
  further than Jackdaw does

---

## 2. Two kinds of running: preview and play

### Decision

Running is **split in two.**

| | **Preview** (phase 1) | **Play** (phase 3) |
| --- | --- | --- |
| Process | the editor's own | a separate one (§1) |
| Game code | **not included** | the user's binary itself |
| What moves | tiles, sprites, camera, animation | all game logic |
| Build | **none; immediate** | `cargo build` first |
| For | checking how it looks and sits | checking that it works |

Preview replays the level inside the editor's own Bevy World, so it needs
**neither IPC nor the build pipeline.**

The UI distinguishes them plainly: Preview, and Run game.

### The contradiction this resolves

The initial MVP in [roadmap.md §1](./roadmap.md) and phase 1 in
[roadmap.md §3](./roadmap.md) both listed running, and phase 3 listed run mode,
stop mode and editor-to-runtime communication as well. **The same thing appeared
in two phases.**

And by §1, running properly means all of this:

* a `cargo build` pipeline over the user's project
* extracting the reflection schema from what it produced
* spawning a child process and managing its lifetime
* IPC, whose transport is still open
  ([open-questions.md §1](./open-questions.md))

Jackdaw implements that across `jackdaw_project_build` (6,657 lines),
`jackdaw_pie_protocol` (1,013 lines) and more, and **spent April to August 2026
redesigning it** (the timeline is in [architecture.md §7](./architecture.md)).

That is not one line item in an MVP.

### Rejected options

**A. Drop running from phase 1 entirely, and keep it only in phase 3**

The most honest option, and it closes phase 1's scope cleanly. But **nothing
built in phase 1 is ever seen moving.** The third principle in
[concepts.md §9](../concepts.md), that every edit shows its result immediately,
would stop at a static viewport, and there would be no way to check the work.

**C. Put real play in phase 1**

Phase 1 would then contain Jackdaw's four months of work, against
[roadmap.md §1](./roadmap.md)'s instruction not to build everything at once.

### Why this was chosen

**1. It softens the first-build wait that §1 creates.**

Jackdaw states the cost:

> Expect that first build to take around **nine minutes**: it compiles Bevy
> from source, the same as any Bevy project.

and its mitigation:

> A new project opens immediately. ... **Placing brushes and saving scenes
> works right away, so you do not have to wait for it.**

Preview extends that idea: it **makes the editor worth opening before the build
pipeline exists, and before the user's own build finishes.**

**2. It suits a 2D editor.**

Unlike 3D, a 2D game's visuals are almost entirely tiles, sprites, camera and
animation. **Without any game logic, that is already a useful thing to look at.**
It is close to the relationship between Unity's Scene view and Game view.

### Accepted risk

* **"It worked in preview but not in play" can happen in principle.** Editor and
  runtime share a renderer ([level-editor.md §2](./level-editor.md)), so what is
  drawn agrees; what can differ is whether game logic runs. Naming the two
  clearly in the UI is what keeps that from being a surprise.
* Two paths are needed: one that expands a level into the editor's own World for
  preview, and one that hands it to the game for play. The first is the Editor
  Model to ECS path from [data-model.md §5](./data-model.md), which the viewport
  needs anyway.
