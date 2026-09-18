//! The editor: panels, tools, and the plugin group that composes them.
//!
//! This is the only place the generic widgets in `b2d_editor_ui` meet the
//! document types in `b2d_data`, which is what keeps the widgets from knowing
//! the model. See `docs/specs/crates.md` §3.
//!
//! The editor is composed here rather than in the binary, so that a test can
//! build it without going through `main`. `docs/specs/architecture.md` §3
//! settles what the composition is, a compile-time `PluginGroup`; that it
//! lives in the library is this crate's own decision and the reason is below,
//! on `editor`.

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
/// hands in `DefaultPlugins`; a test hands in something it can build.
///
/// Bevy has a switch for this and it does not reach far enough:
/// `WinitPlugin::run_on_any_thread` says of itself that it "only works on
/// Linux (X11/Wayland) and Windows" and "is ignored on other platforms", and
/// `.github/workflows/ci.yml` runs `macos-latest`. The parameter is what
/// covers the platform the switch does not.
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
    use bevy::app::PluginGroupBuilder;
    use bevy::prelude::*;

    /// A plugin no platform group carries, so that a test can tell one group
    /// from another.
    struct Marker;

    impl Plugin for Marker {
        fn build(&self, _app: &mut App) {}
    }

    /// A platform group that is not `MinimalPlugins` and is not
    /// `DefaultPlugins`.
    fn marked() -> PluginGroupBuilder {
        MinimalPlugins.build().add(Marker)
    }

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

    /// The platform handed in is the one the app carries, and not one the seam
    /// chose for itself.
    ///
    /// Asserted through a plugin that no platform group carries, because every
    /// plugin that `MinimalPlugins` brings is in `DefaultPlugins` too:
    /// `TaskPoolPlugin` is listed in both. A test that asked only about those
    /// could not tell "the group the caller passed" from "a group the function
    /// built regardless", and an `editor` that ignored its argument would pass
    /// it while the real editor came up with no renderer and no window.
    ///
    /// Mutation: ignore the argument and add a fixed group in `editor`, and
    /// this fails.
    #[test]
    fn the_platform_handed_in_is_the_one_the_app_carries() {
        assert!(
            !editor(MinimalPlugins).is_plugin_added::<Marker>(),
            "the marker is in a platform group, so it cannot tell them apart"
        );
        assert!(
            editor(marked()).is_plugin_added::<Marker>(),
            "the app came back without the group it was handed"
        );
    }
}
