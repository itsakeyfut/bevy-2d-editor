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
