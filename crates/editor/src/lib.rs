//! The editor: panels, tools, and the plugin group that composes them.
//!
//! This is the only place the generic widgets in `b2d_editor_ui` meet the
//! document types in `b2d_data`, which is what keeps the widgets from knowing
//! the model. See `docs/specs/crates.md` §3.
//!
//! The composition seam `docs/specs/architecture.md` §1 describes belongs here
//! rather than in the binary, so that a test can build the editor without
//! going through `main`.

use bevy::app::PluginGroup;
use bevy::prelude::*;

/// The editor, built and not running.
///
/// The platform plugins are handed in rather than chosen here. `DefaultPlugins`
/// builds a winit event loop, and winit refuses to build one off the main
/// thread:
///
/// ```text
/// Initializing the event loop outside of the main thread is a significant
/// cross-platform compatibility hazard.
/// ```
///
/// `cargo test` runs its tests on worker threads, so a seam that chose
/// `DefaultPlugins` itself would be a seam no test could call, and the
/// composition would go back into `main` where nothing can reach it. `main`
/// hands in `DefaultPlugins`; a test hands in `MinimalPlugins`.
///
/// It composes nothing of its own yet. The editor's own plugins hang off this
/// same seam and arrive with the plugin group.
///
/// Mutation: add `DefaultPlugins` here instead of taking it, and
/// `the_editor_is_built_off_the_main_thread` panics naming winit's event loop.
pub fn editor(platform: impl PluginGroup) -> App {
    let mut app = App::new();
    app.add_plugins(platform);
    app
}

#[cfg(test)]
mod tests {
    use super::editor;
    use bevy::prelude::*;

    /// The editor is built on a thread that is not the main one.
    ///
    /// This is the whole reason the platform is a parameter. The panic it
    /// prevents is winit's, and it is a panic rather than a failure, so
    /// without this test the next person to move `DefaultPlugins` into
    /// `editor` learns about it from a stack trace in an unrelated run.
    ///
    /// Mutation: write `app.add_plugins(DefaultPlugins)` in `editor`, and this
    /// panics.
    #[test]
    fn the_editor_is_built_off_the_main_thread() {
        let app = editor(MinimalPlugins);
        assert!(
            app.is_plugin_added::<bevy::app::TaskPoolPlugin>(),
            "the app did not come back with the platform it was handed"
        );
    }

    /// The platform handed in is the one the app carries.
    ///
    /// Asserted against a plugin that only `MinimalPlugins` brings, so an
    /// `editor` that ignored its argument and added nothing fails here rather
    /// than returning an empty app that looks fine.
    ///
    /// Mutation: drop the `add_plugins` call in `editor`, and this fails.
    #[test]
    fn the_platform_handed_in_is_the_one_the_app_carries() {
        let empty = App::new();
        assert!(
            !empty.is_plugin_added::<bevy::app::TaskPoolPlugin>(),
            "an app with no platform already carries the plugin this asserts"
        );
        let app = editor(MinimalPlugins);
        assert!(app.is_plugin_added::<bevy::app::TaskPoolPlugin>());
    }
}
