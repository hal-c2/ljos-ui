//! `app_metrics` with `memory`: where the application's memory is, as an
//! observer's allocator view shows it.
//!
//! What has a running count (the script heap, the process's resident
//! pages, the image cache) is read; the objects are walked once, and the
//! text caches, each scene and each renderer add up their own tables.
//! Nothing is kept up to date for it, so an application at rest stays at
//! rest. Bytes no allocator counts for us are estimates and say so.
//!
//! `owners` puts it together by owner, each with bytes and a count, so
//! that it adds up to what is resident: the program and its libraries,
//! each owner, what the allocator holds free, and what is left, which is
//! not attributed. What a renderer keeps in textures is listed but left
//! out of the sum: where there is a GPU it lives there, not in the
//! process.

use std::fmt::Write as _;

use ljos_engine::{Engine, ObjectId};
use ljos_mcp_core::ToolReply;
use ljos_quick::SceneMemory;
use ljos_render::BackendMemory;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::objects::handle;
use super::observe::Resident;

/// The most types one answer lists unless told otherwise.
const DEFAULT_MAX: usize = 20;
/// The most types one answer may list.
pub const MOST_TYPES: usize = 1000;

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryArgs {
    /// Only the objects of this type, by the name `app_objects` gives it.
    #[serde(default, rename = "type")]
    pub r#type: Option<String>,
    /// The most types to list, the most bytes first (default 20, at most
    /// 1000). The rest are counted in `types_left_out`.
    #[serde(default)]
    pub max: Option<usize>,
}

/// What one window holds, as its host tells it.
#[derive(Clone, Copy, Debug)]
pub struct WindowMemory {
    /// The QML `Window` it shows; `None` for a root the host gave a window.
    pub window: Option<ObjectId>,
    pub scene: SceneMemory,
    /// What its renderer keeps; `None` where nothing draws it.
    pub render: Option<BackendMemory>,
}

fn render_json(r: &BackendMemory) -> Value {
    json!({
        "meshes": r.meshes,
        "mesh_bytes": r.mesh_bytes,
        "glyphs": r.glyphs,
        "glyph_atlas_bytes": r.glyph_atlas_bytes,
        "textures": r.textures,
        "texture_bytes": r.texture_bytes,
        "pages": r.pages,
        "page_bytes": r.page_bytes,
        "shape_textures": r.shape_textures,
        "shape_texture_bytes": r.shape_texture_bytes,
        "bytes": r.bytes(),
        "approximate": true,
    })
}

fn window_json(w: &WindowMemory) -> Value {
    let s = &w.scene;
    json!({
        "window": w.window.map(handle),
        "scene": {
            "items": s.items,
            "nodes": s.tree.nodes,
            "lists": s.tree.lists,
            "ops": s.tree.ops,
            "bytes": s.bytes,
            "approximate": true,
        },
        "render": w.render.as_ref().map(render_json),
    })
}

fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

/// What glibc's allocator holds: in use by anyone, and freed but kept.
#[derive(Clone, Copy, Debug)]
struct Heap {
    in_use: u64,
    free: u64,
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn heap() -> Option<Heap> {
    // SAFETY: mallinfo2 reads the allocator's own counts under its locks
    // and takes nothing.
    let m = unsafe { libc::mallinfo2() };
    Some(Heap {
        in_use: (m.uordblks + m.hblkhd) as u64,
        free: m.fordblks as u64,
    })
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn heap() -> Option<Heap> {
    None
}

/// One owner: what it is, its bytes, how many of what it counts.
fn owner(key: &str, label: &str, bytes: usize, count: usize, of: &str, approximate: bool) -> Value {
    json!({
        "owner": key,
        "label": label,
        "bytes": bytes,
        "count": count,
        "of": of,
        "approximate": approximate,
    })
}

/// The rows of the models and about what they hold: a ListModel counts
/// its own rows, a model written in Rust a value a cell. A model another
/// stands for (a DelegateModel over a ListModel) is counted once.
fn model_memory(engine: &Engine) -> (usize, usize) {
    let ptr = |m: &std::rc::Rc<dyn ljos_engine::ItemModel>| std::rc::Rc::as_ptr(m).cast::<()>();
    let mut seen = std::collections::HashSet::new();
    let (mut rows, mut bytes) = (0, 0);
    engine.untrack(|| {
        let objects = engine.object_ids();
        for &obj in &objects {
            if let Some((r, b)) = engine.list_model_memory(obj) {
                if let Some(m) = engine.object_model(obj) {
                    seen.insert(ptr(&m));
                }
                rows += r;
                bytes += b;
            }
        }
        for &obj in &objects {
            if engine.is_list_model(obj) {
                continue;
            }
            if let Some(m) = engine.object_model(obj)
                && seen.insert(ptr(&m))
            {
                let r = m.row_count();
                rows += r;
                bytes +=
                    r * m.role_names().len().max(1) * std::mem::size_of::<ljos_engine::Value>();
            }
        }
    });
    (rows, bytes)
}

/// The answer, for the windows the host told of.
pub(crate) fn memory(engine: &Engine, windows: &[WindowMemory], args: &MemoryArgs) -> ToolReply {
    let max = args.max.unwrap_or(DEFAULT_MAX).clamp(1, MOST_TYPES);
    let all = engine.object_memory();
    let (count, bytes) = all
        .iter()
        .fold((0, 0), |(c, b), t| (c + t.count, b + t.bytes));
    let kept: Vec<_> = all
        .iter()
        .filter(|t| args.r#type.as_deref().is_none_or(|name| t.name == name))
        .collect();
    let left_out = kept.len().saturating_sub(max);
    let types: Vec<Value> = kept
        .iter()
        .take(max)
        .map(|t| json!({ "type": t.name, "count": t.count, "bytes": t.bytes }))
        .collect();

    let script = engine.script_heap_bytes();
    let nodes = engine.node_count();
    let graph = engine.graph_bytes();
    let text = ljos_quick::text_engine_in_use().map(|t| t.borrow().memory());
    let images = ljos_quick::ImageStore::of(engine);
    let image_bytes = images.bytes();
    let process = Resident::now();
    let script_stats = engine.script_memory();
    let compiled = engine.compiled_memory();
    let (model_rows, model_bytes) = model_memory(engine);
    let heap = heap();

    let scenes: usize = windows.iter().map(|w| w.scene.bytes).sum();
    let renders: usize = windows
        .iter()
        .filter_map(|w| w.render.as_ref())
        .map(BackendMemory::bytes)
        .sum();
    let counted = bytes
        + script.unwrap_or(0)
        + graph
        + text.map_or(0, |t| t.bytes)
        + image_bytes
        + scenes
        + renders;

    // By owner. The script's is QuickJS's own count, with its objects,
    // when it can be asked (not inside a run); else what its allocator
    // holds.
    let render_sum = |f: fn(&BackendMemory) -> usize| -> usize {
        windows
            .iter()
            .filter_map(|w| w.render.as_ref())
            .map(f)
            .sum()
    };
    let scene_nodes: usize = windows.iter().map(|w| w.scene.tree.nodes).sum();
    let mut owners = vec![
        match script_stats {
            Some(s) => owner("script", "JS heap", s.bytes, s.objects, "objects", false),
            None => owner(
                "script",
                "JS heap",
                script.unwrap_or(0),
                0,
                "objects",
                false,
            ),
        },
        owner(
            "objects",
            "objects and items",
            bytes,
            count,
            "objects",
            true,
        ),
        owner("reactive", "reactive graph", graph, nodes, "nodes", true),
        owner("scene", "scene nodes", scenes, scene_nodes, "nodes", true),
        owner(
            "text",
            "text layouts",
            text.map_or(0, |t| t.bytes),
            text.map_or(0, |t| t.layouts),
            "layouts",
            true,
        ),
        owner(
            "glyph_atlas",
            "glyph atlas",
            render_sum(|r| r.glyph_atlas_bytes),
            render_sum(|r| r.glyphs),
            "glyphs",
            true,
        ),
        owner(
            "meshes",
            "meshes",
            render_sum(|r| r.mesh_bytes),
            render_sum(|r| r.meshes),
            "meshes",
            true,
        ),
        owner(
            "images",
            "image cache",
            image_bytes,
            images.len(),
            "images",
            false,
        ),
        owner(
            "models",
            "model storage",
            model_bytes,
            model_rows,
            "rows",
            true,
        ),
        owner(
            "compiled",
            "compiled units",
            compiled.bytes,
            compiled.files,
            "files",
            true,
        ),
    ];
    let attributed: u64 = owners
        .iter()
        .map(|o| o["bytes"].as_u64().unwrap_or(0))
        .sum();
    let textures = owner(
        "textures",
        "textures",
        render_sum(|r| r.texture_bytes + r.page_bytes + r.shape_texture_bytes),
        render_sum(|r| r.textures + r.pages + r.shape_textures),
        "textures",
        true,
    );
    // What is resident, less the program and its libraries, the owners
    // and what the allocator holds free, is what nobody told of: other
    // libraries' heaps, stacks, the driver's maps.
    let mut reconciled = Value::Null;
    if let Some(p) = process {
        let free = heap.map_or(0, |h| h.free);
        let gap = p.own() as i64 - attributed as i64 - free as i64;
        owners.push(owner(
            "program",
            "program and libraries",
            p.shared as usize,
            0,
            "",
            false,
        ));
        if heap.is_some() {
            owners.push(owner(
                "allocator_free",
                "freed, kept by the allocator",
                free as usize,
                0,
                "",
                false,
            ));
        }
        owners.push(owner(
            "not_attributed",
            "not attributed",
            gap.max(0) as usize,
            0,
            "",
            true,
        ));
        reconciled = json!({
            "resident_bytes": p.rss,
            "attributed_bytes": attributed,
            "not_attributed_bytes": gap,
            "not_attributed_share": if p.rss == 0 { 0.0 } else { gap.max(0) as f64 / p.rss as f64 },
            "heap_in_use_bytes": heap.map(|h| h.in_use),
        });
    }
    owners.push({
        let mut t = textures;
        t["gpu"] = json!(true);
        t
    });

    let value = json!({
        "objects": {
            "count": count,
            "bytes": bytes,
            "approximate": true,
            "types": types,
            "types_left_out": left_out,
        },
        "script": script.map(|b| json!({ "bytes": b, "approximate": false })),
        "reactive": { "nodes": nodes, "bytes": graph, "approximate": true },
        "text": text.map(|t| json!({
            "layouts": t.layouts,
            "shapes": t.shapes,
            "paragraphs": t.paragraphs,
            "styles": t.styles,
            "glyphs": t.glyphs,
            "bytes": t.bytes,
            "approximate": true,
        })),
        "images": {
            "count": images.len(),
            "bytes": image_bytes,
            "resident_bytes": images.resident_bytes(),
            "budget": images.budget(),
        },
        "windows": windows.iter().map(window_json).collect::<Vec<_>>(),
        "process": process.map(|p| json!({
            "resident_bytes": p.rss,
            "own_bytes": p.own(),
            "virtual_bytes": p.virt,
        })),
        "counted_bytes": counted,
        "owners": owners,
        "reconciled": reconciled,
    });

    let mut out = String::new();
    if let Some(p) = process {
        let _ = writeln!(
            out,
            "process: {:.1} MiB resident, {:.1} MiB of it its own; {:.1} MiB counted below",
            mib(p.rss),
            mib(p.own()),
            mib(counted as u64)
        );
    }
    out.push_str("by owner:\n");
    for o in value["owners"].as_array().into_iter().flatten() {
        let n = o["count"].as_u64().unwrap_or(0);
        let _ = writeln!(
            out,
            "  {:<30} {:>10.2} MiB{}{}",
            o["label"].as_str().unwrap_or(""),
            mib(o["bytes"].as_u64().unwrap_or(0)),
            if n > 0 || !o["of"].as_str().unwrap_or("").is_empty() {
                format!("  {n} {}", o["of"].as_str().unwrap_or(""))
            } else {
                String::new()
            },
            if o["gpu"] == true {
                "  (on the GPU where there is one; not in the sum)"
            } else {
                ""
            }
        );
    }
    if let Some(gap) = value["reconciled"]["not_attributed_bytes"].as_i64() {
        let share = value["reconciled"]["not_attributed_share"]
            .as_f64()
            .unwrap_or(0.0);
        let _ = writeln!(
            out,
            "not attributed: {:.2} MiB, {:.0} % of resident",
            gap as f64 / (1024.0 * 1024.0),
            share * 100.0
        );
    }
    let _ = writeln!(
        out,
        "objects: {count} holding about {:.2} MiB",
        mib(bytes as u64)
    );
    for t in &kept[..kept.len().min(max)] {
        let _ = writeln!(
            out,
            "  {:<24} {:>7} {:>10.1} KiB",
            t.name,
            t.count,
            t.bytes as f64 / 1024.0
        );
    }
    if left_out > 0 {
        let _ = writeln!(out, "  … {left_out} more types");
    }
    match script {
        Some(b) => {
            let _ = writeln!(out, "script heap: {:.2} MiB", mib(b as u64));
        }
        None => out.push_str("script heap: no script has run\n"),
    }
    let _ = writeln!(
        out,
        "reactive graph: {nodes} nodes, about {:.2} MiB",
        mib(graph as u64)
    );
    if let Some(t) = text {
        let _ = writeln!(
            out,
            "text: {} layouts, {} shaped, {} paragraphs, {} glyphs, about {:.2} MiB",
            t.layouts,
            t.shapes,
            t.paragraphs,
            t.glyphs,
            mib(t.bytes as u64)
        );
    }
    let _ = writeln!(
        out,
        "images: {} cached, {:.2} MiB ({:.2} MiB held now)",
        images.len(),
        mib(image_bytes as u64),
        mib(images.resident_bytes() as u64)
    );
    for (i, w) in windows.iter().enumerate() {
        let name = w.window.map_or_else(|| format!("window {i}"), handle);
        let _ = write!(
            out,
            "{name}: scene {} items, {} display lists, about {:.2} MiB",
            w.scene.items,
            w.scene.tree.lists,
            mib(w.scene.bytes as u64)
        );
        match &w.render {
            Some(r) => {
                let _ = writeln!(
                    out,
                    "; renderer {} meshes, {} textures, about {:.2} MiB",
                    r.meshes,
                    r.textures + r.pages + r.shape_textures,
                    mib(r.bytes() as u64)
                );
            }
            None => out.push_str("; nothing draws it\n"),
        }
    }
    ToolReply::from(value).with_text(out)
}
