//! What a developer tool may ask of a running engine beyond reading it:
//! script run in the context of an object, and the binding graph of a
//! property (ARCHITECTURE.md §19.4, `app_eval` and `app_inspect`).
//!
//! Both are for tools that sit outside the application. Nothing here is
//! used by the application itself.

use std::time::Duration;

use ljos_js::{Slice, rquickjs};

use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::{ObjectId, PropIndex};
use crate::introspect::BindingTier;
use crate::js::obj_to_u64;

/// The script a devtools evaluation runs inside. It is sloppy-mode code on
/// purpose: `with (this)` lets the source name the properties of the object
/// it runs in (`width`, `text`) as a binding written in that object would.
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
    result = (function () { with (self || {}) { return eval(src); } }).call(self);
  } catch (e) {
    var msg = (e && e.message !== undefined) ? String(e.message) : String(e);
    return JSON.stringify({ error: msg, name: (e && e.name) ? String(e.name) : 'Error' });
  }
  var value = ser(result, 0);
  return JSON.stringify({ value: value, truncated: cut });
})"#;

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

impl Engine {
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
        let js = self.js()?;
        let limit = limit.min(MAX_EVAL_TIME);
        let slice = Slice {
            soft: limit,
            hard: limit,
        };
        self.untrack(|| {
            js.host
                .enter("devtools eval", slice, |s| {
                    let ctx = s.ctx();
                    // `with` is a syntax error in the strict mode rquickjs defaults to.
                    let mut opts = rquickjs::context::EvalOptions::default();
                    opts.strict = false;
                    let runner: rquickjs::Function = ctx.eval_with_options(RUNNER, opts)?;
                    let me: rquickjs::Value = match this {
                        Some(o) => s.wrapper(obj_to_u64(o))?.into_value(),
                        None => rquickjs::Value::new_undefined(ctx.clone()),
                    };
                    let [depth, items, chars, nodes, budget] = bounds;
                    let out: String =
                        runner.call((me, source, depth, items, chars, nodes, budget))?;
                    Ok(out)
                })
                .map_err(|e| EngineError::Js(e.to_string()))
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

    fn existing_node(&self, obj: ObjectId, prop: PropIndex) -> Option<crate::NodeId> {
        let (obj, prop) = self.redirect(obj, prop);
        self.inner.store.borrow().objects.get(obj)?.node(prop)
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
