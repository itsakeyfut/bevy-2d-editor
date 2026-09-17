//! Generic editor widgets: split panels, tree views, inspector field rows,
//! docking.
//!
//! **This crate knows nothing about levels or scenarios.** That is deliberate:
//! a widget that knew the model is how the UI becomes the source of truth,
//! which `docs/specs/architecture.md` §6 forbids. It depends on neither
//! `b2d_data` nor `b2d_runtime`, and `docs/specs/crates.md` §3 says why.
//!
//! Depends on Bevy and `bevy_feathers`, once the UI arrives.
