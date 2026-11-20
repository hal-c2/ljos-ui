//! Where the files of an application's user may come from: the one
//! directory the application names, and nothing outside it (the `ljos`
//! feature `user-qml`, ARCHITECTURE.md §12.1). Every read of a user's
//! file, and every address the engine is asked to load while the user's
//! files run, is checked here and nowhere else.
//!
//! A user's directory is the application's root: its files are keyed as
//! the application's are, and an address in one resolves against
//! `file:///`, so `../` climbs no higher than that root and an absolute
//! path names a file of it. What is refused is told, never skipped
//! silently: a link that leads out, an address that names a place
//! neither the application nor the directory has, and a network address.

use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};

use crate::reload::CompileFault;

/// Why a file or an address was not loaded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Refusal {
    /// A link (or a directory on its way) leads out of the directory: the
    /// user's, or one the application loads from.
    #[error("{0}: a link that leads out of its directory")]
    LinkOut(String),
    /// A path or address that names a place outside the application's
    /// files and its user's directory.
    #[error("{0}: names a place outside the application and its user's directory")]
    Outside(String),
    /// An address on the network, or on another host.
    #[error("{0}: a network address; the user's files load from their directory only")]
    Network(String),
    /// A name that is not text.
    #[error("{0}: a file name that is not text")]
    Unnamed(String),
}

impl Refusal {
    /// The refusal as a fault a host tells, as it tells a file that does
    /// not compile.
    #[must_use]
    pub fn fault(&self) -> CompileFault {
        let file = match self {
            Self::LinkOut(f) | Self::Outside(f) | Self::Network(f) | Self::Unnamed(f) => f.clone(),
        };
        CompileFault {
            file,
            line: 0,
            column: 0,
            message: format!("refused: {self}"),
        }
    }
}

/// Why a user's file was not read.
#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error(transparent)]
    Refused(Refusal),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// More than this is not a user's QML but a disk.
const MAX_DIRS: usize = 4096;

/// Whether `name` is in the directory `dir` (both `/`-separated).
fn under(name: &str, dir: &str) -> bool {
    name.strip_prefix(dir)
        .is_some_and(|rest| rest.starts_with('/'))
}

/// The directory an application's user's files are in.
#[derive(Debug, Clone)]
pub struct UserDir {
    root: PathBuf,
    /// The directory in it (`/`-separated) whose files are named as on
    /// the application's import path.
    modules: Option<String>,
}

impl UserDir {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        // Absolute, so that the paths a watcher reports compare to it.
        let root = std::path::absolute(&root).unwrap_or(root);
        Self {
            root,
            modules: None,
        }
    }

    /// The files of `dir`, a directory in this one, are named from it, as
    /// on the application's import path: `qml/Lib/Badge.qml` under
    /// `with_modules("qml")` is `Lib/Badge.qml`. Of a module's file and
    /// another of the same name, the module's is the one read.
    #[must_use]
    pub fn with_modules(mut self, dir: &str) -> Self {
        let dir = dir
            .split(['/', '\\'])
            .filter(|part| !part.is_empty() && *part != ".")
            .collect::<Vec<_>>()
            .join("/");
        self.modules = (!dir.is_empty()).then_some(dir);
        self
    }

    /// The name of the file at `path`, a path under the directory: its
    /// path from the root, or from the modules' directory for one in it.
    pub fn name_of(&self, path: &Path) -> Result<String, Refusal> {
        let relative = self.relative(path)?;
        Ok(self.named(relative).0)
    }

    /// `relative` (a path from the root) as a name, and whether it is a
    /// module's.
    fn named(&self, relative: String) -> (String, bool) {
        let Some(modules) = &self.modules else {
            return (relative, false);
        };
        match relative
            .strip_prefix(modules.as_str())
            .and_then(|rest| rest.strip_prefix('/'))
        {
            Some(rest) => (rest.to_owned(), true),
            None => (relative, false),
        }
    }

    /// Where the file named `name` is read from: among the modules when
    /// one of them is there, else from the root.
    #[must_use]
    pub fn path_of(&self, name: &str) -> PathBuf {
        match &self.modules {
            // Only a module's file has a name in the modules' directory
            // (`qml/qml/Card.qml` is `qml/Card.qml`): gone, it is not the
            // root's path of that name, which is the module `Card.qml`.
            Some(modules) if self.module_has(name) || under(name, modules) => {
                self.root.join(modules).join(name)
            }
            _ => self.root.join(name),
        }
    }

    /// Whether the modules have a file named `name`, which hides the
    /// root's of that name.
    fn module_has(&self, name: &str) -> bool {
        self.modules
            .as_ref()
            .is_some_and(|modules| self.root.join(modules).join(name).is_file())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The root with its links followed; None while it does not exist.
    fn real_root(&self) -> Option<PathBuf> {
        self.root.canonicalize().ok()
    }

    /// The path from the root (`/`-separated) of `path`, a path under it;
    /// a path elsewhere, or one that steps up, is refused.
    pub fn relative(&self, path: &Path) -> Result<String, Refusal> {
        let shown = || path.display().to_string();
        let rest = path
            .strip_prefix(&self.root)
            .map_err(|_| Refusal::Outside(shown()))?;
        let mut parts = Vec::new();
        for part in rest.components() {
            match part {
                Component::Normal(name) => {
                    parts.push(name.to_str().ok_or_else(|| Refusal::Unnamed(shown()))?);
                }
                Component::CurDir => {}
                _ => return Err(Refusal::Outside(shown())),
            }
        }
        Ok(parts.join("/"))
    }

    /// The text of the file at `path`. A path outside the directory, or
    /// whose links lead out of it, is refused. The file is opened first,
    /// and what is checked is where that handle is, as the system says
    /// (`/proc/self/fd` on Linux, `F_GETPATH` on Apple's systems,
    /// `GetFinalPathNameByHandleW` on Windows): a link or a directory
    /// swapped while it is read changes nothing that is read. Elsewhere
    /// the path is resolved again and must be the handle's file (device
    /// and inode). Anything but a regular file (a pipe, a device) is not
    /// read, and opening one does not wait for a writer.
    pub fn read(&self, path: &Path) -> Result<String, ReadError> {
        use std::io::Read as _;
        let relative = self.relative(path).map_err(ReadError::Refused)?;
        let mut file = open_without_waiting(path)?;
        let opened = file.metadata()?;
        let root = self.real_root().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "the directory is gone")
        })?;
        let inside = match opened_path(&file) {
            Some(at) => at.starts_with(&root),
            None => resolved_is(path, &opened, &root)?,
        };
        if !inside {
            return Err(ReadError::Refused(Refusal::LinkOut(relative)));
        }
        if !opened.is_file() {
            return Err(ReadError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "not a file",
            )));
        }
        let mut text = String::new();
        file.read_to_string(&mut text)?;
        Ok(text)
    }

    /// Whether the directory `dir` (a name, `""` for the root) is there,
    /// among the modules or from the root, and in the directory, links
    /// followed.
    #[must_use]
    pub fn has_dir(&self, dir: &str) -> bool {
        if dir.split('/').any(|part| part == ".." || part == ".") {
            return false;
        }
        let Some(root) = self.real_root() else {
            return false;
        };
        let modules = self.modules.as_ref().map(|m| self.root.join(m).join(dir));
        modules
            .into_iter()
            .chain([self.root.join(dir)])
            .filter_map(|at| at.canonicalize().ok())
            .any(|real| real.starts_with(&root) && real.is_dir())
    }

    /// Every file under the directory a user may change the application
    /// with (`wanted` chooses by name), by name ([`name_of`](Self::name_of))
    /// with its text; and what was refused, by name too. Links inside the directory are read
    /// as the files they name; a link that leads out is refused. A
    /// directory a walk skips ([`crate::skipped_dir`]) is not entered.
    #[must_use]
    pub fn list(&self, wanted: impl Fn(&str) -> bool) -> (BTreeMap<String, String>, Vec<Refusal>) {
        let mut files = BTreeMap::new();
        let mut refused = Vec::new();
        let Some(root) = self.real_root() else {
            return (files, refused);
        };
        let mut stack = vec![self.root.clone()];
        let mut dirs = 0;
        while let Some(dir) = stack.pop() {
            dirs += 1;
            if dirs > MAX_DIRS {
                break;
            }
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(relative) = self.relative(&path) else {
                    refused.push(Refusal::Unnamed(path.display().to_string()));
                    continue;
                };
                let name = relative.rsplit('/').next().unwrap_or(&relative);
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if kind.is_dir() {
                    if !crate::skipped_dir(name) {
                        stack.push(path);
                    }
                    continue;
                }
                if name.starts_with('.') || !wanted(name) {
                    continue;
                }
                let (relative, module) = self.named(relative);
                // Hidden whatever the order of the walk: a file that is not
                // read is neither listed nor refused.
                if !module && self.module_has(&relative) {
                    continue;
                }
                if kind.is_symlink() {
                    match path.canonicalize() {
                        Ok(real) if !real.starts_with(&root) => {
                            refused.push(Refusal::LinkOut(relative));
                            continue;
                        }
                        // A link to a directory is not entered: it can loop.
                        Ok(real) if !real.is_file() => continue,
                        Ok(_) => {}
                        // A link to nothing is no file.
                        Err(_) => continue,
                    }
                }
                match self.read(&path) {
                    Ok(text) => {
                        files.insert(relative, text);
                    }
                    Err(ReadError::Refused(why)) => refused.push(why),
                    // Being written, or not text: its next save brings it.
                    Err(ReadError::Io(_)) => {}
                }
            }
        }
        (files, refused)
    }
}

/// Where the open `file` is, links resolved; None where the system
/// cannot say.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn opened_path(file: &std::fs::File) -> Option<PathBuf> {
    use std::os::fd::AsRawFd as _;
    std::fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd())).ok()
}

#[cfg(target_vendor = "apple")]
fn opened_path(file: &std::fs::File) -> Option<PathBuf> {
    use std::os::{fd::AsRawFd as _, unix::ffi::OsStrExt as _};
    let mut buf = vec![0u8; libc::PATH_MAX as usize];
    // SAFETY: F_GETPATH writes at most PATH_MAX bytes, NUL-terminated,
    // into `buf`, which is that long; the descriptor is open.
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETPATH, buf.as_mut_ptr()) } == -1 {
        return None;
    }
    let len = buf.iter().position(|&b| b == 0)?;
    Some(PathBuf::from(std::ffi::OsStr::from_bytes(&buf[..len])))
}

#[cfg(windows)]
fn opened_path(file: &std::fs::File) -> Option<PathBuf> {
    use std::os::windows::{ffi::OsStringExt as _, io::AsRawHandle as _};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_NAME_NORMALIZED, GetFinalPathNameByHandleW, VOLUME_NAME_DOS,
    };
    let mut buf = vec![0u16; 1024];
    loop {
        let size = u32::try_from(buf.len()).ok()?;
        // SAFETY: `buf` holds `size` u16s; the handle is open.
        let n = unsafe {
            GetFinalPathNameByHandleW(
                file.as_raw_handle() as _,
                buf.as_mut_ptr(),
                size,
                FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
            )
        } as usize;
        if n == 0 {
            return None;
        }
        if n < buf.len() {
            // `\\?\`-prefixed, as `canonicalize` gives the root.
            return Some(PathBuf::from(std::ffi::OsString::from_wide(&buf[..n])));
        }
        buf.resize(n + 1, 0);
    }
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_vendor = "apple",
    windows
)))]
fn opened_path(_: &std::fs::File) -> Option<PathBuf> {
    None
}

/// Whether `path`, resolved again, is under `root` and is the file
/// `opened` describes (device and inode, on unix).
fn resolved_is(path: &Path, opened: &std::fs::Metadata, root: &Path) -> std::io::Result<bool> {
    let real = path.canonicalize()?;
    if !real.starts_with(root) {
        return Ok(false);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        let checked = std::fs::metadata(&real)?;
        Ok((checked.dev(), checked.ino()) == (opened.dev(), opened.ino()))
    }
    #[cfg(not(unix))]
    {
        let _ = opened;
        Ok(true)
    }
}

/// Opens `path` for reading without blocking on a pipe that has no
/// writer, and without making a terminal the process's.
fn open_without_waiting(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY);
    }
    options.open(path)
}

/// Whether an address the engine is asked to load may be: a `file:` URL
/// with no host and no step up, a `qrc:` one, or an address with no
/// scheme. Anything else is on the network or another host.
pub fn check_address(url: &str) -> Result<(), Refusal> {
    let refused_outside = || Refusal::Outside(url.to_owned());
    let path = if let Some(rest) = url.strip_prefix("file://") {
        let (host, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
        if !host.is_empty() && host != "localhost" {
            return Err(Refusal::Network(url.to_owned()));
        }
        path
    } else if let Some(rest) = url.strip_prefix("qrc:") {
        rest
    } else if has_scheme(url) {
        return Err(Refusal::Network(url.to_owned()));
    } else {
        url
    };
    let path = path.split(['?', '#']).next().unwrap_or(path);
    if path.contains('\\') || path.contains('\0') || path.split('/').any(|part| part == "..") {
        return Err(refused_outside());
    }
    Ok(())
}

/// Whether `url` begins with a scheme (`https:`), as RFC 3986 spells one;
/// a drive letter (`C:`) is a path.
fn has_scheme(url: &str) -> bool {
    let Some((scheme, _)) = url.split_once(':') else {
        return false;
    };
    scheme.len() > 1
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_local_addresses_without_a_step_up_are_loaded() {
        assert_eq!(check_address("file:///Shell.qml"), Ok(()));
        assert_eq!(check_address("file://localhost/parts/Card.qml"), Ok(()));
        assert_eq!(check_address("qrc:/Main.qml"), Ok(()));
        assert_eq!(check_address("Shell.qml"), Ok(()));
        assert!(matches!(
            check_address("https://example.com/Evil.qml"),
            Err(Refusal::Network(_))
        ));
        assert!(matches!(
            check_address("file://server/share/Evil.qml"),
            Err(Refusal::Network(_))
        ));
        assert!(matches!(
            check_address("file:///../etc/Evil.qml"),
            Err(Refusal::Outside(_))
        ));
    }

    #[test]
    fn a_link_out_of_the_directory_is_refused_and_one_inside_is_read() {
        let outside = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("Evil.qml"), "Item {}").unwrap();
        std::fs::write(root.path().join("Shell.qml"), "Item {}").unwrap();
        std::fs::create_dir(root.path().join("parts")).unwrap();
        std::fs::write(root.path().join("parts/Card.qml"), "Rectangle {}").unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                outside.path().join("Evil.qml"),
                root.path().join("Out.qml"),
            )
            .unwrap();
            std::os::unix::fs::symlink(root.path().join("Shell.qml"), root.path().join("In.qml"))
                .unwrap();
        }
        let dir = UserDir::new(root.path());
        let (files, refused) = dir.list(|name| name.ends_with(".qml"));
        assert!(files.contains_key("Shell.qml"));
        assert!(files.contains_key("parts/Card.qml"));
        #[cfg(unix)]
        {
            assert!(files.contains_key("In.qml"));
            assert!(!files.contains_key("Out.qml"));
            assert_eq!(refused, vec![Refusal::LinkOut("Out.qml".to_owned())]);
            assert!(matches!(
                dir.read(&root.path().join("Out.qml")),
                Err(ReadError::Refused(Refusal::LinkOut(_)))
            ));
        }
        assert!(matches!(
            dir.read(&root.path().join("../x.qml")),
            Err(ReadError::Refused(Refusal::Outside(_)))
        ));
        assert!(dir.has_dir("parts"));
        assert!(!dir.has_dir("../"));
    }

    #[test]
    fn a_modules_directory_names_its_files_from_itself() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("qml/Lib")).unwrap();
        std::fs::write(root.path().join("shell.qml"), "Item {}").unwrap();
        std::fs::write(root.path().join("qml/Lib/Badge.qml"), "Item {}").unwrap();
        std::fs::write(root.path().join("qml/Card.qml"), "Rectangle {}").unwrap();
        // Hidden by the module of its name.
        std::fs::write(root.path().join("Card.qml"), "Item {}").unwrap();
        let dir = UserDir::new(root.path()).with_modules("./qml/");
        let (files, refused) = dir.list(|name| name.ends_with(".qml"));
        assert_eq!(
            files.keys().collect::<Vec<_>>(),
            ["Card.qml", "Lib/Badge.qml", "shell.qml"]
        );
        assert_eq!(files["Card.qml"], "Rectangle {}");
        assert!(refused.is_empty());
        let module = std::path::absolute(root.path().join("qml/Lib/Badge.qml")).unwrap();
        assert_eq!(dir.name_of(&module).unwrap(), "Lib/Badge.qml");
        assert_eq!(dir.path_of("Lib/Badge.qml"), module);
        assert_eq!(
            dir.path_of("shell.qml"),
            std::path::absolute(root.path().join("shell.qml")).unwrap()
        );
        // A module's name in the modules' directory is its path there
        // while that file is gone, never the root's file of that name.
        assert_eq!(
            dir.path_of("qml/Card.qml"),
            std::path::absolute(root.path().join("qml/qml/Card.qml")).unwrap()
        );
        assert!(dir.has_dir("Lib"));
        assert!(dir.has_dir("qml"));
        assert!(!dir.has_dir("Other"));
        // Without, the directory is a directory like any other.
        let plain = UserDir::new(root.path());
        assert_eq!(plain.name_of(&module).unwrap(), "qml/Lib/Badge.qml");
    }

    #[test]
    fn a_directory_of_a_module_s_name_hides_no_file_of_the_root() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("qml/Card.qml")).unwrap();
        std::fs::write(root.path().join("Card.qml"), "Item {}").unwrap();
        let dir = UserDir::new(root.path()).with_modules("qml");
        let (files, refused) = dir.list(|name| name.ends_with(".qml"));
        assert_eq!(files.keys().collect::<Vec<_>>(), ["Card.qml"]);
        assert_eq!(files["Card.qml"], "Item {}");
        assert!(refused.is_empty());
        assert_eq!(
            dir.path_of("Card.qml"),
            std::path::absolute(root.path().join("Card.qml")).unwrap()
        );
    }

    /// The walk reads the root's files before it enters the modules', so a
    /// hidden file is one the walk meets first.
    #[cfg(unix)]
    #[test]
    fn a_file_a_module_hides_is_not_refused() {
        let outside = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("Evil.qml"), "Item {}").unwrap();
        std::fs::create_dir(root.path().join("qml")).unwrap();
        std::fs::write(root.path().join("qml/Card.qml"), "Rectangle {}").unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("Evil.qml"),
            root.path().join("Card.qml"),
        )
        .unwrap();
        let dir = UserDir::new(root.path()).with_modules("qml");
        let (files, refused) = dir.list(|name| name.ends_with(".qml"));
        assert_eq!(files.keys().collect::<Vec<_>>(), ["Card.qml"]);
        assert_eq!(files["Card.qml"], "Rectangle {}");
        assert!(refused.is_empty(), "{refused:?}");
        // The module's gone, the root's is the one named, and refused.
        std::fs::remove_file(root.path().join("qml/Card.qml")).unwrap();
        let (files, refused) = dir.list(|name| name.ends_with(".qml"));
        assert!(files.is_empty());
        assert_eq!(refused, vec![Refusal::LinkOut("Card.qml".to_owned())]);
    }

    #[cfg(unix)]
    #[test]
    fn a_pipe_is_not_read_and_is_not_waited_for() {
        let root = tempfile::tempdir().unwrap();
        let fifo = root.path().join("Pipe.qml");
        let name = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: a NUL-terminated path that lives through the call.
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        std::fs::write(root.path().join("Shell.qml"), "Item {}").unwrap();
        let dir = UserDir::new(root.path());
        assert!(matches!(dir.read(&fifo), Err(ReadError::Io(_))));
        let (files, refused) = dir.list(|name| name.ends_with(".qml"));
        assert_eq!(files.keys().collect::<Vec<_>>(), ["Shell.qml"]);
        assert!(refused.is_empty());
    }
}
