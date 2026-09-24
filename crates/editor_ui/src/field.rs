//! The inspector's field row: a name, and the value beside it.

use bevy::ecs::hierarchy::Children;
use bevy::feathers::constants::size;
use bevy::feathers::display::{label, label_dim};
use bevy::scene::{Scene, bsn};
use bevy::ui::{AlignItems, Display, FlexDirection, Node, Val};

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
const NAME_WIDTH: Val = Val::Px(110.0);

/// The gap between the name and the value.
const GAP: Val = Val::Px(8.0);

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
        Node {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            min_height: size::ROW_HEIGHT,
            column_gap: GAP,
        }
        Children [
            (Node { width: NAME_WIDTH } Children [label(name)]),
            label_dim(value),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::field_row;
    use bevy::app::{App, TaskPoolPlugin};
    use bevy::asset::{AssetApp, AssetPlugin};
    use bevy::ecs::entity::Entity;
    use bevy::ecs::hierarchy::Children;
    use bevy::scene::{ScenePlugin, WorldSceneExt};
    use bevy::ui::widget::Text;

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
}
