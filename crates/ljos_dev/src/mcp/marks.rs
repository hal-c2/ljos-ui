//! `app_objects`' `mark` and `since_mark`: what a step leaves behind.
//!
//! A client marks a point, the user does something (opens and closes a
//! dialog, pushes and pops a page), and `since_mark` lists what was made
//! since the mark and is still alive: the component instances and the
//! objects, by where each is declared, with the bytes they hold. What an
//! action that undoes itself leaves listed is a leak, or a cache.
//!
//! A mark is the set of objects alive when it was taken; an object's
//! handle is never reused, so one not in the set was made since. Before
//! comparing, the script collector runs, so that what only waits for it
//! is not listed, and objects waiting to be deleted are left out.
//!
//! One mark is kept for the clients that give no `holder`, the next
//! replacing it, and one for each holder (at most [`MAX_HOLDERS`]), gone
//! when the holder's watch lapses. Nothing runs between the calls.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::time::Instant;

use ljos_engine::{Engine, ObjectFlags, ObjectId};
use ljos_mcp_core::{SurfaceError, ToolReply};
use serde_json::{Value, json};

use super::objects::handle;
use super::observe::declared_at;
use super::tools::text_property;

/// The most clients that keep a mark at once.
const MAX_HOLDERS: usize = 16;
/// The most places listed unless told otherwise.
const DEFAULT_MAX: usize = 50;
/// The most handles listed for one place or component.
const MAX_HANDLES: usize = 5;

struct Mark {
    at: Instant,
    alive: HashSet<ObjectId>,
    script: Option<usize>,
}

#[derive(Default)]
pub(crate) struct Marks {
    unheld: Option<Mark>,
    held: HashMap<String, Mark>,
}

/// The objects made at one place since the mark.
#[derive(Default)]
struct Place {
    count: usize,
    bytes: usize,
    handles: Vec<String>,
    names: Vec<String>,
}

fn alive(engine: &Engine, obj: ObjectId) -> bool {
    engine
        .flags(obj)
        .is_some_and(|f| !f.contains(ObjectFlags::PENDING_DELETE))
}

impl Marks {
    /// Marks now, for `holder` or for whoever gives none.
    pub(crate) fn mark(
        &mut self,
        engine: &Engine,
        holder: Option<&str>,
    ) -> Result<ToolReply, SurfaceError> {
        if let Some(h) = holder
            && !self.held.contains_key(h)
            && self.held.len() >= MAX_HOLDERS
        {
            return Err(SurfaceError::invalid_argument(format!(
                "`app_objects`: {MAX_HOLDERS} clients keep a mark already"
            )));
        }
        engine.collect_garbage();
        let alive: HashSet<ObjectId> = engine
            .object_ids()
            .into_iter()
            .filter(|&o| alive(engine, o))
            .collect();
        let count = alive.len();
        let mark = Mark {
            at: Instant::now(),
            alive,
            script: engine.script_heap_bytes(),
        };
        match holder {
            Some(h) => {
                self.held.insert(h.to_owned(), mark);
            }
            None => self.unheld = Some(mark),
        }
        Ok(ToolReply::from(json!({ "marked": count }))
            .with_text(format!("marked: {count} objects alive")))
    }

    /// Forgets `holder`'s mark.
    pub(crate) fn end(&mut self, holder: &str) {
        self.held.remove(holder);
    }

    /// What was made since the mark and is alive.
    pub(crate) fn since(
        &self,
        engine: &Engine,
        holder: Option<&str>,
        max: Option<usize>,
    ) -> Result<ToolReply, SurfaceError> {
        let mark = match holder {
            Some(h) => self.held.get(h),
            None => self.unheld.as_ref(),
        }
        .ok_or_else(|| {
            SurfaceError::invalid_argument(
                "`app_objects`: no mark to compare with; `mark: true` takes one",
            )
        })?;
        let max = max.unwrap_or(DEFAULT_MAX);
        engine.collect_garbage();
        let made: Vec<ObjectId> = engine
            .object_ids()
            .into_iter()
            .filter(|o| !mark.alive.contains(o) && alive(engine, *o))
            .collect();
        let gone = mark.alive.iter().filter(|&&o| !engine.is_alive(o)).count();
        let new: HashSet<ObjectId> = made.iter().copied().collect();

        let (places, bytes) = engine.untrack(|| {
            let mut places: BTreeMap<(String, String), Place> = BTreeMap::new();
            let mut bytes = 0;
            for &obj in &made {
                let ty = engine
                    .meta_of(obj)
                    .map_or_else(|| "object".to_owned(), |m| m.name.to_string());
                let at = declared_at(engine, obj).unwrap_or_else(|| "(made by Rust)".to_owned());
                let b = engine.object_bytes(obj).unwrap_or(0);
                bytes += b;
                let p = places.entry((at, ty)).or_default();
                p.count += 1;
                p.bytes += b;
                if p.handles.len() < MAX_HANDLES {
                    p.handles.push(handle(obj));
                    if let Some(n) =
                        text_property(engine, obj, "objectName").filter(|n| !n.is_empty())
                    {
                        p.names.push(n);
                    }
                }
            }
            (places, bytes)
        });
        let mut places: Vec<((String, String), Place)> = places.into_iter().collect();
        places.sort_by(|a, b| b.1.bytes.cmp(&a.1.bytes).then(b.1.count.cmp(&a.1.count)));
        let place_count = places.len();
        places.truncate(max);

        let components: Vec<Value> = engine
            .components()
            .into_iter()
            .filter_map(|c| {
                let fresh: Vec<ObjectId> = c
                    .instances
                    .iter()
                    .copied()
                    .filter(|o| new.contains(o))
                    .collect();
                (!fresh.is_empty()).then(|| {
                    json!({
                        "component": c.id,
                        "name": c.name,
                        "kind": c.kind.name(),
                        "url": c.url,
                        "instances": fresh.len(),
                        "handles": fresh.iter().take(MAX_HANDLES).map(|&o| handle(o)).collect::<Vec<_>>(),
                        "at": fresh.first().and_then(|&o| declared_at(engine, o)),
                    })
                })
            })
            .collect();

        let script = engine.script_heap_bytes();
        let script_delta = match (mark.script, script) {
            (Some(a), Some(b)) => Some(b as i64 - a as i64),
            (None, Some(b)) => Some(b as i64),
            _ => None,
        };
        let since_ms = mark.at.elapsed().as_millis() as u64;
        let mut text = format!(
            "since the mark {:.1} s ago: {} objects made and still alive, about {:.1} KiB; {gone} of those marked are gone",
            since_ms as f64 / 1000.0,
            made.len(),
            bytes as f64 / 1024.0
        );
        if let Some(d) = script_delta {
            let _ = write!(text, "; script heap {:+.1} KiB", d as f64 / 1024.0);
        }
        text.push('\n');
        if !components.is_empty() {
            text.push_str("component instances:\n");
            for c in &components {
                let _ = writeln!(
                    text,
                    "  {} {} ×{} at {}",
                    c["kind"].as_str().unwrap_or(""),
                    c["name"].as_str().unwrap_or(""),
                    c["instances"],
                    c["at"].as_str().unwrap_or("?")
                );
            }
        }
        if !places.is_empty() {
            text.push_str("objects, by where they are declared:\n");
        }
        for ((at, ty), p) in &places {
            let _ = write!(text, "  {at} {ty} ×{} {} bytes", p.count, p.bytes);
            if !p.names.is_empty() {
                let _ = write!(text, " ({})", p.names.join(", "));
            }
            text.push('\n');
        }
        if place_count > places.len() {
            let _ = writeln!(text, "  … {} more places", place_count - places.len());
        }
        if made.is_empty() {
            text.push_str("nothing made since the mark is alive\n");
        }
        Ok(ToolReply::from(json!({
            "since_ms": since_ms,
            "marked": mark.alive.len(),
            "made": made.len(),
            "gone": gone,
            "bytes": bytes,
            "script_heap_delta_bytes": script_delta,
            "components": components,
            "places": places.iter().map(|((at, ty), p)| json!({
                "at": at,
                "type": ty,
                "count": p.count,
                "bytes": p.bytes,
                "handles": p.handles,
                "object_names": p.names,
            })).collect::<Vec<_>>(),
            "places_left_out": place_count - places.len(),
        }))
        .with_text(text.trim_end().to_owned()))
    }
}
