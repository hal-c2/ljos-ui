//! The desktop's font, which becomes the application font
//! ([`ljos_engine::Font::set_application_font`]) as it does in Qt.
//!
//! Qt takes it from its platform theme, and what that is depends on the
//! desktop (`XDG_CURRENT_DESKTOP`). Measured on Qt 6.11 (qml6, from
//! outside):
//!
//! - GNOME and the desktops built on its settings (Unity, Cinnamon, MATE,
//!   Xfce, Budgie, Pantheon): the interface font GNOME's settings name
//!   (`org.gnome.desktop.interface font-name`, a Pango name such as
//!   `Adwaita Sans 11`). Qt keeps its family and its whole points and
//!   drops the style words: `Noto Serif Bold Italic 13` is Noto Serif at
//!   13 pt, weight 400. A name without a size is 10 pt. Here it is read
//!   from the desktop portal (`org.freedesktop.portal.Settings`), and
//!   where no portal answers, from GTK's `settings.ini`.
//! - KDE, in a KDE session (`KDE_SESSION_VERSION`): `font` in the
//!   `[General]` group of `kdeglobals`, a `QFont::toString` string.
//! - Anything else: none, and Qt's generic Unix font stays (Sans Serif,
//!   9 pt). So does an off-screen run, which has no desktop.
//!
//! It is read once, as the application starts, on a thread of its own:
//! asking the bus may wait, and the drivers start meanwhile. The shell
//! waits for the answer a short while before the root is built. A font the
//! desktop changes later is not followed; an application that hears of
//! one can hand it on with `ljos_controls::set_application_font`.

use std::sync::mpsc;
use std::time::Duration;

use ljos_engine::{Atom, Font};

/// How long the root waits for the desktop to answer. A portal that is
/// started on demand can take longer; the application then starts with
/// Qt's generic font.
const PATIENCE: Duration = Duration::from_millis(250);

/// The desktop's font, being read.
pub(crate) struct Reading(Option<mpsc::Receiver<Option<Font>>>);

impl Reading {
    /// Starts reading the font of the desktop this process runs on.
    pub(crate) fn start() -> Self {
        let Some(source) = Source::of_desktop(&Env::real()) else {
            return Self(None);
        };
        let (tx, rx) = mpsc::sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("ljos-desktop-font".into())
            .spawn(move || {
                let _ = tx.send(source.read(&Env::real()));
            });
        Self(spawned.ok().map(|_| rx))
    }

    /// Makes what the desktop answered the application font.
    pub(crate) fn apply(self) {
        let font = self
            .0
            .and_then(|rx| rx.recv_timeout(PATIENCE).ok().flatten());
        if let Some(font) = &font {
            Font::set_application_font(Some(font));
        }
    }
}

/// The environment a source is chosen and read in.
#[derive(Clone, Debug, Default)]
struct Env {
    desktop: Option<String>,
    kde_session: Option<String>,
    config_home: Option<String>,
    config_dirs: Option<String>,
    home: Option<String>,
}

impl Env {
    fn real() -> Self {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        Env {
            desktop: var("XDG_CURRENT_DESKTOP"),
            kde_session: var("KDE_SESSION_VERSION"),
            config_home: var("XDG_CONFIG_HOME"),
            config_dirs: var("XDG_CONFIG_DIRS"),
            home: var("HOME"),
        }
    }

    /// The configuration directories, the user's first (XDG Base
    /// Directory).
    fn config_dirs(&self) -> Vec<std::path::PathBuf> {
        let mut dirs = Vec::new();
        match (&self.config_home, &self.home) {
            (Some(c), _) => dirs.push(c.into()),
            (None, Some(h)) => dirs.push(std::path::Path::new(h).join(".config")),
            (None, None) => {}
        }
        let system = self.config_dirs.as_deref().unwrap_or("/etc/xdg");
        dirs.extend(system.split(':').filter(|d| !d.is_empty()).map(Into::into));
        dirs
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    Gnome,
    Kde,
}

/// Desktops whose interface font Qt takes from GNOME's settings.
const GNOME_LIKE: [&str; 9] = [
    "GNOME",
    "UNITY",
    "X-CINNAMON",
    "CINNAMON",
    "MATE",
    "XFCE",
    "BUDGIE",
    "PANTHEON",
    "GNOME-FLASHBACK",
];

impl Source {
    fn of_desktop(env: &Env) -> Option<Self> {
        let desktop = env.desktop.as_deref()?.to_ascii_uppercase();
        let names: Vec<&str> = desktop.split(':').collect();
        if names.contains(&"KDE") {
            return env.kde_session.is_some().then_some(Source::Kde);
        }
        names
            .iter()
            .any(|n| GNOME_LIKE.contains(n))
            .then_some(Source::Gnome)
    }

    fn read(self, env: &Env) -> Option<Font> {
        match self {
            Source::Gnome => portal_font_name()
                .or_else(|| config_value(env, "gtk-3.0/settings.ini", "Settings", "gtk-font-name"))
                .and_then(|name| from_pango(&name)),
            Source::Kde => {
                config_value(env, "kdeglobals", "General", "font").and_then(|s| from_qt_string(&s))
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn portal_font_name() -> Option<String> {
    use zbus::zvariant::{OwnedValue, Value};
    let bus = zbus::blocking::Connection::session().ok()?;
    let ask = |method: &str| -> Option<OwnedValue> {
        bus.call_method(
            Some("org.freedesktop.portal.Desktop"),
            "/org/freedesktop/portal/desktop",
            Some("org.freedesktop.portal.Settings"),
            method,
            &("org.gnome.desktop.interface", "font-name"),
        )
        .ok()?
        .body()
        .deserialize::<OwnedValue>()
        .ok()
    };
    // `ReadOne` is version 2 of the interface; `Read`, which older portals
    // have alone, wraps the value once more.
    let value = ask("ReadOne").or_else(|| ask("Read"))?;
    let mut v: &Value<'_> = &value;
    while let Value::Value(inner) = v {
        v = inner;
    }
    match v {
        Value::Str(s) => Some(s.as_str().to_owned()),
        _ => None,
    }
}

#[cfg(not(target_os = "linux"))]
fn portal_font_name() -> Option<String> {
    None
}

/// `key` in `[group]` of the first file named `name` in the configuration
/// directories that has it.
fn config_value(env: &Env, name: &str, group: &str, key: &str) -> Option<String> {
    env.config_dirs().into_iter().find_map(|dir| {
        let text = std::fs::read_to_string(dir.join(name)).ok()?;
        ini_value(&text, group, key)
    })
}

/// `key` in `[group]` of an INI-style file (a desktop entry, `kdeglobals`,
/// GTK's `settings.ini`).
fn ini_value(text: &str, group: &str, key: &str) -> Option<String> {
    let mut in_group = false;
    for line in text.lines().map(str::trim) {
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            in_group = name == group;
        } else if in_group
            && let Some((k, v)) = line.split_once('=')
            // KDE marks a value an administrator locked: `font[$i]=`.
            && k.trim().split('[').next() == Some(key)
        {
            return Some(v.trim().trim_matches('"').to_owned());
        }
    }
    None
}

/// The words of a Pango font name that say a style, not the family.
const PANGO_STYLE_WORDS: [&str; 44] = [
    "normal",
    "roman",
    "oblique",
    "italic",
    "small-caps",
    "all-small-caps",
    "petite-caps",
    "all-petite-caps",
    "unicase",
    "title-caps",
    "ultra-condensed",
    "extra-condensed",
    "condensed",
    "semi-condensed",
    "semi-expanded",
    "expanded",
    "extra-expanded",
    "ultra-expanded",
    "thin",
    "ultra-light",
    "extra-light",
    "light",
    "semi-light",
    "demi-light",
    "book",
    "regular",
    "medium",
    "semi-bold",
    "demi-bold",
    "bold",
    "ultra-bold",
    "extra-bold",
    "heavy",
    "black",
    "ultra-black",
    "extra-black",
    "ultra-heavy",
    "not-rotated",
    "south",
    "upside-down",
    "north",
    "rotated-left",
    "east",
    "rotated-right",
];

/// A Pango font name (`Family [style words] [size]`) as Qt makes it the
/// application font: the family at the size's whole points, or 10 pt.
fn from_pango(name: &str) -> Option<Font> {
    // Variations (`@wght=500`) and families after the first are not the
    // family.
    let name = name.split('@').next()?;
    let mut words: Vec<&str> = name.split_whitespace().collect();
    let mut points = 10.0;
    if let Some(&last) = words.last()
        && let Ok(size) = last.trim_end_matches("px").parse::<f64>()
    {
        words.pop();
        if !last.ends_with("px") && size >= 1.0 {
            points = size.trunc();
        }
    }
    while words.len() > 1
        && words
            .last()
            .is_some_and(|w| PANGO_STYLE_WORDS.contains(&w.to_ascii_lowercase().as_str()))
    {
        words.pop();
    }
    let family = words.join(" ");
    let family = family.split(',').next()?.trim();
    (!family.is_empty()).then(|| Font {
        family: Atom::from(family),
        point_size: points,
        pixel_size: -1,
        ..Font::fallback()
    })
}

/// Weights of Qt 5's scale (0 to 99) and what they are on Qt 6's (CSS).
const QT5_WEIGHTS: [(i32, i32); 9] = [
    (0, 100),
    (12, 200),
    (25, 300),
    (50, 400),
    (57, 500),
    (63, 600),
    (75, 700),
    (81, 800),
    (87, 900),
];

/// A font as `QFont::toString` writes it (`Family,pt,px,hint,weight,
/// style,underline,strikeout,…`), Qt 5's ten or eleven fields or Qt 6's
/// sixteen or more.
fn from_qt_string(s: &str) -> Option<Font> {
    let fields: Vec<&str> = s.split(',').map(str::trim).collect();
    if fields.len() < 10 {
        return None;
    }
    let family = fields[0];
    let num = |i: usize| fields.get(i).and_then(|f| f.parse::<f64>().ok());
    let point_size = num(1)?;
    let pixel_size = num(2)? as i32;
    let qt6 = fields.len() >= 16;
    let weight = num(4)? as i32;
    let weight = if qt6 {
        weight.clamp(1, 1000)
    } else {
        // The nearest weight Qt 5 names.
        QT5_WEIGHTS
            .iter()
            .min_by_key(|(w, _)| (w - weight).abs())
            .map_or(400, |(_, css)| *css)
    };
    let style_name = fields.get(if qt6 { 16 } else { 10 }).copied().unwrap_or("");
    let (point_size, pixel_size) = if pixel_size > 0 {
        (-1.0, pixel_size)
    } else if point_size > 0.0 {
        (point_size, -1)
    } else {
        return None;
    };
    (!family.is_empty()).then(|| Font {
        family: Atom::from(family),
        point_size,
        pixel_size,
        weight,
        italic: num(5).is_some_and(|v| v != 0.0),
        style_name: Atom::from(style_name),
        ..Font::fallback()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(f: Option<Font>) -> Option<(String, f64, i32, i32, bool, String)> {
        f.map(|f| {
            (
                f.family.as_str().to_owned(),
                f.point_size,
                f.pixel_size,
                f.weight,
                f.italic,
                f.style_name.as_str().to_owned(),
            )
        })
    }

    fn env(desktop: &str, kde: bool) -> Env {
        Env {
            desktop: Some(desktop.into()),
            kde_session: kde.then(|| "6".into()),
            ..Env::default()
        }
    }

    #[test]
    fn the_desktop_says_where_its_font_is() {
        for d in [
            "GNOME",
            "ubuntu:GNOME",
            "Unity",
            "X-Cinnamon",
            "MATE",
            "XFCE",
            "Budgie:GNOME",
            "Pantheon",
        ] {
            assert_eq!(
                Source::of_desktop(&env(d, false)),
                Some(Source::Gnome),
                "{d}"
            );
        }
        assert_eq!(Source::of_desktop(&env("KDE", true)), Some(Source::Kde));
        // Qt takes KDE's font in a KDE session only.
        assert_eq!(Source::of_desktop(&env("KDE", false)), None);
        for d in ["LXQt", "Hyprland", "sway"] {
            assert_eq!(Source::of_desktop(&env(d, false)), None, "{d}");
        }
        assert_eq!(Source::of_desktop(&Env::default()), None);
    }

    #[test]
    fn a_pango_name_is_its_family_at_whole_points() {
        let f = |s: &str| summary(from_pango(s)).map(|(f, p, ..)| (f, p));
        assert_eq!(f("Adwaita Sans 11"), Some(("Adwaita Sans".into(), 11.0)));
        // Measured on Qt 6.11: the style words go, the size is cut.
        assert_eq!(
            f("Noto Serif Bold Italic 13"),
            Some(("Noto Serif".into(), 13.0))
        );
        assert_eq!(f("Noto Sans 10.5"), Some(("Noto Sans".into(), 10.0)));
        assert_eq!(f("Cantarell"), Some(("Cantarell".into(), 10.0)));
        assert_eq!(
            f("DejaVu Sans Condensed Semi-Bold 12"),
            Some(("DejaVu Sans".into(), 12.0))
        );
        assert_eq!(f("Noto Sans, 12"), Some(("Noto Sans".into(), 12.0)));
        assert_eq!(f(""), None);
        assert_eq!(summary(from_pango("Bold 12")).map(|s| s.3), Some(400));
    }

    #[test]
    fn a_kde_font_is_a_qfont_string() {
        // What Qt 6.11 made of each, read back.
        assert_eq!(
            summary(from_qt_string("Noto Sans,10,-1,5,50,0,0,0,0,0")),
            Some(("Noto Sans".into(), 10.0, -1, 400, false, String::new()))
        );
        assert_eq!(
            summary(from_qt_string("Noto Sans,10,-1,5,75,1,0,0,0,0")),
            Some(("Noto Sans".into(), 10.0, -1, 700, true, String::new()))
        );
        assert_eq!(
            summary(from_qt_string("Noto Serif,13,-1,5,400,0,0,0,0,0")),
            Some(("Noto Serif".into(), 13.0, -1, 900, false, String::new()))
        );
        assert_eq!(
            summary(from_qt_string(
                "Hack,11,-1,5,700,0,0,0,0,0,0,0,0,0,0,1,Bold"
            )),
            Some(("Hack".into(), 11.0, -1, 700, false, "Bold".into()))
        );
        assert_eq!(
            summary(from_qt_string("Noto Sans,10.5,-1,5,50,0,0,0,0,0")).map(|s| s.1),
            Some(10.5)
        );
        assert_eq!(summary(from_qt_string("Noto Sans,10")), None);
    }

    #[test]
    fn a_value_is_found_in_its_group() {
        let text = "[Icons]\nfont=no\n[General]\nname=x\nfont[$i]=Hack,11,-1,5,50,0,0,0,0,0\n";
        assert_eq!(
            ini_value(text, "General", "font").as_deref(),
            Some("Hack,11,-1,5,50,0,0,0,0,0")
        );
        assert_eq!(
            ini_value(
                "[Settings]\ngtk-font-name = \"Ubuntu 11\"\n",
                "Settings",
                "gtk-font-name"
            )
            .as_deref(),
            Some("Ubuntu 11")
        );
        assert_eq!(ini_value(text, "General", "missing"), None);
    }

    #[test]
    fn the_users_configuration_comes_before_the_systems() {
        let e = Env {
            home: Some("/home/u".into()),
            config_dirs: Some("/etc/a:/etc/b".into()),
            ..Env::default()
        };
        let dirs: Vec<String> = e
            .config_dirs()
            .iter()
            .map(|d| d.display().to_string())
            .collect();
        assert_eq!(dirs, ["/home/u/.config", "/etc/a", "/etc/b"]);
    }
}
