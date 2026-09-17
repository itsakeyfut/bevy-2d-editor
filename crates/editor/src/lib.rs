//! The editor: panels, tools, and the plugin group that composes them.
//!
//! This is the only place the generic widgets in `b2d_editor_ui` meet the
//! document types in `b2d_data`, which is what keeps the widgets from knowing
//! the model. See `docs/specs/crates.md` §3.
//!
//! The composition seam `docs/specs/architecture.md` §1 describes belongs here
//! rather than in the binary, so that a test can build the editor without
//! going through `main`. Nothing is composed yet: the application arrives with
//! the `Editor Core` milestone.
