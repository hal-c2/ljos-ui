//! The token file an inspector writes, how far one is believed, and
//! waiting for one to appear.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde_json::Value as Json;

use crate::TokenFile;

/// What a token file says of the inspector that wrote it.
#[derive(Debug, Clone)]
pub struct Target {
    pub url: String,
    pub token: String,
    pub pid: Option<u32>,
}

/// How far a token file is believed. One found by looking in a directory
/// may have been put there by whoever made the directory, so it is
/// believed only when it is what an inspector of this user writes: a
/// file of its own, owner-only, naming a live process and an address on
/// this machine. One named on the command line is the user's choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Trust {
    /// Named rather than found.
    pub named: bool,
    /// `--remote`: a named file may give an address beyond this machine.
    pub remote: bool,
}

impl Trust {
    pub const FOUND: Trust = Trust {
        named: false,
        remote: false,
    };
}

/// Why a token file was not read, or not believed.
///
/// No variant names its cause as a `source`: the text already carries it,
/// as `context: cause`, so a chain printed with `{:#}` says it once.
#[derive(Debug, thiserror::Error)]
pub enum TokenError {
    #[error("{path}: {error}")]
    Io {
        path: PathBuf,
        error: std::io::Error,
    },
    #[error("{path} is not JSON: {error}")]
    NotJson {
        path: PathBuf,
        error: serde_json::Error,
    },
    #[error("{path} has no {field:?}")]
    Missing { path: PathBuf, field: &'static str },
    #[error("{path} has no inspector's token")]
    NotAToken { path: PathBuf },
    #[error("{path} does not name the process its name does")]
    OtherProcess { path: PathBuf },
    #[error("{path} names no running process")]
    NotRunning { path: PathBuf },
    #[error("{path} is not a plain file")]
    NotPlainFile { path: PathBuf },
    #[error("{path} is not a file")]
    NotAFile { path: PathBuf },
    #[error("{path} changed while it was read")]
    Changed { path: PathBuf },
    #[error("{path} is larger than a token file")]
    TooLarge { path: PathBuf },
    #[error("{path} belongs to another user")]
    OtherUser { path: PathBuf },
    #[error("{path} can be read by others")]
    ReadableByOthers { path: PathBuf },
    #[error("{url} is neither an `http://` address nor a `unix:` socket")]
    NotAnAddress { url: String },
    #[error("{url} names no host")]
    NoHost { url: String },
    #[error(
        "{url} is not a loopback port: name the token file and pass `--remote` to reach an \
         inspector elsewhere"
    )]
    NotLoopback { url: String },
    #[error("{url}: no socket there: {error}")]
    NoSocket { url: String, error: std::io::Error },
    #[error("{url} is not a socket of yours: name the token file and pass `--remote` to reach it")]
    NotYourSocket { url: String },
    #[error("{} is neither a token file nor a directory", .0.display())]
    NoSuchTarget(PathBuf),
    #[error(
        "no running application with an inspector of yours in {}: start one with `ljos run \
         --mcp FILE` in that directory, or name its token file",
        .0.display()
    )]
    NoneFound(PathBuf),
    #[error("process {pid} ended before its inspector wrote a token file in {}", dir.display())]
    Ended { dir: PathBuf, pid: u32 },
    #[error(
        "no token file of process {pid} in {} within {} ms{}",
        dir.display(),
        waited.as_millis(),
        last.as_ref().map(|why| format!(": {why}")).unwrap_or_default()
    )]
    NotYet {
        dir: PathBuf,
        pid: u32,
        waited: Duration,
        /// Why the file that was there was not believed, when one was.
        last: Option<Box<TokenError>>,
    },
}

impl TokenError {
    /// The file is not there (yet).
    fn absent(&self) -> bool {
        matches!(self, TokenError::Io { error, .. } if error.kind() == std::io::ErrorKind::NotFound)
    }
}

type Result<T> = std::result::Result<T, TokenError>;

/// More than any inspector writes; a file past it is not read on.
pub const MAX_TOKEN_FILE: u64 = 4096;

fn shown(path: &Path) -> PathBuf {
    path.to_owned()
}

pub fn read_token_file(path: &Path, trust: Trust) -> Result<Target> {
    let text = read_bounded(path, trust)?;
    let body: Json = serde_json::from_str(&text).map_err(|error| TokenError::NotJson {
        path: shown(path),
        error,
    })?;
    let field = |field: &'static str| {
        body[field]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| TokenError::Missing {
                path: shown(path),
                field,
            })
    };
    let url = field("url")?;
    // Said now rather than as a connection that fails on every call.
    let address = Address::of(&url)?;
    let token = field("token")?;
    // An inspector's token is 256 random bits in hex; anything else was
    // not written by one.
    if token.len() != 64 || !token.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(TokenError::NotAToken { path: shown(path) });
    }
    let pid = body["pid"].as_u64().and_then(|p| u32::try_from(p).ok());
    if !trust.named {
        let named = pid_in_name(path);
        if pid.is_none() || pid != named {
            return Err(TokenError::OtherProcess { path: shown(path) });
        }
        if pid.and_then(crate::process_alive) != Some(true) {
            return Err(TokenError::NotRunning { path: shown(path) });
        }
    }
    if !(trust.named && trust.remote) {
        address.check_local(&url)?;
    }
    Ok(Target { url, token, pid })
}

/// `PID` of a file named `mcp-PID.json`.
fn pid_in_name(path: &Path) -> Option<u32> {
    let name = path.file_name()?.to_str()?;
    let digits = name.strip_prefix("mcp-")?.strip_suffix(".json")?;
    if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// The text of a token file, read without hanging on a pipe or reading
/// without end; one found is not followed through a link, and has to be
/// its user's own and nobody else's.
fn read_bounded(path: &Path, trust: Trust) -> Result<String> {
    use std::io::Read as _;
    let io = |error| TokenError::Io {
        path: shown(path),
        error,
    };
    // Asked of the name itself, before anything is opened: a link, a
    // pipe or a device in a directory is not what an inspector writes.
    let before = if trust.named {
        None
    } else {
        let meta = std::fs::symlink_metadata(path).map_err(io)?;
        check_found(path, &meta)?;
        Some(meta)
    };
    // A pipe put in place after the check above opens at once rather than
    // waiting for a writer; it is turned away next.
    let file = TokenFile::open_for_reading(path, trust.named).map_err(io)?;
    let meta = file.metadata().map_err(io)?;
    if !meta.is_file() {
        return Err(TokenError::NotAFile { path: shown(path) });
    }
    if let Some(before) = before {
        check_found(path, &meta)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            // What was checked is what was opened.
            if (before.dev(), before.ino()) != (meta.dev(), meta.ino()) {
                return Err(TokenError::Changed { path: shown(path) });
            }
        }
        #[cfg(not(unix))]
        let _ = before;
    }
    if meta.len() > MAX_TOKEN_FILE {
        return Err(TokenError::TooLarge { path: shown(path) });
    }
    let mut text = String::new();
    file.take(MAX_TOKEN_FILE + 1)
        .read_to_string(&mut text)
        .map_err(io)?;
    if text.len() as u64 > MAX_TOKEN_FILE {
        return Err(TokenError::TooLarge { path: shown(path) });
    }
    Ok(text)
}

/// What a token file found in a directory has to be: a plain file, and
/// on Unix its user's own, readable by nobody else.
fn check_found(path: &Path, meta: &std::fs::Metadata) -> Result<()> {
    if !meta.file_type().is_file() {
        return Err(TokenError::NotPlainFile { path: shown(path) });
    }
    if meta.len() > MAX_TOKEN_FILE {
        return Err(TokenError::TooLarge { path: shown(path) });
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        if Some(meta.uid()) != crate::effective_uid() {
            return Err(TokenError::OtherUser { path: shown(path) });
        }
        if meta.mode() & 0o077 != 0 {
            return Err(TokenError::ReadableByOthers { path: shown(path) });
        }
    }
    Ok(())
}

/// The directories a token file is looked for in: `dir` itself and its
/// `target/ljos`, where `ljos run --mcp` writes one.
fn places(dir: &Path) -> [PathBuf; 2] {
    [dir.to_owned(), dir.join("target/ljos")]
}

/// The token file `target` names, or the newest live one in it, with how
/// far it is to be believed.
pub fn find_token_file(target: &Path) -> Result<(PathBuf, Trust)> {
    if target.is_file() {
        return Ok((
            target.to_owned(),
            Trust {
                named: true,
                ..Trust::default()
            },
        ));
    }
    if !target.is_dir() {
        return Err(TokenError::NoSuchTarget(target.to_owned()));
    }
    let mut found: Vec<(SystemTime, PathBuf)> = Vec::new();
    for dir in places(target) {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if pid_in_name(&path).is_none() {
                continue;
            }
            // A file a killed process left names a port nobody listens on;
            // one somebody else left is not believed at all.
            if read_token_file(&path, Trust::FOUND).is_err() {
                continue;
            }
            let at = std::fs::symlink_metadata(&path)
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            found.push((at, path));
        }
    }
    found.sort();
    match found.pop() {
        Some((_, path)) => Ok((path, Trust::FOUND)),
        None => Err(TokenError::NoneFound(target.to_owned())),
    }
}

/// The token file process `pid` wrote in `dir` (or its `target/ljos`),
/// believed as one found: it has to name `pid`, which has to run.
pub fn token_file_of(dir: &Path, pid: u32) -> Result<PathBuf> {
    let name = format!("mcp-{pid}.json");
    let mut said: Option<TokenError> = None;
    for place in places(dir) {
        let path = place.join(&name);
        match read_token_file(&path, Trust::FOUND) {
            Ok(_) => return Ok(path),
            // A file there that is not believed says more than one that
            // is not there at all.
            Err(why) if said.as_ref().is_none_or(|s| s.absent() && !why.absent()) => {
                said = Some(why);
            }
            Err(_) => {}
        }
    }
    Err(said.unwrap_or_else(|| TokenError::NoneFound(dir.to_owned())))
}

/// The first pause between looks; each is twice the last, up to `LONGEST`.
const FIRST: Duration = Duration::from_millis(2);
const LONGEST: Duration = Duration::from_millis(100);

/// One look for the token file of `pid`: `Ok(Some)` found, `Ok(None)` not
/// yet, `Err` never will be.
fn look(dir: &Path, pid: u32, last: &mut Option<TokenError>) -> Result<Option<PathBuf>> {
    match token_file_of(dir, pid) {
        Ok(path) => Ok(Some(path)),
        Err(why) => {
            // Asked after the look, so a process that wrote its file and
            // ended at once is not mistaken for one that never wrote it.
            if crate::process_alive(pid) == Some(false) {
                return Err(TokenError::Ended {
                    dir: dir.to_owned(),
                    pid,
                });
            }
            *last = (!why.absent()).then_some(why);
            Ok(None)
        }
    }
}

fn not_yet(dir: &Path, pid: u32, waited: Duration, last: Option<TokenError>) -> TokenError {
    TokenError::NotYet {
        dir: dir.to_owned(),
        pid,
        waited,
        last: last.map(Box::new),
    }
}

/// Waits up to `limit` for process `pid`, just started, to write its token
/// file in `dir` (or its `target/ljos`), as [`token_file_of`] believes one.
///
/// The inspector writes its file whole, by renaming it into place, so the
/// first look that finds it can read it. The waits between looks double
/// from 2 ms to 100 ms: a start that is quick is seen at once, and a slow
/// one is asked a few times a second, only for as long as `limit`. A
/// process that ends first is said to have ended rather than waited out.
pub async fn wait_for_token(dir: &Path, pid: u32, limit: Duration) -> Result<PathBuf> {
    let start = tokio::time::Instant::now();
    let mut pause = FIRST;
    let mut last = None;
    loop {
        if let Some(path) = look(dir, pid, &mut last)? {
            return Ok(path);
        }
        let waited = start.elapsed();
        if waited >= limit {
            return Err(not_yet(dir, pid, waited, last));
        }
        tokio::time::sleep(pause.min(limit - waited)).await;
        pause = (pause * 2).min(LONGEST);
    }
}

/// [`wait_for_token`] for code without a runtime: the waits sleep the
/// calling thread, and no runtime is entered, so it may be called from
/// anywhere, a runtime's worker thread included.
pub fn wait_for_token_blocking(dir: &Path, pid: u32, limit: Duration) -> Result<PathBuf> {
    let start = std::time::Instant::now();
    let mut pause = FIRST;
    let mut last = None;
    loop {
        if let Some(path) = look(dir, pid, &mut last)? {
            return Ok(path);
        }
        let waited = start.elapsed();
        if waited >= limit {
            return Err(not_yet(dir, pid, waited, last));
        }
        std::thread::sleep(pause.min(limit - waited));
        pause = (pause * 2).min(LONGEST);
    }
}

/// Where a token file's `url` points.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Address {
    /// `http://host:port/path`.
    Tcp { authority: String, path: String },
    /// `unix:PATH`: the inspector's socket, served at `/mcp`.
    Unix(PathBuf),
}

impl Address {
    pub(super) fn of(url: &str) -> Result<Address> {
        if let Some(path) = url.strip_prefix("unix:") {
            return Ok(Address::Unix(PathBuf::from(path)));
        }
        let Some(rest) = url.strip_prefix("http://") else {
            return Err(TokenError::NotAnAddress {
                url: url.to_owned(),
            });
        };
        let (authority, path) = match rest.find('/') {
            Some(at) => (&rest[..at], &rest[at..]),
            None => (rest, "/"),
        };
        if authority.is_empty() {
            return Err(TokenError::NoHost {
                url: url.to_owned(),
            });
        }
        Ok(Address::Tcp {
            authority: authority.to_owned(),
            path: path.to_owned(),
        })
    }

    /// Turns away an address beyond this machine: an inspector of this
    /// user listens on a loopback port, as an address rather than a name
    /// a resolver could send elsewhere, or on a socket of this user's.
    fn check_local(&self, url: &str) -> Result<()> {
        match self {
            Address::Tcp { authority, .. } => {
                let local = authority
                    .parse::<std::net::SocketAddr>()
                    .is_ok_and(|a| a.ip().is_loopback());
                if !local {
                    return Err(TokenError::NotLoopback {
                        url: url.to_owned(),
                    });
                }
            }
            #[cfg(unix)]
            Address::Unix(socket) => {
                use std::os::unix::fs::{FileTypeExt as _, MetadataExt as _};
                let meta =
                    std::fs::symlink_metadata(socket).map_err(|error| TokenError::NoSocket {
                        url: url.to_owned(),
                        error,
                    })?;
                if !meta.file_type().is_socket() || Some(meta.uid()) != crate::effective_uid() {
                    return Err(TokenError::NotYourSocket {
                        url: url.to_owned(),
                    });
                }
            }
            #[cfg(not(unix))]
            Address::Unix(_) => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    pub(crate) const TOKEN: &str =
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const NAMED: Trust = Trust {
        named: true,
        remote: false,
    };
    const REMOTE: Trust = Trust {
        named: true,
        remote: true,
    };

    /// Writes `body` to `path` as an inspector writes its token file.
    fn write_token(path: &Path, body: &Json) {
        use std::io::Write as _;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let mut file = options.open(path).unwrap();
        file.write_all(body.to_string().as_bytes()).unwrap();
    }

    fn mine(dir: &Path) -> PathBuf {
        dir.join(format!("mcp-{}.json", std::process::id()))
    }

    fn genuine(url: &str) -> Json {
        json!({"url": url, "token": TOKEN, "pid": std::process::id()})
    }

    #[test]
    fn the_newest_live_token_file_in_a_directory_is_found() {
        let dir = tempfile::tempdir().unwrap();
        let below = dir.path().join("target/ljos");
        std::fs::create_dir_all(&below).unwrap();
        let file = |pid: u32| {
            let path = below.join(format!("mcp-{pid}.json"));
            let body = json!({"url": "http://127.0.0.1:1/mcp", "token": TOKEN, "pid": pid});
            write_token(&path, &body);
            path
        };
        let mine = file(std::process::id());
        // No process has this pid: the kernel's limit is below it.
        file(u32::MAX - 1);
        assert_eq!(
            find_token_file(dir.path()).unwrap(),
            (mine.clone(), Trust::FOUND)
        );
        assert_eq!(find_token_file(&mine).unwrap(), (mine.clone(), NAMED));
        assert_eq!(token_file_of(dir.path(), std::process::id()).unwrap(), mine);
        let why = token_file_of(dir.path(), u32::MAX - 1).unwrap_err();
        assert!(
            why.to_string().contains("names no running process"),
            "{why}"
        );
    }

    /// Why `path` is not believed as a file found, and that the directory
    /// it is in offers no inspector.
    fn refused(dir: &Path, path: &Path) -> String {
        assert!(
            find_token_file(dir).is_err(),
            "{} was found",
            path.display()
        );
        format!("{:#}", read_token_file(path, Trust::FOUND).unwrap_err())
    }

    #[cfg(unix)]
    #[test]
    fn a_pipe_or_a_link_found_is_not_opened() {
        let dir = tempfile::tempdir().unwrap();
        let path = mine(dir.path());
        let made = std::process::Command::new("mkfifo")
            .arg(&path)
            .status()
            .unwrap();
        assert!(made.success());
        // Opening it to read would wait for a writer that never comes.
        let why = refused(dir.path(), &path);
        assert!(why.contains("not a plain file"), "{why}");
        // Named, it is opened without waiting, and turned away.
        let why = read_token_file(&path, NAMED).unwrap_err().to_string();
        assert!(why.contains("not a file"), "{why}");
        std::fs::remove_file(&path).unwrap();

        let elsewhere = tempfile::tempdir().unwrap();
        let real = elsewhere.path().join("real.json");
        write_token(&real, &genuine("http://127.0.0.1:1/mcp"));
        std::os::unix::fs::symlink(&real, &path).unwrap();
        let why = refused(dir.path(), &path);
        assert!(why.contains("not a plain file"), "{why}");
        // A link the user names is the user's choice.
        assert!(read_token_file(&path, NAMED).is_ok());
    }

    #[test]
    fn a_file_found_names_its_own_live_process() {
        let dir = tempfile::tempdir().unwrap();
        let path = mine(dir.path());
        write_token(
            &path,
            &json!({"url": "http://127.0.0.1:1/mcp", "token": TOKEN}),
        );
        let why = refused(dir.path(), &path);
        assert!(why.contains("does not name the process"), "{why}");
        // Named, a file without a pid is still read.
        assert!(read_token_file(&path, NAMED).is_ok());
        std::fs::remove_file(&path).unwrap();

        let other = dir.path().join("mcp-1.json");
        write_token(&other, &genuine("http://127.0.0.1:1/mcp"));
        let why = refused(dir.path(), &other);
        assert!(why.contains("does not name the process"), "{why}");
    }

    #[test]
    fn a_file_found_holds_an_inspectors_token_and_no_more() {
        let dir = tempfile::tempdir().unwrap();
        let path = mine(dir.path());
        let mut body = genuine("http://127.0.0.1:1/mcp");
        body["token"] = json!("t");
        write_token(&path, &body);
        let why = refused(dir.path(), &path);
        assert!(why.contains("no inspector's token"), "{why}");
        std::fs::remove_file(&path).unwrap();

        let mut body = genuine("http://127.0.0.1:1/mcp");
        body["padding"] = json!("x".repeat(MAX_TOKEN_FILE as usize));
        write_token(&path, &body);
        let why = refused(dir.path(), &path);
        assert!(why.contains("larger than a token file"), "{why}");
    }

    #[cfg(unix)]
    #[test]
    fn a_file_found_is_its_owners_alone() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let path = mine(dir.path());
        write_token(&path, &genuine("http://127.0.0.1:1/mcp"));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let why = refused(dir.path(), &path);
        assert!(why.contains("read by others"), "{why}");
    }

    #[test]
    fn a_file_found_gives_an_address_on_this_machine() {
        for url in [
            "http://10.0.0.1:80/mcp",
            "http://example.com:80/mcp",
            // A name, which a resolver could send anywhere.
            "http://localhost:1/mcp",
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = mine(dir.path());
            write_token(&path, &genuine(url));
            let why = refused(dir.path(), &path);
            assert!(why.contains("not a loopback port"), "{url}: {why}");
            // Named, it is still refused unless `--remote` is given.
            assert!(read_token_file(&path, NAMED).is_err(), "{url}");
            assert_eq!(read_token_file(&path, REMOTE).unwrap().url, url);
            // `--remote` widens nothing for a file found.
            let remote_found = Trust {
                remote: true,
                ..Trust::FOUND
            };
            assert!(read_token_file(&path, remote_found).is_err(), "{url}");
        }
        let dir = tempfile::tempdir().unwrap();
        let path = mine(dir.path());
        write_token(&path, &genuine("http://[::1]:1/mcp"));
        assert!(read_token_file(&path, Trust::FOUND).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn a_file_found_gives_a_socket_of_its_users() {
        let dir = tempfile::tempdir().unwrap();
        // A plain file where the socket should be.
        let not_socket = dir.path().join("x.sock");
        std::fs::write(&not_socket, "").unwrap();
        let path = mine(dir.path());
        write_token(&path, &genuine(&format!("unix:{}", not_socket.display())));
        let why = refused(dir.path(), &path);
        assert!(why.contains("not a socket of yours"), "{why}");
        assert!(read_token_file(&path, REMOTE).is_ok());
    }

    #[test]
    fn a_directory_without_an_inspector_says_how_to_start_one() {
        let dir = tempfile::tempdir().unwrap();
        let why = find_token_file(dir.path()).unwrap_err().to_string();
        assert!(why.contains("ljos run --mcp"), "{why}");
    }

    #[test]
    fn an_inspector_that_is_gone_is_said_to_be_unreached() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp-1.json");
        let gone = dir.path().join("gone.sock");
        let body = json!({"url": format!("unix:{}", gone.display()), "token": TOKEN});
        write_token(&path, &body);
        let why = format!("{:#}", read_token_file(&path, NAMED).unwrap_err());
        assert!(why.contains("gone.sock"), "{why}");
    }

    #[test]
    fn an_address_is_a_port_or_a_socket() {
        assert_eq!(
            Address::of("http://127.0.0.1:7/mcp").unwrap(),
            Address::Tcp {
                authority: "127.0.0.1:7".into(),
                path: "/mcp".into()
            }
        );
        assert_eq!(
            Address::of("unix:/run/x.sock").unwrap(),
            Address::Unix("/run/x.sock".into())
        );
        let why = Address::of("https://example.com/mcp").unwrap_err();
        assert!(why.to_string().contains("unix:"), "{why}");
    }

    /// A token file that appears while it is waited for is found, by the
    /// async wait and the blocking one alike.
    #[test]
    fn a_token_file_written_while_waited_for_is_found() {
        let dir = tempfile::tempdir().unwrap();
        let below = dir.path().join("target/ljos");
        let path = below.join(format!("mcp-{}.json", std::process::id()));
        let writer = {
            let (below, path) = (below.clone(), path.clone());
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(30));
                std::fs::create_dir_all(&below).unwrap();
                write_token(&path, &genuine("http://127.0.0.1:1/mcp"));
            })
        };
        let found =
            wait_for_token_blocking(dir.path(), std::process::id(), Duration::from_secs(20))
                .unwrap();
        writer.join().unwrap();
        assert_eq!(found, path);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        let found = runtime
            .block_on(wait_for_token(
                dir.path(),
                std::process::id(),
                Duration::from_secs(20),
            ))
            .unwrap();
        assert_eq!(found, path);
    }

    #[test]
    fn a_token_file_never_written_is_waited_for_no_longer_than_asked() {
        let dir = tempfile::tempdir().unwrap();
        let why =
            wait_for_token_blocking(dir.path(), std::process::id(), Duration::from_millis(50))
                .unwrap_err();
        assert!(
            matches!(why, TokenError::NotYet { last: None, .. }),
            "{why}"
        );
        // A file there that is not believed is the reason given.
        let path = mine(dir.path());
        let mut body = genuine("http://127.0.0.1:1/mcp");
        body["token"] = json!("t");
        write_token(&path, &body);
        let why =
            wait_for_token_blocking(dir.path(), std::process::id(), Duration::from_millis(50))
                .unwrap_err();
        assert!(why.to_string().contains("no inspector's token"), "{why}");
    }

    #[test]
    fn a_process_that_ends_before_writing_is_not_waited_out() {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--list")
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id();
        // Reaped, so the pid is not a zombie that still answers `kill(0)`.
        child.wait().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let why = wait_for_token_blocking(dir.path(), pid, Duration::from_secs(20)).unwrap_err();
        assert!(matches!(why, TokenError::Ended { .. }), "{why}");
    }
}
