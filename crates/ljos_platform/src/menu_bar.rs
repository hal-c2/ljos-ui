//! A menu bar the system shows, as macOS's is: one per application, at the
//! top of the screen, showing the active window's menus.
//!
//! The controls describe the bar ([`SystemBar`]) and hand it to the
//! installed [`MenuBarService`]; the service shows it and, when the user
//! chooses an item, the host gives the item's [`SystemItem::id`] back to
//! the controls. Where the system has no menu bar of its own (the
//! default, [`RecordedMenuBar`] unless told otherwise), a bar stays in
//! its window.

use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::Rc;

/// What shows the system's menu bar, or a stand-in. Called on the Engine
/// thread; `show` must not wait for the desktop.
pub trait MenuBarService {
    /// Whether the system has a menu bar of its own. A bar is in its
    /// window where it has not.
    fn native(&self) -> bool;
    /// Shows `bar` as the application's menu bar, in place of the one
    /// shown before.
    fn show(&self, bar: &SystemBar);
}

/// The application's menu bar: the items the application menu takes from
/// the application's menus, then those menus.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SystemBar {
    /// "About", "Preferences…" and "Quit" in the application menu, from
    /// the items whose role says so; `None` where no item has the role.
    pub about: Option<SystemItem>,
    pub preferences: Option<SystemItem>,
    pub quit: Option<SystemItem>,
    pub menus: Vec<SystemMenu>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SystemMenu {
    /// Without the `&` of a mnemonic.
    pub title: String,
    pub role: MenuRole,
    pub entries: Vec<SystemEntry>,
}

/// What the system makes of one of the bar's menus.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MenuRole {
    /// Told by its title, as Qt's `TextHeuristicRole` tells an item's:
    /// "Window" lists the application's windows.
    #[default]
    TextHeuristic,
    /// The application's items alone.
    None,
    /// The system lists the application's windows in it, whatever its
    /// title says (macOS's Window menu).
    Window,
}

impl SystemMenu {
    /// Whether the system may list the application's windows in this
    /// menu: which one does is [`SystemBar::windows_menu`]'s choice.
    pub fn lists_windows(&self) -> bool {
        match self.role {
            MenuRole::Window => true,
            MenuRole::TextHeuristic => self.title == "Window",
            MenuRole::None => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SystemEntry {
    Item(SystemItem),
    Separator,
    Menu(SystemMenu),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SystemItem {
    /// What the host gives back when the item is chosen.
    pub id: u64,
    /// Without the `&` of a mnemonic.
    pub text: String,
    pub enabled: bool,
    pub checkable: bool,
    pub checked: bool,
    pub key: Option<KeyEquivalent>,
}

/// A shortcut of one chord, as a menu shows it and takes it. The
/// modifiers are the Mac's: Ljós's `Ctrl` is Command, `Meta` is Control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyEquivalent {
    /// The key, as Qt numbers keys (`Qt.Key_N`).
    pub key: i32,
    pub command: bool,
    pub shift: bool,
    pub option: bool,
    pub control: bool,
}

impl SystemBar {
    /// The menu titled `path[0]`, then its sub-menus by title, then the
    /// item by text; `None` if there is no such item.
    pub fn item(&self, path: &[&str]) -> Option<&SystemItem> {
        let (last, menus) = path.split_last()?;
        let (first, subs) = menus.split_first()?;
        let mut menu = self.menus.iter().find(|m| m.title == *first)?;
        for title in subs {
            menu = menu.entries.iter().find_map(|e| match e {
                SystemEntry::Menu(m) if m.title == *title => Some(m),
                _ => None,
            })?;
        }
        menu.entries.iter().find_map(|e| match e {
            SystemEntry::Item(i) if i.text == *last => Some(i),
            _ => None,
        })
    }

    /// The index of the menu the system lists the application's windows
    /// in: the first with [`MenuRole::Window`], or else the first whose
    /// title says so; `None` if none does.
    pub fn windows_menu(&self) -> Option<usize> {
        self.menus
            .iter()
            .position(|m| m.role == MenuRole::Window)
            .or_else(|| self.menus.iter().position(SystemMenu::lists_windows))
    }

    /// An item of the application menu by its text.
    pub fn application_item(&self, text: &str) -> Option<&SystemItem> {
        [&self.about, &self.preferences, &self.quit]
            .into_iter()
            .flatten()
            .find(|i| i.text == text)
    }
}

impl fmt::Display for KeyEquivalent {
    /// As macOS shows it: ⌃⌥⇧⌘ before the key.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (on, sign) in [
            (self.control, '⌃'),
            (self.option, '⌥'),
            (self.shift, '⇧'),
            (self.command, '⌘'),
        ] {
            if on {
                write!(f, "{sign}")?;
            }
        }
        match u32::try_from(self.key).ok().and_then(char::from_u32) {
            Some(c) if self.key < 0x0100_0000 => write!(f, "{c}"),
            _ => write!(f, "0x{:x}", self.key),
        }
    }
}

impl fmt::Display for SystemItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.checkable && self.checked {
            write!(f, "✓ ")?;
        }
        write!(f, "{}", self.text)?;
        if let Some(key) = self.key {
            write!(f, "  {key}")?;
        }
        if !self.enabled {
            write!(f, "  disabled")?;
        }
        Ok(())
    }
}

fn write_menu(f: &mut fmt::Formatter<'_>, menu: &SystemMenu, depth: usize) -> fmt::Result {
    writeln!(f, "{:w$}{}", "", menu.title, w = depth * 2)?;
    for entry in &menu.entries {
        let pad = (depth + 1) * 2;
        match entry {
            SystemEntry::Item(item) => writeln!(f, "{:pad$}{item}", "")?,
            SystemEntry::Separator => writeln!(f, "{:pad$}---", "")?,
            SystemEntry::Menu(sub) => write_menu(f, sub, depth + 1)?,
        }
    }
    Ok(())
}

impl fmt::Display for SystemBar {
    /// One line a menu, item or separator (`---`), indented two spaces a
    /// level; the application menu first, as "Application", when it has
    /// items of the application's.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let app: Vec<_> = [&self.about, &self.preferences, &self.quit]
            .into_iter()
            .flatten()
            .collect();
        if !app.is_empty() {
            writeln!(f, "Application")?;
            for item in app {
                writeln!(f, "  {item}")?;
            }
        }
        for menu in &self.menus {
            write_menu(f, menu, 0)?;
        }
        Ok(())
    }
}

/// Shows nothing and keeps the last bar it was given. Says the system has
/// a menu bar of its own only when made with [`RecordedMenuBar::native`].
/// Its clones share one record.
#[derive(Clone, Debug, Default)]
pub struct RecordedMenuBar {
    native: bool,
    shown: Rc<RefCell<Option<SystemBar>>>,
    shows: Rc<Cell<usize>>,
}

impl RecordedMenuBar {
    /// A stand-in for a system with a menu bar of its own.
    pub fn native() -> Self {
        Self {
            native: true,
            ..Self::default()
        }
    }

    /// The bar shown last; `None` if none was.
    pub fn shown(&self) -> Option<SystemBar> {
        self.shown.borrow().clone()
    }

    /// How many times a bar was shown.
    pub fn shows(&self) -> usize {
        self.shows.get()
    }
}

impl MenuBarService for RecordedMenuBar {
    fn native(&self) -> bool {
        self.native
    }

    fn show(&self, bar: &SystemBar) {
        *self.shown.borrow_mut() = Some(bar.clone());
        self.shows.set(self.shows.get() + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: u64, text: &str) -> SystemItem {
        SystemItem {
            id,
            text: text.into(),
            enabled: true,
            checkable: false,
            checked: false,
            key: None,
        }
    }

    #[test]
    fn a_bar_is_written_one_line_an_entry() {
        let mut new = item(1, "New");
        new.key = Some(KeyEquivalent {
            key: 'N' as i32,
            command: true,
            shift: true,
            option: false,
            control: true,
        });
        let mut wrap = item(3, "Wrap");
        wrap.checkable = true;
        wrap.checked = true;
        wrap.enabled = false;
        let bar = SystemBar {
            quit: Some(item(9, "Quit")),
            menus: vec![SystemMenu {
                title: "File".into(),
                role: MenuRole::TextHeuristic,
                entries: vec![
                    SystemEntry::Item(new),
                    SystemEntry::Separator,
                    SystemEntry::Menu(SystemMenu {
                        title: "Recent".into(),
                        role: MenuRole::TextHeuristic,
                        entries: vec![SystemEntry::Item(item(2, "a"))],
                    }),
                    SystemEntry::Item(wrap),
                ],
            }],
            ..SystemBar::default()
        };
        assert_eq!(
            bar.to_string(),
            "Application\n  Quit\nFile\n  New  ⌃⇧⌘N\n  ---\n  Recent\n    a\n  ✓ Wrap  disabled\n"
        );
        assert_eq!(bar.item(&["File", "Recent", "a"]).map(|i| i.id), Some(2));
        assert_eq!(bar.item(&["File", "a"]), None);
        assert_eq!(bar.application_item("Quit").map(|i| i.id), Some(9));
    }

    fn menu(title: &str, role: MenuRole) -> SystemMenu {
        SystemMenu {
            title: title.into(),
            role,
            entries: Vec::new(),
        }
    }

    fn bar(menus: Vec<SystemMenu>) -> SystemBar {
        SystemBar {
            menus,
            ..SystemBar::default()
        }
    }

    #[test]
    fn a_menu_with_the_window_role_lists_the_windows_whatever_its_title() {
        let b = bar(vec![
            menu("Datei", MenuRole::TextHeuristic),
            menu("Fenster", MenuRole::Window),
            menu("Window", MenuRole::Window),
        ]);
        assert_eq!(b.windows_menu(), Some(1));
    }

    #[test]
    fn without_the_role_a_menu_titled_window_lists_them() {
        let b = bar(vec![
            menu("File", MenuRole::TextHeuristic),
            menu("Window", MenuRole::TextHeuristic),
        ]);
        assert_eq!(b.windows_menu(), Some(1));
        // A role said is taken over a title, wherever the menus are.
        let b = bar(vec![
            menu("Window", MenuRole::TextHeuristic),
            menu("Fenster", MenuRole::Window),
        ]);
        assert_eq!(b.windows_menu(), Some(1));
    }

    #[test]
    fn no_menu_lists_the_windows_without_the_role_or_the_title() {
        let b = bar(vec![
            menu("Fenster", MenuRole::TextHeuristic),
            menu("Window", MenuRole::None),
        ]);
        assert_eq!(b.windows_menu(), None);
        assert_eq!(SystemBar::default().windows_menu(), None);
    }
}
