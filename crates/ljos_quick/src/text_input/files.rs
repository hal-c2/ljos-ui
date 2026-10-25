//! The file of a `TextEdit`'s document: `TextDocument.source`, `save()` and
//! `saveAs()` (doc.qt.io, `qml-qtquick-textdocument.html`).
//!
//! A file is read and written on the compute pool, never on the Engine
//! thread; the answer comes back through the platform's inbox
//! ([`ljos_platform::Platform::expect`]), which the frame drains. Until it
//! does, `status` says `Loading` or `Saving`.
//!
//! What a file holds is told by the editor's `textFormat`, and with
//! `AutoText` by the file's suffix: `.md` and `.markdown` are Markdown,
//! `.html` and `.htm` are HTML, anything else is plain text. A file is
//! written in the format its suffix names, or in the editor's own when the
//! suffix names none. Only local files are read and written.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use ljos_engine::{Atom, Engine, LogLevel, ObjectId, PropIndex, Value};
use ljos_platform::{Platform, Reply, url_path};
use ljos_runtime::{ComputePool, Lane};
use ljos_text::rich::{RichText, TextFormat};

use super::base::{SharedState, check, publish, state_of};
use super::edit::TextEdit;
use super::highlight::{TextDocument, sync_modified};
use super::text_format;
use crate::enums::EnumTable;
use crate::item::write_computed;

const CATEGORY: &str = "qt.quick.textdocument";

/// `TextDocument.Status`.
pub mod status {
    pub const NULL: i32 = 0;
    pub const LOADING: i32 = 1;
    pub const LOADED: i32 = 2;
    pub const SAVING: i32 = 3;
    pub const SAVED: i32 = 4;
    pub const READ_ERROR: i32 = 5;
    pub const WRITE_ERROR: i32 = 6;
    pub const NON_LOCAL_FILE_ERROR: i32 = 7;
}

pub(crate) fn document_enums() -> EnumTable {
    vec![(
        "Status",
        vec![
            ("Null", i64::from(status::NULL)),
            ("Loading", i64::from(status::LOADING)),
            ("Loaded", i64::from(status::LOADED)),
            ("Saving", i64::from(status::SAVING)),
            ("Saved", i64::from(status::SAVED)),
            ("ReadError", i64::from(status::READ_ERROR)),
            ("WriteError", i64::from(status::WRITE_ERROR)),
            ("NonLocalFileError", i64::from(status::NON_LOCAL_FILE_ERROR)),
        ],
    )]
}

/// What a file holds, as a `TextEdit` reads and writes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Plain,
    Markdown,
    Html,
}

impl Kind {
    /// What the suffix of `path` names, if anything.
    fn of_suffix(path: &Path) -> Option<Kind> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "md" | "markdown" => Some(Kind::Markdown),
            "html" | "htm" | "xhtml" => Some(Kind::Html),
            "txt" | "text" => Some(Kind::Plain),
            _ => None,
        }
    }

    fn of_editor(text_format: i32) -> Option<Kind> {
        match text_format {
            text_format::PLAIN_TEXT => Some(Kind::Plain),
            text_format::RICH_TEXT => Some(Kind::Html),
            text_format::MARKDOWN_TEXT => Some(Kind::Markdown),
            _ => None,
        }
    }

    fn format(self) -> Option<TextFormat> {
        match self {
            Kind::Plain => None,
            Kind::Markdown => Some(TextFormat::Markdown),
            Kind::Html => Some(TextFormat::Rich),
        }
    }
}

fn text_format_of(engine: &Engine, edit: ObjectId) -> i32 {
    engine
        .untrack(|| engine.read(edit, TextEdit::TEXT_FORMAT.index))
        .to_number() as i32
}

fn set(engine: &Engine, doc: ObjectId, p: PropIndex, v: Value) {
    if engine.is_alive(doc) {
        check(engine, "textDocument", write_computed(engine, doc, p, v));
    }
}

fn set_status(engine: &Engine, doc: ObjectId, status: i32, error: &str) {
    engine.batch(|| {
        set(
            engine,
            doc,
            TextDocument::ERROR_STRING.index,
            Value::String(Atom::from(error)),
        );
        set(engine, doc, TextDocument::STATUS.index, Value::Int(status));
    });
}

fn fail(engine: &Engine, doc: ObjectId, status: i32, error: String) {
    engine.log(LogLevel::Warning, CATEGORY, error.clone());
    set_status(engine, doc, status, &error);
}

/// Starts another read or write of `doc`; an answer to an earlier one is
/// dropped when it comes.
fn next_generation(engine: &Engine, doc: ObjectId) -> Option<u64> {
    engine
        .with_mut(doc, |d: &mut TextDocument| {
            d.generation += 1;
            d.generation
        })
        .ok()
}

fn is_current(engine: &Engine, doc: ObjectId, generation: u64) -> bool {
    engine.is_alive(doc)
        && engine
            .with(doc, |d: &TextDocument| d.generation == generation)
            .unwrap_or(false)
}

type Work = Box<dyn FnOnce() + Send>;

/// Runs `work` on the compute pool, or here when the pool is full: late
/// beats never.
fn off_thread(work: impl FnOnce() + Send + 'static) {
    let slot: Arc<Mutex<Option<Work>>> = Arc::new(Mutex::new(Some(Box::new(work))));
    let run = |slot: &Mutex<Option<Work>>| {
        let work = slot.lock().unwrap_or_else(PoisonError::into_inner).take();
        if let Some(work) = work {
            work();
        }
    };
    let for_pool = Arc::clone(&slot);
    if ComputePool::global()
        .spawn(Lane::Visible, move || run(&for_pool))
        .is_err()
    {
        run(&slot);
    }
}

/// A file as the application names it, and where it is.
struct Target {
    named: PathBuf,
    real: PathBuf,
}

/// The file of `url`, which `doc` gives relative to its editor's file.
/// `None`: non-local or denied by the sandbox, reported on the document.
fn local_path(
    engine: &Engine,
    doc: ObjectId,
    edit: ObjectId,
    url: &str,
    error_status: i32,
) -> Option<Target> {
    let resolved = engine.resolve_url(edit, url);
    let Some(named) = url_path(&resolved) else {
        fail(
            engine,
            doc,
            status::NON_LOCAL_FILE_ERROR,
            format!("Cannot load from non-local file {resolved}"),
        );
        return None;
    };
    // A sandboxed application's `/` is a directory of its own.
    let real = match Platform::of(engine).env().real(&named) {
        Ok(real) => real,
        Err(err) => {
            fail(engine, doc, error_status, io_message(&named, &err));
            return None;
        }
    };
    Some(Target { named, real })
}

fn io_message(path: &Path, err: &std::io::Error) -> String {
    match err.kind() {
        std::io::ErrorKind::NotFound => format!("{} does not exist", path.display()),
        _ => format!("{}: {err}", path.display()),
    }
}

/// `source` was written: reads the file it names into the editor.
pub(crate) fn load(engine: &Engine, doc: ObjectId, edit: ObjectId, st: &SharedState, url: &str) {
    st.borrow_mut().doc_source = url.to_owned();
    let Some(generation) = next_generation(engine, doc) else {
        return;
    };
    if url.is_empty() {
        set_status(engine, doc, status::NULL, "");
        return;
    }
    let Some(Target { named, real }) = local_path(engine, doc, edit, url, status::READ_ERROR)
    else {
        return;
    };
    set_status(engine, doc, status::LOADING, "");
    let reply: Reply = Platform::of(engine).expect_for("TextDocument load", Some(doc));
    off_thread(move || {
        let read = std::fs::read(&real).map(|b| String::from_utf8_lossy(&b).into_owned());
        reply.send(move |e| loaded(e, doc, edit, generation, &named, read));
    });
}

fn loaded(
    engine: &Engine,
    doc: ObjectId,
    edit: ObjectId,
    generation: u64,
    path: &Path,
    read: std::io::Result<String>,
) {
    if !is_current(engine, doc, generation) {
        return;
    }
    let Some(st) = state_of(engine, edit) else {
        return;
    };
    let text = match read {
        Ok(text) => text,
        Err(err) => {
            fail(engine, doc, status::READ_ERROR, io_message(path, &err));
            return;
        }
    };
    let editor = text_format_of(engine, edit);
    let kind = Kind::of_editor(editor)
        .or_else(|| Kind::of_suffix(path))
        .unwrap_or(Kind::Plain);
    let multi = st.borrow().multi;
    let font = super::rich::font_of(engine, edit);
    {
        let mut s = st.borrow_mut();
        let s = &mut *s;
        let format = if multi { kind.format() } else { None };
        super::rich::assign(s, format, &text, &font);
        s.model.move_to(0, false);
        s.model.set_modified(false);
        s.preedit.clear();
        s.preedit_cursor = None;
        s.editing = false;
        s.run = None;
        s.sticky = None;
    }
    check(engine, "source", publish(engine, edit, &st));
    set_status(engine, doc, status::LOADED, "");
}

/// `save()` (`to`: `None`) or `saveAs(to)`: writes the text to the file.
pub(crate) fn save(engine: &Engine, doc: ObjectId, to: Option<&str>) {
    let Some(edit) = engine
        .untrack(|| engine.with(doc, |d: &TextDocument| d.edit))
        .ok()
        .flatten()
    else {
        return;
    };
    let Some(st) = super::ops::ready(engine, edit) else {
        return;
    };
    let url = match to {
        Some(u) => u.to_owned(),
        None => st.borrow().doc_source.clone(),
    };
    if url.is_empty() {
        fail(
            engine,
            doc,
            status::WRITE_ERROR,
            "Cannot save: the document has no source".to_owned(),
        );
        return;
    }
    let Some(Target { named, real }) = local_path(engine, doc, edit, &url, status::WRITE_ERROR)
    else {
        return;
    };
    let Some(generation) = next_generation(engine, doc) else {
        return;
    };
    let kind = Kind::of_suffix(&named)
        .or_else(|| Kind::of_editor(text_format_of(engine, edit)))
        .unwrap_or_else(|| match st.borrow().rich.as_ref().map(|r| r.format) {
            Some(TextFormat::Markdown) => Kind::Markdown,
            Some(_) => Kind::Html,
            None => Kind::Plain,
        });
    let font = super::rich::font_of(engine, edit);
    // Written out here, where the document is; only the bytes travel.
    let (contents, saved_text) = {
        let mut s = st.borrow_mut();
        super::rich::sync_doc(&mut s);
        let text = s.model.text().to_owned();
        let contents = match (kind, s.rich.as_ref()) {
            (Kind::Plain, _) => text.replace(ljos_text::rich::OBJECT, ""),
            (_, Some(r)) => {
                super::rich::write_out(&r.doc, kind.format().unwrap_or_default(), &font)
            }
            (_, None) => super::rich::write_out(
                &RichText::plain(&text),
                kind.format().unwrap_or_default(),
                &font,
            ),
        };
        (contents, text)
    };
    set_status(engine, doc, status::SAVING, "");
    let renamed = to.map(str::to_owned);
    let reply: Reply = Platform::of(engine).expect_for("TextDocument save", Some(doc));
    off_thread(move || {
        let written = std::fs::write(&real, contents.as_bytes());
        reply.send(move |e| {
            saved(
                e,
                doc,
                edit,
                generation,
                &named,
                written,
                renamed,
                &saved_text,
            );
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn saved(
    engine: &Engine,
    doc: ObjectId,
    edit: ObjectId,
    generation: u64,
    path: &Path,
    written: std::io::Result<()>,
    renamed: Option<String>,
    text: &str,
) {
    if !is_current(engine, doc, generation) {
        return;
    }
    if let Err(err) = written {
        fail(engine, doc, status::WRITE_ERROR, io_message(path, &err));
        return;
    }
    let Some(st) = state_of(engine, edit) else {
        return;
    };
    if let Some(url) = renamed {
        // Said first, so the watch of `source` knows the file is the one
        // the editor holds and does not read it again.
        st.borrow_mut().doc_source = url.clone();
        set(
            engine,
            doc,
            TextDocument::SOURCE.index,
            Value::Url(Atom::from(url.as_str())),
        );
    }
    let unchanged = {
        let mut s = st.borrow_mut();
        let same = s.model.text() == text;
        if same {
            s.model.set_modified(false);
            s.run = None;
            super::rich::saved(&mut s);
        }
        same
    };
    if unchanged {
        sync_modified(engine, &st);
    }
    set_status(engine, doc, status::SAVED, "");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_suffix_names_what_a_file_holds() {
        let kind = |p: &str| Kind::of_suffix(Path::new(p));
        assert_eq!(kind("/a/notes.md"), Some(Kind::Markdown));
        assert_eq!(kind("/a/notes.MARKDOWN"), Some(Kind::Markdown));
        assert_eq!(kind("/a/page.htm"), Some(Kind::Html));
        assert_eq!(kind("/a/plain.txt"), Some(Kind::Plain));
        assert_eq!(kind("/a/unknown.cfg"), None);
        assert_eq!(kind("/a/none"), None);
    }

    #[test]
    fn the_editor_format_wins_over_the_suffix_except_auto() {
        assert_eq!(Kind::of_editor(text_format::RICH_TEXT), Some(Kind::Html));
        assert_eq!(Kind::of_editor(text_format::AUTO_TEXT), None);
    }

    #[test]
    fn a_missing_file_is_said_as_qt_says_it() {
        let err = std::io::Error::from(std::io::ErrorKind::NotFound);
        assert_eq!(
            io_message(Path::new("/x/missing.md"), &err),
            "/x/missing.md does not exist"
        );
    }
}
