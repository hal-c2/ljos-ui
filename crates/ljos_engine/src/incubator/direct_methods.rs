//! A function declared in QML that a script calls (`display.clear()`,
//! `button.getTextColor()` in a binding) runs as a script function of its
//! own, the host out of the way: the values `Engine::call_function` would
//! have passed it before the caller's arguments are given once or read as
//! the call starts, and what it throws or gives back is the caller's as it
//! is, as with any function of the script.
//!
//! Only where nothing the host does around a call can tell: not while a
//! reload is tracked (the body may change under it), not before the object
//! completed (a type made from it may still give the method another body),
//! not for a function that makes objects (`createObject` and
//! `createQmlObject` read which file's code is running, which the host
//! notes as it calls) or reads a script file whose text may make them or
//! that imports another, and not with a parameter only the host can supply
//! at the call. A script
//! function holds its object, which a script-owned object must not have
//! hold it back from collection.

use std::rc::Rc;

use ljos_ir::{Arg, BindingKind, ExprId, ImportSource, Resolved};
use ljos_js::{DirectCall, DirectParam, JsValue, MethodFace, Scope};

use super::Instance;
use crate::engine::Engine;
use crate::ids::{MethodIndex, ObjectId};
use crate::object::ObjectFlags;
use crate::value::Value;

impl Engine {
    /// Notes the code `obj`'s method `method` runs, as
    /// [`Engine::set_script_method`] gives it its body.
    pub(crate) fn set_direct_method(
        &self,
        obj: ObjectId,
        method: MethodIndex,
        inst: Rc<Instance>,
        expr: ExprId,
    ) {
        let mut store = self.inner.store.borrow_mut();
        let Some(h) = store.objects.get_mut(obj) else {
            return;
        };
        let ext = h.ext_mut();
        match ext.direct.iter_mut().find(|(m, ..)| *m == method) {
            Some(e) => *e = (method, inst, expr),
            None => ext.direct.push((method, inst, expr)),
        }
    }

    /// What a script reads for `obj.method`.
    pub(crate) fn method_face<'js>(
        &self,
        s: &Scope<'js>,
        obj: ObjectId,
        method: MethodIndex,
    ) -> MethodFace<'js> {
        let call = MethodFace::Call { keep: true };
        if self.inner.reload.tracking.get() {
            return call;
        }
        let (inst, expr) = {
            let store = self.inner.store.borrow();
            let Some(h) = store.objects.get(obj) else {
                return call;
            };
            let Some((_, inst, expr)) = h
                .ext
                .as_ref()
                .and_then(|x| x.direct.iter().find(|(m, ..)| *m == method))
            else {
                return call;
            };
            if !h.flags.contains(ObjectFlags::COMPLETED) {
                return MethodFace::Call { keep: false };
            }
            (inst.clone(), *expr)
        };
        let Some(direct) = self.direct_call(s, &inst, expr) else {
            return call;
        };
        let name = self
            .meta_of(obj)
            .and_then(|m| m.method(method).map(|m| m.name.to_string()))
            .unwrap_or_default();
        match s.direct_method(direct, &name) {
            Ok(f) => MethodFace::Direct(f),
            Err(_) => {
                let _ = s.ctx().catch();
                call
            }
        }
    }

    fn direct_call(&self, s: &Scope<'_>, inst: &Instance, expr: ExprId) -> Option<DirectCall> {
        let file = &inst.file;
        let b = file.ir.expr(expr);
        if b.kind != BindingKind::Function {
            return None;
        }
        let js = file.ir.js.functions.get(b.js_fn.index())?;
        if makes_objects(&file.ir.js.source, &js.export) {
            return None;
        }
        let held = |o: ObjectId| {
            (!self.has_flag(o, ObjectFlags::JS_OWNED)).then(|| self.to_js(&Value::Object(o)))
        };
        let mut params = Vec::with_capacity(b.params.len());
        for p in &b.params {
            params.push(match &p.arg {
                Arg::Object(r) => DirectParam::Value(held(inst.resolve(*r)?)?),
                // A name that is a method and no property is the method
                // bound to its object (`Engine::method_js`).
                Arg::Property { object, name } => {
                    let o = inst.resolve(*object)?;
                    self.property_index(o, name).ok()?;
                    DirectParam::Read(held(o)?, name.clone())
                }
                Arg::Namespace {
                    source: ImportSource::Path(path),
                    ..
                } if path.ends_with(".js") || path.ends_with(".mjs") => {
                    let script = file.ir.scripts.iter().find(|sc| &sc.path == path)?;
                    // What the script imports could make objects in turn.
                    if !script.imports.is_empty() || names_a_maker(&script.source) {
                        return None;
                    }
                    DirectParam::Value(self.arg_js(s, inst, &p.arg))
                }
                Arg::Type(_) | Arg::Singleton(_) | Arg::Attached { .. } | Arg::Namespace { .. } => {
                    DirectParam::Value(self.arg_js(s, inst, &p.arg))
                }
                _ => return None,
            });
        }
        let this = match inst.resolve(b.scope) {
            Some(o) => held(o)?,
            None => JsValue::Undefined,
        };
        // As `Engine::eval_expr_as` retells a call that threw.
        let local = |n: &str| {
            b.refs
                .iter()
                .any(|r| r.name == n && matches!(r.resolved, Resolved::Local))
        };
        let names = b
            .params
            .iter()
            .map(|p| {
                if local(&p.name) {
                    String::new()
                } else {
                    p.name.clone()
                }
            })
            .collect();
        Some(DirectCall {
            func: file.function(self, b.js_fn).ok()?,
            this,
            params,
            file: file.url.to_string(),
            export: js.export.clone(),
            names,
            takes: b.args.clone(),
        })
    }

    fn has_flag(&self, obj: ObjectId, flag: ObjectFlags) -> bool {
        self.inner
            .store
            .borrow()
            .objects
            .get(obj)
            .is_some_and(|h| h.flags.contains(flag))
    }
}

/// Whether the body of the module's function `export` names a way to make
/// an object. Its text is what is read, so a name that only looks like
/// one counts too.
fn makes_objects(source: &str, export: &str) -> bool {
    let head = format!("export function {export}(");
    let Some(start) = source.find(&head) else {
        return true;
    };
    let rest = &source[start + head.len()..];
    let body = rest
        .find("\nexport function ")
        .map_or(rest, |end| &rest[..end]);
    names_a_maker(body)
}

/// Whether `text` names a way to make an object, or to read another
/// script that could.
fn names_a_maker(text: &str) -> bool {
    [
        "createObject",
        "createQmlObject",
        "createComponent",
        "incubateObject",
        "include",
    ]
    .iter()
    .any(|w| text.contains(w))
}

#[cfg(test)]
mod tests {
    use super::makes_objects;

    #[test]
    fn only_the_function_s_own_text_is_read() {
        let src = "// a.qml:1:1 f\nexport function f0(a) {\n  return a.x;\n}\n\
                   // a.qml:2:1 g\nexport function f1() {\n  c.createObject(p);\n}\n";
        assert!(!makes_objects(src, "f0"));
        assert!(makes_objects(src, "f1"));
        assert!(makes_objects(src, "f2"));
    }
}
