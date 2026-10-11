//! What the system prefers: the model behind `Qt.styleHints`.
//!
//! Plain values with no toolkit and no engine in them. A host reads its
//! platform's settings into a [`SystemPrefs`] and hands it to the scene's
//! side (`ljos_quick::system_prefs`), which makes each value something a
//! binding can follow. Nothing here polls: a host tells when a setting
//! changes.
//!
//! See `docs/ARCHITECTURE.md` §14.

/// `Qt::ColorScheme`, with Qt's numbers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ColorScheme {
    /// The system has not said. As an application's own wish
    /// (`Qt.styleHints.colorScheme = Qt.ColorScheme.Unknown`) it means
    /// "whatever the system has".
    Unknown,
    #[default]
    Light,
    Dark,
}

impl ColorScheme {
    pub fn qt_value(self) -> i32 {
        match self {
            ColorScheme::Unknown => 0,
            ColorScheme::Light => 1,
            ColorScheme::Dark => 2,
        }
    }

    /// A number Qt does not list is `Unknown`.
    pub fn from_qt(value: i32) -> ColorScheme {
        match value {
            1 => ColorScheme::Light,
            2 => ColorScheme::Dark,
            _ => ColorScheme::Unknown,
        }
    }
}

/// `Qt::ContrastPreference`, with Qt's numbers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Contrast {
    #[default]
    NoPreference,
    High,
}

impl Contrast {
    pub fn qt_value(self) -> i32 {
        match self {
            Contrast::NoPreference => 0,
            Contrast::High => 1,
        }
    }

    pub fn from_qt(value: i32) -> Contrast {
        if value == 1 {
            Contrast::High
        } else {
            Contrast::NoPreference
        }
    }
}

/// The settings of the system an application adapts to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SystemPrefs {
    pub color_scheme: ColorScheme,
    pub contrast: Contrast,
    /// The user asked for less motion: animations end in the step they
    /// start in.
    pub reduced_motion: bool,
    /// How much larger (or smaller) than written the user wants text.
    /// Text only: the scale of the screen is the window's.
    pub font_scale: f64,
    /// An assistive technology reads the application.
    pub screen_reader_active: bool,
}

impl Default for SystemPrefs {
    /// A system that says nothing: light, no preference, text as written.
    fn default() -> Self {
        SystemPrefs {
            color_scheme: ColorScheme::Light,
            contrast: Contrast::NoPreference,
            reduced_motion: false,
            font_scale: 1.0,
            screen_reader_active: false,
        }
    }
}

impl SystemPrefs {
    /// The text scale as it is applied: a value a platform could hand
    /// over by mistake (zero, negative, not a number) is "as written",
    /// and the scale is kept where text stays legible and layouts stay
    /// finite.
    pub fn sane_font_scale(scale: f64) -> f64 {
        if scale.is_finite() && scale > 0.0 {
            scale.clamp(0.25, 8.0)
        } else {
            1.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_system_that_says_nothing_is_light_and_plain() {
        let p = SystemPrefs::default();
        assert_eq!(p.color_scheme, ColorScheme::Light);
        assert_eq!(p.contrast, Contrast::NoPreference);
        assert!(!p.reduced_motion);
        assert_eq!(p.font_scale, 1.0);
        assert!(!p.screen_reader_active);
    }

    #[test]
    fn schemes_and_contrasts_have_qts_numbers() {
        for (s, n) in [
            (ColorScheme::Unknown, 0),
            (ColorScheme::Light, 1),
            (ColorScheme::Dark, 2),
        ] {
            assert_eq!(s.qt_value(), n);
            assert_eq!(ColorScheme::from_qt(n), s);
        }
        assert_eq!(ColorScheme::from_qt(7), ColorScheme::Unknown);
        for (c, n) in [(Contrast::NoPreference, 0), (Contrast::High, 1)] {
            assert_eq!(c.qt_value(), n);
            assert_eq!(Contrast::from_qt(n), c);
        }
        assert_eq!(Contrast::from_qt(-3), Contrast::NoPreference);
    }

    #[test]
    fn a_font_scale_that_makes_no_sense_is_as_written() {
        assert_eq!(SystemPrefs::sane_font_scale(1.5), 1.5);
        assert_eq!(SystemPrefs::sane_font_scale(0.0), 1.0);
        assert_eq!(SystemPrefs::sane_font_scale(-2.0), 1.0);
        assert_eq!(SystemPrefs::sane_font_scale(f64::NAN), 1.0);
        assert_eq!(SystemPrefs::sane_font_scale(f64::INFINITY), 1.0);
        assert_eq!(SystemPrefs::sane_font_scale(100.0), 8.0);
        assert_eq!(SystemPrefs::sane_font_scale(0.01), 0.25);
    }
}
