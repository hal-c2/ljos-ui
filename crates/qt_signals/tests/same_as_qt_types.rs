//! Every type every module exports has, here, the properties and signals
//! `qt_types` finds on it: by name, with the same arguments, along the same
//! chain of what it inherits.

/// A signal's name, its arguments and their types.
type Signal = (String, Vec<String>, Vec<String>);
/// Property names and signals.
type Members = (Vec<String>, Vec<Signal>);

/// A type's members as a handler sees them: each name once, nearest first.
fn chain(mut properties: Vec<String>, mut signals: Vec<Signal>) -> Members {
    let mut seen = std::collections::HashSet::new();
    properties.retain(|p| seen.insert(p.clone()));
    seen.clear();
    signals.retain(|s| seen.insert(s.0.clone()));
    properties.sort();
    signals.sort();
    (properties, signals)
}

fn theirs(ty: &'static qt_types::Type) -> Members {
    let (mut properties, mut signals) = (Vec::new(), Vec::new());
    let mut stack = vec![ty];
    let mut seen = Vec::new();
    while let Some(t) = stack.pop() {
        if seen.contains(&std::ptr::from_ref(t)) {
            continue;
        }
        seen.push(std::ptr::from_ref(t));
        properties.extend(t.properties.iter().map(|p| p.name.to_owned()));
        signals.extend(t.signals.iter().map(|s| {
            (
                s.name.to_owned(),
                s.parameters.iter().map(|p| (*p).to_owned()).collect(),
                s.parameter_types.iter().map(|p| (*p).to_owned()).collect(),
            )
        }));
        stack.extend(t.prototype());
        stack.extend(t.member_extension());
    }
    chain(properties, signals)
}

fn ours(ty: &'static qt_signals::Type) -> Members {
    let (mut properties, mut signals) = (Vec::new(), Vec::new());
    let mut t = Some(ty);
    while let Some(ty) = t {
        properties.extend(ty.properties.iter().map(|p| p.name.to_owned()));
        signals.extend(ty.signals.iter().map(|s| {
            (
                s.name.to_owned(),
                s.parameters.iter().map(|p| (*p).to_owned()).collect(),
                s.parameter_types.iter().map(|p| (*p).to_owned()).collect(),
            )
        }));
        t = ty.prototype();
    }
    chain(properties, signals)
}

#[test]
fn every_exported_type_has_what_qt_types_finds_on_it() {
    let (mut modules, mut types) = (0, 0);
    let mut wrong = Vec::new();
    assert_eq!(qt_signals::modules(), qt_types::modules());
    for uri in qt_types::modules() {
        let full = qt_types::module(uri).expect("listed");
        let small = qt_signals::module(uri).expect("listed alike");
        modules += 1;
        let mut names = qt_types::native_types(uri).unwrap_or_default();
        names.extend(qt_types::written_types(uri).unwrap_or_default());
        for name in names {
            let (Some(full), Some(small)) = (full.type_named(name), small.type_named(name)) else {
                wrong.push(format!("{uri} {name}: found by one table only"));
                continue;
            };
            types += 1;
            if full.name != small.name || theirs(full) != ours(small) {
                wrong.push(format!("{uri} {name}"));
            }
            for s in &full.signals {
                if small.signal(s.name).is_none() {
                    wrong.push(format!("{uri} {name}: no {}", s.name));
                }
            }
        }
    }
    eprintln!("{modules} modules, {types} exported types alike");
    assert!(types > 0);
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn a_type_is_found_through_what_its_module_imports() {
    // `Timer` is QtQml's; a file that imports QtQuick has it.
    let quick = qt_signals::module("QtQuick").expect("QtQuick is described");
    let timer = quick.type_named("Timer").expect("QtQuick brings QtQml");
    assert!(timer.signal("triggered").is_some());
    assert!(timer.signal("intervalChanged").is_some());
    let item = quick
        .type_named("Rectangle")
        .expect("QtQuick has Rectangle");
    assert!(item.property("width").is_some(), "inherited from Item");
}

#[test]
fn creatable_is_what_qt_types_says_and_exports_are_listed() {
    let mut wrong = Vec::new();
    let (mut creatable, mut not) = (0, 0);
    for uri in qt_types::modules() {
        let full = qt_types::module(uri).expect("listed");
        let small = qt_signals::module(uri).expect("listed alike");
        for name in small.exported() {
            let (Some(full), Some(small)) = (full.type_named(name), small.type_named(name)) else {
                wrong.push(format!("{uri} {name}: found by one table only"));
                continue;
            };
            if full.is_creatable != small.creatable {
                wrong.push(format!("{uri} {name}"));
            }
            if small.creatable {
                creatable += 1;
            } else {
                not += 1;
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    assert!(creatable > 0 && not > 0, "{creatable} creatable, {not} not");
}

#[test]
fn inherited_members_are_listed_once_nearest_first() {
    let quick = qt_signals::module("QtQuick").expect("QtQuick is described");
    let rectangle = quick.type_named("Rectangle").expect("exported");
    assert!(rectangle.creatable);
    let names: Vec<_> = rectangle.all_properties().iter().map(|p| p.name).collect();
    assert!(names.contains(&"width"), "from Item");
    assert!(names.contains(&"radius"), "its own");
    let unique: std::collections::HashSet<_> = names.iter().collect();
    assert_eq!(unique.len(), names.len());
    assert!(
        rectangle
            .all_signals()
            .iter()
            .any(|s| s.name == "widthChanged")
    );
}
