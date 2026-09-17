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

**The whole gate runs on Linux, macOS and Windows**, on every push to `main`
and every pull request, as one command: `cargo xtask all`.

Section 1 claims the product is not Windows-only. This is what makes that claim
checkable rather than said. Running everything everywhere, rather than putting
the platform-independent checks on one runner, is the decision: the failure
worth designing against is a check that examines nothing on one platform while
passing on another, which has already happened here and was invisible from
Linux.

**The workflow names one command and no checks.** What is checked lives in
`xtask/`, so a check added there runs on all three platforms without the
workflow being touched. Putting the gate in the repository was for exactly
that, and a workflow that enumerated the rows would undo it.

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

### Rejected

* **`fmt`, `clippy` and `doc` on one runner**, with only `test` on three. Saves
  little today and leaves `cfg(windows)` code unlinted once there is any.
* **The other two platforms only on pushes to `main`.** A Windows-only failure
  would then be found after the merge, which is not what a pull request check
  is for.
* **A job per check**, crossed with the platforms. It reads best of all on the
  Checks page and costs a second copy of the list of checks, in YAML, beside
  the one in `xtask/`.

### Accepted risk

Three full builds per push, which is cheap now and will not be once Bevy is a
dependency. That cost is revisited with the cache, on the same trigger.
