//! `app_metrics` with `memory`: where the application's memory is, as an
//! observer's allocator view shows it.
//!
//! What has a running count (the script heap, the process's resident
//! pages, the image cache) is read; the objects are walked once, and the
//! text caches, each scene and each renderer add up their own tables.
//! Nothing is kept up to date for it, so an application at rest stays at
//! rest. Bytes no allocator counts for us are estimates and say so.

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
