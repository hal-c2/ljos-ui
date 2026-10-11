//! The compiler's table of the modules that are Ljós's own (`Ljos`,
//! `QtTest`) is what their native types declare.
//!
//! The front end reads QML against one table of types (`qt_types`). Qt's
//! part of it comes from a Qt installation; the part for Ljós's own modules
//! is `crates/qt_types/src/ljos.txt`, which is committed. The crates that
//! read QML must not link the runtime that has the native types, so they
//! cannot ask the types: the description reaches them as that text, and
//! this is what writes it and holds it to the types.
//!
//! ```text
//! LJOS_OWN_TYPES=write cargo test -p ljos_cli --test own_types
//! ```
//!
//! writes the file from the types as they are now. Without that, the test
//! fails when the file is not what would be written: a property added to
//! `ErrorBoundary` in Rust cannot be left out of what the compiler knows.

use std::{
    collections::{BTreeSet, VecDeque},
    fmt::Write as _,
    fs,
    path::PathBuf,
};

use ljos_engine::{Engine, MetaObject, MetaRef, PropertyAccess, PropertyFlags, TypeTag};
use qml_front::{QtTypeOracle, TypeOracle};

/// The modules of native types that Qt's table does not describe.
const OWN: [&str; 2] = ["Ljos", "QtTest"];

/// Properties that hold a `Component`: what is written in one is a template
/// made later, not an object. A native type cannot say so yet (a field that
/// refers to an object declares no type for it), so it is said here.
/// `the_components_are_still_undeclared` fails once a type says it itself.
const COMPONENTS: [(&str, &str, &str); 2] = [
    ("Ljos", "ErrorBoundary", "content"),
    ("Ljos", "ErrorBoundary", "fallback"),
];

/// Set to `write` to write the table instead of comparing it.
const WRITE: &str = "LJOS_OWN_TYPES";

const HEADER: &str = "\
# The modules that are Ljós's own (`import Ljos`, `import QtTest`), as their
# native types declare them, in the format of `qt_types`. Do not edit:
# `crates/ljos_cli/tests/own_types.rs` writes this file, and fails when it
# is not what the types declare now.
#
#     LJOS_OWN_TYPES=write cargo test -p ljos_cli --test own_types
";

fn committed() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../qt_types/src/ljos.txt")
}

/// An engine with every native module the `ljos` program can run.
fn engine() -> Engine {
    let engine = Engine::new();
    ljos_quick::register(&engine);
    ljos_controls::register(&engine);
    ljos_test::qttest::register(&engine);
    engine
}

/// The types the module `uri` exports, by name.
fn exported(engine: &Engine, uri: &str) -> Vec<MetaRef> {
    let dump = engine.type_dump();
    let module = dump
        .modules
        .iter()
        .find(|m| m.uri == uri)
        .unwrap_or_else(|| panic!("no module `{uri}` is registered"));
    let mut names: Vec<&str> = module.types.iter().map(|t| t.name.as_str()).collect();
    names.sort_unstable();
    names
        .into_iter()
        .map(|name| engine.lookup_type(uri, name).expect("a registered type"))
        .collect()
}

/// The table of the own modules, from the native types.
fn table(engine: &Engine) -> String {
    let mut out = String::from(HEADER);
    for uri in OWN {
        out.push('\n');
        out.push_str(&Module::new(uri).written(&exported(engine, uri)));
    }
    out
}

/// One module being written.
struct Module<'a> {
    uri: &'a str,
    /// The modules whose names the descriptions are written with.
    uses: Vec<String>,
    /// Types the exported ones lead to that no module exports: a group
    /// (`fault` of `fault.kind`), what a type attaches, a base that has no
    /// name in QML.
    unnamed: VecDeque<MetaRef>,
    described: BTreeSet<String>,
    types: String,
}

impl<'a> Module<'a> {
    fn new(uri: &'a str) -> Self {
        Module {
            uri,
            uses: Vec::new(),
            unnamed: VecDeque::new(),
            described: BTreeSet::new(),
            types: String::new(),
        }
    }

    fn written(mut self, exported: &[MetaRef]) -> String {
        for meta in exported {
            self.described.insert(meta.name.to_string());
        }
        for meta in exported {
            self.describe(meta, true);
        }
        while let Some(meta) = self.unnamed.pop_front() {
            if self.described.insert(meta.name.to_string()) {
                self.describe(&meta, false);
            }
        }
        // The value types (`color`, `font`) are named as QtQuick names them.
        self.used("QtQuick");
        let mut out = format!("module {}\n", self.uri);
        for used in &self.uses {
            let _ = writeln!(out, "\tuses {used}");
        }
        for meta in exported {
            let _ = writeln!(out, "\t{} {}", meta.name, self.class(&meta.name));
        }
        out + &self.types
    }

    /// The class a type of this module is in the table. No class of Qt's
    /// has a dot in its name.
    fn class(&self, name: &str) -> String {
        format!("{}.{name}", self.uri)
    }

    fn used(&mut self, module: &str) {
        if module != self.uri && !self.uses.iter().any(|used| used == module) {
            self.uses.push(module.to_owned());
        }
    }

    /// A type that is written by its class: one of this module that no
    /// module exports.
    fn unnamed(&mut self, meta: MetaRef) -> String {
        let class = self.class(&meta.name);
        self.unnamed.push_back(meta);
        class
    }

    fn describe(&mut self, meta: &MetaObject, exported: bool) {
        let mut out = format!("\ttype {} {}\n", self.class(&meta.name), meta.name);
        if let Some(base) = &meta.base {
            let link = match &base.module {
                Some(module) => {
                    self.used(module);
                    format!("{} named", base.name)
                }
                None => self.unnamed(base.clone()),
            };
            let _ = writeln!(out, "\t\tprototype {link}");
        }
        if let Some(attached) = meta.attached {
            let class = self.unnamed(attached());
            let _ = writeln!(out, "\t\tattached {class}");
        }
        if let Some(name) = &meta.default_property {
            let _ = writeln!(out, "\t\tdefault {name}");
        }
        if !exported || meta.create.is_none() {
            let _ = writeln!(out, "\t\tis uncreatable");
        }
        let inherited: BTreeSet<&str> = meta
            .base
            .iter()
            .flat_map(|base| base.enums.iter().map(|e| e.name.as_str()))
            .collect();
        for enumeration in &meta.enums {
            if inherited.contains(enumeration.name.as_str()) {
                continue;
            }
            let is_flags = meta
                .own_properties()
                .iter()
                .any(|p| matches!(&p.ty, TypeTag::Flags(name) if *name == enumeration.name));
            let word = if is_flags { "flags" } else { "enum" };
            let _ = write!(out, "\t\t{word} {}", enumeration.name);
            for (key, number) in &enumeration.keys {
                let _ = write!(out, " {key}={number}");
            }
            out.push('\n');
        }
        for property in meta.own_properties() {
            // `fault.kind` is a property of what `fault` is.
            if let PropertyAccess::Group { meta: group, .. } = &property.access {
                self.unnamed(MetaRef::from(*group));
                continue;
            }
            let is_component = COMPONENTS.contains(&(self.uri, &meta.name, &property.name));
            let value = if is_component {
                "Component named pointer".to_owned()
            } else {
                self.value(&property.ty)
            };
            let _ = write!(out, "\t\tprop {} {value}", property.name);
            for (flag, word) in [
                (PropertyFlags::READONLY, " readonly"),
                (PropertyFlags::REQUIRED, " required"),
            ] {
                if property.flags.contains(flag) {
                    out.push_str(word);
                }
            }
            if property.reset.is_some() {
                out.push_str(" reset");
            }
            out.push('\n');
        }
        for signal in meta.own_signals() {
            let _ = write!(out, "\t\tsignal {}", signal.name);
            for (name, _) in &signal.params {
                let _ = write!(out, " {}", if name.is_empty() { "_" } else { name });
            }
            out.push('\n');
        }
        let mut methods = BTreeSet::new();
        for method in meta.own_methods() {
            if methods.insert(method.name.as_str()) {
                let _ = writeln!(out, "\t\tmethod {}", method.name);
            }
        }
        self.types.push_str(&out);
    }

    /// What a property holds, in the table's words: the type by its QML
    /// name (`named`), and what kind of value it is.
    fn value(&self, ty: &TypeTag) -> String {
        match ty {
            TypeTag::Object(_) => format!("{} named pointer", ty.name()),
            TypeTag::List(None) => "var named list".to_owned(),
            TypeTag::List(Some(of)) => {
                let object = of.starts_with(|c: char| c.is_ascii_uppercase());
                format!("{of} named list{}", if object { " pointer" } else { "" })
            }
            // An enum is named as the table names one: by its own name in
            // the type that has it, `Qt::Alignment` in another.
            TypeTag::Enum(name) | TypeTag::Flags(name) => name.replace('.', "::"),
            TypeTag::Group(name) => format!("{} pointer", self.class(name)),
            TypeTag::Void => "var named".to_owned(),
            _ => format!("{} named", ty.name()),
        }
    }
}

/// Where two texts first differ, for a failure a reader can act on.
fn first_difference(committed: &str, now: &str) -> String {
    let (mut old, mut new) = (committed.lines(), now.lines());
    let mut line = 1;
    loop {
        match (old.next(), new.next()) {
            (Some(a), Some(b)) if a == b => line += 1,
            (None, None) => return "the line ends differ".to_owned(),
            (a, b) => {
                return format!(
                    "line {line}\n  committed: {}\n  declared:  {}",
                    a.unwrap_or("(the file ends)"),
                    b.unwrap_or("(nothing more)")
                );
            }
        }
    }
}

#[test]
fn the_table_of_own_types_is_what_the_native_types_declare() {
    let now = table(&engine());
    let path = committed();
    if std::env::var_os(WRITE).is_some_and(|value| value == "write") {
        fs::write(&path, now).expect("the table can be written");
        return;
    }
    let committed = fs::read_to_string(&path).expect("the table is committed");
    assert!(
        committed == now,
        "{} is not what the native types of {OWN:?} declare; write it again with\n\n    \
         {WRITE}=write cargo test -p ljos_cli --test own_types\n\nfirst difference: {}",
        path.display(),
        first_difference(&committed, &now)
    );
}

#[test]
fn the_components_are_still_undeclared() {
    let engine = engine();
    for (uri, ty, property) in COMPONENTS {
        let meta = engine
            .lookup_type(uri, ty)
            .unwrap_or_else(|| panic!("`{uri}` has no `{ty}`: take it out of COMPONENTS"));
        let declared = meta
            .own_properties()
            .iter()
            .find(|p| p.name == property)
            .unwrap_or_else(|| panic!("`{ty}` has no `{property}`: take it out of COMPONENTS"));
        assert_eq!(
            declared.ty,
            TypeTag::Object(None),
            "`{ty}.{property}` declares its type now: take it out of COMPONENTS"
        );
    }
}

/// The compiler's oracle, when there is a table of Qt's types to put the
/// own ones beside. Without one nothing is known at all, the own types
/// included: what they inherit is Qt's to describe.
fn oracle() -> Option<QtTypeOracle> {
    let oracle = QtTypeOracle;
    if oracle.has_module("QtQuick") {
        Some(oracle)
    } else {
        eprintln!("skipped: there is no table of Qt's types (`mise run types`)");
        None
    }
}

#[test]
fn the_compiler_knows_every_module_that_is_registered() {
    let Some(oracle) = oracle() else { return };
    for module in engine().type_dump().modules {
        assert!(
            oracle.has_module(&module.uri),
            "`{}` has native types and the compiler does not know it: if it is not one of Qt's, \
             add it to OWN",
            module.uri
        );
    }
}

#[test]
fn the_compiler_sees_every_member_the_native_types_declare() {
    let Some(oracle) = oracle() else { return };
    let engine = engine();
    for uri in OWN {
        for meta in exported(&engine, uri) {
            let name = meta.name.as_str();
            let ty = oracle
                .module_type(uri, name)
                .unwrap_or_else(|| panic!("`{uri}` has no `{name}`"));
            assert_eq!(oracle.name(ty), name);
            assert_eq!(oracle.module(ty).as_deref(), Some(uri));
            assert!(oracle.members_known(ty), "{name}");
            assert!(!oracle.is_singleton(ty), "{name}");
            assert_eq!(
                oracle.default_property(ty),
                meta.default_property().map(|name| name.as_str()),
                "the default property of {name}"
            );
            for declared in meta.own_properties() {
                let what = format!("{name}.{}", declared.name);
                // `fault.kind`: a property of the value of `fault`.
                let mut path = declared.name.split('.');
                let first = path.next().expect("a name");
                let mut known = oracle.property(ty, first).expect(&what);
                for part in path {
                    let value = known.value.expect(&what);
                    known = oracle.property(value, part).expect(&what);
                }
                assert_eq!(known.is_readonly, declared.is_readonly(), "{what}");
                assert_eq!(known.is_list, declared.is_list(), "{what}");
                assert_eq!(known.is_text, declared.ty == TypeTag::String, "{what}");
                assert_eq!(known.is_url, declared.ty == TypeTag::Url, "{what}");
                let is_component = COMPONENTS.contains(&(uri, name, &declared.name));
                assert_eq!(known.is_component, is_component, "{what}");
                // Every type a property names is one the table has: a
                // value under it can be looked into.
                let is_enum = matches!(declared.ty, TypeTag::Enum(_) | TypeTag::Flags(_));
                assert!(known.value.is_some() || is_enum, "the type of {what}");
            }
            for signal in meta.own_signals() {
                let parameters: Vec<String> =
                    signal.params.iter().map(|(p, _)| p.to_string()).collect();
                assert_eq!(
                    oracle.signal(ty, &signal.name),
                    Some(parameters),
                    "{name}.{}",
                    signal.name
                );
            }
            for method in meta.own_methods() {
                assert!(
                    oracle.has_method(ty, &method.name),
                    "{name}.{}",
                    method.name
                );
            }
            // What it inherits is there too, and nothing that is not.
            if meta.base.is_some() {
                assert!(oracle.property(ty, "objectName").is_some(), "{name}");
            }
            assert!(oracle.property(ty, "nothing").is_none(), "{name}");
            assert!(oracle.signal(ty, "nothing").is_none(), "{name}");
            assert!(!oracle.has_method(ty, "nothing"), "{name}");
        }
    }
}

#[test]
fn a_test_case_has_its_own_members_and_an_items() {
    let Some(oracle) = oracle() else { return };
    let case = oracle.module_type("QtTest", "TestCase").expect("TestCase");
    assert!(oracle.has_method(case, "compare"));
    assert!(oracle.property(case, "when").is_some());
    assert!(oracle.property(case, "width").is_some(), "an Item's");
    assert!(oracle.signal(case, "whenChanged").is_some());
    assert!(oracle.signal(case, "widthChanged").is_some());
    assert_eq!(oracle.default_property(case), Some("data"));
    let spy = oracle
        .module_type("QtTest", "SignalSpy")
        .expect("SignalSpy");
    assert!(oracle.has_method(spy, "clear") && !oracle.has_method(spy, "compare"));
}

#[test]
fn an_own_module_has_its_own_types_and_no_others() {
    let Some(oracle) = oracle() else { return };
    assert!(oracle.module_type("QtQuick", "TestCase").is_none());
    assert!(oracle.module_type("QtQuick", "ErrorBoundary").is_none());
    assert!(oracle.module_type("QtTest", "ErrorBoundary").is_none());
    // What a module's types are written with is not the module's: a file
    // that imports only `Ljos` has no `Item`.
    assert!(oracle.module_type("Ljos", "Item").is_none());
    assert!(oracle.module_type("QtTest", "Item").is_none());
    let item = oracle.module_type("QtQuick", "Item").expect("Item");
    assert!(!oracle.has_method(item, "compare"));
    // A group is a type of its own that no file can write.
    assert!(oracle.module_type("Ljos", "ErrorBoundaryFault").is_none());
}
