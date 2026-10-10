//! `Shortcut` (QtQuick, doc.qt.io/qt-6/qml-qtquick-shortcut.html): a key
//! sequence that acts wherever focus is.
//!
//! Every Shortcut of an engine is in one list. A press is matched against
//! the sequences of the enabled ones whose context covers the window;
//! nothing is matched, parsed or looked at until a key arrives.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use ljos_engine::{Cx, Engine, EngineError, ObjectId, QmlObject, QtObject, Value, qml_impl};

use super::key;
use super::sequence::{KeyChord, KeySequence};
use super::standard::{self, CHORD_MODIFIERS};
use crate::focus::tree as focus;
use crate::input::{KeyEvent, Modifiers};
use crate::item::is_item;

/// `Shortcut.context`, with Qt's `Qt.ShortcutContext` values.
pub mod shortcut_context {
    /// The window the Shortcut is in has the keyboard.
    pub const WINDOW: i32 = 1;
    /// Any window of the application has the keyboard.
    pub const APPLICATION: i32 = 2;
}

/// Shortcuts are told apart by the order they were made in, which is the
/// order they are written in a document.
static SERIAL: AtomicU64 = AtomicU64::new(0);

/// `Shortcut` (QtQuick).
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "Shortcut", extends = QtObject)]
pub struct Shortcut {
    #[qml(base)]
    pub object: QtObject,
    /// A string (`"Ctrl+S"`) or a `StandardKey`; undefined when unset.
    #[qml(notify)]
    pub sequence: Value,
    /// A list of the same; each `StandardKey` stands for all its
    /// sequences.
    #[qml(notify)]
    pub sequences: Value,
    #[qml(notify)]
    pub enabled: bool,
    #[qml(notify)]
    pub auto_repeat: bool,
    /// [`shortcut_context`].
    #[qml(notify)]
    pub context: i32,
    #[qml(notify, readonly)]
    pub native_text: String,
    #[qml(notify, readonly)]
    pub portable_text: String,
    #[qml(skip)]
    serial: u64,
}

impl Default for Shortcut {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            sequence: Value::Undefined,
            sequences: Value::list(Vec::new()),
            enabled: true,
            auto_repeat: true,
            context: shortcut_context::WINDOW,
            native_text: String::new(),
            portable_text: String::new(),
            serial: SERIAL.fetch_add(1, Ordering::Relaxed),
        }
    }
}

#[qml_impl]
impl Shortcut {
    #[qml(signal)]
    fn activated();
    #[qml(signal, name = "activatedAmbiguously")]
    fn activated_ambiguously();

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, this) = (cx.engine, cx.this);
        e.extension::<Shortcuts>().all.borrow_mut().push(this);
        e.bind_initial(this, Shortcut::PORTABLE_TEXT.index, move |e| {
            let first = sequences(e, this).into_iter().next().unwrap_or_default();
            Value::from(first.portable_text().as_str())
        })?;
        e.bind_initial(this, Shortcut::NATIVE_TEXT.index, move |e| {
            let first = sequences(e, this).into_iter().next().unwrap_or_default();
            Value::from(first.native_text(standard::platform(e)).as_str())
        })?;
        Ok(())
    }
}

/// What one press may be in a sequence: the chord as it came, and the
/// ways a sequence may write the same thing.
type Variants = Vec<KeyChord>;

#[derive(Default)]
struct Shortcuts {
    /// Every completed Shortcut; the dead ones are dropped when a key
    /// next arrives.
    all: RefCell<Vec<ObjectId>>,
    /// Per window: the presses so far of a sequence of several chords.
    pending: RefCell<HashMap<ObjectId, Vec<Variants>>>,
    /// Per ambiguous sequence: how often it was typed, so that each time
    /// tells another of the shortcuts that have it.
    rounds: RefCell<HashMap<KeySequence, usize>>,
}

/// One value of `sequence` or `sequences` as key sequences. `all`: every
/// sequence of a standard key, not just the preferred one.
fn read(engine: &Engine, v: &Value, all: bool, out: &mut Vec<KeySequence>) {
    match v {
        Value::String(s) => out.extend(KeySequence::parse(s).filter(|q| !q.is_empty())),
        Value::Int(_) | Value::Real(_) | Value::Enum(_) => {
            let list =
                standard::standard_key_sequences(v.to_number() as i32, standard::platform(engine));
            let n = if all { list.len() } else { 1 };
            out.extend(list.into_iter().take(n));
        }
        _ => {}
    }
}

/// The sequences a Shortcut waits for, `sequence` first. A tracked read
/// of both properties.
pub(crate) fn sequences(engine: &Engine, shortcut: ObjectId) -> Vec<KeySequence> {
    let mut out = Vec::new();
    read(
        engine,
        &engine.read(shortcut, Shortcut::SEQUENCE.index),
        false,
        &mut out,
    );
    if let Value::List(list) = engine.read(shortcut, Shortcut::SEQUENCES.index) {
        for v in list.iter() {
            read(engine, v, true, &mut out);
        }
    }
    out
}

/// Whether a Shortcut acts on keys that arrive in the window `root`.
fn in_context(engine: &Engine, shortcut: ObjectId, root: ObjectId) -> bool {
    if engine.read(shortcut, Shortcut::CONTEXT.index).to_number() as i32
        == shortcut_context::APPLICATION
    {
        return true;
    }
    // The window of a Shortcut is that of the item it was declared in.
    let mut cur = engine.owner(shortcut);
    for _ in 0..4096 {
        match cur {
            Some(o) if is_item(engine, o) => return focus::top_of(engine, o) == root,
            Some(o) => cur = engine.owner(o),
            None => break,
        }
    }
    // Declared in no item: it belongs to no window, so to all of them.
    true
}

/// What a sequence may have written for the press `ev`.
fn variants(ev: &KeyEvent) -> Variants {
    let mods = ev.modifiers & CHORD_MODIFIERS;
    let mut out = vec![KeyChord::new(ev.key, mods)];
    let shift = mods.contains(Modifiers::SHIFT);
    let letter = (key::A..=key::Z).contains(&ev.key);
    // Shift is part of how a sign is typed (`+` is Shift+`=` on many
    // keyboards), so "Ctrl++" must not ask for it. A letter is the same
    // key with and without Shift, so there Shift counts.
    if shift && !letter && (0x21..=0xff).contains(&ev.key) {
        out.push(KeyChord::new(ev.key, mods - Modifiers::SHIFT));
    }
    // Shift+Tab arrives as Backtab, and is written either way.
    match ev.key {
        key::BACKTAB => {
            out.push(KeyChord::new(key::TAB, mods | Modifiers::SHIFT));
            out.push(KeyChord::new(key::BACKTAB, mods | Modifiers::SHIFT));
            out.push(KeyChord::new(key::BACKTAB, mods - Modifiers::SHIFT));
        }
        key::TAB if shift => {
            out.push(KeyChord::new(key::BACKTAB, mods));
            out.push(KeyChord::new(key::BACKTAB, mods - Modifiers::SHIFT));
        }
        _ => {}
    }
    if ev.modifiers.contains(Modifiers::KEYPAD) {
        let with: Vec<KeyChord> = out
            .iter()
            .map(|c| KeyChord::new(c.key, c.modifiers | Modifiers::KEYPAD))
            .collect();
        out.extend(with);
    }
    out
}

/// Whether `seq` starts with the presses `typed`.
fn starts_with(seq: &KeySequence, typed: &[Variants]) -> bool {
    seq.chords().len() >= typed.len()
        && seq
            .chords()
            .iter()
            .zip(typed)
            .all(|(c, variants)| variants.contains(c))
}

/// Whether a sequence of several chords is under way in the window.
pub(crate) fn sequence_pending(engine: &Engine, root: ObjectId) -> bool {
    engine
        .extension::<Shortcuts>()
        .pending
        .borrow()
        .get(&root)
        .is_some_and(|p| !p.is_empty())
}

/// Forgets a half-typed sequence: the window lost the keyboard or went
/// away.
pub(crate) fn reset(engine: &Engine, root: ObjectId) {
    engine
        .extension::<Shortcuts>()
        .pending
        .borrow_mut()
        .remove(&root);
}

/// Gives the press `ev`, which arrived in the window `root`, to the
/// shortcuts. True: a shortcut took it (it was activated, or the press is
/// part of a longer sequence, or it ended one that came to nothing).
pub(crate) fn press(engine: &Engine, root: ObjectId, ev: &KeyEvent) -> bool {
    let reg = engine.extension::<Shortcuts>();
    let pending = reg.pending.borrow().get(&root).cloned().unwrap_or_default();
    if key::is_modifier(ev.key) {
        // Holding Ctrl down for the second chord is not a wrong key.
        return !pending.is_empty();
    }
    let candidates: Vec<(u64, ObjectId)> = {
        let mut all = reg.all.borrow_mut();
        all.retain(|s| engine.is_alive(*s));
        if all.is_empty() {
            return false;
        }
        let mut list: Vec<(u64, ObjectId)> = all
            .iter()
            .filter(|s| {
                engine.read(**s, Shortcut::ENABLED.index).to_boolean()
                    && in_context(engine, **s, root)
            })
            .filter_map(|s| engine.with(*s, |n: &Shortcut| (n.serial, *s)).ok())
            .collect();
        list.sort_unstable();
        list
    };
    let mut typed = pending.clone();
    typed.push(variants(ev));
    let mut exact: Vec<ObjectId> = Vec::new();
    let mut matched: Option<KeySequence> = None;
    let mut partial = false;
    for (_, s) in &candidates {
        for seq in sequences(engine, *s) {
            if !starts_with(&seq, &typed) {
                continue;
            }
            if seq.chords().len() > typed.len() {
                partial = true;
            } else if !exact.contains(s) {
                exact.push(*s);
                matched.get_or_insert(seq);
            }
        }
    }
    if exact.is_empty() {
        if partial {
            reg.pending.borrow_mut().insert(root, typed);
            return true;
        }
        // A wrong key ends a sequence under way and is used up by it.
        reset(engine, root);
        return !pending.is_empty();
    }
    reset(engine, root);
    // A held key acts again only on the shortcuts that ask for it; the
    // others still own the key.
    let acting: Vec<ObjectId> = exact
        .iter()
        .copied()
        .filter(|s| !ev.auto_repeat || engine.read(*s, Shortcut::AUTO_REPEAT.index).to_boolean())
        .collect();
    match acting.as_slice() {
        [] => {}
        [one] => {
            let _ = engine.emit_by_name(*one, "activated", &[]);
        }
        several => {
            // None acts; one is told, the last declared first and another
            // each time, so that the application can step through them.
            let round = {
                let mut rounds = reg.rounds.borrow_mut();
                let n = rounds.entry(matched.unwrap_or_default()).or_insert(0);
                let round = *n;
                *n = n.wrapping_add(1);
                round
            };
            let told = several[several.len() - 1 - round % several.len()];
            let _ = engine.emit_by_name(told, "activatedAmbiguously", &[]);
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seq(text: &str) -> KeySequence {
        KeySequence::parse(text).unwrap()
    }

    fn typed(ev: KeyEvent) -> Vec<Variants> {
        vec![variants(&ev)]
    }

    #[test]
    fn shift_counts_for_a_letter_and_not_for_a_sign() {
        let shift_x = KeyEvent::press(0x58).with_modifiers(Modifiers::SHIFT);
        assert!(!starts_with(&seq("X"), &typed(shift_x.clone())));
        assert!(starts_with(&seq("Shift+X"), &typed(shift_x)));
        let plus = KeyEvent::press(key::PLUS).with_modifiers(Modifiers::CONTROL | Modifiers::SHIFT);
        assert!(starts_with(&seq("Ctrl++"), &typed(plus.clone())));
        assert!(starts_with(&seq("Ctrl+Shift++"), &typed(plus)));
    }

    #[test]
    fn shift_tab_is_backtab_either_way() {
        let backtab = KeyEvent::press(key::BACKTAB).with_modifiers(Modifiers::SHIFT);
        assert!(starts_with(&seq("Shift+Tab"), &typed(backtab.clone())));
        assert!(starts_with(&seq("Backtab"), &typed(backtab.clone())));
        assert!(starts_with(&seq("Shift+Backtab"), &typed(backtab.clone())));
        assert!(!starts_with(&seq("Tab"), &typed(backtab)));
        let tab = KeyEvent::press(key::TAB);
        assert!(starts_with(&seq("Tab"), &typed(tab.clone())));
        assert!(!starts_with(&seq("Shift+Tab"), &typed(tab)));
    }

    #[test]
    fn the_keypad_matches_with_and_without_num() {
        let enter = KeyEvent::press(key::ENTER).with_modifiers(Modifiers::KEYPAD);
        assert!(starts_with(&seq("Enter"), &typed(enter.clone())));
        assert!(starts_with(&seq("Num+Enter"), &typed(enter)));
        let plain = KeyEvent::press(key::ENTER);
        assert!(!starts_with(&seq("Num+Enter"), &typed(plain)));
    }

    #[test]
    fn a_longer_sequence_starts_with_its_first_chords() {
        let k = KeyEvent::press(0x4b).with_modifiers(Modifiers::CONTROL);
        let c = KeyEvent::press(0x43).with_modifiers(Modifiers::CONTROL);
        let both = vec![variants(&k), variants(&c)];
        assert!(starts_with(&seq("Ctrl+K, Ctrl+C"), &typed(k)));
        assert!(starts_with(&seq("Ctrl+K, Ctrl+C"), &both));
        assert!(!starts_with(&seq("Ctrl+K"), &both));
        assert!(!starts_with(&seq("Ctrl+K, Ctrl+S"), &both));
    }
}
