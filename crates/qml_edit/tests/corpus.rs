//! Every edit, on every object of every QML file in the repository, undone
//! again: by its inverse changes the file must come back byte for byte, and
//! by the opposite edit too, except that a move may leave an empty line
//! elsewhere (see `qml_edit`'s `Snippet::gaps`).
//!
//! The files are the ones Ljós ships and tests with (the Controls styles,
//! the design module, the QML test suites, the examples and benchmarks), so
//! they carry the comments, blank lines, one-line objects and nesting real
//! authors write.

use std::path::{Path, PathBuf};

use qml_edit::{Applied, Edit, Error, NodePath, Step, apply, objects, splice};
use qml_syntax::Allocator;
use qml_syntax::ast::{QmlBindingValue, QmlMember, QmlObject};

const DIRECTORIES: &[&str] = &[
    "crates/ljos_controls/qml",
    "crates/ljos_design_qml/qml",
    "crates/ljos_design/fixtures",
    "crates/ljos_test/tests/qml",
    "crates/ljos_mcp_author/tests/fixtures",
    "examples",
    "bench",
];

fn files() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut found = Vec::new();
    for directory in DIRECTORIES {
        collect(&root.join(directory), &mut found);
    }
    found.sort();
    found
}

fn collect(directory: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, found);
        } else if path.extension().is_some_and(|e| e == "qml") {
            found.push(path);
        }
    }
}

#[derive(Default, Debug)]
struct Tally {
    files: usize,
    objects: usize,
    round_trips: usize,
    /// Opposite moves that gave the same document but not the same bytes.
    respaced: Vec<String>,
    /// Objects whose members shared lines that an insertion opened up.
    opened: Vec<String>,
    /// Opposite moves that put a child elsewhere among its parent's
    /// bindings.
    reordered: Vec<String>,
    unsupported: Vec<String>,
}

/// Applies `edit`, checks that the untouched text is the input's, and
/// returns the new text. `Unsupported` is counted, not failed.
fn step(tally: &mut Tally, where_: &str, source: &str, edit: &Edit) -> Option<String> {
    match apply(source, edit) {
        Ok(Applied { text, changes }) => {
            let mut at = 0;
            let mut out = 0;
            for change in &changes {
                let kept = &source[at..change.range.start];
                assert_eq!(
                    &text[out..out + kept.len()],
                    kept,
                    "{where_}: {edit:?} changed text it did not report"
                );
                out += kept.len() + change.replacement.len();
                at = change.range.end;
            }
            assert_eq!(
                &text[out..],
                &source[at..],
                "{where_}: {edit:?} changed text it did not report"
            );
            let applied = Applied { text, changes };
            assert!(
                splice(&applied.text, &applied.inverse(source)) == source,
                "{where_}: undoing {edit:?} did not give the file back"
            );
            Some(applied.text)
        }
        Err(Error::Unsupported(why)) => {
            tally.unsupported.push(format!("{where_}: {why}"));
            None
        }
        Err(error) => panic!("{where_}: {edit:?}: {error}"),
    }
}

fn round_trip(
    tally: &mut Tally,
    where_: &str,
    source: &str,
    there: &Edit,
    back: impl FnOnce(&str) -> Edit,
) {
    let Some(changed) = step(tally, where_, source, there) else {
        return;
    };
    let back = back(&changed);
    let Some(restored) = step(tally, where_, &changed, &back) else {
        return;
    };
    if restored != source && matches!(there, Edit::MoveObject { .. }) {
        // Child indices do not say where among the bindings a child stood,
        // so moving it back may also put it elsewhere among them, even past
        // an object a binding holds (`environment: …`). Each object must
        // still hold what it held: its own lines, its children in order and
        // the objects its bindings hold by name, so a value that moved from
        // one object to another fails even when every line is still there.
        assert!(
            tree(source) == tree(&restored),
            "{where_}: {there:?} then {back:?} changed the document\n{restored}"
        );
        if qml_fmt::same_tree(source, &restored).is_ok() {
            tally.respaced.push(where_.to_owned());
        } else {
            tally.reordered.push(where_.to_owned());
        }
        return;
    }
    if restored != source && matches!(there, Edit::InsertObject { .. }) {
        // A one-line object was opened up to take several lines.
        if let Err(difference) = qml_fmt::same_tree(source, &restored) {
            panic!(
                "{where_}: {there:?} then {back:?} changed the document: {difference}\n{restored}"
            );
        }
        tally.opened.push(where_.to_owned());
        return;
    }
    assert!(
        restored == source,
        "{where_}: {there:?} then {back:?} did not give the file back\n--- after the first edit\n{changed}\n--- after the second\n{restored}"
    );
    tally.round_trips += 1;
}

/// An object as what it holds, wherever among its members each thing is
/// written: its own lines without the objects in it (blanks and spacing
/// aside, sorted), its children in order, and its groups and the objects
/// its bindings hold, by name.
#[derive(Debug, PartialEq, Eq)]
struct Tree {
    type_name: String,
    lines: Vec<String>,
    children: Vec<Tree>,
    held: Vec<(String, Tree)>,
}

fn tree(source: &str) -> Tree {
    let allocator = Allocator::default();
    let parsed = qml_syntax::parse(&allocator, source);
    assert!(parsed.is_ok(), "the file no longer parses\n{source}");
    shape(source, &parsed.document.root)
}

fn shape(source: &str, object: &QmlObject<'_>) -> Tree {
    let mut nested = Vec::new();
    let mut children = Vec::new();
    let mut held = Vec::new();
    let mut hold = |name: String, inner: &QmlObject<'_>, nested: &mut Vec<_>| {
        nested.push(inner.span);
        held.push((name, shape(source, inner)));
    };
    for member in &object.members {
        match member {
            QmlMember::Object(inner)
                if inner.type_name.parts[0].starts_with(char::is_lowercase) =>
            {
                hold(format!("{} {{", inner.type_name), inner, &mut nested);
            }
            QmlMember::Object(inner) => {
                nested.push(inner.span);
                children.push(shape(source, inner));
            }
            QmlMember::Binding(binding) => match &binding.value {
                QmlBindingValue::Object(inner) => {
                    hold(binding.name.to_string(), inner, &mut nested)
                }
                QmlBindingValue::Objects(list) => {
                    for inner in list {
                        hold(binding.name.to_string(), inner, &mut nested);
                    }
                }
                _ => {}
            },
            QmlMember::Property(declared) => match &declared.value {
                Some(QmlBindingValue::Object(inner)) => {
                    hold(declared.name.name.to_string(), inner, &mut nested);
                }
                Some(QmlBindingValue::Objects(list)) => {
                    for inner in list {
                        hold(declared.name.name.to_string(), inner, &mut nested);
                    }
                }
                _ => {}
            },
            QmlMember::InlineComponent(component) => {
                hold(
                    component.name.name.to_string(),
                    &component.object,
                    &mut nested,
                );
            }
            _ => {}
        }
    }
    // A list keeps its order; only where each name stands may change.
    held.sort_by(|a, b| a.0.cmp(&b.0));
    let mut own = String::new();
    let mut at = object.span.start as usize;
    for span in &nested {
        own.push_str(&source[at..span.start as usize]);
        at = span.end as usize;
    }
    own.push_str(&source[at..object.span.end as usize]);
    let mut lines: Vec<String> = own
        .lines()
        .map(|line| line.split_whitespace().collect::<String>())
        .filter(|line| !line.is_empty())
        .collect();
    lines.sort();
    Tree {
        type_name: object.type_name.to_string(),
        lines,
        children,
        held,
    }
}

fn children(source: &str, parent: &NodePath) -> usize {
    objects(source)
        .unwrap()
        .iter()
        .filter(|(path, _)| {
            path.0.len() == parent.0.len() + 1
                && path.0.starts_with(&parent.0)
                && matches!(path.0.last(), Some(Step::Child(_)))
        })
        .count()
}

#[test]
fn every_edit_on_the_repository_files_can_be_undone_byte_for_byte() {
    let mut tally = Tally::default();
    for file in files() {
        let source = std::fs::read_to_string(&file).unwrap();
        let Ok(all) = objects(&source) else {
            // Files that do not parse on purpose (error fixtures).
            continue;
        };
        tally.files += 1;
        let name = file.display().to_string();
        for (path, _) in &all {
            tally.objects += 1;
            let where_ = format!("{name} at {path}");

            // A plain name, and a dotted one that goes dotted or into a
            // group by the file's style and is found again either way.
            for name in ["ljosEditProbe", "ljosEditProbe.value"] {
                round_trip(
                    &mut tally,
                    &where_,
                    &source,
                    &Edit::SetBinding {
                        object: path.clone(),
                        name: name.into(),
                        value: "1 + 2".into(),
                    },
                    |_| Edit::RemoveBinding {
                        object: path.clone(),
                        name: name.into(),
                    },
                );
            }

            let count = children(&source, path);
            for index in [0, count] {
                round_trip(
                    &mut tally,
                    &where_,
                    &source,
                    &Edit::InsertObject {
                        parent: path.clone(),
                        index,
                        text: "Item {\n    // probe\n    width: 10\n}".into(),
                    },
                    |_| Edit::RemoveObject {
                        object: path.child(index),
                    },
                );
            }

            // A child moved to the end of the root and back to its place.
            if let Some((parent, n)) = split_child(path)
                && !parent.0.iter().any(|s| !matches!(s, Step::Child(_)))
            {
                let root = NodePath::root();
                round_trip(
                    &mut tally,
                    &where_,
                    &source,
                    &Edit::MoveObject {
                        object: path.clone(),
                        parent: root.clone(),
                        index: children(&source, &root) - usize::from(parent == root),
                    },
                    |changed| Edit::MoveObject {
                        object: root.child(children(changed, &root) - 1),
                        parent: parent.clone(),
                        index: n,
                    },
                );
            }
        }
    }
    eprintln!(
        "{} files, {} objects, {} byte-identical round trips, {} respaced moves, {} reordered moves, {} opened one-line objects, {} unsupported",
        tally.files,
        tally.objects,
        tally.round_trips,
        tally.respaced.len(),
        tally.reordered.len(),
        tally.opened.len(),
        tally.unsupported.len()
    );
    for line in &tally.respaced {
        eprintln!("respaced: {line}");
    }
    for line in &tally.reordered {
        eprintln!("reordered: {line}");
    }
    for line in &tally.unsupported {
        eprintln!("unsupported: {line}");
    }
    assert!(tally.files > 150, "the corpus went missing: {tally:?}");
    // Every layout the files have is handled: objects whose members share
    // lines with a brace are opened up rather than refused.
    assert!(
        tally.unsupported.is_empty(),
        "edits refused: {:#?}",
        tally.unsupported
    );
}

fn split_child(path: &NodePath) -> Option<(NodePath, usize)> {
    match path.0.last() {
        Some(Step::Child(n)) => Some((NodePath(path.0[..path.0.len() - 1].to_vec()), *n)),
        _ => None,
    }
}
