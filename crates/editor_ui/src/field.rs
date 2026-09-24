//! The inspector's field row: a name, and the value beside it.

use bevy::ecs::hierarchy::Children;
use bevy::feathers::constants::size;
use bevy::feathers::display::{label, label_dim};
use bevy::scene::{Scene, bsn};
use bevy::ui::{AlignItems, Display, FlexDirection, Node, Overflow, Val};

/// How wide the name column is.
///
/// Fixed rather than a fraction, so that the values in a run of these rows
/// start at the same x and a column of numbers reads down. **It says nothing
/// about a panel**, only about the rows that go through here: a caller that
/// draws some of its lines another way gets no alignment between the two, and
/// the inspector is one, drawing a component that is not a named struct as a
/// bare value with no name beside it. It is a `const` and not a design
/// token because colour is the only token kind `ThemeProps` carries in 0.19.1,
/// which [`docs/specs/ui.md` §3](../../../docs/specs/ui.md) records; Feathers
/// keeps its own spacing in `constants.rs` the same way.
const NAME_WIDTH: Val = Val::Px(140.0);

/// The gap between the name and the value.
const GAP: Val = Val::Px(8.0);

/// A name, and room beside it for whatever the caller puts there.
///
/// **It takes one string and knows nothing else**: not what a component is,
/// not what reflection is, and not where the name came from. That is what
/// [`docs/specs/crates.md` §3](../../../docs/specs/crates.md) requires of this
/// crate.
///
/// [`field_row`] is this with a label already beside the name, so both read the
/// same name column width and a panel mixing the two still reads as a column. The
/// inspector in `crates/editor/src/inspector.rs` uses this one for the lines
/// that carry number boxes, which [`docs/specs/ui.md` §7](../../../docs/specs/ui.md)
/// settles.
///
/// Mutation: give [`field_row`] a name column of its own rather than building
/// on this, and `a_row_and_a_line_share_the_name_column` fails.
pub fn field_line(name: impl Into<String>) -> impl Scene {
    bsn! {
        Node {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            min_height: size::ROW_HEIGHT,
            column_gap: GAP,
        }
        Children [
            (
                Node { width: NAME_WIDTH, overflow: {Overflow::clip()} }
                Children [label(name)]
            ),
        ]
    }
}

/// A name and the value beside it, as one line.
///
/// **It takes two strings and knows nothing else**: not what a component is,
/// not what reflection is, and not where the text came from. That is what
/// [`docs/specs/crates.md` §3](../../../docs/specs/crates.md) requires of this
/// crate, and turning the value into text is the caller's job. The inspector
/// in `crates/editor/src/inspector.rs` is the first caller, and the database
/// editor's table and the manuscript UI are the ones this shape is kept plain
/// for.
///
/// Mutation: spawn the name in place of the value, and
/// `a_field_row_carries_the_name_and_the_value` fails.
pub fn field_row(name: impl Into<String>, value: impl Into<String>) -> impl Scene {
    bsn! {
        field_line(name)
        Children [label_dim(value)]
    }
}

#[cfg(test)]
mod tests {
    use super::{field_line, field_row};
    use bevy::app::{App, TaskPoolPlugin};
    use bevy::asset::{AssetApp, AssetPlugin};
    use bevy::ecs::entity::Entity;
    use bevy::ecs::hierarchy::Children;
    use bevy::scene::{Scene, ScenePlugin, WorldSceneExt};
    use bevy::ui::widget::Text;
    use bevy::ui::{Node, Val};

    /// A field row carries the name and the value, in that order.
    ///
    /// Mutation: spawn the name in place of the value in [`field_row`], and
    /// this fails.
    #[test]
    fn a_field_row_carries_the_name_and_the_value() {
        let mut app = App::new();
        app.add_plugins((
            TaskPoolPlugin::default(),
            AssetPlugin::default(),
            ScenePlugin,
        ))
        // The label asks for a font handle, and allocating one needs the asset
        // type registered. `TextPlugin` would do it and would bring a renderer
        // with it, which a test that reads strings has no use for.
        .init_asset::<bevy::text::Font>();
        let root = app
            .world_mut()
            .spawn_scene(field_row("translation", "Vec3(1.0, 2.0, 3.0)"))
            .expect("the row is a scene the world can spawn")
            .id();

        let mut found: Vec<String> = Vec::new();
        let mut stack = vec![root];
        while let Some(entity) = stack.pop() {
            if let Some(text) = app.world().entity(entity).get::<Text>() {
                found.push(text.0.clone());
            }
            if let Some(children) = app.world().entity(entity).get::<Children>() {
                let kids: Vec<Entity> = children.iter().copied().collect();
                for child in kids.into_iter().rev() {
                    stack.push(child);
                }
            }
        }

        assert_eq!(found, ["translation", "Vec3(1.0, 2.0, 3.0)"]);
    }

    /// A row and a line start their values at the same place.
    ///
    /// The claim on [`NAME_WIDTH`] is that a panel mixing the two reads as one
    /// column, and that holds only while [`field_row`] is built on
    /// [`field_line`] rather than drawing a name column of its own. Read off
    /// the name node itself rather than by flattening the tree, per RK-012:
    /// a width on a wrapper would satisfy a flattened read either way.
    ///
    /// Mutation: give `field_row` its own `Node { width: ... }` for the name,
    /// at any other width, and this fails.
    #[test]
    fn a_row_and_a_line_share_the_name_column() {
        let mut app = harness();
        let line = spawn(&mut app, field_line("translation"));
        let row = spawn(&mut app, field_row("translation", "Vec3(1.0, 2.0, 3.0)"));

        assert_eq!(
            name_width(&app, line),
            name_width(&app, row),
            "a line and a row do not start their values at the same place"
        );
        assert!(
            matches!(name_width(&app, line), Val::Px(_)),
            "the name column has no fixed width, so the comparison above holds              for two columns that are both unset"
        );
    }

    /// An app that can spawn one of these scenes and nothing more.
    fn harness() -> App {
        let mut app = App::new();
        app.add_plugins((
            TaskPoolPlugin::default(),
            AssetPlugin::default(),
            ScenePlugin,
        ))
        // The label asks for a font handle, and allocating one needs the asset
        // type registered. `TextPlugin` would do it and would bring a renderer
        // with it, which a test that reads strings has no use for.
        .init_asset::<bevy::text::Font>();
        app
    }

    /// Spawn a scene and say which entity is its root.
    fn spawn(app: &mut App, scene: impl Scene) -> Entity {
        app.world_mut()
            .spawn_scene(scene)
            .expect("the scene is one the world can spawn")
            .id()
    }

    /// How wide the name column of one of these rows is.
    ///
    /// The name column is the row's first child, and this reads the `Node` on
    /// that entity rather than searching the subtree for one.
    fn name_width(app: &App, row: Entity) -> Val {
        let name = app
            .world()
            .entity(row)
            .get::<Children>()
            .and_then(|children| children.iter().next().copied())
            .expect("a field row has a name column");
        app.world()
            .entity(name)
            .get::<Node>()
            .expect("the name column is a node")
            .width
    }
}
