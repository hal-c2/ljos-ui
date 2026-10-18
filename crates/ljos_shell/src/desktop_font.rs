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
//! It is read as the application starts, on a thread of its own: asking
//! the bus may wait, and the drivers start meanwhile. The shell waits for
//! the answer a short while before the root is built. On Linux the font
//! the desktop's portal says after that ([`followed`], heard by the
//! settings watch, `crate::settings`) becomes the application font of the
//! running application (`ljos_controls::set_application_font`): a portal
//! that answered late, and each font the user picks.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use ljos_a11y::DesktopFont;
use ljos_controls::desktop_font::{from_pango, from_qt_string};
use ljos_engine::Font;

/// The source [`Reading::start`] chose, which a later change follows: 0
/// none, 1 GNOME's, 2 KDE's.
static FOLLOWED: AtomicU8 = AtomicU8::new(0);

/// As if the application had started on a GNOME desktop.
#[cfg(test)]
pub(crate) fn follow_gnome() {
    FOLLOWED.store(1, Ordering::Relaxed);
}

/// The font the application takes from what the desktop names, when it is
/// the font of the source the application started with; `None` for any
/// other and for a name that is no font.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn followed(font: &DesktopFont) -> Option<Font> {
    match (FOLLOWED.load(Ordering::Relaxed), font) {
        (1, DesktopFont::Pango(name)) => from_pango(name),
        (2, DesktopFont::Qt(s)) => from_qt_string(s),
        _ => None,
    }
}

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
        let tag = match source {
            Source::Gnome => 1,
            Source::Kde => 2,
        };
        FOLLOWED.store(tag, Ordering::Relaxed);
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

#[cfg(test)]
mod tests {
    use super::*;

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
