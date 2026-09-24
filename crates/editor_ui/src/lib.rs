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
use bevy::feathers::theme::{ThemeProps, ThemeToken, UiTheme};
use bevy::feathers::tokens;
use bevy::platform::collections::HashMap;

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

/// The colour a token asks for, read from the role its name gives it.
///
/// Feathers names a token `feathers.<widget>.<role>`, and the roles repeat
/// across widgets: `bg`, `bg.hover`, `border`, `text.disabled` and so on. So
/// this matches the role rather than the widget, and one ramp answers all of
/// them. The alternative is 137 values written out, most of them for widgets
/// this editor has not put on screen yet, each one invented separately.
///
/// `None` is a role this does not recognise, which is how a token added by a
/// Feathers upgrade is caught: it arrives in the roster, gets no colour, and
/// `every_token_feathers_reads_has_a_colour` fails.
///
/// Mutation: return `Some(palette::WINDOW)` for the unmatched arm, and that
/// test can no longer fail.
fn role(token: &ThemeToken) -> Option<Color> {
    // Two tokens name a thing rather than a role, and the suffix rules below
    // would answer for them wrongly: a window's background is not a panel's,
    // and a focus ring is not a border. They go through Feathers' own
    // constants rather than the strings behind them, so that dropping either
    // one upstream stops this compiling instead of quietly changing what it
    // answers.
    if *token == tokens::WINDOW_BG {
        return Some(palette::WINDOW);
    }
    if *token == tokens::FOCUS_RING {
        return Some(palette::ACCENT_HOVER);
    }
    let name = token.to_string();
    let role = name
        .strip_prefix("feathers.")
        .and_then(|rest| rest.split_once('.').map(|(_widget, role)| role))
        .unwrap_or("");
    // One arm per colour rather than per widget, because the roles repeat and
    // `match_same_arms` is a warning this workspace turns into an error. The
    // patterns are ordered surface, control, accent, line, word.
    Some(match role {
        "bg" | "body.bg" | "slide.bg" | "label.bg" => palette::SURFACE,
        "header.bg" => palette::RECESSED,
        "plain.bg" | "thumb" | "bar" | "selection.unfocused" => palette::CONTROL,
        "bg.hover"
        | "slide.bg.hover"
        | "plain.bg.hover"
        | "thumb.hover"
        | "bar.hover"
        | "border.hover"
        | "border.hover.pressed"
        | "slide.border.hover" => palette::CONTROL_HOVER,
        "bg.pressed"
        | "slide.bg.pressed"
        | "plain.bg.pressed"
        | "bar.pressed"
        | "border.pressed"
        | "slide.border.pressed" => palette::CONTROL_PRESSED,
        "bg.disabled"
        | "slide.bg.disabled"
        | "plain.bg.disabled"
        | "bar.disabled"
        | "border.disabled"
        | "slide.border.disabled" => palette::CONTROL_DISABLED,
        "bg.checked"
        | "slide.bg.checked"
        | "primary.bg"
        | "selection"
        | "bg.selected"
        | "border.checked"
        | "slide.border.checked" => palette::ACCENT,
        "bg.checked.hover"
        | "slide.bg.checked.hover"
        | "primary.bg.hover"
        | "bg.focused"
        | "border.checked.hover"
        | "slide.border.checked.hover" => palette::ACCENT_HOVER,
        "bg.checked.pressed"
        | "slide.bg.checked.pressed"
        | "primary.bg.pressed"
        | "border.checked.pressed"
        | "slide.border.checked.pressed" => palette::ACCENT_PRESSED,
        "bg.checked.disabled"
        | "slide.bg.checked.disabled"
        | "primary.bg.disabled"
        | "border.checked.disabled"
        | "slide.border.checked.disabled" => palette::ACCENT_DISABLED,
        "border" | "body.border" | "slide.border" | "header.border" | "header.divider" => {
            palette::LINE
        }
        "text" | "txt" | "main" | "header.text" | "primary.txt" | "cursor" | "mark"
        | "mark.hover" | "mark.pressed" => palette::TEXT,
        "text.disabled" | "txt.disabled" | "dim" | "primary.txt.disabled" | "mark.disabled" => {
            palette::TEXT_DIM
        }
        "axis.x" => palette::AXIS_X,
        "axis.y" => palette::AXIS_Y,
        "axis.z" => palette::AXIS_Z,
        _ => return None,
    })
}

/// Every colour this editor draws with, under the token keys Feathers reads.
///
/// The keys are Feathers' own and stay that way. Its widgets reference the
/// constants in `bevy_feathers::tokens` directly, and `UiTheme::color` on a key
/// the theme lacks logs a warning and returns an error colour, so a renamed key
/// would be wrong everywhere Feathers draws. What `docs/specs/ui.md` §3 asks to
/// replace is the appearance, which is the values.
///
/// The roster comes from Feathers rather than from a list here, so a token it
/// adds in a later version arrives on its own.
///
/// Mutation: build this from an empty map, and
/// `every_token_feathers_reads_has_a_colour` fails.
pub fn unity_theme() -> ThemeProps {
    let mut color: HashMap<ThemeToken, Color> = HashMap::default();
    for token in create_dark_theme().color.keys() {
        if let Some(chosen) = role(token) {
            color.insert(token.clone(), chosen);
        }
    }
    ThemeProps { color }
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
    use bevy::feathers::theme::{ThemeToken, UiTheme};

    /// Every token Feathers reads has a colour here.
    ///
    /// The roster is Feathers' own, taken from the theme it ships, so this is
    /// asserted against a set this project does not own: a token added by an
    /// upgrade arrives here and fails until it has a role. That is RK-001's
    /// rule that a set is checked against something other than itself.
    ///
    /// Mutation: give `role` a catch-all arm returning a colour, and this can
    /// no longer fail. Remove a role arm, and it fails naming the token.
    #[test]
    fn every_token_feathers_reads_has_a_colour() {
        let roster = create_dark_theme().color;
        assert!(
            roster.len() > 100,
            "the roster is {} tokens, which is not the set this was written for",
            roster.len()
        );
        let ours = unity_theme().color;
        let missing: Vec<String> = roster
            .keys()
            .filter(|token| !ours.contains_key(*token))
            .map(ToString::to_string)
            .collect();
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
        let theirs = create_dark_theme().color;
        let ours = unity_theme().color;
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

    /// A named token draws with the ramp its role names.
    ///
    /// Three tokens rather than a count, which is what the issue asks for: a
    /// surface, a line and a word, each read back through the map.
    ///
    /// Mutation: move `header.bg` off `RECESSED` in `role`, and this fails.
    #[test]
    fn a_token_draws_with_the_ramp_its_role_names() {
        let ours = unity_theme().color;
        let of = |name: &'static str| ours.get(&ThemeToken::new_static(name)).copied();
        assert_eq!(of("feathers.pane.header.bg"), Some(palette::RECESSED));
        assert_eq!(of("feathers.pane.header.border"), Some(palette::LINE));
        assert_eq!(of("feathers.text.main"), Some(palette::TEXT));
    }

    /// A role this does not recognise gets no colour.
    ///
    /// This is what makes the coverage test above able to fail.
    ///
    /// Mutation: give `role` a catch-all arm, and this fails.
    #[test]
    fn a_role_this_does_not_know_gets_no_colour() {
        assert_eq!(
            role(&ThemeToken::new_static("feathers.button.nonsense")),
            None
        );
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
        assert_eq!(theme.0.color.len(), unity_theme().color.len());
    }
}
