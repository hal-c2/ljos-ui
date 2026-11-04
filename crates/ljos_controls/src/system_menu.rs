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

use std::cell::RefCell;

use ljos_engine::{Engine, EngineError, MetaRef, ObjectId, OverrideContent, QmlType, Value};
use ljos_platform::{KeyEquivalent, Platform, SystemBar, SystemEntry, SystemItem, SystemMenu};
use ljos_quick::Item;
use ljos_quick::Modifiers;
use ljos_quick::keys::{KeySequence, platform, standard_key_sequences};
use slotmap::{Key, KeyData};

use crate::action::Action;
use crate::button::AbstractButton;
use crate::menu::{Menu, MenuItem, MenuSeparator, action_of, items, sub_menu_of};
use crate::menu_bar::MenuBar;
use crate::util::{flag, is_a, object};
use crate::window::Window;
use ljos_quick::focus::WindowAttached;

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
    let att = e.attached_object(bar, &MetaRef::Static(Window::static_meta()))?;
    e.effect_in(bar, move |e| {
        let described = describe(e, bar);
        // The window's own `active`, which follows activation; the
        // attached one of an item in a window's bar did not.
        let active = object(e, att, WindowAttached::WINDOW.index)
            .is_some_and(|win| flag(e, win, Window::ACTIVE.index));
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
    if !e.is_alive(item) || !is_a::<AbstractButton>(e, item) {
        return false;
    }
    let _ = e.invoke_by_name(item, "click", &[]);
    true
}

/// The user chose Quit in the application menu: the item that quits is
/// triggered, or with none every window is asked to close as its user
/// would close it, and the application quits if all of them do, as
/// `QCoreApplication::quit()` does: a window that will not close stops it.
pub fn quit(e: &Engine) {
    let quit_item = e
        .extension::<Bars>()
        .shown
        .borrow()
        .as_ref()
        .and_then(|(_, bar)| bar.quit.as_ref().map(|q| q.id));
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
        if let Some(app) = app.as_deref_mut() {
            let place = match role_of(&described.text) {
                Role::About => Some(&mut app.about),
                Role::Preferences => Some(&mut app.preferences),
                Role::Quit => Some(&mut app.quit),
                Role::None => None,
            };
            if let Some(place) = place {
                place.get_or_insert(described);
                continue;
            }
        }
        entries.push(SystemEntry::Item(described));
    }
    // Separators at the ends or side by side show nothing.
    entries.dedup_by(|a, b| {
        matches!(a, SystemEntry::Separator) && matches!(b, SystemEntry::Separator)
    });
    while matches!(entries.last(), Some(SystemEntry::Separator)) {
        entries.pop();
    }
    while matches!(entries.first(), Some(SystemEntry::Separator)) {
        entries.remove(0);
    }
    (!entries.is_empty()).then_some(SystemMenu { title, entries })
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
