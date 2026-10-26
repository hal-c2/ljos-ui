//! What a developer tool may ask of a running engine beyond reading it:
//! script run in the context of an object, and the binding graph of a
//! property (ARCHITECTURE.md §19.4, `app_eval` and `app_inspect`).
//!
//! Both are for tools that sit outside the application. Nothing here is
//! used by the application itself.

use std::rc::Rc;
use std::time::Duration;

use ljos_js::{HostFn, JsRef, JsValue, Slice, rquickjs};
use ljos_reactive::OverrideId;

use crate::engine::{Engine, OverrideContent};
use crate::error::EngineError;
use crate::ids::{ObjectId, PropIndex};
use crate::introspect::BindingTier;
use crate::js::{JsState, obj_to_u64, u64_to_obj};
use crate::types::TypeTag;
use crate::value::Value;

/// The script a devtools evaluation runs inside. It is sloppy-mode code on
/// purpose: `with (this)` lets the source name the properties of the object
/// it runs in (`width`, `text`) as a binding written in that object would.
/// The scopes around it give the names its file sees as such a binding
/// does: the ids before the object's properties, the context's other
/// names after them.
///
/// The result is written as JSON by this script rather than by the caller,
/// so that the size of what crosses into Rust is bounded here, before it is
/// copied: depth, entries per container, characters per string and nodes
/// in all. A wrapper of a host object, a function or a cycle is described
/// by a word, not followed.
const RUNNER: &str = r#"(function (self, src, depthMax, itemsMax, charsMax, nodesMax, budgetMax) {
  var nodes = 0, spent = 0, cut = false;
  function word(v) {
    var n = v && v.constructor && v.constructor.name;
    return '[' + (n || typeof v) + ']';
  }
  function ser(v, depth) {
    if (++nodes > nodesMax || spent > budgetMax) { cut = true; return '[…]'; }
    spent += 8;
    switch (typeof v) {
      case 'undefined': return { '$undefined': true };
      case 'boolean': return v;
      case 'number':
        return isFinite(v) ? v : { '$number': String(v) };
      case 'string':
        var room = Math.min(charsMax, Math.max(0, budgetMax - spent));
        if (v.length > room) { cut = true; spent += room; return v.slice(0, room) + '…'; }
        spent += v.length;
        return v;
      case 'bigint': return { '$bigint': String(v) };
      case 'function': return '[function]';
      case 'symbol': return '[symbol]';
    }
    if (v === null) return null;
    if (depth >= depthMax) { cut = true; return word(v); }
    if (Array.isArray(v)) {
      var out = [];
      for (var i = 0; i < v.length && i < itemsMax; i++) out.push(ser(v[i], depth + 1));
      if (v.length > itemsMax) { cut = true; out.push('[… ' + (v.length - itemsMax) + ' more]'); }
      return out;
    }
    var proto = Object.getPrototypeOf(v);
    if (proto !== Object.prototype && proto !== null) return word(v);
    var keys = Object.keys(v), obj = {};
    for (var k = 0; k < keys.length && k < itemsMax; k++) {
      spent += keys[k].length;
      obj[keys[k]] = ser(v[keys[k]], depth + 1);
    }
    if (keys.length > itemsMax) { cut = true; obj['…'] = (keys.length - itemsMax) + ' more'; }
    return obj;
  }
  var result;
  try {
    result = (function () {
      with (__ljos_scopeOf(self || null, false)) with (self || {})
        with (self ? __ljos_scopeOf(self, true) : {}) { return eval(src); }
    }).call(self);
  } catch (e) {
    var msg = (e && e.message !== undefined) ? String(e.message) : String(e);
    return JSON.stringify({ error: msg, name: (e && e.name) ? String(e.name) : 'Error' });
  }
  var value = ser(result, 0);
  return JSON.stringify({ value: value, truncated: cut });
})"#;

/// Makes the function of a binding an agent writes: `this` is the object,
/// and `with` names the ids of its file, its properties and then the
/// context's other names bare, in that order, as in a binding written in it.
/// The newline lets the expression end in a `//` comment. What does not
/// compile comes back as the text of the error, not thrown, so that it is
/// told to the agent and not logged as the application's.
const MAKER: &str = r#"(function (src) {
  try {
    return (0, eval)('(function () { with (__ljos_scopeOf(this, false)) with (this) with (__ljos_scopeOf(this, true)) { return (' + src + '\n); } })');
  } catch (e) {
    return String((e && e.name) || 'Error') + ': ' + String(e && e.message);
  }
})"#;

/// The names an object's file sees besides its own properties, for `with`:
/// its ids alone, or every name, found from the object's context outwards.
/// A name the script declares is never asked, as `with` asks only for what
/// is not found closer in.
const SCOPE_OF: &str = r#"globalThis.__ljos_scopeOf = (self, ids) => new Proxy({}, {
  has: (_, k) => typeof k === 'string' && __ljos_name(self, k, ids) !== null,
  get: (_, k) => {
    const r = typeof k === 'string' ? __ljos_name(self, k, ids) : null;
    return r === null ? undefined : r[0];
  },
});"#;

/// The most a devtools script may be allowed to take.
pub const MAX_EVAL_TIME: Duration = Duration::from_secs(10);

/// What a script run by [`Engine::eval_js_in`] answered, as JSON text:
/// `{"value": …, "truncated": bool}` or `{"error": "…", "name": "…"}`.
pub type EvalJson = String;

/// One end of a dependency of a property.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyReport {
    /// The object whose property it is. `None` for a node that is not a
    /// property of an object: a signal handler, a polish effect.
    pub object: Option<ObjectId>,
    pub property: Option<String>,
    /// Whether a binding drives it.
    pub bound: bool,
    pub tier: Option<BindingTier>,
}

/// What a property reads and what reads it, as of the last time each was
/// worked out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BindingGraph {
    /// Whether a binding drives the property now.
    pub bound: bool,
    /// How the binding runs, when it is a binding written in a file.
    pub tier: Option<BindingTier>,
    /// The properties its binding read when it last ran.
    pub reads: Vec<DependencyReport>,
    /// The bindings and handlers that read it when they last ran.
    pub read_by: Vec<DependencyReport>,
    /// Whether a list was cut at `limit`.
    pub truncated: bool,
}

/// [`RUNNER`] and [`MAKER`] compiled. Compiling the runner again for each
/// evaluation cost more than most of the scripts a tool sends.
pub(crate) struct Runners {
    run: JsRef,
    make: JsRef,
}

impl Engine {
    /// The script state with what devtools scripts use defined, which is
    /// done the first time one runs rather than when the state is made:
    /// a shipped application that no tool evaluates in never pays for it.
    fn devtools_js(&self) -> Result<(Rc<JsState>, JsRef, JsRef), EngineError> {
        let js = self.js()?;
        if js.devtools.get().is_none() {
            let runners = self.install_devtools_globals(&js)?;
            let _ = js.devtools.set(runners);
        }
        let r = js.devtools.get().expect("set above");
        let (run, make) = (r.run.clone(), r.make.clone());
        Ok((js, run, make))
    }

    /// What devtools scripts use to name what a binding written in an
    /// object names: `__ljos_name(object, name, ids)` is `[value]` when the
    /// object's context finds it (an id only, with `ids`), else `null`.
    fn install_devtools_globals(&self, js: &JsState) -> Result<Runners, EngineError> {
        let err = |e: ljos_js::JsFault| EngineError::Js(e.to_string());
        let w = Rc::downgrade(&self.inner);
        let name: HostFn = Rc::new(move |_scope, args| {
            let (Some(inner), [me, JsValue::String(name), ids, ..]) = (w.upgrade(), args) else {
                return Ok(JsValue::Null);
            };
            let engine = Engine::from_inner(inner);
            let me = match me {
                JsValue::Object(o) => {
                    let o = u64_to_obj(*o);
                    let Some(ctx) = engine.context_of(o) else {
                        return Ok(JsValue::Null);
                    };
                    Some((o, ctx))
                }
                // The global scope: only what the application's types are.
                _ => None,
            };
            // Tracked: a binding that names a property of a context object
            // follows it, as one written in the file does.
            let found = match (ids, me) {
                (JsValue::Bool(true), Some((_, ctx))) => {
                    engine.resolve_id(ctx, name).map(Value::Object)
                }
                (JsValue::Bool(true), None) => None,
                (_, Some((_, ctx))) => engine.context_name(ctx, name),
                (_, None) => None,
            };
            let found = found.or_else(|| {
                if matches!(ids, JsValue::Bool(true)) {
                    return None;
                }
                let from = me.and_then(|(o, _)| engine.document_key(o));
                engine
                    .singleton_for_tool(name, me.map(|(_, c)| c), from.as_deref())
                    .map(Value::Object)
            });
            Ok(found.map_or(JsValue::Null, |v| JsValue::Array(vec![engine.to_js(&v)])))
        });
        js.host
            .set_global_function("__ljos_name", name)
            .map_err(err)?;
        js.host
            .eval_script("devtools-scope", SCOPE_OF)
            .map_err(err)?;
        let slice = Slice {
            soft: MAX_EVAL_TIME,
            hard: MAX_EVAL_TIME,
        };
        js.host
            .enter("devtools runners", slice, |s| {
                // `with` is a syntax error in the strict mode rquickjs defaults to.
                let mut opts = rquickjs::context::EvalOptions::default();
                opts.strict = false;
                let run: rquickjs::Value = s.ctx().eval_with_options(RUNNER, opts)?;
                let mut opts = rquickjs::context::EvalOptions::default();
                opts.strict = false;
                let make: rquickjs::Value = s.ctx().eval_with_options(MAKER, opts)?;
                Ok(Runners {
                    run: s.keep(run),
                    make: s.keep(make),
                })
            })
            .map_err(err)
    }

    /// Runs `source` as a script. With `this` it runs in that object: `this`
    /// is the object and its properties can be named bare. The script is
    /// interrupted after `limit`, which is never more than
    /// [`MAX_EVAL_TIME`]. Nothing it reads becomes a dependency of anything.
    ///
    /// The result comes back as JSON text, cut by the bounds the caller
    /// gives (`depth`, entries per container, characters per string, nodes
    /// in all, and characters in all), so that it is small before it is copied.
    pub fn eval_js_in(
        &self,
        this: Option<ObjectId>,
        source: &str,
        limit: Duration,
        bounds: [u32; 5],
    ) -> Result<EvalJson, EngineError> {
        let (js, run, _) = self.devtools_js()?;
        let limit = limit.min(MAX_EVAL_TIME);
        let slice = Slice {
            soft: limit,
            hard: limit,
        };
        self.untrack(|| {
            js.host
                .enter("devtools eval", slice, |s| {
                    let runner: rquickjs::Function = s.restore(&run)?.get()?;
                    let me: rquickjs::Value = match this {
                        Some(o) => s.wrapper(obj_to_u64(o))?.into_value(),
                        None => rquickjs::Value::new_undefined(s.ctx().clone()),
                    };
                    let [depth, items, chars, nodes, budget] = bounds;
                    let out: String =
                        runner.call((me, source, depth, items, chars, nodes, budget))?;
                    Ok(out)
                })
                .map_err(|e| EngineError::Js(e.to_string()))
        })
    }

    /// Binds `prop` of `obj` to the JavaScript expression `source`, in place
    /// of what it held, as `Qt.binding` would: the expression names the
    /// object's properties bare, its reads are tracked, and a run that
    /// throws keeps the last value. A source that does not compile is an
    /// error and changes nothing.
    pub fn bind_expression(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        source: &str,
    ) -> Result<(), EngineError> {
        let f = self.expression_binding(obj, prop, source)?;
        let (target, target_prop) = self.redirect(obj, prop);
        self.bind_fallible_checked(target, target_prop, f)
    }

    /// [`Engine::bind_expression`] as an override over what `prop` holds,
    /// so that removing it brings that back.
    pub fn cover_with_expression(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        source: &str,
    ) -> Result<OverrideId, EngineError> {
        let f = self.expression_binding(obj, prop, source)?;
        self.push_override(obj, prop, OverrideContent::FallibleBinding(Box::new(f)))
    }

    /// Whether `source` compiles as the expression of a binding: the
    /// error says why not.
    pub fn check_expression(&self, source: &str) -> Result<(), EngineError> {
        self.compile_expression(source).map(drop)
    }

    fn compile_expression(&self, source: &str) -> Result<JsRef, EngineError> {
        let (js, _, make) = self.devtools_js()?;
        let slice = Slice {
            soft: MAX_EVAL_TIME,
            hard: MAX_EVAL_TIME,
        };
        let made = self.untrack(|| {
            js.host.enter("devtools binding", slice, |s| {
                let maker: rquickjs::Function = s.restore(&make)?.get()?;
                let made: rquickjs::Value = maker.call((source,))?;
                Ok(match made.as_string() {
                    Some(why) => Err(why.to_string()?),
                    None => Ok(s.keep(made)),
                })
            })
        });
        match made {
            Ok(Ok(f)) => Ok(f),
            Ok(Err(why)) => Err(EngineError::Js(why)),
            Err(e) => Err(EngineError::Js(e.to_string())),
        }
    }

    fn expression_binding(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        source: &str,
    ) -> Result<impl FnMut(&Engine) -> Option<Value> + 'static, EngineError> {
        let f = self.compile_expression(source)?;
        let (target, target_prop) = self.redirect(obj, prop);
        let hint = self
            .meta_of(target)
            .and_then(|m| m.property(target_prop).map(|i| i.ty.clone()))
            .unwrap_or(TypeTag::Var);
        // The object the agent named: an alias is read through, but the
        // names are the ones of the object it is written in.
        let this = JsValue::Object(obj_to_u64(obj));
        Ok(move |e: &Engine| {
            e.run_js("devtools binding", |s| {
                let out = s.call_ref(&f, &this, &[])?;
                Ok(e.js_to_value(s, out, &hint))
            })
        })
    }

    /// The binding graph of the property `name` of `obj`.
    pub fn binding_graph(
        &self,
        obj: ObjectId,
        name: &str,
        limit: usize,
    ) -> Result<BindingGraph, EngineError> {
        let prop = self.property_index(obj, name)?;
        let Some(node) = self.existing_node(obj, prop) else {
            // Nothing has read or written it yet: nothing depends on it and
            // it reads nothing.
            return Ok(BindingGraph::default());
        };
        self.give_field_reads_nodes(node);
        let g = &self.inner.graph;
        let ends = |nodes: Vec<crate::NodeId>| {
            let truncated = nodes.len() > limit;
            let reports = nodes
                .into_iter()
                .take(limit)
                .map(|n| self.dependency_at(n))
                .collect::<Vec<_>>();
            (reports, truncated)
        };
        let (reads, cut_reads) = ends(g.sources(node));
        let (read_by, cut_by) = ends(g.observers(node));
        let own = self.dependency_at(node);
        Ok(BindingGraph {
            bound: own.bound,
            tier: own.tier,
            reads,
            read_by,
            truncated: cut_reads || cut_by,
        })
    }

    /// The node of a property, made if only a read without one
    /// (`FieldReads`) observes it, so that its readers are listed.
    fn existing_node(&self, obj: ObjectId, prop: PropIndex) -> Option<crate::NodeId> {
        let (obj, prop) = self.redirect(obj, prop);
        self.observed_node(obj, prop)
    }

    /// Gives a node to each property `reader` read without one, as its
    /// next change would: the node takes the read over, so the reader's
    /// sources list it. A walk of every object, which only a tool asks for.
    fn give_field_reads_nodes(&self, reader: crate::NodeId) {
        let g = &self.inner.graph;
        let read: Vec<(ObjectId, PropIndex)> = {
            let store = self.inner.store.borrow();
            store
                .objects
                .iter()
                .filter_map(|(id, h)| Some((id, h.field_reads.as_ref()?)))
                .flat_map(|(id, r)| {
                    r.readers_of(reader)
                        .filter(|(_, r)| g.is_current(*r))
                        .map(move |(p, _)| (id, p))
                })
                .collect()
        };
        for (obj, prop) in read {
            let _ = self.observed_node(obj, prop);
        }
    }

    fn dependency_at(&self, n: crate::NodeId) -> DependencyReport {
        let key = self.inner.store.borrow().node_keys.get(n).copied();
        let g = &self.inner.graph;
        let Some(key) = key else {
            return DependencyReport {
                object: None,
                property: None,
                bound: g.is_bound(n),
                tier: None,
            };
        };
        let property = self
            .meta_of(key.object)
            .and_then(|m| m.property(key.prop).map(|p| p.name.to_string()));
        let tier = property.as_deref().and_then(|name| {
            self.inspect_properties(key.object)
                .into_iter()
                .find(|p| p.name == name)
                .and_then(|p| p.tier)
        });
        DependencyReport {
            object: Some(key.object),
            property,
            bound: g.is_bound(n),
            tier,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defined(engine: &Engine, name: &str) -> bool {
        let js = engine.js().unwrap();
        let slice = Slice {
            soft: MAX_EVAL_TIME,
            hard: MAX_EVAL_TIME,
        };
        let probe = format!("typeof globalThis.{name} !== 'undefined'");
        js.host
            .enter("test", slice, |s| s.ctx().eval::<bool, _>(probe))
            .unwrap()
    }

    #[test]
    fn an_engine_no_tool_evaluated_in_has_no_devtools_names() {
        let engine = Engine::new();
        assert!(!defined(&engine, "__ljos_scopeOf"));
        assert!(!defined(&engine, "__ljos_name"));
    }

    #[test]
    fn the_first_devtools_eval_defines_them() {
        let engine = Engine::new();
        let out = engine
            .eval_js_in(None, "1 + 1", MAX_EVAL_TIME, [4, 16, 64, 64, 1024])
            .unwrap();
        assert!(out.contains("\"value\":2"), "{out}");
        assert!(defined(&engine, "__ljos_scopeOf"));
    }

    #[test]
    fn an_expression_checked_defines_them() {
        let engine = Engine::new();
        engine.check_expression("width * 2").unwrap();
        assert!(defined(&engine, "__ljos_scopeOf"));
    }
}
