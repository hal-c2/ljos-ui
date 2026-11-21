//! A packed application (`features/tooling/packed-log.feature`): the
//! scenario's project compiled into a pack, as a shipped program embeds
//! it, run by a program on the `ljos` facade, the `packed_log` example of
//! `ljos`. Its stderr is read, never read, read late or closed, and its
//! stdout, where a handler of its own writes the log, is read.

use std::{
    io::{BufRead, BufReader, Read},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
};

use ljos_child::Reaped;

use super::{App, Rig, lock, thread_wakes};
use crate::backend::testrun;

/// How long a packed application may run before it is taken as stalled.
const LIMIT: Duration = Duration::from_secs(60);
/// How long stderr is left unread when it is read late: longer than the
/// application takes to log more than the pipe and the queue hold.
const LATE: Duration = Duration::from_secs(2);
/// The name of the thread that writes the errors (`ljos_runtime::stderr`).
const WRITER: &str = "ljos-stderr";

/// What becomes of the application's stderr.
#[derive(Clone, Copy, Debug)]
pub enum Errors {
    /// Read as it is written.
    Read,
    /// A pipe nobody reads.
    NeverRead,
    /// Read only after [`LATE`].
    ReadLate,
    /// Closed before the program starts.
    Closed,
}

/// What the program does with the log, as `packed_log` takes it.
#[derive(Clone, Copy, Debug)]
pub enum Log {
    Stderr,
    Own,
    Setup,
    Kept,
}

impl Log {
    fn arg(self) -> &'static str {
        match self {
            Log::Stderr => "stderr",
            Log::Own => "own",
            Log::Setup => "setup",
            Log::Kept => "kept",
        }
    }
}

/// How the project is packed: as a build script packs it unless said
/// otherwise.
#[derive(Clone, Debug, Default)]
pub struct Packing {
    /// The kinds of files carried besides the QML, by extension; `None`
    /// for the build's own choice.
    pub kinds: Option<Vec<String>>,
    /// A file that does not compile is packed as a failure, where a build
    /// would stop.
    pub keep_failing: bool,
}

/// A packed application that ran to its end.
pub(super) struct Packed {
    took: Duration,
    output: String,
    errors: String,
}

impl Rig {
    /// The project compiled into a pack beside it, as `packing` says, and
    /// read back as the program reads it.
    pub(super) fn pack(&mut self, packing: &Packing) -> Result<PathBuf, String> {
        let on_error = if packing.keep_failing {
            ljos::dev::OnError::Record
        } else {
            ljos::dev::OnError::Fail
        };
        let source = ljos::dev::compile(self.dir.path(), None, on_error)
            .map_err(|e| format!("the project does not compile: {e}"))?;
        let options = ljos::build::Options::new().without_sources();
        let options = match &packing.kinds {
            Some(kinds) => options.with_files(kinds.iter().map(String::as_str)),
            None => options,
        };
        let pack = options
            .pack(&source, &[self.dir.path()])
            .map_err(|e| format!("the project cannot be packed: {e}"))?;
        let file: PathBuf = self.outside.path().join("app.pack");
        std::fs::write(&file, &pack).map_err(|e| format!("cannot write the pack: {e}"))?;
        self.pack = Some(
            ljos_engine::MapSource::from_pack(&pack)
                .map_err(|e| format!("the pack cannot be read: {e}"))?,
        );
        Ok(file)
    }

    /// The project compiled into a pack beside it, and the program that
    /// runs a pack.
    fn packed_command(&mut self, log: Log) -> Result<Command, String> {
        let file = self.pack(&Packing::default())?;
        let bin = testrun::example_bin("LJOS_PACKED_LOG_BIN", "ljos", "packed_log")
            .map_err(|e| e.to_string())?;
        // As `Rig::command` runs `ljos`: no display, no session bus.
        let mut command = Command::new(bin);
        command
            .arg(file)
            .args(["Main", log.arg()])
            .current_dir(self.dir.path())
            .env(
                "DBUS_SESSION_BUS_ADDRESS",
                format!(
                    "unix:path={}",
                    self.outside.path().join("no-session-bus").display()
                ),
            )
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("DISPLAY")
            .stdin(Stdio::null());
        Ok(command)
    }

    /// Runs the packed application to its end, at most [`LIMIT`].
    pub(super) fn run_packed(&mut self, log: Log, errors: Errors) -> Result<(), String> {
        self.packed = None;
        let mut command = self.packed_command(log)?;
        let mut command = match errors {
            Errors::Closed => {
                let mut sh = Command::new("sh");
                sh.args(["-c", r#"exec "$0" "$@" 2>&-"#])
                    .arg(command.get_program())
                    .args(command.get_args())
                    .current_dir(self.dir.path())
                    .stdin(Stdio::null());
                for (key, value) in command.get_envs() {
                    match value {
                        Some(value) => sh.env(key, value),
                        None => sh.env_remove(key),
                    };
                }
                sh
            }
            _ => {
                command.stderr(Stdio::piped());
                command
            }
        };
        let started = Instant::now();
        let mut child = Reaped::spawn(command.stdout(Stdio::piped()))
            .map_err(|e| format!("cannot start the packed application: {e}"))?;
        let output = child.stdout.take().map(|out| read_all(out, Duration::ZERO));
        // A pipe nobody reads is held open, so that a write to it blocks
        // once it is full instead of failing.
        let mut held = None;
        let read = match errors {
            Errors::Read => child.stderr.take().map(|err| read_all(err, Duration::ZERO)),
            Errors::ReadLate => child.stderr.take().map(|err| read_all(err, LATE)),
            Errors::NeverRead => {
                held = child.stderr.take();
                None
            }
            Errors::Closed => None,
        };
        let status = loop {
            if let Some(status) = child
                .try_wait()
                .map_err(|e| format!("cannot wait for the packed application: {e}"))?
            {
                break status;
            }
            if started.elapsed() > LIMIT {
                let _ = child.stop();
                return Err(format!(
                    "the packed application did not end in {} s",
                    LIMIT.as_secs()
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let took = started.elapsed();
        drop(held);
        let output = joined(output);
        let errors = joined(read);
        if !status.success() {
            return Err(format!(
                "the packed application failed ({status}); it wrote:\n{output}\n{errors}"
            ));
        }
        self.packed = Some(Packed {
            took,
            output,
            errors,
        });
        Ok(())
    }

    /// Starts the packed application, its log on stderr, and leaves it
    /// running: the application the steps of a started one look at.
    pub(super) fn start_packed(&mut self) -> Result<(), String> {
        self.server = None;
        self.app = None;
        let mut command = self.packed_command(Log::Stderr)?;
        let mut child = Reaped::spawn(command.stdout(Stdio::null()).stderr(Stdio::piped()))
            .map_err(|e| format!("cannot start the packed application: {e}"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or("the packed application has no errors to read")?;
        let errors = Arc::new(Mutex::new(Vec::new()));
        {
            let errors = Arc::clone(&errors);
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines() {
                    let Ok(line) = line else { break };
                    lock(&errors).push(line);
                }
            });
        }
        self.app = Some(App {
            child,
            errors,
            _desktop: None,
            wakes_from: None,
            token_file: PathBuf::new(),
            burst: None,
        });
        Ok(())
    }

    pub(super) fn packed(&self) -> Result<&Packed, String> {
        self.packed.as_ref().ok_or_else(|| {
            "the packed application has not run; use `When the packed application runs`".to_owned()
        })
    }
}

impl Packed {
    pub(super) fn errors(&self) -> &str {
        &self.errors
    }

    pub(super) fn output(&self) -> &str {
        &self.output
    }

    pub(super) fn took(&self) -> Duration {
        self.took
    }
}

/// Whether the started application has a thread that writes its errors.
pub(super) fn has_writer(app: &App) -> Result<bool, String> {
    Ok(thread_wakes(app.child.id())?
        .values()
        .any(|(name, _)| name == WRITER))
}

/// Reads `from` to its end on a thread of its own, after `wait`.
fn read_all(mut from: impl Read + Send + 'static, wait: Duration) -> JoinHandle<String> {
    std::thread::spawn(move || {
        std::thread::sleep(wait);
        let mut bytes = Vec::new();
        let _ = from.read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

fn joined(reader: Option<JoinHandle<String>>) -> String {
    reader
        .and_then(|reader| reader.join().ok())
        .unwrap_or_default()
}
