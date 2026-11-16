//! `ljos i18n extract`: lists the translatable strings of a tree in a
//! catalogue, keeping what translators already did (ARCHITECTURE.md §15).
//! Ljós's own messages (`` t`…` `` and its family) go to a PO file, Qt's
//! `qsTr` to a Qt Linguist `.ts` file; the catalogue's extension says
//! which.

use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, bail};
use clap::{Args, Subcommand};
use ljos_compiler::i18n::extract::{Messages, messages_module, messages_qml};
use ljos_compiler::i18n::{Extraction, extract_js, extract_qml};
use ljos_i18n::po::{PoEntry, PoFile};
use ljos_i18n::ts::{Extracted, TsFile, TsLocation};

#[derive(Subcommand)]
pub enum I18nCommand {
    /// Find every translatable message in a tree and merge them into a
    /// catalogue: a `.po` (or `.pot`) file gets Ljós's `t` messages, a
    /// `.ts` file Qt's `qsTr` texts.
    Extract(Extract),
}

#[derive(Args)]
pub struct Extract {
    /// The directory to search: every `.qml` file below it, with the `.js`
    /// files those import for a `.ts` catalogue, and the `.ts` and `.lts`
    /// modules below it for a `.po` one.
    dir: PathBuf,
    /// The catalogue to create or bring up to date: `is.po`, `app.pot` or
    /// `app_is.ts`. Translations in it are kept; texts that left the
    /// sources are kept as obsolete if they were translated. A `.pot`
    /// template has no translations.
    #[arg(short, long)]
    output: PathBuf,
    /// Write nothing; fail if the file's messages differ from the sources
    /// (a text added, changed or gone). Line numbers alone do not count.
    #[arg(long)]
    check: bool,
    /// The language of the translations (`is_IS`, `de`), for a new file or
    /// one that does not say. A `.po` file without one takes its name
    /// (`is.po`); in a `.ts` file it decides how many plural forms a text
    /// gets.
    #[arg(long, alias = "target-language")]
    language: Option<String>,
}

pub fn run(command: I18nCommand) -> anyhow::Result<()> {
    match command {
        I18nCommand::Extract(extract) => extract.run(),
    }
}

/// Which catalogue a file is, by its extension.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Po,
    Pot,
    Ts,
}

impl Kind {
    fn of(path: &Path) -> anyhow::Result<Self> {
        match path.extension().and_then(|e| e.to_str()) {
            Some("po") => Ok(Self::Po),
            Some("pot") => Ok(Self::Pot),
            Some("ts") => Ok(Self::Ts),
            _ => bail!(
                "{}: a catalogue is a `.po` or `.pot` file (Ljós's `t` messages) or a `.ts` file (Qt's `qsTr`)",
                path.display()
            ),
        }
    }
}

impl Extract {
    fn run(self) -> anyhow::Result<()> {
        match Kind::of(&self.output)? {
            Kind::Ts => self.run_ts(),
            kind => self.run_po(kind == Kind::Pot),
        }
    }

    fn run_ts(self) -> anyhow::Result<()> {
        let sources = Sources::read(&self.dir)?;
        // Qt Linguist finds the sources by paths relative to the `.ts` file.
        let base = absolute(self.output.parent().unwrap_or(Path::new("")))?;
        let extracted = sources.extracted(&base);

        let before = match fs::read_to_string(&self.output) {
            Ok(text) => Some(
                TsFile::read(&text)
                    .with_context(|| format!("reading {}", self.output.display()))?,
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e).with_context(|| format!("reading {}", self.output.display())),
        };
        let mut after = before.clone().unwrap_or_default();
        if let Some(language) = &self.language {
            after.language = Some(language.clone());
        }
        let report = after.merge(&extracted);

        for note in &sources.notes {
            eprintln!("{note}");
        }
        let others = MessageSources::read(&self.dir, &base)?.found();
        if others > 0 {
            eprintln!(
                "{others} t message(s) go to a .po catalogue: ljos i18n extract {} -o <language>.po",
                self.dir.display()
            );
        }
        let mut lines = vec![
            format!(
                "Searched {} QML and {} JavaScript file(s)",
                sources.qml, sources.js
            ),
            format!(
                "Found {} source text(s) ({} new and {} already existing)",
                report.found,
                report.new,
                report.found - report.new
            ),
        ];
        if report.kept_obsolete > 0 {
            lines.push(format!(
                "Kept {} translated text(s) that are no longer in the sources",
                report.kept_obsolete
            ));
        }
        if report.dropped > 0 {
            lines.push(format!(
                "Removed {} untranslated text(s) that are no longer in the sources",
                report.dropped
            ));
        }

        if self.check {
            let current = before.is_some_and(|before| messages_of(before) == messages_of(after));
            if !current {
                say(&lines);
                bail!(
                    "{} is out of date with the sources; run without --check to update it",
                    self.output.display()
                );
            }
            say(&lines);
            return Ok(());
        }
        fs::write(&self.output, after.write())
            .with_context(|| format!("writing {}", self.output.display()))?;
        lines.push(format!("Updated {}", self.output.display()));
        say(&lines);
        Ok(())
    }
}

impl Extract {
    fn run_po(self, template: bool) -> anyhow::Result<()> {
        // As in a `.ts` file, a place is a path from the catalogue.
        let base = absolute(self.output.parent().unwrap_or(Path::new("")))?;
        let sources = MessageSources::read(&self.dir, &base)?;
        let name = self.output.display();

        let before = match fs::read_to_string(&self.output) {
            Ok(text) => Some(PoFile::parse(&text).with_context(|| format!("reading {name}"))?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e).with_context(|| format!("reading {name}")),
        };
        let mut after = before.clone().unwrap_or_default();
        let report = after.merge(sources.entries());
        let language = if template {
            // A template is what every language starts from: it has no
            // translations, and so nothing obsolete.
            after.entries.retain(|entry| !entry.obsolete);
            for entry in &mut after.entries {
                entry.translation.clear();
            }
            None
        } else {
            Some(
                self.language
                    .clone()
                    .or_else(|| after.language.clone())
                    .unwrap_or_else(|| stem(&self.output)),
            )
        };
        after.language.clone_from(&language);

        for note in &sources.notes {
            eprintln!("{note}");
        }
        let qt = Sources::read(&self.dir)?;
        let calls: usize = qt.files.iter().map(|(_, e)| e.messages.len()).sum();
        if calls > 0 {
            eprintln!(
                "{calls} qsTr call(s) go to a .ts catalogue: ljos migrate rewrites them as t messages"
            );
        }

        let written = after.write(language.as_deref());
        if self.check {
            let current = before.is_some_and(|before| {
                before.without_origins().write(language.as_deref())
                    == after.without_origins().write(language.as_deref())
            });
            if !current {
                bail!(
                    "{name} is behind the sources: {} new, {} gone; run without --check to update it",
                    report.new,
                    report.gone
                );
            }
            return Ok(());
        }
        fs::write(&self.output, written).with_context(|| format!("writing {name}"))?;
        say(&[format!(
            "{name}: {} messages, {} new, {} translated, {} obsolete",
            report.messages,
            report.new,
            if template { 0 } else { report.translated },
            if template { 0 } else { report.obsolete },
        )]);
        Ok(())
    }
}

/// Ljós's messages in the files of a tree.
struct MessageSources {
    /// Each file, as a path from the catalogue, with what was found in it,
    /// in the order searched.
    files: Vec<(String, Messages)>,
    /// `file:line: message` for every macro that says no message.
    notes: Vec<String>,
}

impl MessageSources {
    fn read(dir: &Path, base: &Path) -> anyhow::Result<Self> {
        let mut paths = Vec::new();
        crate::walk::files_below(dir, is_source, &mut paths)
            .with_context(|| format!("searching {}", dir.display()))?;
        paths.sort_by_cached_key(|path| (path.to_string_lossy().to_lowercase(), path.clone()));
        let mut sources = Self {
            files: Vec::new(),
            notes: Vec::new(),
        };
        for path in paths {
            let text = read(&path)?;
            let messages = if crate::walk::is_qml(&path) {
                messages_qml(&text)
            } else if is_catalogue(&text) {
                // A `.ts` that is Qt Linguist's catalogue, not a module.
                continue;
            } else {
                messages_module(&path.to_string_lossy(), &text)
            };
            if messages.syntax_errors > 0 {
                sources.notes.push(format!(
                    "{}: {} syntax error(s); messages in the parts that did not parse are missing",
                    path.display(),
                    messages.syntax_errors
                ));
            }
            for note in &messages.notes {
                sources.notes.push(format!(
                    "{}:{}: {}",
                    path.display(),
                    note.line,
                    note.message
                ));
            }
            let place = absolute(&path)
                .map(|path| relative(&path, base))
                .unwrap_or(path);
            // Catalogues are read on every platform.
            let place = place.to_string_lossy().replace('\\', "/");
            sources.files.push((place, messages));
        }
        Ok(sources)
    }

    fn found(&self) -> usize {
        self.files.iter().map(|(_, m)| m.messages.len()).sum()
    }

    /// One entry for each message where it is found, in the order found.
    fn entries(&self) -> impl Iterator<Item = PoEntry> + '_ {
        self.files.iter().flat_map(|(place, messages)| {
            messages.messages.iter().map(move |found| {
                let call = &found.call;
                let explicit = call.id.is_some();
                PoEntry {
                    context: call.context.clone(),
                    id: call.key().unwrap_or_default().to_owned(),
                    comments: call.comment.iter().cloned().collect(),
                    explicit_id: explicit,
                    default: if explicit { call.message.clone() } else { None },
                    origins: vec![format!("{place}:{}", found.line)],
                    ..PoEntry::default()
                }
            })
        })
    }
}

/// A file that may hold Ljós's messages: QML, or a `.ts` or `.lts` module
/// (not a declaration file, which runs nothing).
fn is_source(path: &Path) -> bool {
    let name = path.to_string_lossy();
    crate::walk::is_qml(path)
        || name.ends_with(".lts")
        || (name.ends_with(".ts") && !name.ends_with(".d.ts"))
}

/// Whether a `.ts` file's text is a Qt Linguist catalogue, which is XML.
pub(crate) fn is_catalogue(text: &str) -> bool {
    let start = text.trim_start_matches('\u{feff}').trim_start();
    start.starts_with("<?xml") || start.starts_with("<!DOCTYPE TS") || start.starts_with("<TS")
}

/// Prints the summary. A reader that stopped reading (`| head`) is not an
/// error: the work is done by then.
fn say(lines: &[String]) {
    let mut out = std::io::stdout().lock();
    for line in lines {
        if writeln!(out, "{line}").is_err() {
            break;
        }
    }
}

/// A file without where its messages are used: a line that moved is not a
/// reason for a translator to look again.
fn messages_of(mut file: TsFile) -> TsFile {
    for context in &mut file.contexts {
        for message in &mut context.messages {
            message.locations.clear();
        }
    }
    file
}

/// What the files of a tree hold.
#[derive(Default)]
struct Sources {
    /// Each file with what was found in it, in the order searched.
    files: Vec<(PathBuf, Extraction)>,
    /// `file:line: message` for everything a translator will not get.
    notes: Vec<String>,
    qml: usize,
    js: usize,
}

impl Sources {
    fn read(dir: &Path) -> anyhow::Result<Self> {
        let mut qml = Vec::new();
        crate::walk::files_below(dir, crate::walk::is_qml, &mut qml)
            .with_context(|| format!("searching {}", dir.display()))?;
        // Without regard to case, as `lupdate` orders them, so that the two
        // tools write the messages of a context in the same order.
        qml.sort_by_cached_key(|path| (path.to_string_lossy().to_lowercase(), path.clone()));

        let mut sources = Self::default();
        let mut scripts: Vec<PathBuf> = Vec::new();
        for path in qml {
            let text = read(&path)?;
            let extraction = extract_qml(&text, &stem(&path));
            sources.qml += 1;
            scripts.extend(sources.add(path, extraction));
        }
        // A script is searched once however many files import it.
        let mut seen = BTreeSet::new();
        while let Some(path) = scripts.pop() {
            let Ok(canonical) = path.canonicalize() else {
                // An import of a file that is not there is the compiler's
                // business; here there is only nothing to find.
                continue;
            };
            if !seen.insert(canonical) {
                continue;
            }
            let text = read(&path)?;
            let extraction = extract_js(&text, &stem(&path));
            sources.js += 1;
            scripts.extend(sources.add(path, extraction));
        }
        Ok(sources)
    }

    /// Records a file; gives back the scripts it imports.
    fn add(&mut self, path: PathBuf, extraction: Extraction) -> Vec<PathBuf> {
        let dir = path.parent().unwrap_or(Path::new(""));
        let scripts = extraction
            .scripts
            .iter()
            .filter(|script| !script.contains("://") && !script.starts_with("qrc:"))
            .map(|script| dir.join(script))
            .collect();
        if extraction.syntax_errors > 0 {
            self.notes.push(format!(
                "{}: {} syntax error(s); texts in the parts that did not parse are missing",
                path.display(),
                extraction.syntax_errors
            ));
        }
        for note in &extraction.notes {
            self.notes.push(format!(
                "{}:{}: {}",
                path.display(),
                note.line,
                note.message
            ));
        }
        self.files.push((path, extraction));
        scripts
    }

    fn extracted(&self, base: &Path) -> Vec<Extracted> {
        let mut out = Vec::new();
        for (path, extraction) in &self.files {
            let filename = absolute(path)
                .map(|path| relative(&path, base))
                .unwrap_or_else(|_| path.clone());
            // `.ts` files are read on every platform.
            let filename = filename.to_string_lossy().replace('\\', "/");
            out.extend(extraction.messages.iter().map(|m| Extracted {
                context: m.context.clone(),
                source: m.source.clone(),
                comment: m.disambiguation.clone(),
                id: m.id.clone(),
                numerus: m.plural,
                extra_comment: m.extra_comment.clone(),
                extras: m.extras.clone(),
                location: TsLocation {
                    filename: Some(filename.clone()),
                    line: Some(m.line.to_string()),
                },
            }));
        }
        out
    }
}

fn read(path: &Path) -> anyhow::Result<String> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    // A stray Latin-1 byte in a comment must not hide a file's texts.
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// `path` from the current directory, with `.` and `..` worked out, without
/// asking the file system (the `.ts` file need not exist yet).
fn absolute(path: &Path) -> std::io::Result<PathBuf> {
    let joined = std::env::current_dir()?.join(path);
    let mut out = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    Ok(out)
}

/// `path` as seen from the directory `base`; both absolute.
fn relative(path: &Path, base: &Path) -> PathBuf {
    let path: Vec<_> = path.components().collect();
    let base: Vec<_> = base.components().collect();
    let shared = path.iter().zip(&base).take_while(|(a, b)| a == b).count();
    let mut out = PathBuf::new();
    for _ in shared..base.len() {
        out.push("..");
    }
    for component in &path[shared..] {
        out.push(component);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths() {
        let r = |path: &str, base: &str| relative(Path::new(path), Path::new(base));
        assert_eq!(r("/a/b/Main.qml", "/a/b"), Path::new("Main.qml"));
        assert_eq!(r("/a/b/ui/Main.qml", "/a/b"), Path::new("ui/Main.qml"));
        assert_eq!(
            r("/a/src/Main.qml", "/a/i18n"),
            Path::new("../src/Main.qml")
        );
    }
}
