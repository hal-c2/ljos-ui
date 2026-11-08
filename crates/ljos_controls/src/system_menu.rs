//! A `MenuBar` in the system's menu bar, where the system has one (macOS).
//!
//! Since Qt 6.8 a MenuBar is the system's menu bar on macOS, with nothing
//! asking for it; the bar keeps `visible` and takes no room in its window
//! (`height` 0). Each bar is described by an effect, so whatever it reads
//! (the menus, their titles and items, each item's text, `enabled`,
//! `checked` and its action's shortcut, and whether its window is active)
//! describes it again when it changes. The system shows the bar of the
//! active window ([`ljos_platform::MenuBarService::show`]), and is told
//! only when what it shows changes. The host gives a chosen item back
//! through [`choose`], and the application menu's Quit through [`quit`].
//!
//! The application menu takes "About", "Preferences…" and "Quit" from
//! the items directly in the bar's menus, by their text as Qt's
//! `TextHeuristicRole` does (doc.qt.io/qt-6/qmenubar.html): text starting
//! with "about"; "config", "options", "setup", "settings" or
//! "preferences"; "quit" or "exit". The first item of each role takes the
//! place and the others are not shown, and a menu left with nothing is
//! not shown, as on Qt 6.11.
//!
//! `Qt.labs.platform`'s `MenuBar` ([`crate::platform_menu`]) is shown the
//! same way, for the window it belongs to; its items say their place with
//! `role`, the text heuristic being the default, and what is not
//! `visible` is not shown.
//!
//! The system lists the application's windows in the menu whose
//! `SystemMenu.role` (module `Ljos`) is `SystemMenu.WindowRole`, or else
//! in the one titled "Window" ([`SystemBar::windows_menu`]); such a menu
//! is shown even with nothing of its own in it. Qt has no such role: a
//! menu whose title is translated says it.

use std::cell::RefCell;

use ljos_engine::{
    Engine, EngineError, MetaRef, ObjectId, OverrideContent, QmlObject, QmlType, Value,
};
use ljos_platform::{
    KeyEquivalent, MenuRole, Platform, SystemBar, SystemEntry, SystemItem, SystemMenu,
};
use ljos_quick::Item;
use ljos_quick::Modifiers;
use ljos_quick::keys::{KeySequence, platform, standard_key_sequences};
use slotmap::{Key, KeyData};

use crate::action::Action;
use crate::button::AbstractButton;
use crate::menu::{Menu, MenuItem, MenuSeparator, action_of, items, sub_menu_of};
use crate::menu_bar::MenuBar;
use crate::platform_menu::{self, PlatformMenu, PlatformMenuBar, PlatformMenuItem};
use crate::util::{check, flag, is_a, object};
use crate::window::Window;
use ljos_quick::focus::WindowAttached;

/// `SystemMenu` (module `Ljos`): only attached, to a `Menu` of a
/// `MenuBar`, saying what the system's menu bar makes of it.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "Ljos",
    name = "SystemMenu",
    attached = SystemMenuAttached,
    uncreatable,
    enums = system_menu_enums
)]
pub struct SystemMenuAttacher {}

/// What `SystemMenu.role: …` attaches to a menu.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "SystemMenuAttached", enums = system_menu_enums)]
pub struct SystemMenuAttached {
    /// `SystemMenu.TextHeuristicRole` (the title says), `NoRole` or
    /// `WindowRole` (the system lists the windows in it).
    #[qml(notify, enumeration = "Role")]
    pub role: i32,
}

const TEXT_HEURISTIC_ROLE: i32 = 0;
const NO_ROLE: i32 = 1;
const WINDOW_ROLE: i32 = 2;

fn system_menu_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "Role",
        vec![
            ("TextHeuristicRole", i64::from(TEXT_HEURISTIC_ROLE)),
            ("NoRole", i64::from(NO_ROLE)),
            ("WindowRole", i64::from(WINDOW_ROLE)),
        ],
    )]
}

/// Registers `SystemMenu` in `Ljos`.
pub(crate) fn register(engine: &Engine) {
    engine.register_module(ljos_engine::module!("Ljos" 1.0 => [SystemMenuAttacher]));
}

/// The role `SystemMenu.role` gives `menu` (tracked).
fn menu_role(e: &Engine, menu: ObjectId) -> MenuRole {
    let attacher = MetaRef::Static(SystemMenuAttacher::static_meta());
    let Some(att) = e.existing_attached_object(menu, &attacher) else {
        return MenuRole::TextHeuristic;
    };
    match e.read(att, SystemMenuAttached::ROLE.index).to_number() as i32 {
        NO_ROLE => MenuRole::None,
        WINDOW_ROLE => MenuRole::Window,
        _ => MenuRole::TextHeuristic,
    }
}

#[derive(Default)]
struct Bars {
    /// Each bar shown by the system, with what it is and whether its
    /// window is active, as its effect last read them.
    bars: RefCell<Vec<(ObjectId, SystemBar, bool)>>,
    shown: RefCell<Option<(ObjectId, SystemBar)>>,
    /// Whether destroyed bars are being let go of.
    listening: std::cell::Cell<bool>,
}

/// Gives `bar` to the system's menu bar, if the system has one: done when
/// the bar is complete. Elsewhere the bar stays in its window.
pub(crate) fn adopt(e: &Engine, bar: ObjectId) -> Result<(), EngineError> {
    if !Platform::of(e).menu_bar().native() {
        return Ok(());
    }
    e.push_cover(
        bar,
        Item::HEIGHT.index,
        OverrideContent::Value(Value::Real(0.0)),
    )?;
    // Its items are still there, under a bar of no height.
    e.push_cover(
        bar,
        Item::CLIP.index,
        OverrideContent::Value(Value::Bool(true)),
    )?;
    let att = e.attached_object(bar, &MetaRef::Static(Window::static_meta()))?;
    track(e, bar, move |e| {
        // The window's own `active`, which follows activation; the
        // attached one of an item in a window's bar did not.
        let active = object(e, att, WindowAttached::WINDOW.index)
            .is_some_and(|win| flag(e, win, Window::ACTIVE.index));
        (describe(e, bar), active)
    })
}

/// Gives a `Qt.labs.platform` `MenuBar` to the system's menu bar, if the
/// system has one: done when the bar is complete. Elsewhere it makes
/// nothing.
pub(crate) fn adopt_platform(e: &Engine, bar: ObjectId) -> Result<(), EngineError> {
    if !Platform::of(e).menu_bar().native() {
        return Ok(());
    }
    // Unless `window` names one, the bar's window is the first of its
    // parents that is a window, as Qt finds it; they do not change once
    // the bar is complete.
    let declared_in =
        std::iter::successors(e.owner(bar), |o| e.owner(*o)).find(|o| is_a::<Window>(e, *o));
    track(e, bar, move |e| {
        let window = object(e, bar, PlatformMenuBar::WINDOW.index).or(declared_in);
        let active = window.is_some_and(|win| flag(e, win, Window::ACTIVE.index));
        (describe_platform(e, bar), active)
    })
}

/// Follows what `read` says of `bar` (what it is, and whether its window
/// is active), and shows the active window's bar.
fn track(
    e: &Engine,
    bar: ObjectId,
    read: impl Fn(&Engine) -> (SystemBar, bool) + 'static,
) -> Result<(), EngineError> {
    let bars = e.extension::<Bars>();
    if !bars.listening.replace(true) {
        // A bar that goes (its window unloaded) is shown no more, and its
        // items are not offered: nothing of it runs again to say so.
        e.on_destroyed(|e, gone| {
            let bars = e.extension::<Bars>();
            let before = bars.bars.borrow().len();
            bars.bars.borrow_mut().retain(|(b, ..)| !gone.contains(b));
            if bars.bars.borrow().len() != before {
                publish(e);
            }
        });
    }
    e.effect_in(bar, move |e| {
        let (described, active) = read(e);
        e.untrack(|| {
            let bars = e.extension::<Bars>();
            {
                let mut all = bars.bars.borrow_mut();
                all.retain(|(b, ..)| e.is_alive(*b));
                match all.iter_mut().find(|(b, ..)| *b == bar) {
                    Some(slot) => *slot = (bar, described, active),
                    None => all.push((bar, described, active)),
                }
            }
            publish(e);
        });
    })?;
    Ok(())
}

/// Shows the active window's bar; with no window active, the bar shown
/// before, or else the first.
fn publish(e: &Engine) {
    let bars = e.extension::<Bars>();
    let pick = {
        let all = bars.bars.borrow();
        let before = bars.shown.borrow().as_ref().map(|(b, _)| *b);
        all.iter()
            .find(|(_, _, active)| *active)
            .or_else(|| all.iter().find(|(b, ..)| Some(*b) == before))
            .or_else(|| all.first())
            .map(|(b, bar, _)| (*b, bar.clone()))
    };
    let Some(pick) = pick else {
        // No bar is left: the application menu alone.
        if bars.shown.borrow_mut().take().is_some() {
            Platform::of(e).menu_bar().show(&SystemBar::default());
        }
        return;
    };
    if bars.shown.borrow().as_ref() == Some(&pick) {
        return;
    }
    Platform::of(e).menu_bar().show(&pick.1);
    *bars.shown.borrow_mut() = Some(pick);
}

/// The user chose the item `id` in the system's menu bar: it is triggered
/// as a click on it would. Returns whether there was such an item.
pub fn choose(e: &Engine, id: u64) -> bool {
    let item = ObjectId::from(KeyData::from_ffi(id));
    if !e.is_alive(item) {
        return false;
    }
    if is_a::<PlatformMenuItem>(e, item) {
        check(e, "MenuItem", platform_menu::trigger(e, item));
        return true;
    }
    if !is_a::<AbstractButton>(e, item) {
        return false;
    }
    let _ = e.invoke_by_name(item, "click", &[]);
    true
}

/// The user chose Quit in the application menu: the item that quits is
/// triggered, or with none that can be chosen every window is asked to
/// close as its user would close it, and the application quits if all of
/// them do, as `QCoreApplication::quit()` does: a window that will not
/// close stops it.
pub fn quit(e: &Engine) {
    let quit_item = e
        .extension::<Bars>()
        .shown
        .borrow()
        .as_ref()
        .and_then(|(_, bar)| quit_target(bar));
    if quit_item.is_some_and(|id| choose(e, id)) {
        return;
    }
    let sys = crate::window::WindowSystem::of(e);
    for win in sys.windows(e) {
        // The first window that will not close stops the quit, and the
        // rest are not asked.
        if flag(e, win, Window::VISIBLE.index) && !sys.request_close(e, win) {
            return;
        }
    }
    e.request_quit();
}

/// The item Quit in the application menu triggers: the bar's item that
/// quits, while it can be chosen. One that is disabled, or in a disabled
/// menu, leaves Quit to do what it does with none, as Qt's does.
fn quit_target(bar: &SystemBar) -> Option<u64> {
    bar.quit.as_ref().filter(|q| q.enabled).map(|q| q.id)
}

#[derive(Clone, Copy, PartialEq)]
enum Role {
    None,
    About,
    Preferences,
    Quit,
}

/// The role Qt's text heuristic gives an item called `text`.
fn role_of(text: &str) -> Role {
    let t = text
        .trim()
        .trim_end_matches(['.', '…'])
        .trim()
        .to_lowercase();
    if t.starts_with("about") {
        Role::About
    } else if matches!(
        t.as_str(),
        "config" | "options" | "setup" | "settings" | "preferences"
    ) {
        Role::Preferences
    } else if matches!(t.as_str(), "quit" | "exit") {
        Role::Quit
    } else {
        Role::None
    }
}

/// `text` as a menu shows it: a `&` marks the next letter as the
/// mnemonic, and `&&` is one `&`.
fn without_mnemonic(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '&' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// The bar as the system shows it (tracked).
fn describe(e: &Engine, bar: ObjectId) -> SystemBar {
    let mut out = SystemBar::default();
    let menus = match e.read(bar, MenuBar::MENUS.index) {
        Value::List(list) => list
            .iter()
            .filter_map(|v| match v {
                Value::Object(m) => Some(*m),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    for menu in menus {
        if let Some(m) = describe_menu(e, menu, Some(&mut out)) {
            out.menus.push(m);
        }
    }
    // Empty menus were kept that might list the windows; only the one
    // that does stays.
    out.drop_empty_menus();
    out
}

/// A menu and what is in it; `None` if it has nothing to show. Items of
/// the application menu are moved to `app` where it is given (the bar's
/// own menus).
fn describe_menu(
    e: &Engine,
    menu: ObjectId,
    mut app: Option<&mut SystemBar>,
) -> Option<SystemMenu> {
    if !is_a::<Menu>(e, menu) {
        return None;
    }
    let title = match e.read(menu, Menu::TITLE.index) {
        Value::String(s) => without_mnemonic(&s),
        _ => String::new(),
    };
    // Changes whenever an item joins or leaves.
    let _ = e.read(menu, Menu::COUNT.index);
    let mut entries = Vec::new();
    for item in items(e, menu) {
        if is_a::<MenuSeparator>(e, item) {
            entries.push(SystemEntry::Separator);
            continue;
        }
        if !is_a::<MenuItem>(e, item) {
            continue;
        }
        // Read so that a sub-menu set later is seen.
        let _ = e.read(item, MenuItem::SUB_MENU.index);
        if let Some(sub) = sub_menu_of(e, item) {
            if let Some(m) = describe_menu(e, sub, None) {
                entries.push(SystemEntry::Menu(m));
            }
            continue;
        }
        let described = describe_item(e, item);
        let role = role_of(&described.text);
        if let Some(described) = place(app.as_deref_mut(), role, described) {
            entries.push(SystemEntry::Item(described));
        }
    }
    trim_separators(&mut entries);
    let menu = SystemMenu {
        title,
        role: menu_role(e, menu),
        entries,
    };
    // A menu of the bar's the system may list the windows in is shown
    // with nothing of its own, for the system to fill.
    let keep = !menu.entries.is_empty() || (app.is_some() && menu.lists_windows());
    keep.then_some(menu)
}

/// Puts `item` in the application menu where its role has a place there
/// and `app` is given (an item of the bar's own menus), the first item of
/// a role taking the place; otherwise gives it back for its menu.
fn place(app: Option<&mut SystemBar>, role: Role, item: SystemItem) -> Option<SystemItem> {
    let Some(app) = app else {
        return Some(item);
    };
    let place = match role {
        Role::About => &mut app.about,
        Role::Preferences => &mut app.preferences,
        Role::Quit => &mut app.quit,
        Role::None => return Some(item),
    };
    place.get_or_insert(item);
    None
}

/// Separators at the ends or side by side show nothing.
fn trim_separators(entries: &mut Vec<SystemEntry>) {
    entries.dedup_by(|a, b| {
        matches!(a, SystemEntry::Separator) && matches!(b, SystemEntry::Separator)
    });
    while matches!(entries.last(), Some(SystemEntry::Separator)) {
        entries.pop();
    }
    while matches!(entries.first(), Some(SystemEntry::Separator)) {
        entries.remove(0);
    }
}

/// The objects of the list property `p` of `obj` (tracked).
fn objects(e: &Engine, obj: ObjectId, p: ljos_engine::PropIndex) -> Vec<ObjectId> {
    match e.read(obj, p) {
        Value::List(list) => list
            .iter()
            .filter_map(|v| match v {
                Value::Object(o) => Some(*o),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// A `Qt.labs.platform` bar as the system shows it (tracked).
fn describe_platform(e: &Engine, bar: ObjectId) -> SystemBar {
    let mut out = SystemBar::default();
    for menu in objects(e, bar, PlatformMenuBar::MENUS.index) {
        if let Some(m) = describe_platform_menu(e, menu, true, Some(&mut out)) {
            out.menus.push(m);
        }
    }
    out
}

/// A `Qt.labs.platform` menu and what is in it; `None` if it is not
/// visible or has nothing to show. A disabled menu's items cannot be
/// chosen. Items of the application menu go to `app` where it is given.
fn describe_platform_menu(
    e: &Engine,
    menu: ObjectId,
    enabled: bool,
    mut app: Option<&mut SystemBar>,
) -> Option<SystemMenu> {
    if !is_a::<PlatformMenu>(e, menu) || !flag(e, menu, PlatformMenu::VISIBLE.index) {
        return None;
    }
    let title = match e.read(menu, PlatformMenu::TITLE.index) {
        Value::String(s) => without_mnemonic(&s),
        _ => String::new(),
    };
    let enabled = enabled && flag(e, menu, PlatformMenu::ENABLED.index);
    let mut entries = Vec::new();
    for item in objects(e, menu, PlatformMenu::ITEMS.index) {
        if is_a::<PlatformMenu>(e, item) {
            if let Some(m) = describe_platform_menu(e, item, enabled, None) {
                entries.push(SystemEntry::Menu(m));
            }
            continue;
        }
        if !is_a::<PlatformMenuItem>(e, item) || !flag(e, item, PlatformMenuItem::VISIBLE.index) {
            continue;
        }
        if flag(e, item, PlatformMenuItem::SEPARATOR.index) {
            entries.push(SystemEntry::Separator);
            continue;
        }
        let text = match e.read(item, PlatformMenuItem::TEXT.index) {
            Value::String(s) => without_mnemonic(&s),
            _ => String::new(),
        };
        use platform_menu::role as r;
        let role = match e.read(item, PlatformMenuItem::ROLE.index).to_number() as i32 {
            r::TEXT_HEURISTIC_ROLE => role_of(&text),
            r::ABOUT_ROLE => Role::About,
            r::PREFERENCES_ROLE => Role::Preferences,
            r::QUIT_ROLE => Role::Quit,
            _ => Role::None,
        };
        let described = SystemItem {
            id: item.data().as_ffi(),
            text,
            enabled: enabled && flag(e, item, PlatformMenuItem::ENABLED.index),
            checkable: flag(e, item, PlatformMenuItem::CHECKABLE.index),
            checked: flag(e, item, PlatformMenuItem::CHECKED.index),
            key: key_of(e, &e.read(item, PlatformMenuItem::SHORTCUT.index)),
        };
        if let Some(described) = place(app.as_deref_mut(), role, described) {
            entries.push(SystemEntry::Item(described));
        }
    }
    trim_separators(&mut entries);
    let menu = SystemMenu {
        title,
        role: menu_role(e, menu),
        entries,
    };
    // A menu of the bar's the system may list the windows in is kept
    // with nothing of its own, for the system to fill if `describe` finds
    // it is the one.
    let keep = !menu.entries.is_empty() || (app.is_some() && menu.lists_windows());
    keep.then_some(menu)
}

fn describe_item(e: &Engine, item: ObjectId) -> SystemItem {
    let text = match e.read(item, AbstractButton::TEXT.index) {
        Value::String(s) => without_mnemonic(&s),
        _ => String::new(),
    };
    // Read so that an action set later is seen.
    let _ = e.read(item, AbstractButton::ACTION.index);
    let key = action_of(e, item).and_then(|a| key_of(e, &e.read(a, Action::SHORTCUT.index)));
    SystemItem {
        id: item.data().as_ffi(),
        text,
        enabled: flag(e, item, Item::ENABLED.index),
        checkable: flag(e, item, AbstractButton::CHECKABLE.index),
        checked: flag(e, item, AbstractButton::CHECKED.index),
        key,
    }
}

/// A shortcut of one chord as a key equivalent: a sequence, or a
/// standard key as the platform has it.
fn key_of(e: &Engine, shortcut: &Value) -> Option<KeyEquivalent> {
    let sequence = match shortcut {
        Value::String(s) => KeySequence::parse(s)?,
        Value::Int(_) | Value::Real(_) | Value::Enum(_) => {
            standard_key_sequences(shortcut.to_number() as i32, platform(e))
                .into_iter()
                .next()?
        }
        _ => return None,
    };
    let [chord] = sequence.chords() else {
        return None;
    };
    let m = chord.modifiers;
    Some(KeyEquivalent {
        key: chord.key,
        command: m.contains(Modifiers::CONTROL),
        shift: m.contains(Modifiers::SHIFT),
        option: m.contains(Modifiers::ALT),
        control: m.contains(Modifiers::META),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quit_triggers_the_item_that_quits_only_while_it_can_be_chosen() {
        let mut item = SystemItem {
            id: 7,
            text: "Beenden".into(),
            enabled: true,
            checkable: false,
            checked: false,
            key: None,
        };
        let bar = |item: &SystemItem| SystemBar {
            quit: Some(item.clone()),
            ..SystemBar::default()
        };
        assert_eq!(quit_target(&bar(&item)), Some(7));
        // Disabled itself, or by its menu, which describes it disabled.
        item.enabled = false;
        assert_eq!(quit_target(&bar(&item)), None);
        assert_eq!(quit_target(&SystemBar::default()), None);
    }

    #[test]
    fn a_mnemonic_is_not_shown() {
        assert_eq!(without_mnemonic("Save &As…"), "Save As…");
        assert_eq!(without_mnemonic("Fish && &Chips"), "Fish & Chips");
    }

    #[test]
    fn the_text_says_which_items_the_application_menu_takes() {
        assert!(role_of("About Hof") == Role::About);
        assert!(role_of("Settings…") == Role::Preferences);
        assert!(role_of("Preferences...") == Role::Preferences);
        assert!(role_of("Exit") == Role::Quit);
        assert!(role_of("Quit Hof") == Role::None);
        assert!(role_of("Options for printing") == Role::None);
    }
}
