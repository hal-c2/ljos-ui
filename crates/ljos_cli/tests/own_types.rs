//! The compiler's table of the modules Ljós implements is what their types
//! declare.
//!
//! The front end reads QML against a table of types (`qt_types`). Ljós's
//! description of its own modules is `crates/qt_types/src/ljos.txt`, which
//! is committed: it is the whole table of a build that has no table of Qt's
//! types, and the part for Ljós's own modules (`Ljos`, `QtTest`) of a build
//! that has one. The crates that read QML must not link the runtime that has
//! the types, so they cannot ask them: the description reaches them as that
//! text, and this is what writes it and holds it to the types.
//!
//! ```text
//! LJOS_OWN_TYPES=write cargo test -p ljos_cli --test own_types
//! ```
//!
//! writes the file from the types as they are now. Without that, the test
//! fails when the file is not what would be written: a property added to
//! `Rectangle` in Rust cannot be left out of what the compiler knows.
//!
//! Nothing here reads Qt's table to write: what is written is what the
//! engine's registry, the value types and the style's QML files say
//! (CLEAN_ROOM.md, ARCHITECTURE.md D1). Where Qt's table is compiled in,
//! `what_qt_says_of_a_type_ljos_has` compares the two, which is how a
//! native type that says too little is found.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    fmt::Write as _,
    fs,
    path::PathBuf,
};

use ljos_engine::{
    Engine, EnumInfo, MetaObject, MetaRef, PropertyAccess, PropertyFlags, PropertyInfo, TypeTag,
    VALUE_TYPES, qt_namespace,
};
use qml_front::{QtTypeOracle, TypeOracle};

/// Set to `write` to write the table instead of comparing it.
const WRITE: &str = "LJOS_OWN_TYPES";

/// The command that writes the table.
const COMMAND: &str = "LJOS_OWN_TYPES=write cargo test -p ljos_cli --test own_types";

/// The module whose `Qt` is the namespace of enums every script has.
const QT_NAMESPACE: (&str, &str) = ("QtQml", "Qt");

/// The type an object is when nothing says more, as a description names it.
const OBJECT: &str = "QtQml.QtObject";

const HEADER: &str = "\
# Every module Ljós implements, as its native types and the QML files of its
# style declare them, in the format of `qt_types` (see that crate's
# documentation). Nothing in it comes from Qt's description of its types.
#
# Do not edit: `crates/ljos_cli/tests/own_types.rs` writes this file from a
# running engine, and fails when it is not what the types declare now.
#
#     LJOS_OWN_TYPES=write cargo test -p ljos_cli --test own_types
#
# The order things are built in, which is not a circle because this text is
# committed: `qt_types` compiles this file in; `ljos_controls`' build script
# compiles the style's QML against it; the command above links the runtime
# and writes this file. After a native member is added that a style file
# uses, a build without Qt's table needs the command twice: once to write
# the native types (the style does not compile yet and is left out), once
# more for the style's own. The command says when that is so.
";

fn committed() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../qt_types/src/ljos.txt")
}

/// An engine with every native module the `ljos` program can run.
fn engine() -> Engine {
    let engine = Engine::new();
    ljos_quick::register(&engine);
    ljos_controls::register(&engine);
    ljos_media::register(&engine);
    ljos_localstorage::register(&engine);
    ljos_test::qttest::register(&engine);
    engine
}

/// What tells one registered type from another: a native type is its
/// description, a type written in QML is its file (the engine makes a
/// description of it wherever it is asked for one).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum Id {
    Native(usize),
    File(String),
}

fn pointer(meta: &MetaObject) -> usize {
    std::ptr::from_ref(meta) as usize
}

struct Registered {
    meta: MetaRef,
    id: Id,
}

/// Where a type is described. Several modules may register one type
/// (`Window` is in `QtQuick` and in `QtQuick.Window`): it is described in
/// one, its home, and the others export it from there.
struct Home {
    module: String,
    name: String,
    class: String,
}

impl Home {
    /// The type as another description names it: what a file that imports
    /// the module calls it.
    fn named(&self) -> String {
        format!("{}.{}", self.module, self.name)
    }
}

/// Everything the engine has registered, by module and name.
struct Types<'a> {
    engine: &'a Engine,
    modules: BTreeMap<String, BTreeMap<String, Registered>>,
    homes: HashMap<Id, Home>,
    by_pointer: HashMap<usize, Id>,
}

impl<'a> Types<'a> {
    fn new(engine: &'a Engine) -> Self {
        let mut modules: BTreeMap<String, BTreeMap<String, Registered>> = BTreeMap::new();
        let mut by_pointer = HashMap::new();
        for value in VALUE_TYPES {
            modules.entry(value.module.to_owned()).or_default();
        }
        for module in engine.type_dump().modules {
            let types = modules.entry(module.uri.clone()).or_default();
            for ty in module.types {
                let meta = engine
                    .lookup_type(&module.uri, &ty.name)
                    .expect("a registered type");
                let id = match engine.qml_type_file(&module.uri, &ty.name) {
                    Some(key) => Id::File(key),
                    None => Id::Native(pointer(&meta)),
                };
                by_pointer.insert(pointer(&meta), id.clone());
                types.insert(ty.name, Registered { meta, id });
            }
        }

        // In the order of the modules' names, so that the home of a type no
        // module claims does not depend on what was registered first.
        let mut exporters: HashMap<&Id, Vec<&str>> = HashMap::new();
        for (uri, types) in &modules {
            for registered in types.values() {
                exporters.entry(&registered.id).or_default().push(uri);
            }
        }
        let mut homes = HashMap::new();
        for types in modules.values() {
            for (name, registered) in types {
                let exporters = &exporters[&registered.id];
                let claimed = match &registered.id {
                    // A file is of the module its directory is.
                    Id::File(key) => key.rsplit_once('/').map(|(dir, _)| dir.replace('/', ".")),
                    Id::Native(_) => registered.meta.module.as_ref().map(ToString::to_string),
                };
                let module = claimed
                    .filter(|claimed| exporters.contains(&claimed.as_str()))
                    .unwrap_or_else(|| exporters[0].to_owned());
                let class = match &registered.id {
                    Id::File(key) => format!("{key}.qml"),
                    Id::Native(_) => format!("{module}.{name}"),
                };
                homes.insert(
                    registered.id.clone(),
                    Home {
                        module,
                        name: name.clone(),
                        class,
                    },
                );
            }
        }
        Types {
            engine,
            modules,
            homes,
            by_pointer,
        }
    }

    /// The home of the type a description refers to, when a module exports
    /// that type.
    fn home_of(&self, meta: &MetaObject) -> Option<&Home> {
        self.homes.get(self.by_pointer.get(&pointer(meta))?)
    }

    /// The modules that come with `uri`, itself first.
    fn with_imports(&self, uri: &str) -> Vec<String> {
        let mut all = vec![uri.to_owned()];
        let mut at = 0;
        while at < all.len() {
            for imported in self.engine.imports_of(&all[at]) {
                if !all.contains(&imported) {
                    all.push(imported);
                }
            }
            at += 1;
        }
        all
    }

    /// The type `name` is in a file that imports `uri`.
    fn visible(&self, uri: &str, name: &str) -> Option<&Registered> {
        let near = self.with_imports(uri);
        near.iter().find_map(|uri| self.modules.get(uri)?.get(name))
    }

    /// The type a property of a type of `module` means by `name`: the one a
    /// file that imports the module has by that name, or else the one of
    /// that name anywhere (a native type names the type of another module
    /// without importing anything).
    fn meant(&self, module: &str, name: &str) -> Option<&Home> {
        self.visible(module, name)
            .or_else(|| self.modules.values().find_map(|types| types.get(name)))
            .map(|registered| &self.homes[&registered.id])
    }

    /// The whole table.
    fn written(&self) -> String {
        let mut out = String::from(HEADER);
        let mut classes = HashMap::new();
        for uri in self.modules.keys() {
            out.push('\n');
            out.push_str(&Module::new(self, uri, &mut classes).written());
        }
        out
    }
}

/// One module being written.
struct Module<'a, 'c> {
    all: &'a Types<'a>,
    uri: &'a str,
    /// Types the described ones lead to that no module exports: a group
    /// (`fault` of `fault.kind`), what a type attaches, a base that has no
    /// name in QML.
    unnamed: VecDeque<MetaRef>,
    /// Every class described so far, in any module, and the type it is.
    classes: &'c mut HashMap<String, usize>,
    types: String,
}

impl<'a, 'c> Module<'a, 'c> {
    fn new(all: &'a Types<'a>, uri: &'a str, classes: &'c mut HashMap<String, usize>) -> Self {
        Module {
            all,
            uri,
            unnamed: VecDeque::new(),
            classes,
            types: String::new(),
        }
    }

    fn written(mut self) -> String {
        let all = self.all;
        let mut out = format!("module {}\n", self.uri);
        for imported in all.engine.imports_of(self.uri) {
            let _ = writeln!(out, "\timport {imported}");
        }

        let mut exports = BTreeMap::new();
        let uri = self.uri;
        for value in VALUE_TYPES.iter().filter(|value| value.module == uri) {
            let name = (value.tag)().name();
            exports.insert(name.clone(), self.class(&name));
            self.describe_value(&name, value.members);
        }
        if self.uri == QT_NAMESPACE.0 {
            exports.insert(QT_NAMESPACE.1.to_owned(), self.class(QT_NAMESPACE.1));
            self.describe_namespace();
        }
        for (name, registered) in &all.modules[self.uri] {
            let home = &all.homes[&registered.id];
            exports.insert(name.clone(), home.class.clone());
            if home.module == self.uri {
                self.claim(&home.class, &registered.meta);
                self.describe(&registered.meta, &home.class, true);
            }
        }
        while let Some(meta) = self.unnamed.pop_front() {
            let class = self.class(&meta.name);
            self.describe(&meta, &class, false);
        }

        for (name, class) in exports {
            let _ = writeln!(out, "\t{name} {class}");
        }
        out + &self.types
    }

    /// The class of a type of this module that is not a file. No class of
    /// Qt's has a dot before its last word.
    fn class(&self, name: &str) -> String {
        format!("{}.{name}", self.uri)
    }

    /// Takes `class` for `meta`. False when it is that type's already.
    fn claim(&mut self, class: &str, meta: &MetaObject) -> bool {
        match self.classes.insert(class.to_owned(), pointer(meta)) {
            None => true,
            Some(other) => {
                assert!(
                    other == pointer(meta),
                    "two types would be the class `{class}`: one that `{}` exports and one that \
                     it does not. Give one of them another name.",
                    self.uri
                );
                false
            }
        }
    }

    /// How a description refers to the type `meta`: by what a module calls
    /// it when one exports it, by a class of this module otherwise.
    fn link(&mut self, meta: &MetaRef) -> String {
        if let Some(home) = self.all.home_of(meta) {
            return format!("{} named", home.named());
        }
        let class = self.class(&meta.name);
        if self.claim(&class, meta) {
            self.unnamed.push_back(meta.clone());
        }
        class
    }

    fn describe_value(&mut self, name: &str, members: &[(&str, &str)]) {
        let _ = writeln!(self.types, "\ttype {} {name}", self.class(name));
        let _ = writeln!(self.types, "\t\tis value");
        for (member, of) in members {
            let of = value_named(of);
            let _ = writeln!(self.types, "\t\tprop {member} {of} named");
        }
    }

    /// `Qt`: one object every script has, and the enums read off it.
    fn describe_namespace(&mut self) {
        let _ = writeln!(
            self.types,
            "\ttype {} {}",
            self.class(QT_NAMESPACE.1),
            QT_NAMESPACE.1
        );
        let _ = writeln!(self.types, "\t\tis singleton");
        for enumeration in qt_namespace() {
            let name = match (enumeration.name, enumeration.alias) {
                // Keys the engine has without the name of their enum.
                ("", _) => "enum _".to_owned(),
                (name, "") => format!("enum {name}"),
                (name, alias) => format!("flags {name}={alias}"),
            };
            let _ = write!(self.types, "\t\t{name}");
            for (key, number) in &enumeration.keys {
                let _ = write!(self.types, " {key}={number}");
            }
            self.types.push('\n');
        }
    }

    fn describe(&mut self, meta: &MetaRef, class: &str, exported: bool) {
        let engine = self.all.engine;
        let mut out = format!("\ttype {class} {}\n", meta.name);
        if let Some(base) = &meta.base {
            let link = self.link(base);
            let _ = writeln!(out, "\t\tprototype {link}");
        }
        if let Some(attached) = meta.attached {
            let link = self.link(&attached());
            let _ = writeln!(out, "\t\tattached {link}");
        }
        if let Some(name) = &meta.default_property {
            let _ = writeln!(out, "\t\tdefault {name}");
        }
        if exported && engine.is_native_singleton(self.uri, &meta.name) {
            let _ = writeln!(out, "\t\tis singleton");
        } else if !exported || meta.create.is_none() {
            let _ = writeln!(out, "\t\tis uncreatable");
        }
        if engine.has_open_body(meta) {
            let _ = writeln!(out, "\t\tis parser");
        }
        // The enums are flattened: one a base has is said there, unless
        // this type gives the name keys of its own.
        let inherited = |own: &EnumInfo| {
            meta.base
                .iter()
                .flat_map(|base| base.enums.iter())
                .any(|e| e.name == own.name && e.keys == own.keys)
        };
        for enumeration in &meta.enums {
            if inherited(enumeration) {
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
        // A group's type, by the group's name, from its leaves.
        let groups: HashMap<&str, &'static MetaObject> = meta
            .own_properties()
            .iter()
            .filter_map(|p| match &p.access {
                PropertyAccess::Group { meta: group, .. } => {
                    Some((p.name.split_once('.')?.0, *group))
                }
                _ => None,
            })
            .collect();
        for property in meta.own_properties() {
            // `fault.kind` is a property of what `fault` is.
            if let PropertyAccess::Group { meta: group, .. } = &property.access {
                let _ = self.link(&MetaRef::from(*group));
                continue;
            }
            // A property that takes any object says the class a
            // description has it hold, where it says one.
            let value = match (&property.ty, &property.holds) {
                (TypeTag::Object(None), Some(class)) => {
                    self.value(&TypeTag::Object(Some(class.clone())))
                }
                // A group whose type a module exports is that type, which
                // is described there and not again as a class of this one.
                (TypeTag::Group(_), _) => match groups
                    .get(&*property.name)
                    .and_then(|group| self.all.home_of(group))
                {
                    Some(home) => format!("{} pointer", home.named()),
                    None => self.value(&property.ty),
                },
                (ty, _) => self.value(ty),
            };
            let _ = write!(out, "\t\tprop {} {value}", property.name);
            // A list by how it is assigned, whatever it is declared to hold.
            if property.is_list() && !matches!(property.ty, TypeTag::List(_)) {
                out.push_str(" list");
            }
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
            if engine.is_script_property_of(meta, &property.name) {
                out.push_str(" script");
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

    /// An object type as a property names it.
    fn object(&self, name: &str) -> String {
        match self.all.meant(self.uri, name) {
            Some(home) => format!("{} named", home.named()),
            // A name no module has: a value of no known type.
            None => name.to_owned(),
        }
    }

    /// What a property holds, in the table's words: the type as a module
    /// calls it (`named`), and what kind of value it is.
    fn value(&self, ty: &TypeTag) -> String {
        match ty {
            TypeTag::Object(None) => format!("{OBJECT} named pointer"),
            TypeTag::Object(Some(name)) => format!("{} pointer", self.object(name)),
            TypeTag::List(None) => format!("{} named list", value_named("var")),
            TypeTag::List(Some(of)) => {
                if is_object_name(of) {
                    format!("{} list pointer", self.object(of))
                } else {
                    format!("{} named list", value_named(of))
                }
            }
            // An enum is named as the table names one: by its own name in
            // the type that has it, `Qt::Alignment` in another.
            TypeTag::Enum(name) | TypeTag::Flags(name) => name.replace('.', "::"),
            TypeTag::Group(name) => format!("{} pointer", self.class(name)),
            TypeTag::Void => format!("{} named", value_named("var")),
            _ => format!("{} named", value_named(&ty.name())),
        }
    }
}

/// Whether a type a property names is an object's: those are capitalised.
fn is_object_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_uppercase())
}

/// A value type as a description names it. A name the engine has no value
/// type for is held as anything.
fn value_named(name: &str) -> String {
    let value = VALUE_TYPES
        .iter()
        .find(|value| (value.tag)().name() == name)
        .unwrap_or(&VALUE_TYPES[0]);
    format!("{}.{}", value.module, (value.tag)().name())
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

/// Whether the style's QML files are types of this engine. They are not
/// when they did not compile as this build was made.
fn has_style(engine: &Engine) -> bool {
    let dump = engine.type_dump();
    dump.modules.iter().any(|module| {
        module
            .types
            .iter()
            .any(|ty| engine.qml_type_file(&module.uri, &ty.name).is_some())
    })
}

/// Whether the table is being written. The table this build has compiled
/// in is then the one being replaced, and nothing here judges it: run the
/// tests again without `LJOS_OWN_TYPES` for that.
fn writing() -> bool {
    std::env::var_os(WRITE).is_some_and(|value| value == "write")
}

#[test]
fn the_table_of_own_types_is_what_the_types_declare() {
    let engine = engine();
    let now = Types::new(&engine).written();
    let path = committed();
    if writing() {
        fs::write(&path, now).expect("the table can be written");
        assert!(
            has_style(&engine),
            "{} is written from the native types, but the QML files of the style did not compile \
             against the table as it was, so their types are not in it yet. Run the same command \
             once more:\n\n    {COMMAND}\n",
            path.display()
        );
        return;
    }
    let committed = fs::read_to_string(&path).expect("the table is committed");
    assert!(
        committed == now,
        "{} is not what Ljós's types declare now: a type or a member was added, changed or \
         taken away. Write it again with\n\n    {COMMAND}\n\nand commit it. First difference: {}",
        path.display(),
        first_difference(&committed, &now)
    );
}

/// The modules the table has from Ljós's own description: all of them in a
/// build without Qt's table, the ones Qt has not in a build with it.
fn own_modules(engine: &Engine) -> Vec<String> {
    let dump = engine.type_dump();
    let uris = dump.modules.into_iter().map(|module| module.uri);
    uris.filter(|uri| qt_types::module(uri).is_some_and(|module| module.is_own))
        .collect()
}

#[test]
fn the_compiler_knows_every_module_that_is_registered() {
    if writing() {
        return;
    }
    let oracle = QtTypeOracle;
    for module in engine().type_dump().modules {
        assert!(
            oracle.has_module(&module.uri),
            "`{}` has types and the compiler does not know it: write the table again with\n\n    \
             {COMMAND}\n",
            module.uri
        );
    }
}

#[test]
fn the_compiler_sees_every_member_the_types_declare() {
    if writing() {
        return;
    }
    let oracle = QtTypeOracle;
    let engine = engine();
    let types = Types::new(&engine);
    let own = own_modules(&engine);
    assert!(own.iter().any(|uri| uri == "Ljos"), "{own:?}");
    for uri in &own {
        for (name, registered) in &types.modules[uri] {
            let meta = &registered.meta;
            let home = &types.homes[&registered.id];
            let what = format!("{uri}.{name}");
            let ty = oracle
                .module_type(uri, name)
                .unwrap_or_else(|| panic!("the compiler has no `{what}`"));
            if !own.contains(&home.module) {
                // Described where Qt's table describes that module: what it
                // is there is Qt's to say.
                continue;
            }
            assert_eq!(oracle.name(ty), name, "{what}");
            assert_eq!(
                oracle.module(ty).as_deref(),
                Some(home.module.as_str()),
                "{what}"
            );
            assert_eq!(
                oracle.is_singleton(ty),
                engine.is_native_singleton(uri, name),
                "{what}"
            );
            assert_eq!(
                oracle.default_property(ty),
                meta.default_property().map(|name| name.as_str()),
                "the default property of {what}"
            );
            assert_eq!(
                oracle.members_known(ty),
                !meta.chain().any(|meta| engine.has_open_body(meta)),
                "whether {what} takes only its members"
            );
            assert_eq!(
                oracle.attached(ty).is_some(),
                meta.chain().any(|meta| meta.attached.is_some()),
                "what {what} attaches"
            );
            for declared in meta.own_properties() {
                let what = format!("{what}.{}", declared.name);
                // `fault.kind`: a property of the value of `fault`.
                let mut path = declared.name.split('.');
                let first = path.next().expect("a name");
                let mut known = oracle.property(ty, first).expect(&what);
                for part in path {
                    let value = known.value.expect(&what);
                    known = oracle.property(value, part).expect(&what);
                }
                assert_eq!(known.is_readonly, declared.is_readonly(), "{what}");
                assert_eq!(known.is_list, holds_list(declared), "{what}");
                assert_eq!(known.is_text, declared.ty == TypeTag::String, "{what}");
                assert_eq!(known.is_url, declared.ty == TypeTag::Url, "{what}");
                assert_eq!(known.is_component, holds_component(&declared.ty), "{what}");
                assert_eq!(
                    known.is_script,
                    engine.is_script_property_of(meta, &declared.name),
                    "{what}"
                );
                // Every type a property names is one the table has: a
                // value under it can be looked into.
                let is_enum = matches!(declared.ty, TypeTag::Enum(_) | TypeTag::Flags(_));
                let is_unknown = matches!(
                    &declared.ty,
                    TypeTag::Object(Some(of)) | TypeTag::List(Some(of))
                        if is_object_name(of) && types.meant(uri, of).is_none()
                );
                assert!(
                    known.value.is_some() || is_enum || is_unknown,
                    "the type of {what}"
                );
            }
            for signal in meta.own_signals() {
                let parameters: Vec<String> =
                    signal.params.iter().map(|(p, _)| p.to_string()).collect();
                assert_eq!(
                    oracle.signal(ty, &signal.name),
                    Some(parameters),
                    "{what}.{}",
                    signal.name
                );
            }
            for method in meta.own_methods() {
                assert!(
                    oracle.has_method(ty, &method.name),
                    "{what}.{}",
                    method.name
                );
            }
            for enumeration in &meta.enums {
                for (key, _) in &enumeration.keys {
                    assert!(oracle.has_enum_key(ty, None, key), "{what}.{key}");
                }
            }
            // What it inherits is there too, and nothing that is not.
            if meta.base.is_some() {
                assert!(oracle.property(ty, "objectName").is_some(), "{what}");
            }
            assert!(oracle.property(ty, "nothing").is_none(), "{what}");
            assert!(oracle.signal(ty, "nothing").is_none(), "{what}");
            assert!(!oracle.has_method(ty, "nothing"), "{what}");
        }
    }
}

/// Whether a property takes several values: declared a list, or assigned
/// as one.
fn holds_list(property: &PropertyInfo) -> bool {
    property.is_list() || matches!(property.ty, TypeTag::List(_))
}

/// Whether what is written in a property of the type is a template.
fn holds_component(ty: &TypeTag) -> bool {
    matches!(ty, TypeTag::Object(Some(of)) if of == "Component")
}

#[test]
fn a_value_has_the_members_the_engine_gives_it() {
    if writing() {
        return;
    }
    let oracle = QtTypeOracle;
    for value in VALUE_TYPES {
        let name = (value.tag)().name();
        let ty = oracle
            .module_type(value.module, &name)
            .unwrap_or_else(|| panic!("`{}` has no `{name}`", value.module));
        for (member, _) in value.members {
            let what = format!("{name}.{member}");
            // Qt's table describes Qt's values: what Ljós adds to one is
            // not in it, and is all that may be missing from it.
            let added = qt_types::has_qt_table() && ADDED_TO_QT_S_VALUES.contains(&what.as_str());
            assert_eq!(oracle.property(ty, member).is_some(), !added, "{what}");
        }
    }
}

/// The members Ljós gives a value type of Qt's that Qt does not have
/// (`features/a11y/font-scale.feature`). A build with Qt's table does not
/// know them, so `ljos check` there holds one to be a slip.
const ADDED_TO_QT_S_VALUES: &[&str] = &["font.scaleWithSystem"];

#[test]
fn the_namespace_has_the_keys_the_engine_gives_a_script() {
    if writing() {
        return;
    }
    let oracle = QtTypeOracle;
    let qt = oracle
        .module_type(QT_NAMESPACE.0, QT_NAMESPACE.1)
        .expect("Qt");
    for enumeration in qt_namespace() {
        for (key, number) in &enumeration.keys {
            assert!(oracle.has_enum_key(qt, None, key), "Qt.{key}");
            // Qt's description has the keys without their numbers.
            if !qt_types::has_qt_table() {
                assert_eq!(oracle.enum_value(qt, None, key), Some(*number), "Qt.{key}");
            }
        }
        if !enumeration.alias.is_empty() {
            assert!(oracle.enum_is_flags(qt, enumeration.name));
            assert!(!oracle.enum_is_flags(qt, enumeration.alias));
        }
    }
}

#[test]
fn a_test_case_has_its_own_members_and_an_items() {
    if writing() {
        return;
    }
    let oracle = QtTypeOracle;
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
fn a_module_has_its_own_types_and_no_others() {
    if writing() {
        return;
    }
    let oracle = QtTypeOracle;
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
    // What comes with a module is the module's to a file that imports it.
    assert!(oracle.module_type("QtQuick", "Timer").is_some());
    assert!(oracle.module_type("QtQml", "Item").is_none());
}

#[test]
fn a_control_is_its_style_file_over_its_template() {
    if writing() {
        return;
    }
    let oracle = QtTypeOracle;
    let button = oracle
        .module_type("QtQuick.Controls", "Button")
        .expect("Button");
    assert_eq!(
        oracle.module_type("QtQuick.Controls.Basic", "Button"),
        Some(button)
    );
    let template = oracle
        .module_type("QtQuick.Templates", "Button")
        .expect("the template");
    assert_ne!(button, template);
    assert!(oracle.property(button, "text").is_some(), "the template's");
    assert!(oracle.property(button, "width").is_some(), "an Item's");
    assert!(oracle.signal(button, "clicked").is_some());
}

/// What the front end goes by, of one type.
#[derive(Debug, PartialEq, Eq)]
struct Facts {
    creatable: bool,
    singleton: bool,
    attaches: bool,
    default_property: Option<String>,
}

/// What the front end goes by, of one property.
#[derive(Debug, PartialEq, Eq)]
struct PropertyFacts {
    list: bool,
    component: bool,
    script: bool,
    text: bool,
    url: bool,
}

/// Qt's table and Ljós's types describe the same modules. Neither is
/// written from the other, so where both have a name they can disagree, and
/// a file would then be read one way with Qt's table and another without.
/// This finds it: for every type and property Ljós has that Qt's table has
/// too, what the front end goes by is the same.
///
/// A difference is a native type that says too little or the wrong thing
/// (a `delegate` that does not say it holds a `Component`); the fix is in
/// the type. What Qt has and Ljós has not is no difference: that is what
/// `ljos check` reports.
///
/// The differences there are today are in [`NOT_YET`], each with what it
/// would take. The test fails on one that is not listed, and on one that is
/// listed and gone, so the list only gets shorter.
#[test]
fn what_qt_says_of_a_type_ljos_has() {
    if writing() {
        return;
    }
    if !qt_types::test_has_qt_table("what_qt_says_of_a_type_ljos_has") {
        return;
    }
    let oracle = QtTypeOracle;
    let engine = engine();
    let types = Types::new(&engine);
    let mut differences: Vec<(String, String)> = Vec::new();
    let every_name: BTreeSet<&String> = types.modules.values().flat_map(BTreeMap::keys).collect();
    for (uri, registered) in &types.modules {
        if qt_types::module(uri).is_none_or(|module| module.is_own) {
            continue;
        }
        // Which of the names Ljós has an import of the module gives.
        for name in &every_name {
            let in_qt = oracle.module_type(uri, name).is_some();
            let in_ljos = types.visible(uri, name).is_some();
            if in_qt && !in_ljos {
                differences.push((
                    format!("{uri}: {name}"),
                    "a type of a file that imports the module to Qt and not to Ljós, which has \
                     the type elsewhere"
                        .to_owned(),
                ));
            }
        }
        for (name, registered) in registered {
            let Some(ty) = oracle.module_type(uri, name) else {
                // A type of Ljós's own in a module of Qt's: there only
                // without Qt's table.
                differences.push((
                    format!("{uri}: {name}"),
                    "no type of Qt's module".to_owned(),
                ));
                continue;
            };
            let meta = &registered.meta;
            let singleton = engine.is_native_singleton(uri, name);
            let ljos = Facts {
                creatable: meta.create.is_some() && !singleton,
                singleton,
                attaches: meta.chain().any(|meta| meta.attached.is_some()),
                default_property: meta.default_property().map(ToString::to_string),
            };
            let qt = Facts {
                creatable: ty.0.is_creatable,
                singleton: oracle.is_singleton(ty),
                attaches: oracle.attached(ty).is_some(),
                default_property: oracle.default_property(ty).map(str::to_owned),
            };
            if ljos != qt {
                differences.push((format!("{uri}.{name}"), format!("Ljós {ljos:?}, Qt {qt:?}")));
            }
            for declared in &meta.properties {
                if declared.name.contains('.') {
                    continue;
                }
                let Some(known) = oracle.property(ty, &declared.name) else {
                    continue;
                };
                let ljos = PropertyFacts {
                    list: holds_list(declared),
                    component: holds_component(&declared.ty),
                    script: engine.is_script_property_of(meta, &declared.name),
                    text: declared.ty == TypeTag::String,
                    url: declared.ty == TypeTag::Url,
                };
                let qt = PropertyFacts {
                    list: known.is_list,
                    component: known.is_component,
                    script: known.is_script,
                    text: known.is_text,
                    url: known.is_url,
                };
                if ljos != qt {
                    differences.push((
                        format!("{uri}.{name}.{}", declared.name),
                        format!("Ljós {ljos:?}, Qt {qt:?}"),
                    ));
                }
            }
        }
    }
    differences.sort();
    differences.dedup();
    let listed = |subject: &str| NOT_YET.iter().any(|(known, _)| *known == subject);
    let new: Vec<String> = differences
        .iter()
        .filter(|(subject, _)| !listed(subject))
        .map(|(subject, detail)| format!("{subject}: {detail}"))
        .collect();
    assert!(
        new.is_empty(),
        "{} differences between what Ljós's types declare and Qt's table that `NOT_YET` does not \
         list. Each is a type that says too little (fix the type), or is added to the list with \
         what it would take:\n{}",
        new.len(),
        new.join("\n")
    );
    let gone: Vec<&str> = NOT_YET
        .iter()
        .map(|(subject, _)| *subject)
        .filter(|subject| !differences.iter().any(|(found, _)| found == subject))
        .collect();
    assert!(
        gone.is_empty(),
        "no longer differences, so take them out of `NOT_YET`: {gone:?}"
    );
}

/// Where Ljós's types and Qt's table disagree today, and why each is still
/// there. `module.Type` is the type itself, `module.Type.property` one of
/// its properties, `module: Type` a type an import of the module gives a
/// file on Qt and not here.
const NOT_YET: &[(&str, &str)] = &[
    (
        "QtMultimedia.AudioDevice.id",
        "a byte array to Qt, which a script sees as an ArrayBuffer; Ljós has no such value, so the id is the device's name as text",
    ),
    (
        "QtMultimedia.Video",
        "an Item to Qt that holds a VideoOutput and declares no default property; here it is a VideoOutput, and so an item with one",
    ),
    (
        "QtQuick.Templates: SwipeDelegateSwipe",
        "the type of `SwipeDelegate.swipe`, anonymous to Qt; named here so that the table can describe it and a file's `swipe.left: Rectangle {}` is read as a component",
    ),
    (
        "QtQuick.Templates: Calendar",
        "Qt's Templates make it a creatable type; here it is the enum holder of QtQuick.Controls alone",
    ),
    (
        "QtQuick.Controls.Calendar",
        "a singleton to Qt; here a type that only holds the enum, which is all a file reads of it",
    ),
    (
        "QtQuick.Controls.Basic.Calendar",
        "a singleton to Qt; here a type that only holds the enum, which is all a file reads of it",
    ),
    (
        "QtQuick.Controls.Fusion.Calendar",
        "a singleton to Qt; here a type that only holds the enum, which is all a file reads of it",
    ),
    (
        "QtQuick.Controls.Material.Calendar",
        "a singleton to Qt; here a type that only holds the enum, which is all a file reads of it",
    ),
    (
        "QtQuick.Controls.Universal.Calendar",
        "a singleton to Qt; here a type that only holds the enum, which is all a file reads of it",
    ),
    (
        "QtQuick.PointerDevice.uniqueId",
        "a string here; Qt's pointingDeviceUniqueId value type is not a Ljós value yet",
    ),
    (
        "QtQml: Easing",
        "registered in QtQuick alone; Qt has it in QtQml as well",
    ),
    (
        "QtQuick.Easing",
        "a singleton to Qt; here a type that only holds the enum, which is all a file reads of it",
    ),
    // Qt's positioners attach `Positioner`'s properties under their own
    // names as well; the documented name is the only one here.
    ("QtQuick.Column", "does not attach under its own name"),
    ("QtQuick.Flow", "does not attach under its own name"),
    ("QtQuick.Grid", "does not attach under its own name"),
    ("QtQuick.Row", "does not attach under its own name"),
    // Items to Qt, so they have an item's default property; neither can be
    // written as an object, so nothing is ever written in one.
    ("QtQuick.Positioner", "attached only: no default property"),
    (
        "QtQuick.Templates.Overlay",
        "attached only: no default property",
    ),
    (
        "QtQuick.Controls.Overlay",
        "attached only: no default property",
    ),
    (
        "QtQuick.Controls.Basic.Overlay",
        "attached only: no default property",
    ),
    (
        "QtQuick.Controls.Fusion.Overlay",
        "attached only: no default property",
    ),
    (
        "QtQuick.Controls.Material.Overlay",
        "attached only: no default property",
    ),
    (
        "QtQuick.Controls.Universal.Overlay",
        "attached only: no default property",
    ),
    (
        "QtQuick.Shortcut.sequences",
        "one sequence here, a list to Qt",
    ),
    // `TextArea.flickable`, which puts a text area in a Flickable.
    (
        "QtQuick.Templates.TextArea",
        "`TextArea.flickable` is not there",
    ),
    (
        "QtQuick.Controls.TextArea",
        "`TextArea.flickable` is not there",
    ),
    (
        "QtQuick.Controls.Basic.TextArea",
        "`TextArea.flickable` is not there",
    ),
    (
        "QtQuick.Controls.Fusion.TextArea",
        "`TextArea.flickable` is not there",
    ),
    (
        "QtQuick.Controls.Material.TextArea",
        "`TextArea.flickable` is not there",
    ),
    (
        "QtQuick.Controls.Universal.TextArea",
        "`TextArea.flickable` is not there",
    ),
    // The header delegates, which Qt's table gives no default property
    // although the TableViewDelegate they are built on has one.
    (
        "QtQuick.Controls.HorizontalHeaderViewDelegate",
        "a style file here, which has the default property of the control it is",
    ),
    (
        "QtQuick.Controls.VerticalHeaderViewDelegate",
        "a style file here, which has the default property of the control it is",
    ),
    (
        "QtQuick.Controls.Basic.HorizontalHeaderViewDelegate",
        "a style file here, which has the default property of the control it is",
    ),
    (
        "QtQuick.Controls.Basic.VerticalHeaderViewDelegate",
        "a style file here, which has the default property of the control it is",
    ),
    (
        "QtQuick.Controls.Fusion.HorizontalHeaderViewDelegate",
        "a style file here, which has the default property of the control it is",
    ),
    (
        "QtQuick.Controls.Fusion.VerticalHeaderViewDelegate",
        "a style file here, which has the default property of the control it is",
    ),
    (
        "QtQuick.Controls.Material.HorizontalHeaderViewDelegate",
        "a style file here, which has the default property of the control it is",
    ),
    (
        "QtQuick.Controls.Material.VerticalHeaderViewDelegate",
        "a style file here, which has the default property of the control it is",
    ),
    (
        "QtQuick.Controls.Universal.HorizontalHeaderViewDelegate",
        "a style file here, which has the default property of the control it is",
    ),
    (
        "QtQuick.Controls.Universal.VerticalHeaderViewDelegate",
        "a style file here, which has the default property of the control it is",
    ),
    // `ObjectModel.index` on an item, and the rest of what Qt's models
    // attach to the items they make.
    (
        "QtQml.Models.ObjectModel",
        "attaches nothing to its items yet",
    ),
    // An object written in a view goes where one written in a Flickable
    // goes; to Qt it is the view's own (`data`).
    (
        "QtQuick.GridView",
        "the default property is the Flickable's `flickableData`, not `data`",
    ),
    (
        "QtQuick.ListView",
        "the default property is the Flickable's `flickableData`, not `data`",
    ),
    (
        "QtQuick.ViewSection",
        "only what a view's `section` is; Qt lets a file make one as well",
    ),
    // Qt's table names the parts of its Fusion style but describes nothing
    // of them, so it gives them no default property; here each is the Item
    // its file is built on. No file writes an object in one.
    (
        "QtQuick.Controls.Fusion.impl.ButtonPanel",
        "described by nothing in Qt's table, which so gives it no default property",
    ),
    (
        "QtQuick.Controls.Fusion.impl.CheckIndicator",
        "described by nothing in Qt's table, which so gives it no default property",
    ),
    (
        "QtQuick.Controls.Fusion.impl.RadioIndicator",
        "described by nothing in Qt's table, which so gives it no default property",
    ),
    (
        "QtQuick.Controls.Fusion.impl.SliderGroove",
        "described by nothing in Qt's table, which so gives it no default property",
    ),
    (
        "QtQuick.Controls.Fusion.impl.SliderHandle",
        "described by nothing in Qt's table, which so gives it no default property",
    ),
    (
        "QtQuick.Controls.Fusion.impl.SwitchIndicator",
        "described by nothing in Qt's table, which so gives it no default property",
    ),
    (
        "QtQuick.Controls.Fusion.impl.TextFieldBackground",
        "described by nothing in Qt's table, which so gives it no default property",
    ),
    (
        "QtQuick.Controls.Fusion.impl: BusyIndicatorImpl",
        "a part of Qt's Fusion; Ljós's Fusion draws its ring in BusyIndicator itself, and has the name in QtQuick.Controls.Basic.impl alone",
    ),
];
