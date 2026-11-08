//! Generic editor widgets: split panels, tree views, inspector field rows,
//! docking.
//!
//! **This crate knows nothing about levels or scenarios.** That is deliberate:
//! a widget that knew the model is how the UI becomes the source of truth,
//! which `docs/specs/architecture.md` §6 forbids. It depends on neither
//! `b2d_data` nor `b2d_runtime`, and `docs/specs/crates.md` §3 says why.
//!
//! What it carries today is the theme and the inspector's field row.
//! `docs/specs/ui.md` §3 settles the stack as Bevy UI plus `bevy_feathers` and
//! says that what Feathers supplies is widget structure and behaviour, not
//! appearance.

mod field;

pub use field::{field_line, field_row};

use bevy::app::{App, Plugin};
use bevy::color::Color;
use bevy::feathers::dark_theme::create_dark_theme;
use bevy::feathers::theme::{SemanticToken, ThemeProps, UiTheme};
use bevy::feathers::tokens::semantic;

/// The greys and the accent this editor is drawn from.
///
/// Placed where Unity's dark editor places them, which
/// [`docs/specs/ui.md` §2](../../../docs/specs/ui.md) makes the design target.
/// **They are chosen to sit there rather than sampled from it**, and saying so
/// is the point: a comment claiming these are Unity's own values would be a
/// claim nobody here can check.
mod palette {
    use bevy::color::Color;

    /// Behind everything.
    pub const WINDOW: Color = Color::srgb(0.22, 0.22, 0.22);
    /// A panel's body.
    pub const SURFACE: Color = Color::srgb(0.24, 0.24, 0.24);
    /// A panel's header, and anything meant to recede.
    pub const RECESSED: Color = Color::srgb(0.18, 0.18, 0.18);
    /// A control at rest.
    pub const CONTROL: Color = Color::srgb(0.35, 0.35, 0.35);
    /// A control under the pointer.
    pub const CONTROL_HOVER: Color = Color::srgb(0.40, 0.40, 0.40);
    /// A control being pressed.
    pub const CONTROL_PRESSED: Color = Color::srgb(0.30, 0.30, 0.30);
    /// A control that cannot be used.
    pub const CONTROL_DISABLED: Color = Color::srgb(0.26, 0.26, 0.26);
    /// The line between things.
    pub const LINE: Color = Color::srgb(0.14, 0.14, 0.14);
    /// Words.
    pub const TEXT: Color = Color::srgb(0.82, 0.82, 0.82);
    /// Words that are not for now.
    pub const TEXT_DIM: Color = Color::srgb(0.50, 0.50, 0.50);
    /// What is chosen, and the ring around what has focus.
    pub const ACCENT: Color = Color::srgb(0.17, 0.36, 0.53);
    /// The same, darker, for a press.
    pub const ACCENT_PRESSED: Color = Color::srgb(0.13, 0.29, 0.43);
    /// The same, lighter, for hover.
    pub const ACCENT_HOVER: Color = Color::srgb(0.21, 0.43, 0.62);
    /// The same, dimmer, for disabled.
    pub const ACCENT_DISABLED: Color = Color::srgb(0.20, 0.27, 0.33);
    /// The three axes, where a widget names them.
    pub const AXIS_X: Color = Color::srgb(0.78, 0.24, 0.27);
    /// The three axes, where a widget names them.
    pub const AXIS_Y: Color = Color::srgb(0.45, 0.70, 0.22);
    /// The three axes, where a widget names them.
    pub const AXIS_Z: Color = Color::srgb(0.24, 0.45, 0.78);
}

/// The colour each of Feathers' semantic tokens draws with here.
///
/// Feathers 0.20 maps its design tokens, one per widget part, onto a few dozen
/// semantic ones (`surface.pane.header`, `fill.solid.hover`, `text.dim`), and
/// colours those. So the roles this project assigns are Feathers' own
/// semantic tokens, and its design-to-semantic assignment is kept as it
/// ships: [`docs/specs/ui.md` §3](../../../docs/specs/ui.md).
///
/// Each was chosen to keep what the editor looked like on 0.19, where this
/// coloured the design tokens by the role in their names: for every semantic
/// token, the colour most of the design tokens now pointing at it had. The
/// exceptions are named beside their rows.
///
/// A table rather than a `match`, because a `SemanticToken` holds a string and
/// cannot be a pattern; the constants are Feathers', so dropping one upstream
/// stops this compiling rather than quietly changing what it answers.
const ROLES: [(SemanticToken, Color); 33] = [
    (semantic::SURFACE_WINDOW, palette::WINDOW),
    (semantic::SURFACE_PANE_BODY, palette::SURFACE),
    (semantic::SURFACE_PANE_HEADER, palette::RECESSED),
    (semantic::SURFACE_SUBPANE_BODY, palette::SURFACE),
    (semantic::SURFACE_SUBPANE_HEADER, palette::RECESSED),
    (semantic::SURFACE_GROUP, palette::SURFACE),
    (semantic::SURFACE_DIALOG, palette::SURFACE),
    (semantic::SURFACE_MENU, palette::SURFACE),
    (semantic::BORDER_DEFAULT, palette::LINE),
    (semantic::TEXT_DEFAULT, palette::TEXT),
    (semantic::TEXT_DIM, palette::TEXT_DIM),
    (semantic::TEXT_DISABLED, palette::TEXT_DIM),
    (semantic::TEXT_ON_ACCENT, palette::TEXT),
    (semantic::FILL_ACCENT_DEFAULT, palette::ACCENT),
    (semantic::FILL_ACCENT_HOVER, palette::ACCENT_HOVER),
    (semantic::FILL_ACCENT_PRESSED, palette::ACCENT_PRESSED),
    (semantic::FILL_ACCENT_DISABLED, palette::ACCENT_DISABLED),
    // A button: `palette::CONTROL` is "a control at rest", which is what the
    // semantic token says, where most of the design tokens behind it had been
    // given the panel's own surface by the suffix rule.
    (semantic::FILL_SOLID_DEFAULT, palette::CONTROL),
    (semantic::FILL_SOLID_HOVER, palette::CONTROL_HOVER),
    (semantic::FILL_SOLID_PRESSED, palette::CONTROL_PRESSED),
    (semantic::FILL_SOLID_DISABLED, palette::CONTROL_DISABLED),
    // Transparent, as Feathers ships them: an item and a plain button take the
    // colour of what they sit on, which is a structure rather than a colour.
    (semantic::FILL_ITEM_DEFAULT, Color::NONE),
    (semantic::FILL_ITEM_HOVER, palette::CONTROL_HOVER),
    (semantic::FILL_ITEM_PRESSED, palette::CONTROL_PRESSED),
    (semantic::FILL_FIELD_DEFAULT, palette::SURFACE),
    (semantic::FILL_FIELD_HOVER, palette::CONTROL_HOVER),
    (semantic::FILL_FIELD_PRESSED, palette::CONTROL_PRESSED),
    (semantic::FILL_NONE, Color::NONE),
    (semantic::AXIS_X, palette::AXIS_X),
    (semantic::AXIS_Y, palette::AXIS_Y),
    (semantic::AXIS_Z, palette::AXIS_Z),
    // New in 0.20, with nothing on 0.19 to keep: a quaternion's fourth
    // number, which is not an axis on screen.
    (semantic::AXIS_W, palette::TEXT_DIM),
    (semantic::FOCUS_RING, palette::ACCENT_HOVER),
];

/// The colour a semantic token draws with here, or `None` for one this does
/// not know.
///
/// `None` is how a token added by a Feathers upgrade is caught: it arrives in
/// the roster, gets no colour, and
/// `every_semantic_token_feathers_reads_has_a_colour` fails.
fn role(token: &SemanticToken) -> Option<Color> {
    ROLES
        .iter()
        .find(|(known, _)| known == token)
        .map(|(_, colour)| *colour)
}

/// Feathers' dark theme with every colour replaced by this project's.
///
/// The design-to-semantic assignment is Feathers' own and stays that way: its
/// widgets reference the design tokens directly, and a renamed key would be
/// wrong everywhere Feathers draws. What `docs/specs/ui.md` §3 asks to replace
/// is the appearance, which is the semantic colours.
///
/// **The per-surface overrides keep their keys and take the base colour.**
/// Feathers lifts its fills on lighter surfaces, a group or a menu, because its
/// surfaces step by level. This project's do not, every body is
/// `palette::SURFACE`, so nothing needs lifting; and leaving Feathers' values
/// in place would draw its greys inside a group. The keys are kept so that a
/// surface Feathers adds arrives here with this project's colours.
///
/// Mutation: build the base from an empty map, and
/// `every_semantic_token_feathers_reads_has_a_colour` fails. Mutation: leave
/// the overrides as Feathers ships them, and
/// `every_surface_override_draws_with_this_projects_colour` fails.
pub fn unity_theme() -> ThemeProps {
    let mut theme = create_dark_theme();
    let assigned: Vec<SemanticToken> = theme.token_assignments.values().cloned().collect();
    theme.semantic_base = theme
        .semantic_base
        .keys()
        .cloned()
        .chain(assigned)
        .filter_map(|token| role(&token).map(|colour| (token, colour)))
        .collect();
    for overrides in theme.semantic_overrides.values_mut() {
        for (token, colour) in overrides.iter_mut() {
            if let Some(ours) = role(token) {
                *colour = ours;
            }
        }
    }
    theme
}

/// Puts [`unity_theme`] in place of whatever Feathers set up.
///
/// `FeathersCorePlugin` does `init_resource::<UiTheme>()`, so this overwrites
/// rather than initialises, which is what `UiTheme`'s own documentation says to
/// do: "Overwriting this resource changes the theme."
///
/// Mutation: drop the `insert_resource` call, and
/// `the_editor_draws_with_this_projects_colours` fails.
pub struct ThemePlugin;

impl Plugin for ThemePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(UiTheme(unity_theme()));
    }
}

#[cfg(test)]
mod tests {
    use super::{ThemePlugin, palette, role, unity_theme};
    use bevy::app::App;
    use bevy::feathers::dark_theme::create_dark_theme;
    use bevy::feathers::theme::{SemanticToken, ThemeToken, UiTheme};
    use bevy::platform::collections::HashSet;

    /// Every semantic token Feathers reads, from what it ships.
    ///
    /// The tokens it colours, the tokens its design tokens are assigned to,
    /// and the tokens its per-surface overrides name. Taken from Feathers'
    /// own theme so that the set is one this project does not own: RK-001's
    /// rule that a set is checked against something other than itself.
    fn roster() -> HashSet<SemanticToken> {
        let theirs = create_dark_theme();
        theirs
            .semantic_base
            .keys()
            .cloned()
            .chain(theirs.token_assignments.values().cloned())
            .chain(
                theirs
                    .semantic_overrides
                    .values()
                    .flat_map(|overrides| overrides.keys().cloned()),
            )
            .collect()
    }

    /// Every semantic token Feathers reads has a colour here.
    ///
    /// A token added by an upgrade arrives in the roster and fails this until
    /// it has a role.
    ///
    /// Mutation: build `unity_theme`'s base from an empty map, and this fails.
    /// Remove a row from `ROLES`, and it fails naming the token.
    #[test]
    fn every_semantic_token_feathers_reads_has_a_colour() {
        let roster = roster();
        assert!(
            roster.len() >= 30,
            "the roster is {} tokens, which is not the set this was written for",
            roster.len()
        );
        let ours = unity_theme().semantic_base;
        let mut missing: Vec<String> = roster
            .iter()
            .filter(|token| !ours.contains_key(*token))
            .map(ToString::to_string)
            .collect();
        missing.sort();
        assert!(missing.is_empty(), "no colour for {missing:?}");
    }

    /// The theme is this project's, not the one Feathers ships.
    ///
    /// Coverage alone is satisfied by copying Feathers' own map, which would
    /// be the opposite of what `docs/specs/ui.md` §3 asks for.
    ///
    /// Mutation: return `create_dark_theme()` from `unity_theme`, and this
    /// fails.
    #[test]
    fn the_theme_is_not_the_one_feathers_ships() {
        let theirs = create_dark_theme().semantic_base;
        let ours = unity_theme().semantic_base;
        let same: Vec<String> = ours
            .iter()
            .filter(|(token, colour)| theirs.get(*token) == Some(*colour))
            .map(|(token, _)| token.to_string())
            .collect();
        assert!(
            same.len() * 2 < ours.len(),
            "{} of {} colours are still Feathers': {same:?}",
            same.len(),
            ours.len()
        );
    }

    /// A widget's design token draws with the ramp its role names, through
    /// Feathers' assignment and this project's colour both.
    ///
    /// Three tokens rather than a count: a surface, a line and a word, each
    /// read the way a widget reads it, by design token. The line is a menu's
    /// border rather than the pane header's, which Feathers 0.20 assigns to
    /// the pane body's colour so that the divider blends into the body.
    ///
    /// Mutation: move `surface.pane.header` off `RECESSED` in `ROLES`, and
    /// this fails.
    #[test]
    fn a_token_draws_with_the_ramp_its_role_names() {
        let ours = unity_theme();
        let of = |name: &'static str| ours.color(&ThemeToken::new_static(name));
        assert_eq!(of("feathers.pane.header.bg"), palette::RECESSED);
        assert_eq!(of("feathers.menu.border"), palette::LINE);
        assert_eq!(of("feathers.text.main"), palette::TEXT);
    }

    /// A semantic token this does not recognise gets no colour.
    ///
    /// This is what makes the coverage test above able to fail.
    ///
    /// Mutation: give `role` a fallback colour, and this fails.
    #[test]
    fn a_role_this_does_not_know_gets_no_colour() {
        assert_eq!(role(&SemanticToken::new_static("nonsense")), None);
    }

    /// Every per-surface override draws with this project's colour for its
    /// token, so that Feathers' greys do not appear inside a group or a menu.
    ///
    /// Mutation: leave the overrides as Feathers ships them in `unity_theme`,
    /// and this fails.
    #[test]
    fn every_surface_override_draws_with_this_projects_colour() {
        let theme = unity_theme();
        assert!(
            !theme.semantic_overrides.is_empty(),
            "Feathers ships no overrides, so this asserts nothing"
        );
        for (level, overrides) in &theme.semantic_overrides {
            for (token, colour) in overrides {
                assert_eq!(
                    Some(*colour),
                    role(token),
                    "{token} on {level:?} is not this project's colour"
                );
            }
        }
    }

    /// The editor draws with this project's colours.
    ///
    /// Mutation: drop the `insert_resource` in `ThemePlugin::build`, and this
    /// fails.
    #[test]
    fn the_editor_draws_with_this_projects_colours() {
        let mut app = App::new();
        app.add_plugins(ThemePlugin);
        let theme = app.world().resource::<UiTheme>();
        assert_eq!(theme.0.semantic_base, unity_theme().semantic_base);
    }
}
