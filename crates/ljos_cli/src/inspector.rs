//! A client of a running application's inspector (ARCHITECTURE.md §19.7):
//! the token file that names it, and one JSON-RPC message posted to it as
//! streamable HTTP, on the loopback address or on its 0600 Unix socket.
//! `ljos observe` and `ljos attach` are its two users.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime};

use anyhow::{Context as _, bail};
use bytes::Bytes;
use http_body_util::{BodyExt as _, Full};
use hyper_util::rt::TokioIo;
use ljos_mcp_core::TokenFile;
use serde_json::Value as Json;
use tokio::io::{AsyncRead, AsyncWrite};

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

/// More than any inspector writes; a file past it is not read on.
const MAX_TOKEN_FILE: u64 = 4096;

pub fn read_token_file(path: &Path, trust: Trust) -> anyhow::Result<Target> {
    let text = read_bounded(path, trust)?;
    let body: Json =
        serde_json::from_str(&text).with_context(|| format!("{} is not JSON", path.display()))?;
    let field = |name: &str| {
        body[name]
            .as_str()
            .map(str::to_owned)
            .with_context(|| format!("{} has no {name:?}", path.display()))
    };
    let url = field("url")?;
    // Said now rather than as a connection that fails on every call.
    let address = Address::of(&url)?;
    let token = field("token")?;
    // An inspector's token is 256 random bits in hex; anything else was
    // not written by one.
    if token.len() != 64 || !token.bytes().all(|c| c.is_ascii_hexdigit()) {
        bail!("{} has no inspector's token", path.display());
    }
    let pid = body["pid"].as_u64().and_then(|p| u32::try_from(p).ok());
    if !trust.named {
        let named = pid_in_name(path);
        if pid.is_none() || pid != named {
            bail!("{} does not name the process its name does", path.display());
        }
        if pid.and_then(ljos_mcp_core::process_alive) != Some(true) {
            bail!("{} names no running process", path.display());
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
fn read_bounded(path: &Path, trust: Trust) -> anyhow::Result<String> {
    use std::io::Read as _;
    let shown = || path.display().to_string();
    // Asked of the name itself, before anything is opened: a link, a
    // pipe or a device in a directory is not what an inspector writes.
    let before = if trust.named {
        None
    } else {
        let meta = std::fs::symlink_metadata(path).with_context(shown)?;
        check_found(path, &meta)?;
        Some(meta)
    };
    // A pipe put in place after the check above opens at once rather than
    // waiting for a writer; it is turned away next.
    let file = TokenFile::open_for_reading(path, trust.named).with_context(shown)?;
    let meta = file.metadata().with_context(shown)?;
    if !meta.is_file() {
        bail!("{} is not a file", path.display());
    }
    if let Some(before) = before {
        check_found(path, &meta)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            // What was checked is what was opened.
            if (before.dev(), before.ino()) != (meta.dev(), meta.ino()) {
                bail!("{} changed while it was read", path.display());
            }
        }
        #[cfg(not(unix))]
        let _ = before;
    }
    if meta.len() > MAX_TOKEN_FILE {
        bail!("{} is larger than a token file", path.display());
    }
    let mut text = String::new();
    file.take(MAX_TOKEN_FILE + 1)
        .read_to_string(&mut text)
        .with_context(shown)?;
    if text.len() as u64 > MAX_TOKEN_FILE {
        bail!("{} is larger than a token file", path.display());
    }
    Ok(text)
}

/// What a token file found in a directory has to be: a plain file, and
/// on Unix its user's own, readable by nobody else.
fn check_found(path: &Path, meta: &std::fs::Metadata) -> anyhow::Result<()> {
    if !meta.file_type().is_file() {
        bail!("{} is not a plain file", path.display());
    }
    if meta.len() > MAX_TOKEN_FILE {
        bail!("{} is larger than a token file", path.display());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        if Some(meta.uid()) != ljos_mcp_core::effective_uid() {
            bail!("{} belongs to another user", path.display());
        }
        if meta.mode() & 0o077 != 0 {
            bail!("{} can be read by others", path.display());
        }
    }
    Ok(())
}

/// The token file `target` names, or the newest live one in it, with how
/// far it is to be believed.
pub fn find_token_file(target: &Path) -> anyhow::Result<(PathBuf, Trust)> {
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
        bail!(
            "{} is neither a token file nor a directory",
            target.display()
        );
    }
    let mut found: Vec<(SystemTime, PathBuf)> = Vec::new();
    for dir in [target.to_owned(), target.join("target/ljos")] {
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
        None => bail!(
            "no running application with an inspector of yours in {}: start \
             one with `ljos run --mcp FILE` in that directory, or name its \
             token file",
            target.display()
        ),
    }
}

/// Where a token file's `url` points.
#[derive(Debug, PartialEq, Eq)]
enum Address {
    /// `http://host:port/path`.
    Tcp { authority: String, path: String },
    /// `unix:PATH`: the inspector's socket, served at `/mcp`.
    Unix(PathBuf),
}

impl Address {
    fn of(url: &str) -> anyhow::Result<Address> {
        if let Some(path) = url.strip_prefix("unix:") {
            return Ok(Address::Unix(PathBuf::from(path)));
        }
        let Some(rest) = url.strip_prefix("http://") else {
            bail!("{url} is neither an `http://` address nor a `unix:` socket");
        };
        let (authority, path) = match rest.find('/') {
            Some(at) => (&rest[..at], &rest[at..]),
            None => (rest, "/"),
        };
        if authority.is_empty() {
            bail!("{url} names no host");
        }
        Ok(Address::Tcp {
            authority: authority.to_owned(),
            path: path.to_owned(),
        })
    }

    /// Turns away an address beyond this machine: an inspector of this
    /// user listens on a loopback port, as an address rather than a name
    /// a resolver could send elsewhere, or on a socket of this user's.
    fn check_local(&self, url: &str) -> anyhow::Result<()> {
        match self {
            Address::Tcp { authority, .. } => {
                let local = authority
                    .parse::<std::net::SocketAddr>()
                    .is_ok_and(|a| a.ip().is_loopback());
                if !local {
                    bail!(
                        "{url} is not a loopback port: name the token file and \
                         pass `--remote` to reach an inspector elsewhere"
                    );
                }
            }
            #[cfg(unix)]
            Address::Unix(socket) => {
                use std::os::unix::fs::{FileTypeExt as _, MetadataExt as _};
                let meta = std::fs::symlink_metadata(socket)
                    .with_context(|| format!("{url}: no socket there"))?;
                if !meta.file_type().is_socket()
                    || Some(meta.uid()) != ljos_mcp_core::effective_uid()
                {
                    bail!(
                        "{url} is not a socket of yours: name the token file and \
                         pass `--remote` to reach it"
                    );
                }
            }
            #[cfg(not(unix))]
            Address::Unix(_) => {}
        }
        Ok(())
    }
}

/// The longest `Retry-After` that is waited for: a server asking for more
/// is answered with its refusal.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(5);

/// What the inspector sent back: its status and the body, as it came.
#[derive(Debug)]
pub struct Answer {
    pub status: u16,
    pub body: Vec<u8>,
    /// When the inspector turned the call away (429), how long its
    /// `Retry-After` says to wait.
    pub retry_after: Option<Duration>,
}

impl Answer {
    /// The JSON-RPC messages of the body: the `data:` lines of a stream,
    /// or the body itself when it is plain JSON.
    pub fn messages(&self) -> Vec<&str> {
        let text = std::str::from_utf8(&self.body).unwrap_or("");
        let data: Vec<&str> = text
            .lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        match (data.is_empty(), text.trim()) {
            (true, "") => Vec::new(),
            (true, plain) => vec![plain],
            (false, _) => data,
        }
    }
}

/// Posts `message` (one JSON-RPC message as text) to `target`; `Err` is
/// why the inspector could not be reached at all. Gives up after `limit`.
///
/// A call turned away for asking too often (429) is asked again once, after
/// the `Retry-After` the inspector gave, within `limit`: it was refused
/// before anything ran, so asking again cannot do a thing twice. A second
/// refusal is the answer.
pub async fn post(target: &Target, message: String, limit: Duration) -> Result<Answer, String> {
    let asked = async {
        let first = post_now(target, message.clone()).await?;
        match first.retry_after {
            Some(wait) if first.status == 429 => {
                tokio::time::sleep(wait.min(MAX_RETRY_AFTER)).await;
                post_now(target, message).await
            }
            _ => Ok(first),
        }
    };
    match tokio::time::timeout(limit, asked).await {
        Ok(answer) => answer,
        Err(_) => Err(format!(
            "the inspector at {} did not answer within {} s",
            target.url,
            limit.as_secs()
        )),
    }
}

async fn post_now(target: &Target, message: String) -> Result<Answer, String> {
    let address = Address::of(&target.url).map_err(|e| e.to_string())?;
    let fail = |e: std::io::Error| format!("{}: {e}", target.url);
    match address {
        Address::Tcp { authority, path } => {
            let stream = tokio::net::TcpStream::connect(authority.as_str())
                .await
                .map_err(fail)?;
            exchange(stream, &authority, &path, &target.token, message).await
        }
        #[cfg(unix)]
        Address::Unix(socket) => {
            let stream = tokio::net::UnixStream::connect(&socket)
                .await
                .map_err(fail)?;
            // The inspector answers only requests addressed to this
            // machine, whichever way they come.
            exchange(stream, "localhost", "/mcp", &target.token, message).await
        }
        #[cfg(not(unix))]
        Address::Unix(_) => Err(format!("{}: no Unix sockets here", target.url)),
    }
}

struct AbortConnection(tokio::task::AbortHandle);

impl Drop for AbortConnection {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn exchange<S>(
    stream: S,
    host: &str,
    path: &str,
    token: &str,
    message: String,
) -> Result<Answer, String>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .map_err(|e| e.to_string())?;
    // A timeout or oversized body must also stop the HTTP driver.
    let connection = tokio::spawn(connection);
    let _connection = AbortConnection(connection.abort_handle());
    let request = http::Request::post(path)
        .header(http::header::HOST, host)
        .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
        .header(http::header::ACCEPT, "application/json, text/event-stream")
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Full::new(Bytes::from(message)))
        .map_err(|e| e.to_string())?;
    let response = sender
        .send_request(request)
        .await
        .map_err(|e| e.to_string())?;
    let status = response.status().as_u16();
    let retry_after = response
        .headers()
        .get(http::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(Duration::from_secs);
    let body = read_body(response.into_body()).await?;
    Ok(Answer {
        status,
        body,
        retry_after,
    })
}

// DECISION: match the server's 1 MiB request cap for inspector responses.
// Count streaming frames rather than trusting Content-Length.
pub const MAX_BODY_BYTES: usize = 1024 * 1024;

async fn read_body<B>(mut body: B) -> Result<Vec<u8>, String>
where
    B: hyper::body::Body<Data = Bytes> + Unpin,
    B::Error: std::fmt::Display,
{
    let mut bytes = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|e| e.to_string())?;
        if let Ok(data) = frame.into_data() {
            if data.len() > MAX_BODY_BYTES.saturating_sub(bytes.len()) {
                return Err("inspector response exceeds 1 MiB".into());
            }
            bytes.extend_from_slice(&data);
        }
    }
    Ok(bytes)
}

/// Calls in flight on the client thread at most; one more is refused
/// rather than queued, as the inspector refuses one past its own queue.
pub const IN_FLIGHT: usize = 16;

type Job = Pin<Box<dyn Future<Output = ()> + Send>>;

struct Thread {
    tx: tokio::sync::mpsc::UnboundedSender<Job>,
    in_flight: Arc<AtomicUsize>,
}

/// Runs `job` on the one client thread of the process, made the first
/// time it is asked for; `Err` says why it was not run. A thread per call
/// would be born, wake the process and end each time; this one sleeps in
/// its reactor between calls, with no timer, so an idle window costs
/// nothing.
pub fn spawn(job: impl Future<Output = ()> + Send + 'static) -> Result<(), String> {
    static THREAD: OnceLock<Result<Thread, String>> = OnceLock::new();
    let thread = THREAD.get_or_init(start).as_ref().map_err(Clone::clone)?;
    let in_flight = Arc::clone(&thread.in_flight);
    if in_flight.fetch_add(1, Ordering::AcqRel) >= IN_FLIGHT {
        in_flight.fetch_sub(1, Ordering::AcqRel);
        return Err(format!(
            "busy: {IN_FLIGHT} calls are waiting for the inspector already"
        ));
    }
    let job = Box::pin(async move {
        job.await;
        in_flight.fetch_sub(1, Ordering::AcqRel);
    });
    thread
        .tx
        .send(job)
        .map_err(|_| "the client thread has ended".to_owned())
}

fn start() -> Result<Thread, String> {
    // Unbounded in type only: `spawn` counts what is in flight.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Job>();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(|e| format!("no runtime for the client: {e}"))?;
    std::thread::Builder::new()
        .name("ljos-observe".to_owned())
        .spawn(move || {
            runtime.block_on(async move {
                while let Some(job) = rx.recv().await {
                    tokio::spawn(job);
                }
            });
        })
        .map_err(|e| format!("no thread for the client: {e}"))?;
    Ok(Thread {
        tx,
        in_flight: Arc::new(AtomicUsize::new(0)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// An inspector that turns the first call away for a moment, as its
    /// rate limit does, and answers the second.
    #[tokio::test]
    async fn a_call_turned_away_is_asked_again_after_the_retry_after() {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let served = tokio::spawn(async move {
            let mut statuses = Vec::new();
            for reply in [
                "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 0\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
            ] {
                let (mut conn, _) = listener.accept().await.unwrap();
                let mut buf = vec![0u8; 4096];
                let _ = conn.read(&mut buf).await.unwrap();
                conn.write_all(reply.as_bytes()).await.unwrap();
                statuses.push(reply.len());
            }
            statuses.len()
        });
        let target = Target {
            url: format!("http://127.0.0.1:{port}/mcp"),
            token: "t".into(),
            pid: None,
        };
        let answer = post(&target, "{}".into(), Duration::from_secs(10))
            .await
            .unwrap();
        assert_eq!(answer.status, 200);
        assert_eq!(served.await.unwrap(), 2);
    }

    #[tokio::test]
    async fn response_frames_are_capped_even_without_content_length() {
        use hyper::body::Frame;
        use std::collections::VecDeque;
        struct Frames(VecDeque<Bytes>);
        impl hyper::body::Body for Frames {
            type Data = Bytes;
            type Error = std::convert::Infallible;
            fn poll_frame(
                mut self: Pin<&mut Self>,
                _: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
                std::task::Poll::Ready(self.0.pop_front().map(|data| Ok(Frame::data(data))))
            }
        }
        let exact = Bytes::from(vec![b'x'; MAX_BODY_BYTES]);
        assert_eq!(
            read_body(Full::new(exact.clone())).await.unwrap().len(),
            MAX_BODY_BYTES
        );
        let oversized = Frames(VecDeque::from([exact, Bytes::from_static(b"x")]));
        assert!(
            read_body(oversized)
                .await
                .unwrap_err()
                .contains("exceeds 1 MiB")
        );
        assert_eq!(
            read_body(Full::new(Bytes::from_static(b"valid")))
                .await
                .unwrap(),
            b"valid"
        );
    }

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
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
        assert_eq!(find_token_file(&mine).unwrap(), (mine, NAMED));
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

    #[test]
    fn an_answer_is_read_plain_or_streamed() {
        let answer = |body: &str| Answer {
            status: 200,
            body: body.as_bytes().to_vec(),
            retry_after: None,
        };
        let plain = r#"{"jsonrpc":"2.0","id":1,"result":{}}"#;
        assert_eq!(answer(plain).messages(), [plain]);
        let streamed = format!("event: message\ndata: {plain}\n\nid: 2\ndata: {plain}\n\n");
        assert_eq!(answer(&streamed).messages(), [plain, plain]);
        assert!(answer("").messages().is_empty());
    }

    /// An inspector with one tool, `echo`, as `ljos run --mcp` serves one.
    struct Echo(ljos_mcp_core::ToolRegistry);

    impl ljos_mcp_core::McpSurface for Echo {
        fn meta(&self) -> ljos_mcp_core::ServerMeta {
            ljos_mcp_core::ServerMeta {
                name: "echo",
                version: "0",
                instructions: String::new(),
            }
        }
        fn tools(&self) -> &ljos_mcp_core::ToolRegistry {
            &self.0
        }
        fn call_tool<'a>(
            &'a self,
            name: &'a str,
            ctx: &'a ljos_mcp_core::CallCtx,
            args: Json,
        ) -> ljos_mcp_core::ToolFuture<'a> {
            Box::pin(async move {
                let (v, _) = self.0.call(name, ctx, args);
                Ok(ljos_mcp_core::ToolReply::from(v?))
            })
        }
    }

    fn serve(
        dir: &Path,
        listen: ljos_mcp_core::glue::http::Listen,
    ) -> ljos_mcp_core::glue::http::HttpServer {
        use ljos_mcp_core::glue::http::{HttpConfig, HttpServer};
        use ljos_mcp_core::{Cap, CapSet, ToolDef, ToolRegistry, typed};
        let echo = Echo(ToolRegistry::new().with(
            ToolDef::new::<Json>("echo", "Says it back."),
            typed(|_, a: Json| Ok(json!({ "said": a }))),
        ));
        let mut config =
            HttpConfig::loopback(CapSet::of([Cap::Inspect])).token_file(dir.join("mcp-1.json"));
        config.listen = listen;
        HttpServer::start(Arc::new(echo), config).unwrap()
    }

    /// `echo` called through the client thread, as the observer calls.
    fn echo(token_file: &Path, trust: Trust) -> Result<Answer, String> {
        let target = read_token_file(token_file, trust).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let call = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"echo","arguments":{"say":"hi"}}}"#;
        spawn(async move {
            let _ = tx.send(post(&target, call.to_owned(), Duration::from_secs(20)).await);
        })
        .unwrap();
        rx.recv_timeout(Duration::from_secs(30)).unwrap()
    }

    fn said(answer: &Answer) -> Json {
        assert_eq!(answer.status, 200, "{:?}", answer.messages());
        let message: Json = serde_json::from_str(answer.messages()[0]).unwrap();
        message["result"]["structuredContent"]["said"].clone()
    }

    #[test]
    fn an_inspector_is_reached_on_its_port_and_on_its_socket() {
        use ljos_mcp_core::glue::http::Listen;
        let dir = tempfile::Builder::new().prefix("lj").tempdir().unwrap();
        let mut on_port = serve(dir.path(), Listen::Loopback(0));
        let answer = echo(&dir.path().join("mcp-1.json"), NAMED).unwrap();
        assert_eq!(said(&answer), json!({"say": "hi"}));
        on_port.stop();

        let socket = dir.path().join("m.sock");
        let mut on_socket = serve(dir.path(), Listen::Unix(socket.clone()));
        let target = read_token_file(&dir.path().join("mcp-1.json"), NAMED).unwrap();
        assert_eq!(target.url, format!("unix:{}", socket.display()));
        let answer = echo(&dir.path().join("mcp-1.json"), NAMED).unwrap();
        assert_eq!(said(&answer), json!({"say": "hi"}));
        on_socket.stop();
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
        let why = echo(&path, REMOTE).unwrap_err();
        assert!(why.contains("gone.sock"), "{why}");
    }
}
