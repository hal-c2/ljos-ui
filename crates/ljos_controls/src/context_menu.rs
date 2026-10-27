//! `ContextMenu` (QtQuick.Controls): the menu an item opens when the user
//! asks for one, and the edit menu `TextField` and `TextArea` have of
//! their own.
//!
//! The scene says when the user asks (`ljos_quick::register_context_menu`);
//! this says which items answer and how. An item answers when it has a
//! menu or something listens to its `requested`; an editor always does,
//! with its own menu unless it was given one. What qml6 does is in
//! `features/controls/context-menu.feature` and
//! `features/controls/text-editing-menu.feature`.

use ljos_engine::{
    Atom, Engine, EngineError, MetaRef, ObjectId, Point, QmlObject, QmlType, SignalIndex, Value,
    qml_impl,
};
use ljos_quick::text_input::TextEditor;
use ljos_quick::{ContextMenuHook, ContextMenuReason, register_context_menu};
use ljos_render::kurbo;

use crate::menu::{self, Menu};
use crate::style::BASIC;
use crate::text_area::TextArea;
use crate::text_field::TextField;
use crate::util::{check, is_a, object, warn};

/// `ContextMenu`: only attached.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick.Controls",
    name = "ContextMenu",
    attached = ContextMenuAttached,
    uncreatable
)]
pub struct ContextMenu {}

/// What `ContextMenu.menu: …` attaches to an item.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "ContextMenuAttached")]
pub struct ContextMenuAttached {
    /// The menu the item opens; an editor's own once it has opened it.
    #[qml(notify, object = "Menu")]
    pub menu: Option<ObjectId>,
}

#[qml_impl]
impl ContextMenuAttached {
    /// The user asked for a context menu, at `position` in the item's
    /// coordinates, before the menu opens.
    #[qml(signal)]
    fn requested(position: Point);
}

fn requested_signal() -> SignalIndex {
    ContextMenuAttached::static_meta()
        .signal_index("requested")
        .expect("declared by #[qml_impl]")
}

fn attacher() -> MetaRef {
    MetaRef::Static(ContextMenu::static_meta())
}

fn is_editor(e: &Engine, obj: ObjectId) -> bool {
    is_a::<TextField>(e, obj) || is_a::<TextArea>(e, obj)
}

fn takes(e: &Engine, obj: ObjectId) -> bool {
    if is_editor(e, obj) {
        return true;
    }
    let Some(att) = e.existing_attached_object(obj, &attacher()) else {
        return false;
    };
    object(e, att, ContextMenuAttached::MENU.index).is_some()
        || e.signal_connected(att, requested_signal())
}

fn request(e: &Engine, obj: ObjectId, at: kurbo::Point, _reason: ContextMenuReason) {
    check(e, "ContextMenu", open(e, obj, at));
}

fn open(e: &Engine, obj: ObjectId, at: kurbo::Point) -> Result<(), EngineError> {
    let editor = is_editor(e, obj);
    // An editor's menu is for the text at its cursor, wherever the click.
    let at = if editor { cursor_middle(e, obj) } else { at };
    if let Some(att) = e.existing_attached_object(obj, &attacher()) {
        e.emit(att, requested_signal(), &[Value::point(at.x, at.y)])?;
    }
    if !e.is_alive(obj) {
        return Ok(());
    }
    let att = e.existing_attached_object(obj, &attacher());
    let mut menu = att.and_then(|a| object(e, a, ContextMenuAttached::MENU.index));
    if menu.is_none() && editor {
        let made = edit_menu(e, obj)?;
        let att = e.attached_object(obj, &attacher())?;
        crate::util::set_derived(e, att, ContextMenuAttached::MENU.index, Value::Object(made))?;
        menu = Some(made);
    }
    match menu {
        Some(m) if e.is_alive(m) => menu::popup_on(e, m, obj, at),
        _ => Ok(()),
    }
}

/// The middle of the editor's cursor, in whole pixels, as qml6 has it.
fn cursor_middle(e: &Engine, obj: ObjectId) -> kurbo::Point {
    match e.untrack(|| e.read(obj, TextEditor::CURSOR_RECTANGLE.index)) {
        Value::Rect(r) => kurbo::Point::new(
            (r.x + r.width / 2.0).round(),
            (r.y + r.height / 2.0).round(),
        ),
        _ => kurbo::Point::ZERO,
    }
}

/// What an edit menu item does to the editor, and when it can.
struct Entry {
    text: &'static str,
    enabled: fn(&Engine, ObjectId) -> bool,
    act: fn(&Engine, ObjectId) -> Result<(), EngineError>,
}

fn flag_of(e: &Engine, obj: ObjectId, p: ljos_engine::PropIndex) -> bool {
    e.read(obj, p).to_boolean()
}

fn selected(e: &Engine, obj: ObjectId) -> bool {
    !e.read(obj, TextEditor::SELECTED_TEXT.index)
        .to_js_string()
        .is_empty()
}

fn writable(e: &Engine, obj: ObjectId) -> bool {
    !flag_of(e, obj, TextEditor::READ_ONLY.index)
}

fn call(e: &Engine, obj: ObjectId, name: &str) -> Result<(), EngineError> {
    e.invoke_by_name(obj, name, &[]).map(drop)
}

/// The items, a `None` for a separator. A password field's Cut and Copy
/// are enabled as any other's, as in qml6.
const ENTRIES: [Option<Entry>; 9] = [
    Some(Entry {
        text: "Undo",
        enabled: |e, o| flag_of(e, o, TextEditor::CAN_UNDO.index),
        act: |e, o| call(e, o, "undo"),
    }),
    Some(Entry {
        text: "Redo",
        enabled: |e, o| flag_of(e, o, TextEditor::CAN_REDO.index),
        act: |e, o| call(e, o, "redo"),
    }),
    None,
    Some(Entry {
        text: "Cut",
        enabled: |e, o| selected(e, o) && writable(e, o),
        act: |e, o| call(e, o, "cut"),
    }),
    Some(Entry {
        text: "Copy",
        enabled: selected,
        act: |e, o| call(e, o, "copy"),
    }),
    Some(Entry {
        text: "Paste",
        enabled: |e, o| flag_of(e, o, TextEditor::CAN_PASTE.index) && writable(e, o),
        act: |e, o| call(e, o, "paste"),
    }),
    Some(Entry {
        text: "Delete",
        enabled: |e, o| selected(e, o) && writable(e, o),
        act: |e, o| {
            let start = e.read(o, TextEditor::SELECTION_START.index);
            let end = e.read(o, TextEditor::SELECTION_END.index);
            e.invoke_by_name(o, "remove", &[start, end]).map(drop)
        },
    }),
    None,
    Some(Entry {
        text: "Select All",
        enabled: |_, _| true,
        act: |e, o| call(e, o, "selectAll"),
    }),
];

/// The library directory of the style `editor` is drawn by, Basic when
/// it is none of the built-in styles'.
fn style_of(e: &Engine, editor: ObjectId) -> String {
    let mut cur = e.meta_of(editor);
    while let Some(meta) = cur {
        if let Some(key) = e.library_key_of(&meta)
            && let Some((dir, _)) = key.as_str().rsplit_once('/')
            && dir.starts_with("QtQuick/Controls/")
        {
            return dir.to_string();
        }
        cur = meta.base.clone();
    }
    BASIC.to_string()
}

/// One of the style's types, Basic's when the style has none of its own.
fn make(e: &Engine, style: &str, name: &str, parent: ObjectId) -> Result<ObjectId, EngineError> {
    e.create_from_url(&format!("{style}/{name}"), Some(parent), Vec::new())
        .or_else(|_| e.create_from_url(&format!("{BASIC}/{name}"), Some(parent), Vec::new()))
}

/// The edit menu of `editor`, in its style, made the first time it opens.
fn edit_menu(e: &Engine, editor: ObjectId) -> Result<ObjectId, EngineError> {
    let style = style_of(e, editor);
    let m = make(e, &style, "Menu", editor)?;
    for entry in &ENTRIES {
        let Some(entry) = entry else {
            let sep = make(e, &style, "MenuSeparator", m)?;
            menu::add_entry(e, m, sep)?;
            continue;
        };
        let item = make(e, &style, "MenuItem", m)?;
        let text = entry.text;
        let text_at = e.property_index(item, "text")?;
        e.bind(item, text_at, move |e| {
            Value::from(Atom::from(e.translate("ContextMenu", text, "", None)))
        })?;
        let enabled = entry.enabled;
        let enabled_at = e.property_index(item, "enabled")?;
        e.bind(item, enabled_at, move |e| {
            Value::Bool(e.is_alive(editor) && enabled(e, editor))
        })?;
        let act = entry.act;
        e.connect_by_name(item, "triggered", move |e, _| {
            if e.is_alive(editor) {
                check(e, "ContextMenu", act(e, editor));
            }
        })?;
        menu::add_entry(e, m, item)?;
    }
    if !is_a::<Menu>(e, m) {
        warn(e, format!("{style}/Menu is not a Menu"));
    }
    Ok(m)
}

pub(crate) fn register(engine: &Engine) {
    register_context_menu(engine, ContextMenuHook { takes, request });
}
