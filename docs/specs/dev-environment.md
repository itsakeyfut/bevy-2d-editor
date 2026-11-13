# Development environment and documentation

How development is set up, and how the documentation is built.

---

## 1. Development environment

The editor is GUI, GPU and windowing work, so Windows native is the main
development environment.

```text
Windows
├── Rust
├── Bevy
├── Level Editor
├── Scenario Editor
├── Rider
├── Git
├── Aseprite
└── Blender

WSL2
├── Claude Code
├── Linux CLI
├── shell scripts
└── Linux-specific development tools
```

**The product itself is not Windows-only.** Bevy and Rust are cross-platform and
this project keeps that property, with CI covering Linux and macOS.

**What the gate costs on that machine, now that Bevy is in the workspace.**
Sampled once a second through six `cargo xtask all` runs (the task runner
retired in §3; the same five commands and the two document checks), each preceded by
touching `core`, `editor_ui` and `editor`, which is enough for the two crates
that take the engine to be rebuilt and relinked: physical memory
in use peaked at 16307 MiB of 16309 MiB and stayed there for most of every run,
while commit charge peaked at 32036 MiB against a limit of 43957 MiB. The
concurrency at this size is the dependency graph's rather than `-j`'s: two to
five `rustc.exe`, and up to eight `link.exe` at about 1 GiB resident each. So a
16 GiB machine runs the gate with nothing to spare, and a full run is not
something to start beside work whose unsaved state matters. The number is here
because it is measured rather than estimated, and because §3's accepted risk
said the cost would be revisited once the engine arrived.

---

## 2. Documentation

### Decision

Documentation is built with **mdBook**, as Jackdaw's is, and follows the same
four-part structure.

```text
book/
├── book.toml
├── theme/
└── src/
    ├── SUMMARY.md
    ├── introduction.md
    ├── getting-started/     # installation / first level / ...
    ├── user-guide/          # viewport / tilemap / scenario / play / shortcuts
    ├── developer-guide/     # architecture / crate structure / data model / ...
    └── reference/           # configuration
```

Jackdaw's `developer-guide/architecture.md` and `crate-structure.md` are
particularly well written, and are the template this structure follows.

---

## 3. Continuous integration

### Decision

~~**Every check runs on Linux, macOS and Windows**, on every push to `main`
and every pull request, **each as a job of its own**.~~ **Reversed** by
[#91](https://github.com/itsakeyfut/bevy-2d-editor/issues/91), for what is
below. It read: running everything everywhere, rather than putting the
platform-independent checks on one runner, is the decision, because the
failure worth designing against is a check that examines nothing on one
platform while passing on another, which had happened here and was invisible
from Linux.

**`test` runs on Linux, macOS and Windows; every other check runs on Linux
only.** Each is a job of its own, on every push to `main` and every pull
request, named on the Checks page as the check and the platform,
`test (windows-latest)`, `clippy (ubuntu-latest)`. The checks are `fmt`,
`game`, `clippy`, `doc` and `test`. ~~They ran as `cargo xtask <check>`, and
two tasks beside them, `docs` and `commands`, ran too.~~ **Reversed** by
[#92](https://github.com/itsakeyfut/bevy-2d-editor/issues/92); the next three
paragraphs say what replaced each.

Section 1 claims the product is not Windows-only, and `test` is what makes that
claim checkable: it is the one check that builds the editor and runs it. The
others read the same sources on every platform and found nothing on one that
they did not find on the others: nothing in the workspace is compiled only on
one platform, measured by a search for `cfg(windows)`, `cfg(unix)` and
`target_os` that found two, both in a test's comments. What a check that
examines nothing on one platform looks like is now held where it can happen,
in `test`. Linux rather than Windows for the rest is not a matter of speed:
on `main`'s last run before this, warm, the six took 339 s on Linux and 376 s
on Windows in all, Linux faster at `clippy` (29 s against 86 s) and `game`
(36 s against 59 s), Windows at the other four. It is that a Linux runner is
the cheapest kind GitHub offers, and that the Linux leg of `test` needs the
libraries the engine links against installed anyway.

~~**The workflow names one command and no checks.**~~ **Withdrawn** by
[#70](https://github.com/itsakeyfut/bevy-2d-editor/issues/70). It read: what
is checked lives in `xtask/`, so a check added there runs on all three
platforms without the workflow being touched, and a workflow that enumerated
the rows would undo that. What it cost was a Checks page that named a
platform and not a check: a failed `clippy` and a failed `test` both read as a
red `Gate (windows-latest)`, and which one was a log away.

~~**The workflow lists the checks, and a test holds the list equal to
xtask's.**~~ **Withdrawn** by
[#92](https://github.com/itsakeyfut/bevy-2d-editor/issues/92), because there is
no second list left to hold it equal to. It read: the matrix is a second copy
of xtask's list, made safe by a test that parsed both and failed naming a check
in one and not the other, and the flags of each check stayed in
`xtask/src/gate.rs`.

**Each leg runs a plain `cargo` command written in the workflow, and nothing
else holds the list.** Every leg of the matrix carries its command and its
flags (`--locked`, `-D warnings`, rustdoc's warnings as errors), so what a
check runs is read in one place, the place that runs it. The cost is that the
workflow is now the only definition: there is no task to type locally, and a
reader runs what the workflow says.

**Nothing guards the workflow, the lints or the engine's features by test.**
xtask's tests held which check ran where and with which flags, that every
crate inherits `unsafe_code = "forbid"`, that every crate takes the engine
with the features [ui.md §3](./ui.md) settled and no defaults, and that every
crate a game is built from has a `game` leg. They were first to move to
`crates/editor/tests/`, and the author decided on
[#92](https://github.com/itsakeyfut/bevy-2d-editor/issues/92), while it was
being built, to have none. What that costs, each a change nobody is told
about: a leg that loses `--locked` or `-D warnings` and stays green; a new
crate without `[lints] workspace = true`, where `unsafe` is then allowed; a
feature added to or dropped from the engine's entry, which only changes the
minutes; a crate a game is built from that is never compiled without the
editor's features unified in. Each surfaces, if at all, when somebody reads
the file it lives in.

**There is no local gate script.** "Before saying anything is done" in
`CLAUDE.md` names the workflow's Linux commands to run, rather than a script
that ran the same ones. It had been one script so that "the gate passed" was a
claim about one exit code; that is given up, and the claim becomes the five
commands' exit codes, read off the run. A script outside the repository, as
it was, could not be held equal to the workflow by any test, and a copy that
can drift silently is what this section has refused twice.

**The documents are not checked against each other by a machine.** The
`docs` task resolved cross-file links, section numbers and the decision index,
and checked every record in `docs/adr/` against its row; the `commands` task
checked that a command named in prose exists here. Both went with xtask, by
the author's decision, and neither was moved. What that costs is the class of
defect `docs` was written for: a link or a section number that has gone stale
and still reads as correct, which splitting one document into sixteen
produced. A reader who follows a link and finds nothing is where that now
surfaces, which is row 4 of `CLAUDE.md`'s list.

**The required check is one job over all of them.** `All platforms` fails
unless `needs.check.result` is `success`, which is one result over every leg
of the matrix and a total condition over its four values, so a leg that is
skipped does not pass it.

**`unsafe` is refused by the compiler**, through `unsafe_code = "forbid"` in
the root manifest, rather than by a gate row that scanned the sources as
text. The scan was a check that could examine the wrong set of files; the
lint cannot.

**The Linux leg installs what the engine links against.** Bevy's `2d` feature
pulls `default_platform`, which names x11, wayland and gamepad support, so
`wayland-sys` and `libudev-sys` probe for their libraries through `pkg-config`
and panic when they are absent. An `ubuntu-latest` runner does not carry them.
The list in the workflow is derived from `Cargo.lock` rather than copied from
elsewhere: everything else Linux-facing there opens its library at run time and
needs nothing at build time, and no alsa package appears because §3 of
[ui.md](./ui.md) does not take the audio feature. The alternative, dropping
`wayland` and shipping an x11-only editor, is a decision about what Linux users
get rather than a way to make a build pass, and it is not taken here.

**No third-party action other than `actions/checkout`, for now.** The toolchain
comes from `rust-toolchain.toml` through `rustup show`, so the pin in the
repository is the only place a version is written. There is no build cache:
with two external dependencies there is almost nothing to cache, and a pinned
action to keep current costs more than the seconds it saves.

> **`Swatinem/rust-cache` goes in in the same change that adds Bevy to the
> workspace.** That is the point at which a cold build stops being seconds, and
> it is a trigger rather than a plan so that it does not become a thing nobody
> remembers ([open-questions.md §1](./open-questions.md) on why a deferral
> carries one).

**Caches per platform and kind of build, not one per check.** `game`,
`clippy` and `doc` compile dependencies in check mode and share one, saved by
`clippy`; `test` builds and keeps its own; `fmt` builds nothing and takes
none (before [#92](https://github.com/itsakeyfut/bevy-2d-editor/issues/92),
`docs` and `commands` were beside it, building only xtask). Since
[#91](https://github.com/itsakeyfut/bevy-2d-editor/issues/91) the check-mode
cache exists on Linux only, so four caches rather than six; the sizes below
are from before it. Before this split the three platform caches measured
1.40, 1.67 and 1.84 GiB against the repository's default limit of 10 GiB, and
a cache per check was estimated at about 11 GiB, which evicts at random into
cold builds. After it the six caches measured 0.29 to 0.39 GiB for check mode
and 0.74 to 0.93 GiB for the test build, 3.6 GiB in all.

### Rejected

* ~~**`fmt`, `clippy` and `doc` on one runner**, with only `test` on three.
  Saves little today and leaves `cfg(windows)` code unlinted once there is
  any.~~ **Reversed** by
  [#91](https://github.com/itsakeyfut/bevy-2d-editor/issues/91). There is no
  such code, and when there is, [open-questions.md](./open-questions.md) says
  `clippy` goes back to three platforms.
* **The other checks on Windows, the development machine, rather than
  Linux.** About as fast in all, measured above, on a runner that costs more,
  and the development machine already runs every check locally through the
  gate.
* **The other two platforms only on pushes to `main`.** A Windows-only failure
  would then be found after the merge, which is not what a pull request check
  is for.
* ~~**A job per check**, crossed with the platforms. It reads best of all on the
  Checks page and costs a second copy of the list of checks, in YAML, beside
  the one in `xtask/`.~~ **Reversed** by
  [#70](https://github.com/itsakeyfut/bevy-2d-editor/issues/70). The second
  copy is kept and held equal to the first by a test that parses both, which
  is what made the objection answerable: a copy that cannot drift silently is
  not the cost the objection priced.
* **Each check as a cargo alias** in `.cargo/config.toml`, run as
  `cargo <check>`, with xtask keeping only what cargo cannot express. An alias
  cannot take a built-in's name (cargo ignores it with a warning and runs the
  built-in without the flags), so the checks would have been `tests`,
  `fmt-check` and `docs-api` beside `test`, `fmt` and `doc`: `cargo test`
  passing locally while the job named `tests` fails on `--locked` is a trap,
  and three names that differ by a suffix cannot be read. It also split the
  entry point in two. One prefix, `cargo xtask`, which prints the list when
  typed alone, is the thing nobody has to be told. Still rejected after
  [#92](https://github.com/itsakeyfut/bevy-2d-editor/issues/92): plain
  commands in the workflow have none of the naming trouble.
* **The guard tests moved to `crates/editor/tests/`**, as first designed on
  [#92](https://github.com/itsakeyfut/bevy-2d-editor/issues/92), and written
  before the author withdrew it. Withdrawn because the author wanted no test
  of the workflow and the manifests.
* **Keeping `docs`, `commands` and the guard tests in a test-only package**
  outside `crates/`, run by `cargo test`, which would have kept every check
  and dropped only the task runner. Not taken by the author's decision on
  [#92](https://github.com/itsakeyfut/bevy-2d-editor/issues/92).
* **Rewriting `docs` and `commands` as Python scripts** beside `knowledge.sh`.
  About 2,300 lines rewritten and the 60 tests that hold them lost, and
  `.claude/` is not in the repository, so CI could not run them.
* **A local gate script that reads the workflow's commands and runs them**, or
  one that holds a copy. The author chose to have none.
* **The matrix generated by a job that runs xtask** and hands the list over
  through `fromJSON`. No second copy at all, at the cost of a serial job before
  every other and a Checks page with no check names until it finishes.
* **A cache per check.** Simplest, and over the repository's limit; see above.

### Accepted risk

Three full builds per push, which is cheap now and will not be once Bevy is a
dependency. That cost is revisited with the cache, on the same trigger.

Twenty-one check jobs per push rather than three. Every job pays its own checkout,
toolchain and cache restore, and the engine is compiled in check mode and in
build mode separately on each platform, so runner minutes go up. Measured on
warm caches, per push:

| | wall time | job time, summed |
|---|---|---|
| one job per platform | 7m10s to 10m18s, three pushes | 14m17s, on the 8m06s push |
| one job per check and platform | 6m52s | 25m24s |

Wall time did not go up, because the Windows `test` job, 6m43s, is now the
whole critical path where it used to follow the other rows. Job time nearly
doubled. The first run on empty caches took 15m31s of wall time and 50m13s of
job time. The repository is public, so the minutes are not billed; what is
spent is the concurrency, five macOS jobs at a time on the free plan, so two
of the seven macOS jobs wait for a runner.
