//! Streamable HTTP for a surface, on this machine only (ARCHITECTURE.md
//! §19.7).
//!
//! The server has its own thread with a single-threaded tokio runtime, so
//! an application needs no runtime of its own to be inspected. That thread
//! sleeps in the kernel until a client writes: there is no keep-alive ping,
//! no session to expire and no timer, so a connected client that asks
//! nothing costs no CPU. Each `POST` is answered on its own (MCP's
//! stateless mode), which is also what lets a client reconnect after the
//! application was restarted without a handshake to repeat.
//!
//! The server listens on this machine: the loopback address or a Unix
//! socket only its owner can open. Any other address needs two words, an
//! address given on purpose and [`HttpConfig::insecure_lan`], and comes
//! with a loud warning ([`LAN_WARNING`]); the token still has to be shown.

use std::convert::Infallible;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::Bytes;
use http::{Request, Response, StatusCode, header};
use http_body_util::{BodyExt as _, Full, Limited, combinators::BoxBody};
use hyper::body::Incoming;
use hyper_util::rt::{TokioIo, TokioTimer};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::never::NeverSessionManager,
};
use tokio::sync::{Notify, Semaphore};

use super::RmcpServer;
use crate::{CapSet, McpSurface, RateLimit, Token, TokenFile, TokenFileError};

/// The path the server answers on.
pub const MCP_PATH: &str = "/mcp";

// Absolute read deadlines stop trickling clients from retaining connection permits.
// Only request input is timed: app_watch may legitimately wait for an hour.
const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(5);
const BODY_READ_TIMEOUT: Duration = Duration::from_secs(5);
// Inspector requests carry commands, not assets. Bound allocation even for chunked input.
const MAX_REQUEST_BODY: usize = 1024 * 1024;

/// What is printed, loudly, when the server listens beyond this machine.
pub const LAN_WARNING: &str = "the inspector listens on an address that is not this machine's: \
     anyone on the network who gets the token can read this application and, with the \
     capabilities granted, drive it. Use it on a network you trust, for as long as you need it";

/// Where the server listens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Listen {
    /// `127.0.0.1` and this port; `0` lets the system choose one.
    Loopback(u16),
    /// An address given on purpose (`--mcp-bind`). One that is not this
    /// machine's is refused unless [`HttpConfig::insecure_lan`] is set.
    Address(SocketAddr),
    /// A Unix socket only its owner can connect to.
    #[cfg(unix)]
    Unix(PathBuf),
}

#[derive(Clone, Debug)]
pub struct HttpConfig {
    pub listen: Listen,
    /// What every client of this server may do.
    pub caps: CapSet,
    /// Where the address and the token are written for clients to read.
    pub token_file: Option<PathBuf>,
    /// §19.7: at most 20.
    pub calls_per_second: usize,
    /// More connections than this are closed as they come.
    pub max_connections: usize,
    /// The second word that an address beyond this machine needs
    /// (`--mcp-insecure-lan`).
    pub insecure_lan: bool,
    /// Remove the token file and the socket, and end the process as the
    /// signal would have, when it gets SIGINT or SIGTERM. A process that is
    /// killed by a signal runs no destructor, so without this its token
    /// file would stay and name an address where nothing listens. For the
    /// process of an application, not for a test that hosts a server.
    pub clean_up_on_signals: bool,
}

impl HttpConfig {
    pub fn loopback(caps: CapSet) -> Self {
        HttpConfig {
            listen: Listen::Loopback(0),
            caps,
            token_file: None,
            calls_per_second: 20,
            max_connections: 32,
            insecure_lan: false,
            clean_up_on_signals: false,
        }
    }

    #[must_use]
    pub fn token_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.token_file = Some(path.into());
        self
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("cannot listen on {what}: {source}")]
    Bind {
        what: String,
        source: std::io::Error,
    },
    #[error(
        "cannot listen on {addr}: it is not an address of this machine. Start the application \
         with `--mcp-bind {addr}` and `--mcp-insecure-lan` to listen there anyway"
    )]
    NotLoopback { addr: SocketAddr },
    #[error(transparent)]
    SocketPath(#[from] crate::SocketPathError),
    #[error(transparent)]
    Token(#[from] TokenFileError),
    #[error("cannot start the MCP server thread: {0}")]
    Thread(std::io::Error),
}

/// Whether the server may listen on `addr`: any address of this machine,
/// and one beyond it only with `insecure_lan`. Returns the warning that
/// must be shown when it is one beyond.
///
/// # Errors
/// `addr` is not this machine's and `insecure_lan` is not set.
pub fn bind_policy(
    addr: SocketAddr,
    insecure_lan: bool,
) -> Result<Option<&'static str>, HttpError> {
    if addr.ip().is_loopback() {
        Ok(None)
    } else if insecure_lan {
        Ok(Some(LAN_WARNING))
    } else {
        Err(HttpError::NotLoopback { addr })
    }
}

/// A running server. It stops, and its token file and socket are removed,
/// when this is dropped.
pub struct HttpServer {
    url: String,
    addr: Option<SocketAddr>,
    token: Token,
    token_file: Option<TokenFile>,
    socket: Option<PathBuf>,
    warning: Option<&'static str>,
    shutdown: Arc<Notify>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl std::fmt::Debug for HttpServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpServer")
            .field("url", &self.url)
            .field("token_file", &self.token_file)
            .finish_non_exhaustive()
    }
}

enum Listener {
    Tcp(std::net::TcpListener),
    #[cfg(unix)]
    Unix(std::os::unix::net::UnixListener),
}

type Body = BoxBody<Bytes, Infallible>;

struct State<S: McpSurface> {
    token: Token,
    port: Option<u16>,
    /// Bound beyond this machine: `Host` may be any address.
    lan: bool,
    rate: Mutex<RateLimit>,
    service: StreamableHttpService<RmcpServer<S>, NeverSessionManager>,
}

impl HttpServer {
    /// Starts listening. The socket is bound before this returns, so the
    /// address in the token file is one a client can connect to at once.
    pub fn start<S: McpSurface>(surface: Arc<S>, config: HttpConfig) -> Result<Self, HttpError> {
        let token = Token::generate()?;
        let wanted = match &config.listen {
            Listen::Loopback(port) => Some(SocketAddr::from((Ipv4Addr::LOCALHOST, *port))),
            Listen::Address(addr) => Some(*addr),
            #[cfg(unix)]
            Listen::Unix(_) => None,
        };
        let mut warning = None;
        if let Some(addr) = wanted {
            warning = bind_policy(addr, config.insecure_lan)?;
        }
        let lan = warning.is_some();
        let (listener, url, addr, socket) = match &config.listen {
            Listen::Loopback(_) | Listen::Address(_) => {
                let wanted = wanted.unwrap_or_else(|| SocketAddr::from((Ipv4Addr::LOCALHOST, 0)));
                let bind = |source| HttpError::Bind {
                    what: wanted.to_string(),
                    source,
                };
                let l = std::net::TcpListener::bind(wanted).map_err(bind)?;
                l.set_nonblocking(true).map_err(bind)?;
                let addr = l.local_addr().map_err(bind)?;
                let url = format!("http://{addr}{MCP_PATH}");
                (Listener::Tcp(l), url, Some(addr), None)
            }
            #[cfg(unix)]
            Listen::Unix(path) => {
                use std::os::unix::fs::PermissionsExt as _;
                // Before anything is removed or made, and never answered by
                // choosing another place.
                crate::check_socket_path(path)?;
                let bind = |source| HttpError::Bind {
                    what: path.display().to_string(),
                    source,
                };
                // A socket file left by a process that died is in the way
                // of `bind`; anything else there is not ours to remove.
                if std::fs::symlink_metadata(path).is_ok_and(|m| {
                    use std::os::unix::fs::FileTypeExt as _;
                    m.file_type().is_socket()
                }) {
                    std::fs::remove_file(path).map_err(bind)?;
                }
                let l = std::os::unix::net::UnixListener::bind(path).map_err(bind)?;
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                    .map_err(bind)?;
                l.set_nonblocking(true).map_err(bind)?;
                let url = format!("unix:{}", path.display());
                (Listener::Unix(l), url, None, Some(path.clone()))
            }
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(HttpError::Thread)?;
        // The handlers are in place before the token file names this
        // process, so a signal that comes the moment a client reads the
        // file is already cleaned up after.
        let ends = if config.clean_up_on_signals {
            let _in_runtime = runtime.enter();
            Ends::install()
        } else {
            None
        };
        let token_file = match &config.token_file {
            Some(path) => Some(TokenFile::write(path, &url, &token, config.caps)?),
            None => None,
        };
        let leftovers: Vec<PathBuf> = config
            .token_file
            .iter()
            .cloned()
            .chain(socket.clone())
            .collect();
        let shutdown = Arc::new(Notify::new());
        let thread = {
            let (token, shutdown) = (token.clone(), shutdown.clone());
            let port = addr.map(|a| a.port());
            std::thread::Builder::new()
                .name("ljos-mcp".into())
                .spawn(move || {
                    runtime.block_on(async move {
                        let caps = config.caps;
                        let service = StreamableHttpService::new(
                            move || Ok(RmcpServer::new(surface.clone(), caps)),
                            Arc::new(NeverSessionManager::default()),
                            StreamableHttpServerConfig::default()
                                // Each of these would be a timer.
                                .with_sse_keep_alive(None)
                                .with_sse_retry(None)
                                .with_legacy_session_mode(false)
                                .with_json_response(true),
                        );
                        let state = Arc::new(State {
                            token,
                            port,
                            lan,
                            rate: Mutex::new(RateLimit::per_second(config.calls_per_second)),
                            service,
                        });
                        let serving = accept(listener, state, config.max_connections, shutdown);
                        match ends {
                            Some(mut ends) => tokio::select! {
                                () = serving => {}
                                code = ends.wait() => {
                                    for path in leftovers {
                                        let _ = std::fs::remove_file(path);
                                    }
                                    // As a shell reports a process that
                                    // a signal ended.
                                    end(code);
                                }
                            },
                            None => serving.await,
                        }
                    });
                })
                .map_err(HttpError::Thread)?
        };
        Ok(HttpServer {
            url,
            addr,
            token,
            token_file,
            socket,
            warning,
            shutdown,
            thread: Some(thread),
        })
    }

    /// `http://127.0.0.1:PORT/mcp`, or `unix:PATH`.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The loopback address; `None` for a Unix socket.
    pub fn addr(&self) -> Option<SocketAddr> {
        self.addr
    }

    pub fn token(&self) -> &Token {
        &self.token
    }

    /// What must be shown to whoever started the server, when it listens
    /// beyond this machine.
    pub fn warning(&self) -> Option<&'static str> {
        self.warning
    }

    pub fn token_file(&self) -> Option<&Path> {
        self.token_file.as_ref().map(TokenFile::path)
    }

    /// Stops listening, closes every connection and removes the token
    /// file. Returns when the server's thread has ended.
    pub fn stop(&mut self) {
        // `notify_one` keeps the wake for a thread that is not waiting yet.
        self.shutdown.notify_one();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        self.token_file = None;
        if let Some(path) = self.socket.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

impl Drop for HttpServer {
    fn drop(&mut self) {
        self.stop();
    }
}

async fn accept<S: McpSurface>(
    listener: Listener,
    state: Arc<State<S>>,
    max_connections: usize,
    shutdown: Arc<Notify>,
) {
    let permits = Arc::new(Semaphore::new(max_connections.max(1)));
    macro_rules! serve {
        ($listener:expr) => {{
            let listener = match $listener {
                Ok(l) => l,
                Err(_) => return,
            };
            loop {
                let stream = tokio::select! {
                    biased;
                    _ = shutdown.notified() => return,
                    conn = listener.accept() => match conn {
                        Ok((stream, _)) => stream,
                        Err(_) => {
                            // Out of descriptors, most likely. Waiting is
                            // what keeps this from spinning; the timer is
                            // there only while the fault lasts.
                            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                            continue;
                        }
                    },
                };
                // Over the cap the connection is dropped, which closes it.
                let Ok(permit) = permits.clone().try_acquire_owned() else {
                    continue;
                };
                let state = state.clone();
                tokio::spawn(async move {
                    let service = hyper::service::service_fn(move |req| {
                        let state = state.clone();
                        async move { Ok::<_, Infallible>(answer(&state, req).await) }
                    });
                    let _ = hyper::server::conn::http1::Builder::new()
                        .timer(TokioTimer::new())
                        .header_read_timeout(HEADER_READ_TIMEOUT)
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                    drop(permit);
                });
            }
        }};
    }
    match listener {
        Listener::Tcp(l) => serve!(tokio::net::TcpListener::from_std(l)),
        #[cfg(unix)]
        Listener::Unix(l) => serve!(tokio::net::UnixListener::from_std(l)),
    }
}

fn refuse(status: StatusCode, code: &str, message: &str) -> Response<Body> {
    let body = serde_json::json!({ "error": { "code": code, "message": message } }).to_string();
    let mut response = Response::new(Full::new(Bytes::from(body)).boxed());
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json"),
    );
    // Refusals can leave unread input, including an unauthenticated body.
    // Close rather than let draining that input retain a connection permit.
    response.headers_mut().insert(
        header::CONNECTION,
        header::HeaderValue::from_static("close"),
    );
    response
}

fn refuse_body(status: StatusCode, message: &str) -> Response<Body> {
    refuse(status, "invalid_request_body", message)
}

/// One request: is it from this machine, does it have the token, is it
/// within the rate, and only then what it asks.
async fn answer<S: McpSurface>(state: &State<S>, request: Request<Incoming>) -> Response<Body> {
    let text = |name: header::HeaderName| {
        request
            .headers()
            .get(name)
            .map(|v| v.to_str().unwrap_or("\u{0}"))
    };
    // A browser page on another site can reach 127.0.0.1 (by a name it
    // made resolve there, or plainly). It cannot choose these two headers.
    let host_ok = |h: &str| {
        if state.lan {
            crate::address_host(h, state.port)
        } else {
            crate::loopback_host(h, state.port)
        }
    };
    if !text(header::HOST).is_some_and(host_ok) {
        return refuse(
            StatusCode::FORBIDDEN,
            "forbidden_host",
            if state.lan {
                "this server answers only requests addressed to it by address, not by a name"
            } else {
                "this server answers only requests addressed to this machine (Host: 127.0.0.1)"
            },
        );
    }
    if text(header::ORIGIN).is_some_and(|o| !crate::loopback_origin(o)) {
        return refuse(
            StatusCode::FORBIDDEN,
            "forbidden_origin",
            "this server does not answer web pages from other sites",
        );
    }
    if !state.token.authorizes(text(header::AUTHORIZATION)) {
        let mut response = refuse(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "send the token of this start of the application as `Authorization: Bearer <token>`; \
             it is in the token file the application named when it started",
        );
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            header::HeaderValue::from_static("Bearer realm=\"ljos-mcp\""),
        );
        return response;
    }
    let allowed = state
        .rate
        .lock()
        .map(|mut r| r.allow(Instant::now()))
        .unwrap_or(true);
    if !allowed {
        let mut response = refuse(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "too many calls in one second; wait a moment and ask again",
        );
        response
            .headers_mut()
            .insert(header::RETRY_AFTER, header::HeaderValue::from_static("1"));
        return response;
    }
    if request.uri().path() != MCP_PATH {
        return refuse(
            StatusCode::NOT_FOUND,
            "not_found",
            "the MCP endpoint of this server is /mcp",
        );
    }
    let (parts, body) = request.into_parts();
    if parts
        .headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .is_some_and(|n| n > MAX_REQUEST_BODY as u64)
    {
        return refuse_body(StatusCode::PAYLOAD_TOO_LARGE, "request body exceeds 1 MiB");
    }
    let body = match tokio::time::timeout(
        BODY_READ_TIMEOUT,
        Limited::new(body, MAX_REQUEST_BODY).collect(),
    )
    .await
    {
        Ok(Ok(body)) => body.to_bytes(),
        Ok(Err(error)) => {
            let status = if error.is::<http_body_util::LengthLimitError>() {
                StatusCode::PAYLOAD_TOO_LARGE
            } else {
                StatusCode::BAD_REQUEST
            };
            return refuse_body(status, "request body is invalid or exceeds 1 MiB");
        }
        Err(_) => {
            return refuse_body(
                StatusCode::REQUEST_TIMEOUT,
                "request body read exceeded 5 seconds",
            );
        }
    };
    state
        .service
        .handle(Request::from_parts(parts, Full::new(body)))
        .await
}

/// SIGINT and SIGTERM, awaited by the server's thread.
#[cfg(unix)]
struct Ends {
    interrupt: tokio::signal::unix::Signal,
    terminate: tokio::signal::unix::Signal,
}

#[cfg(unix)]
impl Ends {
    /// Must be called inside the runtime. `None` when the system will not
    /// say: the server then runs as it would without the wish.
    fn install() -> Option<Ends> {
        use tokio::signal::unix::{SignalKind, signal};
        Some(Ends {
            interrupt: signal(SignalKind::interrupt()).ok()?,
            terminate: signal(SignalKind::terminate()).ok()?,
        })
    }

    /// The exit status a shell reports for the signal that came.
    async fn wait(&mut self) -> i32 {
        tokio::select! {
            _ = self.interrupt.recv() => 128 + 2,
            _ = self.terminate.recv() => 128 + 15,
        }
    }
}

/// Ends the process the way the signal would have, without running exit
/// handlers or library destructors. They would run on this thread while
/// the window's thread still draws with what they free: the GL driver's
/// destructors did that and crashed the process on its way out.
#[cfg(unix)]
fn end(code: i32) -> ! {
    // SAFETY: `_exit` takes every thread down at once and reads nothing
    // of the process's state.
    unsafe { libc::_exit(code) }
}

/// Where there are no such signals the server has nothing to wait for.
#[cfg(not(unix))]
struct Ends;

#[cfg(not(unix))]
fn end(code: i32) -> ! {
    std::process::exit(code)
}

#[cfg(not(unix))]
impl Ends {
    fn install() -> Option<Ends> {
        None
    }

    async fn wait(&mut self) -> i32 {
        std::future::pending().await
    }
}
