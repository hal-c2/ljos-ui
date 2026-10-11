//! How long the path of a Unix socket may be.
//!
//! The system keeps the path in a small fixed field of the address, so a
//! path past it cannot be bound. The check is made before the bind so the
//! refusal can say which path, how long, and what to do; the bind alone
//! says only "invalid argument".

use std::path::{Path, PathBuf};

/// The most bytes a socket path may have. The address field holds one more
/// byte than this for the terminating zero.
#[cfg(any(target_os = "linux", target_os = "android"))]
pub const SOCKET_PATH_MAX: usize = 107;
/// The most bytes a socket path may have (macOS and the BSDs).
#[cfg(not(any(target_os = "linux", target_os = "android")))]
pub const SOCKET_PATH_MAX: usize = 103;

/// A socket path that the system would refuse.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "cannot listen on the socket {}: the path is {len} bytes long and a socket path may have at \
     most {max}. Choose a shorter place with `--mcp-socket <path>` (for example a file under \
     `$XDG_RUNTIME_DIR` or `/tmp`); the inspector does not move it for you",
    path.display()
)]
pub struct SocketPathError {
    pub path: PathBuf,
    pub len: usize,
    pub max: usize,
}

/// Whether the system can bind a socket at `path`.
///
/// # Errors
/// The path is longer than [`SOCKET_PATH_MAX`] bytes.
pub fn check_socket_path(path: &Path) -> Result<(), SocketPathError> {
    #[cfg(unix)]
    let len = std::os::unix::ffi::OsStrExt::as_bytes(path.as_os_str()).len();
    #[cfg(not(unix))]
    let len = path.as_os_str().len();
    if len > SOCKET_PATH_MAX {
        return Err(SocketPathError {
            path: path.to_path_buf(),
            len,
            max: SOCKET_PATH_MAX,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of(len: usize) -> PathBuf {
        PathBuf::from(format!("/{}", "a".repeat(len - 1)))
    }

    #[test]
    fn a_path_at_the_limit_is_allowed() {
        assert_eq!(check_socket_path(&of(SOCKET_PATH_MAX)), Ok(()));
    }

    #[test]
    fn a_path_one_past_the_limit_is_refused_with_its_length_and_the_limit() {
        let err = check_socket_path(&of(SOCKET_PATH_MAX + 1)).unwrap_err();
        assert_eq!((err.len, err.max), (SOCKET_PATH_MAX + 1, SOCKET_PATH_MAX));
        let said = err.to_string();
        assert!(said.contains(&(SOCKET_PATH_MAX + 1).to_string()), "{said}");
        assert!(said.contains("at most"), "{said}");
        assert!(said.contains("--mcp-socket"), "{said}");
    }

    #[test]
    #[cfg(unix)]
    fn the_limit_is_what_the_system_binds() {
        // The dir is short so only the file name decides the length.
        let dir =
            std::path::PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").unwrap_or("/tmp".into()));
        let room = SOCKET_PATH_MAX.saturating_sub(dir.as_os_str().len() + 1);
        if room < 8 {
            return;
        }
        let ok = dir.join("m".repeat(room.min(40)));
        let _ = std::fs::remove_file(&ok);
        if check_socket_path(&ok).is_ok() {
            std::os::unix::net::UnixListener::bind(&ok).unwrap();
            std::fs::remove_file(&ok).unwrap();
        }
    }
}
