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

use crate::engine::Engine;
use crate::error::EngineError;
use crate::loader::{ComponentSource, LoadedFile};
use crate::registry::Version;

/// What the engine keeps for the QML-defined types of modules.
#[derive(Default)]
pub(crate) struct QmlTypes {
    sources: Vec<Rc<dyn ComponentSource>>,
    files: HashMap<String, Rc<LoadedFile>>,
    /// Module URI, then type name, to the key of the file that defines it.
    types: HashMap<String, HashMap<String, String>>,
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
    /// `…/Button` is `Button`. The modules the file imports have to be
    /// registered first.
    pub fn register_qml_type(
        &self,
        uri: &str,
        version: Version,
        key: &str,
    ) -> Result<(), EngineError> {
        let file = self
            .library_file(key)?
            .ok_or_else(|| EngineError::Load(format!("{key}: no component library has it")))?;
        let comp = file
            .find_component(None)
            .ok_or_else(|| EngineError::Load(format!("{}: no component", file.url)))?;
        let meta = self.object_meta(&file, comp, 0)?;
        let name = meta.name.to_string();
        self.register_type(uri, version, meta);
        self.inner
            .loader
            .borrow_mut()
            .qml_types
            .types
            .entry(uri.to_owned())
            .or_default()
            .insert(name, key.to_owned());
        Ok(())
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
