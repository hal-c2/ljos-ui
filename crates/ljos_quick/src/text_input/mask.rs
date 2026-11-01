//! `TextInput.inputMask` (doc.qt.io, `qml-qtquick-textinput.html`): what each
//! place of a field may hold. The model holds the text as it is shown, with
//! the separators and a blank in each empty place, so that the cursor, the
//! selection and `length` count what is seen; `text` is that without the
//! blanks.

use std::ops::Range;

use ljos_engine::{Atom, Engine, ObjectId, Value};
use ljos_text::TextModel;
use unicode_general_category::{GeneralCategory as G, get_general_category};

use super::base::{SharedState, TextEditor};
use super::input::TextInput;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Case {
    Keep,
    Upper,
    Lower,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Letter,
    Number,
    LetterOrNumber,
    Printable,
    NonZeroDigit,
    NumberOrSign,
    Hex,
    Binary,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Cell {
    /// One UTF-16 unit of a separator: one beyond the Basic Multilingual
    /// Plane takes two places, as `length` counts it.
    Literal(u16),
    Slot {
        kind: Kind,
        required: bool,
        case: Case,
    },
}

/// A mask that has been read.
#[derive(Debug)]
pub(crate) struct Mask {
    /// What `inputMask` reads back as.
    source: String,
    cells: Vec<Cell>,
    blank: u16,
}

/// What `inputMask` reads back as, written as `source`: the mask, a `;` and
/// its blank. A mask without places or separators before the `;` is none.
pub(crate) fn normalized(source: &str) -> String {
    let (part, rest) = source.split_once(';').unwrap_or((source, ""));
    if part.is_empty() {
        return String::new();
    }
    // DECISION: a blank beyond the Basic Multilingual Plane cannot fill one
    // place, so it reads as a space; not measured.
    let blank = rest.chars().next().filter(|c| bmp(*c)).unwrap_or(' ');
    format!("{part};{blank}")
}

fn bmp(c: char) -> bool {
    (c as u32) <= 0xFFFF
}

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

impl Mask {
    /// `None` when `source` is no mask.
    pub(crate) fn parse(source: &str) -> Option<Mask> {
        let source = normalized(source);
        let (part, rest) = source.split_once(';')?;
        let blank = rest.chars().next().unwrap_or(' ') as u16;
        let mut cells = Vec::new();
        let mut case = Case::Keep;
        let mut escaped = false;
        let literal = |cells: &mut Vec<Cell>, c: char| {
            let mut buf = [0; 2];
            cells.extend(c.encode_utf16(&mut buf).iter().map(|u| Cell::Literal(*u)));
        };
        for c in part.chars() {
            if std::mem::take(&mut escaped) {
                literal(&mut cells, c);
                continue;
            }
            let slot = match c {
                '\\' => {
                    escaped = true;
                    continue;
                }
                '<' => {
                    case = Case::Lower;
                    continue;
                }
                '>' => {
                    case = Case::Upper;
                    continue;
                }
                '!' => {
                    case = Case::Keep;
                    continue;
                }
                '[' | ']' | '{' | '}' => continue,
                'A' => (Kind::Letter, true),
                'a' => (Kind::Letter, false),
                'N' => (Kind::LetterOrNumber, true),
                'n' => (Kind::LetterOrNumber, false),
                'X' => (Kind::Printable, true),
                'x' => (Kind::Printable, false),
                '9' => (Kind::Number, true),
                '0' => (Kind::Number, false),
                'D' => (Kind::NonZeroDigit, true),
                'd' => (Kind::NonZeroDigit, false),
                '#' => (Kind::NumberOrSign, false),
                'H' => (Kind::Hex, true),
                'h' => (Kind::Hex, false),
                'B' => (Kind::Binary, true),
                'b' => (Kind::Binary, false),
                other => {
                    literal(&mut cells, other);
                    continue;
                }
            };
            cells.push(Cell::Slot {
                kind: slot.0,
                required: slot.1,
                case,
            });
        }
        Some(Mask {
            source,
            cells,
            blank,
        })
    }

    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    pub(crate) fn len(&self) -> usize {
        self.cells.len()
    }

    fn is_slot(&self, at: usize) -> bool {
        matches!(self.cells.get(at), Some(Cell::Slot { .. }))
    }

    fn empty(&self) -> Vec<u16> {
        self.cells
            .iter()
            .map(|c| match c {
                Cell::Literal(u) => *u,
                Cell::Slot { .. } => self.blank,
            })
            .collect()
    }

    /// The text of what is shown: the blanks of its places go.
    pub(crate) fn strip(&self, display: &str) -> String {
        let kept: Vec<u16> = units(display)
            .into_iter()
            .zip(&self.cells)
            .filter(|(u, c)| matches!(c, Cell::Literal(_)) || *u != self.blank)
            .map(|(u, _)| u)
            .collect();
        String::from_utf16_lossy(&kept)
    }

    /// Whether every place that must be filled is.
    pub(crate) fn acceptable(&self, display: &str) -> bool {
        units(display).into_iter().zip(&self.cells).all(|(u, c)| {
            let Cell::Slot {
                kind,
                required: true,
                ..
            } = c
            else {
                return true;
            };
            char::from_u32(u.into()).is_some_and(|c| self.of_kind(*kind, c))
        })
    }

    fn of_kind(&self, kind: Kind, c: char) -> bool {
        let letter = is_letter(c);
        let number = is_number(c);
        match kind {
            Kind::Letter => letter,
            Kind::Number => number,
            Kind::LetterOrNumber => letter || number,
            Kind::Printable => c as u32 != u32::from(self.blank) && is_printable(c),
            Kind::NonZeroDigit => is_non_zero_digit(c),
            Kind::NumberOrSign => number || c == '+' || c == '-',
            Kind::Hex => number || c.is_ascii_hexdigit(),
            Kind::Binary => c == '0' || c == '1',
        }
    }

    fn takes(&self, cell: Cell, c: char) -> bool {
        match cell {
            Cell::Literal(_) => false,
            Cell::Slot { kind, required, .. } => {
                bmp(c)
                    && ((!required && c as u32 == u32::from(self.blank)) || self.of_kind(kind, c))
            }
        }
    }

    /// Puts each character of `text` in the first place from `at` that takes
    /// it, or passes the separator it is. A character with neither goes.
    /// Returns where it ended and whether anything fitted.
    fn place(&self, shown: &mut [u16], mut at: usize, text: &str) -> (usize, bool) {
        let mut fitted = false;
        for c in text.chars() {
            let mut buf = [0; 2];
            let code = c.encode_utf16(&mut buf);
            let found = (at..self.cells.len()).find(|i| {
                self.takes(self.cells[*i], c)
                    || code.iter().enumerate().all(|(n, u)| {
                        self.cells
                            .get(i + n)
                            .is_some_and(|c| *c == Cell::Literal(*u))
                    })
            });
            let Some(i) = found else { continue };
            fitted = true;
            if let Cell::Slot { case, .. } = self.cells[i] {
                shown[i] = cased(case, c) as u16;
                at = i + 1;
            } else {
                at = i + code.len();
            }
        }
        (at, fitted)
    }

    /// What is shown when `text` is assigned.
    pub(crate) fn assigned(&self, text: &str) -> String {
        let mut shown = self.empty();
        self.place(&mut shown, 0, text);
        String::from_utf16_lossy(&shown)
    }

    fn next_slot(&self, from: usize) -> usize {
        (from..self.cells.len())
            .find(|i| self.is_slot(*i))
            .unwrap_or(self.cells.len())
    }

    fn slot_before(&self, to: usize) -> usize {
        (0..to.min(self.cells.len()))
            .rev()
            .find(|i| self.is_slot(*i))
            .unwrap_or(0)
    }

    pub(crate) fn first_slot(&self) -> usize {
        self.next_slot(0)
    }

    /// Where a cursor going from `from` to `to` stops: never on a separator,
    /// but past it in the direction it went.
    pub(crate) fn snap(&self, from: usize, to: usize) -> usize {
        if to >= self.cells.len() || self.is_slot(to) || to == from {
            to
        } else if to > from {
            self.next_slot(to)
        } else {
            self.slot_before(to)
        }
    }

    fn blanked(&self, shown: &mut [u16], range: Range<usize>) {
        for i in range.start..range.end.min(shown.len()) {
            if self.is_slot(i) {
                shown[i] = self.blank;
            }
        }
    }

    /// `m` shows `shown` and has its cursor at `cursor`. As little as differs
    /// is replaced, and nothing when nothing does, so that no step that
    /// changes nothing joins the undo history.
    fn show(&self, m: &mut TextModel, shown: &[u16], cursor: usize) -> bool {
        let old = units(m.text());
        let first = old.iter().zip(shown).position(|(a, b)| a != b);
        let changed = first.is_some_and(|first| {
            let last = old
                .iter()
                .zip(shown)
                .rposition(|(a, b)| a != b)
                .unwrap_or(first);
            m.replace(
                first..last + 1,
                &String::from_utf16_lossy(&shown[first..=last]),
            )
        });
        m.move_to(cursor, false);
        changed
    }

    /// The text with the selection blanked, and where the cursor is then.
    fn without_selection(&self, m: &TextModel) -> (Vec<u16>, usize) {
        let mut shown = units(m.text());
        if m.has_selection() {
            let r = m.selection();
            self.blanked(&mut shown, r.clone());
            (shown, r.start)
        } else {
            (shown, m.cursor())
        }
    }

    /// Typing one character after the separator it is does nothing, so that
    /// the separator can be typed in passing.
    pub(crate) fn type_in(&self, m: &mut TextModel, text: &str) -> bool {
        let (mut shown, at) = self.without_selection(m);
        let mut chars = text.chars();
        if let (Some(c), None) = (chars.next(), chars.next())
            && at > 0
            && bmp(c)
            && self.cells.get(at - 1) == Some(&Cell::Literal(c as u16))
            && !self.cells.get(at).is_some_and(|cell| self.takes(*cell, c))
        {
            return self.show(m, &shown, at);
        }
        self.put_in(m, &mut shown, at, text)
    }

    /// A paste, or text from an input method: no separator is passed by.
    pub(crate) fn paste(&self, m: &mut TextModel, text: &str) -> bool {
        let (mut shown, at) = self.without_selection(m);
        self.put_in(m, &mut shown, at, text)
    }

    fn put_in(&self, m: &mut TextModel, shown: &mut [u16], at: usize, text: &str) -> bool {
        let (end, fitted) = self.place(shown, at, text);
        let cursor = if fitted { self.next_slot(end) } else { at };
        self.show(m, shown, cursor)
    }

    /// `insert(position, text)`: over what is there, and the cursor and the
    /// selection stay.
    pub(crate) fn insert(&self, m: &mut TextModel, at: usize, text: &str) -> bool {
        let mut shown = units(m.text());
        self.place(&mut shown, at, text);
        self.show_keeping(m, &shown)
    }

    /// `remove(start, end)`: the cursor and the selection stay.
    pub(crate) fn remove(&self, m: &mut TextModel, range: Range<usize>) -> bool {
        let mut shown = units(m.text());
        self.blanked(&mut shown, range);
        self.show_keeping(m, &shown)
    }

    fn show_keeping(&self, m: &mut TextModel, shown: &[u16]) -> bool {
        let (anchor, cursor) = (m.anchor(), m.cursor());
        let changed = self.show(m, shown, cursor);
        m.select(anchor, cursor);
        changed
    }

    pub(crate) fn clear(&self, m: &mut TextModel) -> bool {
        self.show(m, &self.empty(), 0)
    }

    /// The selection, or nothing when there is none.
    pub(crate) fn delete_selection(&self, m: &mut TextModel) -> bool {
        let (shown, at) = self.without_selection(m);
        self.show(m, &shown, at)
    }

    /// Backspace: empties the place before the cursor and goes to it.
    pub(crate) fn backspace(&self, m: &mut TextModel) -> bool {
        let target = self.slot_before(m.cursor());
        self.delete_back_to(m, target)
    }

    /// Delete: empties the place at the cursor, which stays.
    pub(crate) fn delete(&self, m: &mut TextModel) -> bool {
        let to = m.cursor() + 1;
        self.delete_forward_to(m, to)
    }

    /// From the cursor back to `target`, where the cursor goes; a selection
    /// goes instead.
    pub(crate) fn delete_back_to(&self, m: &mut TextModel, target: usize) -> bool {
        if m.has_selection() {
            return self.delete_selection(m);
        }
        let cursor = m.cursor();
        let mut shown = units(m.text());
        let target = if self.is_slot(target) || target == 0 {
            target
        } else {
            self.slot_before(target)
        };
        self.blanked(&mut shown, target..cursor);
        self.show(m, &shown, target)
    }

    /// From the cursor on to `to`, and the cursor stays; a selection goes
    /// instead.
    pub(crate) fn delete_forward_to(&self, m: &mut TextModel, to: usize) -> bool {
        if m.has_selection() {
            return self.delete_selection(m);
        }
        let cursor = m.cursor();
        let mut shown = units(m.text());
        self.blanked(&mut shown, cursor..to);
        self.show(m, &shown, cursor)
    }

    /// Replaces a range, for an input method.
    pub(crate) fn replace(&self, m: &mut TextModel, range: Range<usize>, text: &str) -> bool {
        let mut shown = units(m.text());
        self.blanked(&mut shown, range.clone());
        self.put_in(m, &mut shown, range.start, text)
    }

    /// The whole field takes `text`, as an edit that can be undone.
    pub(crate) fn set_value(&self, m: &mut TextModel, text: &str) -> bool {
        let shown = units(&self.assigned(text));
        let end = shown.len();
        self.show(m, &shown, end)
    }

    /// Takes the selection from the start to where the text ends.
    pub(crate) fn select_all(&self, m: &mut TextModel) {
        m.select(0, ljos_text::utf16_len(&self.strip(m.text())));
    }

    /// Where the End key goes, before stopping at a place.
    pub(crate) fn text_end(&self, display: &str) -> usize {
        ljos_text::utf16_len(&self.strip(display))
    }
}

/// The mapped character only where it stays one character.
fn cased(case: Case, c: char) -> char {
    fn one(mut it: impl Iterator<Item = char>, c: char) -> char {
        match (it.next(), it.next()) {
            (Some(mapped), None) if bmp(mapped) => mapped,
            _ => c,
        }
    }
    match case {
        Case::Keep => c,
        Case::Upper => one(c.to_uppercase(), c),
        Case::Lower => one(c.to_lowercase(), c),
    }
}

fn is_letter(c: char) -> bool {
    matches!(
        get_general_category(c),
        G::UppercaseLetter
            | G::LowercaseLetter
            | G::TitlecaseLetter
            | G::ModifierLetter
            | G::OtherLetter
    )
}

fn is_number(c: char) -> bool {
    matches!(
        get_general_category(c),
        G::DecimalNumber | G::LetterNumber | G::OtherNumber
    )
}

fn is_printable(c: char) -> bool {
    !matches!(
        get_general_category(c),
        G::Control | G::Format | G::Surrogate | G::PrivateUse | G::Unassigned
    )
}

/// The zeros of the decimal digits of the Basic Multilingual Plane.
const ZEROS: [u32; 37] = [
    0x0030, 0x0660, 0x06F0, 0x07C0, 0x0966, 0x09E6, 0x0A66, 0x0AE6, 0x0B66, 0x0BE6, 0x0C66, 0x0CE6,
    0x0D66, 0x0DE6, 0x0E50, 0x0ED0, 0x0F20, 0x1040, 0x1090, 0x17E0, 0x1810, 0x1946, 0x19D0, 0x1A80,
    0x1A90, 0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620, 0xA8D0, 0xA900, 0xA9D0, 0xA9F0, 0xAA50, 0xABF0,
    0xFF10,
];

/// The digits written otherwise that have a value from 1 to 9.
const OTHER_DIGITS: [(u32, u32); 12] = [
    (0x00B2, 0x00B3),
    (0x00B9, 0x00B9),
    (0x1369, 0x1371),
    (0x2074, 0x2079),
    (0x2081, 0x2089),
    (0x2460, 0x2468),
    (0x2474, 0x247C),
    (0x2488, 0x2490),
    (0x24F5, 0x24FD),
    (0x2776, 0x277E),
    (0x2780, 0x2788),
    (0x278A, 0x2792),
];

fn is_non_zero_digit(c: char) -> bool {
    let c = c as u32;
    ZEROS.iter().any(|z| (z + 1..=z + 9).contains(&c))
        || OTHER_DIGITS.iter().any(|(a, b)| (*a..=*b).contains(&c))
}

/// The mask of an editor, cloned out so that it can be held across an edit.
pub(crate) fn of(st: &SharedState) -> Option<std::rc::Rc<Mask>> {
    st.borrow().mask.clone()
}

/// `inputMask` as written now, for a field whose mask is another: the text
/// is read again by the new mask, or goes with the last mask.
pub(crate) fn sync(engine: &Engine, obj: ObjectId, st: &SharedState) -> bool {
    let Ok(source) = engine.with(obj, |t: &TextInput| t.input_mask.clone()) else {
        return false;
    };
    if st.borrow().mask.as_ref().map_or("", |m| m.source()) == source.as_str() {
        return false;
    }
    let text = engine
        .with(obj, |t: &TextEditor| {
            Value::String(t.text.clone()).to_js_string()
        })
        .map(|t| t.as_str().to_owned())
        .unwrap_or_default();
    apply(engine, obj, st, &source, &text);
    true
}

/// Makes `source` the mask of the field, and `text` what it reads.
pub(crate) fn apply(engine: &Engine, obj: ObjectId, st: &SharedState, source: &Atom, text: &str) {
    let max = {
        let mut s = st.borrow_mut();
        let was = s.mask.is_some();
        s.mask = Mask::parse(source.as_str()).map(std::rc::Rc::new);
        // The limit is lifted first: the shown text may be longer than the
        // one it replaces.
        s.model.set_max_length(usize::MAX);
        let max = match s.mask.clone() {
            Some(mask) => {
                s.model.set_text(&mask.assigned(text));
                s.model.move_to(mask.first_slot(), false);
                mask.len()
            }
            None => {
                if was {
                    s.model.set_text("");
                }
                super::input::MAXIMUM_LENGTH as usize
            }
        };
        s.model.clear_undo();
        s.model.set_max_length(max);
        max
    };
    let was_syncing = std::mem::replace(&mut st.borrow_mut().syncing, true);
    super::base::check(
        engine,
        "maximumLength",
        engine.write_derived(obj, TextInput::MAXIMUM_LENGTH.index, Value::Int(max as i32)),
    );
    st.borrow_mut().syncing = was_syncing;
}
