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
use bevy::feathers::FeathersPlugins;
use bevy::feathers::containers::pane;
use bevy::feathers::theme::{ThemeBackgroundColor, ThemeBorderColor};
use bevy::feathers::tokens;
use bevy::prelude::*;
use bevy::scene::bsn;
use bevy::ui::{UiRect, percent, px};

/// A member of the editor's plugin group: the name a test names it by, and the
/// one line that adds it.
type Member = (&'static str, fn(PluginGroupBuilder) -> PluginGroupBuilder);

/// Write a row of [`MEMBERS`].
///
/// Both columns come out of one token, so the name cannot drift from the plugin
/// it names. `core::any::type_name` would say it better and is not a `const fn`,
/// which a row of a `const` table has to be.
///
/// A row is a plugin **type**. `$plugin` is an `expr`, so `member!(P { x: 1 })`
/// would compile and put the whole expression in the name column, which is not
/// what `a_rows_name_is_the_plugin_it_adds` reads that column as. A plugin that
/// needs configuring is a reason to widen this deliberately, not to write one
/// through by accident.
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

/// The members that were built, in the order they were built in.
#[cfg(test)]
#[derive(Resource, Default)]
struct BuiltInOrder(Vec<&'static str>);

/// Say that a member was built, so that a test can read the order back.
#[cfg(test)]
fn record(app: &mut App, member: &'static str) {
    app.init_resource::<BuiltInOrder>();
    app.world_mut()
        .resource_mut::<BuiltInOrder>()
        .0
        .push(member);
}

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
        record(app, "Probe");
    }
}

/// A second one, because order is not a property one member has.
#[cfg(test)]
struct SecondProbe;

#[cfg(test)]
impl Plugin for SecondProbe {
    fn build(&self, app: &mut App) {
        record(app, "SecondProbe");
    }
}

/// Which of `docs/specs/ui.md` §1's regions an entity is.
///
/// One component carrying a name rather than five marker types: the regions
/// are a list in a drawing, and a test that asks for all of them wants to
/// compare a list rather than write five separate queries.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    /// Across the top.
    MenuBar,
    /// Down the left.
    AssetBrowser,
    /// The middle, where the viewport arrives with its own issue.
    Viewport,
    /// Down the right.
    Inspector,
    /// Along the bottom.
    Status,
}

/// The five regions, drawn with this project's colours.
///
/// Feathers supplies the pane and this project supplies the appearance, which
/// is the split `docs/specs/ui.md` §3 decided. The arrangement is fixed:
/// docking is deferred with a trigger in `docs/specs/open-questions.md` §1,
/// because the engine has none and what it should do is undecided.
///
/// Mutation: drop `ThemePlugin` from what this adds, and
/// `the_panels_carry_this_projects_theme` fails.
pub struct PanelsPlugin;

impl Plugin for PanelsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeathersPlugins)
            .add_plugins(b2d_editor_ui::ThemePlugin)
            .add_systems(Startup, spawn_regions);
    }
}

/// Put the regions on screen, arranged the way `docs/specs/ui.md` §1 draws
/// them.
///
/// A column of three: the menu bar, a row of three panes, and the status bar.
/// Each region is a Feathers `pane`, which is where the widget structure comes
/// from, with this project's colours on it, which is the split §3 decided.
///
/// The regions are given three different tokens on purpose. The first version
/// gave all five `PANE_BODY_BG`, and the window came up as one flat rectangle:
/// every pixel measured `(61, 61, 61)`, the layout was drawing correctly and
/// nothing in it could be told apart. §1 draws lines between the regions, so
/// the bars recede, the viewport is the window's own colour, and every edge §1
/// draws is a border.
///
/// The regions are empty. What goes in each is its own issue, and an empty
/// pane is what lets the arrangement land before there is anything to arrange.
///
/// `Region` is inserted after the scene rather than written inside it. `bsn!`
/// requires a component to implement `Default`, and an enum of five named
/// places has no default that means anything; inventing one to satisfy a macro
/// would put a wrong answer in the type rather than in the call.
///
/// Mutation: spawn all but the last of these, and
/// `every_region_declared_is_on_screen` fails.
fn spawn_regions(mut commands: Commands) {
    // Bevy UI is drawn through `ComputedUiTargetCamera`, so a node with no
    // camera to target is a node nothing draws: the window comes up and stays
    // empty, which is what this looked like before the camera was here.
    //
    // Whether the viewport's own camera is this one or a second is #23's to
    // decide; what this issue needs is that the panels can be seen at all.
    commands.spawn(Camera2d);

    let root = commands
        .spawn_scene(bsn! {
            Node {
                width: percent(100),
                height: percent(100),
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
            }
            ThemeBackgroundColor(tokens::WINDOW_BG)
        })
        .id();

    commands
        .spawn_scene(bsn! {
            pane()
            ThemeBackgroundColor(tokens::PANE_HEADER_BG)
            ThemeBorderColor(tokens::PANE_HEADER_BORDER)
            Node { height: px(28), border: UiRect::bottom(px(1)) }
        })
        .insert((Region::MenuBar, ChildOf(root)));

    let middle = commands
        .spawn_scene(bsn! {
            Node {
                flex_grow: 1.0,
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
            }
        })
        .insert(ChildOf(root))
        .id();

    commands
        .spawn_scene(bsn! {
            pane()
            ThemeBackgroundColor(tokens::PANE_BODY_BG)
            ThemeBorderColor(tokens::PANE_HEADER_BORDER)
            Node { width: px(240), border: UiRect::right(px(1)) }
        })
        .insert((Region::AssetBrowser, ChildOf(middle)));

    commands
        .spawn_scene(bsn! {
            pane()
            ThemeBackgroundColor(tokens::WINDOW_BG)
            Node { flex_grow: 1.0 }
        })
        .insert((Region::Viewport, ChildOf(middle)));

    commands
        .spawn_scene(bsn! {
            pane()
            ThemeBackgroundColor(tokens::PANE_BODY_BG)
            ThemeBorderColor(tokens::PANE_HEADER_BORDER)
            Node { width: px(300), border: UiRect::left(px(1)) }
        })
        .insert((Region::Inspector, ChildOf(middle)));

    commands
        .spawn_scene(bsn! {
            pane()
            ThemeBackgroundColor(tokens::PANE_HEADER_BG)
            ThemeBorderColor(tokens::PANE_HEADER_BORDER)
            Node { height: px(22), border: UiRect::top(px(1)) }
        })
        .insert((Region::Status, ChildOf(root)));
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
/// Mutation: remove the row, and
/// `the_group_carries_the_members_the_table_names` fails.
pub(crate) const MEMBERS: [Member; 1] = [member!(PanelsPlugin)];

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
    use super::{
        BuiltInOrder, MEMBERS, Member, Probe, ProbeMark, Region, SecondProbe, compose, editor,
    };
    use bevy::app::PluginGroupBuilder;
    use bevy::feathers::theme::UiTheme;
    use bevy::math::Vec2;
    use bevy::prelude::*;
    use bevy::render::RenderPlugin;
    use bevy::render::settings::{RenderCreation, WgpuSettings};
    use bevy::sprite::BorderRect;
    use bevy::ui::ComputedNode;
    use bevy::winit::WinitPlugin;

    /// A plugin no platform group carries, so that a test can tell one group
    /// from another.
    struct Marker;

    impl Plugin for Marker {
        fn build(&self, _app: &mut App) {}
    }

    /// A platform group that is not `headless` and is not `DefaultPlugins`.
    fn marked() -> PluginGroupBuilder {
        headless().add(Marker)
    }

    /// The platform a test hands in.
    ///
    /// `MinimalPlugins` stopped being enough the moment the group had a
    /// member: `FeathersCorePlugin::build` calls `embedded_asset!` eight times,
    /// which needs `AssetPlugin`'s resources and panics without them. So the
    /// platform is the real one with the two parts a test cannot have.
    ///
    /// `WinitPlugin` goes because winit refuses to build an event loop off the
    /// main thread, which is what `editor` takes a parameter for at all.
    ///
    /// The GPU goes because a runner has none. Measured on this machine: with
    /// an adapter the suite took 3.24s and named an RTX 3070 Ti, and with
    /// `backends: None` it took 0.72s and named nothing. A test that passes
    /// here and fails on a runner is the failure
    /// `docs/specs/dev-environment.md` §3 put three platforms in the matrix to
    /// catch.
    ///
    /// It logs one `ERROR` and one `WARN` about the render app being absent.
    /// Both are this setting working, not a failure.
    fn headless() -> PluginGroupBuilder {
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(RenderPlugin {
                render_creation: RenderCreation::Automatic(Box::new(WgpuSettings {
                    backends: None,
                    ..default()
                })),
                ..default()
            })
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
        let app = editor(headless());
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
            !editor(headless()).is_plugin_added::<Marker>(),
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
        assert_eq!(names, ["PanelsPlugin"]);
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
        // `editor` adds the group, and the group needs a platform now that a
        // member of it does: `PanelsPlugin` adds Feathers, which reaches for
        // `AssetPlugin`.
        let app = editor(headless());
        assert!(
            !app.is_plugin_added::<Probe>(),
            "a plugin the table does not name is in the app"
        );
        assert!(
            !app.world().contains_resource::<ProbeMark>(),
            "a plugin the table does not name left its registration behind"
        );
    }

    /// The five regions the drawing names are on screen.
    ///
    /// Spelled out here rather than read from a constant the spawning also
    /// reads: a test that compares a table with itself passes whatever the
    /// table says, which is RK-001 and which an earlier version of this test
    /// had.
    ///
    /// The app is run for one update, because the regions are spawned by a
    /// `Startup` system and a built app has not run one yet.
    ///
    /// Mutation: stop spawning any one of the five, and this fails naming it.
    #[test]
    fn the_five_regions_the_drawing_names_are_on_screen() {
        let mut app = editor(headless());
        app.update();
        let found: Vec<Region> = app
            .world_mut()
            .query::<&Region>()
            .iter(app.world())
            .copied()
            .collect();
        for region in [
            Region::MenuBar,
            Region::AssetBrowser,
            Region::Viewport,
            Region::Inspector,
            Region::Status,
        ] {
            assert!(found.contains(&region), "{region:?} is not on screen");
        }
        assert_eq!(found.len(), 5, "got {found:?}");
    }

    /// The three middle regions share a row, and the bars do not.
    ///
    /// This is the arrangement `docs/specs/ui.md` §1 draws, and it is the part
    /// a list of five cannot say: the same five regions stacked in a column
    /// would satisfy the test above. Asserted through the parent each one
    /// hangs from rather than through positions, which are not decided until a
    /// layout pass has run.
    ///
    /// Mutation: give the asset browser the root as its parent, so the three
    /// no longer share a row, and this fails.
    #[test]
    fn the_three_middle_regions_share_a_row_and_the_bars_do_not() {
        let mut app = editor(headless());
        app.update();
        let parents: Vec<(Region, Entity)> = app
            .world_mut()
            .query::<(&Region, &ChildOf)>()
            .iter(app.world())
            .map(|(region, parent)| (*region, parent.parent()))
            .collect();
        let of = |wanted: Region| {
            parents
                .iter()
                .find(|(region, _)| *region == wanted)
                .map(|(_, parent)| *parent)
                .expect("every region has a parent")
        };
        let row = of(Region::Viewport);
        assert_eq!(
            of(Region::AssetBrowser),
            row,
            "the asset browser is not in the row"
        );
        assert_eq!(
            of(Region::Inspector),
            row,
            "the inspector is not in the row"
        );
        assert_ne!(of(Region::MenuBar), row, "the menu bar is in the row");
        assert_eq!(
            of(Region::Status),
            of(Region::MenuBar),
            "the bars do not share the column"
        );
    }

    /// What each region measures, after Bevy has laid it out.
    ///
    /// Returns the size and the border widths `ComputedNode` carries once
    /// `ui_layout_system` has run, keyed by region, at the default window size
    /// of 1280 by 720.
    fn laid_out() -> Vec<(Region, Vec2, BorderRect)> {
        let mut app = editor(headless());
        app.update();
        app.world_mut()
            .query::<(&Region, &ComputedNode)>()
            .iter(app.world())
            .map(|(region, node)| (*region, node.size, node.border))
            .collect()
    }

    /// The regions measure what `docs/specs/ui.md` §1 draws.
    ///
    /// The sizes and the edges are the layout. They were confirmed once by
    /// measuring a screenshot of the running editor, which is a thing a person
    /// did; this is the same claim made by a machine, so that changing 240 to
    /// something else is caught rather than noticed later.
    ///
    /// The middle band spreading rather than stacking is part of it: the test
    /// that says the three share a parent says nothing about which way that
    /// parent lays them out, and an earlier version of this change had exactly
    /// that gap.
    ///
    /// Mutation: change a width, drop a border, or make the middle band a
    /// `Column`, and this fails.
    #[test]
    fn the_regions_measure_what_the_drawing_draws() {
        let laid = laid_out();
        let of = |wanted: Region| {
            laid.iter()
                .find(|(region, _, _)| *region == wanted)
                .map(|(_, size, border)| (*size, *border))
                .expect("every region is laid out")
        };
        let (menu, menu_border) = of(Region::MenuBar);
        assert_eq!(menu.x, 1280.0, "the menu bar does not span the window");
        assert_eq!(menu.y, 28.0, "the menu bar is not 28 high");
        assert_eq!(
            menu_border.max_inset.y, 1.0,
            "the menu bar has no edge below it"
        );

        let (assets, assets_border) = of(Region::AssetBrowser);
        assert_eq!(assets.x, 240.0, "the asset browser is not 240 wide");
        assert_eq!(
            assets_border.max_inset.x, 1.0,
            "the asset browser has no edge"
        );

        let (inspector, inspector_border) = of(Region::Inspector);
        assert_eq!(inspector.x, 300.0, "the inspector is not 300 wide");
        assert_eq!(
            inspector_border.min_inset.x, 1.0,
            "the inspector has no edge"
        );

        let (status, status_border) = of(Region::Status);
        assert_eq!(status.y, 22.0, "the status bar is not 22 high");
        assert_eq!(
            status_border.min_inset.y, 1.0,
            "the status bar has no edge above it"
        );

        // The middle band spreads: the three sit side by side and fill the
        // width between them. Stacked, each would be the full width.
        let viewport = of(Region::Viewport).0;
        assert_eq!(
            assets.x + viewport.x + inspector.x,
            1280.0,
            "the three middle regions do not share the width"
        );
        assert_eq!(
            viewport.y, assets.y,
            "the three middle regions are not the same height"
        );
    }

    /// The root is declared to fill whatever it is drawn to.
    ///
    /// This is as far as a test here reaches into the issue's "resizing the
    /// window does not lose one". Setting `Window::resolution` in a headless
    /// app does not move the layout: what `ui_layout_system` measures against
    /// is the camera's render target, and without winit nothing updates that
    /// from the window. Three update cycles after a resize left the menu bar
    /// at its old width, measured.
    ///
    /// So this asserts the declaration rather than the behaviour: a root that
    /// fills its target is what makes the layout follow a resize, and a root
    /// with pixels written into it is what stops it. **That it does follow is
    /// still a person's to check**, and the pull request says so rather than
    /// implying this test covers it.
    ///
    /// Mutation: write `px(1280)` and `px(720)` on the root instead of
    /// `percent(100)`, and this fails.
    #[test]
    fn the_root_is_declared_to_fill_whatever_it_is_drawn_to() {
        let mut app = editor(headless());
        app.update();
        let root = app
            .world_mut()
            .query_filtered::<&Node, Without<Region>>()
            .iter(app.world())
            .find(|node| node.flex_direction == FlexDirection::Column)
            .expect("the root is a column")
            .clone();
        assert_eq!(
            root.width,
            percent(100),
            "the root does not fill its target"
        );
        assert_eq!(
            root.height,
            percent(100),
            "the root does not fill its target"
        );
    }

    /// There is a camera for the panels to be drawn to.
    ///
    /// Bevy UI is drawn through `ComputedUiTargetCamera`, and a node with no
    /// camera to target is a node nothing draws. Without this the editor came
    /// up as an empty window with five regions in the world and none of them
    /// visible, and every other test here passed: they ask the world what it
    /// holds, and the world held them.
    ///
    /// That is the gap this closes. It is not a claim that the layout looks
    /// right, which is a person's to make.
    ///
    /// Mutation: drop the `Camera2d` from `spawn_regions`, and this fails.
    #[test]
    fn there_is_a_camera_for_the_panels_to_be_drawn_to() {
        let mut app = editor(headless());
        app.update();
        let cameras = app.world_mut().query::<&Camera>().iter(app.world()).count();
        assert_eq!(cameras, 1, "the panels have no camera to be drawn to");
    }

    /// The panels carry this project's theme, not the one Feathers ships.
    ///
    /// `FeathersCorePlugin` initialises `UiTheme` itself, so the claim is that
    /// what ends up in the resource is this project's map. Asserted against
    /// `b2d_editor_ui::unity_theme`, which is where that decision lives.
    ///
    /// Mutation: drop `ThemePlugin` from `PanelsPlugin::build`, and this fails
    /// with Feathers' own theme in place.
    #[test]
    fn the_panels_carry_this_projects_theme() {
        let app = editor(headless());
        let theme = app.world().resource::<UiTheme>();
        let ours = b2d_editor_ui::unity_theme();
        assert_eq!(theme.0.color.len(), ours.color.len());
        let differing: Vec<String> = ours
            .color
            .iter()
            .filter(|(token, colour)| theme.0.color.get(*token) != Some(*colour))
            .map(|(token, _)| token.to_string())
            .collect();
        assert!(
            differing.is_empty(),
            "the editor draws {differing:?} differently"
        );
    }

    /// A row's name is the plugin it adds.
    ///
    /// The name column is the whole of what
    /// `the_group_carries_the_members_the_table_names` reads, so a `member!`
    /// that wrote the same name for every row would leave that test agreeing
    /// with a table that means something else. RK-001 in its narrowest form: a
    /// column added to a table that has no rows yet is a column nothing has
    /// looked at.
    ///
    /// Asserted against the type's own name rather than against the literal
    /// alone, so that the two columns are held to each other and not to a third
    /// copy of the same string.
    ///
    /// Mutation: write a literal in place of `stringify!($plugin)` in
    /// `member!`, and this fails.
    #[test]
    fn a_rows_name_is_the_plugin_it_adds() {
        let row: Member = member!(Probe);
        assert_eq!(row.0, "Probe", "the name column is not the plugin's name");
        assert!(
            core::any::type_name::<Probe>().ends_with(row.0),
            "the name column does not name the plugin the row adds"
        );
    }

    /// The table is built in the order it is written.
    ///
    /// `MEMBERS` says it holds its members in the order they are built, and a
    /// plugin that inserts a resource a later one reads depends on that being
    /// true. Nothing could check it while the table had fewer than two rows,
    /// which is why the rows are handed to `compose` here rather than taken
    /// from `MEMBERS`.
    ///
    /// Mutation: fold `members.iter().rev()` in `compose`, and this fails.
    #[test]
    fn the_table_is_built_in_the_order_it_is_written() {
        let rows: [Member; 2] = [member!(Probe), member!(SecondProbe)];
        let mut app = App::new();
        app.add_plugins(compose(&rows));
        assert_eq!(
            app.world().resource::<BuiltInOrder>().0,
            ["Probe", "SecondProbe"],
            "the members were not built in the order the table writes them"
        );
    }
}
