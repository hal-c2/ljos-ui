//! The table as text: a line a fact, in an order that does not depend on
//! the machine, so that a newer Qt shows up as a diff of what changed.
//!
//! ```text
//! module QtQuick
//!     import QtQml
//!     Item QQuickItem
//!     type QQuickItem Item
//!         prototype QObject
//!         default data
//!         enum TransformOrigin TopLeft Top ...
//!         prop width double
//!         prop anchors QQuickAnchors readonly pointer   also `list`, `required`, `reset`
//!         signal childrenRectChanged _
//!         method forceActiveFocus
//! ```
//!
//! `qt_types` documents what the lines mean.

use std::{collections::BTreeMap, fmt::Write};

use crate::model::{Class, ImportKind, Modules, Semantics};

pub fn write(modules: &Modules, (major, minor): (u32, u32)) -> String {
    let mut out = format!(
        "# The QML types of Qt {major}.{minor}, as its qmldir and qmltypes files describe them.\n\
         # Written by `mise run types` (crates/qmltypes), read by qt_types. Not edited by hand.\n"
    );
    // The QML name a class is known by: the one its own module gives it,
    // else the first another does.
    let mut names: BTreeMap<&str, &str> = BTreeMap::new();
    let own = modules.values().flat_map(|module| {
        module
            .exports
            .iter()
            .filter(|(_, class)| module.classes.contains_key(*class))
    });
    for (name, class) in own.chain(modules.values().flat_map(|module| &module.exports)) {
        names.entry(class).or_insert(name);
    }

    for (uri, module) in modules {
        line(&mut out, 0, &["module", uri]);
        for import in &module.imports {
            let kind = match import.kind {
                ImportKind::Always => "",
                ImportKind::Default => "default",
                ImportKind::Optional => "optional",
            };
            line(&mut out, 1, &["import", &import.module, kind]);
        }
        for (name, class) in &module.exports {
            line(&mut out, 1, &[name, class]);
        }
        for class in module.classes.values() {
            self::class(
                &mut out,
                class,
                names.get(class.name.as_str()).copied().unwrap_or(""),
            );
        }
    }
    out
}

fn class(out: &mut String, class: &Class, name: &str) {
    line(out, 1, &["type", &class.name, name]);
    for (word, link) in [
        ("prototype", &class.prototype),
        ("attached", &class.attached),
    ] {
        if let Some(link) = link {
            line(out, 2, &[word, link]);
        }
    }
    if let Some(extension) = &class.extension {
        let namespace = if class.extension_is_namespace {
            "namespace"
        } else {
            ""
        };
        line(out, 2, &["extension", extension, namespace]);
    }
    if let Some(property) = &class.default_property {
        line(out, 2, &["default", property]);
    }
    let flags = [
        (class.is_singleton, "singleton"),
        (class.is_uncreatable, "uncreatable"),
        (class.semantics == Semantics::Value, "value"),
        (class.semantics == Semantics::None, "namespace"),
        (class.has_custom_parser, "parser"),
    ];
    if flags.iter().any(|(set, _)| *set) {
        let mut words = vec!["is"];
        words.extend(flags.iter().filter(|(set, _)| *set).map(|(_, word)| *word));
        line(out, 2, &words);
    }

    for enumeration in &class.enums {
        let mut name = enumeration.name.clone();
        if let Some(alias) = &enumeration.alias {
            write!(name, "={alias}").unwrap();
        }
        let keys: Vec<String> = enumeration
            .keys
            .iter()
            .map(|(key, number)| match number {
                Some(number) => format!("{key}={number}"),
                None => key.clone(),
            })
            .collect();
        let mut words = vec![if enumeration.is_flag { "flags" } else { "enum" }, &name];
        words.extend(keys.iter().map(String::as_str));
        line(out, 2, &words);
    }
    for property in &class.properties {
        let flag = |set: bool, word: &'static str| if set { word } else { "" };
        line(
            out,
            2,
            &[
                "prop",
                &property.name,
                &property.type_name,
                flag(property.is_list, "list"),
                flag(property.is_readonly, "readonly"),
                flag(property.is_pointer, "pointer"),
                flag(property.is_required, "required"),
                flag(property.has_reset, "reset"),
            ],
        );
    }
    for signal in &class.signals {
        let mut words = vec!["signal", signal.name.as_str()];
        // A name Qt's header does not give is still an argument.
        words.extend(
            signal
                .parameters
                .iter()
                .map(|name| if name.is_empty() { "_" } else { name }),
        );
        line(out, 2, &words);
    }
    for method in &class.methods {
        line(out, 2, &["method", method]);
    }
}

/// The words that are not empty, on a line of their own.
fn line(out: &mut String, depth: usize, words: &[&str]) {
    out.extend(std::iter::repeat_n('\t', depth));
    let mut words = words.iter().filter(|word| !word.is_empty());
    out.push_str(words.next().expect("a line says something"));
    for word in words {
        assert!(
            !word.contains(char::is_whitespace),
            "{word:?} would read as two words"
        );
        out.push(' ');
        out.push_str(word);
    }
    out.push('\n');
}
