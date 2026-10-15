//! JavaScript files a QML file imports by path (`import "calc.js" as Calc`).
//!
//! The compiler hands over each script as an expression that makes its
//! object. A script without `.pragma library` is evaluated once per
//! instance of the importing component, as Qt does, so what it keeps is
//! that instance's own; a library is evaluated once for the engine.

use std::cell::RefCell;
use std::collections::HashMap;

use ljos_ir::Script;
use ljos_js::{JsRef, JsValue, Scope};

use super::Instance;
use crate::engine::Engine;
use crate::ids::ContextId;
use crate::log::LogLevel;

/// Each script's object: by the importing context (none for a library)
/// and the script's file, with the text it was made from. A context's
/// entries go with it ([`Engine::release_scripts`]), so a delegate that
/// imports a script leaves nothing behind.
type Made = HashMap<(Option<ContextId>, String), (String, JsRef)>;

#[derive(Default)]
pub(crate) struct ScriptObjects {
    made: RefCell<Made>,
}

impl Engine {
    /// How many script objects the engine holds. For tests that watch
    /// for a leak.
    pub fn script_objects_held(&self) -> usize {
        self.extension::<ScriptObjects>().made.borrow().len()
    }

    /// Drops what the scripts of `ctx` made, so QuickJS can collect it.
    pub(crate) fn release_scripts(&self, ctx: ContextId) {
        let all = self.extension::<ScriptObjects>();
        let gone: Vec<_> = {
            let mut made = all.made.borrow_mut();
            let keys: Vec<_> = made
                .keys()
                .filter(|(o, _)| *o == Some(ctx))
                .cloned()
                .collect();
            keys.into_iter().filter_map(|k| made.remove(&k)).collect()
        };
        // Dropped unborrowed: a handle may run a finalizer.
        drop(gone);
    }

    /// The object `import "path" as Name` is in `inst`'s file.
    pub(crate) fn script_object(&self, s: &Scope<'_>, inst: &Instance, path: &str) -> JsValue {
        let Some(script) = inst.file.ir.scripts.iter().find(|sc| sc.path == path) else {
            self.log(
                LogLevel::Warning,
                "qml",
                format!("{}: the script \"{path}\" is not available", inst.file.url),
            );
            return JsValue::Undefined;
        };
        // The delegates and templates of a component share its scripts.
        let mut root = inst;
        while let Some(outer) = root.outer.as_deref() {
            root = outer;
        }
        let owner = (!script.library).then_some(root.context);
        let all = self.extension::<ScriptObjects>();
        let key = (owner, script.key.clone());
        if let Some((text, held)) = all.made.borrow().get(&key)
            && *text == script.source
        {
            return JsValue::Handle(held.clone());
        }
        // Beside the file that imports it, as its own `path` says.
        let url = crate::loader::join_url(&inst.file.url, &script.path);
        match self.evaluate_script(s, script, &url) {
            Some(held) => {
                all.made
                    .borrow_mut()
                    .insert(key, (script.source.clone(), held.clone()));
                JsValue::Handle(held)
            }
            None => JsValue::Undefined,
        }
    }

    fn evaluate_script(&self, s: &Scope<'_>, script: &Script, url: &str) -> Option<JsRef> {
        // The source is a function of the script's own address.
        let made: Result<ljos_js::rquickjs::Value, _> = s
            .ctx()
            .eval::<ljos_js::rquickjs::Function, _>(script.source.as_bytes())
            .and_then(|f| f.call((url,)));
        match made {
            Ok(object) => Some(s.keep(object)),
            Err(error) => {
                // What the script threw, as a person would want to read it.
                let detail = match s.ctx().catch().into_exception() {
                    Some(exception) => exception.to_string(),
                    None => error.to_string(),
                };
                self.log(
                    LogLevel::Warning,
                    "qml",
                    format!("{}: {detail}", script.key),
                );
                None
            }
        }
    }
}
