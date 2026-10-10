//! `.ljl`: the catalogue format a shipped application loads (ARCHITECTURE.md §15).
//!
//! The file is read in place. [`Ljl`] borrows or owns any `AsRef<[u8]>`, so
//! a host can hand it a memory-mapped file, bytes embedded with
//! `include_bytes!`, or a `Vec<u8>` off the network; nothing is copied and
//! no XML parser is linked. This crate itself maps nothing.
//!
//! Layout, all integers little-endian:
//!
//! ```text
//! header   "LJL1", flags u32, language (offset u32, length u32),
//!          texts (count u32, offset u32), ids (count u32, offset u32),
//!          forms (count u32, offset u32), strings (offset u32, length u32)
//! texts    count × { hash u64, context, source, comment, first form u32,
//!                    form count u16, flags u16 }      sorted by hash
//! ids      count × { hash u64, id, first form u32, form count u16, flags u16 }
//! forms    count × string
//! strings  UTF-8, no terminators
//! ```
//!
//! A string is `(offset u32, length u32)` into the strings block. A lookup
//! is a binary search for the key's hash, then a comparison of the key
//! itself, so two messages whose hashes collide are both found.
//!
//! [`Ljl::open`] checks the whole file once; after that no lookup can read
//! outside it, whatever the bytes are.

use std::collections::HashMap;

use crate::catalog::{Catalog, Entry, Messages};

const MAGIC: &[u8; 4] = b"LJL1";
const HEADER: usize = 48;
const TEXT_RECORD: usize = 40;
const ID_RECORD: usize = 24;
const FORM_RECORD: usize = 8;
/// Bit 0 of a record's flags.
const NUMERUS: u16 = 1;

/// Why bytes are not a `.ljl` catalogue.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LjlError {
    #[error("not a .ljl catalogue (it does not start with \"LJL1\")")]
    Magic,
    #[error("the .ljl catalogue is cut short or damaged: {0}")]
    Damaged(&'static str),
    #[error("the catalogue is too large for the .ljl format, which addresses 4 GiB")]
    TooLarge,
}

/// FNV-1a, 64 bits. The parts of a key are separated so that
/// `("ab", "c")` and `("a", "bc")` hash differently.
fn hash(parts: &[&str]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |byte: u8| {
        h ^= u64::from(byte);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    };
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            eat(0xff);
        }
        part.bytes().for_each(&mut eat);
    }
    h
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let b = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    let b = bytes.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([b[0], b[1]]))
}

fn u64_at(bytes: &[u8], at: usize) -> Option<u64> {
    let b = bytes.get(at..at.checked_add(8)?)?;
    Some(u64::from_le_bytes([
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
    ]))
}

/// Where the tables are; read from the header once.
#[derive(Debug, Clone, Copy)]
struct Tables {
    texts: (usize, usize),
    ids: (usize, usize),
    forms: (usize, usize),
    strings: (usize, usize),
}

/// A `.ljl` catalogue over bytes it does not copy.
#[derive(Debug, Clone)]
pub struct Ljl<B> {
    bytes: B,
    tables: Tables,
}

impl<B: AsRef<[u8]>> Ljl<B> {
    /// Checks that `bytes` are a whole, consistent catalogue.
    ///
    /// # Errors
    /// When they are not: the wrong magic, a table or string that reaches
    /// outside the file, text that is not UTF-8, or records out of order.
    pub fn open(bytes: B) -> Result<Self, LjlError> {
        let tables = check(bytes.as_ref())?;
        Ok(Self { bytes, tables })
    }

    /// The bytes the catalogue reads from.
    pub const fn bytes(&self) -> &B {
        &self.bytes
    }

    /// How many messages the catalogue has, text ids included.
    pub const fn len(&self) -> usize {
        self.tables.texts.0 + self.tables.ids.0
    }

    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn string(&self, at: usize) -> Option<&str> {
        string(self.bytes.as_ref(), self.tables.strings, at)
    }

    fn entry(&self, at: usize) -> Option<Entry> {
        let bytes = self.bytes.as_ref();
        Some(Entry {
            first: u32_at(bytes, at)?,
            count: u32::from(u16_at(bytes, at + 4)?),
            numerus: u16_at(bytes, at + 6)? & NUMERUS != 0,
        })
    }

    /// The records of one table whose hash is `wanted`, as byte offsets.
    fn candidates(
        &self,
        (count, offset): (usize, usize),
        record: usize,
        wanted: u64,
    ) -> impl Iterator<Item = usize> + '_ {
        let bytes = self.bytes.as_ref();
        let hash_of = move |i: usize| u64_at(bytes, offset + i * record);
        // Binary search for the first record whose hash is not below `wanted`.
        let (mut low, mut high) = (0, count);
        while low < high {
            let middle = low + (high - low) / 2;
            if hash_of(middle).is_some_and(|h| h < wanted) {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        (low..count)
            .take_while(move |i| hash_of(*i) == Some(wanted))
            .map(move |i| offset + i * record)
    }
}

/// The string whose `(offset, length)` pair is at byte `at`.
fn string(bytes: &[u8], (strings, length): (usize, usize), at: usize) -> Option<&str> {
    let offset = u32_at(bytes, at)? as usize;
    let len = u32_at(bytes, at + 4)? as usize;
    let end = offset.checked_add(len)?;
    if end > length {
        return None;
    }
    let block = bytes.get(strings..strings.checked_add(length)?)?;
    // `open` validated the block once; this is a slice of it. `get` instead
    // of indexing keeps a lookup from panicking on bytes that changed
    // underneath a mapping.
    std::str::from_utf8(block).ok()?.get(offset..end)
}

fn check(bytes: &[u8]) -> Result<Tables, LjlError> {
    let cut = LjlError::Damaged("the header is incomplete");
    if bytes.get(..4) != Some(MAGIC.as_slice()) {
        return Err(LjlError::Magic);
    }
    let field = |i: usize| {
        u32_at(bytes, 4 + i * 4)
            .map(|v| v as usize)
            .ok_or(cut.clone())
    };
    // Field 0 is the flags word: none are defined yet, and a reader ignores
    // the ones it does not know. Fields 1 and 2 are the language, checked
    // below like any other string.
    let tables = Tables {
        texts: (field(3)?, field(4)?),
        ids: (field(5)?, field(6)?),
        forms: (field(7)?, field(8)?),
        strings: (field(9)?, field(10)?),
    };
    let within = |(count, offset): (usize, usize), record: usize| {
        count
            .checked_mul(record)
            .and_then(|size| offset.checked_add(size))
            .is_some_and(|end| offset >= HEADER && end <= bytes.len())
    };
    if !within(tables.texts, TEXT_RECORD)
        || !within(tables.ids, ID_RECORD)
        || !within(tables.forms, FORM_RECORD)
    {
        return Err(LjlError::Damaged("a table reaches outside the file"));
    }
    let (strings, length) = tables.strings;
    let block = strings
        .checked_add(length)
        .and_then(|end| bytes.get(strings..end))
        .ok_or(LjlError::Damaged("the strings reach outside the file"))?;
    let block =
        std::str::from_utf8(block).map_err(|_| LjlError::Damaged("the strings are not UTF-8"))?;
    let text = |at: usize| -> Result<(), LjlError> {
        let offset = u32_at(bytes, at).ok_or(cut.clone())? as usize;
        let len = u32_at(bytes, at + 4).ok_or(cut.clone())? as usize;
        offset
            .checked_add(len)
            .and_then(|end| block.get(offset..end))
            .map(|_| ())
            .ok_or(LjlError::Damaged("a string reaches outside the strings"))
    };
    // The language is stored like a string record, in the header itself.
    text(8)?;
    let forms_in = |at: usize| -> Result<(), LjlError> {
        let first = u32_at(bytes, at).ok_or(cut.clone())? as usize;
        let count = usize::from(u16_at(bytes, at + 4).ok_or(cut.clone())?);
        if first
            .checked_add(count)
            .is_some_and(|end| end <= tables.forms.0)
        {
            Ok(())
        } else {
            Err(LjlError::Damaged("a message names forms that do not exist"))
        }
    };
    let sorted = |(count, offset): (usize, usize), record: usize| {
        (1..count)
            .all(|i| u64_at(bytes, offset + (i - 1) * record) <= u64_at(bytes, offset + i * record))
    };
    if !sorted(tables.texts, TEXT_RECORD) || !sorted(tables.ids, ID_RECORD) {
        return Err(LjlError::Damaged("the messages are not in hash order"));
    }
    for i in 0..tables.texts.0 {
        let at = tables.texts.1 + i * TEXT_RECORD;
        text(at + 8)?;
        text(at + 16)?;
        text(at + 24)?;
        forms_in(at + 32)?;
    }
    for i in 0..tables.ids.0 {
        let at = tables.ids.1 + i * ID_RECORD;
        text(at + 8)?;
        forms_in(at + 16)?;
    }
    for i in 0..tables.forms.0 {
        text(tables.forms.1 + i * FORM_RECORD)?;
    }
    Ok(tables)
}

impl<B: AsRef<[u8]>> Messages for Ljl<B> {
    fn language(&self) -> &str {
        self.string(8).unwrap_or_default()
    }

    fn text(&self, context: &str, source: &str, comment: &str) -> Option<Entry> {
        let wanted = hash(&[context, source, comment]);
        self.candidates(self.tables.texts, TEXT_RECORD, wanted)
            .find(|at| {
                self.string(at + 8) == Some(context)
                    && self.string(at + 16) == Some(source)
                    && self.string(at + 24) == Some(comment)
            })
            .and_then(|at| self.entry(at + 32))
    }

    fn id(&self, id: &str) -> Option<Entry> {
        self.candidates(self.tables.ids, ID_RECORD, hash(&[id]))
            .find(|at| self.string(at + 8) == Some(id))
            .and_then(|at| self.entry(at + 16))
    }

    fn form(&self, entry: Entry, index: usize) -> Option<&str> {
        if index >= entry.forms() {
            return None;
        }
        let form = (entry.first as usize).checked_add(index)?;
        if form >= self.tables.forms.0 {
            return None;
        }
        self.string(self.tables.forms.1 + form * FORM_RECORD)
    }
}

/// Collects strings into one block, each stored once: contexts and common
/// words repeat across a catalogue.
#[derive(Default)]
struct Strings<'a> {
    block: Vec<u8>,
    seen: HashMap<&'a str, (u32, u32)>,
}

impl<'a> Strings<'a> {
    fn add(&mut self, text: &'a str) -> Result<(u32, u32), LjlError> {
        if let Some(found) = self.seen.get(text) {
            return Ok(*found);
        }
        let offset = u32::try_from(self.block.len()).map_err(|_| LjlError::TooLarge)?;
        let len = u32::try_from(text.len()).map_err(|_| LjlError::TooLarge)?;
        self.block.extend_from_slice(text.as_bytes());
        u32::try_from(self.block.len()).map_err(|_| LjlError::TooLarge)?;
        self.seen.insert(text, (offset, len));
        Ok((offset, len))
    }
}

fn put(out: &mut Vec<u8>, (offset, len): (u32, u32)) {
    out.extend_from_slice(&offset.to_le_bytes());
    out.extend_from_slice(&len.to_le_bytes());
}

/// The form part of a record. Forms are copied message by message so each
/// message's forms stay adjacent, whatever order the catalogue stored them in.
fn entry<'a>(
    catalog: &'a Catalog,
    source: Entry,
    strings: &mut Strings<'a>,
    forms: &mut Vec<(u32, u32)>,
) -> Result<[u8; 8], LjlError> {
    let first = u32::try_from(forms.len()).map_err(|_| LjlError::TooLarge)?;
    let count = u16::try_from(source.forms()).map_err(|_| LjlError::TooLarge)?;
    for i in 0..source.forms() {
        // A catalogue only hands out entries whose forms it has.
        let text = catalog.form(source, i).unwrap_or_default();
        forms.push(strings.add(text)?);
    }
    let mut out = [0; 8];
    out[..4].copy_from_slice(&first.to_le_bytes());
    out[4..6].copy_from_slice(&count.to_le_bytes());
    out[6..].copy_from_slice(&(if source.numerus { NUMERUS } else { 0 }).to_le_bytes());
    Ok(out)
}

/// Writes a catalogue as `.ljl`. The same catalogue always gives the same
/// bytes, so a build that regenerates the file does not dirty it.
///
/// # Errors
/// [`LjlError::TooLarge`] when the catalogue does not fit 32-bit offsets or
/// a message has more than 65 535 forms.
pub fn write(catalog: &Catalog) -> Result<Vec<u8>, LjlError> {
    let mut strings = Strings::default();
    let mut forms: Vec<(u32, u32)> = Vec::new();
    let language = strings.add(catalog.language())?;

    let mut texts: Vec<(u64, Vec<u8>)> = Vec::new();
    for (key, source) in catalog.texts() {
        let mut record = Vec::with_capacity(TEXT_RECORD);
        let h = hash(&[key.context, key.source, key.comment]);
        record.extend_from_slice(&h.to_le_bytes());
        put(&mut record, strings.add(key.context)?);
        put(&mut record, strings.add(key.source)?);
        put(&mut record, strings.add(key.comment)?);
        record.extend_from_slice(&entry(catalog, source, &mut strings, &mut forms)?);
        texts.push((h, record));
    }
    let mut ids: Vec<(u64, Vec<u8>)> = Vec::new();
    for (id, source) in catalog.ids() {
        let mut record = Vec::with_capacity(ID_RECORD);
        let h = hash(&[id]);
        record.extend_from_slice(&h.to_le_bytes());
        put(&mut record, strings.add(id)?);
        record.extend_from_slice(&entry(catalog, source, &mut strings, &mut forms)?);
        ids.push((h, record));
    }
    // Stable, so records with one hash keep the catalogue's key order.
    texts.sort_by_key(|(h, _)| *h);
    ids.sort_by_key(|(h, _)| *h);

    let size = |n: usize| u32::try_from(n).map_err(|_| LjlError::TooLarge);
    let texts_at = HEADER;
    let ids_at = texts_at + texts.len() * TEXT_RECORD;
    let forms_at = ids_at + ids.len() * ID_RECORD;
    let strings_at = forms_at + forms.len() * FORM_RECORD;
    let total = strings_at + strings.block.len();
    size(total)?;

    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&0u32.to_le_bytes());
    put(&mut out, language);
    put(&mut out, (size(texts.len())?, size(texts_at)?));
    put(&mut out, (size(ids.len())?, size(ids_at)?));
    put(&mut out, (size(forms.len())?, size(forms_at)?));
    put(&mut out, (size(strings_at)?, size(strings.block.len())?));
    for (_, record) in texts.iter().chain(&ids) {
        out.extend_from_slice(record);
    }
    for form in forms {
        put(&mut out, form);
    }
    out.extend_from_slice(&strings.block);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Translation;

    fn catalog() -> Catalog {
        let mut c = Catalog::new("is_IS");
        c.insert("Main", "Open", "", Translation::Text("Opna".into()));
        c.insert(
            "Main",
            "Open",
            "adjective",
            Translation::Text("Opinn".into()),
        );
        c.insert("Dialog", "Open", "", Translation::Text("Opna".into()));
        c.insert(
            "Main",
            "%n file(s)",
            "",
            Translation::Numerus(vec!["%n skrá".into(), "%n skrár".into()]),
        );
        c.insert_id("app-title", Translation::Text("Ljós".into()));
        c
    }

    #[test]
    fn what_is_written_is_what_is_read() {
        let source = catalog();
        let ljl = Ljl::open(write(&source).unwrap()).unwrap();
        assert_eq!(ljl.language(), "is_IS");
        assert_eq!(ljl.len(), source.len());
        for (key, entry) in source.texts() {
            let found = ljl.text(key.context, key.source, key.comment).unwrap();
            assert_eq!(found.numerus, entry.numerus);
            assert_eq!(found.forms(), entry.forms());
            for i in 0..=entry.forms() {
                assert_eq!(
                    ljl.form(found, i),
                    source.form(entry, i),
                    "{key:?} form {i}"
                );
            }
        }
        let title = ljl.id("app-title").unwrap();
        assert_eq!(ljl.form(title, 0), Some("Ljós"));
        assert_eq!(ljl.text("Main", "Close", ""), None);
        assert_eq!(ljl.text("Main", "Open", "noun"), None);
        assert_eq!(ljl.id("Open"), None);
    }

    #[test]
    fn the_same_catalogue_gives_the_same_bytes_and_stores_a_string_once() {
        let bytes = write(&catalog()).unwrap();
        assert_eq!(bytes, write(&catalog()).unwrap());
        let text = String::from_utf8_lossy(&bytes);
        assert_eq!(text.matches("Opna").count(), 1);
        assert_eq!(text.matches("Main").count(), 1);
    }

    #[test]
    fn it_reads_from_borrowed_bytes_as_a_mapping_would_give() {
        let bytes = write(&catalog()).unwrap();
        let ljl = Ljl::open(bytes.as_slice()).unwrap();
        let e = ljl.text("Dialog", "Open", "").unwrap();
        assert_eq!(ljl.form(e, 0), Some("Opna"));
    }

    #[test]
    fn an_empty_catalogue_is_a_valid_file() {
        let ljl = Ljl::open(write(&Catalog::new("")).unwrap()).unwrap();
        assert!(ljl.is_empty());
        assert_eq!(ljl.language(), "");
        assert_eq!(ljl.text("a", "b", ""), None);
    }

    #[test]
    fn many_messages_are_all_found() {
        let mut c = Catalog::new("is");
        for i in 0..2000 {
            c.insert(
                "Main",
                &format!("text {i}"),
                "",
                Translation::Text(format!("texti {i}")),
            );
        }
        let ljl = Ljl::open(write(&c).unwrap()).unwrap();
        for i in 0..2000 {
            let e = ljl.text("Main", &format!("text {i}"), "").unwrap();
            assert_eq!(ljl.form(e, 0), Some(format!("texti {i}").as_str()));
        }
    }

    #[test]
    fn other_files_are_refused() {
        assert_eq!(Ljl::open(b"<?xml".as_slice()).unwrap_err(), LjlError::Magic);
        assert_eq!(Ljl::open(b"".as_slice()).unwrap_err(), LjlError::Magic);
        assert!(matches!(
            Ljl::open(b"LJL1\0\0".as_slice()),
            Err(LjlError::Damaged(_))
        ));
    }

    /// Every way of cutting the file short or flipping one of its bytes
    /// either is refused or opens into a catalogue whose lookups stay
    /// inside it. None may panic.
    #[test]
    fn damage_is_refused_or_harmless() {
        let source = catalog();
        let good = write(&source).unwrap();
        let ask = |bytes: &[u8]| {
            if let Ok(ljl) = Ljl::open(bytes) {
                let _ = ljl.language();
                for (key, _) in source.texts() {
                    if let Some(e) = ljl.text(key.context, key.source, key.comment) {
                        for i in 0..e.forms().min(8) {
                            let _ = ljl.form(e, i);
                        }
                    }
                }
                if let Some(e) = ljl.id("app-title") {
                    let _ = ljl.form(e, 0);
                }
            }
        };
        for len in 0..good.len() {
            assert!(Ljl::open(&good[..len]).is_err(), "cut at {len} opened");
        }
        for at in 0..good.len() {
            for bit in [0x01, 0x80, 0xff] {
                let mut bad = good.clone();
                bad[at] ^= bit;
                ask(&bad);
            }
        }
    }
}
