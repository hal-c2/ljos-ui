//! `app_frames`' `export`: what is recorded, as a trace file a standard
//! trace viewer opens (the Chrome trace-event format, which Perfetto and
//! `chrome://tracing` read).
//!
//! The application writes the file: a recording is up to 600 frames, 600
//! wakes and 20 000 runs of bindings, more than an answer carries, and a
//! client that is an agent has no file of its own to put pages into. It
//! is written below the directory the host names for it ([`TraceDir`]:
//! for an application, `traces` in its own per-user state directory),
//! under a plain name the client gives: a client names a file but not a
//! place, so a client that may only look cannot write anywhere else.
//! Written whole or not at all.
//!
//! One process, three tracks: the frames with their passes and painting,
//! the wakes, and the runs of bindings and handlers, nested as they ran.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ljos_engine::{Engine, HotKind};
use ljos_mcp_core::{SurfaceError, ToolReply};
use serde_json::{Value, json};

use super::objects::handle;

pub(crate) const TID_FRAMES: u32 = 1;
pub(crate) const TID_WAKES: u32 = 2;
pub(crate) const TID_BINDINGS: u32 = 3;
/// The longest name a client may give.
const MAX_NAME: usize = 64;

/// Where trace files go.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum TraceDir {
    /// Nowhere: `export` says the application was given no place.
    #[default]
    Nowhere,
    /// This directory, as a host or a test names it.
    At(PathBuf),
    /// `traces` in the application's per-user state directory, as
    /// `StandardPaths.StateLocation` names it: on Linux
    /// `$XDG_STATE_HOME/<organization>/<name>/traces`, by default below
    /// `~/.local/state`. Found when a file is written, so that the name the
    /// application gives itself (`Qt.application.name`) is the one used.
    PerUser,
}

impl TraceDir {
    fn resolve(&self, engine: &Engine) -> Option<PathBuf> {
        match self {
            TraceDir::Nowhere => None,
            TraceDir::At(dir) => Some(dir.clone()),
            TraceDir::PerUser => {
                let platform = ljos_platform::Platform::of(engine);
                let (org, app) = platform.application(engine);
                let env = platform.env();
                let dir = ljos_platform::Location::State.writable(&env, &org, &app);
                Some(env.real(&dir.join("traces")).ok()?)
            }
        }
    }
}

/// One trace event, with when it starts and, a slice, how long it lasts;
/// its times are filled in once the earliest is known.
pub(crate) struct Event {
    start: Instant,
    duration: Option<Duration>,
    value: Value,
}

impl Event {
    /// A slice of `duration` from `start`.
    pub(crate) fn slice(
        start: Instant,
        duration: Duration,
        tid: u32,
        name: String,
        cat: &str,
        args: Value,
    ) -> Event {
        Event {
            start,
            duration: Some(duration),
            value: json!({
                "name": name,
                "cat": cat,
                "ph": "X",
                "pid": 1,
                "tid": tid,
                "args": args,
            }),
        }
    }

    /// A moment of its thread's.
    pub(crate) fn instant(start: Instant, tid: u32, name: String, cat: &str, args: Value) -> Event {
        Event {
            start,
            duration: None,
            value: json!({
                "name": name,
                "cat": cat,
                "ph": "i",
                "s": "t",
                "pid": 1,
                "tid": tid,
                "args": args,
            }),
        }
    }
}

/// Microseconds, as the format counts, to a tenth.
fn micros(d: Duration) -> f64 {
    (d.as_nanos() as f64 / 100.0).round() / 10.0
}

fn metadata(tid: Option<u32>, name: &str) -> Value {
    match tid {
        None => json!({
            "name": "process_name", "ph": "M", "pid": 1,
            "args": { "name": name },
        }),
        Some(tid) => json!({
            "name": "thread_name", "ph": "M", "pid": 1, "tid": tid,
            "args": { "name": name },
        }),
    }
}

/// The runs of bindings and handlers kept, as slices.
fn binding_events(engine: &Engine, out: &mut Vec<Event>) -> usize {
    let profile = engine.profile();
    for run in &profile.runs {
        let Some(e) = profile.entries.get(run.entry) else {
            continue;
        };
        let (cat, what) = match e.kind {
            HotKind::Binding => ("binding", "binding"),
            HotKind::Handler => ("handler", "handler"),
        };
        out.push(Event::slice(
            run.start,
            run.duration,
            TID_BINDINGS,
            format!("{}.{}", e.type_name, e.name),
            cat,
            json!({ "object": handle(e.object), "at": e.at, "kind": what }),
        ));
    }
    profile.runs.len()
}

/// The file `name` asks for, below `dir`: a plain name, `.json` added
/// when it has not got it.
fn file_name(name: &str) -> Result<String, SurfaceError> {
    let plain = !name.is_empty()
        && name.len() <= MAX_NAME
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if !plain {
        return Err(SurfaceError::invalid_argument(format!(
            "`export`: give a plain file name of at most {MAX_NAME} letters, digits, `.`, `_` \
             and `-`, not a path"
        )));
    }
    Ok(if name.ends_with(".json") {
        name.to_owned()
    } else {
        format!("{name}.json")
    })
}

/// Writes what is recorded to `name` below `dir`.
pub(crate) fn export(
    engine: &Engine,
    dir: &TraceDir,
    name: &str,
    timeline: &super::timeline::Timeline,
    wakes: &super::wakes::Wakes,
) -> Result<ToolReply, SurfaceError> {
    let file = file_name(name)?;
    let Some(dir) = dir.resolve(engine) else {
        return Err(SurfaceError::unavailable(
            "`export`: this application was given no place for trace files",
        ));
    };
    let mut events = Vec::new();
    timeline.trace_events(&mut events);
    wakes.trace_events(&mut events);
    let runs = binding_events(engine, &mut events);
    let (frames, woke) = (timeline.kept(), wakes.kept());
    if events.is_empty() {
        return Err(SurfaceError::invalid_argument(
            "`export`: nothing is recorded; record frames, wakes or bindings first",
        ));
    }
    events.sort_by_key(|e| e.start);
    let base = events[0].start;
    let mut body = String::from("{\"traceEvents\":[\n");
    let mut first = true;
    let mut line = |v: &Value, body: &mut String| {
        if !first {
            body.push_str(",\n");
        }
        first = false;
        let _ = write!(body, "{v}");
    };
    line(&metadata(None, "Ljós application"), &mut body);
    line(&metadata(Some(TID_FRAMES), "frames"), &mut body);
    line(&metadata(Some(TID_WAKES), "wakes"), &mut body);
    line(
        &metadata(Some(TID_BINDINGS), "bindings and handlers"),
        &mut body,
    );
    for mut e in events.drain(..) {
        // Both ends are rounded, not the length: a slice rounded on its own
        // could end a tenth past the slice it lies in, and a viewer would
        // no longer put it inside.
        let ts = micros(e.start.duration_since(base));
        e.value["ts"] = json!(ts);
        if let Some(d) = e.duration {
            let end = micros((e.start + d).duration_since(base));
            e.value["dur"] = json!(((end - ts) * 10.0).round() / 10.0);
        }
        line(&e.value, &mut body);
    }
    body.push_str("\n],\"displayTimeUnit\":\"ms\"}\n");
    let path = dir.join(&file);
    write_whole(&dir, &path, body.as_bytes()).map_err(|e| {
        SurfaceError::unavailable(format!(
            "`export`: {} could not be written: {e}",
            path.display()
        ))
    })?;
    let path = path.canonicalize().unwrap_or(path);
    let text = format!(
        "wrote {} ({} bytes): {frames} frames, {woke} wakes, {runs} runs of bindings and \
         handlers; open it in a trace viewer (ui.perfetto.dev, chrome://tracing)",
        path.display(),
        body.len()
    );
    Ok(ToolReply::from(json!({
        "path": path.display().to_string(),
        "bytes": body.len(),
        "frames": frames,
        "wakes": woke,
        "runs": runs,
    }))
    .with_text(text))
}

/// Writes `bytes` to `path` through a file beside it, so that a reader
/// never sees half of it.
fn write_whole(dir: &Path, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let part = path.with_extension("json.part");
    let mut f = std::fs::File::create(&part)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(&part, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_a_plain_file_name() {
        assert_eq!(file_name("run-1").unwrap(), "run-1.json");
        assert_eq!(file_name("a.json").unwrap(), "a.json");
        for bad in [
            "",
            "../x",
            "/etc/x",
            ".hidden",
            "a/b",
            "a b",
            &"x".repeat(65),
        ] {
            assert!(file_name(bad).is_err(), "{bad:?}");
        }
    }
}
