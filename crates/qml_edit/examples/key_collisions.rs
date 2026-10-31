//! How often an object's key depends on the order of its siblings, and
//! what that does to two writers, over every `.qml` file under the given
//! directories:
//!
//! `cargo run -p qml_edit --example key_collisions -- corpus/qtdoc crates`
//!
//! A key step is `Type[n]` for an object with neither an `id` nor an
//! `objectName`. Among siblings of one type that is their order, so a
//! sibling put in or taken out before one shifts its key: a change one
//! writer addressed by key lands on another object. Each such object is
//! taken out in turn and the two texts compared: by key alone the later
//! siblings are misnamed; [`qml_edit::diff`] should follow each to where it
//! is. When the object taken out is written exactly like a sibling the two
//! cannot be told apart, nor need be: either removal gives the same text.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ljos_ir::KeySeg;
use qml_edit::{Edit, NodePath, Step, TreeChange};

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

/// Component, parent key and type: what unnamed siblings share.
type Sibling = (Option<String>, Vec<KeySeg>, String);

#[derive(Default)]
struct Count {
    files: usize,
    objects: usize,
    /// Objects keyed `Type[n]` with an unnamed sibling of the same type.
    ordered: usize,
    /// Groups of two or more such siblings.
    groups: usize,
    /// Objects whose whole key another object of the file also has.
    duplicate: usize,
    /// Removals tried, and the objects key-only matching then misnames.
    removals: usize,
    misnamed_by_key: usize,
    /// What the diff got wrong, and the removals of an object written
    /// exactly like a sibling, where either answer is right.
    misnamed_by_diff: usize,
    interchangeable: usize,
    /// Removals the diff saw differently, written out.
    wrong: Vec<String>,
}

fn main() {
    let dirs: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let mut files = Vec::new();
    for dir in &dirs {
        qml_under(dir, &mut files);
    }
    let mut count = Count::default();
    for path in &files {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let Ok(objects) = qml_edit::keys(&text) else {
            continue;
        };
        count.files += 1;
        count.objects += objects.len();
        let mut seen: HashMap<(&Option<String>, &[KeySeg]), usize> = HashMap::new();
        for o in &objects {
            *seen.entry((&o.component, &o.key[..])).or_default() += 1;
        }
        count.duplicate += seen.values().filter(|&&n| n > 1).sum::<usize>();
        // Unnamed siblings by parent and type, in order.
        let mut groups: HashMap<Sibling, Vec<&qml_edit::Object>> = HashMap::new();
        for o in &objects {
            if let Some(KeySeg::Typed { ty, .. }) = o.key.last() {
                let parent = o.key[..o.key.len() - 1].to_vec();
                groups
                    .entry((o.component.clone(), parent, ty.clone()))
                    .or_default()
                    .push(o);
            }
        }
        let mut groups: Vec<_> = groups.into_values().filter(|g| g.len() > 1).collect();
        groups.sort_by_key(|g| g[0].path.to_string());
        for group in &groups {
            count.groups += 1;
            count.ordered += group.len();
            for (i, gone) in group.iter().enumerate() {
                if !matches!(gone.path.0.last(), Some(Step::Child(_))) {
                    continue;
                }
                let Ok(applied) = qml_edit::apply(
                    &text,
                    &Edit::RemoveObject {
                        object: gone.path.clone(),
                    },
                ) else {
                    continue;
                };
                count.removals += 1;
                count.misnamed_by_key += group.len() - 1 - i;
                let changes = qml_edit::diff(&text, &applied.text).expect("both texts parse");
                match judge(&text, &applied.text, group, i, &changes) {
                    Verdict::Right => {}
                    Verdict::Interchangeable => count.interchangeable += 1,
                    Verdict::Wrong(n) => {
                        count.misnamed_by_diff += n;
                        count
                            .wrong
                            .push(format!("{}: without {}", path.display(), gone.path));
                    }
                }
            }
        }
    }
    println!("files                      {}", count.files);
    println!("objects                    {}", count.objects);
    println!("keys shared by two objects {}", count.duplicate);
    println!(
        "order-keyed objects        {} in {} groups of unnamed siblings of one type",
        count.ordered, count.groups
    );
    println!("removals tried             {}", count.removals);
    println!("  misnamed by key alone    {}", count.misnamed_by_key);
    println!("  misnamed by the diff     {}", count.misnamed_by_diff);
    println!(
        "  interchangeable          {} (written exactly like a sibling)",
        count.interchangeable
    );
    for w in &count.wrong {
        println!("    {w}");
    }
}

enum Verdict {
    Right,
    Interchangeable,
    /// How many siblings it misnamed.
    Wrong(usize),
}

/// Whether `changes` says what taking `group[gone]` out did: that one
/// removed, the later siblings one place earlier, nothing else.
fn judge(
    old: &str,
    new: &str,
    group: &[&qml_edit::Object],
    gone: usize,
    changes: &[TreeChange],
) -> Verdict {
    let removed: Vec<&NodePath> = changes
        .iter()
        .filter_map(|c| match c {
            TreeChange::Removed(o) => Some(&o.path),
            _ => None,
        })
        .collect();
    let others = changes
        .iter()
        .filter(|c| !matches!(c, TreeChange::Removed(_) | TreeChange::Rekeyed { .. }))
        .count();
    let rekeyed: Vec<(&[KeySeg], &[KeySeg])> = changes
        .iter()
        .filter_map(|c| match c {
            TreeChange::Rekeyed { was, now } => Some((&was.key[..], &now.key[..])),
            _ => None,
        })
        .collect();
    let expected: Vec<(&[KeySeg], &[KeySeg])> = group[gone + 1..]
        .iter()
        .zip(&group[gone..])
        .map(|(was, now)| (&was.key[..], &now.key[..]))
        .collect();
    if removed == [&group[gone].path] && others == 0 && rekeyed == expected {
        return Verdict::Right;
    }
    // Another object was taken for it: right if taking that one out gives
    // the same objects, spacing aside.
    if let [other] = removed[..]
        && others == 0
        && qml_edit::apply(
            old,
            &Edit::RemoveObject {
                object: other.clone(),
            },
        )
        .is_ok_and(|a| qml_edit::diff(&a.text, new).is_ok_and(|d| d.is_empty()))
    {
        return Verdict::Interchangeable;
    }
    let misnamed = expected.iter().filter(|e| !rekeyed.contains(e)).count()
        + rekeyed.iter().filter(|r| !expected.contains(r)).count();
    Verdict::Wrong(misnamed.max(1))
}
