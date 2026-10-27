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

use ljos_i18n::datetime::{Fields, Part};
use ljos_i18n::{Collator, I18n, LocaleInfo, Messages};
use ljos_ir::{Translatable, TranslationKind};
use ljos_js::{HostError, HostFn, JsValue};

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
    /// The collator `localeCompare` used last, and the locale name it was
    /// asked for (`None`: the application's). A sort asks for the same one
    /// for every pair.
    collator: RefCell<Option<(Option<String>, Rc<Collator>)>>,
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

/// The locale a host call names: a name, or the application's locale when
/// it names none.
fn locale_arg(v: Option<&JsValue>) -> Option<&str> {
    match v {
        Some(JsValue::String(name)) => Some(name),
        _ => None,
    }
}

fn number(v: Option<&JsValue>) -> f64 {
    match v {
        Some(JsValue::Number(n)) => *n,
        _ => f64::NAN,
    }
}

/// `Locale.LongFormat` is the long width; `ShortFormat`, `NarrowFormat`
/// and anything else is the short one, as a locale's formats have no
/// narrow kind.
fn long_format(v: Option<&JsValue>) -> bool {
    number(v) == 0.0
}

fn part(v: Option<&JsValue>) -> Part {
    match number(v) {
        0.0 => Part::Date,
        1.0 => Part::Time,
        _ => Part::DateTime,
    }
}

impl Engine {
    /// Re-runs every binding that produced a translated string (`qsTr`),
    /// after the host changed the language.
    pub fn retranslate(&self) {
        let n = self.i18n_node(&self.inner.translation);
        let g = &self.inner.graph;
        let next = match g.untrack(|| g.try_get(n)) {
            Some(Value::Int(i)) => i.wrapping_add(1),
            _ => 1,
        };
        let _ = g.write(n, Value::Int(next), WriteKind::Initial);
    }

    /// Makes the running binding depend on the language.
    pub fn track_translation(&self) {
        self.i18n_track(&self.inner.translation);
    }

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
        self.set_locale_info(LocaleInfo::new(name));
    }

    /// Sets the application's locale, whose categories may differ.
    pub fn set_locale_info(&self, locale: LocaleInfo) {
        let state = self.i18n();
        let changed = state.i18n.borrow_mut().set_locale_info(locale);
        if changed {
            self.i18n_bump(&state.locale);
            // `%Ln` in a translated text is written by the locale.
            self.retranslate();
        }
    }

    /// The application's locale (`Qt.locale()`). Tracked: a binding or an
    /// effect that reads it runs again when it changes.
    pub fn locale(&self) -> LocaleInfo {
        self.locale_info(None)
    }

    /// Calls `f` with the application's locale whenever it changes (not
    /// now), until the returned node is disposed.
    pub fn on_locale_changed(
        &self,
        f: impl Fn(&Engine, &LocaleInfo) + 'static,
    ) -> Result<NodeId, EngineError> {
        let mut first = true;
        self.effect(move |e| {
            let locale = e.locale();
            if std::mem::take(&mut first) {
                return;
            }
            e.untrack(|| f(e, &locale));
        })
    }

    /// Takes the locale and the UI language from the environment, as an
    /// application does when it starts. The engine itself starts in `en`
    /// and `en_US` whatever the machine, so that tests do not depend on it.
    pub fn use_system_locale(&self) {
        self.use_system_locale_from(&ljos_i18n::SystemLocale::from_env());
    }

    /// Takes the locale and the UI language from what an environment
    /// names for each category ([`ljos_i18n::SystemLocale`]): the locale
    /// is named by its messages, writes numbers, dates and times by its
    /// own categories, and the UI language is the messages locale's. A
    /// UI language of the `C` locale leaves the UI language as it was.
    pub fn use_system_locale_from(&self, system: &ljos_i18n::SystemLocale) {
        self.set_locale_info(system.locale_info());
        if let Some(language) = system.ui_language() {
            self.set_ui_language(&language);
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
    pub(crate) fn arg_number(&self, template: &str, value: f64) -> Option<String> {
        let state = self.i18n();
        if template.contains("%L") {
            self.i18n_track(&state.locale);
        }
        let (plain, localized) = ljos_i18n::arg::number_pair(value, state.i18n.borrow().locale());
        ljos_i18n::arg::arg(template, &plain, &localized)
    }

    /// The members of `Locale` that M1 has (ARCHITECTURE.md §15): the name,
    /// the signs numbers are written with, and the direction.
    fn locale_info(&self, name: Option<&str>) -> LocaleInfo {
        let state = self.i18n();
        match name {
            Some(name) => LocaleInfo::new(name),
            None => {
                self.i18n_track(&state.locale);
                state.i18n.borrow().locale().clone().as_application()
            }
        }
    }

    /// What `localeCompare` orders by: the application's collation
    /// (tracked), or that of the locale `name` names.
    fn collator(&self, name: Option<&str>) -> Rc<Collator> {
        let state = self.i18n();
        if name.is_none() {
            self.i18n_track(&state.locale);
        }
        let i18n = state.i18n.borrow();
        let mut last = state.collator.borrow_mut();
        if let Some((key, c)) = last.as_ref() {
            let hit = match (key.as_deref(), name) {
                (Some(k), Some(n)) => k == n,
                (None, None) => c.locale() == i18n.locale().collation_locale(),
                _ => false,
            };
            if hit {
                return Rc::clone(c);
            }
        }
        let c = Rc::new(match name {
            Some(n) => Collator::new(LocaleInfo::new(n).collation_locale()),
            None => Collator::new(i18n.locale().collation_locale()),
        });
        *last = Some((name.map(str::to_owned), Rc::clone(&c)));
        c
    }

    fn locale_object(&self, name: Option<&str>) -> JsValue {
        let info = self.locale_info(name);
        let s = |v: String| JsValue::String(v);
        let (am, pm) = info.am_pm();
        let n = |v: u8| JsValue::Number(f64::from(v));
        JsValue::Map(vec![
            ("amText".into(), s(am)),
            ("pmText".into(), s(pm)),
            ("measurementSystem".into(), n(info.measurement_system())),
            (
                "weekDays".into(),
                JsValue::Array(info.week_days().into_iter().map(n).collect()),
            ),
            ("percent".into(), s("%".to_owned())),
            ("exponential".into(), s("e".to_owned())),
            ("name".into(), s(info.name().to_owned())),
            ("decimalPoint".into(), s(info.decimal_point())),
            ("groupSeparator".into(), s(info.group_separator())),
            ("zeroDigit".into(), s(info.zero_digit())),
            ("negativeSign".into(), s(info.negative_sign())),
            ("positiveSign".into(), s("+".to_owned())),
            // `Qt.DayOfWeek`: Sunday is 0.
            (
                "firstDayOfWeek".into(),
                JsValue::Number(f64::from(info.first_day_of_week() % 7)),
            ),
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
        let functions: [(&str, HostFn); 15] = [
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
            (
                "__ljos_localeName",
                // (locale, month?, index, width, standalone?): a day (0 is
                // Sunday, as `Locale.dayName` counts) or a month (0 is
                // January) written by the locale.
                host(|e, a| {
                    let info = e.locale_info(locale_arg(a.first()));
                    let index = number(a.get(2));
                    let index = if index.is_finite() { index as i64 } else { -1 };
                    let width = match number(a.get(3)) {
                        0.0 => ljos_i18n::calendar::NameWidth::Long,
                        2.0 => ljos_i18n::calendar::NameWidth::Narrow,
                        _ => ljos_i18n::calendar::NameWidth::Short,
                    };
                    let standalone = matches!(a.get(4), Some(JsValue::Bool(true)));
                    let text = if matches!(a.get(1), Some(JsValue::Bool(true))) {
                        let month = u8::try_from(index).unwrap_or(u8::MAX);
                        if standalone {
                            info.standalone_month_name(month, width)
                        } else {
                            info.month_name(month, width)
                        }
                    } else {
                        // `Qt::DayOfWeek` numbers Sunday 7.
                        let day = match index {
                            0 => 7,
                            d => u8::try_from(d).unwrap_or(u8::MAX),
                        };
                        if standalone {
                            info.standalone_day_name(day, width)
                        } else {
                            info.day_name(day, width)
                        }
                    };
                    Ok(JsValue::String(text))
                }),
            ),
            (
                "__ljos_localeFormat",
                // (locale, part, format): `Locale.dateFormat` and its family.
                host(|e, a| {
                    let info = e.locale_info(locale_arg(a.first()));
                    let long = long_format(a.get(2));
                    Ok(JsValue::String(match part(a.get(1)) {
                        Part::Date => info.date_format(long),
                        Part::Time => info.time_format(long),
                        Part::DateTime => info.date_time_format(long),
                    }))
                }),
            ),
            (
                "__ljos_formatDate",
                // (locale, pattern, part, year, month, day, hour, minute,
                // second, millisecond, offset): local fields, as the
                // script's `Date` has them.
                host(|e, a| {
                    let info = e.locale_info(locale_arg(a.first()));
                    let field = |i: usize| number(a.get(i));
                    let fields = Fields {
                        year: field(3) as i32,
                        month: field(4) as u32,
                        day: field(5) as u32,
                        hour: field(6) as u32,
                        minute: field(7) as u32,
                        second: field(8) as u32,
                        millisecond: field(9) as u32,
                        offset: field(10) as i32,
                    };
                    Ok(JsValue::String(info.format_date_time(
                        string(a.get(1)),
                        &fields,
                        part(a.get(2)),
                    )))
                }),
            ),
            (
                "__ljos_parseDate",
                // (locale, pattern, text, part, year, month, day): the
                // fields read, a time's on the day given, and the offset
                // they are at, null when neither the text nor the platform
                // names a zone; null when the text does not fit.
                host(|e, a| {
                    let info = e.locale_info(locale_arg(a.first()));
                    let part = part(a.get(3));
                    let Some(mut p) =
                        info.parse_date_time(string(a.get(1)), string(a.get(2)), part)
                    else {
                        return Ok(JsValue::Null);
                    };
                    if part == Part::Time {
                        p.fields.year = number(a.get(4)) as i32;
                        p.fields.month = number(a.get(5)) as u32;
                        p.fields.day = number(a.get(6)) as u32;
                    }
                    let offset = p.resolved_offset();
                    let f = p.fields;
                    let n = |v: f64| JsValue::Number(v);
                    Ok(JsValue::Array(vec![
                        n(f64::from(f.year)),
                        n(f64::from(f.month)),
                        n(f64::from(f.day)),
                        n(f64::from(f.hour)),
                        n(f64::from(f.minute)),
                        n(f64::from(f.second)),
                        n(f64::from(f.millisecond)),
                        offset.map_or(JsValue::Null, |o| n(f64::from(o))),
                    ]))
                }),
            ),
            (
                "__ljos_formatDataSize",
                // (locale, bytes, precision, format)
                host(|e, a| {
                    let info = e.locale_info(locale_arg(a.first()));
                    let bytes = number(a.get(1));
                    let bytes = if bytes.is_finite() { bytes as i64 } else { 0 };
                    let precision = number(a.get(2));
                    let precision = if precision.is_finite() {
                        precision as i32
                    } else {
                        2
                    };
                    let format = number(a.get(3));
                    let format = if format.is_finite() { format as i32 } else { 0 };
                    Ok(JsValue::String(info.format_data_size(
                        bytes,
                        precision,
                        ljos_i18n::DataSizeFormat::from_qt(format),
                    )))
                }),
            ),
            (
                "__ljos_localeCompare",
                // (text, other, locale)
                host(|e, a| {
                    let order = e
                        .collator(locale_arg(a.get(2)))
                        .order(string(a.first()), string(a.get(1)));
                    Ok(JsValue::Number(f64::from(order)))
                }),
            ),
            (
                "__ljos_formatNumber",
                // (locale, value, format, precision)
                host(|e, a| {
                    let info = e.locale_info(locale_arg(a.first()));
                    let format = string(a.get(2)).chars().next().unwrap_or('g');
                    let precision = number(a.get(3));
                    let precision = if precision.is_finite() && precision >= 0.0 {
                        precision as usize
                    } else {
                        6
                    };
                    Ok(JsValue::String(info.format_double(
                        number(a.get(1)),
                        format,
                        precision.min(99),
                    )))
                }),
            ),
        ];
        for (name, f) in functions {
            js.host
                .set_global_function(name, f)
                .map_err(EngineError::js)?;
        }
        js.host
            .eval_script("i18n", PRELUDE)
            .map_err(EngineError::js)?;
        Ok(())
    }
}

/// The JS face of localisation. The argument checks are here because their
/// messages and error types are part of what a QML author sees.
pub(crate) const PRELUDE: &str = r##"
(function () {
  "use strict";
  const g = globalThis;
  const tr = g.__ljos_tr, trId = g.__ljos_trId, fill = g.__ljos_arg, plainNumber = g.__ljos_argNumber;
  const uiLanguage = g.__ljos_uiLanguage, setUiLanguage = g.__ljos_setUiLanguage;
  const rightToLeft = g.__ljos_rightToLeft, hostLocale = g.__ljos_locale;
  const localeName = g.__ljos_localeName, formatDataSize = g.__ljos_formatDataSize;
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
  const localeFormat = g.__ljos_localeFormat, formatDate = g.__ljos_formatDate;
  const parseDate = g.__ljos_parseDate, formatNumber = g.__ljos_formatNumber;
  // What makes an object a locale, and the name it was made for
  // (undefined for the application's, which follows it when it changes).
  const LOCALE = Symbol("Locale");
  const isLocale = (v) => v !== null && typeof v === "object" && LOCALE in v;
  const nameOf = (l) => l[LOCALE];
  // Locale.LongFormat is 0; the formats of a date default to it.
  const LONG = 0, SHORT = 1;
  const DATE = 0, TIME = 1, BOTH = 2;

  define(Qt, "locale", function locale(name) {
    if (arguments.length > 1) throw new Error("Too many arguments");
    const o = hostLocale(name);
    const width = (format) => format === undefined ? LONG : format;
    define(o, "dayName", (d, f) => localeName(name, false, d, width(f), false));
    define(o, "standaloneDayName", (d, f) => localeName(name, false, d, width(f), true));
    define(o, "monthName", (m, f) => localeName(name, true, m, width(f), false));
    define(o, "standaloneMonthName", (m, f) => localeName(name, true, m, width(f), true));
    define(o, "dateFormat", (f) => localeFormat(name, DATE, width(f)));
    define(o, "timeFormat", (f) => localeFormat(name, TIME, width(f)));
    define(o, "dateTimeFormat", (f) => localeFormat(name, BOTH, width(f)));
    define(o, "formattedDataSize", (bytes, precision, format) => formatDataSize(name, Number(bytes),
      precision === undefined ? 2 : Number(precision), format === undefined ? 0 : Number(format)));
    define(o, "toString", function toString(value, format, precision) {
      return localeToString(name, this, arguments.length, value, format, precision);
    });
    define(o, LOCALE, name);
    return Object.freeze(o);
  });

  // Locale.toString: a whole number in int's range as an int, any other
  // number as a double ('g', six digits), a date in a format of the locale
  // or a format string. A format that is not one of the letters is 'f'.
  const INT_MIN = -2147483648, INT_MAX = 2147483647;
  const localeToString = (name, self, count, value, format, precision) => {
    // Without anything to write it is the object's own text.
    if (count === 0 || value === undefined) return Object.prototype.toString.call(self);
    if (count > 3) throw new Error("Locale: toString(): Expected 1-3 arguments, but received " + count);
    if (value instanceof Date) {
      if (count > 2)
        throw new Error("Locale: the toString() overload that takes a Date as its first argument "
          + "expects 1 or 2 arguments, but received " + count);
      if (format === undefined) format = LONG;
      if (typeof format !== "string" && typeof format !== "number")
        throw new Error("Locale: the second argument to the toString overloads whose first argument "
          + "is a Date should be a string or FormatType");
      if (isNaN(value.getTime())) return "";
      const pattern = typeof format === "string" ? format : localeFormat(name, BOTH, format);
      return write(value, name, pattern, BOTH);
    }
    if (typeof value === "boolean") value = value ? 1 : 0;
    if (typeof value !== "number")
      throw new Error("Locale: toString() expects either an int, double, or Date as its first argument");
    if (format === undefined && Number.isInteger(value) && value >= INT_MIN && value <= INT_MAX)
      return formatNumber(name, value === 0 ? 0 : value, "f", 0);
    if (format === "") return "";
    const letter = format === undefined ? "g"
      : typeof format === "string" && format.length === 1 && "eEfgG".includes(format) ? format : "f";
    return formatNumber(name, value, letter, precision === undefined ? 6 : Number(precision));
  };

  const write = (d, name, pattern, part) => formatDate(name, pattern, part,
    d.getFullYear(), d.getMonth() + 1, d.getDate(), d.getHours(), d.getMinutes(),
    d.getSeconds(), d.getMilliseconds(), -d.getTimezoneOffset() * 60);

  // Date.prototype.toLocaleDateString and its family, as Qt has them: a
  // locale and a Locale.FormatType or a format string; a locale alone is
  // its long format; nothing at all the application's long format, and
  // anything that is not a locale its short one.
  const toLocale = (part) => function (locale, format) {
    if (isNaN(this.getTime())) return "";
    if (arguments.length === 0) return write(this, undefined, localeFormat(undefined, part, LONG), part);
    if (!isLocale(locale)) return write(this, undefined, localeFormat(undefined, part, SHORT), part);
    const name = nameOf(locale);
    const pattern = typeof format === "string" ? format
      : localeFormat(name, part, typeof format === "number" ? format : LONG);
    return write(this, name, pattern, part);
  };
  define(Date.prototype, "toLocaleDateString", toLocale(DATE));
  define(Date.prototype, "toLocaleTimeString", toLocale(TIME));
  define(Date.prototype, "toLocaleString", toLocale(BOTH));

  // Date.fromLocaleDateString and its family. What does not fit the format
  // is an Invalid Date, as in Qt.
  const fromLocale = (part, fn) => function (locale, text, format) {
    if (arguments.length === 0 || (!isLocale(locale) && typeof locale !== "string"))
      throw new Error("Locale: Date." + fn + "(): Invalid arguments");
    if (!isLocale(locale)) return new Date(NaN);
    const name = nameOf(locale);
    const pattern = typeof format === "string" ? format
      : localeFormat(name, part, typeof format === "number" ? format : LONG);
    const today = new Date();
    const read = parseDate(name, pattern, String(text), part,
      today.getFullYear(), today.getMonth() + 1, today.getDate());
    if (read === null) return new Date(NaN);
    const [year, month, day, hours, minutes, seconds, ms, offset] = read;
    // setFullYear, because the Date constructor reads 0 to 99 as 1900 to 1999.
    const out = new Date(0);
    if (offset === null) {
      out.setFullYear(year, month - 1, day);
      out.setHours(hours, minutes, seconds, ms);
      return out;
    }
    out.setUTCFullYear(year, month - 1, day);
    out.setUTCHours(hours, minutes, seconds, ms);
    return new Date(out.getTime() - offset * 1000);
  };
  define(Date, "fromLocaleDateString", fromLocale(DATE, "fromLocaleDateString"));
  define(Date, "fromLocaleTimeString", fromLocale(TIME, "fromLocaleTimeString"));
  define(Date, "fromLocaleString", fromLocale(BOTH, "fromLocaleString"));

  // Qt.formatDateTime and its family: a Qt.DateFormat, a format string, or
  // a locale and a Locale.FormatType; nothing is the application's short
  // format. The Qt.DateFormat forms are written in English whatever the
  // locale.
  const NAMED = {
    0: ["ddd MMM d yyyy", "HH:mm:ss", "ddd MMM d HH:mm:ss yyyy"],
    1: ["yyyy-MM-dd", "HH:mm:ss", "yyyy-MM-ddTHH:mm:ss"],
    8: ["dd MMM yyyy", "HH:mm:ss", "dd MMM yyyy HH:mm:ss tt"],
    9: ["yyyy-MM-dd", "HH:mm:ss.zzz", "yyyy-MM-ddTHH:mm:ss.zzz"],
  };
  const qtFormat = (part) => function (date, format, type) {
    const d = date instanceof Date ? date : new Date(date);
    if (isNaN(d.getTime())) return "";
    if (format === undefined) return write(d, undefined, localeFormat(undefined, part, SHORT), part);
    if (typeof format === "string") return write(d, undefined, format, part);
    if (isLocale(format)) {
      const name = nameOf(format);
      return write(d, name, localeFormat(name, part, typeof type === "number" ? type : SHORT), part);
    }
    const named = NAMED[format];
    return named === undefined ? "" : write(d, "C", named[part], part);
  };
  define(Qt, "formatDate", qtFormat(DATE));
  define(Qt, "formatTime", qtFormat(TIME));
  define(Qt, "formatDateTime", qtFormat(BOTH));

  // Number.prototype.toLocaleString(locale, format, precision): `f` with
  // two decimals unless told otherwise. Without arguments it is `%g` in
  // the application's locale; with something that is not a locale, the
  // number as JS writes it.
  const numberValue = (n) => Number.prototype.valueOf.call(n);
  const toLocaleNumber = function (locale, format, precision) {
    const v = numberValue(this);
    if (arguments.length === 0) return formatNumber(undefined, v, "g", 6);
    if (!isLocale(locale)) return String(v);
    return formatNumber(nameOf(locale), v, typeof format === "string" && format !== "" ? format : "f",
      typeof precision === "number" ? precision : 2);
  };
  define(Number.prototype, "toLocaleString", toLocaleNumber);
  // The currency symbol goes before the amount. Without a symbol it is
  // empty: Ljós has no locale's currency yet.
  define(Number.prototype, "toLocaleCurrencyString", function (locale, symbol) {
    if (arguments.length === 0 || !isLocale(locale)) return toLocaleNumber.call(this);
    const amount = formatNumber(nameOf(locale), numberValue(this), "f", 2);
    return (typeof symbol === "string" ? symbol : "") + amount;
  });

  // String.prototype.localeCompare(that, locale): by the application's
  // collation, or by that of the locale the second argument is or names,
  // which Qt does not take.
  const localeCompare = g.__ljos_localeCompare;
  define(String.prototype, "localeCompare", function (that, locale) {
    if (this === null || this === undefined)
      throw new TypeError("String.prototype.localeCompare called on null or undefined");
    const name = isLocale(locale) ? nameOf(locale) : typeof locale === "string" ? locale : undefined;
    return localeCompare(String(this), String(that), name);
  });

  if (Qt.application === undefined) define(Qt, "application", {});
  Object.defineProperty(Qt.application, "layoutDirection", {
    get() { return rightToLeft() ? Qt.RightToLeft : Qt.LeftToRight; },
    enumerable: true, configurable: true,
  });
})();
"##;
