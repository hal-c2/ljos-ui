//! QML a running program holds as text (`Qt.createQmlObject`).
//!
//! Only a development build can make an object of it: the text has to be
//! compiled, and a shipped program has no compiler. The engine asks
//! whoever was installed with [`Engine::set_qml_text_compiler`]; this is
//! what a development host installs.
//!
//! [`Engine::set_qml_text_compiler`]: ljos_engine::Engine::set_qml_text_compiler

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ljos_compiler::{Diagnostic, Project, Severity, compile_text};
use ljos_engine::{QmlText, QmlTextCompiler, QmlTextError};
use qml_front::{QtTypeOracle, TypeOracle};

/// Compiles texts against `project`: a text sees the types a file of its
/// key would see.
pub fn text_compiler<O: TypeOracle + 'static>(project: Project<O>) -> QmlTextCompiler {
    let project = RefCell::new(project);
    Rc::new(move |text: QmlText<'_>| compile(&mut project.borrow_mut(), text))
}

/// The texts of a program whose files are a directory that changes while
/// it runs.
///
/// The directory is read when a text is asked for, not when a file is
/// saved: most programs never make an object from text.
#[derive(Debug)]
pub struct ProjectTexts {
    dirs: Vec<PathBuf>,
    project: RefCell<Option<Project<QtTypeOracle>>>,
}

impl ProjectTexts {
    #[must_use]
    pub fn new(dir: &Path) -> Rc<Self> {
        Self::over(&[dir.to_owned()])
    }

    /// Over the files of every one of `dirs`, an import path.
    #[must_use]
    pub fn over(dirs: &[PathBuf]) -> Rc<Self> {
        Rc::new(Self {
            dirs: dirs.to_vec(),
            project: RefCell::new(None),
        })
    }

    /// The files changed: the next text is compiled against them as they
    /// are then.
    pub fn changed(&self) {
        self.project.take();
    }

    #[must_use]
    pub fn compiler(self: &Rc<Self>) -> QmlTextCompiler {
        let texts = self.clone();
        Rc::new(move |text: QmlText<'_>| {
            let mut project = texts.project.borrow_mut();
            let project = match &mut *project {
                Some(project) => project,
                empty => {
                    let dirs: Vec<&Path> = texts.dirs.iter().map(PathBuf::as_path).collect();
                    let loaded = Project::load_roots(&dirs, QtTypeOracle).map_err(|e| {
                        vec![QmlTextError {
                            line: 0,
                            column: 0,
                            message: e.to_string(),
                        }]
                    })?;
                    empty.insert(loaded)
                }
            };
            compile(project, text)
        })
    }
}

fn compile<O: TypeOracle>(
    project: &mut Project<O>,
    text: QmlText<'_>,
) -> Result<ljos_ir::CompiledFile, Vec<QmlTextError>> {
    match compile_text(project, text.key, text.text) {
        Ok(compiled) if !compiled.has_errors() => Ok(compiled.file),
        Ok(compiled) => Err(errors(&compiled.diagnostics)),
        Err(diagnostics) => Err(errors(&diagnostics)),
    }
}

fn errors(diagnostics: &[Diagnostic]) -> Vec<QmlTextError> {
    diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| QmlTextError {
            line: d.span.line,
            column: d.span.column,
            message: d.message.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> Project<QtTypeOracle> {
        let mut project = Project::new(QtTypeOracle);
        project.add("Panel", "import QtQuick\nItem { property int n: 1 }\n");
        project.add("dyn/Panel", "import QtQuick\nItem { property int n: 2 }\n");
        project
    }

    #[test]
    fn a_text_is_compiled_as_a_file_of_its_key() {
        let compiler = text_compiler(project());
        let here = compiler(QmlText {
            key: "inline",
            url: "file:///inline.qml",
            text: "import QtQuick\nPanel { }\n",
        })
        .expect("compiles");
        let there = compiler(QmlText {
            key: "dyn/snippet",
            url: "file:///dyn/snippet.qml",
            text: "import QtQuick\nPanel { }\n",
        })
        .expect("compiles");
        assert_ne!(
            format!("{:?}", here.components),
            format!("{:?}", there.components),
            "each finds the Panel beside it"
        );
    }

    #[test]
    fn what_is_wrong_with_a_text_says_where() {
        let compiler = text_compiler(project());
        let errors = compiler(QmlText {
            key: "inline",
            url: "file:///inline.qml",
            text: "import QtQuick\nItem { ",
        })
        .expect_err("it does not parse");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, 2);
        assert!(errors[0].message.contains("Expected"), "{errors:?}");
    }

    #[test]
    fn a_directory_is_read_again_after_a_change() {
        let dir = tempfile::tempdir().expect("a directory");
        let texts = ProjectTexts::new(dir.path());
        let compiler = texts.compiler();
        let text = QmlText {
            key: "inline",
            url: "file:///inline.qml",
            text: "import QtQuick\nPanel { }\n",
        };
        assert!(compiler(text).is_err(), "there is no Panel yet");
        std::fs::write(dir.path().join("Panel.qml"), "import QtQuick\nItem { }\n")
            .expect("written");
        assert!(compiler(text).is_err(), "nobody said the files changed");
        texts.changed();
        assert!(compiler(text).is_ok());
    }
}
