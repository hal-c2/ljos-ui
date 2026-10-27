//! Small things every part of the crate uses.

use ljos_engine::{Engine, EngineError, LogLevel, ObjectId, PropIndex, Provenance, QmlType, Value};

/// The category of this crate's log lines.
pub const CATEGORY: &str = "qt.quick.controls";

/// Whether nothing has given the property a value: neither the author nor
/// a binding. A value this crate derived for it does not count.
pub(crate) fn is_unset(engine: &Engine, obj: ObjectId, p: PropIndex) -> bool {
    matches!(
        engine.provenance(obj, p),
        Some(Provenance::Default | Provenance::Animated | Provenance::Host) | None
    )
}

/// Reads a number (tracked).
pub(crate) fn num(engine: &Engine, obj: ObjectId, p: PropIndex) -> f64 {
    engine.read(obj, p).to_number()
}

/// Reads a boolean (tracked).
pub(crate) fn flag(engine: &Engine, obj: ObjectId, p: PropIndex) -> bool {
    engine.read(obj, p).to_boolean()
}

/// Reads an object reference (tracked); a destroyed object reads as none.
pub(crate) fn object(engine: &Engine, obj: ObjectId, p: PropIndex) -> Option<ObjectId> {
    engine
        .read(obj, p)
        .as_object()
        .filter(|o| engine.is_alive(*o))
}

/// Whether the object's type derives from `T`.
pub(crate) fn is_a<T: QmlType>(engine: &Engine, obj: ObjectId) -> bool {
    engine
        .meta_of(obj)
        .is_some_and(|m| m.inherits(T::static_meta()))
}

pub(crate) fn warn(engine: &Engine, what: impl Into<String>) {
    engine.log(LogLevel::Warning, CATEGORY, what.into());
}

/// Emits `signal` of `obj`, which takes no arguments, if `obj` is still
/// alive.
pub(crate) fn emit(engine: &Engine, obj: ObjectId, signal: &str) {
    if engine.is_alive(obj) {
        check(engine, signal, engine.emit_by_name(obj, signal, &[]));
    }
}

/// Logs what went wrong where a hook cannot return it.
pub(crate) fn check(engine: &Engine, what: &str, r: Result<(), EngineError>) {
    if let Err(err) = r {
        warn(engine, format!("{what}: {err}"));
    }
}

/// Makes `prop` follow `f` unless it was given a value. Assigning the
/// property later replaces the binding, as assigning any bound property
/// does.
pub(crate) fn bind_default(
    engine: &Engine,
    obj: ObjectId,
    prop: PropIndex,
    f: impl Fn(&Engine) -> Value + 'static,
) -> Result<(), EngineError> {
    engine.bind_default(obj, prop, f)
}

/// Writes a value this crate computed, unless it is the value already.
pub(crate) fn set_computed(
    engine: &Engine,
    obj: ObjectId,
    prop: PropIndex,
    v: Value,
) -> Result<(), EngineError> {
    if engine.untrack(|| engine.read(obj, prop)).change_equals(&v) {
        return Ok(());
    }
    engine.write_internal(obj, prop, v, ljos_engine::WriteKind::Imperative)
}

/// The text a control draws for `text`: an `&` marks the letter after it
/// as the mnemonic and is not drawn, and `&&` is one literal `&`. Measured
/// on qml6 for longer runs: an `&` is dropped unless another `&` comes right
/// before it, so `a&&&b` draws as `a&&b` (pinned by
/// `features/controls/mnemonics.feature`).
pub(crate) fn without_mnemonic(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut after_ampersand = false;
    for c in text.chars() {
        if c != '&' || after_ampersand {
            out.push(c);
        }
        after_ampersand = c == '&';
    }
    out
}

/// The name an assistive technology is given for a button's `text`. It
/// reads pairs, not what is drawn: `&&` is one `&`, an `&` before any
/// other character goes, and a last `&` stays, so `a&&&b` is `a&b` and
/// `Trail&` is `Trail&` (measured on qml6 through AT-SPI; pinned by
/// `features/controls/accessibility.feature`).
pub(crate) fn spoken_without_mnemonic(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            match chars.next() {
                Some(next) => out.push(next),
                None => out.push('&'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// What [`without_mnemonic`] draws, as `StyledText` with the letter after
/// the first dropped `&` underlined (qml6 underlines it before Alt is
/// pressed too), or `None` when no letter is.
pub(crate) fn mnemonic_underlined(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len() + 8);
    let mut after_ampersand = false;
    let mut dropped = false;
    let mut marked = false;
    for c in text.chars() {
        if c == '&' && !after_ampersand {
            after_ampersand = true;
            dropped = true;
            continue;
        }
        let underline = dropped && !marked && c != '&' && !c.is_whitespace();
        marked |= underline;
        dropped = false;
        after_ampersand = c == '&';
        if underline {
            out.push_str("<u>");
        }
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("<br>"),
            c => out.push(c),
        }
        if underline {
            out.push_str("</u>");
        }
    }
    marked.then_some(out)
}

/// The mnemonic of `text`: the letter after the first single `&`,
/// lower-cased, so that Alt+that letter activates the control.
pub(crate) fn mnemonic_of(text: &str) -> Option<char> {
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '&' {
            continue;
        }
        match chars.next() {
            Some('&') => {}
            Some(m) if !m.is_whitespace() => return m.to_lowercase().next(),
            _ => return None,
        }
    }
    None
}

/// Calls `f` when the object `prop` names becomes a model after `obj`
/// took it: completion runs last-made-first, so a model declared before
/// `obj` makes itself one only after `obj` completed. A new `prop` value
/// is left to `prop`'s own watch.
pub(crate) fn on_model_made(
    engine: &Engine,
    obj: ObjectId,
    prop: PropIndex,
    f: impl Fn(&Engine) + 'static,
) -> Result<(), EngineError> {
    let mut seen: Option<(Value, u64)> = None;
    engine.effect_in(obj, move |e| {
        let model = e.read(obj, prop);
        let generation = e.track_model(&model);
        let made = seen
            .as_ref()
            .is_some_and(|(v, g)| v.change_equals(&model) && *g != generation);
        seen = Some((model, generation));
        if made {
            e.untrack(|| f(e));
        }
    })?;
    Ok(())
}

#[cfg(test)]
mod mnemonic_tests {
    use super::*;

    #[test]
    fn an_ampersand_marks_a_letter_and_two_make_one() {
        assert_eq!(without_mnemonic("&File"), "File");
        assert_eq!(without_mnemonic("A&&B"), "A&B");
        assert_eq!(without_mnemonic("a&&&b"), "a&&b");
        assert_eq!(without_mnemonic("&"), "");
        assert_eq!(without_mnemonic("Save &as"), "Save as");
    }

    #[test]
    fn a_spoken_name_reads_the_ampersands_in_pairs() {
        assert_eq!(spoken_without_mnemonic("&Save"), "Save");
        assert_eq!(spoken_without_mnemonic("Copy && Paste"), "Copy & Paste");
        assert_eq!(spoken_without_mnemonic("a&&&b"), "a&b");
        assert_eq!(spoken_without_mnemonic("Trail&"), "Trail&");
        assert_eq!(spoken_without_mnemonic(""), "");
    }

    #[test]
    fn the_mnemonic_is_the_letter_after_a_single_ampersand() {
        assert_eq!(mnemonic_of("&File"), Some('f'));
        assert_eq!(mnemonic_of("A&&B"), None);
        assert_eq!(mnemonic_of("A&&&b"), Some('b'));
        assert_eq!(mnemonic_of("File"), None);
    }
}
