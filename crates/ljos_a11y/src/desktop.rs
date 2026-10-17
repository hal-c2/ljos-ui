//! The keys of a Linux desktop's settings that say what its user prefers,
//! as the settings portal (`org.freedesktop.portal.Settings`) hands them
//! over: the portal's own `org.freedesktop.appearance`, and the keys of
//! GNOME's and KDE's settings a portal passes on. Plain values: the shell
//! reads the bus, and a test says what the bus would.

use crate::prefs::{ColorScheme, Contrast, HostPrefs};

/// A setting's value, of the D-Bus types the keys have.
#[derive(Clone, Debug, PartialEq)]
pub enum SettingValue {
    Bool(bool),
    Uint(u32),
    Int(i64),
    Double(f64),
    Text(String),
}

impl SettingValue {
    fn number(&self) -> Option<f64> {
        match *self {
            SettingValue::Uint(n) => Some(f64::from(n)),
            SettingValue::Int(n) => Some(n as f64),
            SettingValue::Double(n) => Some(n),
            _ => None,
        }
    }
}

/// What the desktop said last, of the keys that carry a preference.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DesktopSettings {
    /// `org.freedesktop.appearance color-scheme`: 1 dark, 2 light, 0 no
    /// preference, which desktops show light.
    appearance_scheme: Option<ColorScheme>,
    /// `org.freedesktop.appearance reduced-motion`: 1 is less motion.
    appearance_motion: Option<bool>,
    /// `org.freedesktop.appearance contrast`: 1 is higher contrast.
    appearance_contrast: Option<bool>,
    /// `org.gnome.desktop.a11y.interface reduced-motion`: `reduce`.
    gnome_motion: Option<bool>,
    /// `org.gnome.desktop.interface enable-animations`, turned off.
    gnome_no_animations: Option<bool>,
    /// `org.gnome.desktop.a11y.interface high-contrast`.
    gnome_contrast: Option<bool>,
    /// `org.gnome.desktop.interface text-scaling-factor`.
    gnome_text_scale: Option<f64>,
    /// `org.kde.kdeglobals.KDE AnimationDurationFactor` of 0: no
    /// animations.
    kde_no_animations: Option<bool>,
}

impl DesktopSettings {
    /// The namespaces to ask the portal for.
    pub const NAMESPACES: [&str; 4] = [
        "org.freedesktop.appearance",
        "org.gnome.desktop.interface",
        "org.gnome.desktop.a11y.interface",
        "org.kde.kdeglobals.KDE",
    ];

    /// Takes what the desktop says of one key. Whether it is a key read
    /// here with a value of its type; any other leaves everything as it
    /// was.
    pub fn set(&mut self, namespace: &str, key: &str, value: &SettingValue) -> bool {
        let flag = || match value {
            SettingValue::Bool(b) => Some(*b),
            _ => None,
        };
        let one = || value.number().map(|n| n == 1.0);
        let (slot, read) = match (namespace, key) {
            ("org.freedesktop.appearance", "color-scheme") => {
                let scheme = match value.number() {
                    Some(1.0) => ColorScheme::Dark,
                    Some(0.0 | 2.0) => ColorScheme::Light,
                    _ => return false,
                };
                self.appearance_scheme = Some(scheme);
                return true;
            }
            ("org.freedesktop.appearance", "reduced-motion") => {
                (&mut self.appearance_motion, one())
            }
            ("org.freedesktop.appearance", "contrast") => (&mut self.appearance_contrast, one()),
            ("org.gnome.desktop.a11y.interface", "reduced-motion") => (
                &mut self.gnome_motion,
                match value {
                    SettingValue::Text(t) => Some(t == "reduce"),
                    _ => None,
                },
            ),
            ("org.gnome.desktop.a11y.interface", "high-contrast") => {
                (&mut self.gnome_contrast, flag())
            }
            ("org.gnome.desktop.interface", "enable-animations") => {
                (&mut self.gnome_no_animations, flag().map(|on| !on))
            }
            ("org.gnome.desktop.interface", "text-scaling-factor") => {
                let Some(scale) = value.number() else {
                    return false;
                };
                self.gnome_text_scale = Some(scale);
                return true;
            }
            ("org.kde.kdeglobals.KDE", "AnimationDurationFactor") => (
                &mut self.kde_no_animations,
                value.number().map(|f| f <= 0.0),
            ),
            _ => return false,
        };
        match read {
            Some(read) => {
                *slot = Some(read);
                true
            }
            None => false,
        }
    }

    /// What the keys say together. Less motion or more contrast in any
    /// key the desktop has is what the user asked for: a desktop keeps
    /// older keys beside newer ones, and turning one off is the user's
    /// wish whichever it was.
    pub fn prefs(&self) -> HostPrefs {
        let any = |keys: &[Option<bool>]| {
            let said: Vec<bool> = keys.iter().flatten().copied().collect();
            (!said.is_empty()).then(|| said.contains(&true))
        };
        HostPrefs {
            reduced_motion: any(&[
                self.appearance_motion,
                self.gnome_motion,
                self.gnome_no_animations,
                self.kde_no_animations,
            ]),
            contrast: any(&[self.appearance_contrast, self.gnome_contrast]).map(|high| {
                if high {
                    Contrast::High
                } else {
                    Contrast::NoPreference
                }
            }),
            font_scale: self.gnome_text_scale,
            color_scheme: self.appearance_scheme,
            ..HostPrefs::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_desktop_that_says_nothing_says_nothing() {
        assert_eq!(DesktopSettings::default().prefs(), HostPrefs::default());
    }

    #[test]
    fn less_motion_in_any_key_is_less_motion() {
        let mut d = DesktopSettings::default();
        assert!(d.set(
            "org.gnome.desktop.a11y.interface",
            "reduced-motion",
            &SettingValue::Text("no-preference".into())
        ));
        assert_eq!(d.prefs().reduced_motion, Some(false));
        d.set(
            "org.kde.kdeglobals.KDE",
            "AnimationDurationFactor",
            &SettingValue::Double(0.0),
        );
        assert_eq!(d.prefs().reduced_motion, Some(true));
    }

    #[test]
    fn a_value_of_another_type_is_not_taken() {
        let mut d = DesktopSettings::default();
        d.set(
            "org.gnome.desktop.interface",
            "enable-animations",
            &SettingValue::Bool(false),
        );
        assert!(!d.set(
            "org.gnome.desktop.interface",
            "enable-animations",
            &SettingValue::Text("yes".into())
        ));
        assert_eq!(d.prefs().reduced_motion, Some(true));
        assert!(!d.set("org.example", "contrast", &SettingValue::Uint(1)));
    }

    #[test]
    fn contrast_and_text_scale() {
        let mut d = DesktopSettings::default();
        d.set(
            "org.freedesktop.appearance",
            "contrast",
            &SettingValue::Uint(1),
        );
        d.set(
            "org.gnome.desktop.interface",
            "text-scaling-factor",
            &SettingValue::Double(1.25),
        );
        let p = d.prefs();
        assert_eq!(p.contrast, Some(Contrast::High));
        assert_eq!(p.font_scale, Some(1.25));
        assert_eq!(p.color_scheme, None);
    }
}
