//! The gate an application is started through: it waits inside the limit
//! until the bench has checked that the limit is the one asked for, and
//! only then becomes the application. Nothing of the application runs
//! outside a limit, and its clock starts when it is let through.

use std::{
    ffi::OsString,
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::unix::{fs::OpenOptionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::Command,
};

/// What the gate is told to do.
const GO: u8 = b'g';

/// The bench's end of a gate: a FIFO it opened for reading and writing, so
/// neither end waits for the other to open it.
#[derive(Debug)]
pub struct Gate {
    path: PathBuf,
    fifo: File,
}

impl Gate {
    /// # Errors
    /// The FIFO cannot be made.
    pub fn new(path: &Path) -> std::io::Result<Self> {
        let _ = std::fs::remove_file(path);
        let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
            .map_err(std::io::Error::other)?;
        // SAFETY: a NUL-terminated path that lives across the call.
        if unsafe { libc::mkfifo(c.as_ptr(), 0o600) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        let fifo = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)?;
        Ok(Self {
            path: path.to_owned(),
            fifo,
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Lets the application start.
    ///
    /// # Errors
    /// The FIFO cannot be written.
    pub fn open(&mut self) -> std::io::Result<()> {
        self.fifo.write_all(&[GO])
    }
}

impl Drop for Gate {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// The gate's side, in the child: waits for the bench, sets the
/// application's environment and becomes it. Returns only on failure.
#[must_use]
pub fn pass(
    fifo: &Path,
    env: &[(String, String)],
    unset: &[String],
    program: &OsString,
    args: &[OsString],
) -> std::io::Error {
    let mut byte = [0u8; 1];
    let read = File::open(fifo).and_then(|mut f| f.read_exact(&mut byte));
    if let Err(e) = read {
        return e;
    }
    if byte[0] != GO {
        return std::io::Error::other("the bench did not let the application start");
    }
    let mut command = Command::new(program);
    command.args(args);
    for key in unset {
        command.env_remove(key);
    }
    for (key, value) in env {
        command.env(key, value);
    }
    command.exec()
}
