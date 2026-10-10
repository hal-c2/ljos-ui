//! `ljos i18n extract`: lists the translatable strings of a QML tree in a Qt
//! Linguist `.ts` file, keeping what translators already did
//! (ARCHITECTURE.md §15).

use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, bail};
use clap::{Args, Subcommand};
use ljos_compiler::i18n::{Extraction, extract_js, extract_qml};
use ljos_i18n::ts::{Extracted, TsFile, TsLocation};

#[derive(Subcommand)]
pub enum I18nCommand {
    /// Find every translatable string in a QML tree and merge them into a
    /// `.ts` file.
    Extract(Extract),
}

#[derive(Args)]
pub struct Extract {
    /// The directory to search: every `.qml` file below it, and the `.js`
    /// files those import.
    dir: PathBuf,
    /// The `.ts` file to create or bring up to date. Translations in it are
    /// kept; texts that left the sources are kept as vanished if they were
    /// translated.
    #[arg(short, long)]
    output: PathBuf,
    /// Write nothing; fail if the file's messages differ from the sources
    /// (a text added, changed or gone). Line numbers alone do not count.
    #[arg(long)]
    check: bool,
    /// The language of the translations (`is_IS`, `de`), for a new file or
    /// one that does not say. It decides how many plural forms a text gets.
    #[arg(long)]
    target_language: Option<String>,
}

pub fn run(command: I18nCommand) -> anyhow::Result<()> {
    match command {
        I18nCommand::Extract(extract) => extract.run(),
    }
}

impl Extract {
    fn run(self) -> anyhow::Result<()> {
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
        if let Some(language) = &self.target_language {
            after.language = Some(language.clone());
        }
        let report = after.merge(&extracted);

        for note in &sources.notes {
            eprintln!("{note}");
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
        qml_files(dir, &mut qml).with_context(|| format!("searching {}", dir.display()))?;
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

/// Every `.qml` file below `dir`. Hidden directories and links to
/// directories are not entered: the first hold tools' files, the second can
/// loop.
fn qml_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_dir() {
            if !entry.file_name().to_string_lossy().starts_with('.') {
                qml_files(&path, out)?;
            }
        } else if path.extension().is_some_and(|extension| extension == "qml")
            && (kind.is_file() || path.is_file())
        {
            out.push(path);
        }
    }
    Ok(())
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
