//! What the QML files of a project say about each other.
//!
//! A file is read on its own, but the types it names are often other files:
//! the ones next to it, the ones of a module the project makes, or inline
//! components. Each file added is summarised once, by what it imports and by
//! what each of its components declares (its [`Shape`]), so that any file can
//! ask what another one's types have without parsing it again.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use oxc_ast_visit::Visit;
use qml_syntax::{
    Allocator,
    ast::*,
    js::{Expression, IdentifierReference},
};

use crate::{Code, Error, oracle::TypeOracle};

/// The files of a project, by the key each is added under: its path from the
/// project's root without `.qml` (`content/Clock`).
#[derive(Debug, Clone, Default)]
pub struct Project {
    files: HashMap<String, Summary>,
    /// The project's own modules: what `import Thermostat` brings, by the
    /// name of each type and the file it is.
    modules: HashMap<String, HashMap<String, String>>,
    /// The modules each file is a type of.
    memberships: HashMap<String, Vec<String>>,
    /// The files that stand in for a type the program provides natively.
    standins: HashSet<String>,
    /// The files the program keeps inside itself, by what it names each by
    /// (`qrc:/qt/qml/Thermostat/images/icon.png`): where each is, from the
    /// project's root.
    resources: BTreeMap<String, String>,
    /// What was wrong with the files that were kept in part. A file that is
    /// not here parsed whole.
    diagnostics: HashMap<String, Vec<Error>>,
    /// The style `import QtQuick.Controls` brings, when one was chosen.
    controls_style: Option<ControlsStyle>,
}

/// The style behind `import QtQuick.Controls`, as Qt chooses one when the
/// program starts (`QT_QUICK_CONTROLS_STYLE`, `qtquickcontrols2.conf`):
/// a built-in style (`Material`) or a module of the project's own that is
/// a custom style, whose types come first and whose missing ones come from
/// the fallback style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlsStyle {
    pub name: String,
    /// The built-in style a custom style's missing types come from; Basic
    /// when none is named, as in Qt 6.
    pub fallback: Option<String>,
}

impl ControlsStyle {
    /// The module of the built-in style `name`.
    pub fn builtin_module(name: &str) -> String {
        format!("QtQuick.Controls.{name}")
    }
}

/// A file's component or one of its inline components.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Key {
    pub file: String,
    pub inline: Option<String>,
}

/// What one file says, as far as the other files care.
#[derive(Debug, Clone, Default)]
pub struct Summary {
    /// What the file imports, in the order it does.
    pub imports: Vec<Import>,
    /// `pragma Singleton`: the file is one object, not a type to instantiate.
    pub is_singleton: bool,
    /// `pragma ComponentBehavior: Bound`: the components written in the file
    /// see the context they are made in and nothing else.
    pub is_bound: bool,
    /// The file's component, then its inline components.
    pub shapes: Vec<Shape>,
}

/// One `import` of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    pub source: Source,
    /// `import QtQuick.Controls as C`: its types are `C.Button`.
    pub alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A module, by its URI.
    Module(String),
    /// A directory or a JavaScript file, as written.
    Path(String),
}

/// What a component declares, by name, for the files that use it: an instance
/// is an object of the root's type with these members too.
#[derive(Debug, Clone)]
pub struct Shape {
    /// None for the file's own component.
    pub inline: Option<String>,
    /// The type of the root object, as written.
    pub root: Vec<String>,
    pub properties: HashMap<String, Declaration>,
    /// A signal's parameter names.
    pub signals: HashMap<String, Vec<String>>,
    pub functions: HashSet<String>,
    /// The properties whoever makes the object has to set, the inherited ones
    /// the component asks for (`required name`) among them.
    pub required: Vec<String>,
    /// Objects written inside an instance go somewhere the component says,
    /// not where its root type would put them.
    pub has_default: bool,
    /// The default property, when the component declares it a `Component`:
    /// a single object written inside an instance is a template for it.
    pub default_component: Option<String>,
    /// `enum Theme { Light, Dark }`: the keys of each, in the order written,
    /// with their numbers.
    pub enums: HashMap<String, Vec<EnumKey>>,
    /// The ids of its objects.
    pub ids: HashSet<String>,
    /// The names its scripts use that nothing in it declares: members of the
    /// runtime's types, mostly.
    pub mentions: HashSet<String>,
}

/// One key of an enum a component declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumKey {
    pub name: String,
    /// The number written (`Dark = 5`), or one more than the key before it
    /// (0 for the first).
    pub value: i64,
}

/// A `property` declaration, as far as an instance cares.
#[derive(Debug, Clone)]
pub struct Declaration {
    /// `int`, `var`, `alias`, `Component`, `Item`.
    pub type_name: String,
    pub is_list: bool,
    pub is_readonly: bool,
    /// What an alias is an alias of: the type of the object it names, as
    /// written, and the property of that object, where it names one.
    pub alias: Option<(Vec<String>, Vec<String>)>,
}

impl Project {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds the file `name`, with everything of it that parsed.
    ///
    /// A file with syntax errors is kept for what it has (the parser leaves
    /// out the members it could not read), so the files that use it, hot
    /// reload and a checker see it. `Err` has what was wrong; it does not
    /// mean the file was left out. Only a file without a root object, which
    /// has nothing to say, is: see [`has`](Self::has). What was wrong with a
    /// kept file is also under [`diagnostics`](Self::diagnostics), and
    /// [`is_partial`](Self::is_partial) is true of it, so that whoever reads
    /// its members knows more may have been declared.
    pub fn add(&mut self, name: &str, source: &str) -> Result<(), Vec<Error>> {
        let allocator = Allocator::default();
        let parsed = qml_syntax::parse(&allocator, source);
        let is_ok = parsed.is_ok();
        let mut errors: Vec<Error> = parsed
            .diagnostics
            .iter()
            .cloned()
            .map(Error::from)
            .collect();
        if !is_ok && errors.is_empty() {
            errors.push(Error::with_code(
                Code::Syntax,
                "the file could not be parsed",
                qml_syntax::Span::default(),
            ));
        }
        if parsed.panicked {
            // A previous version of the file is not what is written now.
            self.files.remove(name);
            self.diagnostics.insert(name.to_string(), errors.clone());
            return Err(errors);
        }
        self.insert(name, summarize(&parsed.document));
        if is_ok {
            return Ok(());
        }
        self.diagnostics.insert(name.to_string(), errors.clone());
        Err(errors)
    }

    /// Adds a summary. The file is taken to have parsed whole.
    pub fn insert(&mut self, name: &str, summary: Summary) {
        self.diagnostics.remove(name);
        self.files.insert(name.to_string(), summary);
    }

    /// Forgets the file `name` and what was wrong with it.
    pub fn remove(&mut self, name: &str) {
        self.files.remove(name);
        self.diagnostics.remove(name);
    }

    /// What was wrong with the file when it was last added, also when it was
    /// left out. Empty for a file that parsed whole, and for one that was
    /// not added.
    pub fn diagnostics(&self, file: &str) -> &[Error] {
        self.diagnostics.get(file).map_or(&[], Vec::as_slice)
    }

    /// Whether the file was added with syntax errors, and so has only what
    /// parsed. A file that is not here at all is not partial.
    pub fn is_partial(&self, file: &str) -> bool {
        self.files.contains_key(file)
            && self
                .diagnostics
                .get(file)
                .is_some_and(|errors| !errors.is_empty())
    }

    /// The directories the files import by path (`import "content"`), as the
    /// keys their files are to be added under: `content` for `content/Clock`.
    /// What is in them may import others, so this is asked again after they
    /// are added.
    pub fn directories(&self) -> Vec<String> {
        let mut directories = BTreeSet::new();
        for (file, summary) in &self.files {
            for import in &summary.imports {
                if let Source::Path(path) = &import.source
                    && !is_script(path)
                {
                    directories.insert(join(directory(file), path));
                }
            }
        }
        directories.into_iter().collect()
    }

    /// Says the module `uri` has the type `name`, which is the file `file`:
    /// what a `qmldir` says, or the build that makes the module.
    pub fn add_type(&mut self, uri: &str, name: &str, file: &str) {
        self.modules
            .entry(uri.to_string())
            .or_default()
            .insert(name.to_string(), file.to_string());
        let memberships = self.memberships.entry(file.to_string()).or_default();
        if !memberships.iter().any(|module| module == uri) {
            memberships.push(uri.to_string());
        }
    }

    /// Says the program keeps the file at `path` inside itself, and names it
    /// by `address`.
    pub fn add_resource(&mut self, address: &str, path: &str) {
        self.resources.insert(address.to_string(), path.to_string());
    }

    /// The files kept in the program that `address` names: the one it is
    /// the name of, or else, as it may be the start of a name put together
    /// when the program runs, every one whose name starts so.
    pub fn resources(&self, address: &str) -> Vec<(&String, &String)> {
        match self.resources.get_key_value(address) {
            Some(one) => vec![one],
            None => self
                .resources
                .range(address.to_string()..)
                .take_while(|(name, _)| name.starts_with(address))
                .collect(),
        }
    }

    /// Says `file` stands in for a type the program provides natively: what
    /// it is given is taken as such a type takes it.
    pub fn stand_in(&mut self, file: &str) {
        self.standins.insert(file.to_string());
    }

    pub fn stands_in(&self, file: &str) -> bool {
        self.standins.contains(file)
    }

    /// The modules `file` is a type of: it sees their other types as it sees
    /// the files next to it.
    pub fn modules_of(&self, file: &str) -> &[String] {
        self.memberships.get(file).map_or(&[], Vec::as_slice)
    }

    /// Chooses the style behind `import QtQuick.Controls`; `None` keeps the
    /// runtime's own.
    pub fn set_controls_style(&mut self, style: Option<ControlsStyle>) {
        self.controls_style = style;
    }

    pub fn controls_style(&self) -> Option<&ControlsStyle> {
        self.controls_style.as_ref()
    }

    /// Whether `uri` is a module of the project's own.
    pub fn has_module(&self, uri: &str) -> bool {
        self.modules.contains_key(uri)
    }

    /// The file the type `name` of the project's module `uri` is.
    pub fn module_type(&self, uri: &str, name: &str) -> Option<&str> {
        self.modules.get(uri)?.get(name).map(String::as_str)
    }

    /// Whether the file was added, and has a root object: also true of one
    /// kept in part.
    pub fn has(&self, file: &str) -> bool {
        self.files.contains_key(file)
    }

    pub fn summary(&self, file: &str) -> Option<&Summary> {
        self.files.get(file)
    }

    pub fn shape(&self, key: &Key) -> Option<&Shape> {
        self.files
            .get(&key.file)?
            .shapes
            .iter()
            .find(|shape| shape.inline == key.inline)
    }

    /// The names QML finds only when the program runs: an id of one
    /// component, or something its root declares, that another one's scripts
    /// use. An object is made in the context of whatever made it, and a name
    /// its own component does not have is looked for there.
    ///
    /// A name some type of the runtime's has is taken to be that, unless it
    /// is an id: nobody names an object for a property.
    pub fn dynamic_names(&self, oracle: &impl TypeOracle) -> HashSet<String> {
        let mut has: HashSet<&str> = HashSet::new();
        for shape in self.files.values().flat_map(|summary| &summary.shapes) {
            has.extend(shape.ids.iter().map(String::as_str));
            let declared = shape
                .properties
                .keys()
                .chain(shape.signals.keys())
                .chain(&shape.functions)
                .map(String::as_str);
            has.extend(declared.filter(|name| !oracle.is_member_name(name)));
        }
        self.files
            .values()
            .flat_map(|summary| &summary.shapes)
            .flat_map(|shape| {
                shape
                    .mentions
                    .iter()
                    .filter(|name| has.contains(name.as_str()))
            })
            .cloned()
            .collect()
    }

    /// The names the scripts of `file` use that nothing in it declares.
    pub fn mentions(&self, file: &str) -> impl Iterator<Item = &String> {
        self.files
            .get(file)
            .into_iter()
            .flat_map(|summary| &summary.shapes)
            .flat_map(|shape| &shape.mentions)
    }
}

/// What a parsed file says, as far as the other files care.
pub fn summarize(document: &QmlDocument<'_>) -> Summary {
    let mut summary = Summary {
        imports: document
            .imports
            .iter()
            .map(|import| Import {
                source: match &import.source {
                    QmlImportSource::Module(module) => Source::Module(module.to_string()),
                    QmlImportSource::Path(path) => Source::Path((*path).to_string()),
                },
                alias: import.alias.as_ref().map(|alias| alias.name.to_string()),
            })
            .collect(),
        is_singleton: document
            .pragmas
            .iter()
            .any(|pragma| pragma.name == "Singleton"),
        is_bound: document
            .pragmas
            .iter()
            .any(|pragma| pragma.name == "ComponentBehavior" && pragma.value == Some("Bound")),
        shapes: Vec::new(),
    };
    shape(&mut summary, None, &document.root);
    summary
}

fn shape(summary: &mut Summary, inline: Option<&str>, root: &QmlObject<'_>) {
    let mut names = Names::default();
    names.object(root);
    let mut shape = Shape {
        inline: inline.map(str::to_string),
        root: root
            .type_name
            .parts
            .iter()
            .map(|part| (*part).to_string())
            .collect(),
        properties: HashMap::new(),
        signals: HashMap::new(),
        functions: HashSet::new(),
        required: Vec::new(),
        has_default: false,
        default_component: None,
        enums: HashMap::new(),
        ids: HashSet::new(),
        mentions: HashSet::new(),
    };
    shape.mentions = names
        .mentions
        .into_iter()
        .filter(|name| !names.ids.contains(name) && !names.declared.contains(name))
        .collect();
    shape.ids = names.ids;
    let typed = names.typed;
    for member in &root.members {
        match member {
            QmlMember::Property(property) => {
                let name = property.name.name.to_string();
                if property.is_required {
                    shape.required.push(name.clone());
                }
                shape.has_default |= property.is_default;
                if property.is_default
                    && property.type_name.as_ref().is_some_and(|type_name| {
                        type_name.name.as_simple() == Some("Component") && !type_name.is_list
                    })
                {
                    shape.default_component = Some(name.clone());
                }
                if let Some(type_name) = &property.type_name {
                    let alias = match &property.value {
                        Some(QmlBindingValue::Expression(expression))
                            if type_name.name.as_simple() == Some("alias") =>
                        {
                            dotted(expression).and_then(|path| {
                                let (id, rest) = path.split_first()?;
                                Some((typed.get(id)?.clone(), rest.to_vec()))
                            })
                        }
                        _ => None,
                    };
                    shape.properties.insert(
                        name,
                        Declaration {
                            type_name: type_name.name.to_string(),
                            is_list: type_name.is_list,
                            is_readonly: property.is_readonly,
                            alias,
                        },
                    );
                }
            }
            QmlMember::Signal(signal) => {
                shape.signals.insert(
                    signal.name.name.to_string(),
                    signal
                        .params
                        .iter()
                        .map(|param| param.name.name.to_string())
                        .collect(),
                );
            }
            QmlMember::Function(function) => {
                if let Some(id) = &function.id {
                    shape.functions.insert(id.name.to_string());
                }
            }
            QmlMember::Enum(declaration) => {
                shape.enums.insert(
                    declaration.name.name.to_string(),
                    declaration
                        .members
                        .iter()
                        .map(|member| EnumKey {
                            name: member.name.name.to_string(),
                            value: member.value,
                        })
                        .collect(),
                );
            }
            _ => {}
        }
    }
    summary.shapes.push(shape);
    inline_shapes(summary, root);
}

/// `image.source`: the names of a path that is written with nothing else.
pub(crate) fn dotted(expression: &Expression<'_>) -> Option<Vec<String>> {
    match expression {
        Expression::Identifier(id) => Some(vec![id.name.to_string()]),
        Expression::StaticMemberExpression(member) => {
            let mut path = dotted(&member.object)?;
            path.push(member.property.name.to_string());
            Some(path)
        }
        _ => None,
    }
}

/// The ids of a component's objects and the names its scripts use. An inline
/// component is one of its own.
#[derive(Default)]
struct Names {
    ids: HashSet<String>,
    /// The type each id is the id of, as written.
    typed: HashMap<String, Vec<String>>,
    /// What any of its objects declares.
    declared: HashSet<String>,
    mentions: HashSet<String>,
}

impl Names {
    fn object(&mut self, object: &QmlObject<'_>) {
        for member in &object.members {
            match member {
                QmlMember::Property(property) => {
                    self.declared.insert(property.name.name.to_string());
                }
                QmlMember::Signal(signal) => {
                    self.declared.insert(signal.name.name.to_string());
                }
                QmlMember::Function(function) => {
                    self.declared
                        .extend(function.id.as_ref().map(|id| id.name.to_string()));
                }
                _ => {}
            }
            match member {
                QmlMember::Object(child) => self.object(child),
                QmlMember::Binding(QmlBinding {
                    name,
                    value: QmlBindingValue::Expression(Expression::Identifier(id)),
                    ..
                }) if name.as_simple() == Some("id") => {
                    self.ids.insert(id.name.to_string());
                    self.typed.insert(
                        id.name.to_string(),
                        object
                            .type_name
                            .parts
                            .iter()
                            .map(|part| (*part).to_string())
                            .collect(),
                    );
                }
                QmlMember::Binding(QmlBinding { value, .. })
                | QmlMember::Property(QmlPropertyDeclaration {
                    value: Some(value), ..
                }) => match value {
                    QmlBindingValue::Expression(expression) => self.visit_expression(expression),
                    QmlBindingValue::Statement(statement) => self.visit_statement(statement),
                    QmlBindingValue::Object(child) => self.object(child),
                    QmlBindingValue::Objects(children) => {
                        for child in children {
                            self.object(child);
                        }
                    }
                },
                QmlMember::Function(function) => {
                    if let Some(body) = &function.body {
                        self.visit_function_body(body);
                    }
                }
                _ => {}
            }
        }
    }
}

impl<'a> Visit<'a> for Names {
    fn visit_identifier_reference(&mut self, identifier: &IdentifierReference<'a>) {
        if identifier
            .name
            .starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
        {
            self.mentions.insert(identifier.name.to_string());
        }
    }
}

/// Inline components may be declared anywhere in the file.
fn inline_shapes(summary: &mut Summary, object: &QmlObject<'_>) {
    for member in &object.members {
        match member {
            QmlMember::Object(child) => inline_shapes(summary, child),
            QmlMember::Binding(QmlBinding { value, .. })
            | QmlMember::Property(QmlPropertyDeclaration {
                value: Some(value), ..
            }) => match value {
                QmlBindingValue::Object(child) => inline_shapes(summary, child),
                QmlBindingValue::Objects(children) => {
                    for child in children {
                        inline_shapes(summary, child);
                    }
                }
                _ => {}
            },
            QmlMember::InlineComponent(inline) => {
                shape(summary, Some(inline.name.name.as_str()), &inline.object);
            }
            _ => {}
        }
    }
}

/// `import "lib.js"`: a script, not a directory of types.
pub(crate) fn is_script(path: &str) -> bool {
    path.ends_with(".js") || path.ends_with(".mjs")
}

/// The directory of a file's key: `content` of `content/Clock`.
pub fn directory(file: &str) -> &str {
    file.rsplit_once('/').map_or("", |(directory, _)| directory)
}

/// `path` from `directory`, without `.` and with `..` taken up where it can be.
pub fn join(directory: &str, path: &str) -> String {
    let absolute = path.starts_with('/') || directory.starts_with('/');
    let mut parts: Vec<&str> = if path.starts_with('/') {
        Vec::new()
    } else {
        directory
            .split('/')
            .filter(|part| !part.is_empty())
            .collect()
    };
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." if parts.last().is_some_and(|last| *last != "..") => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    let joined = parts.join("/");
    if absolute {
        format!("/{joined}")
    } else {
        joined
    }
}

/// The path to reach `target` by from the directory of `file`, both keys.
pub fn relative(file: &str, target: &str) -> String {
    if target.starts_with('/') {
        return target.to_string();
    }
    let from: Vec<&str> = directory(file)
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let to: Vec<&str> = target.split('/').collect();
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<&str> = vec![".."; from.len() - common];
    parts.extend(&to[common..]);
    let path = parts.join("/");
    if path.starts_with("../") {
        path
    } else {
        format!("./{path}")
    }
}
