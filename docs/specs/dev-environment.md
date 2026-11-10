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
Sampled once a second through six `cargo xtask all` runs, each preceded by
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

**Every check runs on Linux, macOS and Windows**, on every push to `main`
and every pull request, **each as a job of its own**: `cargo xtask <check>`,
named on the Checks page as the check and the platform, `clippy
(windows-latest)`. The checks are the gate's rows (`fmt`, `game`, `clippy`,
`doc`, `test`) and the two tasks beside it (`docs`, `commands`).

Section 1 claims the product is not Windows-only. This is what makes that claim
checkable rather than said. Running everything everywhere, rather than putting
the platform-independent checks on one runner, is the decision: the failure
worth designing against is a check that examines nothing on one platform while
passing on another, which has already happened here and was invisible from
Linux.

~~**The workflow names one command and no checks.**~~ **Withdrawn** by
[#70](https://github.com/itsakeyfut/bevy-2d-editor/issues/70). It read: what
is checked lives in `xtask/`, so a check added there runs on all three
platforms without the workflow being touched, and a workflow that enumerated
the rows would undo that. What it cost was a Checks page that named a
platform and not a check: a failed `clippy` and a failed `test` both read as a
red `Gate (windows-latest)`, and which one was a log away.

**The workflow lists the checks, and a test holds the list equal to xtask's.**
The matrix in `.github/workflows/ci.yml` is a second copy of the list, which
is what the withdrawn paragraph above was avoiding. What makes the copy safe
is `the_workflow_runs_every_check_xtask_lists`, which reads the workflow
through a YAML parser and xtask's list from its task tables, and fails naming
a check that is in one and not the other. A parser rather than a text scrape
because a scrape here has already been defeated by comments, three ways,
each reporting `ok` while the two lists disagreed. The flags of each check stay
in `xtask/src/gate.rs`, held by the tests beside them.

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

**Two caches per platform, not one per check.** `game`, `clippy` and `doc`
compile dependencies in check mode and share one, saved by `clippy`; `test`
builds and keeps its own; `fmt`, `docs` and `commands` build only xtask and
take none. Before this split the three platform caches measured 1.40, 1.67
and 1.84 GiB against the repository's default limit of 10 GiB, and a cache per
check was estimated at about 11 GiB, which evicts at random into cold builds.

### Rejected

* **`fmt`, `clippy` and `doc` on one runner**, with only `test` on three. Saves
  little today and leaves `cfg(windows)` code unlinted once there is any.
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
  typed alone, is the thing nobody has to be told.
* **The matrix generated by a job that runs xtask** and hands the list over
  through `fromJSON`. No second copy at all, at the cost of a serial job before
  every other and a Checks page with no check names until it finishes.
* **A cache per check.** Simplest, and over the repository's limit; see above.

### Accepted risk

Three full builds per push, which is cheap now and will not be once Bevy is a
dependency. That cost is revisited with the cache, on the same trigger.

Twenty-one check jobs per push rather than three. Every job pays its own checkout,
toolchain and cache restore, and the engine is compiled in check mode and in
build mode separately on each platform, so runner minutes go up. Before the
split, three pushes measured 8m06s, 10m18s and 7m10s of wall time, with the
Windows leg alone 6 to 10 minutes of it. The repository is public, so the
minutes are not billed; what is spent is the concurrency, five macOS jobs at a
time on the free plan, so two of the seven macOS jobs wait for a runner.
