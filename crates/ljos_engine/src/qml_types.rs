//! Module types written in QML: a library crate compiles its QML files to
//! IR when it is built, and registers them here as types of a module
//! (`QtQuick.Controls`' `Button` is the file `Button.qml` of the style).
//!
//! An application file that uses such a type names it as a native type of
//! the module, which is all the compiler knows; the object creator then
//! instantiates the library's component on that object, exactly as it does
//! for a component file of the application itself.
//!
//! Library files are kept apart from the application's: setting another
//! component source (a hot reload) drops the application's files, and the
//! registered types have to stay the ones the library's files describe.

use rustc_hash::FxHashMap as HashMap;
use std::rc::Rc;

use ljos_ir::ImportSource;

use crate::atom::Atom;
use crate::engine::Engine;
use crate::error::EngineError;
use crate::loader::{ComponentSource, LoadedFile};
use crate::meta::MetaRef;
use crate::registry::Slot;
use crate::registry::Version;

/// What the engine keeps for the QML-defined types of modules.
#[derive(Default)]
pub(crate) struct QmlTypes {
    sources: Vec<Rc<dyn ComponentSource>>,
    files: HashMap<String, Rc<LoadedFile>>,
    /// Module URI, then type name, to the key of the file that defines it.
    types: HashMap<String, HashMap<String, String>>,
    /// The files being described. One that names its own type while it is
    /// being described does not find it, as it did not when every type was
    /// described as it was registered.
    describing: Vec<String>,
    /// The library file each described type is written in, by the address
    /// of its description.
    keys: HashMap<usize, Atom>,
}

impl Engine {
    /// Adds compiled files that module types are made of
    /// ([`Engine::register_qml_type`]). A library keeps its keys apart from
    /// an application's by a prefix (`QtQuick/Controls/Basic/Button`).
    pub fn add_component_library(&self, source: impl ComponentSource + 'static) {
        self.inner
            .loader
            .borrow_mut()
            .qml_types
            .sources
            .push(Rc::new(source));
    }

    /// Registers the file `key` of a component library as a type of module
    /// `uri`. The type is named by the file, as QML names a component:
    /// `…/Button` is `Button`. The file is read, and the type described,
    /// when the type is first looked up; the modules the file imports have
    /// to be registered by then. A file that does not load then is warned
    /// about and its type taken away.
    pub fn register_qml_type(
        &self,
        uri: &str,
        version: Version,
        key: &str,
    ) -> Result<(), EngineError> {
        let known = {
            let l = self.inner.loader.borrow();
            l.qml_types.sources.iter().any(|s| s.contains(key))
        };
        if !known {
            return Err(EngineError::Load(format!(
                "{key}: no component library has it"
            )));
        }
        let name = key.rsplit_once('/').map_or(key, |(_, n)| n);
        self.inner
            .registry
            .borrow_mut()
            .register_qml(uri, version, name, key);
        self.inner
            .loader
            .borrow_mut()
            .qml_types
            .types
            .entry(uri.to_owned())
            .or_default()
            .insert(name.to_owned(), key.to_owned());
        Ok(())
    }

    /// The description of a registered type, made now for a QML type that
    /// has none yet.
    pub(crate) fn described(&self, slot: Slot) -> Option<MetaRef> {
        match slot {
            Slot::Meta(meta) => Some(meta),
            Slot::Qml(key) => self.describe_qml_type(&key),
        }
    }

    /// Describes the types written in the library file `key`.
    pub(crate) fn describe_qml_type(&self, key: &str) -> Option<MetaRef> {
        {
            let mut l = self.inner.loader.borrow_mut();
            if l.qml_types.describing.iter().any(|k| k == key) {
                return None;
            }
            l.qml_types.describing.push(key.to_owned());
        }
        let meta = self.qml_type_meta(key);
        self.inner
            .loader
            .borrow_mut()
            .qml_types
            .describing
            .retain(|k| k != key);
        if let Err(err) = &meta {
            let uris: Vec<String> = {
                let l = self.inner.loader.borrow();
                l.qml_types
                    .types
                    .iter()
                    .filter(|(_, names)| names.values().any(|k| k == key))
                    .map(|(uri, _)| uri.clone())
                    .collect()
            };
            let name = key.rsplit_once('/').map_or(key, |(_, n)| n);
            for uri in &uris {
                self.warn(format!("{uri}.{name}: {err}"));
            }
            let mut l = self.inner.loader.borrow_mut();
            for names in l.qml_types.types.values_mut() {
                names.retain(|_, k| k != key);
            }
        }
        let meta = meta.ok();
        if let Some(m) = &meta {
            self.inner
                .loader
                .borrow_mut()
                .qml_types
                .keys
                .insert(meta_address(m), Atom::intern(key));
        }
        self.inner
            .registry
            .borrow_mut()
            .describe(key, meta.as_ref());
        meta
    }

    /// The key of the library file the type `meta` is written in, for a
    /// type registered with [`Engine::register_qml_type`].
    pub fn library_key_of(&self, meta: &MetaRef) -> Option<Atom> {
        let l = self.inner.loader.borrow();
        l.qml_types.keys.get(&meta_address(meta)).cloned()
    }

    fn qml_type_meta(&self, key: &str) -> Result<MetaRef, EngineError> {
        let Some(file) = self.library_file(key)? else {
            let failure = {
                let l = self.inner.loader.borrow();
                l.qml_types.sources.iter().find_map(|s| s.failure(key))
            };
            return Err(EngineError::Load(match failure {
                Some(why) => format!("{key}: {why}"),
                None => format!("{key}: no component library has it"),
            }));
        };
        let comp = file
            .find_component(None)
            .ok_or_else(|| EngineError::Load(format!("{}: no component", file.url)))?;
        self.object_meta(&file, comp, 0)
    }

    /// The library file behind the type `name` of `module`, if that type is
    /// written in QML.
    pub(crate) fn qml_type_key(&self, module: &str, name: &str) -> Option<String> {
        let l = self.inner.loader.borrow();
        let types = &l.qml_types.types;
        if types.is_empty() {
            return None;
        }
        types.get(module)?.get(name).cloned()
    }

    /// As [`Engine::qml_type_key`] for a type the compiler could not place
    /// in a module.
    pub(crate) fn qml_type_key_any(&self, name: &str) -> Option<String> {
        let l = self.inner.loader.borrow();
        l.qml_types
            .types
            .values()
            .find_map(|m| m.get(name).cloned())
    }

    /// The library file `key`, loaded on first use; `None` when no library
    /// has it.
    pub(crate) fn library_file(&self, key: &str) -> Result<Option<Rc<LoadedFile>>, EngineError> {
        let sources = {
            let l = self.inner.loader.borrow();
            if let Some(f) = l.qml_types.files.get(key) {
                return Ok(Some(f.clone()));
            }
            l.qml_types.sources.clone()
        };
        let Some((source, ir)) = sources.iter().find_map(|s| Some((s, s.load(key)?))) else {
            return Ok(None);
        };
        let url = source.url(key);
        if ir.version != ljos_ir::IR_VERSION {
            return Err(EngineError::Load(format!(
                "{url}: compiled for IR version {}, this engine reads {}",
                ir.version,
                ljos_ir::IR_VERSION
            )));
        }
        for import in &ir.imports {
            if let ImportSource::Module(uri) = &import.source
                && !import.in_project
                && !self.has_module(uri)
            {
                return Err(EngineError::Load(format!(
                    "{url}: module \"{uri}\" is not installed"
                )));
            }
        }
        let file = Rc::new(LoadedFile::new(key, url, ir, source.bytecode(key)));
        self.inner
            .loader
            .borrow_mut()
            .qml_types
            .files
            .insert(key.to_owned(), file.clone());
        Ok(Some(file))
    }
}

fn meta_address(meta: &MetaRef) -> usize {
    std::ptr::from_ref(&**meta) as usize
}
