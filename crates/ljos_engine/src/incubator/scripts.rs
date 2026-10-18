//! JavaScript files a QML file imports by path (`import "calc.js" as Calc`).
//!
//! The compiler hands over each script as an expression that makes its
//! object. A script without `.pragma library` is evaluated once per
//! instance of the importing component, as Qt does, so what it keeps is
//! that instance's own; a library is evaluated once for the engine.
//!
//! Across a reload a script keeps what it holds: an instance kept by the
//! reload takes the old instance's object, and a script whose text
//! changed is made again with each top-level variable carried over unless
//! the value the file starts it with changed. The objects that set a
//! script up in `Component.onCompleted` are kept and do not run it again.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use ljos_ir::Script;
use ljos_js::{JsRef, JsValue, Scope};

use super::Instance;
use crate::engine::Engine;
use crate::ids::ContextId;
use crate::log::LogLevel;

/// Each script's object: by the importing context (none for a library)
/// and the script's file. A context's entries go with it
/// ([`Engine::release_scripts`]), so a delegate that imports a script
/// leaves nothing behind.
type Made = HashMap<(Option<ContextId>, String), Held>;

/// The value each top-level variable of a script started with, as JSON,
/// by name; functions are left out, as they are always the new file's.
type Starts = HashMap<String, String>;

#[derive(Clone)]
struct Held {
    /// The text the object was made from.
    text: String,
    object: JsRef,
    /// Taken only while reloads are tracked, for a reload to tell a
    /// variable whose declaration was edited from one the script set.
    starts: Option<Rc<Starts>>,
}

/// The top-level variables of a script object and what they hold, as
/// `name\tJSON` lines. What JSON cannot say is told by its type, so two
/// values JSON cannot tell apart count as the same start.
const STARTS: &str = r#"(o) => Object.keys(o).flatMap((k) => {
    const v = o[k];
    if (typeof v === "function") return [];
    let j;
    try { j = JSON.stringify(v); } catch (e) { j = undefined; }
    return [k + "\t" + (j === undefined ? "?" + typeof v : j)];
}).join("\n")"#;

/// Gives `to` what `from` holds in the variables named, one per line.
const CARRY: &str = r#"(from, to, names) => {
    for (const k of names.split("\n")) if (k) to[k] = from[k];
}"#;

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
        // Beside the file that imports it, as its own `path` says. Only a
        // script made now needs it: this runs at every call that reads it.
        let url = || crate::loader::join_url(&inst.file.url, &script.path);
        match self.made_script(s, &inst.file.ir.scripts, script, &url, owner) {
            Some(held) => JsValue::Handle(held),
            None => JsValue::Undefined,
        }
    }

    /// The object of `script` for `owner`, made now unless it was made
    /// from the same text before.
    fn made_script(
        &self,
        s: &Scope<'_>,
        all: &[Script],
        script: &Script,
        url: &dyn Fn() -> String,
        owner: Option<ContextId>,
    ) -> Option<JsRef> {
        let objects = self.extension::<ScriptObjects>();
        let key = (owner, script.key.clone());
        let old = match objects.made.borrow().get(&key) {
            Some(h) if h.text == script.source => return Some(h.object.clone()),
            Some(h) => Some(h.clone()),
            None => None,
        };
        let object = self.evaluate_script(s, all, script, &url(), &mut Vec::new())?;
        let starts = self
            .reload_tracking()
            .then(|| Rc::new(script_starts(s, &object)));
        let held = Held {
            text: script.source.clone(),
            object: object.clone(),
            starts,
        };
        if let Some(old) = &old {
            carry(s, old, &held);
        }
        objects.made.borrow_mut().insert(key, held);
        Some(object)
    }

    /// The scripts of `old`, an instance a reload replaces with `new`, go
    /// on in `new`: as they are, or into what `new` has made of them.
    pub(crate) fn scripts_go_on(&self, old: ContextId, new: ContextId) {
        let objects = self.extension::<ScriptObjects>();
        let mut into: Vec<(Held, Held)> = Vec::new();
        {
            let mut made = objects.made.borrow_mut();
            let theirs: Vec<(String, Held)> = made
                .iter()
                .filter(|((o, _), _)| *o == Some(old))
                .map(|((_, k), h)| (k.clone(), h.clone()))
                .collect();
            for (k, h) in theirs {
                match made.get(&(Some(new), k.clone())) {
                    Some(ours) => into.push((h, ours.clone())),
                    None => {
                        made.insert((Some(new), k), h);
                    }
                }
            }
        }
        if into.is_empty() {
            return;
        }
        let Ok(js) = self.js() else { return };
        let _ = js
            .host
            .enter("reload scripts", js.host.config().slice, |s| {
                for (old, new) in &into {
                    carry(s, old, new);
                }
                Ok(())
            });
    }

    /// Makes `script` and what it imports: a library is the one the engine
    /// has, any other script a copy of the importing script's own (as qml6
    /// does). `chain` is what is being made, which the compiler keeps from
    /// going round; a project changed under it gets a warning instead.
    fn evaluate_script(
        &self,
        s: &Scope<'_>,
        all: &[Script],
        script: &Script,
        url: &str,
        chain: &mut Vec<String>,
    ) -> Option<JsRef> {
        let imports = ljos_js::rquickjs::Array::new(s.ctx().clone()).ok()?;
        chain.push(script.key.clone());
        for (index, import) in script.imports.iter().enumerate() {
            let at = crate::loader::join_url(url, &import.path);
            let made = match all.iter().find(|sc| sc.key == import.key) {
                _ if chain.contains(&import.key) => None,
                Some(inner) if inner.library => {
                    self.made_script(s, all, inner, &|| at.clone(), None)
                }
                Some(inner) => self.evaluate_script(s, all, inner, &at, chain),
                None => None,
            };
            match made.map(|held| s.restore(&held)) {
                Some(Ok(object)) => {
                    let _ = imports.set(index, object);
                }
                _ => self.log(
                    LogLevel::Warning,
                    "qml",
                    format!(
                        "{}: the script \"{}\" is not available",
                        script.key, import.path
                    ),
                ),
            }
        }
        chain.pop();
        // The source is a function of the script's own address and of
        // what it imports. Run under that address, what it throws is told
        // at the file's own line.
        if let Ok(js) = self.js() {
            js.lines.borrow_mut().add_script(url, &script.source);
        }
        let mut options = ljos_js::rquickjs::context::EvalOptions::default();
        options.filename = Some(url.to_owned());
        let made: Result<ljos_js::rquickjs::Value, _> = s
            .ctx()
            .eval_with_options::<ljos_js::rquickjs::Function, _>(script.source.as_bytes(), options)
            .and_then(|f| f.call((url, imports)));
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

fn script_starts(s: &Scope<'_>, object: &JsRef) -> Starts {
    let lines: Option<String> = (|| {
        let f: ljos_js::rquickjs::Function = s.ctx().eval(STARTS).ok()?;
        f.call((s.restore(object).ok()?,)).ok()
    })();
    lines
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

/// Gives `new` the variables of `old` whose start is the same in both.
/// Without the starts of both, nothing is known to be the same.
fn carry(s: &Scope<'_>, old: &Held, new: &Held) {
    let (Some(was), Some(is)) = (&old.starts, &new.starts) else {
        return;
    };
    let names: Vec<&str> = was
        .iter()
        .filter(|(k, v)| is.get(*k) == Some(*v))
        .map(|(k, _)| k.as_str())
        .collect();
    if names.is_empty() {
        return;
    }
    let _ = (|| {
        let f: ljos_js::rquickjs::Function = s.ctx().eval(CARRY).ok()?;
        f.call::<_, ()>((
            s.restore(&old.object).ok()?,
            s.restore(&new.object).ok()?,
            names.join("\n"),
        ))
        .ok()
    })();
}
