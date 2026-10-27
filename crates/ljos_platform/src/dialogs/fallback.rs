//! The dialogs the application draws when the system shows none: a
//! QtQuick.Controls `Dialog` per kind (`qml/QtQuick/Dialogs/Ljos/`),
//! compiled into this crate ([`crate::library`]).
//!
//! The popup is made the first time its dialog opens, owned by the dialog,
//! and over the nearest item the dialog is declared in. It shows the
//! dialog's values through its `dialog` property and says how it ended
//! with its `answered(accepted, button)` signal; what the user entered is
//! read back from it here.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

use ljos_engine::{Color, Engine, EngineError, LogLevel, ObjectId, Value};

use super::{ColorDialog, FileDialog, FolderDialog, FontDialog, MessageDialog};
use crate::is_a;
use crate::paths::url_path;
use crate::platform::{CATEGORY, item_of};

/// The drawn popup of each dialog that has been opened.
#[derive(Debug, Default)]
struct Popups(RefCell<HashMap<ObjectId, ObjectId>>);

fn key(engine: &Engine, obj: ObjectId) -> Option<&'static str> {
    if is_a::<ColorDialog>(engine, obj) {
        Some("QtQuick/Dialogs/Ljos/ColorDialog")
    } else if is_a::<FontDialog>(engine, obj) {
        Some("QtQuick/Dialogs/Ljos/FontDialog")
    } else if is_a::<MessageDialog>(engine, obj) {
        Some("QtQuick/Dialogs/Ljos/MessageDialog")
    } else if is_a::<FileDialog>(engine, obj) || is_a::<FolderDialog>(engine, obj) {
        Some("QtQuick/Dialogs/Ljos/FileDialog")
    } else {
        None
    }
}

/// Makes and opens the popup of `obj`; `None` when there is none to make.
pub(super) fn open(engine: &Engine, obj: ObjectId) -> Option<ObjectId> {
    let key = key(engine, obj)?;
    let popups = engine.extension::<Popups>();
    // A popup is made afresh each time, so that it starts from what the
    // dialog holds now rather than from what was typed and cancelled.
    let old = popups.0.borrow_mut().remove(&obj);
    if let Some(old) = old.filter(|o| engine.is_alive(*o)) {
        engine.destroy(old);
    }
    let over = item_of(engine, obj)?;
    let made = engine.create_from_url(
        key,
        Some(obj),
        vec![
            ("dialog".to_owned(), Value::Object(obj)),
            ("parent".to_owned(), Value::Object(over)),
        ],
    );
    let popup = match made {
        Ok(p) => p,
        Err(e) => {
            engine.log(LogLevel::Warning, CATEGORY, format!("{key}: {e}"));
            return None;
        }
    };
    popups.0.borrow_mut().insert(obj, popup);
    let connected = engine.connect_by_name(popup, "answered", move |e, args| {
        let accepted = args.first().is_some_and(|a| a.to_number() != 0.0);
        let button = args.get(1).map_or(0, |b| b.to_number() as i32);
        super::drawn_answered(e, obj, accepted, button);
    });
    let opened = connected.and_then(|_| engine.invoke_by_name(popup, "open", &[]));
    if let Err(e) = opened {
        engine.log(LogLevel::Warning, CATEGORY, format!("{key}: {e}"));
        engine.destroy(popup);
        return None;
    }
    Some(popup)
}

/// Closes the popup, if it is still open.
pub(super) fn close(engine: &Engine, popup: ObjectId) {
    if !engine.is_alive(popup) {
        return;
    }
    if engine
        .untrack(|| engine.read_by_name(popup, "visible"))
        .to_boolean()
    {
        let _ = engine.invoke_by_name(popup, "close", &[]);
    }
}

fn read(engine: &Engine, popup: ObjectId, name: &str) -> Value {
    engine.untrack(|| engine.read_by_name(popup, name))
}

/// Writes what the user entered in the popup into the dialog it accepted.
pub(super) fn take_values(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    let popup = engine.extension::<Popups>().0.borrow().get(&obj).copied();
    let Some(popup) = popup.filter(|p| engine.is_alive(*p)) else {
        return Ok(());
    };
    if is_a::<ColorDialog>(engine, obj) {
        let typed = read(engine, popup, "colorText").to_js_string();
        match Color::parse(typed.trim()) {
            Some(c) => engine.set(obj, ColorDialog::SELECTED_COLOR, c)?,
            None => engine.log(
                LogLevel::Warning,
                CATEGORY,
                format!("{typed:?} is not a colour; the colour stays as it was"),
            ),
        }
    } else if is_a::<FontDialog>(engine, obj) {
        let mut font = engine.get(obj, FontDialog::SELECTED_FONT)?;
        let family = read(engine, popup, "family").to_js_string();
        if !family.trim().is_empty() {
            font.family = ljos_engine::Atom::from(family.trim());
        }
        font.weight = if read(engine, popup, "bold").to_boolean() {
            700
        } else {
            400
        };
        font.italic = read(engine, popup, "italic").to_boolean();
        engine.set(obj, FontDialog::CURRENT_FONT, font.clone())?;
        engine.set(obj, FontDialog::SELECTED_FONT, font)?;
    } else if is_a::<FileDialog>(engine, obj) || is_a::<FolderDialog>(engine, obj) {
        let typed = read(engine, popup, "path").to_js_string();
        let path = url_path(typed.trim()).map(|p| absolute(engine, obj, p));
        if let Some(p) = path {
            super::chose(engine, obj, &[p])?;
        }
    }
    Ok(())
}

/// A relative path typed into a file dialog is in its current folder.
fn absolute(engine: &Engine, obj: ObjectId, p: PathBuf) -> PathBuf {
    if p.is_absolute() {
        return p;
    }
    let folder = read(engine, obj, "currentFolder").to_js_string();
    match url_path(&folder) {
        Some(f) => f.join(p),
        None => p,
    }
}
