//! The only crate a user's game depends on.
//!
//! Everything public here is a promise to every project built with this
//! editor, and taking one back means migrating all of them. Adding a
//! dependency here adds it to those projects too, which is why physics and
//! Aseprite arrive behind default-on features rather than directly. See
//! `docs/specs/crates.md` §3.
//!
//! Depends on `b2d_data` and `b2d_core`, and later on Bevy.
