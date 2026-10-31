//! `qml_edit::keys` names every object as the compiler does: a designer
//! that works on the text alone and hot reload, which works on what this
//! crate made, must agree on what an object's key is.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use ljos_compiler::{Project, compile_text};
use ljos_ir::{CompIdx, KeySeg, ObjIdx, ObjRef};
use qml_front::QtTypeOracle;

fn qml_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            qml_under(&path, out);
        } else if path.extension().is_some_and(|e| e == "qml") {
            out.push(path);
        }
    }
}

#[test]
fn qml_edit_keys_objects_as_the_compiler_does() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    for dir in ["crates", "examples", "bench", "corpus/qtdoc"] {
        qml_under(&root.join(dir), &mut files);
    }
    let mut project = Project::new(QtTypeOracle);
    let (mut checked, mut objects, mut stand_in) = (0, 0, 0);
    let mut wrong = Vec::new();
    for path in &files {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        if qml_edit::keys(&text).is_err() {
            continue;
        }
        let Ok(compiled) = compile_text(&mut project, "Main", &text) else {
            continue;
        };
        let file = &compiled.file;
        // Each component by its id after the file's key: what
        // `qml_edit::Object::component` holds.
        let mut theirs: BTreeMap<Option<String>, HashSet<Vec<KeySeg>>> = BTreeMap::new();
        for (c, component) in file.components.iter().enumerate() {
            let name = component.id.strip_prefix("Main").unwrap_or(&component.id);
            let name = (!name.is_empty()).then(|| name.to_owned());
            for o in 0..component.objects.len() {
                let key = file.object_key(ObjRef {
                    component: CompIdx(c as u32),
                    object: ObjIdx(o as u32),
                });
                theirs.entry(name.clone()).or_default().insert(key.segments);
            }
        }
        // Which values are templates is the types' to say: these are the
        // compiler's.
        let templates = |owner: &qml_edit::Object, property: &str| {
            let mut id = owner.component.clone().unwrap_or_default();
            for seg in &owner.key {
                id.push('/');
                id.push_str(&match seg {
                    KeySeg::Id(id) => id.clone(),
                    KeySeg::Named(name) => format!("'{name}'"),
                    KeySeg::ModelRow(row) => format!("[{row}]"),
                    KeySeg::Typed { ty, nth } => format!("{ty}[{nth}]"),
                });
            }
            id.push('.');
            id.push_str(property.rsplit('.').next().unwrap_or(property));
            // A member of a type the runtime only stands in for keeps its
            // objects as the stand-in's own, under `default`.
            let fallback = format!(
                "{}.default",
                id.rsplit_once('.').map_or("", |(owner, _)| owner)
            );
            theirs.keys().flatten().any(|c| {
                [&id, &fallback].iter().any(|id| {
                    c == *id
                        || c.strip_prefix(id.as_str())
                            .is_some_and(|rest| rest.starts_with('['))
                })
            })
        };
        let Ok(ours) = qml_edit::keys_with(&text, &templates) else {
            continue;
        };
        checked += 1;
        for object in ours {
            objects += 1;
            let known = theirs
                .get(&object.component)
                .is_some_and(|keys| keys.contains(&object.key));
            let kept = || {
                let component = object.component.as_deref().unwrap_or("");
                let owner = component.rsplit_once('.').map_or("", |(owner, _)| owner);
                theirs.iter().any(|(c, keys)| {
                    c.as_deref()
                        .and_then(|c| c.strip_prefix(owner))
                        .is_some_and(|rest| rest.starts_with(".default"))
                        && keys.contains(&object.key)
                })
            };
            if !known && object.component.is_some() && kept() {
                stand_in += 1;
            } else if !known {
                wrong.push(format!(
                    "{}: {} {:?}",
                    path.display(),
                    object.path,
                    object.key
                ));
            }
        }
    }
    eprintln!(
        "{checked} files, {objects} objects keyed alike ({stand_in} in members of types the runtime only stands in for, which it keeps as `default`)"
    );
    assert!(checked > 0);
    assert!(
        wrong.is_empty(),
        "{} keys differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}
