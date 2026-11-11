//! Which colour space an application's windows tell the system their
//! pixels are in, decided once for every window.
//!
//! A display with a wider gamut than sRGB (Display P3) shows a window's
//! values in its own primaries unless the window says what they are. QML
//! colours are written in sRGB, so the shell says sRGB, and a colour
//! looks the same on every display. Qt Quick's windows say nothing, so
//! there the same values look more saturated; an application ported from
//! Qt that should look as it did there chooses [`ColorSpace::Untagged`]
//! (#382).
//!
//! The decision lives here, apart from the platform code, so it can be
//! tested where the platform is not there: macOS's layer code only does
//! what [`ColorSpace::layer_tag`] says.

/// The colour space an application's windows say their pixels are in
/// ([`crate::ShellConfig::color_space`]).
///
/// macOS is told, for windows drawn with the GPU. Elsewhere the shell
/// tells the system nothing, and both choices are shown alike; a window
/// drawn on the processor ([`crate::ShellConfig::drawn_on_processor`])
/// keeps what it says either way. More may come (Display P3), so a
/// `match` on it needs a wildcard arm.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ColorSpace {
    /// sRGB, which QML colours are written in: a colour looks the same on
    /// every display.
    #[default]
    Srgb,
    /// None at all, as Qt Quick's windows: the display shows the values
    /// in its own primaries, so on a wide-gamut display they look more
    /// saturated than they are written.
    Untagged,
}

/// The variable a user sets to choose either way, over the application.
pub(crate) const VAR: &str = "LJOS_COLOR_SPACE";

/// What a window's layer is given before a frame is shown.
// Only macOS's layers are told; the decision is tested everywhere.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LayerTag {
    Srgb,
}

impl ColorSpace {
    /// The environment's choice (`value` of [`VAR`]: `srgb` or
    /// `untagged`), else the application's (`chosen`). A value that is
    /// neither keeps the application's and is given back, to be logged.
    pub(crate) fn from_env(
        value: Option<&str>,
        chosen: ColorSpace,
    ) -> (ColorSpace, Option<String>) {
        match value.map(str::trim) {
            None | Some("") => (chosen, None),
            Some(v) if v.eq_ignore_ascii_case("srgb") => (ColorSpace::Srgb, None),
            Some(v) if v.eq_ignore_ascii_case("untagged") => (ColorSpace::Untagged, None),
            Some(v) => (
                chosen,
                Some(format!(
                    "{VAR}={v} is neither srgb nor untagged: the application's {chosen:?} stays"
                )),
            ),
        }
    }

    /// What a window's layer is given before a frame, when it already
    /// has a colour space (`has_one`) or not. One it has is kept: the
    /// GPU's surface gives an extended or HDR layer its own, and the
    /// application's choice is about the plain one it leaves with none.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn layer_tag(self, has_one: bool) -> Option<LayerTag> {
        match self {
            _ if has_one => None,
            ColorSpace::Srgb => Some(LayerTag::Srgb),
            ColorSpace::Untagged => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ColorSpace, LayerTag};
    use crate::ShellConfig;

    /// The layer decision for a window opened with `config`, with the
    /// environment's `value`, as the shell makes it before each frame.
    fn tag(config: &ShellConfig, value: Option<&str>, has_one: bool) -> Option<LayerTag> {
        ColorSpace::from_env(value, config.color_space)
            .0
            .layer_tag(has_one)
    }

    #[test]
    fn a_window_says_srgb_unless_the_application_chooses_otherwise() {
        let config = ShellConfig::new("app");
        assert_eq!(config.color_space, ColorSpace::Srgb);
        assert_eq!(tag(&config, None, false), Some(LayerTag::Srgb));
    }

    /// #382: an application ported from Qt Quick leaves its windows
    /// untagged, and the layer is left with none.
    #[test]
    fn an_application_that_chooses_untagged_leaves_the_layer_untagged() {
        let config = ShellConfig::new("app").with_color_space(ColorSpace::Untagged);
        assert_eq!(tag(&config, None, false), None);
    }

    #[test]
    fn a_layer_that_has_a_colour_space_keeps_it_either_way() {
        for space in [ColorSpace::Srgb, ColorSpace::Untagged] {
            let config = ShellConfig::new("app").with_color_space(space);
            assert_eq!(tag(&config, None, true), None, "{space:?}");
        }
    }

    #[test]
    fn the_user_chooses_over_the_application() {
        let srgb = ShellConfig::new("app");
        let untagged = ShellConfig::new("app").with_color_space(ColorSpace::Untagged);
        assert_eq!(tag(&srgb, Some(" Untagged "), false), None);
        assert_eq!(tag(&untagged, Some("srgb"), false), Some(LayerTag::Srgb));
        assert_eq!(tag(&untagged, Some(""), false), None);
    }

    #[test]
    fn a_value_the_shell_does_not_know_keeps_the_application_s_and_says_so() {
        let (space, note) = ColorSpace::from_env(Some("p3"), ColorSpace::Untagged);
        assert_eq!(space, ColorSpace::Untagged);
        let note = note.expect("a note to log");
        assert!(note.contains("LJOS_COLOR_SPACE=p3"), "{note}");
    }
}
