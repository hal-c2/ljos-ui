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
//!
//! With `mappings` the program and its libraries, and what is not
//! attributed, are split by the mapping the pages are in (see
//! [`mappings`](super::mappings)).

use std::fmt::Write as _;

use ljos_engine::{Engine, ObjectId};
use ljos_mcp_core::ToolReply;
use ljos_quick::SceneMemory;
use ljos_render::BackendMemory;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::mappings::{Group, Mappings};
use super::objects::handle;
use super::process::Resident;
use super::room::{self, Room};
use super::rust_heap::rust_heap_bytes;

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
    /// Also split what is resident by the mapping it is in: the program,
    /// LLVM, the GPU driver, fonts, other libraries, the allocator's heap,
    /// other anonymous maps (Linux; elsewhere not known). Reads the
    /// process's memory map, a few milliseconds.
    #[serde(default)]
    pub mappings: bool,
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
        "glyph_runs_incomplete": r.glyph_runs_incomplete,
        "glyph_meshes_built": r.glyph_meshes_built,
        "warnings": r.warnings,
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

/// What the C allocator holds: in use by anyone, and freed but kept.
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

#[cfg(target_os = "macos")]
fn heap() -> Option<Heap> {
    let mut s = std::mem::MaybeUninit::<libc::malloc_statistics_t>::zeroed();
    // SAFETY: a null zone sums every zone; `s` has room for the counts,
    // which the allocator writes under its locks.
    let s = unsafe {
        libc::malloc_zone_statistics(std::ptr::null_mut(), s.as_mut_ptr());
        s.assume_init()
    };
    Some(Heap {
        in_use: s.size_in_use as u64,
        free: s.size_allocated.saturating_sub(s.size_in_use) as u64,
    })
}

#[cfg(not(any(all(target_os = "linux", target_env = "gnu"), target_os = "macos")))]
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
pub(crate) fn memory(
    engine: &Engine,
    windows: &[WindowMemory],
    args: &MemoryArgs,
    driver: Option<&str>,
    per_copy: usize,
) -> ToolReply {
    let max = args.max.unwrap_or(DEFAULT_MAX).clamp(1, MOST_TYPES);
    let all = engine.object_memory();
    let (count, bytes) = all
        .iter()
        .fold((0, 0), |(c, b), t| (c + t.count, b + t.bytes));
    let kept: Vec<_> = all
        .iter()
        .filter(|t| args.r#type.as_deref().is_none_or(|name| t.name == name))
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
            "rust_heap_bytes": rust_heap_bytes(),
        });
    }
    owners.push({
        let mut t = textures;
        t["gpu"] = json!(true);
        t
    });
    let maps = args.mappings.then(Mappings::now);
    let rust = rust_heap_bytes();
    let script_bytes = owners[0]["bytes"].as_u64().unwrap_or(0);
    let mapped = match &maps {
        None => Value::Null,
        Some(None) => json!({ "known": false }),
        Some(Some(m)) => {
            let mut v = m.json(driver);
            v["program_split"] = json!(
                m.file_backed()
                    .into_iter()
                    .map(|(g, b)| part(g.key(), &group_label(g, driver), b))
                    .collect::<Vec<_>>()
            );
            v["not_attributed_split"] = json!(not_attributed_split(
                m,
                heap,
                rust,
                (attributed, script_bytes),
                driver
            ));
            v
        }
    };

    let mut value = json!({
        "objects": {
            "count": count,
            "bytes": bytes,
            "approximate": true,
            "types": [],
            "types_left_out": kept.len(),
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
        "mappings": mapped,
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
    match &maps {
        None => {}
        Some(None) => out.push_str("by mapping: not known on this platform\n"),
        Some(Some(m)) => {
            let _ = writeln!(out, "program and libraries, by mapping:");
            for p in value["mappings"]["program_split"]
                .as_array()
                .into_iter()
                .flatten()
            {
                part_line(&mut out, p);
            }
            out.push_str("not attributed, by where it is:\n");
            for p in value["mappings"]["not_attributed_split"]
                .as_array()
                .into_iter()
                .flatten()
            {
                part_line(&mut out, p);
            }
            let _ = writeln!(
                out,
                "mappings: {:.1} MiB resident in {} groups",
                mib(m.resident()),
                m.groups.len()
            );
        }
    }
    let _ = writeln!(
        out,
        "objects: {count} holding about {:.2} MiB",
        mib(bytes as u64)
    );
    let head = std::mem::take(&mut out);
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
    // The types that do not fit are counted with those `max` left out.
    let mut room = Room::new(per_copy, &value, &format!("{head}{out}"));
    let rows = room::rows(&mut room, &kept[..kept.len().min(max)], false, |t| {
        let row = json!({ "type": t.name, "count": t.count, "bytes": t.bytes });
        let line = format!(
            "  {:<24} {:>7} {:>10.1} KiB",
            t.name,
            t.count,
            t.bytes as f64 / 1024.0
        );
        (row, line)
    });
    let left_out = kept.len() - rows.len();
    let mut lines = String::new();
    for (_, _, line) in &rows {
        let _ = writeln!(lines, "{line}");
    }
    if left_out > 0 {
        let _ = writeln!(lines, "  … {left_out} more types");
    }
    value["objects"]["types"] = rows.into_iter().map(|(_, v, _)| v).collect();
    value["objects"]["types_left_out"] = json!(left_out);
    ToolReply::from(value).with_text(format!("{head}{lines}{out}"))
}

/// A group's label, the driver named where the painter says which.
fn group_label(g: Group, driver: Option<&str>) -> String {
    match (g, driver) {
        (Group::Driver, Some(d)) => format!("GPU driver: {d}"),
        _ => g.label().to_owned(),
    }
}

fn part(key: &str, label: &str, bytes: u64) -> Value {
    json!({ "part": key, "label": label, "bytes": bytes })
}

fn part_line(out: &mut String, p: &Value) {
    let _ = writeln!(
        out,
        "  {:<44} {:>10.2} MiB",
        p["label"].as_str().unwrap_or(""),
        mib(p["bytes"].as_u64().unwrap_or(0))
    );
}

/// Where what no owner tells of is: what libraries wrote into their own
/// mappings, code compiled at run time, the stack, and of the allocator's
/// heap and the other anonymous maps, what no owner counts and the
/// allocator does not hold free. Largest first; the parts add up to what
/// is not attributed.
fn not_attributed_split(
    m: &Mappings,
    heap: Option<Heap>,
    rust: Option<u64>,
    (attributed, script): (u64, u64),
    driver: Option<&str>,
) -> Vec<Value> {
    let mut parts = Vec::new();
    let mut anon_maps = 0;
    for (g, b) in m.anonymous() {
        match g {
            Group::Heap | Group::Anonymous => anon_maps += b,
            Group::Jit | Group::Stack | Group::Kernel => {
                parts.push(part(g.key(), g.label(), b));
            }
            _ => parts.push(part(
                &format!("{}_data", g.key()),
                &format!("{}: data it wrote", group_label(g, driver)),
                b,
            )),
        }
    }
    let rest = anon_maps
        .saturating_sub(attributed)
        .saturating_sub(heap.map_or(0, |h| h.free));
    match (heap, rust) {
        // The script engine is C: its heap is among the C libraries'.
        (Some(h), Some(r)) => {
            let rust = r.saturating_sub(attributed.saturating_sub(script));
            let c = h.in_use.saturating_sub(r).saturating_sub(script);
            // What is allocated counts pages never touched, which are not
            // resident: what is resident is shared out by what each has
            // allocated.
            let rust_part = match rust + c {
                0 => 0,
                all => (u128::from(rest) * u128::from(rust) / u128::from(all)) as u64,
            };
            parts.push(part(
                "rust_unowned",
                "allocated by Rust code, no owner tells of it",
                rust_part,
            ));
            parts.push(part(
                "c_libraries",
                "allocated by C libraries (GPU driver, LLVM, fonts)",
                rest - rust_part,
            ));
        }
        (Some(_), None) => parts.push(part(
            "allocated_unowned",
            "allocated, no owner tells of it",
            rest,
        )),
        (None, _) => parts.push(part(
            "anonymous_maps",
            "heap and anonymous maps, less the owners",
            rest,
        )),
    }
    parts.retain(|p| p["bytes"].as_u64().unwrap_or(0) > 0);
    parts.sort_by(|a, b| {
        b["bytes"]
            .as_u64()
            .unwrap_or(0)
            .cmp(&a["bytes"].as_u64().unwrap_or(0))
    });
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Text drawn without some of its glyphs shows in the renderer's
    /// figures, so a blank label can be told from one never drawn.
    #[test]
    fn the_renderer_says_how_its_text_drawing_went() {
        let r = BackendMemory {
            glyph_runs_incomplete: 2,
            glyph_meshes_built: 40,
            warnings: 1,
            ..BackendMemory::default()
        };
        let v = render_json(&r);
        assert_eq!(v["glyph_runs_incomplete"], 2);
        assert_eq!(v["glyph_meshes_built"], 40);
        assert_eq!(v["warnings"], 1);
    }
}
