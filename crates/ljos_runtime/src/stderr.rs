//! Stderr, written by a thread of its own. A write to a terminal, a pipe
//! nobody reads quickly or a file on a busy disk can block for hundreds of
//! milliseconds, or for ever when nobody reads the pipe at all; done from
//! a `console.log` on the UI thread, that is a click a user waits for, or
//! an application that stops. The UI thread hands the line over instead,
//! and never waits for it.
//!
//! The lines waiting are bounded by their bytes ([`QUEUE_BYTES`]): a
//! program that logs faster than stderr takes it, for longer than the
//! queue holds, loses the lines that do not fit, and stderr gets one line
//! in their place saying how many were lost. The UI thread goes on. A
//! line longer than the bound is still written whole when nothing waits
//! before it: the bound is on what stderr has not taken yet, never on
//! one line.
//!
//! The thread is started by the first line, so a program that writes
//! nothing starts none, and it sleeps on the queue between lines: it costs
//! nothing idle.
//!
//! Every program of Ljós's that writes stderr while it runs writes it
//! here: `ljos run` and an application on the `ljos` facade, whose log
//! goes here by default. One queue keeps the lines of both in the order
//! they were written.

use std::collections::VecDeque;
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Duration;

/// Bytes of lines waiting for stderr before a line is dropped.
pub const QUEUE_BYTES: usize = 1 << 20;

/// How long [`flush`] waits for stderr to take what is queued.
pub const FLUSH_WAIT: Duration = Duration::from_secs(2);

enum Msg {
    Line(Vec<u8>),
    /// Lines that did not fit, where they would have been.
    Dropped(u64),
    /// Answered once every line queued before it is written.
    Flush(SyncSender<()>),
}

#[derive(Default)]
struct Queue {
    msgs: VecDeque<Msg>,
    /// The bytes of the lines waiting, and of those being written.
    bytes: usize,
}

struct Writer {
    queue: Mutex<Queue>,
    ready: Condvar,
}

static WRITER: OnceLock<Option<Writer>> = OnceLock::new();

impl Writer {
    fn lock(&self) -> MutexGuard<'_, Queue> {
        // A panic while the lock was held left nothing half done: the
        // queue is only pushed to and drained whole.
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn push(&self, msg: Msg) {
        self.lock().msgs.push_back(msg);
        self.ready.notify_one();
    }

    /// Queues `bytes`, or counts them lost when the queue is full. An
    /// empty queue takes any line, however long.
    fn line(&self, bytes: Vec<u8>) {
        let mut queue = self.lock();
        if queue.bytes > 0 && queue.bytes + bytes.len() > QUEUE_BYTES {
            match queue.msgs.back_mut() {
                Some(Msg::Dropped(n)) => *n += 1,
                _ => queue.msgs.push_back(Msg::Dropped(1)),
            }
        } else {
            queue.bytes += bytes.len();
            queue.msgs.push_back(Msg::Line(bytes));
        }
        drop(queue);
        self.ready.notify_one();
    }

    fn drain(&self) {
        let stderr = std::io::stderr();
        loop {
            let msgs = {
                let mut queue = self.lock();
                while queue.msgs.is_empty() {
                    queue = self
                        .ready
                        .wait(queue)
                        .unwrap_or_else(PoisonError::into_inner);
                }
                std::mem::take(&mut queue.msgs)
            };
            self.write_out(msgs, &mut stderr.lock());
        }
    }

    /// Writes `msgs` to `out`, and then counts their bytes gone.
    fn write_out(&self, msgs: VecDeque<Msg>, out: &mut impl std::io::Write) {
        let mut written = 0;
        for msg in msgs {
            // A closed or broken stderr fails each write; the lines are
            // dropped and the program goes on, as it would with nobody to
            // read them.
            match msg {
                Msg::Line(bytes) => {
                    written += bytes.len();
                    let _ = out.write_all(&bytes);
                }
                Msg::Dropped(n) => {
                    let lines = if n == 1 { "line" } else { "lines" };
                    let _ = writeln!(
                        out,
                        "ljos: {n} {lines} of the log dropped: stderr did not take them \
                         as fast as they were written"
                    );
                }
                Msg::Flush(done) => {
                    let _ = out.flush();
                    let _ = done.send(());
                }
            }
        }
        self.lock().bytes -= written;
    }
}

fn writer() -> Option<&'static Writer> {
    WRITER
        .get_or_init(|| {
            std::thread::Builder::new()
                .name("ljos-stderr".into())
                .spawn(|| {
                    // The writer is in the static once `get_or_init`
                    // returns, before the first line is queued.
                    if let Some(writer) = WRITER.wait() {
                        writer.drain();
                    }
                })
                .ok()?;
            Some(Writer {
                queue: Mutex::default(),
                ready: Condvar::new(),
            })
        })
        .as_ref()
}

/// Writes `bytes` to stderr from the writer's thread. Never waits for
/// stderr: when that thread could not be started, a process out of
/// threads, the bytes are dropped rather than written from this one,
/// which a stderr nobody reads would hold up.
pub fn write(bytes: Vec<u8>) {
    if let Some(w) = writer() {
        w.line(bytes);
    }
}

/// Writes `line` and a newline.
pub fn line(line: &str) {
    let mut bytes = Vec::with_capacity(line.len() + 1);
    bytes.extend_from_slice(line.as_bytes());
    bytes.push(b'\n');
    write(bytes);
}

/// Waits until every line queued so far is written, at most
/// [`FLUSH_WAIT`]: before the process ends, so its last words are not
/// lost, but never for ever when nobody reads stderr. Returns at once
/// when nothing was ever written.
pub fn flush() {
    let Some(writer) = WRITER.get().and_then(Option::as_ref) else {
        return;
    };
    let (done, wait) = sync_channel(1);
    writer.push(Msg::Flush(done));
    let _ = wait.recv_timeout(FLUSH_WAIT);
}

/// Whether the writer's thread was started: by the first line written,
/// unless it could not be, and the lines are dropped.
pub fn started() -> bool {
    WRITER.get().is_some_and(Option::is_some)
}

/// A tracing writer that hands each event to the writer's thread whole.
#[derive(Debug)]
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

#[cfg(test)]
mod tests {
    use super::*;

    fn writer() -> Writer {
        Writer {
            queue: Mutex::default(),
            ready: Condvar::new(),
        }
    }

    fn written(w: &Writer) -> String {
        let msgs = std::mem::take(&mut w.lock().msgs);
        let mut out = Vec::new();
        w.write_out(msgs, &mut out);
        String::from_utf8(out).expect("text")
    }

    #[test]
    fn lines_are_written_in_order() {
        let w = writer();
        w.line(b"one\n".to_vec());
        w.line(b"two\n".to_vec());
        assert_eq!(written(&w), "one\ntwo\n");
        assert_eq!(w.lock().bytes, 0, "what is written is no longer counted");
    }

    #[test]
    fn a_full_queue_drops_lines_and_says_how_many_where_they_were() {
        let w = writer();
        let half = vec![b'a'; QUEUE_BYTES / 2];
        w.line(half.clone());
        w.line(half.clone());
        // Stderr has taken nothing: these do not fit.
        w.line(b"lost\n".to_vec());
        w.line(b"lost\n".to_vec());
        let text = written(&w);
        assert!(!text.contains("lost"), "the lines that did not fit");
        assert!(
            text.ends_with("ljos: 2 lines of the log dropped: stderr did not take them as fast as they were written\n"),
            "{}",
            &text[text.len() - 120..]
        );
        // Room again, once stderr took them.
        w.line(b"kept\n".to_vec());
        assert_eq!(written(&w), "kept\n");
    }

    #[test]
    fn a_line_longer_than_the_queue_is_written_when_nothing_waits() {
        let w = writer();
        let mut long = vec![b'a'; QUEUE_BYTES + 1];
        long.push(b'\n');
        w.line(long.clone());
        // Behind it, the queue is full.
        w.line(b"lost\n".to_vec());
        let text = written(&w);
        assert!(text.starts_with(std::str::from_utf8(&long).expect("text")));
        assert!(
            text.ends_with("ljos: 1 line of the log dropped: stderr did not take them as fast as they were written\n"),
            "{}",
            &text[text.len() - 120..]
        );
    }

    #[test]
    fn a_failing_stderr_loses_the_lines_and_nothing_else() {
        struct Closed;
        impl std::io::Write for Closed {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
        }
        let w = writer();
        w.line(b"gone\n".to_vec());
        let (done, wait) = sync_channel(1);
        w.push(Msg::Flush(done));
        let msgs = std::mem::take(&mut w.lock().msgs);
        w.write_out(msgs, &mut Closed);
        assert!(wait.try_recv().is_ok(), "a flush is answered all the same");
        assert_eq!(w.lock().bytes, 0);
    }
}
