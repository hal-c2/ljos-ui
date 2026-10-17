//! `app_objects` and handles: every object of the scene, shown or not.
//!
//! The outline is the accessibility tree, which has the items a person
//! can perceive. Debugging needs the rest too: an item without a role, a
//! Timer, a model, an animation. This walks the objects themselves. Each
//! one is named by a handle, `@index.version`, made from the object's
//! generational key: it needs no table to be kept, stays the same for as
//! long as the object lives, and reads as stale once it is gone. Every
//! tool that takes a target takes a handle.

use std::collections::HashSet;
use std::fmt::Write as _;

use ljos_engine::{Engine, ObjectId};
use ljos_mcp_core::{ErrorCode, SurfaceError, ToolReply};
use ljos_quick::QuickScene;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use slotmap::{Key, KeyData};

use super::tools::{flag, text_property};

pub const OBJECTS: &str = "\
Every object below `root` (a target; default the scene's root), items or not: Timers, models, \
animations, items without a role. Each has a `@handle` any tool takes as its target, its type, \
id, objectName and depth. `depth` limits how far down; `hidden` counts what it left out.";

/// The most objects one answer lists unless told otherwise, and at all.
const DEFAULT_MAX: usize = 500;
pub const MAX_OBJECTS: usize = 5_000;

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObjectsArgs {
    /// The object to start at: a `@handle` or a locator. Default: the
    /// scene's root.
    #[serde(default)]
    pub root: Option<String>,
    /// How many levels below the root to list (0 is the root alone).
    #[serde(default)]
    pub depth: Option<u32>,
    /// The most objects to list (default 500, at most 5000).
    #[serde(default)]
    pub max: Option<usize>,
}

/// The handle of `obj`: `@12.3` is the object in place 12, of the third
/// object that had that place.
pub(crate) fn handle(obj: ObjectId) -> String {
    let ffi = obj.data().as_ffi();
    format!("@{}.{}", ffi & 0xffff_ffff, ffi >> 32)
}

/// The object a handle names, if `what` is written as one. A handle of an
/// object that is gone is stale.
pub(crate) fn parse_handle(engine: &Engine, what: &str) -> Option<Result<ObjectId, SurfaceError>> {
    let rest = what.trim().strip_prefix('@')?;
    let parsed = rest
        .split_once('.')
        .and_then(|(i, v)| Some((i.parse::<u32>().ok()?, v.parse::<u32>().ok()?)));
    let Some((index, version)) = parsed else {
        return Some(Err(SurfaceError::invalid_argument(format!(
            "`{what}` is not a handle. A handle is `@` and two numbers, as `app_objects` and \
             `app_processes` give them (`@12.3`)"
        ))));
    };
    let obj: ObjectId = KeyData::from_ffi((u64::from(version) << 32) | u64::from(index)).into();
    if engine.is_alive(obj) {
        Some(Ok(obj))
    } else {
        Some(Err(SurfaceError::new(
            ErrorCode::StaleRef,
            format!(
                "`{what}` is gone: the object was destroyed. `app_objects` lists the objects \
                 there are now"
            ),
        )))
    }
}

/// What is below `obj` in the tree: the objects it owns that no item
/// holds, then its items. An object an item holds is listed under that
/// item, which is where it is drawn.
fn below(engine: &Engine, obj: ObjectId) -> Vec<ObjectId> {
    let mut out: Vec<ObjectId> = engine
        .children(obj)
        .into_iter()
        .filter(|c| engine.visual_parent(*c).is_none_or(|p| !engine.is_alive(p)))
        .collect();
    out.extend(engine.visual_children(obj));
    out
}

/// Whether answering needs the scene looked at: a root given by locator
/// is found in the outline.
pub(crate) fn needs_scene(args: &ObjectsArgs) -> bool {
    args.root
        .as_deref()
        .is_some_and(|r| !r.trim_start().starts_with('@'))
}

/// Answers `app_objects`, on the UI thread, from `found`: the object a
/// locator in `root` named.
pub(crate) fn objects(
    engine: &Engine,
    scene: &QuickScene,
    args: &ObjectsArgs,
    found: Option<ObjectId>,
    per_copy: usize,
) -> Result<ToolReply, SurfaceError> {
    let root = match (found, &args.root) {
        (Some(obj), _) => obj,
        (None, Some(what)) => match parse_handle(engine, what) {
            Some(found) => found?,
            None => {
                return Err(SurfaceError::new(
                    ErrorCode::Internal,
                    "a locator root was not looked for",
                ));
            }
        },
        (None, None) => match scene.root() {
            Some(root) => root,
            None => {
                return Ok(
                    ToolReply::from(json!({ "objects": [], "truncated": false }))
                        .with_text("the scene has no root object"),
                );
            }
        },
    };
    let max = args.max.unwrap_or(DEFAULT_MAX).clamp(1, MAX_OBJECTS);
    let depth_limit = args.depth.unwrap_or(u32::MAX);
    let (mut walked, truncated) = engine.untrack(|| {
        let mut seen = HashSet::new();
        let mut out: Vec<(ObjectId, u32, usize, usize)> = Vec::new();
        let mut stack = vec![(root, 0u32)];
        let mut truncated = false;
        while let Some((obj, depth)) = stack.pop() {
            if !seen.insert(obj) || !engine.is_alive(obj) {
                continue;
            }
            if out.len() == max {
                truncated = true;
                break;
            }
            let kids: Vec<ObjectId> = below(engine, obj)
                .into_iter()
                .filter(|c| !seen.contains(c))
                .collect();
            let hidden = if depth >= depth_limit { kids.len() } else { 0 };
            out.push((obj, depth, kids.len(), hidden));
            if depth < depth_limit {
                stack.extend(kids.into_iter().rev().map(|c| (c, depth + 1)));
            }
        }
        (out, truncated)
    });
    let objs: Vec<ObjectId> = walked.iter().map(|w| w.0).collect();
    let ids = engine.inspect_ids(&objs);
    let rows: Vec<(Value, String)> = engine.untrack(|| {
        walked
            .drain(..)
            .zip(ids)
            .map(|((obj, depth, children, hidden), id)| {
                let ty = engine
                    .meta_of(obj)
                    .map_or_else(|| "object".to_owned(), |m| m.name.to_string());
                let item = ljos_quick::is_item(engine, obj);
                let name = text_property(engine, obj, "objectName");
                let h = handle(obj);
                let mut row = json!({
                    "handle": h,
                    "depth": depth,
                    "type": ty,
                    "id": id,
                    "object_name": name,
                    "item": item,
                    "children": children,
                });
                if item {
                    row["visible"] = json!(flag(engine, obj, "visible"));
                }
                if hidden > 0 {
                    row["hidden"] = json!(hidden);
                }
                let mut line = format!("{}{ty}", "  ".repeat(depth as usize));
                if let Some(id) = &id {
                    let _ = write!(line, "#{id}");
                }
                if let Some(name) = &name {
                    let _ = write!(line, " \"{name}\"");
                }
                let _ = write!(line, " {h}");
                if hidden > 0 {
                    let _ = write!(line, " (+{hidden} below)");
                }
                (row, line)
            })
            .collect()
    });
    let mut keep = rows.len();
    loop {
        let cut = keep < rows.len();
        let value = json!({
            "objects": rows[..keep].iter().map(|(v, _)| v.clone()).collect::<Vec<_>>(),
            "truncated": truncated || cut,
        });
        let mut text: String = rows[..keep]
            .iter()
            .map(|(_, l)| l.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if truncated || cut {
            let _ = write!(
                text,
                "\n(more objects than fit: give a `root` lower down, or a `depth`)"
            );
        }
        if keep <= 1 || (text.len() <= per_copy && value.to_string().len() <= per_copy) {
            return Ok(ToolReply::from(value).with_text(text));
        }
        keep -= keep.div_ceil(4);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_handle_is_its_place_and_version() {
        let obj: ObjectId = KeyData::from_ffi((3u64 << 32) | 12).into();
        assert_eq!(handle(obj), "@12.3");
    }
}
