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
//! There is no way to listen on another machine's behalf: [`Listen`] names
//! the loopback address and a Unix socket and nothing else.

use std::convert::Infallible;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use bytes::Bytes;
use http::{Request, Response, StatusCode, header};
use http_body_util::{BodyExt as _, Full, combinators::BoxBody};
use hyper::body::Incoming;
use hyper_util::rt::TokioIo;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::never::NeverSessionManager,
};
use tokio::sync::{Notify, Semaphore};

use super::RmcpServer;
use crate::{CapSet, McpSurface, RateLimit, Token, TokenFile, TokenFileError};

/// The path the server answers on.
pub const MCP_PATH: &str = "/mcp";

/// Where the server listens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Listen {
    /// `127.0.0.1` and this port; `0` lets the system choose one.
    Loopback(u16),
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
}

impl HttpConfig {
    pub fn loopback(caps: CapSet) -> Self {
        HttpConfig {
            listen: Listen::Loopback(0),
            caps,
            token_file: None,
            calls_per_second: 20,
            max_connections: 32,
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
    #[error(transparent)]
    Token(#[from] TokenFileError),
    #[error("cannot start the MCP server thread: {0}")]
    Thread(std::io::Error),
}

/// A running server. It stops, and its token file and socket are removed,
/// when this is dropped.
pub struct HttpServer {
    url: String,
    addr: Option<SocketAddr>,
    token: Token,
    token_file: Option<TokenFile>,
    socket: Option<PathBuf>,
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
    rate: Mutex<RateLimit>,
    service: StreamableHttpService<RmcpServer<S>, NeverSessionManager>,
}

impl HttpServer {
    /// Starts listening. The socket is bound before this returns, so the
    /// address in the token file is one a client can connect to at once.
    pub fn start<S: McpSurface>(surface: Arc<S>, config: HttpConfig) -> Result<Self, HttpError> {
        let token = Token::generate()?;
        let (listener, url, addr, socket) = match &config.listen {
            Listen::Loopback(port) => {
                let bind = |source| HttpError::Bind {
                    what: format!("127.0.0.1:{port}"),
                    source,
                };
                let l = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, *port)).map_err(bind)?;
                l.set_nonblocking(true).map_err(bind)?;
                let addr = l.local_addr().map_err(bind)?;
                let url = format!("http://{addr}{MCP_PATH}");
                (Listener::Tcp(l), url, Some(addr), None)
            }
            #[cfg(unix)]
            Listen::Unix(path) => {
                use std::os::unix::fs::PermissionsExt as _;
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
        let token_file = match &config.token_file {
            Some(path) => Some(TokenFile::write(path, &url, &token, config.caps)?),
            None => None,
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(HttpError::Thread)?;
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
                            rate: Mutex::new(RateLimit::per_second(config.calls_per_second)),
                            service,
                        });
                        accept(listener, state, config.max_connections, shutdown).await;
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
    response
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
    if !text(header::HOST).is_some_and(|h| crate::loopback_host(h, state.port)) {
        return refuse(
            StatusCode::FORBIDDEN,
            "forbidden_host",
            "this server answers only requests addressed to this machine (Host: 127.0.0.1)",
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
    state.service.handle(request).await
}
