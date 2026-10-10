//! Qt's `.ts` translation source files, read and written
//! (<https://doc.qt.io/qt-6/linguist-ts-file-format.html>).
//!
//! [`TsFile`] keeps everything the documented format holds, so a file that
//! is read and written back keeps its translator comments, locations,
//! obsolete messages and `extra-*` fields. [`TsFile::write`] spells the XML
//! the way `lupdate` does, so a file Ljós updated and a file Qt updated
//! differ only where their content does.
//!
//! [`TsFile::catalog`] is what the runtime uses of it, and
//! [`TsFile::merge`] is what `ljos i18n extract` does to it.

use std::fmt::Write as _;

use quick_xml::{
    Reader, XmlVersion,
    escape::resolve_predefined_entity,
    events::{BytesStart, Event},
};

use crate::{
    PluralForms,
    catalog::{Catalog, Translation},
};

/// Why a file could not be read as `.ts`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TsError {
    #[error("line {line}: {message}")]
    Xml { line: usize, message: String },
    #[error("not a translation file: there is no <TS> element")]
    NotTs,
}

/// How far a translation has come.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TsState {
    /// The translator is done with it.
    Finished,
    /// New or changed; a non-empty text is still used at runtime.
    #[default]
    Unfinished,
    /// No longer in the sources, and was never finished.
    Obsolete,
    /// No longer in the sources, and was finished.
    Vanished,
}

impl TsState {
    const fn attribute(self) -> Option<&'static str> {
        match self {
            Self::Finished => None,
            Self::Unfinished => Some("unfinished"),
            Self::Obsolete => Some("obsolete"),
            Self::Vanished => Some("vanished"),
        }
    }

    /// Whether the message is still in the sources.
    #[must_use]
    pub const fn is_current(self) -> bool {
        matches!(self, Self::Finished | Self::Unfinished)
    }
}

/// Where a message is used.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TsLocation {
    pub filename: Option<String>,
    /// As written: `"12"`, or `"+3"` in a file that keeps relative lines.
    pub line: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TsMessage {
    pub id: Option<String>,
    pub locations: Vec<TsLocation>,
    pub source: String,
    pub old_source: Option<String>,
    /// The disambiguation: the second argument of `qsTr`.
    pub comment: Option<String>,
    pub old_comment: Option<String>,
    /// The developer's note to the translator (`//:`).
    pub extra_comment: Option<String>,
    pub translator_comment: Option<String>,
    pub numerus: bool,
    pub state: TsState,
    /// One text, or one per numerus form when `numerus`.
    pub translation: Vec<String>,
    pub user_data: Option<String>,
    /// `<extra-key>value</extra-key>` fields (`//~ key value`), in file order.
    pub extras: Vec<(String, String)>,
}

impl TsMessage {
    fn comment(&self) -> &str {
        self.comment.as_deref().unwrap_or_default()
    }

    fn is_translated(&self) -> bool {
        self.translation.iter().any(|t| !t.is_empty())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TsContext {
    pub name: String,
    pub comment: Option<String>,
    pub messages: Vec<TsMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TsFile {
    pub version: String,
    /// The language of the translations (`is`, `pt_BR`).
    pub language: Option<String>,
    pub source_language: Option<String>,
    pub contexts: Vec<TsContext>,
}

impl Default for TsFile {
    fn default() -> Self {
        Self {
            version: "2.1".to_owned(),
            language: None,
            source_language: None,
            contexts: Vec::new(),
        }
    }
}

// ───────────────────────────── reading ─────────────────────────────

struct Parser<'a> {
    text: &'a str,
    reader: Reader<&'a [u8]>,
}

type Parsed<T> = Result<T, TsError>;

impl<'a> Parser<'a> {
    fn error(&self, message: impl std::fmt::Display) -> TsError {
        let at = usize::try_from(self.reader.error_position()).unwrap_or(usize::MAX);
        let at = at.min(self.text.len());
        let line = self.text.as_bytes()[..at]
            .iter()
            .filter(|b| **b == b'\n')
            .count()
            + 1;
        TsError::Xml {
            line,
            message: message.to_string(),
        }
    }

    fn next(&mut self) -> Parsed<Event<'a>> {
        self.reader.read_event().map_err(|e| self.error(e))
    }

    fn attribute(&self, element: &BytesStart<'_>, name: &str) -> Parsed<Option<String>> {
        let Some(attribute) = element.try_get_attribute(name).map_err(|e| self.error(e))? else {
            return Ok(None);
        };
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|e| self.error(e))?;
        Ok(Some(value.into_owned()))
    }

    /// Skips an element this reader has no field for, with all it contains.
    fn skip(&mut self, element: &BytesStart<'_>) -> Parsed<()> {
        self.reader
            .read_to_end(element.name())
            .map(|_| ())
            .map_err(|e| self.error(e))
    }

    /// The character content of the element just opened, up to its end tag.
    ///
    /// Text arrives in pieces: plain runs, entity and character references,
    /// CDATA sections, and `<byte value="x1"/>` for the control characters
    /// XML cannot hold.
    fn content(&mut self) -> Parsed<String> {
        let mut out = String::new();
        loop {
            match self.next()? {
                Event::Text(t) => out.push_str(&t.xml10_content()),
                Event::CData(t) => out.push_str(&t.xml10_content()),
                Event::GeneralRef(r) => {
                    if let Some(c) = r.resolve_char_ref().map_err(|e| self.error(e))? {
                        out.push(c);
                    } else {
                        let name = r.xml10_content();
                        let text = resolve_predefined_entity(&name)
                            .ok_or_else(|| self.error(format!("unknown entity &{name};")))?;
                        out.push_str(text);
                    }
                }
                Event::Start(e) if e.name().as_ref() == "byte" => {
                    let value = self.attribute(&e, "value")?.unwrap_or_default();
                    let code = value.strip_prefix('x').map_or_else(
                        || value.parse::<u32>().ok(),
                        |hex| u32::from_str_radix(hex, 16).ok(),
                    );
                    let c = code.and_then(char::from_u32).ok_or_else(|| {
                        self.error(format!("<byte value=\"{value}\"/> is not a character"))
                    })?;
                    out.push(c);
                    self.skip(&e)?;
                }
                Event::Start(e) => self.skip(&e)?,
                Event::End(_) => return Ok(out),
                Event::Eof => return Err(self.error("the file ends inside an element")),
                _ => {}
            }
        }
    }

    /// Calls `child` for each element inside the one just opened.
    fn children(
        &mut self,
        mut child: impl FnMut(&mut Self, &BytesStart<'a>) -> Parsed<()>,
    ) -> Parsed<()> {
        loop {
            match self.next()? {
                Event::Start(e) => child(self, &e)?,
                Event::End(_) => return Ok(()),
                Event::Eof => return Err(self.error("the file ends inside an element")),
                _ => {}
            }
        }
    }

    fn context(&mut self) -> Parsed<TsContext> {
        let mut context = TsContext::default();
        self.children(|p, e| {
            match e.name().as_ref() {
                "name" => context.name = p.content()?,
                "comment" => context.comment = Some(p.content()?),
                "message" => {
                    let message = p.message(e)?;
                    context.messages.push(message);
                }
                _ => p.skip(e)?,
            }
            Ok(())
        })?;
        Ok(context)
    }

    fn message(&mut self, element: &BytesStart<'a>) -> Parsed<TsMessage> {
        let mut m = TsMessage {
            id: self.attribute(element, "id")?,
            numerus: self.attribute(element, "numerus")?.as_deref() == Some("yes"),
            state: TsState::Finished,
            ..TsMessage::default()
        };
        let mut translated = false;
        self.children(|p, e| {
            let name = e.name();
            match name.as_ref() {
                "location" => {
                    m.locations.push(TsLocation {
                        filename: p.attribute(e, "filename")?,
                        line: p.attribute(e, "line")?,
                    });
                    p.skip(e)?;
                }
                "source" => m.source = p.content()?,
                "oldsource" => m.old_source = Some(p.content()?),
                "comment" => m.comment = Some(p.content()?),
                "oldcomment" => m.old_comment = Some(p.content()?),
                "extracomment" => m.extra_comment = Some(p.content()?),
                "translatorcomment" => m.translator_comment = Some(p.content()?),
                "userdata" => m.user_data = Some(p.content()?),
                "translation" => {
                    translated = true;
                    m.state = match p.attribute(e, "type")?.as_deref() {
                        Some("unfinished") => TsState::Unfinished,
                        Some("obsolete") => TsState::Obsolete,
                        Some("vanished") => TsState::Vanished,
                        _ => TsState::Finished,
                    };
                    m.translation = p.translation(m.numerus)?;
                }
                other => {
                    if let Some(key) = other.strip_prefix("extra-") {
                        let key = key.to_owned();
                        let value = p.content()?;
                        m.extras.push((key, value));
                    } else {
                        p.skip(e)?;
                    }
                }
            }
            Ok(())
        })?;
        if !translated {
            // A message without the element has nothing a translator did.
            m.state = TsState::Unfinished;
            m.translation = vec![String::new()];
        }
        Ok(m)
    }

    /// The forms of a `<translation>`: its `<numerusform>` children for a
    /// numerus message, its text otherwise.
    fn translation(&mut self, numerus: bool) -> Parsed<Vec<String>> {
        if !numerus {
            return Ok(vec![self.content()?]);
        }
        let mut forms = Vec::new();
        self.children(|p, e| {
            if e.name().as_ref() == "numerusform" {
                forms.push(p.content()?);
            } else {
                p.skip(e)?;
            }
            Ok(())
        })?;
        Ok(forms)
    }
}

impl TsFile {
    /// Reads a `.ts` file.
    ///
    /// # Errors
    /// When the text is not well-formed XML or has no `<TS>` element.
    pub fn read(text: &str) -> Result<Self, TsError> {
        let mut reader = Reader::from_str(text);
        // `<location … />` and `<translation … />` then read like the
        // elements with an end tag that they stand for.
        reader.config_mut().expand_empty_elements = true;
        let mut p = Parser { text, reader };
        loop {
            match p.next()? {
                Event::Start(e) if e.name().as_ref() == "TS" => {
                    let mut file = Self {
                        version: p
                            .attribute(&e, "version")?
                            .unwrap_or_else(|| "2.1".to_owned()),
                        language: p.attribute(&e, "language")?,
                        source_language: p.attribute(&e, "sourcelanguage")?,
                        contexts: Vec::new(),
                    };
                    p.children(|p, e| {
                        if e.name().as_ref() == "context" {
                            let context = p.context()?;
                            file.contexts.push(context);
                        } else {
                            p.skip(e)?;
                        }
                        Ok(())
                    })?;
                    return Ok(file);
                }
                Event::Start(_) | Event::Eof => return Err(TsError::NotTs),
                _ => {}
            }
        }
    }
}

// ───────────────────────────── writing ─────────────────────────────

/// Escapes text the way `lupdate` does: the five XML entities, and
/// `<byte value="x1"/>` for the control characters XML 1.0 cannot hold.
/// Tabs and line breaks stay as they are.
fn escape(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\n' | '\t' => out.push(c),
            c if c < ' ' || c == '\u{7f}' => {
                let _ = write!(out, "<byte value=\"x{:x}\"/>", u32::from(c));
            }
            c => out.push(c),
        }
    }
}

fn element(out: &mut String, indent: &str, name: &str, text: &str) {
    let _ = write!(out, "{indent}<{name}>");
    escape(out, text);
    let _ = writeln!(out, "</{name}>");
}

fn attribute(out: &mut String, name: &str, value: &str) {
    let _ = write!(out, " {name}=\"");
    escape(out, value);
    out.push('"');
}

impl TsFile {
    /// The file as `lupdate` would lay it out.
    #[must_use]
    pub fn write(&self) -> String {
        let mut out =
            String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE TS>\n<TS");
        attribute(&mut out, "version", &self.version);
        if let Some(language) = &self.language {
            attribute(&mut out, "language", language);
        }
        if let Some(language) = &self.source_language {
            attribute(&mut out, "sourcelanguage", language);
        }
        out.push_str(">\n");
        for context in &self.contexts {
            out.push_str("<context>\n");
            element(&mut out, "    ", "name", &context.name);
            if let Some(comment) = &context.comment {
                element(&mut out, "    ", "comment", comment);
            }
            for message in &context.messages {
                write_message(&mut out, message);
            }
            out.push_str("</context>\n");
        }
        out.push_str("</TS>\n");
        out
    }
}

fn write_message(out: &mut String, m: &TsMessage) {
    const INDENT: &str = "        ";
    out.push_str("    <message");
    if let Some(id) = &m.id {
        attribute(out, "id", id);
    }
    if m.numerus {
        out.push_str(" numerus=\"yes\"");
    }
    out.push_str(">\n");
    for location in &m.locations {
        let _ = write!(out, "{INDENT}<location");
        if let Some(filename) = &location.filename {
            attribute(out, "filename", filename);
        }
        if let Some(line) = &location.line {
            attribute(out, "line", line);
        }
        out.push_str("/>\n");
    }
    element(out, INDENT, "source", &m.source);
    let optional = [
        ("oldsource", &m.old_source),
        ("comment", &m.comment),
        ("oldcomment", &m.old_comment),
        ("extracomment", &m.extra_comment),
        ("translatorcomment", &m.translator_comment),
    ];
    for (name, value) in optional {
        if let Some(value) = value {
            element(out, INDENT, name, value);
        }
    }
    let _ = write!(out, "{INDENT}<translation");
    if let Some(state) = m.state.attribute() {
        attribute(out, "type", state);
    }
    out.push('>');
    if m.numerus {
        out.push('\n');
        for form in &m.translation {
            element(out, "            ", "numerusform", form);
        }
        out.push_str(INDENT);
    } else {
        escape(out, m.translation.first().map_or("", String::as_str));
    }
    out.push_str("</translation>\n");
    if let Some(data) = &m.user_data {
        element(out, INDENT, "userdata", data);
    }
    for (key, value) in &m.extras {
        element(out, INDENT, &format!("extra-{key}"), value);
    }
    out.push_str("    </message>\n");
}

// ───────────────────────────── runtime ─────────────────────────────

impl TsFile {
    /// The translations an application shows.
    ///
    /// Left out, so that the source text is shown instead: messages that
    /// are obsolete or vanished, messages with no translation yet, and
    /// numerus messages that lack a form the language needs. A message
    /// with a text id is found by the id only.
    #[must_use]
    pub fn catalog(&self) -> Catalog {
        let language = self.language.as_deref().unwrap_or_default();
        let forms = PluralForms::for_language(language).count();
        let mut catalog = Catalog::new(language);
        for context in &self.contexts {
            for m in &context.messages {
                if !m.state.is_current() {
                    continue;
                }
                let translation = if m.numerus {
                    let wanted = m.translation.get(..forms).unwrap_or_default();
                    if wanted.is_empty() || wanted.iter().any(String::is_empty) {
                        continue;
                    }
                    Translation::Numerus(wanted.to_vec())
                } else {
                    match m.translation.first() {
                        Some(text) if !text.is_empty() => Translation::Text(text.clone()),
                        _ => continue,
                    }
                };
                match &m.id {
                    Some(id) => catalog.insert_id(id, translation),
                    None => catalog.insert(&context.name, &m.source, m.comment(), translation),
                };
            }
        }
        catalog
    }
}

// ───────────────────────────── merging ─────────────────────────────

/// A translatable string found in the sources.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Extracted {
    pub context: String,
    pub source: String,
    /// The disambiguation, `""` for none.
    pub comment: String,
    pub id: Option<String>,
    pub numerus: bool,
    pub extra_comment: Option<String>,
    pub extras: Vec<(String, String)>,
    pub location: TsLocation,
}

/// What [`TsFile::merge`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MergeReport {
    /// Distinct messages in the sources.
    pub found: usize,
    /// Of those, not current in the file before.
    pub new: usize,
    /// Messages kept as obsolete or vanished because they are translated
    /// but no longer in the sources.
    pub kept_obsolete: usize,
    /// Untranslated messages dropped because they left the sources.
    pub dropped: usize,
}

/// What makes two messages the same one.
fn key(context: &str, m: &TsMessage) -> (String, String, String, String) {
    match &m.id {
        // A text id is the whole identity; the source is only its
        // suggested English.
        Some(id) => (String::new(), id.clone(), String::new(), "id".to_owned()),
        None => (
            context.to_owned(),
            m.source.clone(),
            m.comment().to_owned(),
            String::new(),
        ),
    }
}

impl TsFile {
    /// Brings the file up to date with what the sources say now, as
    /// `lupdate` does:
    ///
    /// - a message still in the sources keeps its translation and state,
    ///   and takes its locations and developer comments from the sources;
    /// - a new message is added unfinished, with one empty form per
    ///   numerus form of the language;
    /// - a message that left the sources is kept as `vanished` if it was
    ///   finished, as `obsolete` if it had an unfinished text, and dropped
    ///   if it was never translated;
    /// - a message that comes back takes its kept translation up again.
    ///
    /// Contexts are sorted by name; messages keep the order they have in
    /// the sources.
    pub fn merge(&mut self, extracted: &[Extracted]) -> MergeReport {
        let forms = PluralForms::for_language(self.language.as_deref().unwrap_or_default()).count();
        let mut report = MergeReport::default();

        // What the sources say, one message per key, in source order.
        let mut fresh: Vec<TsContext> = Vec::new();
        for e in extracted {
            let context = match fresh.iter().position(|c| c.name == e.context) {
                Some(i) => &mut fresh[i],
                None => {
                    fresh.push(TsContext {
                        name: e.context.clone(),
                        ..TsContext::default()
                    });
                    fresh.last_mut().unwrap_or_else(|| unreachable!())
                }
            };
            let same = |m: &&mut TsMessage| match (&m.id, &e.id) {
                (Some(a), Some(b)) => a == b,
                (None, None) => m.source == e.source && m.comment() == e.comment,
                _ => false,
            };
            if let Some(m) = context.messages.iter_mut().find(same) {
                m.locations.push(e.location.clone());
                m.numerus |= e.numerus;
                if m.extra_comment.is_none() {
                    m.extra_comment.clone_from(&e.extra_comment);
                }
                if m.source.is_empty() {
                    m.source.clone_from(&e.source);
                }
                continue;
            }
            // By key, as `lupdate` writes them, whatever order the source
            // has them in.
            let mut extras = e.extras.clone();
            extras.sort_by(|a, b| a.0.cmp(&b.0));
            context.messages.push(TsMessage {
                id: e.id.clone(),
                locations: vec![e.location.clone()],
                source: e.source.clone(),
                comment: (!e.comment.is_empty()).then(|| e.comment.clone()),
                extra_comment: e.extra_comment.clone(),
                numerus: e.numerus,
                state: TsState::Unfinished,
                translation: Vec::new(),
                extras,
                ..TsMessage::default()
            });
        }

        // Carry over what translators did.
        let mut old = std::mem::take(&mut self.contexts);
        for context in &mut fresh {
            for m in &mut context.messages {
                report.found += 1;
                let wanted = key(&context.name, m);
                // A message that was current comes first; failing that,
                // one that had left the sources and is now back.
                let mut take = |current: bool| {
                    old.iter_mut().find_map(|c| {
                        let at = c.messages.iter().position(|o| {
                            o.state.is_current() == current && key(&c.name, o) == wanted
                        })?;
                        Some(c.messages.remove(at))
                    })
                };
                let previous = take(true).or_else(|| {
                    report.new += 1;
                    take(false)
                });
                if let Some(previous) = previous {
                    m.translation = previous.translation;
                    m.state = match previous.state {
                        TsState::Vanished => TsState::Finished,
                        TsState::Obsolete => TsState::Unfinished,
                        state => state,
                    };
                    m.translator_comment = previous.translator_comment;
                    m.old_source = previous.old_source;
                    m.old_comment = previous.old_comment;
                    m.user_data = previous.user_data;
                    // A message that became plural, or the reverse, needs
                    // the translator again.
                    if previous.numerus != m.numerus {
                        m.state = TsState::Unfinished;
                    }
                }
                let wanted_forms = if m.numerus { forms } else { 1 };
                if m.translation.len() < wanted_forms {
                    m.translation.resize(wanted_forms, String::new());
                }
            }
        }

        // What is left in `old` is no longer in the sources.
        for context in old {
            let comment = context.comment;
            let mut kept = Vec::new();
            for mut m in context.messages {
                if m.state.is_current() {
                    if !m.is_translated() {
                        report.dropped += 1;
                        continue;
                    }
                    m.state = if m.state == TsState::Finished {
                        TsState::Vanished
                    } else {
                        TsState::Obsolete
                    };
                    m.locations.clear();
                }
                report.kept_obsolete += 1;
                kept.push(m);
            }
            let target = match fresh.iter().position(|c| c.name == context.name) {
                Some(i) => &mut fresh[i],
                None if kept.is_empty() => continue,
                None => {
                    fresh.push(TsContext {
                        name: context.name,
                        ..TsContext::default()
                    });
                    fresh.last_mut().unwrap_or_else(|| unreachable!())
                }
            };
            if target.comment.is_none() {
                target.comment = comment;
            }
            target.messages.extend(kept);
        }

        fresh.sort_by(|a, b| a.name.cmp(&b.name));
        self.contexts = fresh;
        report
    }

    /// How many current messages have no finished translation.
    #[must_use]
    pub fn unfinished(&self) -> usize {
        self.contexts
            .iter()
            .flat_map(|c| &c.messages)
            .filter(|m| m.state == TsState::Unfinished)
            .count()
    }
}
