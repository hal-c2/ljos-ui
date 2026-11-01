//! `Qt.inputMethod`: what QML knows of the platform's input method, and
//! what it may ask of it. There is one, for as long as the engine lives,
//! and it is there on a machine without an on-screen keyboard too.
//!
//! What it says of the editor with focus is kept up by the focus tree and
//! the editors, and after each frame by [`ImeState`](crate::ImeState),
//! which is also how a request reaches the platform. What it says of the
//! keyboard only a host can know: [`set_on_screen_keyboard`].
//!
//! Behaviour follows doc.qt.io/qt-6/qml-qtquick-inputmethod.html and is
//! pinned by `features/text-input/input-method-object.feature`.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use ljos_engine::{
    Cx, Engine, LogLevel, ObjectId, PropIndex, QmlObject, Rect, Value, WriteKind, qml_impl,
};
use ljos_render::kurbo;

use crate::item::{Item, scene_transform};
use crate::keys::text_input_client;

/// `Qt.inputMethod`.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "InputMethod", uncreatable)]
pub struct InputMethod {
    /// The cursor of the editor with focus, in its window's coordinates.
    #[qml(notify, readonly)]
    pub cursor_rectangle: Rect,
    /// Where the selection starts, as `cursorRectangle` is where it ends.
    #[qml(notify, readonly)]
    pub anchor_rectangle: Rect,
    /// What the on-screen keyboard covers; empty when it covers nothing.
    #[qml(notify, readonly)]
    pub keyboard_rectangle: Rect,
    /// The item with focus; the window when nothing has it.
    #[qml(notify, readonly)]
    pub input_item_clip_rectangle: Rect,
    /// Whether the on-screen keyboard is showing.
    #[qml(notify, readonly)]
    pub visible: bool,
    #[qml(notify, readonly)]
    pub animating: bool,
    /// A `Qt.LayoutDirection`: which way the current input language goes.
    #[qml(notify, readonly)]
    pub input_direction: i32,
}

#[qml_impl]
impl InputMethod {
    /// Asks for the on-screen keyboard. It may not come: `visible` says.
    #[qml(invokable)]
    fn show(cx: &Cx) {
        state(cx.engine).keyboard.set(Some(true));
        cx.engine.request_wake();
    }

    #[qml(invokable)]
    fn hide(cx: &Cx) {
        state(cx.engine).keyboard.set(Some(false));
        cx.engine.request_wake();
    }

    /// Something the input method looks at changed. What it looks at is
    /// read again after each frame; here the object catches up at once.
    #[qml(invokable)]
    fn update(cx: &Cx, _queries: Value) {
        refresh(cx.engine);
    }

    /// Drops what is being composed.
    #[qml(invokable)]
    fn reset(cx: &Cx) {
        end_composition(cx.engine, false);
    }

    /// Types what is being composed: an application that reads an editor's
    /// text does not lose the word the user is in the middle of.
    #[qml(invokable)]
    fn commit(cx: &Cx) {
        end_composition(cx.engine, true);
    }

    /// A press at a place in what is being composed. Ljós's editors end a
    /// composition when they are pressed, and no platform's method here
    /// takes the press itself.
    #[qml(invokable, name = "invokeAction")]
    fn invoke_action(_cx: &Cx, _action: Value, _position: Value) {}
}

#[derive(Default)]
struct State {
    object: Cell<Option<ObjectId>>,
    /// What a host said of its keyboard. JavaScript starts when first
    /// needed, and a host may speak before that.
    covers: Cell<Option<kurbo::Rect>>,
    keyboard: Cell<Option<bool>>,
    interrupt: Cell<bool>,
    /// Against a write here making a binding read it again, while it is
    /// being written.
    refreshing: Cell<bool>,
}

fn state(engine: &Engine) -> Rc<State> {
    engine.extension::<State>()
}

/// What the application asked of the platform since the last time.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Requests {
    pub interrupt: bool,
    pub keyboard: Option<bool>,
}

pub(crate) fn take_requests(engine: &Engine) -> Requests {
    let s = state(engine);
    Requests {
        interrupt: s.interrupt.take(),
        keyboard: s.keyboard.take(),
    }
}

/// The window that has the keyboard, else the first there is, with the
/// item that has focus in it.
fn focus_place(engine: &Engine) -> Option<(ObjectId, Option<ObjectId>)> {
    let roots = crate::focus::window_roots(engine);
    let root = roots
        .iter()
        .copied()
        .find(|r| crate::focus::is_window_active(engine, *r))
        .or_else(|| roots.first().copied())?;
    Some((root, crate::focus::active_focus_item(engine, root)))
}

fn end_composition(engine: &Engine, typed: bool) {
    let Some((_, Some(item))) = focus_place(engine) else {
        return;
    };
    let Some(client) = text_input_client(engine, item) else {
        return;
    };
    let composing = client.preedit(engine, item);
    if composing.is_empty() {
        return;
    }
    if typed {
        client.commit(engine, item, &composing);
    } else {
        client.set_preedit(engine, item, "", None);
    }
    state(engine).interrupt.set(true);
    engine.request_wake();
}

fn rect_of(r: kurbo::Rect) -> Rect {
    Rect {
        x: r.x0,
        y: r.y0,
        width: r.width(),
        height: r.height(),
    }
}

fn write_rect(engine: &Engine, object: ObjectId, prop: PropIndex, now: Rect) {
    let old = engine.untrack(|| engine.read(object, prop));
    if !matches!(&old, Value::Rect(r) if **r == now) {
        let _ = engine.write_internal(
            object,
            prop,
            Value::Rect(Arc::new(now)),
            WriteKind::Imperative,
        );
    }
}

/// Makes the object say what is so now. Costs a few reads when nothing
/// changed.
pub(crate) fn refresh(engine: &Engine) {
    let s = state(engine);
    let Some(object) = s.object.get().filter(|o| engine.is_alive(*o)) else {
        return;
    };
    if s.refreshing.replace(true) {
        return;
    }
    let (cursor, anchor, clip) = engine.untrack(|| match focus_place(engine) {
        None => (Rect::default(), Rect::default(), Rect::default()),
        Some((root, focused)) => {
            let size = |o| {
                let (w, h) = engine
                    .with(o, |i: &Item| (i.width, i.height))
                    .unwrap_or((0.0, 0.0));
                kurbo::Rect::new(0.0, 0.0, w, h)
            };
            match focused {
                None => (Rect::default(), Rect::default(), rect_of(size(root))),
                Some(item) => {
                    let to_window = scene_transform(engine, item);
                    let shown = |r| rect_of(to_window.transform_rect_bbox(r));
                    let (cursor, anchor) = text_input_client(engine, item).map_or(
                        (Rect::default(), Rect::default()),
                        |c| {
                            (
                                shown(c.cursor_rect(engine, item)),
                                shown(c.anchor_rect(engine, item)),
                            )
                        },
                    );
                    (cursor, anchor, shown(size(item)))
                }
            }
        }
    });
    write_rect(engine, object, InputMethod::CURSOR_RECTANGLE.index, cursor);
    write_rect(engine, object, InputMethod::ANCHOR_RECTANGLE.index, anchor);
    write_rect(
        engine,
        object,
        InputMethod::INPUT_ITEM_CLIP_RECTANGLE.index,
        clip,
    );
    s.refreshing.set(false);
}

/// An editor's cursor moved. Only the one with focus concerns the object,
/// and an editor publishes often: nothing is done for any other.
pub(crate) fn cursor_moved(engine: &Engine, item: ObjectId) {
    if state(engine).object.get().is_some()
        && matches!(focus_place(engine), Some((_, Some(focused))) if focused == item)
    {
        refresh(engine);
    }
}

/// What the on-screen keyboard covers, in the coordinates of the window,
/// or `None` when it is not showing. For a host that has one.
pub fn set_on_screen_keyboard(engine: &Engine, covers: Option<kurbo::Rect>) {
    state(engine).covers.set(covers);
    show_keyboard(engine);
}

fn show_keyboard(engine: &Engine) {
    let s = state(engine);
    let Some(object) = s.object.get().filter(|o| engine.is_alive(*o)) else {
        return;
    };
    let covers = s.covers.get();
    let _ = engine.write_internal(
        object,
        InputMethod::VISIBLE.index,
        Value::Bool(covers.is_some()),
        WriteKind::Imperative,
    );
    write_rect(
        engine,
        object,
        InputMethod::KEYBOARD_RECTANGLE.index,
        covers.map_or(Rect::default(), rect_of),
    );
}

/// A read of `locale` goes to `Qt.locale()`, so a binding on it follows the
/// application's locale: Ljós knows no input language apart from it.
const INSTALL: &str = r#"(function (method) {
  Object.defineProperty(method, "locale", { get() { return Qt.locale(); }, enumerable: true });
  Object.defineProperty(Qt, "inputMethod", { value: method, enumerable: true });
})"#;

pub(crate) fn register(engine: &Engine) {
    // Made here, or the first item to take focus would pay for it.
    state(engine);
    engine.on_js_started(|e| {
        let Ok(object) = e.create_native_incomplete(InputMethod::default(), None) else {
            return;
        };
        state(e).object.set(Some(object));
        match e.eval_js(INSTALL) {
            Ok(install) => {
                let _ = e.call_js(&install, &[Value::Object(object)]);
            }
            Err(err) => e.log(
                LogLevel::Warning,
                crate::scene::SCENE_CATEGORY,
                format!("Qt.inputMethod is not available: {err}"),
            ),
        }
        show_keyboard(e);
        refresh(e);
    });
}
