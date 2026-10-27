//! A session bus of a test's own, so nothing reaches the desktop's.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use zbus::blocking::{Connection, connection};

/// A bus of the test's own, so nothing reaches the desktop's.
pub(crate) struct Bus {
    daemon: std::process::Child,
    pub(crate) address: String,
    _dir: tempfile::TempDir,
}

impl Bus {
    pub(crate) fn start() -> Option<Self> {
        let dir = tempfile::tempdir().ok()?;
        let mut daemon = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address"])
            .arg(format!(
                "--address=unix:path={}",
                dir.path().join("bus").display()
            ))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let mut address = String::new();
        BufReader::new(daemon.stdout.take()?)
            .read_line(&mut address)
            .ok()?;
        Some(Bus {
            daemon,
            address: address.trim().to_owned(),
            _dir: dir,
        })
    }

    pub(crate) fn connect(&self) -> Connection {
        connection::Builder::address(self.address.as_str())
            .map(|b| b.method_timeout(REPLY_WAIT))
            .and_then(connection::Builder::build)
            .expect("the test's bus")
    }
}

impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

/// How long a test waits for a bus to answer a call: zbus waits for good
/// unless told, and a call that never returns hangs the test with no
/// word of which call it was.
pub(crate) const REPLY_WAIT: Duration = Duration::from_secs(10);

/// The step a bus test has reached, which a hung one is failed with.
#[derive(Clone, Default)]
pub(crate) struct Step(Arc<Mutex<&'static str>>);

impl Step {
    pub(crate) fn at(&self, step: &'static str) {
        *crate::lock(&self.0) = step;
    }
}

/// Runs `test` on a thread of its own, and fails, saying the step it had
/// reached, when it has not ended within `limit`: a wait on the bus with
/// no timeout of its own (connecting, the daemon starting) fails the test
/// with its cause instead of hanging the run. A panic in `test` is the
/// test's failure, as it would be on the test's own thread.
pub(crate) fn within(limit: Duration, test: impl FnOnce(&Step) + Send + 'static) {
    let step = Step::default();
    let (done, ended) = mpsc::channel::<()>();
    let thread = std::thread::spawn({
        let step = step.clone();
        move || {
            // Sent when the thread ends, panicking or not.
            let _done = done;
            test(&step);
        }
    });
    match ended.recv_timeout(limit) {
        Err(mpsc::RecvTimeoutError::Timeout) => {
            panic!("hung for {limit:?} at: {}", crate::lock(&step.0));
        }
        _ => {
            if let Err(panic) = thread.join() {
                std::panic::resume_unwind(panic);
            }
        }
    }
}
