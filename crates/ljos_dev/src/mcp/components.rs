//! `app_objects`' `components` and `component`: what the application has
//! loaded. Each component of each file loaded, written as a file, inline,
//! as a delegate or another template, with how many instances of it live
//! and what they hold; and the instances of one, to be looked at in the
//! tree by their handles. Asked for, never kept.

use std::fmt::Write as _;

use ljos_engine::{Engine, LoadedComponent};
use ljos_mcp_core::{SurfaceError, ToolReply};
use serde_json::{Value, json};

use super::objects::handle;
use super::tools::text_property;

const DEFAULT_LIMIT: usize = 50;

fn file_name(url: &str) -> &str {
    url.rsplit('/').next().unwrap_or(url)
}

fn line(c: &LoadedComponent) -> String {
    let n = c.instances.len();
    format!(
        "{} {} ({}, {}): {n} {}, {} objects, {} bytes",
        c.kind.name(),
        c.name,
        file_name(&c.url),
        c.id,
        if n == 1 { "instance" } else { "instances" },
        c.objects,
        c.bytes
    )
}

fn row(c: &LoadedComponent) -> Value {
    json!({
        "component": c.id,
        "name": c.name,
        "kind": c.kind.name(),
        "url": c.url,
        "instances": c.instances.len(),
        "objects": c.objects,
        "bytes": c.bytes,
    })
}

/// Every component loaded; the most instances first, so that a long list
/// cut short keeps what holds the most.
pub(crate) fn list(engine: &Engine, per_copy: usize) -> ToolReply {
    let mut found = engine.components();
    found.sort_by(|a, b| {
        b.instances
            .len()
            .cmp(&a.instances.len())
            .then_with(|| b.bytes.cmp(&a.bytes))
    });
    let total = found.len();
    let live: usize = found.iter().map(|c| c.instances.len()).sum();
    let mut keep = total;
    loop {
        let value = json!({
            "components": found[..keep].iter().map(row).collect::<Vec<_>>(),
            "instances": live,
            "left_out": total - keep,
        });
        let mut text = if total == 0 {
            "no component is loaded".to_owned()
        } else {
            found[..keep]
                .iter()
                .map(line)
                .collect::<Vec<_>>()
                .join("\n")
        };
        if total > keep {
            let _ = write!(text, "\n({} more components left out)", total - keep);
        }
        if keep <= 1 || (text.len() <= per_copy && value.to_string().len() <= per_copy) {
            return ToolReply::from(value).with_text(text);
        }
        keep -= keep.div_ceil(4);
    }
}

/// The instances of component `id` alive, `limit` from `offset` on.
pub(crate) fn instances(
    engine: &Engine,
    id: &str,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<ToolReply, SurfaceError> {
    let Some(c) = engine.components().into_iter().find(|c| c.id == id) else {
        return Err(SurfaceError::invalid_argument(format!(
            "`app_objects`: no component `{id}` is loaded; `components: true` lists them"
        )));
    };
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(DEFAULT_LIMIT);
    let count = c.instances.len();
    let page: Vec<_> = c
        .instances
        .iter()
        .skip(offset)
        .take(limit)
        .copied()
        .collect();
    let mut text = format!("{}\n", line(&c));
    let rows: Vec<Value> = engine.untrack(|| {
        page.iter()
            .map(|&obj| {
                let ty = engine
                    .meta_of(obj)
                    .map_or_else(|| "object".to_owned(), |m| m.name.to_string());
                let name = text_property(engine, obj, "objectName");
                let bytes = engine.object_bytes(obj);
                let _ = write!(text, "{} {ty}", handle(obj));
                if let Some(name) = &name {
                    let _ = write!(text, " \"{name}\"");
                }
                text.push('\n');
                json!({
                    "handle": handle(obj),
                    "type": ty,
                    "object_name": name,
                    "bytes": bytes,
                })
            })
            .collect()
    });
    let end = offset + page.len();
    let next = (end < count).then_some(end);
    if let Some(next) = next {
        let _ = write!(text, "({} more; `offset: {next}` reads on)", count - end);
    }
    Ok(ToolReply::from(json!({
        "component": c.id,
        "name": c.name,
        "kind": c.kind.name(),
        "url": c.url,
        "count": count,
        "objects": c.objects,
        "bytes": c.bytes,
        "offset": offset,
        "instances": rows,
        "next_offset": next,
    }))
    .with_text(text.trim_end().to_owned()))
}
