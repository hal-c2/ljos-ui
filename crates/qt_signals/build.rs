//! Writes what `qt_types` says an object type can signal, and only that,
//! as the small text `src/lib.rs` reads.
//!
//! The full table is linked here, on the machine that builds, so the
//! choice it makes between Qt's description and Ljós's own (and
//! `LJOS_QT_TYPES`) is the one this table has: a build for the web carries
//! the answer, not the description and the code that reads it.
//!
//! The text is lines of words:
//!
//! ```text
//! m QtQuick                     a module
//! i QtQml                       a module importing it brings along
//! e Item 3                      a type it exports, by index
//! t Item 1                      type 3: its name and its prototype's index, or `-`
//! p x y width+                  its properties; `+`: `widthChanged` is a signal without arguments
//! s clicked mouse:QQuickMouseEvent    another signal, its arguments with their types where known
//! ```

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::{env, fs};

use qt_types::{ImportKind, Type};

fn main() {
    println!("cargo::rerun-if-env-changed=LJOS_QT_TYPES");
    let mut out = Writer::default();
    for uri in qt_types::modules() {
        let Some(module) = qt_types::module(uri) else {
            continue;
        };
        let _ = writeln!(out.modules, "m {uri}");
        // A `default import` is followed when the module has a choice to
        // make, as `qt_types` follows it.
        let chooses = module
            .imports
            .iter()
            .any(|import| import.kind == ImportKind::Optional);
        for import in &module.imports {
            let followed = match import.kind {
                ImportKind::Always => true,
                ImportKind::Default => chooses,
                ImportKind::Optional => false,
            };
            if followed {
                let _ = writeln!(out.modules, "i {}", import.module);
            }
        }
        let mut names = qt_types::native_types(uri).unwrap_or_default();
        names.extend(qt_types::written_types(uri).unwrap_or_default());
        names.sort_unstable();
        for name in names {
            if let Some(ty) = module.type_named(name) {
                let index = out.index(ty);
                let _ = writeln!(out.modules, "e {name} {index}");
            }
        }
    }
    out.write_types();
    let path = PathBuf::from(env::var_os("OUT_DIR").expect("set by cargo")).join("signals.txt");
    fs::write(path, out.modules + &out.types).expect("OUT_DIR is writable");
}

#[derive(Default)]
struct Writer {
    modules: String,
    types: String,
    order: Vec<&'static Type>,
    indexes: HashMap<*const Type, usize>,
}

impl Writer {
    /// The type's index, and its prototype's, written in order.
    fn index(&mut self, ty: &'static Type) -> usize {
        if let Some(&index) = self.indexes.get(&std::ptr::from_ref(ty)) {
            return index;
        }
        let index = self.order.len();
        self.indexes.insert(std::ptr::from_ref(ty), index);
        self.order.push(ty);
        if let Some(prototype) = ty.prototype() {
            self.index(prototype);
        }
        index
    }

    fn write_types(&mut self) {
        for ty in self.order.clone() {
            let prototype = ty
                .prototype()
                .map_or("-".to_owned(), |p| self.index(p).to_string());
            let _ = writeln!(self.types, "t {} {prototype}", ty.name);
            let (properties, signals) = members(ty);
            let names: HashSet<&str> = properties.iter().copied().collect();
            let implied = |s: &qt_types::Signal| {
                s.parameters.is_empty()
                    && s.name
                        .strip_suffix("Changed")
                        .is_some_and(|p| names.contains(p))
            };
            let changing: HashSet<&str> = signals
                .iter()
                .filter(|s| implied(s))
                .filter_map(|s| s.name.strip_suffix("Changed"))
                .collect();
            if !properties.is_empty() {
                self.types.push('p');
                for p in &properties {
                    let mark = if changing.contains(p) { "+" } else { "" };
                    let _ = write!(self.types, " {p}{mark}");
                }
                self.types.push('\n');
            }
            for s in signals.iter().filter(|s| !implied(s)) {
                self.types.push_str("s ");
                self.types.push_str(s.name);
                for (i, parameter) in s.parameters.iter().enumerate() {
                    let _ = write!(self.types, " {parameter}");
                    if let Some(ty) = s.parameter_types.get(i) {
                        let _ = write!(self.types, ":{ty}");
                    }
                }
                self.types.push('\n');
            }
        }
    }
}

/// A type's own properties and signals with those of the class that
/// extends it, as `qt_types` finds them: its own first, then the
/// extension's and what that inherits, a name once.
fn members(ty: &'static Type) -> (Vec<&'static str>, Vec<&'static qt_types::Signal>) {
    let (mut properties, mut signals) = (Vec::new(), Vec::new());
    let (mut seen_p, mut seen_s) = (HashSet::new(), HashSet::new());
    let mut add = |t: &'static Type| {
        for p in &t.properties {
            if seen_p.insert(p.name) {
                properties.push(p.name);
            }
        }
        for s in &t.signals {
            if seen_s.insert(s.name) {
                signals.push(s);
            }
        }
    };
    add(ty);
    let mut stack: Vec<&'static Type> = ty.member_extension().into_iter().collect();
    let mut visited = HashSet::new();
    while let Some(t) = stack.pop() {
        if !visited.insert(std::ptr::from_ref(t)) {
            continue;
        }
        add(t);
        // The extension's own extension before what it inherits.
        stack.extend(t.prototype());
        stack.extend(t.member_extension());
    }
    (properties, signals)
}
