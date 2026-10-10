//! What a type name is in a file, and what an object of that type has.
//!
//! A name is looked up the way QML does: the file's inline components, then
//! its imports from the last to the first, then the files next to it, then the
//! modules the file is a type of. What the runtime's own types have comes
//! from the [`TypeOracle`]; what a component has, from the file that declares
//! it.

use crate::{
    oracle::{Property, TypeOracle},
    project::{Key, Project, Shape, Source, directory, is_script, join},
};

/// What a type name stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind<T> {
    /// A type the runtime provides.
    Builtin(T),
    /// A QML file, or an inline component of one.
    Component(Key),
}

/// Where a type comes from, which is where a back end imports it from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// An export of a module, by the URI the file imports.
    Module(String),
    /// A QML file, by its key in the project.
    File(String),
    /// An inline component of the file itself.
    Inline,
    /// A member of what the file imports `as` something.
    Namespace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found<T> {
    pub kind: Kind<T>,
    pub origin: Origin,
}

/// What a name is on an object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Member<T> {
    Property(Property<T>),
    Signal,
    Method,
}

/// How many components one may be built on before it is taken to be built on
/// itself, by whatever detour.
const DEPTH: usize = 64;
/// How many aliases may lead to one another.
const ALIASES: usize = 16;

/// The types of one file.
#[derive(Debug)]
pub struct Types<'p, O> {
    pub project: &'p Project,
    pub oracle: &'p O,
    /// The file's key in the project.
    pub file: &'p str,
}

impl<O> Clone for Types<'_, O> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<O> Copy for Types<'_, O> {}

impl<'p, O: TypeOracle> Types<'p, O> {
    pub fn new(project: &'p Project, oracle: &'p O, file: &'p str) -> Self {
        Self {
            project,
            oracle,
            file,
        }
    }

    fn of(&self, file: &'p str) -> Self {
        Self { file, ..*self }
    }

    /// What the dotted name `parts` is as a type: `Item`, `C.Button`.
    pub fn find(&self, parts: &[&str]) -> Option<Found<O::Type>> {
        let summary = self.project.summary(self.file)?;
        let (qualifier, name) = match parts {
            [name] => (None, *name),
            [qualifier, name] => (Some(*qualifier), *name),
            _ => return None,
        };
        if qualifier.is_none() {
            let key = Key {
                file: self.file.to_string(),
                inline: Some(name.to_string()),
            };
            if self.project.shape(&key).is_some() {
                return Some(Found {
                    kind: Kind::Component(key),
                    origin: Origin::Inline,
                });
            }
        }
        for import in summary.imports.iter().rev() {
            if import.alias.as_deref() != qualifier {
                continue;
            }
            match &import.source {
                Source::Module(uri) => {
                    // A module of the project's own. Its types are files, and
                    // a file has one name: `M.Type` is not reached yet.
                    if qualifier.is_none()
                        && let Some(file) = self.project.module_type(uri, name)
                    {
                        return Some(file_found(file));
                    }
                    if let Some(ty) = self.oracle.module_type(uri, name) {
                        let origin = match qualifier {
                            Some(_) => Origin::Namespace,
                            None => Origin::Module(uri.clone()),
                        };
                        return Some(Found {
                            kind: Kind::Builtin(ty),
                            origin,
                        });
                    }
                }
                Source::Path(path) if !is_script(path) => {
                    if let Some(file) = self.file_named(&join(directory(self.file), path), name) {
                        let origin = match qualifier {
                            Some(_) => Origin::Namespace,
                            None => Origin::File(file.clone()),
                        };
                        return Some(Found {
                            kind: Kind::Component(Key { file, inline: None }),
                            origin,
                        });
                    }
                }
                Source::Path(_) => {}
            }
        }
        if qualifier.is_some() {
            return None;
        }
        // The file's own directory is imported before anything else, so
        // everything else comes first.
        if let Some(file) = self.file_named(directory(self.file), name) {
            return Some(file_found(&file));
        }
        // So is the module the file is a type of, wherever its files are.
        for uri in self.project.modules_of(self.file) {
            if let Some(file) = self.project.module_type(uri, name) {
                return Some(file_found(file));
            }
        }
        // A module written in QML has types that are not: a style of Qt
        // Quick Controls names `Overlay` and imports nothing for it.
        for uri in self.project.modules_of(self.file) {
            if let Some(ty) = self.oracle.module_type(uri, name) {
                return Some(Found {
                    kind: Kind::Builtin(ty),
                    origin: Origin::Module(uri.clone()),
                });
            }
        }
        None
    }

    /// The file in `directory` that is the type `name`: `Name.qml`, or the
    /// `Name.ui.qml` Qt Design Studio writes.
    fn file_named(&self, directory: &str, name: &str) -> Option<String> {
        let file = join(directory, name);
        if self.project.has(&file) {
            return Some(file);
        }
        let file = format!("{file}.ui");
        self.project.has(&file).then_some(file)
    }

    /// The type is one object, there before it is named.
    pub fn is_singleton(&self, kind: &Kind<O::Type>) -> bool {
        match kind {
            Kind::Builtin(ty) => self.oracle.is_singleton(*ty),
            Kind::Component(Key { file, inline: None }) => self
                .project
                .summary(file)
                .is_some_and(|summary| summary.is_singleton),
            Kind::Component(_) => false,
        }
    }

    /// What `member` is to the enums a component declares: `Some(true)` an
    /// enum (`Type.Theme`, of `Type.Theme.Dark`), `Some(false)` a key. The
    /// enums of what its root is are its own too.
    pub fn declared_enum(&self, kind: &Kind<O::Type>, member: &str) -> Option<bool> {
        let mut kind = kind.clone();
        for _ in 0..DEPTH {
            let key = match kind {
                Kind::Builtin(ty) => {
                    if self.oracle.has_enum_key(ty, None, member) {
                        return Some(false);
                    }
                    return self.oracle.has_enum(ty, member).then_some(true);
                }
                Kind::Component(key) => key,
            };
            let shape = self.project.shape(&key)?;
            if shape
                .enums
                .values()
                .any(|keys| keys.iter().any(|key| key == member))
            {
                return Some(false);
            }
            if shape.enums.contains_key(member) {
                return Some(true);
            }
            kind = self.root(&key, shape)?;
        }
        None
    }

    /// Whether `Type.key`, or `Type.scope.key`, is a key of an enum of the
    /// type: one a component in its chain declares, or one of the builtin
    /// type the chain ends in.
    pub fn is_key(&self, kind: &Kind<O::Type>, scope: Option<&str>, key: &str) -> bool {
        let mut kind = kind.clone();
        for _ in 0..DEPTH {
            let file = match kind {
                Kind::Builtin(ty) => return self.oracle.has_enum_key(ty, scope, key),
                Kind::Component(file) => file,
            };
            let Some(shape) = self.project.shape(&file) else {
                return false;
            };
            if shape.enums.iter().any(|(name, keys)| {
                scope.is_none_or(|scope| scope == name) && keys.iter().any(|known| known == key)
            }) {
                return true;
            }
            let Some(root) = self.root(&file, shape) else {
                return false;
            };
            kind = root;
        }
        false
    }

    /// What the file imports `as` `name`, if anything.
    pub fn namespace(&self, name: &str) -> Option<&'p Source> {
        let summary = self.project.summary(self.file)?;
        summary
            .imports
            .iter()
            .rev()
            .find(|import| import.alias.as_deref() == Some(name))
            .map(|import| &import.source)
    }

    /// The builtin type a component's objects are, however many components
    /// down.
    pub fn base(&self, kind: &Kind<O::Type>) -> Option<O::Type> {
        let mut kind = kind.clone();
        for _ in 0..DEPTH {
            match kind {
                Kind::Builtin(ty) => return Some(ty),
                Kind::Component(key) => {
                    let shape = self.project.shape(&key)?;
                    kind = self.root(&key, shape)?;
                }
            }
        }
        None
    }

    /// What the root object of a component is, in the file it is written in.
    fn root(&self, key: &Key, shape: &Shape) -> Option<Kind<O::Type>> {
        let parts: Vec<&str> = shape.root.iter().map(String::as_str).collect();
        self.of(&key.file).find(&parts).map(|found| found.kind)
    }

    /// What `name` is on an object of the type: something a component in its
    /// chain declares, or a member of the builtin type the chain ends in.
    pub fn member(&self, kind: &Kind<O::Type>, name: &str) -> Option<Member<O::Type>> {
        self.member_through(kind, name, 0)
    }

    /// `aliases` is how many aliases led here: one may be of another, and
    /// what is written wrong may be of itself.
    fn member_through(
        &self,
        kind: &Kind<O::Type>,
        name: &str,
        aliases: usize,
    ) -> Option<Member<O::Type>> {
        let mut kind = kind.clone();
        for _ in 0..DEPTH {
            match kind {
                Kind::Builtin(ty) => return builtin_member(self.oracle, ty, name),
                Kind::Component(key) => {
                    let shape = self.project.shape(&key)?;
                    if let Some(member) = shape_member(shape, name) {
                        // An alias is the property it is an alias of: a path
                        // given to `property alias source: image.source` is
                        // a path.
                        let aliased = shape.properties.get(name).and_then(|declaration| {
                            let (of, path) = declaration.alias.as_ref()?;
                            let (first, rest) = path.split_first()?;
                            if aliases == ALIASES {
                                return None;
                            }
                            let types = self.of(&key.file);
                            let parts: Vec<&str> = of.iter().map(String::as_str).collect();
                            let Member::Property(mut property) = types.member_through(
                                &types.find(&parts)?.kind,
                                first,
                                aliases + 1,
                            )?
                            else {
                                return None;
                            };
                            for name in rest {
                                property = self.oracle.property(property.value?, name)?;
                            }
                            Some(Member::Property(property))
                        });
                        return Some(aliased.unwrap_or(member));
                    }
                    kind = self.root(&key, shape)?;
                }
            }
        }
        None
    }

    /// The file a path given to the property `name` of an object of the type
    /// is taken from, where that is not the file the object is written in:
    /// the one a component declares the property in, which is where it uses
    /// it, and for an alias the one that has the object it is an alias of.
    /// A builtin type takes it from the file that makes the object, and so
    /// does what stands in for one.
    pub fn home(&self, kind: &Kind<O::Type>, name: &str) -> Option<String> {
        self.home_through(kind, name, 0)
    }

    fn home_through(&self, kind: &Kind<O::Type>, name: &str, aliases: usize) -> Option<String> {
        let mut kind = kind.clone();
        for _ in 0..DEPTH {
            let Kind::Component(key) = kind else {
                return None;
            };
            let shape = self.project.shape(&key)?;
            if let Some(declaration) = shape.properties.get(name) {
                let Some((of, path)) = &declaration.alias else {
                    return (!self.project.stands_in(&key.file)).then_some(key.file);
                };
                let first = path.first()?;
                if aliases == ALIASES {
                    return None;
                }
                let types = self.of(&key.file);
                let parts: Vec<&str> = of.iter().map(String::as_str).collect();
                let target = types.find(&parts)?.kind;
                return types
                    .home_through(&target, first, aliases + 1)
                    .or(Some(key.file));
            }
            kind = self.root(&key, shape)?;
        }
        None
    }

    /// The names a handler of the signal `name` may call its arguments. A
    /// type may have a property of the same name: `pressed` of a MouseArea.
    pub fn signal(&self, kind: &Kind<O::Type>, name: &str) -> Option<Vec<String>> {
        let mut kind = kind.clone();
        for _ in 0..DEPTH {
            match kind {
                Kind::Builtin(ty) => return self.oracle.signal(ty, name),
                Kind::Component(key) => {
                    let shape = self.project.shape(&key)?;
                    if let Some(parameters) = shape.signals.get(name) {
                        return Some(parameters.clone());
                    }
                    if let Some(property) = name.strip_suffix("Changed")
                        && shape.properties.contains_key(property)
                    {
                        return Some(Vec::new());
                    }
                    kind = self.root(&key, shape)?;
                }
            }
        }
        None
    }

    /// The property whose new value the signal `name` carries to its
    /// handlers, when it is a builtin type's: `text`, of the
    /// `textChanged(text)` of a Text. Most carry nothing, and the change of a
    /// property a component declares never does.
    pub fn carried<'n>(&self, kind: &Kind<O::Type>, name: &'n str) -> Option<&'n str> {
        let property = name.strip_suffix("Changed")?;
        let mut kind = kind.clone();
        for _ in 0..DEPTH {
            match kind {
                Kind::Builtin(ty) => {
                    let parameters = self.oracle.signal(ty, name)?;
                    let [parameter] = parameters.as_slice() else {
                        return None;
                    };
                    // Not the value, by the name it is given: by how much a
                    // handler moved (`scaleChanged(delta)`), the event
                    // (`mouseXChanged(mouse)`).
                    if matches!(parameter.as_str(), "delta" | "mouse") {
                        return None;
                    }
                    return self.oracle.property(ty, property).map(|_| property);
                }
                Kind::Component(key) => {
                    let shape = self.project.shape(&key)?;
                    if shape.signals.contains_key(name) || shape.properties.contains_key(property) {
                        return None;
                    }
                    kind = self.root(&key, shape)?;
                }
            }
        }
        None
    }

    /// The properties whoever makes an object of the type has to set: what
    /// the components in its chain require, and what the builtin type the
    /// chain ends in does.
    pub fn required(&self, kind: &Kind<O::Type>) -> Vec<String> {
        let mut required: Vec<String> = Vec::new();
        let mut add = |name: &str| {
            if !required.iter().any(|known| known == name) {
                required.push(name.to_string());
            }
        };
        let mut kind = kind.clone();
        for _ in 0..DEPTH {
            match kind {
                Kind::Builtin(ty) => {
                    self.oracle.required(ty).iter().for_each(|name| add(name));
                    break;
                }
                Kind::Component(key) => {
                    let Some(shape) = self.project.shape(&key) else {
                        break;
                    };
                    shape.required.iter().for_each(|name| add(name));
                    let Some(root) = self.root(&key, shape) else {
                        break;
                    };
                    kind = root;
                }
            }
        }
        required
    }

    /// The property objects written inside one of the type go to, when it is
    /// a template: a Repeater's `delegate`.
    pub fn default_component(&self, kind: &Kind<O::Type>) -> Option<String> {
        let mut kind = kind.clone();
        for _ in 0..DEPTH {
            match kind {
                Kind::Builtin(ty) => {
                    let name = self.oracle.default_property(ty)?;
                    let is_component = self
                        .oracle
                        .property(ty, name)
                        .is_some_and(|property| property.is_component);
                    return is_component.then(|| name.to_string());
                }
                Kind::Component(key) => {
                    let shape = self.project.shape(&key)?;
                    if shape.has_default {
                        return None;
                    }
                    kind = self.root(&key, shape)?;
                }
            }
        }
        None
    }
}

fn file_found<T>(file: &str) -> Found<T> {
    Found {
        kind: Kind::Component(Key {
            file: file.to_string(),
            inline: None,
        }),
        origin: Origin::File(file.to_string()),
    }
}

/// What `name` is on a builtin type.
pub fn builtin_member<O: TypeOracle>(
    oracle: &O,
    ty: O::Type,
    name: &str,
) -> Option<Member<O::Type>> {
    if let Some(property) = oracle.property(ty, name) {
        return Some(Member::Property(property));
    }
    if oracle.signal(ty, name).is_some() {
        return Some(Member::Signal);
    }
    oracle.has_method(ty, name).then_some(Member::Method)
}

fn shape_member<T>(shape: &Shape, name: &str) -> Option<Member<T>> {
    if let Some(declaration) = shape.properties.get(name) {
        return Some(Member::Property(Property::declared(
            &declaration.type_name,
            declaration.is_list,
            declaration.is_readonly,
        )));
    }
    if shape.signals.contains_key(name) {
        return Some(Member::Signal);
    }
    if shape.functions.contains(name) {
        return Some(Member::Method);
    }
    // A property comes with the signal that says it changed.
    let property = name.strip_suffix("Changed")?;
    shape
        .properties
        .contains_key(property)
        .then_some(Member::Signal)
}
