//! An MCP client of an inspector in another process: `initialize` once,
//! then tools called and listed, each call one `POST`.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::{Value as Json, json};

use super::token::{Target, TokenError, Trust, read_token_file, token_file_of};
use super::wire::{self, Answer};
use crate::ErrorCode;

/// How long a call may take beyond the wait it asks for.
pub const PATIENCE: Duration = Duration::from_secs(30);

/// The longest wait a call may ask for: `app_watch`'s own limit.
const LONGEST_WAIT: Duration = Duration::from_secs(3600);

/// The protocol revision this client speaks; the inspector answers with
/// the one it will.
const PROTOCOL: &str = "2025-06-18";

/// A picture a tool answered with, as it came: its MIME type and its
/// bytes, decoded from the base64 the protocol carries them in.
#[derive(Clone, PartialEq, Eq)]
pub struct Image {
    pub mime: String,
    pub data: Vec<u8>,
}

impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The bytes of a screenshot would drown any log.
        f.debug_struct("Image")
            .field("mime", &self.mime)
            .field("bytes", &self.data.len())
            .finish()
    }
}

/// A tool's whole answer: what a model reads, the pictures, and the
/// structured value beside them, as the inspector gave them.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolResult {
    /// `structuredContent`; `Null` when the tool gave none.
    pub structured: Json,
    /// The text blocks, one line apart; `None` when there were none.
    pub text: Option<String>,
    pub images: Vec<Image>,
}

impl ToolResult {
    fn of(result: &Json) -> Result<ToolResult, ToolError> {
        let mut texts = Vec::new();
        let mut images = Vec::new();
        for block in result["content"].as_array().into_iter().flatten() {
            match block["type"].as_str() {
                Some("text") => texts.push(block["text"].as_str().unwrap_or("")),
                Some("image") => {
                    let mime = block["mimeType"].as_str().unwrap_or("").to_owned();
                    let data = block["data"]
                        .as_str()
                        .and_then(crate::base64_decode)
                        .ok_or_else(|| {
                            ToolError::BadAnswer(format!("an image ({mime}) that is not base64"))
                        })?;
                    images.push(Image { mime, data });
                }
                // Audio and resources are kept in the result, not read here.
                _ => {}
            }
        }
        Ok(ToolResult {
            structured: result
                .get("structuredContent")
                .cloned()
                .unwrap_or(Json::Null),
            text: (!texts.is_empty()).then(|| texts.join("\n")),
            images,
        })
    }
}

/// Why a call gave no result.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// The token file was not read or not believed.
    #[error(transparent)]
    Token(#[from] TokenError),
    /// The inspector could not be asked at all.
    #[error(transparent)]
    Unreached(#[from] wire::Unreached),
    /// Turned away for asking too often, before anything ran.
    #[error("rate_limited: {body}")]
    RateLimited {
        retry_after: Option<Duration>,
        body: String,
    },
    /// Answered with an HTTP status that is not success.
    #[error("the inspector answered {status}: {body}")]
    Status { status: u16, body: String },
    /// An answer that is not what the protocol says one is.
    #[error("the inspector's answer is not a JSON-RPC answer: {0}")]
    BadAnswer(String),
    /// Refused by the protocol rather than by a tool: an unknown tool or
    /// method, arguments that do not fit.
    #[error("{message} ({code})")]
    Protocol {
        code: i64,
        message: String,
        data: Option<Json>,
    },
    /// The tool ran and refused (`isError`), with its code and details.
    #[error("{}: {message}", code.as_deref().unwrap_or("error"))]
    Tool {
        /// The word the tool gave (`stale_ref`, `capability_denied`, …).
        code: Option<String>,
        message: String,
        details: Option<Json>,
        /// Everything the refusal carried; boxed, as it is seldom read.
        result: Box<ToolResult>,
    },
}

impl ToolError {
    /// The refusal's code as this crate knows it; the word itself is in
    /// [`ToolError::Tool`] for codes this crate does not know.
    pub fn code(&self) -> Option<ErrorCode> {
        match self {
            ToolError::Tool { code, .. } => code.as_deref().and_then(ErrorCode::of),
            ToolError::Protocol { data, .. } => data
                .as_ref()
                .and_then(|d| d["code"].as_str())
                .and_then(ErrorCode::of),
            ToolError::RateLimited { .. } => Some(ErrorCode::Busy),
            _ => None,
        }
    }

    /// The refusal's details, when it gave any.
    pub fn details(&self) -> Option<&Json> {
        match self {
            ToolError::Tool { details, .. } => details.as_ref(),
            ToolError::Protocol { data, .. } => data.as_ref().and_then(|d| d.get("details")),
            _ => None,
        }
    }

    fn of_refusal(result: &Json) -> ToolError {
        let parsed = ToolResult::of(result);
        let error = &result["structuredContent"]["error"];
        let message = error["message"].as_str().map(str::to_owned);
        match parsed {
            Ok(parsed) => ToolError::Tool {
                code: error["code"].as_str().map(str::to_owned),
                message: message
                    .or_else(|| parsed.text.clone())
                    .unwrap_or_else(|| "refused".to_owned()),
                details: error.get("details").cloned(),
                result: Box::new(parsed),
            },
            Err(bad) => bad,
        }
    }
}

/// Where the inspector is named.
#[derive(Debug, Clone)]
enum Source {
    /// Given once.
    Target(Target),
    /// Read again for every call, so an application started again, with
    /// a new port and token, is reached again.
    File { path: PathBuf, trust: Trust },
}

/// A client of one inspector. Every call is its own `POST` with no
/// connection kept, so a client nobody calls costs nothing.
#[derive(Debug)]
pub struct Client {
    source: Source,
    name: String,
    version: String,
    next_id: AtomicU64,
    server: tokio::sync::OnceCell<Json>,
}

impl Client {
    fn of(source: Source) -> Client {
        Client {
            source,
            name: env!("CARGO_PKG_NAME").to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            next_id: AtomicU64::new(1),
            server: tokio::sync::OnceCell::new(),
        }
    }

    /// A client of the inspector `target` names.
    pub fn new(target: Target) -> Client {
        Client::of(Source::Target(target))
    }

    /// A client of the inspector `path` names, believed as far as `trust`
    /// says. The file is read now, to say at once what is wrong with it,
    /// and again for every call.
    pub fn from_token_file(path: impl Into<PathBuf>, trust: Trust) -> Result<Client, TokenError> {
        let path = path.into();
        read_token_file(&path, trust)?;
        Ok(Client::of(Source::File { path, trust }))
    }

    /// A client of the inspector process `pid` serves, by the token file
    /// it wrote in `dir` or its `target/ljos`. For one just started, wait
    /// for the file first with [`super::wait_for_token`].
    pub fn for_pid(dir: &Path, pid: u32) -> Result<Client, TokenError> {
        Client::from_token_file(token_file_of(dir, pid)?, Trust::FOUND)
    }

    /// The name and version told to the inspector, which it shows in its
    /// log beside what this client asks.
    #[must_use]
    pub fn with_client_info(mut self, name: impl Into<String>, version: impl Into<String>) -> Self {
        self.name = name.into();
        self.version = version.into();
        self
    }

    /// The inspector as its token file says now.
    pub fn target(&self) -> Result<Target, TokenError> {
        match &self.source {
            Source::Target(target) => Ok(target.clone()),
            Source::File { path, trust } => read_token_file(path, *trust),
        }
    }

    /// The MCP handshake, made once: the inspector's `initialize` result
    /// (its name, version, capabilities and instructions). The calls below
    /// make it themselves when it has not been made.
    pub async fn initialize(&self) -> Result<&Json, ToolError> {
        self.server
            .get_or_try_init(|| async {
                let params = json!({
                    "protocolVersion": PROTOCOL,
                    "capabilities": {},
                    "clientInfo": { "name": &self.name, "version": &self.version },
                });
                let server = self.request("initialize", params, PATIENCE).await?;
                self.notify("notifications/initialized").await?;
                Ok(server)
            })
            .await
    }

    /// Calls the tool `name` with `args` (a JSON object). A call that asks
    /// to wait (`wait_ms`) is given that long beyond [`PATIENCE`].
    pub async fn call_tool(&self, name: &str, args: Json) -> Result<ToolResult, ToolError> {
        self.initialize().await?;
        let wait = args["wait_ms"]
            .as_u64()
            .map_or(Duration::ZERO, Duration::from_millis)
            .min(LONGEST_WAIT);
        let params = json!({ "name": name, "arguments": args });
        let result = self.request("tools/call", params, wait + PATIENCE).await?;
        if result["isError"] == json!(true) {
            return Err(ToolError::of_refusal(&result));
        }
        ToolResult::of(&result)
    }

    /// Every tool the inspector offers this client, as `tools/list` gives
    /// each: its name, description and schemas.
    pub async fn list_tools(&self) -> Result<Vec<Json>, ToolError> {
        self.initialize().await?;
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let params = match &cursor {
                Some(cursor) => json!({ "cursor": cursor }),
                None => json!({}),
            };
            let page = self.request("tools/list", params, PATIENCE).await?;
            tools.extend(page["tools"].as_array().into_iter().flatten().cloned());
            match page["nextCursor"].as_str() {
                // A cursor given twice would page forever.
                Some(next) if cursor.as_deref() != Some(next) => cursor = Some(next.to_owned()),
                _ => return Ok(tools),
            }
        }
    }

    /// One JSON-RPC request, answered within `limit`: its `result`.
    pub async fn request(
        &self,
        method: &str,
        params: Json,
        limit: Duration,
    ) -> Result<Json, ToolError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let body = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let answer = wire::post(&self.target()?, body.to_string(), limit).await?;
        let mut reply = read_reply(&answer, id)?;
        if let Some(error) = reply.get_mut("error") {
            return Err(ToolError::Protocol {
                code: error["code"].as_i64().unwrap_or(0),
                message: error["message"].as_str().unwrap_or("refused").to_owned(),
                data: error.get_mut("data").map(Json::take),
            });
        }
        match reply.get_mut("result") {
            Some(result) => Ok(result.take()),
            None => Err(ToolError::BadAnswer(format!(
                "the answer to {method} has neither a result nor an error"
            ))),
        }
    }

    async fn notify(&self, method: &str) -> Result<(), ToolError> {
        let body = json!({ "jsonrpc": "2.0", "method": method });
        let answer = wire::post(&self.target()?, body.to_string(), PATIENCE).await?;
        status(&answer)
    }

    /// This client for synchronous code.
    pub fn blocking(self) -> BlockingClient {
        BlockingClient(Arc::new(self))
    }
}

fn status(answer: &Answer) -> Result<(), ToolError> {
    let body = || String::from_utf8_lossy(&answer.body).trim().to_owned();
    match answer.status {
        200..=299 => Ok(()),
        429 => Err(ToolError::RateLimited {
            retry_after: answer.retry_after,
            body: body(),
        }),
        status => Err(ToolError::Status {
            status,
            body: body(),
        }),
    }
}

/// The message of `answer` that answers request `id`.
fn read_reply(answer: &Answer, id: u64) -> Result<Json, ToolError> {
    status(answer)?;
    for message in answer.messages() {
        let message: Json = serde_json::from_str(message)
            .map_err(|e| ToolError::BadAnswer(format!("not JSON: {e}")))?;
        if message["id"] == json!(id) {
            return Ok(message);
        }
    }
    Err(ToolError::BadAnswer(format!("no answer to request {id}")))
}

/// A [`Client`] for synchronous code. Each call runs on the process's one
/// client thread ([`super::spawn`]), on a runtime of its own, and the
/// caller waits for its answer: so it may be called from a thread of any
/// runtime, which is neither entered nor blocked on, and from code with
/// none. A call waits no longer than the client's own limits.
#[derive(Debug, Clone)]
pub struct BlockingClient(Arc<Client>);

impl BlockingClient {
    pub fn new(client: Client) -> BlockingClient {
        client.blocking()
    }

    /// The async client this one runs.
    pub fn client(&self) -> &Arc<Client> {
        &self.0
    }

    fn run<T, F>(&self, call: impl FnOnce(Arc<Client>) -> F) -> Result<T, ToolError>
    where
        T: Send + 'static,
        F: Future<Output = Result<T, ToolError>> + Send + 'static,
    {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let job = call(Arc::clone(&self.0));
        wire::spawn(async move {
            let _ = tx.send(job.await);
        })?;
        rx.recv().unwrap_or_else(|_| {
            Err(ToolError::Unreached(wire::Unreached(
                "the client thread has ended".to_owned(),
            )))
        })
    }

    /// [`Client::initialize`].
    pub fn initialize(&self) -> Result<Json, ToolError> {
        self.run(|c| async move { c.initialize().await.cloned() })
    }

    /// [`Client::call_tool`].
    pub fn call_tool(&self, name: &str, args: Json) -> Result<ToolResult, ToolError> {
        let name = name.to_owned();
        self.run(|c| async move { c.call_tool(&name, args).await })
    }

    /// [`Client::list_tools`].
    pub fn list_tools(&self) -> Result<Vec<Json>, ToolError> {
        self.run(|c| async move { c.list_tools().await })
    }

    /// [`Client::request`].
    pub fn request(&self, method: &str, params: Json, limit: Duration) -> Result<Json, ToolError> {
        let method = method.to_owned();
        self.run(move |c| async move { c.request(&method, params, limit).await })
    }
}
