//! The types a level and a scenario are made of, and how they are written to
//! disk.
//!
//! A user's game reaches this crate through `b2d_runtime`, so what is public
//! here is public to every game built with this editor. The editor-only half
//! sits behind a feature for that reason; see `docs/specs/crates.md` §3.
//!
//! Depends on `b2d_core`, and later on Bevy.
