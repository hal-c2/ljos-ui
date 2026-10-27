//! `ProgressBar`, `BusyIndicator` and `ToolSeparator` (QtQuick.Templates):
//! controls that show something and take nothing.
//!
//! Behaviour follows doc.qt.io/qt-6/qml-qtquick-controls-progressbar.html,
//! -busyindicator.html and -toolseparator.html, and is pinned by
//! `features/controls/progress-bar.feature`, `busy-indicator.feature` and
//! `tool-separator.feature`. What moves (the busy indicator's spin, the
//! indeterminate bar's sweep) is the style's, in QML, and runs only while
//! it is shown.

use ljos_a11y::{NodeData, Range};
use ljos_engine::{
    Cx, Engine, EngineError, MetaRef, ObjectId, QmlObject, QmlType, Value, qml_impl,
};
use ljos_quick::{AccessibleItem, register_accessible_item};

use crate::control::Control;
use crate::control::no_action;
use crate::scroll_bar::orientation;
use crate::slider::Span;
use crate::util::{check, flag, num};

/// `ProgressBar`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "ProgressBar", extends = Control)]
pub struct ProgressBar {
    #[qml(base)]
    pub control: Control,
    #[qml(notify)]
    pub from: f64,
    #[qml(notify)]
    pub to: f64,
    #[qml(notify)]
    pub value: f64,
    #[qml(notify, readonly)]
    pub position: f64,
    #[qml(notify, readonly)]
    pub visual_position: f64,
    #[qml(notify)]
    pub indeterminate: bool,
}

impl Default for ProgressBar {
    fn default() -> Self {
        Self {
            control: Control::default(),
            from: 0.0,
            to: 1.0,
            value: 0.0,
            position: 0.0,
            visual_position: 0.0,
            indeterminate: false,
        }
    }
}

fn bar_span(e: &Engine, obj: ObjectId) -> Span {
    Span::read(e, obj, ProgressBar::FROM.index, ProgressBar::TO.index)
}

/// Brings the value back between the ends. A derived write, so that a
/// binding on `value` stays.
fn keep_value(e: &Engine, obj: ObjectId) {
    let (v, s) = e.untrack(|| (num(e, obj, ProgressBar::VALUE.index), bar_span(e, obj)));
    let k = s.clamp(v);
    if k != v && !(k.is_nan() && v.is_nan()) {
        check(
            e,
            "ProgressBar.value",
            e.write_derived(obj, ProgressBar::VALUE.index, Value::Real(k)),
        );
    }
}

#[qml_impl]
impl ProgressBar {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, obj) = (cx.engine, cx.this);
        keep_value(e, obj);
        for p in [
            ProgressBar::FROM.index,
            ProgressBar::TO.index,
            ProgressBar::VALUE.index,
        ] {
            e.watch(obj, p, move |e, _| keep_value(e, obj))?;
        }
        e.bind_initial(obj, ProgressBar::POSITION.index, move |e| {
            Value::Real(bar_span(e, obj).position(num(e, obj, ProgressBar::VALUE.index)))
        })?;
        e.bind_initial(obj, ProgressBar::VISUAL_POSITION.index, move |e| {
            let p = num(e, obj, ProgressBar::POSITION.index);
            Value::Real(if flag(e, obj, Control::MIRRORED.index) {
                1.0 - p
            } else {
                p
            })
        })?;
        Ok(())
    }
}

/// `BusyIndicator`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "BusyIndicator", extends = Control)]
pub struct BusyIndicator {
    #[qml(base)]
    pub control: Control,
    #[qml(notify)]
    pub running: bool,
}

impl Default for BusyIndicator {
    fn default() -> Self {
        Self {
            control: Control::default(),
            running: true,
        }
    }
}

/// `ToolSeparator`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "ToolSeparator", extends = Control)]
pub struct ToolSeparator {
    #[qml(base)]
    pub control: Control,
    #[qml(notify, enumeration = "Qt.Orientation")]
    pub orientation: i32,
    #[qml(notify, readonly)]
    pub horizontal: bool,
    #[qml(notify, readonly)]
    pub vertical: bool,
}

impl Default for ToolSeparator {
    fn default() -> Self {
        Self {
            control: Control::default(),
            orientation: orientation::VERTICAL,
            horizontal: false,
            vertical: true,
        }
    }
}

#[qml_impl]
impl ToolSeparator {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, obj) = (cx.engine, cx.this);
        let along = move |e: &Engine| {
            num(e, obj, ToolSeparator::ORIENTATION.index) as i32 != orientation::VERTICAL
        };
        e.bind_initial(obj, ToolSeparator::HORIZONTAL.index, move |e| {
            Value::Bool(along(e))
        })?;
        e.bind_initial(obj, ToolSeparator::VERTICAL.index, move |e| {
            Value::Bool(!along(e))
        })?;
        Ok(())
    }
}

/// A progress bar tells how far it has gone; it takes no step, so the
/// range has none.
fn describe_bar(e: &Engine, obj: ObjectId, data: &mut NodeData) {
    if flag(e, obj, ProgressBar::INDETERMINATE.index) {
        return;
    }
    let s = bar_span(e, obj);
    data.range = Some(Range {
        value: num(e, obj, ProgressBar::VALUE.index),
        min: s.lo(),
        max: s.hi(),
        step: 0.0,
    });
}

pub(crate) fn register(engine: &Engine) {
    register_accessible_item(
        engine,
        MetaRef::Static(ProgressBar::static_meta()),
        AccessibleItem {
            describe: describe_bar,
            act: no_action,
        },
    );
}
