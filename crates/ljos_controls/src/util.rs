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

/// Whether the author gave the property a value (assigned or bound). What
/// this crate writes does not count: it writes with
/// [`Engine::write_derived`].
pub(crate) fn author_set(engine: &Engine, obj: ObjectId, p: PropIndex) -> bool {
    matches!(
        engine.provenance(obj, p),
        Some(Provenance::Assigned | Provenance::Bound)
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

/// Writes a value as the type's own code does: a binding on the property
/// stays, as `open()` leaves a `visible:` binding alone in Qt.
pub(crate) fn set_derived(
    engine: &Engine,
    obj: ObjectId,
    prop: PropIndex,
    v: Value,
) -> Result<(), EngineError> {
    if engine.untrack(|| engine.read(obj, prop)).change_equals(&v) {
        return Ok(());
    }
    engine.write_derived(obj, prop, v)
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
    let mut chars = text.chars();
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

/// What [`without_mnemonic`] draws, as `StyledText` with the character
/// after the first dropped `&` underlined (qml6 underlines it before Alt is
/// pressed too), or `None` when none is. Only the first dropped `&` marks
/// one, and a space is marked as a letter is; when an `&` follows it, as
/// in `a&&&b`, nothing is underlined although Alt+B presses the control
/// (see [`mnemonic_of`]; measured on qml6, pinned by
/// `features/controls/mnemonics.feature`).
pub(crate) fn mnemonic_underlined(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len() + 8);
    let mut after_ampersand = false;
    let mut dropped = false;
    let mut first_dropped = true;
    let mut marked = false;
    for c in text.chars() {
        if c == '&' && !after_ampersand {
            after_ampersand = true;
            dropped = true;
            continue;
        }
        let underline = dropped && first_dropped && c != '&' && c != '\n';
        marked |= underline;
        first_dropped &= !dropped;
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
            // StyledText folds a space it does not keep, as one that starts
            // the text; an underlined one is drawn.
            ' ' if underline => out.push_str("&nbsp;"),
            c => out.push(c),
        }
        if underline {
            out.push_str("</u>");
        }
    }
    marked.then_some(out)
}

/// The mnemonic of `text`: the character after the first `&` that is not
/// one of a pair, lower-cased, so that Alt+that character activates the
/// control. It reads pairs, not what is drawn, so `a&&&b` has `b`; and a
/// space is a mnemonic as a letter is, so `a& b &c` has the space, not
/// `c` (measured on qml6, pinned by `features/controls/mnemonics.feature`).
pub(crate) fn mnemonic_of(text: &str) -> Option<char> {
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '&' {
            continue;
        }
        match chars.next() {
            Some('&') => {}
            Some(m) => return m.to_lowercase().next(),
            None => return None,
        }
    }
    None
}

/// The key sequence that presses a control whose mnemonic is `c`.
pub(crate) fn mnemonic_sequence(c: char) -> String {
    match c {
        ' ' => "Alt+Space".to_owned(),
        c => format!("Alt+{}", c.to_uppercase()),
    }
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
        assert_eq!(mnemonic_of("a& b &c"), Some(' '));
        assert_eq!(mnemonic_of("Trail&"), None);
        assert_eq!(mnemonic_sequence(' '), "Alt+Space");
        assert_eq!(mnemonic_sequence('f'), "Alt+F");
    }

    #[test]
    fn only_the_first_dropped_ampersand_underlines() {
        assert_eq!(mnemonic_underlined("&File").as_deref(), Some("<u>F</u>ile"));
        assert_eq!(
            mnemonic_underlined("a& b &c").as_deref(),
            Some("a<u>&nbsp;</u>b c")
        );
        assert_eq!(mnemonic_underlined("a&&&b"), None);
        assert_eq!(mnemonic_underlined("a&&&b &H"), None);
        assert_eq!(mnemonic_underlined("A&&B"), None);
    }
}
