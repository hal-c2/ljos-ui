//! Localisation in the engine (ARCHITECTURE.md §15): the UI language, the
//! catalogues, and what QML sees of them (`qsTr` and its family,
//! `String.prototype.arg`, `Qt.uiLanguage`, `Qt.locale()`,
//! `Qt.application.layoutDirection`).
//!
//! The catalogues and the rules live in `ljos_i18n`; here they meet the
//! reactive graph. Three signals carry a change to the bindings that care:
//!
//! - the **revision** ([`Engine::retranslate`]): read by every lookup, so a
//!   new language or a new catalogue re-runs exactly the bindings that
//!   translated something;
//! - the **UI language**: read by `Qt.uiLanguage` and the layout direction;
//! - the **locale**: read by `Qt.locale()` and by a `%L` marker.
//!
//! Nothing is reloaded and nothing is walked: a binding that never asked
//! for a translation has no edge to any of the three.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_i18n::{I18n, LocaleInfo, Messages};
use ljos_ir::{Translatable, TranslationKind};
use ljos_js::{HostError, HostFn, JsFault, JsValue};

use crate::engine::Engine;
use crate::error::EngineError;
use crate::js::JsState;
use crate::value::Value;
use crate::{NodeId, WriteKind};

/// The engine's localisation state, kept as an [`Engine::extension`].
#[derive(Default)]
pub(crate) struct I18nState {
    i18n: RefCell<I18n>,
    language: Cell<Option<NodeId>>,
    locale: Cell<Option<NodeId>>,
}

/// The count of a plural call, as Qt reads a JS number: an int32, and a
/// negative one is no count at all (the `%n` stays in the text).
fn count(n: Option<&JsValue>) -> Option<u32> {
    match n {
        Some(JsValue::Number(n)) => u32::try_from(crate::to_int32(*n)).ok(),
        _ => None,
    }
}

fn string(v: Option<&JsValue>) -> &str {
    match v {
        Some(JsValue::String(s)) => s,
        _ => "",
    }
}

impl Engine {
    fn i18n(&self) -> Rc<I18nState> {
        self.extension::<I18nState>()
    }

    fn i18n_node(&self, slot: &Cell<Option<NodeId>>) -> NodeId {
        if let Some(n) = slot.get() {
            return n;
        }
        let g = &self.inner.graph;
        let n = g
            .property(g.root(), Value::Int(0))
            .expect("the root scope is always alive");
        slot.set(Some(n));
        n
    }

    fn i18n_track(&self, slot: &Cell<Option<NodeId>>) {
        let n = self.i18n_node(slot);
        let _ = self.inner.graph.try_get(n);
    }

    fn i18n_bump(&self, slot: &Cell<Option<NodeId>>) {
        // No node: nothing ever read the signal, so nothing waits for it.
        let Some(n) = slot.get() else {
            return;
        };
        let g = &self.inner.graph;
        let next = match g.untrack(|| g.try_get(n)) {
            Some(Value::Int(i)) => i.wrapping_add(1),
            _ => 1,
        };
        let _ = g.write(n, Value::Int(next), WriteKind::Initial);
    }

    /// Adds a catalogue (a `.ljl` file, or a `.ts` file read in development)
    /// for the language it names. Texts on screen change at once when that
    /// language is the UI language's.
    pub fn install_catalog(&self, messages: Box<dyn Messages>) {
        self.i18n().i18n.borrow_mut().install(messages);
        self.retranslate();
    }

    /// Takes the catalogues of `language` away again. False when there were
    /// none.
    pub fn remove_catalog(&self, language: &str) -> bool {
        let removed = self.i18n().i18n.borrow_mut().uninstall(language);
        if removed {
            self.retranslate();
        }
        removed
    }

    /// `Qt.uiLanguage`, as it was set.
    pub fn ui_language(&self) -> String {
        self.i18n().i18n.borrow().ui_language().to_owned()
    }

    /// What assigning `Qt.uiLanguage` does: every binding that translated a
    /// text runs again, and nothing else does. Setting the language it
    /// already has does nothing.
    pub fn set_ui_language(&self, language: &str) {
        let state = self.i18n();
        let changed = state.i18n.borrow_mut().set_ui_language(language);
        if changed {
            self.i18n_bump(&state.language);
            self.retranslate();
        }
    }

    /// Whether the UI language is written from right to left
    /// (`Qt.application.layoutDirection`).
    pub fn is_right_to_left(&self) -> bool {
        self.i18n().i18n.borrow().is_right_to_left()
    }

    /// The name of the application's locale (`Qt.locale().name`), which
    /// numbers are formatted by. It is not the UI language.
    pub fn locale_name(&self) -> String {
        self.i18n().i18n.borrow().locale().name().to_owned()
    }

    /// Sets the application's locale (`is_IS`, `de-AT`).
    pub fn set_locale(&self, name: &str) {
        let state = self.i18n();
        let changed = state.i18n.borrow_mut().set_locale(name);
        if changed {
            self.i18n_bump(&state.locale);
            // `%Ln` in a translated text is written by the locale.
            self.retranslate();
        }
    }

    /// Takes the locale and the UI language from the environment, as an
    /// application does when it starts. The engine itself starts in `en`
    /// and `en_US` whatever the machine, so that tests do not depend on it.
    pub fn use_system_locale(&self) {
        if let Some(name) = ljos_i18n::locale::system_locale_name() {
            self.set_locale(&name);
            self.set_ui_language(&name);
        }
    }

    /// `qsTr`: the text for `(context, source, comment)` in the UI language,
    /// the source text when no catalogue has it. `n` picks the plural form
    /// and replaces `%n`. The running binding will run again when the
    /// language changes.
    pub fn translate(&self, context: &str, source: &str, comment: &str, n: Option<u32>) -> String {
        self.track_translation();
        self.i18n()
            .i18n
            .borrow()
            .translate(context, source, comment, n)
    }

    /// `qsTrId`: the text with that id, the id itself when no catalogue has
    /// it.
    pub fn translate_id(&self, id: &str, n: Option<u32>) -> String {
        self.track_translation();
        self.i18n().i18n.borrow().translate_id(id, n)
    }

    /// A translatable literal of compiled QML (`text: qsTr("Open")`), which
    /// never reaches JS.
    pub(crate) fn translate_literal(&self, t: &Translatable) -> String {
        let n = t.plural.and_then(|n| {
            // The same reading of the count as a call from JS gets.
            u32::try_from(crate::to_int32(n as f64)).ok()
        });
        match t.kind {
            TranslationKind::Id => self.translate_id(&t.source, n),
            TranslationKind::Text => self.translate(
                &t.context,
                &t.source,
                t.comment.as_deref().unwrap_or_default(),
                n,
            ),
            TranslationKind::NoOp => t.source.clone(),
        }
    }

    /// `String.prototype.arg` for a number: `%1` takes it plain, `%L1` in
    /// the locale's digits and grouping.
    fn arg_number(&self, template: &str, value: f64) -> Option<String> {
        let state = self.i18n();
        if template.contains("%L") {
            self.i18n_track(&state.locale);
        }
        let (plain, localized) = ljos_i18n::arg::number_pair(value, state.i18n.borrow().locale());
        ljos_i18n::arg::arg(template, &plain, &localized)
    }

    /// The members of `Locale` that M1 has (ARCHITECTURE.md §15): the name,
    /// the signs numbers are written with, and the direction.
    fn locale_object(&self, name: Option<&str>) -> JsValue {
        let state = self.i18n();
        let info = match name {
            Some(name) => LocaleInfo::new(name),
            None => {
                self.i18n_track(&state.locale);
                state.i18n.borrow().locale().clone()
            }
        };
        let s = |v: String| JsValue::String(v);
        JsValue::Map(vec![
            ("name".into(), s(info.name().to_owned())),
            ("decimalPoint".into(), s(info.decimal_point())),
            ("groupSeparator".into(), s(info.group_separator())),
            ("zeroDigit".into(), s(info.zero_digit())),
            ("negativeSign".into(), s(info.negative_sign())),
            ("positiveSign".into(), s("+".to_owned())),
            (
                "textDirection".into(),
                JsValue::Number(f64::from(u8::from(info.is_right_to_left()))),
            ),
            (
                "uiLanguages".into(),
                JsValue::Array(info.ui_languages().into_iter().map(s).collect()),
            ),
        ])
    }

    /// Puts the localisation globals into a new JS host. They replace
    /// nothing a script could have made: the host has run only the prelude.
    pub(crate) fn install_i18n_globals(&self, js: &JsState) -> Result<(), EngineError> {
        let weak = Rc::downgrade(&self.inner);
        let host = |f: fn(&Engine, &[JsValue]) -> Result<JsValue, HostError>| -> HostFn {
            let weak = weak.clone();
            Rc::new(move |_scope, args| match weak.upgrade() {
                Some(inner) => f(&Engine::from_inner(inner), args),
                None => Ok(JsValue::Undefined),
            })
        };
        // The prelude below has checked the arguments, with Qt's messages.
        let functions: [(&str, HostFn); 8] = [
            (
                "__ljos_tr",
                host(|e, a| {
                    Ok(JsValue::String(e.translate(
                        string(a.first()),
                        string(a.get(1)),
                        string(a.get(2)),
                        count(a.get(3)),
                    )))
                }),
            ),
            (
                "__ljos_trId",
                host(|e, a| {
                    Ok(JsValue::String(
                        e.translate_id(string(a.first()), count(a.get(1))),
                    ))
                }),
            ),
            (
                "__ljos_arg",
                host(|e, a| {
                    let template = string(a.first());
                    let filled = match a.get(1) {
                        Some(JsValue::Number(n)) => e.arg_number(template, *n),
                        other => {
                            let text = string(other);
                            ljos_i18n::arg::arg(template, text, text)
                        }
                    };
                    Ok(filled.map_or(JsValue::Undefined, JsValue::String))
                }),
            ),
            (
                "__ljos_argNumber",
                host(|_, a| {
                    Ok(JsValue::String(match a.first() {
                        Some(JsValue::Number(n)) => ljos_i18n::arg::number(*n),
                        _ => String::new(),
                    }))
                }),
            ),
            (
                "__ljos_uiLanguage",
                host(|e, _| {
                    let state = e.i18n();
                    e.i18n_track(&state.language);
                    Ok(JsValue::String(e.ui_language()))
                }),
            ),
            (
                "__ljos_setUiLanguage",
                host(|e, a| {
                    e.set_ui_language(string(a.first()));
                    Ok(JsValue::Undefined)
                }),
            ),
            (
                "__ljos_rightToLeft",
                host(|e, _| {
                    let state = e.i18n();
                    e.i18n_track(&state.language);
                    Ok(JsValue::Bool(e.is_right_to_left()))
                }),
            ),
            (
                "__ljos_locale",
                host(|e, a| {
                    Ok(e.locale_object(match a.first() {
                        Some(JsValue::String(name)) => Some(name.as_str()),
                        // Anything else that is not "no argument" names no
                        // locale, which is the C locale.
                        Some(JsValue::Undefined) | None => None,
                        Some(_) => Some(""),
                    }))
                }),
            ),
        ];
        let err = |e: JsFault| EngineError::Js(e.to_string());
        for (name, f) in functions {
            js.host.set_global_function(name, f).map_err(err)?;
        }
        js.host.eval_script("i18n", PRELUDE).map_err(err)?;
        Ok(())
    }
}

/// The JS face of localisation. The argument checks are here because their
/// messages and error types are part of what a QML author sees.
const PRELUDE: &str = r##"
(function () {
  "use strict";
  const g = globalThis;
  const tr = g.__ljos_tr, trId = g.__ljos_trId, fill = g.__ljos_arg, plainNumber = g.__ljos_argNumber;
  const uiLanguage = g.__ljos_uiLanguage, setUiLanguage = g.__ljos_setUiLanguage;
  const rightToLeft = g.__ljos_rightToLeft, hostLocale = g.__ljos_locale;
  const hostLog = g.__ljos_log;
  const define = (o, k, value) => Object.defineProperty(o, k, { value, writable: true, configurable: true });

  // qsTr for the texts of one file: the context is the file's name
  // (or its `pragma Translator`), which only the compiler knows, so a
  // compiled file that translates binds its own.
  const qsTrIn = (context) => function qsTr(text, disambiguation, n) {
    if (arguments.length < 1) throw new Error("qsTr() requires at least one argument");
    if (typeof text !== "string") throw new Error("qsTr(): first argument (sourceText) must be a string");
    if (arguments.length > 1 && typeof disambiguation !== "string")
      throw new Error("qsTr(): second argument (disambiguation) must be a string");
    if (arguments.length > 2 && typeof n !== "number")
      throw new Error("qsTr(): third argument (n) must be a number");
    return tr(context, text, disambiguation, n);
  };
  define(g, "__ljos_qsTr", qsTrIn);
  define(g, "qsTr", qsTrIn(""));
  define(g, "qsTranslate", function qsTranslate(context, text, disambiguation, n) {
    if (arguments.length < 2) throw new Error("qsTranslate() requires at least two arguments");
    if (typeof context !== "string") throw new Error("qsTranslate(): first argument (context) must be a string");
    if (typeof text !== "string") throw new Error("qsTranslate(): second argument (sourceText) must be a string");
    if (arguments.length > 2 && typeof disambiguation !== "string")
      throw new Error("qsTranslate(): third argument (disambiguation) must be a string");
    return tr(context, text, disambiguation, typeof n === "number" ? n : undefined);
  });
  define(g, "qsTrId", function qsTrId(id, n) {
    if (arguments.length < 1) throw new Error("qsTrId() requires at least one argument");
    if (typeof id !== "string") throw new TypeError("qsTrId(): first argument (id) must be a string");
    if (arguments.length > 1 && typeof n !== "number")
      throw new TypeError("qsTrId(): second argument (n) must be a number");
    return trId(id, n);
  });
  // The markers only tell the extractor about a text.
  define(g, "QT_TR_NOOP", (text) => text);
  define(g, "QT_TRANSLATE_NOOP", (context, text) => text);
  define(g, "QT_TRID_NOOP", (id) => id);

  // String.prototype.arg: fills every marker with the lowest number, as
  // QString::arg does. A number is written as QString::arg(double) writes it.
  const show = (v) => {
    if (typeof v === "string") return v;
    if (typeof v === "boolean") return v ? "1" : "0";
    try { return String(v); } catch (e) { return "[object]"; }
  };
  define(String.prototype, "arg", function (value) {
    if (arguments.length !== 1) throw new Error("String.arg(): Invalid arguments");
    const template = String(this);
    const number = typeof value === "number";
    const filled = fill(template, number ? value : show(value));
    if (filled !== undefined) return filled;
    hostLog(2, "String.arg(): Argument missing: " + JSON.stringify(template) + ", "
      + JSON.stringify(number ? plainNumber(value) : show(value)));
    return template;
  });

  const Qt = g.Qt;
  Object.defineProperty(Qt, "uiLanguage", {
    get() { return uiLanguage(); },
    set(v) { setUiLanguage(String(v)); },
    enumerable: true, configurable: true,
  });
  define(Qt, "locale", function locale(name) {
    if (arguments.length > 1) throw new Error("Too many arguments");
    return Object.freeze(hostLocale(name));
  });
  if (Qt.application === undefined) define(Qt, "application", {});
  Object.defineProperty(Qt.application, "layoutDirection", {
    get() { return rightToLeft() ? Qt.RightToLeft : Qt.LeftToRight; },
    enumerable: true, configurable: true,
  });
})();
"##;
