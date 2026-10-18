//! What each tool answers, written from a [`Capture`] on the UI thread.
//!
//! Every answer has two copies: text for a model to read and JSON for a
//! program. Each is kept under the size its tool allows by leaving out the
//! end of a list and saying so, never by cutting in the middle of a line.

use std::fmt::Write as _;

use ljos_engine::{
    BindingGraph, DependencyReport, Engine, EngineError, ObjectId, Provenance, Value as QmlValue,
};
use ljos_mcp_core::{
    CapSet, Delta, ErrorCode, LineOptions, Locator, Outline, RefTable, SurfaceError, ToolReply,
    render_line,
};
use ljos_quick::QuickScene;
use ljos_render::{FrameView, RasterConfig, Rgba, RgbaImage, VelloCpuReference, kurbo};
use serde_json::{Map, Value, json};

use super::args::{
    DEFAULT_MATCHES, FindArgs, InspectArgs, MAX_MATCHES, MAX_SCREENSHOT_SIDE, ScreenshotArgs,
};
use super::capture::{Capture, REDACTED, all_objects};
use super::surface::Answer;
use crate::top::{FrameStats, LANE_NAMES, owner_name};

/// The most bytes a picture may have (§19.6).
pub(crate) const MAX_PICTURE_BYTES: usize = 1024 * 1024;
/// A property's value is cut after this many characters.
const MAX_VALUE_CHARS: usize = 200;
/// How many candidates an `ambiguous` refusal lists.
const MAX_CANDIDATES: usize = 10;
/// The most properties `dependencies` lists on each side.
const MAX_DEPENDENCIES: usize = 50;
/// The properties whose value is what was typed.
pub(crate) const TYPED: [&str; 4] = ["text", "displayText", "preeditText", "selectedText"];

/// What a tool reads: the application as it is now.
pub(crate) struct Look<'a> {
    pub(crate) engine: &'a Engine,
    pub(crate) scene: &'a QuickScene,
    pub(crate) capture: &'a Capture,
    pub(crate) refs: &'a mut RefTable,
}

/// What a `target` names.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Target {
    /// A line of the outline.
    Node(usize),
    /// An object the outline does not list: an item without a role, or one
    /// that is not shown.
    Unlisted(ObjectId),
}

/// A whole number as an integer, so that a client reads `40` and not
/// `40.0`; anything else to two decimals.
pub(crate) fn num(n: f64) -> Value {
    if !n.is_finite() {
        return Value::Null;
    }
    if n.fract() == 0.0 && n.abs() < 9.0e15 {
        return json!(n as i64);
    }
    json!((n * 100.0).round() / 100.0)
}

fn micros(d: std::time::Duration) -> Value {
    json!(u64::try_from(d.as_micros()).unwrap_or(u64::MAX))
}

pub(crate) fn clip(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.to_string(),
    }
}

pub(crate) fn text_property(engine: &Engine, obj: ObjectId, name: &str) -> Option<String> {
    match engine.read_by_name(obj, name) {
        QmlValue::String(s) if !s.is_empty() => Some(s.to_string()),
        _ => None,
    }
}

pub(crate) fn flag(engine: &Engine, obj: ObjectId, name: &str) -> Option<bool> {
    match engine.read_by_name(obj, name) {
        QmlValue::Undefined | QmlValue::Null => None,
        v => Some(v.to_boolean()),
    }
}

pub(crate) fn number_property(engine: &Engine, obj: ObjectId, name: &str) -> f64 {
    match engine.read_by_name(obj, name) {
        QmlValue::Undefined | QmlValue::Null => 0.0,
        v => v.to_number(),
    }
}

/// Where an item is in the window: `[x, y, width, height]` of the box
/// around it after every transform above it.
pub(crate) fn window_box(engine: &Engine, obj: ObjectId) -> [f64; 4] {
    let w = number_property(engine, obj, "width");
    let h = number_property(engine, obj, "height");
    let r = ljos_quick::scene_transform(engine, obj)
        .transform_rect_bbox(kurbo::Rect::new(0.0, 0.0, w, h));
    [r.x0, r.y0, r.width(), r.height()]
}

/// Whether `obj` and everything above it is visible. Call untracked.
pub(crate) fn visible_to_the_top(engine: &Engine, obj: ObjectId) -> bool {
    let mut up = Some(obj);
    // A parent chain is finite; the bound is against a cycle made by a bug.
    for _ in 0..4096 {
        let Some(o) = up else { break };
        if flag(engine, o, "visible") == Some(false) {
            return false;
        }
        up = engine.visual_parent(o);
    }
    true
}

/// One end of a dependency, as an agent reads it.
fn dependency_end(engine: &Engine, d: &DependencyReport) -> Value {
    let (ty, object_name) = match d.object {
        Some(o) if engine.is_alive(o) => (
            engine
                .inspect_object(o)
                .and_then(|r| r.types.first().cloned()),
            engine.untrack(|| text_property(engine, o, "objectName")),
        ),
        _ => (None, None),
    };
    json!({
        "object": ty,
        "object_name": object_name,
        "property": d.property,
        "bound": d.bound,
        "tier": d.tier.map(|t| t.as_str()),
    })
}

/// The `dependencies` of `app_inspect`: what the binding of `name` read when
/// it last ran, what reads `name`, and the tier its binding runs at.
fn dependencies(engine: &Engine, name: &str, graph: &BindingGraph) -> (Value, String) {
    let reads: Vec<Value> = graph
        .reads
        .iter()
        .map(|d| dependency_end(engine, d))
        .collect();
    let read_by: Vec<Value> = graph
        .read_by
        .iter()
        .map(|d| dependency_end(engine, d))
        .collect();
    let value = json!({
        "property": name,
        "bound": graph.bound,
        "tier": graph.tier.map(|t| t.as_str()),
        "reads": reads,
        "read_by": read_by,
        "truncated": graph.truncated,
    });
    let end = |v: &Value| {
        let property = v["property"].as_str().unwrap_or("(a handler)");
        let object = match (v["object_name"].as_str(), v["object"].as_str()) {
            (Some(n), _) => format!(" of #{n}"),
            (None, Some(t)) => format!(" of a {t}"),
            _ => String::new(),
        };
        format!("{property}{object}")
    };
    let list = |items: &[Value]| {
        if items.is_empty() {
            "nothing".to_string()
        } else {
            items.iter().map(end).collect::<Vec<_>>().join(", ")
        }
    };
    let mut text = format!(
        "dependencies of `{name}`: {}",
        match (graph.bound, graph.tier) {
            (false, _) => "not bound".to_string(),
            (true, Some(t)) => format!("bound at tier {}", t.as_str()),
            (true, None) => "bound".to_string(),
        }
    );
    let _ = write!(
        text,
        "\n  reads: {}\n  read by: {}{}\n",
        list(&reads),
        list(&read_by),
        if graph.truncated { " (cut)" } else { "" }
    );
    (value, text)
}

fn no_match(what: &str) -> SurfaceError {
    SurfaceError::not_found(format!(
        "nothing matches `{what}`. Take an `app_snapshot` to see what there is, or try \
         `app_find` with a wider locator (`role=button`, `text=part of the name`)"
    ))
}

impl Look<'_> {
    fn reference(&mut self, index: usize) -> String {
        self.refs.assign(&self.capture.tree, index)
    }

    /// The one item `what` names, for `tool`.
    /// The object `what` names, listed or not.
    pub(crate) fn object(&mut self, what: &str, tool: &str) -> Result<ObjectId, SurfaceError> {
        match self.resolve(what, tool)? {
            Target::Node(index) => self.capture.items[index].ok_or_else(|| {
                SurfaceError::new(
                    ErrorCode::NotActionable,
                    format!("`{what}` is a line of the outline with no object behind it"),
                )
            }),
            Target::Unlisted(obj) => Ok(obj),
        }
    }

    pub(crate) fn resolve(&mut self, what: &str, tool: &str) -> Result<Target, SurfaceError> {
        if let Some(found) = super::objects::parse_handle(self.engine, what) {
            let obj = found?;
            return Ok(
                match self.capture.items.iter().position(|i| *i == Some(obj)) {
                    Some(index) => Target::Node(index),
                    None => Target::Unlisted(obj),
                },
            );
        }
        let locator = Locator::parse(what)?;
        let found = locator.find(&self.capture.tree, self.refs)?;
        match found.as_slice() {
            [one] => Ok(Target::Node(*one)),
            [] => match self.unlisted(&locator)?.as_slice() {
                [one] => Ok(Target::Unlisted(*one)),
                [] => match locator.id_instead() {
                    Some(by_id) => self
                        .resolve(by_id.source(), tool)
                        .map_err(|e| match e.code {
                            ErrorCode::NotFound => no_match(what),
                            _ => e,
                        }),
                    None => Err(no_match(what)),
                },
                many => Err(self.ambiguous_unlisted(what, tool, many)),
            },
            many => Err(self.ambiguous(what, tool, many)),
        }
    }

    /// Objects the outline does not list that a plain `#objectName` or
    /// `id=` locator names (for `#` with no such name, see
    /// `Locator::id_instead`).
    fn unlisted(&self, locator: &Locator) -> Result<Vec<ObjectId>, SurfaceError> {
        let engine = self.engine;
        if let Some(name) = locator.as_object_name() {
            let all = all_objects(engine, self.scene);
            return Ok(engine.untrack(|| {
                all.into_iter()
                    .filter(|o| text_property(engine, *o, "objectName").as_deref() == Some(name))
                    .collect()
            }));
        }
        if let Some(id) = locator.as_id() {
            let all = all_objects(engine, self.scene);
            let ids = engine.inspect_ids(&all);
            return Ok(all
                .into_iter()
                .zip(ids)
                .filter(|(_, i)| i.as_deref() == Some(id))
                .map(|(o, _)| o)
                .collect());
        }
        Ok(Vec::new())
    }

    /// The outline lists none of `found` (the copies of a delegate, as a
    /// rule), so their handles are the references.
    fn ambiguous_unlisted(&self, what: &str, tool: &str, found: &[ObjectId]) -> SurfaceError {
        let mut message = format!(
            "`{what}` names {} objects the outline does not list and `{tool}` needs exactly \
             one. Candidates:",
            found.len()
        );
        let mut candidates = Vec::new();
        for &obj in found.iter().take(MAX_CANDIDATES) {
            let reference = super::objects::handle(obj);
            let line = super::observe::label(self.engine, obj).0;
            let _ = write!(message, "\n  {reference}  {line}");
            candidates.push(json!({ "ref": reference, "line": line }));
        }
        if found.len() > MAX_CANDIDATES {
            let _ = write!(message, "\n  … and {} more", found.len() - MAX_CANDIDATES);
        }
        message.push_str("\nUse one of the handles, or `app_objects` to see where each one is");
        SurfaceError::new(ErrorCode::Ambiguous, message)
            .with_details(json!({ "count": found.len(), "candidates": candidates }))
    }

    fn ambiguous(&mut self, what: &str, tool: &str, found: &[usize]) -> SurfaceError {
        let mut message = format!(
            "`{what}` matches {} items and `{tool}` needs exactly one. Candidates:",
            found.len()
        );
        let mut candidates = Vec::new();
        for &index in found.iter().take(MAX_CANDIDATES) {
            let reference = self.reference(index);
            let line = render_line(
                self.capture.node(index),
                Some(&reference),
                LineOptions::default(),
            );
            let path = self.capture.tree.path(index);
            let _ = write!(message, "\n  {line}  (in {path})");
            candidates.push(json!({ "ref": reference, "line": line, "path": path }));
        }
        if found.len() > MAX_CANDIDATES {
            let _ = write!(message, "\n  … and {} more", found.len() - MAX_CANDIDATES);
        }
        message.push_str(
            "\nUse one of the references, or add `>> nth=0` to the locator for the first match \
             (`nth=1` for the second)",
        );
        SurfaceError::new(ErrorCode::Ambiguous, message)
            .with_details(json!({ "count": found.len(), "candidates": candidates }))
    }

    // ----- app_find -------------------------------------------------------

    pub(crate) fn find(&mut self, args: &FindArgs, per_copy: usize) -> Answer {
        let locator = Locator::parse(&args.locator)?;
        let mut found = locator.find(&self.capture.tree, self.refs)?;
        if found.is_empty()
            && let Some(by_id) = locator.id_instead()
        {
            found = by_id.find(&self.capture.tree, self.refs)?;
        }
        let total = found.len();
        let limit = args.limit.unwrap_or(DEFAULT_MATCHES).clamp(1, MAX_MATCHES);
        let mut rows: Vec<(Value, String)> = Vec::new();
        for &index in found.iter().take(limit) {
            let reference = self.reference(index);
            let node = self.capture.node(index);
            let path = self.capture.tree.path(index);
            let mut row = Map::new();
            row.insert("ref".into(), json!(reference));
            row.insert("role".into(), json!(node.role));
            row.insert("name".into(), json!(clip(&node.name, MAX_VALUE_CHARS)));
            row.insert("path".into(), json!(path));
            if let Some(obj) = &node.object_name {
                row.insert("obj".into(), json!(obj));
            }
            if let Some(id) = &node.qml_id {
                row.insert("id".into(), json!(id));
            }
            match &node.value {
                Some(ljos_mcp_core::NodeValue::Text(t)) => {
                    row.insert("value".into(), json!(clip(t, MAX_VALUE_CHARS)));
                }
                Some(ljos_mcp_core::NodeValue::Number(n)) => {
                    row.insert("value".into(), num(*n));
                }
                Some(ljos_mcp_core::NodeValue::Redacted) => {
                    row.insert("value".into(), json!(REDACTED));
                }
                None => {}
            }
            row.insert(
                "bounds".into(),
                Value::Array(node.bounds.iter().map(|b| num(*b)).collect()),
            );
            row.insert("states".into(), json!(node.states));
            let opts = LineOptions {
                bounds: true,
                ..LineOptions::default()
            };
            let mut line = render_line(node, Some(&reference), opts);
            if !path.is_empty() {
                let _ = write!(line, "  (in {path})");
            }
            rows.push((Value::Object(row), line));
        }
        loop {
            let listed = rows.len();
            let truncated = listed < total;
            let value = json!({
                "matches": rows.iter().map(|(v, _)| v.clone()).collect::<Vec<_>>(),
                "count": listed,
                "total": total,
                "truncated": truncated,
            });
            let mut text = String::new();
            for (_, line) in &rows {
                let _ = writeln!(text, "{line}");
            }
            let _ = match (total, truncated) {
                (0, _) => write!(
                    text,
                    "(no match for `{}`; this is an answer, not an error)",
                    args.locator
                ),
                (_, false) => write!(text, "({total} matches)"),
                (_, true) => write!(
                    text,
                    "({listed} of {total} matches listed; narrow the locator, or add \
                     `>> nth=N` for one of them)"
                ),
            };
            if listed <= 1 || (text.len() <= per_copy && value.to_string().len() <= per_copy) {
                return Ok(ToolReply::from(value).with_text(text));
            }
            rows.truncate(listed - listed.div_ceil(4));
        }
    }

    // ----- app_inspect ----------------------------------------------------

    pub(crate) fn inspect(
        &mut self,
        args: &InspectArgs,
        per_copy: usize,
        edits: &super::edits::Edits,
    ) -> Answer {
        let target = self.resolve(&args.target, "app_inspect")?;
        let engine = self.engine;
        let (obj, listed) = match target {
            Target::Node(index) => (self.capture.items[index], Some(index)),
            Target::Unlisted(obj) => (Some(obj), None),
        };
        let mut out = Map::new();
        let mut head = String::new();
        match listed {
            Some(index) => {
                let reference = self.reference(index);
                let node = self.capture.node(index);
                out.insert("ref".into(), json!(reference));
                out.insert("role".into(), json!(node.role));
                out.insert("name".into(), json!(clip(&node.name, MAX_VALUE_CHARS)));
                let _ = writeln!(
                    head,
                    "{}",
                    render_line(node, Some(&reference), LineOptions::default())
                );
            }
            None => {
                out.insert("ref".into(), Value::Null);
                out.insert("role".into(), Value::Null);
                out.insert("name".into(), Value::Null);
                let _ = writeln!(
                    head,
                    "(not in the outline: the item has no role, or it is not shown)"
                );
            }
        }
        let Some(obj) = obj.filter(|o| engine.is_alive(*o)) else {
            // A line of the outline with no object behind it (the window of
            // a scene with no root).
            out.insert("type".into(), Value::Null);
            out.insert("properties".into(), json!([]));
            head.push_str("no QML object is behind this line");
            return Ok(ToolReply::from(Value::Object(out)).with_text(head));
        };
        let report = engine.inspect_object(obj);
        let types = report.as_ref().map(|r| r.types.clone()).unwrap_or_default();
        let ty = types.first().cloned();
        let id = report.as_ref().and_then(|r| r.id.clone());
        let key = report.as_ref().and_then(|r| r.key.clone());
        let source = report.as_ref().and_then(|r| r.declared_at.clone());
        let secret = self.capture.secret_items.contains(&obj);
        let (object_name, geometry, state, properties) = engine.untrack(|| {
            let object_name = text_property(engine, obj, "objectName");
            let geometry = ljos_quick::is_item(engine, obj).then(|| {
                let window = window_box(engine, obj);
                json!({
                    "x": num(number_property(engine, obj, "x")),
                    "y": num(number_property(engine, obj, "y")),
                    "width": num(number_property(engine, obj, "width")),
                    "height": num(number_property(engine, obj, "height")),
                    "window": window.iter().map(|v| num(*v)).collect::<Vec<_>>(),
                })
            });
            let visible = flag(engine, obj, "visible");
            // An item the outline lists is shown. One it does not list is
            // hidden when it, or something above it, is not visible.
            let shown = listed.is_some() || visible_to_the_top(engine, obj);
            let state = json!({
                "visible": visible,
                "enabled": flag(engine, obj, "enabled"),
                "focus": flag(engine, obj, "activeFocus"),
                "lattice": if shown { "active" } else { "hidden" },
            });
            (object_name, geometry, state, engine.inspect_properties(obj))
        });

        out.insert("type".into(), json!(ty));
        out.insert("types".into(), json!(types));
        out.insert("id".into(), json!(id));
        out.insert("object_name".into(), json!(object_name));
        out.insert("key".into(), json!(key));
        out.insert(
            "source".into(),
            match &source {
                Some(at) => json!({ "file": at.file, "line": at.line, "column": at.column }),
                None => Value::Null,
            },
        );
        out.insert("geometry".into(), geometry.clone().unwrap_or(Value::Null));
        out.insert("state".into(), state.clone());

        let _ = write!(head, "{}", ty.as_deref().unwrap_or("(unknown type)"));
        if let Some(id) = &id {
            let _ = write!(head, " id={id}");
        }
        if let Some(name) = &object_name {
            let _ = write!(head, " obj={name}");
        }
        head.push('\n');
        if let Some(at) = &source {
            let _ = writeln!(head, "declared at {}:{}:{}", at.file, at.line, at.column);
        }
        if let Some(key) = &key {
            let _ = writeln!(head, "key {key}");
        }
        if let Some(g) = &geometry {
            let _ = writeln!(
                head,
                "geometry x={} y={} {}x{}; in the window at {},{} size {}x{}",
                g["x"],
                g["y"],
                g["width"],
                g["height"],
                g["window"][0],
                g["window"][1],
                g["window"][2],
                g["window"][3]
            );
        }
        let _ = writeln!(
            head,
            "state visible={} enabled={} focus={} lattice={}",
            state["visible"], state["enabled"], state["focus"], state["lattice"]
        );

        // What the object reads by the names of the types that are one
        // for each window: its window's objects of them.
        let window = engine.singleton_scope_root(obj);
        let singletons = window
            .map(|w| engine.scoped_singletons(w))
            .unwrap_or_default();
        let mut shown = Vec::new();
        for s in singletons {
            let ty = engine
                .inspect_object(s)
                .and_then(|r| r.types.first().cloned());
            let mut props = Vec::new();
            let mut said = Vec::new();
            for p in engine.untrack(|| engine.inspect_properties(s)) {
                let Some(v) = p.value.filter(|_| p.provenance != Provenance::Default) else {
                    continue;
                };
                let js = engine.to_js_string(&v).to_string();
                let text = clip(self.capture.shown(&js), MAX_VALUE_CHARS);
                said.push(format!("{}={text}", p.name));
                props.push(json!({ "name": p.name, "value": text }));
            }
            let _ = writeln!(
                head,
                "window singleton {} {}",
                ty.as_deref().unwrap_or("(unknown type)"),
                said.join(" ")
            );
            shown.push(json!({ "type": ty, "properties": props }));
        }
        out.insert("window_singletons".into(), json!(shown));

        if let Some(name) = &args.dependencies {
            let graph = engine
                .untrack(|| engine.binding_graph(obj, name, MAX_DEPENDENCIES))
                .map_err(|e| match e {
                    EngineError::UnknownProperty { .. } => SurfaceError::not_found(format!(
                        "`{}` has no property `{name}`. `app_inspect` without `dependencies` \
                         lists its properties",
                        args.target
                    )),
                    other => SurfaceError::invalid_argument(other.to_string()),
                })?;
            let (value, text) = dependencies(engine, name, &graph);
            out.insert("dependencies".into(), value);
            head.push_str(&text);
        }

        let mut defaults = 0usize;
        let mut rows: Vec<(Value, String)> = Vec::new();
        let mut sources = Sources::default();
        for p in &properties {
            if p.provenance == Provenance::Default && !args.all {
                defaults += 1;
                continue;
            }
            let provenance = match p.provenance {
                Provenance::Default => "default",
                Provenance::Assigned => "assigned",
                Provenance::Bound => "bound",
                Provenance::Animated => "animated",
                Provenance::Host => "host",
            };
            let mut held = None;
            let value = match &p.value {
                None => Value::Null,
                Some(v) => {
                    let seen = super::values::shown(engine, v);
                    let text = seen.text;
                    held = Some((seen.handle, seen.items));
                    let typed = secret && TYPED.contains(&p.name.as_str());
                    let leaked =
                        matches!(v, QmlValue::String(_)) && self.capture.shown(&text) == REDACTED;
                    if typed || leaked {
                        json!(REDACTED)
                    } else {
                        json!(clip(&text, MAX_VALUE_CHARS))
                    }
                }
            };
            let mut row = Map::new();
            row.insert("name".into(), json!(p.name));
            row.insert("type".into(), json!(p.ty));
            row.insert("value".into(), value.clone());
            row.insert("provenance".into(), json!(provenance));
            if let Some((handle, items)) = held {
                if let Some(h) = handle {
                    row.insert("handle".into(), json!(h));
                }
                if let Some(items) = items {
                    row.insert("items".into(), Value::Array(items));
                }
            }
            let covered = edits.covers(obj, &p.name);
            let own = edits.expression(obj, &p.name);
            let by_devtools = covered
                || (p.provenance == Provenance::Assigned && edits.contains(obj, &p.name))
                || (p.provenance == Provenance::Bound && own.is_some());
            if by_devtools {
                row.insert("assigned_by".into(), json!("devtools"));
            }
            if covered {
                row.insert("binding_kept".into(), json!(true));
            }
            let mut line = format!(
                "  {}: {} = {}  ({provenance}",
                p.name,
                p.ty,
                match &value {
                    Value::String(s) => s.as_str(),
                    _ => "(no value)",
                }
            );
            if let Some(tier) = p.tier {
                row.insert("tier".into(), json!(tier.as_str()));
                let _ = write!(line, ", tier {}", tier.as_str());
            }
            if let Some(at) = &p.written_at {
                row.insert("file".into(), json!(at.file));
                row.insert("line".into(), json!(at.line));
                let _ = write!(line, ", line {}", at.line);
            }
            let expression = match own {
                Some(own) => Some(own.to_owned()),
                None if by_devtools => None,
                None => p
                    .written_at
                    .as_ref()
                    .zip(p.expression.clone())
                    .and_then(|(at, bytes)| sources.text(&at.file, bytes)),
            };
            if let Some(expression) = expression {
                let expression = clip(&expression, MAX_VALUE_CHARS);
                let _ = write!(line, ", `{expression}`");
                row.insert("expression".into(), json!(expression));
            }
            if by_devtools {
                line.push_str(", by devtools");
            }
            if p.read_only {
                row.insert("read_only".into(), json!(true));
                line.push_str(", read-only");
            }
            line.push(')');
            rows.push((Value::Object(row), line));
        }

        let all_rows = rows.len();
        loop {
            let cut = all_rows - rows.len();
            let mut value = out.clone();
            value.insert(
                "properties".into(),
                Value::Array(rows.iter().map(|(v, _)| v.clone()).collect()),
            );
            value.insert("omitted".into(), json!(defaults + cut));
            value.insert("truncated".into(), json!(cut > 0));
            let value = Value::Object(value);
            let mut text = head.clone();
            let _ = writeln!(text, "properties ({}):", rows.len());
            for (_, line) in &rows {
                let _ = writeln!(text, "{line}");
            }
            if defaults > 0 {
                let _ = writeln!(
                    text,
                    "({defaults} properties still at their default are left out; `all: true` \
                     lists them)"
                );
            }
            if cut > 0 {
                let _ = writeln!(text, "({cut} more properties did not fit in one answer)");
            }
            if text.ends_with('\n') {
                text.pop();
            }
            if rows.is_empty() || (text.len() <= per_copy && value.to_string().len() <= per_copy) {
                return Ok(ToolReply::from(value).with_text(text));
            }
            let keep = rows.len() - rows.len().div_ceil(4);
            rows.truncate(keep);
        }
    }

    // ----- app_screenshot -------------------------------------------------

    /// A PNG of the window or of one item of it.
    pub(crate) fn screenshot(&mut self, args: &ScreenshotArgs, background: [u8; 3]) -> Answer {
        let engine = self.engine;
        let viewport = self.scene.viewport();
        let whole = kurbo::Rect::new(0.0, 0.0, viewport.width(), viewport.height());
        let area = match &args.target {
            None => whole,
            Some(what) => {
                let b = match self.resolve(what, "app_screenshot")? {
                    Target::Node(index) => self.capture.node(index).bounds,
                    Target::Unlisted(obj) => {
                        if !engine.untrack(|| visible_to_the_top(engine, obj)) {
                            return Err(SurfaceError::new(
                                ErrorCode::NotActionable,
                                format!(
                                    "`{what}` is not shown (it, or an item above it, has \
                                     `visible: false`), so there is nothing to picture"
                                ),
                            ));
                        }
                        engine.untrack(|| window_box(engine, obj))
                    }
                };
                let item = kurbo::Rect::new(b[0], b[1], b[0] + b[2], b[1] + b[3]);
                let seen = item.intersect(whole);
                if seen.width() < 1.0 || seen.height() < 1.0 {
                    return Err(SurfaceError::new(
                        ErrorCode::NotActionable,
                        format!(
                            "`{what}` has nothing to picture: it is {}x{} at {},{} in a window \
                             of {}x{}. `app_inspect` says whether it is visible",
                            num(b[2]),
                            num(b[3]),
                            num(b[0]),
                            num(b[1]),
                            num(whole.width()),
                            num(whole.height())
                        ),
                    ));
                }
                seen
            }
        };
        if area.width() < 1.0 || area.height() < 1.0 {
            return Err(SurfaceError::unavailable(
                "the window has no size yet, so there is nothing to picture",
            ));
        }
        let Some(frame) = self.scene.snapshot() else {
            return Err(SurfaceError::unavailable(
                "the application has not drawn a frame yet; ask again in a moment",
            ));
        };
        // What is typed into a masked field is not in the picture either: a
        // field may show the last character typed, or all of it while it is
        // being edited.
        let covered: Vec<[f64; 4]> = engine.untrack(|| {
            self.capture
                .secret_items
                .iter()
                .filter(|o| engine.is_alive(**o))
                .map(|o| window_box(engine, *o))
                .collect()
        });
        let limit = args
            .max_size
            .unwrap_or(MAX_SCREENSHOT_SIDE)
            .clamp(1, MAX_SCREENSHOT_SIDE);
        let longest = area.width().max(area.height());
        let mut scale = (f64::from(limit) / longest).min(1.0);
        let mut renderer = VelloCpuReference::new(RasterConfig {
            background: Rgba::rgb(background[0], background[1], background[2]),
            ..RasterConfig::default()
        });
        for _ in 0..8 {
            let full = renderer
                .render(&FrameView::new(frame, scale as f32))
                .map_err(|e| SurfaceError::unavailable(format!("the picture failed: {e}")))?;
            let mut picture = crop(&full, area, scale, limit);
            for b in &covered {
                cover(&mut picture, *b, area, scale);
            }
            let png = picture
                .to_png()
                .map_err(|e| SurfaceError::unavailable(format!("the picture failed: {e}")))?;
            if png.len() > MAX_PICTURE_BYTES && picture.width.max(picture.height) > 16 {
                // Smaller until it fits: a picture is for looking, and the
                // outline has the exact text.
                scale /= 2.0;
                continue;
            }
            let value = json!({
                "width": picture.width,
                "height": picture.height,
                "scale": num(scale),
                "bytes": png.len(),
                "area": [num(area.x0), num(area.y0), num(area.width()), num(area.height())],
                "masked": covered.len(),
            });
            let mut text = format!(
                "PNG {}x{} px, scale {} of {}x{} at {},{} in the window",
                picture.width,
                picture.height,
                num(scale),
                num(area.width()),
                num(area.height()),
                num(area.x0),
                num(area.y0)
            );
            if !covered.is_empty() {
                let _ = write!(
                    text,
                    "; {} field(s) whose text must not be shown are painted over",
                    covered.len()
                );
            }
            return Ok(ToolReply::from(value)
                .with_text(text)
                .with_image("image/png", png));
        }
        Err(SurfaceError::unavailable(
            "the picture does not fit in 1 MB at any size; use `target` to picture a part",
        ))
    }

    // ----- app_status -----------------------------------------------------

    pub(crate) fn status(
        &mut self,
        virtual_clock: bool,
        caps: CapSet,
        reload: &Value,
        windows: usize,
        (painter, startup, host_text): (Value, Value, String),
    ) -> Answer {
        let engine = self.engine;
        let viewport = self.scene.viewport();
        let prefs = ljos_quick::system_prefs(engine);
        let scheme = match prefs.color_scheme {
            ljos_a11y::ColorScheme::Unknown => "unknown",
            ljos_a11y::ColorScheme::Light => "light",
            ljos_a11y::ColorScheme::Dark => "dark",
        };
        let root = self.scene.root();
        let visible = root
            .and_then(|r| engine.untrack(|| flag(engine, r, "visible")))
            .unwrap_or(root.is_some());
        let title = self.capture.node(0).name.clone();
        let focus = match self.capture.focus {
            Some(index) => {
                let reference = self.reference(index);
                let node = self.capture.node(index);
                json!({
                    "role": node.role,
                    "name": clip(&node.name, MAX_VALUE_CHARS),
                    "ref": reference,
                })
            }
            None => Value::Null,
        };
        let mut window = Map::new();
        if !title.is_empty() {
            window.insert("title".into(), json!(clip(&title, MAX_VALUE_CHARS)));
        }
        window.insert("width".into(), num(viewport.width()));
        window.insert("height".into(), num(viewport.height()));
        window.insert("scale".into(), num(f64::from(self.scene.scale_factor())));
        window.insert("visible".into(), json!(visible));
        window.insert(
            "appearance".into(),
            json!({
                "color_scheme": scheme,
                "reduced_motion": prefs.reduced_motion,
                "font_scale": num(prefs.font_scale),
            }),
        );
        window.insert("focus".into(), focus.clone());
        let clock = if virtual_clock { "virtual" } else { "system" };
        let nodes = self.capture.tree.len();
        let animations = ljos_quick::running_animation_count(engine);
        let mut value = json!({
            "windows": [Value::Object(window)],
            "window_count": windows,
            "inspected": "primary_window_only",
            "clock": clock,
            "reload_generation": engine.reload_generation(),
            "objects": engine.object_count(),
            "outline_nodes": nodes,
            "animations": animations,
            "capabilities": caps.iter().map(|c| c.as_str()).collect::<Vec<_>>(),
            "reload": reload,
        });
        value["painter"] = painter;
        value["startup"] = startup;
        let mut text = format!(
            "window {}x{} at scale {}{}{}\n",
            num(viewport.width()),
            num(viewport.height()),
            num(f64::from(self.scene.scale_factor())),
            if title.is_empty() {
                String::new()
            } else {
                format!(", title {:?}", clip(&title, MAX_VALUE_CHARS))
            },
            if visible { "" } else { ", not visible" },
        );
        let _ = writeln!(
            text,
            "{windows} {}; only the primary window is inspected",
            if windows == 1 { "window" } else { "windows" }
        );
        let _ = writeln!(
            text,
            "appearance: {scheme} colour scheme, reduced motion {}, font scale {}",
            if prefs.reduced_motion { "on" } else { "off" },
            num(prefs.font_scale)
        );
        let _ = match &focus {
            Value::Null => writeln!(text, "keyboard focus: nowhere"),
            f => writeln!(
                text,
                "keyboard focus: {} {} [{}]",
                f["role"].as_str().unwrap_or_default(),
                f["name"],
                f["ref"].as_str().unwrap_or_default()
            ),
        };
        let _ = writeln!(
            text,
            "{}",
            match animations {
                0 => "no animation under way".to_string(),
                1 => "1 animation under way".to_string(),
                n => format!("{n} animations under way"),
            }
        );
        let _ = write!(
            text,
            "{nodes} items in the outline, {} objects, {clock} clock, reload generation {}\n\
             capabilities: {}",
            engine.object_count(),
            engine.reload_generation(),
            if caps.iter().next().is_none() {
                "none".to_string()
            } else {
                caps.iter()
                    .map(|c| c.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        );
        let last = &reload["last"];
        if !last.is_null() {
            let _ = write!(
                text,
                "\nlast reload: {}{}",
                last["outcome"].as_str().unwrap_or_default(),
                match last["message"].as_str() {
                    Some(m) => format!(": {m}"),
                    None => String::new(),
                }
            );
        }
        if reload["watched"] == json!(false) {
            text.push_str("\nfiles are not watched");
        }
        text.push_str(&host_text);
        Ok(ToolReply::from(value).with_text(text))
    }
}

/// The part `area` (logical pixels) of `full`, which is the window at
/// `scale`. Never larger than `limit` on a side.
fn crop(full: &RgbaImage, area: kurbo::Rect, scale: f64, limit: u32) -> RgbaImage {
    let side = |logical: f64, most: u32| -> u32 {
        ((logical * scale).round() as u32)
            .clamp(1, limit)
            .min(most.max(1))
    };
    let w = side(area.width(), full.width);
    let h = side(area.height(), full.height);
    let x0 = ((area.x0 * scale).round().max(0.0) as u32).min(full.width.saturating_sub(w));
    let y0 = ((area.y0 * scale).round().max(0.0) as u32).min(full.height.saturating_sub(h));
    if x0 == 0 && y0 == 0 && w == full.width && h == full.height {
        return full.clone();
    }
    let mut data = Vec::with_capacity(w as usize * h as usize * 4);
    for y in y0..(y0 + h).min(full.height) {
        let from = (y as usize * full.width as usize + x0 as usize) * 4;
        let to = from + (w.min(full.width - x0)) as usize * 4;
        data.extend_from_slice(&full.data[from..to]);
    }
    data.resize(w as usize * h as usize * 4, 0);
    RgbaImage {
        width: w,
        height: h,
        data,
    }
}

/// Paints over where `item` (logical pixels in the window) is in a picture
/// of `area` at `scale`.
fn cover(picture: &mut RgbaImage, item: [f64; 4], area: kurbo::Rect, scale: f64) {
    const INK: [u8; 4] = [0x80, 0x80, 0x80, 0xff];
    let px = |v: f64, most: u32| -> u32 { (v * scale).clamp(0.0, f64::from(most)) as u32 };
    let x0 = px((item[0] - area.x0).floor(), picture.width);
    let y0 = px((item[1] - area.y0).floor(), picture.height);
    let x1 = px((item[0] + item[2] - area.x0).ceil(), picture.width);
    let y1 = px((item[1] + item[3] - area.y0).ceil(), picture.height);
    for y in y0..y1 {
        for x in x0..x1 {
            let at = (y as usize * picture.width as usize + x as usize) * 4;
            picture.data[at..at + 4].copy_from_slice(&INK);
        }
    }
}

// ----- app_snapshot ---------------------------------------------------------

/// `s3.10`: part of snapshot 3, after its first 10 nodes.
pub(crate) fn cursor(epoch: u64, skip: usize) -> String {
    format!("s{epoch}.{skip}")
}

pub(crate) fn parse_cursor(text: &str) -> Result<(u64, usize), SurfaceError> {
    let parts = text
        .strip_prefix('s')
        .and_then(|rest| rest.split_once('.'))
        .and_then(|(epoch, skip)| Some((epoch.parse().ok()?, skip.parse().ok()?)));
    parts.ok_or_else(|| {
        SurfaceError::invalid_argument(format!(
            "`{}` is not a cursor. A cursor is the `next_cursor` of a cut `app_snapshot`, \
             such as `s3.400`",
            clip(text, 40)
        ))
    })
}

/// The answer for one part of an outline. `epoch` is `None` for the
/// `ljos://app/tree` resource, which is not a snapshot to continue or to
/// compare with.
pub(crate) fn snapshot_reply(
    outline: Outline,
    epoch: Option<u64>,
    per_copy: usize,
    picture: Option<Vec<u8>>,
) -> ToolReply {
    let next = match (epoch, outline.next_skip) {
        (Some(epoch), Some(skip)) => Some(cursor(epoch, skip)),
        _ => None,
    };
    let mut value = Map::new();
    if let Some(epoch) = epoch {
        value.insert("epoch".into(), json!(epoch));
    }
    value.insert("nodes".into(), json!(outline.nodes));
    value.insert("total".into(), json!(outline.total));
    value.insert("truncated".into(), json!(outline.truncated));
    if let Some(next) = &next {
        value.insert("next_cursor".into(), json!(next));
    }
    let mut text = outline.text;
    let _ = match (epoch, &next, outline.truncated) {
        (Some(epoch), Some(next), _) => write!(
            text,
            "\n(snapshot {epoch}: {} of {} items listed; pass cursor \"{next}\" for the next part)",
            outline.nodes, outline.total
        ),
        (Some(epoch), None, _) => write!(text, "\n(snapshot {epoch}, {} items)", outline.nodes),
        (None, _, true) => write!(
            text,
            "\n({} of {} items listed; `app_snapshot` gives the rest in parts)",
            outline.nodes, outline.total
        ),
        (None, _, false) => write!(text, "\n({} items)", outline.nodes),
    };
    // The outline is the same in both copies. In JSON it is longer (every
    // quote and line break is escaped); when that does not fit, the text
    // copy is the one that has it.
    let body = text.rsplit_once('\n').map_or("", |(body, _)| body);
    let escaped = serde_json::to_string(body).map_or(usize::MAX, |s| s.len());
    if escaped + 256 <= per_copy {
        value.insert("outline".into(), json!(body));
    } else {
        value.insert("outline_in_text".into(), json!(true));
    }
    let mut reply = ToolReply::from(Value::Object(value)).with_text(text);
    if let Some(png) = picture {
        reply = reply.with_image("image/png", png);
    }
    reply
}

/// The answer for `since`: what changed between snapshot `since` and the
/// one taken now, which is `epoch`.
pub(crate) fn delta_reply(delta: Delta, since: u64, epoch: u64) -> ToolReply {
    let value = json!({
        "epoch": epoch,
        "since": since,
        "delta": delta.text,
        "changed": delta.changed,
        "added": delta.added,
        "removed": delta.removed,
        "truncated": delta.truncated,
    });
    let text = format!(
        "{}\n(snapshot {epoch} against snapshot {since}: {} changed, {} added, {} removed; pass \
         since: {epoch} next time)",
        delta.text, delta.changed, delta.added, delta.removed
    );
    ToolReply::from(value).with_text(text)
}

// ----- app_metrics ----------------------------------------------------------

/// Where the time went, from the frames the host reported and the
/// inspector's own work.
pub(crate) fn metrics(
    engine: &Engine,
    stats: &FrameStats,
    paint: &super::paint::PaintStats,
    frames: &super::frames::FrameLog,
    queued: usize,
    capacity: usize,
    as_json_text: bool,
) -> ToolReply {
    let mut lanes = Map::new();
    for (i, name) in LANE_NAMES.iter().enumerate() {
        lanes.insert(
            (*name).to_string(),
            json!({ "time_us": micros(stats.per_lane[i]), "pending": stats.pending[i] }),
        );
    }
    let owners: Vec<(String, Value, std::time::Duration)> = stats
        .owners(8)
        .into_iter()
        .map(|(proc, time)| {
            (
                owner_name(engine, proc),
                json!(format!("{}.{}", proc.index(), proc.generation())),
                time,
            )
        })
        .collect();
    let mut value = json!({
        "frames": {
            "count": stats.frames,
            "overruns": stats.overruns,
            "last_us": micros(stats.last_frame),
            "max_us": micros(stats.max_frame),
            "total_us": micros(stats.total_frame),
        },
        "jobs": {
            "polled": stats.jobs_polled,
            "done": stats.jobs_done,
            "yielded": stats.jobs_yielded,
            "failed": stats.jobs_failed,
        },
        "lanes": Value::Object(lanes),
        "owners": owners
            .iter()
            .map(|(name, proc, time)| json!({ "name": name, "proc": proc, "time_us": micros(*time) }))
            .collect::<Vec<_>>(),
        "messages": { "drained": stats.drained, "drain_time_us": micros(stats.drain_time) },
        "objects": engine.object_count(),
        "inbox": { "capacity": capacity, "queued": queued },
    });
    value["paint"] = paint.json();
    let (recent, slow) = frames.json(engine);
    value["recent"] = recent;
    value["slow"] = slow;
    if as_json_text {
        let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
        return ToolReply::from(value).with_text(text);
    }
    let ms = |d: std::time::Duration| format!("{:.2} ms", d.as_secs_f64() * 1000.0);
    let mut text = format!(
        "frames {} ({} over budget), last {}, longest {}, all {}\n",
        stats.frames,
        stats.overruns,
        ms(stats.last_frame),
        ms(stats.max_frame),
        ms(stats.total_frame)
    );
    text.push_str("time per lane:");
    for (i, name) in LANE_NAMES.iter().enumerate() {
        let _ = write!(text, " {name} {}", ms(stats.per_lane[i]));
        if stats.pending[i] > 0 {
            let _ = write!(text, " ({} waiting)", stats.pending[i]);
        }
        if i + 1 < LANE_NAMES.len() {
            text.push(',');
        }
    }
    text.push_str("\ntime per owner, largest first:");
    if owners.is_empty() {
        text.push_str(" (none recorded yet)");
    }
    for (name, _, time) in &owners {
        let _ = write!(text, "\n  {name}  {}", ms(*time));
    }
    let _ = write!(
        text,
        "\njobs: {} polled, {} done, {} yielded, {} failed; messages: {} in {}\n{} objects",
        stats.jobs_polled,
        stats.jobs_done,
        stats.jobs_yielded,
        stats.jobs_failed,
        stats.drained,
        ms(stats.drain_time),
        engine.object_count()
    );
    text.push_str(&paint.text());
    text.push_str(&frames.text(engine));
    ToolReply::from(value).with_text(text)
}

/// The QML files a binding's text is read from, each read once an answer.
/// A file that cannot be read (an application shipped without its
/// sources, a file kept in memory) gives no text.
#[derive(Default)]
struct Sources {
    files: std::collections::HashMap<String, Option<String>>,
}

impl Sources {
    /// A file larger than this is not read for the text of one binding.
    const MOST: u64 = 4 << 20;

    fn text(&mut self, url: &str, bytes: std::ops::Range<u32>) -> Option<String> {
        let file = self.files.entry(url.to_owned()).or_insert_with(|| {
            let path = ljos_engine::url_path(url)?;
            if std::fs::metadata(&path).ok()?.len() > Self::MOST {
                return None;
            }
            std::fs::read_to_string(path).ok()
        });
        let text = file
            .as_deref()?
            .get(bytes.start as usize..bytes.end as usize)?;
        Some(text.trim().to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bindings_text_is_read_from_the_file_it_was_written_in() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Main.qml");
        std::fs::write(&path, "Item { width: parent.width / 2 }").unwrap();
        let url = ljos_engine::file_url(&path);
        let mut sources = Sources::default();
        assert_eq!(
            sources.text(&url, 14..30).as_deref(),
            Some("parent.width / 2")
        );
        // Bytes that are not in the file, or a file that is not there, give
        // no text rather than a wrong one.
        assert_eq!(sources.text(&url, 14..300), None);
        assert_eq!(sources.text("Scenario.qml", 0..4), None);
    }

    #[test]
    fn whole_numbers_are_integers() {
        assert_eq!(num(40.0).to_string(), "40");
        assert_eq!(num(0.25).to_string(), "0.25");
        assert_eq!(num(1.0 / 3.0).to_string(), "0.33");
        assert_eq!(num(f64::NAN), Value::Null);
    }

    #[test]
    fn a_cursor_reads_back() {
        assert_eq!(parse_cursor(&cursor(3, 400)).unwrap(), (3, 400));
        for bad in ["", "3.400", "s3", "s3.x", "sx.1", "e12"] {
            let e = parse_cursor(bad).unwrap_err();
            assert_eq!(e.code, ErrorCode::InvalidArgument, "{bad}");
            assert!(e.message.contains("next_cursor"), "{}", e.message);
        }
    }

    #[test]
    fn a_part_of_a_picture_is_cut_out_and_a_field_is_painted_over() {
        let mut full = RgbaImage::filled(8, 4, [255, 0, 0, 255]);
        // One green pixel at (5, 2).
        let at = (2 * 8 + 5) * 4;
        full.data[at..at + 4].copy_from_slice(&[0, 255, 0, 255]);
        let area = kurbo::Rect::new(4.0, 1.0, 8.0, 4.0);
        let mut part = crop(&full, area, 1.0, 1280);
        assert_eq!((part.width, part.height), (4, 3));
        assert_eq!(part.pixel(1, 1), [0, 255, 0, 255]);
        cover(&mut part, [5.0, 2.0, 1.0, 1.0], area, 1.0);
        assert_eq!(part.pixel(1, 1), [0x80, 0x80, 0x80, 0xff]);
        assert_eq!(part.pixel(0, 0), [255, 0, 0, 255]);
        // A field that is not in the part changes nothing and is no error.
        cover(&mut part, [0.0, 0.0, 2.0, 1.0], area, 1.0);
        assert_eq!(part.pixel(0, 0), [255, 0, 0, 255]);
    }

    #[test]
    fn an_outline_too_long_for_json_stays_in_the_text() {
        let outline = Outline {
            text: "- window [e1]\n".repeat(100).trim_end().to_string(),
            nodes: 100,
            total: 250,
            truncated: true,
            next_skip: Some(100),
        };
        let small = snapshot_reply(outline.clone(), Some(2), 600, None);
        assert_eq!(small.structured["outline_in_text"], json!(true));
        assert!(small.structured.get("outline").is_none());
        assert_eq!(small.structured["next_cursor"], json!("s2.100"));
        assert!(
            small
                .text
                .as_deref()
                .unwrap()
                .ends_with("for the next part)")
        );
        let large = snapshot_reply(outline, Some(2), 20_000, None);
        assert!(
            large.structured["outline"]
                .as_str()
                .unwrap()
                .starts_with("- window")
        );
        assert!(
            !large.structured["outline"]
                .as_str()
                .unwrap()
                .contains("snapshot 2")
        );
    }
}
