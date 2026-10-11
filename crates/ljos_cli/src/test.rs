//! `ljos test`: runs the `TestCase`s of `tst_*.qml` files and prints what
//! Qt's runner for QML tests prints.
//!
//! The runner itself is `ljos_test::qttest`, which is given compiled files
//! and links no parser. This is the part that has one: it finds the test
//! files, compiles them with everything beside them and in the directories
//! named with `-import`, and gives each test file an engine of its own, so
//! that what one file leaves behind cannot change the next one's results.
//!
//! The report goes to standard output as it happens; what a file says
//! while it is being loaded, and what does not compile in a file that is
//! not a test, go to standard error. The exit status is the number of
//! failed tests.

use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    ffi::OsString,
    path::{Component, Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use anyhow::Context as _;
use clap::Args;
use ljos_compiler::{Compiled, Diagnostic, Project, Severity, compile_file, compile_text};
use ljos_engine::{ComponentSource, Engine, QmlText, QmlTextCompiler, QmlTextError};
use ljos_ir::{CompiledFile, ImportSource};
use ljos_test::qttest::{Filter, Options, Session};
// `TestCase` and `SignalSpy` are in the table of types every file is read
// against (`qt_types`), as Qt's own are.
use qml_front::QtTypeOracle;

/// What a test file's name starts with.
const TEST_PREFIX: &str = "tst_";

/// An import directory is not read deeper than this, so one that is not a
/// directory of modules (a home directory) cannot stall a run.
const MAX_IMPORT_DIRS: usize = 20_000;

#[derive(Args)]
pub struct Test {
    /// Test files (`tst_*.qml`), or directories searched for them. The
    /// current directory when none is given.
    paths: Vec<PathBuf>,
    /// A directory of modules only the tests import. May be given more
    /// than once; `-import`, as Qt's runner spells it, is read too.
    #[arg(long = "import", value_name = "DIR")]
    imports: Vec<PathBuf>,
    /// Run only what is named: `Case::function`, `Case::function:tag`, a
    /// test case or a function. May be given more than once.
    #[arg(long, value_name = "NAME")]
    filter: Vec<String>,
    /// Also write the results to this file, as JSON.
    #[arg(long, value_name = "FILE")]
    json: Option<PathBuf>,
}

/// The command line with Qt's spelling of the options turned into ours:
/// a test suite is run by the same script on both runners, and clap reads
/// `-import` as seven one-letter options.
pub fn arguments(args: impl Iterator<Item = OsString>) -> Vec<OsString> {
    let args: Vec<OsString> = args.collect();
    if args.get(1).is_none_or(|command| command != "test") {
        return args;
    }
    args.into_iter()
        .map(|arg| {
            if arg == "-import" {
                "--import".into()
            } else {
                arg
            }
        })
        .collect()
}

/// The files of one directory to run: all its test files, or the ones
/// named.
struct Suite {
    root: PathBuf,
    /// Keys of the files asked for by name; every test file when `None`.
    named: Option<Vec<String>>,
}

impl Test {
    /// Runs the tests and says how many failed, as an exit status.
    pub fn run(self) -> anyhow::Result<u8> {
        let imports = self
            .imports
            .iter()
            .map(|dir| {
                dir.canonicalize()
                    .with_context(|| format!("{}", dir.display()))
            })
            .collect::<anyhow::Result<Vec<PathBuf>>>()?;
        let suites = self.suites()?;
        let options = Options {
            filters: self.filter.iter().map(|f| Filter::parse(f)).collect(),
            // `grabImage(..).save("x.png")` writes beside where the run
            // was started, as with Qt's runner.
            save_dir: std::env::current_dir().context("the current directory")?,
            ..Options::default()
        };
        let mut session = Session::new(
            options,
            Box::new(|line| println!("{line}")),
            Box::new(|line| eprintln!("{line}")),
        );
        let mut found = 0;
        for suite in &suites {
            found += suite.run(&mut session, &imports)?;
        }
        if found == 0 {
            anyhow::bail!("no {TEST_PREFIX}*.qml file to run");
        }
        let report = session.finish();
        if let Some(path) = &self.json {
            let mut text = serde_json::to_string_pretty(&report.to_json())
                .context("writing the report as JSON")?;
            text.push('\n');
            std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))?;
        }
        Ok(report.exit_code())
    }

    /// What was asked for, by directory, in the order asked.
    fn suites(&self) -> anyhow::Result<Vec<Suite>> {
        let here = [PathBuf::from(".")];
        let paths = if self.paths.is_empty() {
            &here[..]
        } else {
            &self.paths[..]
        };
        let mut suites: Vec<Suite> = Vec::new();
        for path in paths {
            let path = path
                .canonicalize()
                .with_context(|| format!("{}", path.display()))?;
            if path.is_dir() {
                suites.push(Suite {
                    root: path,
                    named: None,
                });
                continue;
            }
            let root = path
                .parent()
                .with_context(|| format!("{} is in no directory", path.display()))?
                .to_owned();
            let key = key(&root, &path);
            // Files of one directory are one project, compiled once.
            match suites.last_mut() {
                Some(Suite {
                    root: last,
                    named: Some(keys),
                }) if *last == root => keys.push(key),
                _ => suites.push(Suite {
                    root,
                    named: Some(vec![key]),
                }),
            }
        }
        Ok(suites)
    }
}

impl Suite {
    /// Compiles the directory and runs its test files. Says how many
    /// there were.
    fn run(&self, session: &mut Session, imports: &[PathBuf]) -> anyhow::Result<usize> {
        let root = &self.root;
        let project = project(root, imports)?;
        let tests: Vec<String> = match &self.named {
            Some(keys) => keys.clone(),
            None => project
                .files()
                .filter(|key| name(key).starts_with(TEST_PREFIX))
                // An import directory is not searched for tests.
                .filter(|key| !key.starts_with("../"))
                .map(str::to_owned)
                .collect(),
        };

        let mut files = HashMap::new();
        let mut broken: BTreeMap<String, Diagnostic> = BTreeMap::new();
        // What each test file imports, also of one that has a mistake: a
        // module that is not there is the first thing wrong with it.
        let mut modules: HashMap<String, Vec<String>> = HashMap::new();
        let keys: Vec<String> = project.files().map(str::to_owned).collect();
        for key in keys {
            let result = compile_file(&project, &key);
            if let Ok(compiled) = &result
                && tests.contains(&key)
            {
                modules.insert(key.clone(), runtime_modules(&compiled.file));
            }
            match compiled(result) {
                Ok(file) => {
                    files.insert(key, Arc::new(file));
                }
                Err(first) => {
                    broken.insert(key, first);
                }
            }
        }
        let source = Files(Rc::new(FileMap {
            root: root.clone(),
            files,
        }));
        for (key, diagnostic) in &broken {
            // One of a test file is a failed test in the report.
            if !tests.contains(key) {
                eprintln!("{}:{diagnostic}", path_of(root, key).display());
            }
        }

        let project = Rc::new(RefCell::new(project));
        for key in &tests {
            let stem = name(key);
            let file = path_of(root, key).to_string_lossy().into_owned();
            // An engine a file: a test file starts from nothing.
            let engine = Engine::new();
            ljos_quick::register(&engine);
            ljos_controls::register(&engine);
            ljos_test::qttest::register(&engine);
            // The engine would refuse the file too, but cannot say where
            // in it the import is.
            let missing = modules
                .get(key)
                .and_then(|uris| uris.iter().find(|uri| !engine.has_module(uri)));
            if let Some(uri) = missing {
                let line = import_line(project.borrow().source(key).unwrap_or_default(), uri);
                session.compile_failure(
                    stem,
                    &format!("module \"{uri}\" is not installed"),
                    Some((file, line)),
                );
                continue;
            }
            if let Some(diagnostic) = broken.get(key) {
                session.compile_failure(
                    stem,
                    &diagnostic.message,
                    Some((file, diagnostic.span.line)),
                );
                continue;
            }
            engine.set_component_source(source.clone());
            engine.set_qml_text_compiler(Some(text_compiler(project.clone())));
            session.run_file(&engine, key, stem);
        }
        Ok(tests.len())
    }
}

/// The modules a file imports that the runtime has to have: the ones no
/// file of the project is a type of.
fn runtime_modules(file: &CompiledFile) -> Vec<String> {
    file.imports
        .iter()
        .filter(|import| !import.in_project)
        .filter_map(|import| match &import.source {
            ImportSource::Module(uri) => Some(uri.clone()),
            ImportSource::Path(_) => None,
        })
        .collect()
}

/// The compiled file, or the first thing that was wrong with it. A file
/// with a mistake is not loaded in part: Qt loads none of it either.
fn compiled(result: Result<Compiled, Vec<Diagnostic>>) -> Result<CompiledFile, Diagnostic> {
    let first_error = |diagnostics: &[Diagnostic]| {
        diagnostics
            .iter()
            .find(|d| d.severity == Severity::Error)
            .cloned()
    };
    match result {
        Ok(compiled) => match first_error(&compiled.diagnostics) {
            None => Ok(compiled.file),
            Some(error) => Err(error),
        },
        Err(diagnostics) => Err(first_error(&diagnostics).unwrap_or_else(|| {
            Diagnostic::error("does not compile", ljos_ir::SourceSpan::default())
        })),
    }
}

/// Compiles the text of `Qt.createQmlObject` and `createTemporaryQmlObject`
/// against the files of the tests.
fn text_compiler(project: Rc<RefCell<Project<QtTypeOracle>>>) -> QmlTextCompiler {
    Rc::new(move |text: QmlText<'_>| {
        let errors = |diagnostics: &[Diagnostic]| -> Vec<QmlTextError> {
            diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Error)
                .map(|d| QmlTextError {
                    line: d.span.line,
                    column: d.span.column,
                    message: d.message.clone(),
                })
                .collect()
        };
        match compile_text(&mut project.borrow_mut(), text.key, text.text) {
            Ok(compiled) if !compiled.has_errors() => Ok(compiled.file),
            Ok(compiled) => Err(errors(&compiled.diagnostics)),
            Err(diagnostics) => Err(errors(&diagnostics)),
        }
    })
}

/// The files below `root`, and the modules of the import directories.
///
/// A module is one only where an import directory leads to it
/// (`<dir>/Some/Module/qmldir` is `Some.Module`), as Qt finds modules: a
/// `qmldir` that merely lies among the tests names none. So a test that
/// forgets its import path fails here as it fails on Qt.
fn project(root: &Path, imports: &[PathBuf]) -> anyhow::Result<Project<QtTypeOracle>> {
    // The loader knows which directories are not sources (build output,
    // hidden ones); it also takes every `qmldir` for a module, which is
    // why its files are put in a project of our own.
    let found = Project::load(root, QtTypeOracle).with_context(|| format!("{}", root.display()))?;
    let mut project = Project::new(QtTypeOracle);
    for key in found.files() {
        project.add(key, found.source(key).unwrap_or_default());
    }
    for dir in imports {
        add_import(&mut project, root, dir);
    }
    Ok(project)
}

/// Adds the modules of an import directory, and its files where they are
/// not below the tests. The key of such a file leads out of the root
/// (`../imports/Things/Thing`), which is where it is.
fn add_import(project: &mut Project<QtTypeOracle>, root: &Path, import: &Path) {
    let mut qmldirs = Vec::new();
    let mut stack = vec![import.to_owned()];
    let mut seen = 0;
    while let Some(dir) = stack.pop() {
        seen += 1;
        if seen > MAX_IMPORT_DIRS {
            break;
        }
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            // `file_type` does not follow links, so a link cannot lead
            // the walk round in a circle.
            match entry.file_type() {
                Ok(kind) if kind.is_dir() && !name.starts_with('.') => stack.push(path),
                Ok(kind) if kind.is_file() && name.ends_with(".qml") => {
                    let key = key(root, &path);
                    if project.source(&key).is_none()
                        && let Ok(text) = std::fs::read_to_string(&path)
                    {
                        project.add(&key, text);
                    }
                }
                Ok(kind) if kind.is_file() && name == "qmldir" => qmldirs.push(path),
                _ => {}
            }
        }
    }
    // A `qmldir` names files, which have to be there first.
    qmldirs.sort();
    for path in qmldirs {
        let (Some(dir), Ok(text)) = (path.parent(), std::fs::read_to_string(&path)) else {
            continue;
        };
        let at_its_place = qml_front::discover::qmldir(&text)
            .is_some_and(|module| import.join(module.uri.replace('.', "/")) == dir);
        if at_its_place {
            project.add_qmldir(&key(root, dir), &text);
        }
    }
}

/// The line a file imports the module `uri` on, by its text: the compiled
/// file does not keep it. 0 when it is not found.
fn import_line(source: &str, uri: &str) -> u32 {
    let line = source.lines().position(|line| {
        line.trim_start()
            .strip_prefix("import")
            .filter(|rest| rest.starts_with(char::is_whitespace))
            .and_then(|rest| rest.trim_start().strip_prefix(uri))
            .is_some_and(|rest| !rest.starts_with(|c: char| c == '.' || c.is_alphanumeric()))
    });
    line.map_or(0, |i| u32::try_from(i + 1).unwrap_or(0))
}

/// A file's key: its path from the root without `.qml`, with forward
/// slashes; `..` leads out of the root.
fn key(root: &Path, path: &Path) -> String {
    let from: Vec<Component<'_>> = root.components().collect();
    let to: Vec<Component<'_>> = path.components().collect();
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = vec!["..".to_owned(); from.len() - common];
    parts.extend(
        to[common..]
            .iter()
            .map(|part| part.as_os_str().to_string_lossy().into_owned()),
    );
    let text = parts.join("/");
    text.strip_suffix(".qml").unwrap_or(&text).to_owned()
}

/// The file of a key, without the `..` a key may have.
fn path_of(root: &Path, key: &str) -> PathBuf {
    let mut path = root.to_owned();
    for part in key.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                path.pop();
            }
            part => path.push(part),
        }
    }
    path.as_mut_os_string().push(".qml");
    path
}

/// A key's last part: the file's name without `.qml`.
fn name(key: &str) -> &str {
    key.rsplit('/').next().unwrap_or(key)
}

/// The compiled files of a directory and of what it imports, by key.
#[derive(Clone)]
struct Files(Rc<FileMap>);

struct FileMap {
    root: PathBuf,
    files: HashMap<String, Arc<CompiledFile>>,
}

impl ComponentSource for Files {
    fn load(&self, key: &str) -> Option<Arc<CompiledFile>> {
        self.0.files.get(key).cloned()
    }

    /// The file itself, so that a message names what an editor can open.
    fn url(&self, key: &str) -> String {
        format!(
            "file://{}",
            path_of(&self.0.root, key)
                .to_string_lossy()
                .replace('\\', "/")
        )
    }

    fn key(&self, url: &str) -> String {
        let url = url.split(['?', '#']).next().unwrap_or(url);
        match url.strip_prefix("file://") {
            Some(path) if path.starts_with('/') => key(&self.0.root, Path::new(path)),
            _ => url.strip_suffix(".qml").unwrap_or(url).to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_is_the_path_from_the_root() {
        let root = Path::new("/work/suite");
        assert_eq!(key(root, Path::new("/work/suite/tst_a.qml")), "tst_a");
        assert_eq!(
            key(root, Path::new("/work/suite/deep/tst_a.qml")),
            "deep/tst_a"
        );
        assert_eq!(
            key(root, Path::new("/work/imports/Things/Thing.qml")),
            "../imports/Things/Thing"
        );
        assert_eq!(
            key(root, Path::new("/work/imports/Things")),
            "../imports/Things"
        );
    }

    #[test]
    fn a_key_leads_back_to_its_file() {
        let root = Path::new("/work/suite");
        for path in [
            "/work/suite/tst_a.qml",
            "/work/suite/deep/tst_a.qml",
            "/work/imports/Things/Thing.qml",
        ] {
            assert_eq!(path_of(root, &key(root, Path::new(path))), Path::new(path));
        }
        assert_eq!(name("deep/tst_a"), "tst_a");
        assert_eq!(name("tst_a"), "tst_a");
    }

    #[test]
    fn the_line_of_an_import_is_found_in_the_text() {
        let text = "import QtQuick\n  import Helpers.More 1.0\nimport Helpers\nItem {}\n";
        assert_eq!(import_line(text, "QtQuick"), 1);
        assert_eq!(import_line(text, "Helpers.More"), 2);
        assert_eq!(import_line(text, "Helpers"), 3);
        assert_eq!(import_line(text, "Nothing"), 0);
    }

    #[test]
    fn urls_and_keys_are_each_others_reverse() {
        let files = Files(Rc::new(FileMap {
            root: PathBuf::from("/work/suite"),
            files: HashMap::new(),
        }));
        assert_eq!(files.url("deep/tst_a"), "file:///work/suite/deep/tst_a.qml");
        assert_eq!(files.key("file:///work/suite/deep/tst_a.qml"), "deep/tst_a");
        assert_eq!(
            files.url("../imports/Things/Thing"),
            "file:///work/imports/Things/Thing.qml"
        );
        assert_eq!(
            files.key("file:///work/imports/Things/Thing.qml?generation=2"),
            "../imports/Things/Thing"
        );
        // A bare key or file name is the file of that name.
        assert_eq!(files.key("Counter.qml"), "Counter");
        assert_eq!(files.key("Counter"), "Counter");
    }

    #[test]
    fn qts_spelling_of_the_import_option_is_read_for_this_command_only() {
        let args = |list: &[&str]| -> Vec<OsString> { arguments(list.iter().map(OsString::from)) };
        assert_eq!(
            args(&["ljos", "test", "suite", "-import", "imports"]),
            ["ljos", "test", "suite", "--import", "imports"].map(OsString::from)
        );
        assert_eq!(
            args(&["ljos", "run", "-import"]),
            ["ljos", "run", "-import"].map(OsString::from)
        );
    }
}
