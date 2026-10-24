//! What text follows on each platform: how many pixels a point is, and the
//! font an application starts with. Each is a pure function of the
//! platform, so every platform's answer is checked on any machine; the
//! running one is [`Platform::CURRENT`].
//!
//! Qt gives a point at the screen's logical DPI, which is 72 on macOS and
//! 96 elsewhere: 13 pt is 13 px on macOS and 17 px on Linux. Its
//! application font on macOS is the system UI font (San Francisco, named
//! `.AppleSystemUIFont`) at 13 pt; on Linux without a desktop to ask, and
//! on Windows, it is Sans Serif at 9 pt, which on Windows resolves to the
//! system UI font at 12 px.

/// A platform whose conventions differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Windows,
    /// Linux, the BSDs, Android, the web: 96 dpi and Qt's generic font.
    Other,
}

impl Platform {
    /// The platform this was built for.
    pub const CURRENT: Platform = if cfg!(target_os = "macos") {
        Platform::MacOs
    } else if cfg!(windows) {
        Platform::Windows
    } else {
        Platform::Other
    };

    /// Pixels in a logical inch of the screen, which holds 72 points.
    pub const fn logical_dpi(self) -> f64 {
        match self {
            Platform::MacOs => 72.0,
            Platform::Windows | Platform::Other => 96.0,
        }
    }

    /// Pixels for `points` on this platform.
    pub fn pixels(self, points: f64) -> f64 {
        // Multiplied before it is divided, so whole points that make whole
        // pixels (9 pt, 12 px) come out whole.
        points * self.logical_dpi() / 72.0
    }

    /// Points for `pixels` on this platform.
    pub fn points(self, pixels: f64) -> f64 {
        pixels * 72.0 / self.logical_dpi()
    }

    /// The family of the font an application starts with.
    pub const fn default_family(self) -> &'static str {
        match self {
            Platform::MacOs => SYSTEM_UI_FAMILY,
            Platform::Windows | Platform::Other => "Sans Serif",
        }
    }

    /// The point size of the font an application starts with.
    pub const fn default_point_size(self) -> f64 {
        match self {
            Platform::MacOs => 13.0,
            Platform::Windows | Platform::Other => 9.0,
        }
    }
}

/// The name Qt gives macOS's system UI font. A text engine takes it for
/// the platform's `system-ui` family, whatever the font's file calls it.
pub const SYSTEM_UI_FAMILY: &str = ".AppleSystemUIFont";

/// Pixels for `points` on the running platform.
pub fn points_to_pixels(points: f64) -> f64 {
    Platform::CURRENT.pixels(points)
}

/// Points for `pixels` on the running platform.
pub fn pixels_to_points(pixels: f64) -> f64 {
    Platform::CURRENT.points(pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_is_a_pixel_on_macos_and_four_thirds_elsewhere() {
        assert_eq!(Platform::MacOs.pixels(13.0), 13.0);
        assert_eq!(Platform::Other.pixels(13.0), 13.0 * 4.0 / 3.0);
        assert_eq!(Platform::Windows.pixels(9.0), 12.0);
        assert_eq!(Platform::Other.pixels(9.0), 12.0);
        assert_eq!(Platform::Other.pixels(11.0), 11.0 * 4.0 / 3.0);
    }

    #[test]
    fn pixels_go_back_to_the_points_they_came_from() {
        assert_eq!(Platform::MacOs.points(13.0), 13.0);
        assert_eq!(Platform::Other.points(12.0), 9.0);
        assert_eq!(Platform::Other.points(16.0), 12.0);
        for p in [Platform::MacOs, Platform::Windows, Platform::Other] {
            for pt in [1.0, 7.5, 9.0, 10.0, 11.0, 13.0, 24.0] {
                assert!((p.points(p.pixels(pt)) - pt).abs() < 1e-12, "{p:?} {pt}");
            }
        }
    }

    #[test]
    fn macos_starts_with_the_system_ui_font_at_13_points() {
        assert_eq!(Platform::MacOs.default_family(), ".AppleSystemUIFont");
        assert_eq!(Platform::MacOs.default_point_size(), 13.0);
        assert_eq!(Platform::MacOs.pixels(13.0), 13.0);
    }

    #[test]
    fn elsewhere_qts_generic_font_stays() {
        for p in [Platform::Windows, Platform::Other] {
            assert_eq!(p.default_family(), "Sans Serif");
            assert_eq!(p.default_point_size(), 9.0);
        }
    }

    #[test]
    fn the_running_platform_is_the_target() {
        let expect = if cfg!(target_os = "macos") {
            Platform::MacOs
        } else if cfg!(windows) {
            Platform::Windows
        } else {
            Platform::Other
        };
        assert_eq!(Platform::CURRENT, expect);
        assert_eq!(points_to_pixels(9.0), expect.pixels(9.0));
        assert_eq!(pixels_to_points(12.0), expect.points(12.0));
    }
}
