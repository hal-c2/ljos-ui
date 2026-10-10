//! What the generator keeps of Qt's descriptions, before it is written out.

use std::collections::BTreeMap;

/// A type as `.qmltypes` knows it: by its C++ class name, or for a type
/// written in QML by the path of its file under the Qt directory.
#[derive(Debug, Default, Clone)]
pub struct Class {
    pub name: String,
    pub aliases: Vec<String>,
    pub prototype: Option<String>,
    pub extension: Option<String>,
    /// The extension gives only its enums (`Qt` on the `Qt` singleton).
    pub extension_is_namespace: bool,
    pub attached: Option<String>,
    pub default_property: Option<String>,
    pub semantics: Semantics,
    /// The element type of a sequence (`QList<QUrl>`).
    pub element: Option<String>,
    pub is_singleton: bool,
    pub is_uncreatable: bool,
    pub has_custom_parser: bool,
    /// `(module, QML name)`, and the newest version it has that name at.
    pub exports: BTreeMap<(String, String), (u32, u32)>,
    pub enums: Vec<Enum>,
    pub properties: Vec<Property>,
    pub signals: Vec<Signal>,
    pub methods: Vec<String>,
}

impl Class {
    pub fn members(&self) -> usize {
        self.enums.len() + self.properties.len() + self.signals.len() + self.methods.len()
    }
}

/// qmltypes' `accessSemantics`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Semantics {
    #[default]
    Reference,
    Value,
    Sequence,
    /// A namespace: nothing but enums.
    None,
}

#[derive(Debug, Default, Clone)]
pub struct Enum {
    pub name: String,
    pub alias: Option<String>,
    pub is_flag: bool,
    /// The number is there only when the description gives it.
    pub keys: Vec<(String, Option<i64>)>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Property {
    pub name: String,
    pub type_name: String,
    pub is_list: bool,
    pub is_readonly: bool,
    pub is_pointer: bool,
    pub is_required: bool,
}

#[derive(Debug, Default, Clone)]
pub struct Signal {
    pub name: String,
    pub parameters: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    /// `import`: the module's types are this module's too.
    Always,
    /// `default import`: as `import`, unless another of the module's
    /// `optional import`s is chosen in its place (a Controls style).
    Default,
    /// `optional import`: a module that may be chosen at run time.
    Optional,
}

#[derive(Debug, Clone)]
pub struct Import {
    pub module: String,
    pub kind: ImportKind,
}

#[derive(Debug, Default)]
pub struct Module {
    pub imports: Vec<Import>,
    /// QML name to class.
    pub exports: BTreeMap<String, String>,
    /// The classes described here, by name.
    pub classes: BTreeMap<String, Class>,
}

/// The modules by URI.
pub type Modules = BTreeMap<String, Module>;

/// The class `name` is in `module`: its own, or one of a module it imports.
pub fn export<'a>(modules: &'a Modules, module: &str, name: &str) -> Option<&'a str> {
    fn find<'a>(
        modules: &'a Modules,
        uri: &str,
        name: &str,
        seen: &mut Vec<String>,
    ) -> Option<&'a str> {
        if seen.iter().any(|seen| seen == uri) {
            return None;
        }
        seen.push(uri.to_string());
        let module = modules.get(uri)?;
        if let Some(class) = module.exports.get(name) {
            return Some(class);
        }
        module
            .imports
            .iter()
            .filter(|import| import.kind != ImportKind::Optional)
            .find_map(|import| find(modules, &import.module, name, seen))
    }
    find(modules, module, name, &mut Vec::new())
}
