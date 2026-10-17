//! A session bus of a test's own, so nothing reaches the desktop's.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

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
