//! What a QML object type can signal: the modules, the types each exports,
//! what each inherits, its property names and its signals. That is what
//! a tool needs to say which `onName` handlers an object takes, and it is
//! all this crate has.
//!
//! `qt_types` describes the types whole (enums, methods, what a property
//! holds, attached types) and reads its text when first asked. This crate
//! is that table's answer to the one question, written at build time by a
//! build script that links `qt_types` on the building machine, so a web
//! build carries neither the description nor the code that reads it. Both
//! answer alike: the lookups here are `qt_types`' of the same names
//! (`tests/same_as_qt_types.rs`), with one difference: a type's members
//! include those of the class that extends it (Qt's `extension`), which
//! `qt_types` keeps apart.
//!
//! Only types named with a capital are exported: an object is written
//! `Name { }`, and `color` or `font` never is.

use std::collections::HashMap;
use std::sync::OnceLock;

const TEXT: &str = include_str!(concat!(env!("OUT_DIR"), "/signals.txt"));

/// The module `uri` is imported as, when the table has it.
pub fn module(uri: &str) -> Option<&'static Module> {
    table().modules.get(uri)
}

/// The URI of every module the table has, in order.
pub fn modules() -> Vec<&'static str> {
    let mut uris: Vec<&'static str> = table().modules.keys().copied().collect();
    uris.sort_unstable();
    uris
}

#[derive(Debug)]
pub struct Module {
    pub uri: &'static str,
    /// The modules importing this one brings along, in the order it names
    /// them: `QtQuick` has `Timer` because it brings `QtQml`.
    pub imports: Vec<&'static str>,
    exports: HashMap<&'static str, usize>,
}

impl Module {
    /// The name of every type this module exports itself, in order; not those
    /// of the modules it brings along.
    pub fn exported(&self) -> Vec<&'static str> {
        let mut names: Vec<&'static str> = self.exports.keys().copied().collect();
        names.sort_unstable();
        names
    }

    /// The type `name` is in a file that imports this module: one of its own
    /// or of a module that comes with it.
    pub fn type_named(&self, name: &str) -> Option<&'static Type> {
        let table = table();
        let index = table.type_named(self.uri, name, &mut Vec::new())?;
        Some(&table.types[index])
    }
}

#[derive(Debug)]
pub struct Type {
    /// The QML name, or the class of a type no module exports.
    pub name: &'static str,
    /// The type's own members; the lookups below also see inherited ones.
    pub properties: Vec<Property>,
    pub signals: Vec<Signal>,
    /// May be written as `Name { }`. False for a type that can only be a
    /// base or an attached type, a singleton, a value or a namespace
    /// (`Keys`, `Layout`).
    pub creatable: bool,
    prototype: Option<usize>,
}

impl Type {
    /// Every property the type has, its own and inherited, each name once and
    /// the nearest first.
    pub fn all_properties(&'static self) -> Vec<&'static Property> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        let mut ty = Some(self);
        while let Some(t) = ty {
            out.extend(t.properties.iter().filter(|p| seen.insert(p.name)));
            ty = t.prototype();
        }
        out
    }

    /// Every signal the type has, its own and inherited, each name once and
    /// the nearest first.
    pub fn all_signals(&'static self) -> Vec<&'static Signal> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        let mut ty = Some(self);
        while let Some(t) = ty {
            out.extend(t.signals.iter().filter(|s| seen.insert(s.name)));
            ty = t.prototype();
        }
        out
    }

    pub fn prototype(&self) -> Option<&'static Type> {
        self.prototype.map(|index| &table().types[index])
    }

    pub fn property(&'static self, name: &str) -> Option<&'static Property> {
        self.find(&|ty| ty.properties.iter().find(|p| p.name == name))
    }

    /// The signal `name`, which `onName` handles. A property's change signal
    /// is one of these.
    pub fn signal(&'static self, name: &str) -> Option<&'static Signal> {
        self.find(&|ty| ty.signals.iter().find(|s| s.name == name))
    }

    fn find<T>(&'static self, get: &impl Fn(&'static Type) -> Option<T>) -> Option<T> {
        get(self).or_else(|| self.prototype()?.find(get))
    }
}

#[derive(Debug)]
pub struct Property {
    pub name: &'static str,
}

#[derive(Debug)]
pub struct Signal {
    pub name: &'static str,
    /// What a handler may call the arguments without declaring them; `_`
    /// for an argument with no name.
    pub parameters: Vec<&'static str>,
    /// Each argument's type, beside `parameters`; empty where the table has
    /// none.
    pub parameter_types: Vec<&'static str>,
}

struct Table {
    modules: HashMap<&'static str, Module>,
    types: Vec<Type>,
}

impl Table {
    fn type_named(&self, uri: &str, name: &str, seen: &mut Vec<&'static str>) -> Option<usize> {
        let module = self.modules.get(uri)?;
        if seen.contains(&module.uri) {
            return None;
        }
        seen.push(module.uri);
        if let Some(index) = module.exports.get(name) {
            return Some(*index);
        }
        module
            .imports
            .iter()
            .find_map(|import| self.type_named(import, name, seen))
    }
}

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| read(TEXT))
}

/// Reads the text the build script wrote; its format is described there.
fn read(text: &'static str) -> Table {
    let mut modules = HashMap::new();
    let mut module: Option<Module> = None;
    let mut types: Vec<Type> = Vec::new();
    let index = |word: Option<&str>| word.and_then(|w| w.parse::<usize>().ok());
    for line in text.lines() {
        let mut words = line.split(' ');
        match (words.next(), words.next()) {
            (Some("m"), Some(uri)) => {
                if let Some(done) = module.replace(Module {
                    uri,
                    imports: Vec::new(),
                    exports: HashMap::new(),
                }) {
                    modules.insert(done.uri, done);
                }
            }
            (Some("i"), Some(uri)) => {
                if let Some(m) = &mut module {
                    m.imports.push(uri);
                }
            }
            (Some("e"), Some(name)) => {
                if let (Some(m), Some(at)) = (&mut module, index(words.next())) {
                    m.exports.insert(name, at);
                }
            }
            (Some("t"), Some(name)) => types.push(Type {
                name,
                properties: Vec::new(),
                signals: Vec::new(),
                prototype: index(words.next()),
                // A word the table of before this flag lacks reads as
                // creatable.
                creatable: words.next() != Some("u"),
            }),
            (Some("p"), Some(first)) => {
                let Some(ty) = types.last_mut() else { continue };
                for word in std::iter::once(first).chain(words) {
                    let name = word.strip_suffix('+').unwrap_or(word);
                    ty.properties.push(Property { name });
                    if word.ends_with('+') {
                        // Read once, for the life of the program. Joined
                        // by hand: `format!` would bring in its machinery.
                        let changed: &'static str = [name, "Changed"].concat().leak();
                        ty.signals.push(Signal {
                            name: changed,
                            parameters: Vec::new(),
                            parameter_types: Vec::new(),
                        });
                    }
                }
            }
            (Some("s"), Some(name)) => {
                let Some(ty) = types.last_mut() else { continue };
                let mut signal = Signal {
                    name,
                    parameters: Vec::new(),
                    parameter_types: Vec::new(),
                };
                for word in words {
                    match word.split_once(':') {
                        Some((parameter, ty)) => {
                            signal.parameters.push(parameter);
                            signal.parameter_types.push(ty);
                        }
                        None => signal.parameters.push(word),
                    }
                }
                ty.signals.push(signal);
            }
            _ => {}
        }
    }
    if let Some(done) = module {
        modules.insert(done.uri, done);
    }
    Table { modules, types }
}
