//! QML a scenario's script holds as text (`Qt.createQmlObject`): compiled
//! against the scenario's files, as a development build does.

use std::collections::BTreeMap;

use ljos_compiler::Project;
use ljos_engine::QmlTextCompiler;
use qml_front::QtTypeOracle;

use super::{key_of, qmldir_of};

pub(super) fn compiler(files: &BTreeMap<String, String>) -> QmlTextCompiler {
    let mut project = Project::new(QtTypeOracle);
    for (name, source) in files {
        if qmldir_of(name).is_none() {
            project.add(key_of(name), source.as_str());
        }
    }
    // After the files, which are what a module's lines name.
    for (name, source) in files {
        if let Some(dir) = qmldir_of(name) {
            project.add_qmldir(dir, source);
        }
    }
    ljos_dev::text_compiler(project)
}
