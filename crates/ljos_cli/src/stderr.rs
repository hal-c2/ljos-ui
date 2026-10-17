//! Stderr, written by a thread of its own. A write to a terminal, a pipe
//! nobody reads quickly or a file on a busy disk can block for hundreds of
//! milliseconds; done from a `console.log` on the UI thread, that is a
//! click a user waits for. The UI thread hands the line over instead.
//!
//! The queue is bounded: a program that logs faster than stderr takes it
//! waits for room, as it would have waited for the write, and loses
//! nothing. The thread sleeps on the queue, so it costs nothing idle.

use std::io::Write as _;
use std::sync::OnceLock;
use std::sync::mpsc::{Receiver, SendError, SyncSender, sync_channel};

/// Lines waiting for stderr before a sender waits.
const QUEUE: usize = 1024;

enum Msg {
    Line(Vec<u8>),
    /// Answered once every line queued before it is written.
    Flush(SyncSender<()>),
}

static QUEUED: OnceLock<Option<SyncSender<Msg>>> = OnceLock::new();

fn queue() -> Option<&'static SyncSender<Msg>> {
    QUEUED
        .get_or_init(|| {
            let (queue, rx) = sync_channel(QUEUE);
            std::thread::Builder::new()
                .name("ljos-stderr".into())
                .spawn(move || drain(rx))
                .ok()?;
            Some(queue)
        })
        .as_ref()
}

fn drain(rx: Receiver<Msg>) {
    let stderr = std::io::stderr();
    for msg in rx {
        match msg {
            Msg::Line(bytes) => {
                let _ = stderr.lock().write_all(&bytes);
            }
            Msg::Flush(done) => {
                let _ = stderr.lock().flush();
                let _ = done.send(());
            }
        }
    }
}

/// Writes `bytes` to stderr from the writer's thread; from this one when
/// that thread could not be started.
pub fn write(bytes: Vec<u8>) {
    let bytes = match queue() {
        Some(q) => match q.send(Msg::Line(bytes)) {
            Ok(()) => return,
            Err(SendError(Msg::Line(bytes))) => bytes,
            Err(_) => return,
        },
        None => bytes,
    };
    let _ = std::io::stderr().write_all(&bytes);
}

/// Writes `line` and a newline.
pub fn line(line: &str) {
    let mut bytes = Vec::with_capacity(line.len() + 1);
    bytes.extend_from_slice(line.as_bytes());
    bytes.push(b'\n');
    write(bytes);
}

/// Waits until every line queued so far is written: before the process
/// ends, so its last words are not lost.
pub fn flush() {
    let Some(queue) = QUEUED.get().and_then(Option::as_ref) else {
        return;
    };
    let (done, wait) = sync_channel(1);
    if queue.send(Msg::Flush(done)).is_ok() {
        let _ = wait.recv();
    }
}

/// A tracing writer that hands each event to the writer's thread whole.
pub struct Event(Vec<u8>);

impl std::io::Write for Event {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        if !self.0.is_empty() {
            write(std::mem::take(&mut self.0));
        }
    }
}

/// For `tracing_subscriber::fmt().with_writer(stderr::events)`.
pub fn events() -> Event {
    Event(Vec::new())
}
