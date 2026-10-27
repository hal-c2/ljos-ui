//! Running a corpus through an external renderer.
//!
//! The renderer is a shell command; `{id}` and `{out}` in it are replaced by
//! the (shell-quoted) example id and the PNG path to write. The example's
//! details arrive as JSON on stdin and as `QML_COMPARE_*` environment
//! variables. The command succeeds when it wrote the picture. It may print
//! one JSON object as the last line of stdout to add what it knows
//! (see [`Notes`]). A renderer that takes longer than the timeout is
//! killed with every process it started, and the example fails.

use std::io::{Read, Write as _};
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use ljos_child::Reaped;

use serde::Deserialize;
use serde_json::json;

use crate::image::Rgba;
use crate::manifest::{Example, Manifest};
use crate::metric;
use crate::report::{Row, Said};

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("could not write to {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
}

/// What a renderer may add about an example. Everything is optional; without
/// it a picture that was written means the example compiled and rendered.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notes {
    pub files: Option<u32>,
    pub compiled: Option<u32>,
    pub entry_compiles: Option<bool>,
    pub error: Option<String>,
    pub stack: Option<String>,
    #[serde(default)]
    pub lacks: Vec<String>,
    #[serde(default)]
    pub compile_errors: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub said: Vec<Said>,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub renderer: String,
    pub report_dir: PathBuf,
    pub threshold: f64,
    /// How long one example may take.
    pub timeout: Duration,
    /// Run only these ids (all when empty).
    pub only: Vec<String>,
}

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

fn io_error(path: &Path) -> impl FnOnce(std::io::Error) -> RunError {
    let path = path.display().to_string();
    move |source| RunError::Io { path, source }
}

pub fn run(manifest: &Manifest, options: &Options) -> Result<Vec<Row>, RunError> {
    std::fs::create_dir_all(&options.report_dir).map_err(io_error(&options.report_dir))?;
    let mut rows = Vec::new();
    for example in &manifest.examples {
        if !options.only.is_empty() && !options.only.contains(&example.id) {
            continue;
        }
        rows.push(run_one(manifest, example, options)?);
    }
    rows.sort_by_key(|r| r.id.to_lowercase());
    Ok(rows)
}

/// What a renderer did: how it ended (`None` when it was killed for
/// taking too long) and what it wrote.
struct Rendered {
    status: Option<ExitStatus>,
    stdout: String,
    stderr: String,
}

/// How long a renderer's output is waited for after it ended, in case a
/// process it left behind holds a pipe.
const DRAIN: Duration = Duration::from_secs(2);

/// Runs `command` with `request` on its stdin, for at most `within`.
/// The request is written and the output read at once, so a renderer
/// that writes before it reads cannot stall on a full pipe.
fn render(command: &mut Command, request: String, within: Duration) -> std::io::Result<Rendered> {
    let mut child = Reaped::spawn(command)?;
    if let Some(mut stdin) = child.stdin.take() {
        // A renderer that ignores stdin may exit first; that is not an error.
        thread::spawn(move || {
            let _ = stdin.write_all(request.as_bytes());
        });
    }
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        let (tx, rx) = mpsc::channel();
        if let Some(mut pipe) = pipe {
            thread::spawn(move || {
                let mut text = Vec::new();
                let _ = pipe.read_to_end(&mut text);
                let _ = tx.send(String::from_utf8_lossy(&text).into_owned());
            });
        }
        rx
    };
    let stdout = drain(child.stdout.take().map(|p| Box::new(p) as _));
    let stderr = drain(child.stderr.take().map(|p| Box::new(p) as _));
    let deadline = Instant::now() + within;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if Instant::now() > deadline {
            if let Ok(group) = i32::try_from(child.id()) {
                // SAFETY: the group is our child's, which is not yet waited
                // for, so its id cannot have been reused.
                unsafe {
                    libc::killpg(group, libc::SIGKILL);
                }
            }
            let _ = child.stop();
            break None;
        }
        thread::sleep(Duration::from_millis(10));
    };
    // Once the renderer has ended, what is left in a pipe is read at
    // once; only a process it left behind can hold one open.
    let read = |rx: mpsc::Receiver<String>| rx.recv_timeout(DRAIN).unwrap_or_default();
    Ok(Rendered {
        status,
        stdout: read(stdout),
        stderr: read(stderr),
    })
}

fn run_one(manifest: &Manifest, example: &Example, options: &Options) -> Result<Row, RunError> {
    let out = options
        .report_dir
        .join(format!("{}.actual.png", example.id));
    // A stale picture from an earlier run must not pass for this one.
    let _ = std::fs::remove_file(&out);
    let directory = manifest.directory(example);
    let captured_at = manifest.captured_at(example);
    let settle = example.settle_ms();
    let command = options
        .renderer
        .replace("{id}", &shell_quote(&example.id))
        .replace("{out}", &shell_quote(&out.display().to_string()));

    let request = json!({
        "id": example.id,
        "entry": example.entry,
        "directory": directory,
        "window": example.window,
        "volatile": example.volatile,
        "settle": settle,
        "capturedAt": captured_at,
        "screen": manifest.screen,
        "out": out,
        "example": example,
    });
    let mut renderer = Command::new("sh");
    renderer
        .arg("-c")
        .arg(&command)
        .env("QML_COMPARE_ID", &example.id)
        .env("QML_COMPARE_OUT", &out)
        .env("QML_COMPARE_DIR", &directory)
        .env("QML_COMPARE_ENTRY", &example.entry)
        .env("QML_COMPARE_WIDTH", example.window.width.to_string())
        .env("QML_COMPARE_HEIGHT", example.window.height.to_string())
        .env(
            "QML_COMPARE_VOLATILE",
            example.volatile.as_deref().unwrap_or(""),
        )
        .env("QML_COMPARE_SETTLE_MS", settle.to_string())
        .env(
            "QML_COMPARE_CAPTURED_AT",
            captured_at.as_deref().unwrap_or(""),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Its own group, so that a timeout ends what the shell started.
        .process_group(0);

    let (status_ok, stdout, stderr, spawn_error) =
        match render(&mut renderer, request.to_string(), options.timeout) {
            Ok(Rendered {
                status: Some(status),
                stdout,
                stderr,
            }) => (status.success(), stdout, stderr, None),
            Ok(Rendered { status: None, .. }) => (
                false,
                String::new(),
                String::new(),
                Some(format!(
                    "it took longer than {} s",
                    options.timeout.as_secs_f32()
                )),
            ),
            Err(e) => (false, String::new(), String::new(), Some(e.to_string())),
        };
    let notes: Notes = stdout
        .lines()
        .rev()
        .find(|l| l.trim_start().starts_with('{'))
        .and_then(|l| serde_json::from_str(l).ok())
        .unwrap_or_default();

    let mut error = notes.error.clone();
    let mut picture = None;
    if let Some(e) = spawn_error {
        error.get_or_insert(format!("could not run the renderer: {e}"));
    } else if !status_ok {
        let last = stderr.lines().rev().find(|l| !l.trim().is_empty());
        error.get_or_insert_with(|| {
            last.map_or_else(|| "the renderer failed".into(), |l| l.trim().to_owned())
        });
    } else {
        match Rgba::read(&out) {
            Ok(p) => picture = Some(p),
            Err(e) => {
                error.get_or_insert(format!("the renderer wrote no picture: {e}"));
            }
        }
    }
    let renders = picture.is_some() && error.is_none();

    let reference = manifest.reference(example);
    let (mut pixels, mut content, mut mismatch) = (None, None, None);
    if let (Some(path), Some(picture), true) = (&reference, &picture, renders) {
        match Rgba::read(path) {
            Ok(r) => {
                match metric::compare(picture, &r, options.threshold) {
                    Ok(c) => {
                        (pixels, content) = (Some(c.pixels), Some(c.content));
                        c.diff
                            .write(&options.report_dir.join(format!("{}.diff.png", example.id)))
                            .map_err(|e| RunError::Io {
                                path: options.report_dir.display().to_string(),
                                source: std::io::Error::other(e),
                            })?;
                    }
                    Err(m) => mismatch = Some(m.0),
                }
                // For the HTML page, which sits next to the pictures.
                let _ = std::fs::copy(
                    path,
                    options
                        .report_dir
                        .join(format!("{}.reference.png", example.id)),
                );
            }
            Err(e) => mismatch = Some(format!("the reference picture is unreadable: {e}")),
        }
    }

    // A picture that did not come out is as far from its reference as can be.
    if reference.is_some() && !renders {
        (pixels, content) = (Some(0.0), Some(0.0));
    }

    let has_files = notes.files.unwrap_or(1);
    Ok(Row {
        id: example.id.clone(),
        entry: example.entry.clone(),
        window: example.window,
        files: has_files,
        compiled: notes.compiled.unwrap_or(u32::from(renders) * has_files),
        entry_compiles: notes.entry_compiles.unwrap_or(renders),
        renders,
        error,
        stack: notes.stack,
        reference: reference.is_some(),
        qt: example.qt_reason().map(str::to_owned),
        pixels,
        content,
        mismatch,
        lacks: notes.lacks,
        compile_errors: notes.compile_errors,
        said: notes.said,
    })
}
