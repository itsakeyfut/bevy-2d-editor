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

use bevy::app::{PluginGroup, PluginGroupBuilder};
use bevy::prelude::*;

/// A member of the editor's plugin group: the name a test names it by, and the
/// one line that adds it.
type Member = (&'static str, fn(PluginGroupBuilder) -> PluginGroupBuilder);

/// Write a row of [`MEMBERS`].
///
/// Both columns come out of one token, so the name cannot drift from the plugin
/// it names. `core::any::type_name` would say it better and is not a `const fn`,
/// which a row of a `const` table has to be.
///
/// The expectation retires itself: the table is empty today, and the first row
/// added to it makes this attribute unfulfilled and asks to be deleted.
#[cfg_attr(
    not(test),
    expect(
        unused_macros,
        reason = "the table is empty until the first editor plugin arrives"
    )
)]
macro_rules! member {
    ($plugin:expr) => {
        (stringify!($plugin), |group: PluginGroupBuilder| {
            group.add($plugin)
        })
    };
}

/// What a member leaves behind, so that a test can tell a plugin that is absent
/// from one that is present and does nothing.
#[cfg(test)]
#[derive(Resource)]
struct ProbeMark;

/// A plugin the editor does not have, standing in for one it will.
///
/// It lives here rather than in the tests so that the mutations named below are
/// ones somebody can actually apply: both `MEMBERS` and `compose` are out of
/// reach of a type declared inside `mod tests`, and a doc comment naming a
/// mutation nobody can carry out is worse than one naming none.
#[cfg(test)]
struct Probe;

#[cfg(test)]
impl Plugin for Probe {
    fn build(&self, app: &mut App) {
        app.insert_resource(ProbeMark);
    }
}

/// Every plugin the editor is composed of, in the order they are built.
///
/// Written out as a table because [`EditorPlugins`] builds the table rather
/// than naming its members a second time, and because a `PluginGroupBuilder`
/// cannot be enumerated: it answers `contains::<T>()` one named type at a time,
/// so a group written as a list of `add` calls is a group no test can read as a
/// set. `docs/adr/0001-compose-the-editor-from-a-table-a-test-can-read.md` has
/// what that costs and what was turned down for it.
///
/// It is empty. The editor's first plugin arrives with the panel layout.
///
/// Mutation: give it the row `member!(Probe)`, and
/// `the_group_carries_the_members_the_table_names` fails.
pub(crate) const MEMBERS: [Member; 0] = [];

/// Fold a table of members into the group they compose.
///
/// Takes the table rather than reading [`MEMBERS`] itself, so that a test can
/// hand it a row and watch that row reach the built app. While `MEMBERS` is
/// empty that is the only way any of this is exercised at all.
fn compose(members: &[Member]) -> PluginGroupBuilder {
    members.iter().fold(
        PluginGroupBuilder::start::<EditorPlugins>(),
        |group, member| member.1(group),
    )
}

/// The editor's own plugins.
///
/// `docs/specs/architecture.md` §3 settles that editor features are added as
/// plugins and composed as a compile-time group. This is that group; what is in
/// it is `MEMBERS`, the table below it.
pub struct EditorPlugins;

impl PluginGroup for EditorPlugins {
    fn build(self) -> PluginGroupBuilder {
        compose(&MEMBERS)
    }
}

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
/// What the editor is composed of is [`EditorPlugins`], added here rather than
/// in `main` for the same reason: `main` is not somewhere a test can reach.
/// While that group is empty, nothing observable distinguishes an editor that
/// adds it from one that does not, because Bevy records a group's plugins and
/// not the group. The first member makes that line assertable and is where the
/// test for it belongs.
///
/// Mutation: add `DefaultPlugins` here instead of taking it, and
/// `the_editor_is_built_off_the_main_thread` panics naming winit's event loop.
pub fn editor(platform: impl PluginGroup) -> App {
    let mut app = App::new();
    app.add_plugins(platform);
    app.add_plugins(EditorPlugins);
    app
}

#[cfg(test)]
mod tests {
    use super::{EditorPlugins, MEMBERS, Member, Probe, ProbeMark, compose, editor};
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

    /// The group carries the members the table names.
    ///
    /// `MEMBERS` is what `EditorPlugins` is built from, so a member that
    /// quietly leaves it leaves the editor without saying so, and a member that
    /// quietly joins it arrives with nothing asserting anything about it. That
    /// is the shape RK-001 is about, and asserting the table against a literal
    /// written here is what stops it being a table that agrees with itself.
    ///
    /// It is empty today. The assertion is still the real claim: the editor is
    /// composed of nothing yet.
    ///
    /// Mutation: give `MEMBERS` the row `member!(Probe)`, and this fails.
    #[test]
    fn the_group_carries_the_members_the_table_names() {
        let names: Vec<&str> = MEMBERS.iter().map(|member| member.0).collect();
        assert_eq!(names, Vec::<&str>::new());
    }

    /// A member is a row and nothing else.
    ///
    /// This is the whole of "adding a plugin is one line in the group": a row
    /// handed to `compose` reaches the built app, with nothing written anywhere
    /// else. It is also what exercises the table's columns while the table has
    /// no rows, which RK-001 says is otherwise where a written table quietly
    /// means nothing.
    ///
    /// Mutation: fold `&MEMBERS` in `compose` instead of the argument, and this
    /// fails.
    #[test]
    fn a_member_is_a_row_and_nothing_else() {
        let row: Member = member!(Probe);
        let mut app = App::new();
        app.add_plugins(compose(&[row]));
        assert!(
            app.is_plugin_added::<Probe>(),
            "the row's plugin did not reach the app"
        );
        assert!(
            app.world().contains_resource::<ProbeMark>(),
            "the row's plugin reached the app without being built"
        );
    }

    /// A member left out of the group leaves nothing behind.
    ///
    /// The pair this makes with `a_member_is_a_row_and_nothing_else` is the
    /// point. Either alone is satisfied by a group that does nothing at all:
    /// together they say that a row's plugin is built, and that a plugin which
    /// is not a row is absent rather than present and inactive, which is the
    /// distinction `docs/specs/architecture.md` §3 decided.
    ///
    /// Mutation: start `compose` from `...start::<EditorPlugins>().add(Probe)`,
    /// so that the plugin is in the group without being a row, and this fails.
    #[test]
    fn a_member_left_out_of_the_group_leaves_nothing_behind() {
        let mut app = App::new();
        app.add_plugins(EditorPlugins);
        assert!(
            !app.is_plugin_added::<Probe>(),
            "a plugin the table does not name is in the app"
        );
        assert!(
            !app.world().contains_resource::<ProbeMark>(),
            "a plugin the table does not name left its registration behind"
        );
    }
}
