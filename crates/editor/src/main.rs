//! The editor's entry point.
//!
//! This stays thin on purpose. What the editor is composed of belongs in the
//! library beside it, where a test can reach it; see `b2d_editor`'s module
//! documentation.

use bevy::prelude::DefaultPlugins;

fn main() {
    b2d_editor::editor(DefaultPlugins).run();
}
