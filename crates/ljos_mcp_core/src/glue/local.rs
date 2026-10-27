//! A client in the same process as the surface (ARCHITECTURE.md §19.2 and
//! §19.9): tests, the BDD harness and hosts that drive a surface
//! themselves talk the real protocol over a pipe, with no socket and no
//! second thread.
//!
//! A surface whose answers are worked out on the caller's own thread (the
//! live inspector asks the UI thread, and here the caller *is* the UI
//! thread) needs that thread to do the work while the call waits. So a
//! call takes a `pump`: the surface's wake hook is a [`LocalWake`], and
//! whenever it fires the client runs `pump` between two steps of the
//! protocol. Nothing spins: the client sleeps on the pipe and the wake.

use std::sync::Arc;

use rmcp::{
    RoleClient, ServiceExt as _,
    model::{
        CallToolRequestParams, Implementation, ReadResourceRequestParams, RequestMetaObject,
        ResourceContents,
    },
    service::{RunningService, ServiceError},
};
use serde_json::Value;
use tokio::sync::Notify;

use super::RmcpServer;
use crate::{CapSet, ErrorCode, McpSurface, SurfaceError, ToolImage, base64_decode};

/// The wake of a surface that is served to a [`LocalClient`].
#[derive(Clone, Debug, Default)]
pub struct LocalWake {
    notify: Arc<Notify>,
}

impl LocalWake {
    pub fn new() -> Self {
        Self::default()
    }

    /// What the surface calls when it has work for the pumping thread.
    pub fn hook(&self) -> Arc<dyn Fn() + Send + Sync> {
        let notify = self.notify.clone();
        // `notify_one` keeps one wake for a client that is not waiting yet.
        Arc::new(move || notify.notify_one())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LocalError {
    /// The server refused the request with a protocol error.
    #[error("{}: {}", .0.code.as_str(), .0.message)]
    Refused(SurfaceError),
    #[error("the in-process MCP connection failed: {0}")]
    Transport(String),
}

impl LocalError {
    fn of(e: ServiceError) -> Self {
        match e {
            ServiceError::McpError(e) => {
                // What `glue::protocol_error` writes: the refusal in
                // `data`. The number is all there is when the protocol
                // layer itself refused (a malformed request).
                let said = e
                    .data
                    .as_ref()
                    .and_then(|d| d.get("code")?.as_str())
                    .and_then(ErrorCode::of);
                let code = match (said, e.code.0) {
                    (Some(code), _) => code,
                    (None, -32002) => ErrorCode::NotFound,
                    (None, -32602) => ErrorCode::InvalidArgument,
                    (None, _) => ErrorCode::Internal,
                };
                let mut refusal = SurfaceError::new(code, e.message.to_string());
                let details = match e.data {
                    Some(mut data) if said.is_some() => data.get_mut("details").map(Value::take),
                    other => other,
                };
                if let Some(details) = details {
                    refusal = refusal.with_details(details);
                }
                LocalError::Refused(refusal)
            }
            other => LocalError::Transport(other.to_string()),
        }
    }
}

/// A tool as the client was offered it.
#[derive(Clone, Debug, PartialEq)]
pub struct LocalTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    /// The `readOnlyHint` annotation; `None` when the server gave none.
    pub read_only: Option<bool>,
}

/// What a tool answered.
#[derive(Clone, Debug, PartialEq)]
pub struct LocalReply {
    /// Whether the tool refused (`isError`).
    pub is_error: bool,
    /// `structuredContent`; `Null` when there was none.
    pub structured: Value,
    /// The text parts, joined: what a model reads.
    pub text: String,
    pub images: Vec<ToolImage>,
}

impl LocalReply {
    /// The short code of a refusal: `stale_ref`, `not_found`.
    pub fn error_code(&self) -> Option<&str> {
        self.is_error
            .then(|| self.structured.pointer("/error/code")?.as_str())
            .flatten()
    }

    pub fn error_message(&self) -> Option<&str> {
        self.is_error
            .then(|| self.structured.pointer("/error/message")?.as_str())
            .flatten()
    }
}

/// An MCP client connected to a surface in this process.
pub struct LocalClient {
    // Dropped first: closing the pipe ends the server's task.
    client: RunningService<RoleClient, ()>,
    wake: LocalWake,
    runtime: tokio::runtime::Runtime,
}

impl std::fmt::Debug for LocalClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalClient").finish_non_exhaustive()
    }
}

impl LocalClient {
    /// Serves `surface` with `caps` over a pipe and connects to it. `wake`
    /// is the wake the surface was given (see the module docs); a surface
    /// that answers at once needs none and any `LocalWake` will do.
    pub fn connect<S: McpSurface>(
        surface: Arc<S>,
        caps: CapSet,
        wake: &LocalWake,
    ) -> Result<Self, LocalError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .map_err(|e| LocalError::Transport(e.to_string()))?;
        let client = runtime.block_on(async {
            let (client_io, server_io) = tokio::io::duplex(256 * 1024);
            tokio::spawn(async move {
                let _ = RmcpServer::new(surface, caps).serve(server_io).await;
            });
            ().serve(client_io)
                .await
                .map_err(|e| LocalError::Transport(e.to_string()))
        })?;
        Ok(LocalClient {
            client,
            wake: wake.clone(),
            runtime,
        })
    }

    /// The instructions the server gave when the client connected.
    pub fn instructions(&self) -> Option<String> {
        self.client.peer_info().and_then(|i| i.instructions.clone())
    }

    /// The tools the client is offered, in the server's order.
    pub fn tools(&mut self) -> Result<Vec<LocalTool>, LocalError> {
        let tools = self
            .runtime
            .block_on(self.client.list_all_tools())
            .map_err(LocalError::of)?;
        Ok(tools
            .into_iter()
            .map(|t| LocalTool {
                name: t.name.to_string(),
                description: t.description.as_deref().unwrap_or("").to_string(),
                input_schema: Value::Object((*t.input_schema).clone()),
                read_only: t.annotations.as_ref().and_then(|a| a.read_only_hint),
            })
            .collect())
    }

    /// The URIs of the resources the client is offered.
    pub fn resources(&mut self) -> Result<Vec<String>, LocalError> {
        let list = self
            .runtime
            .block_on(self.client.list_all_resources())
            .map_err(LocalError::of)?;
        Ok(list.into_iter().map(|r| r.uri.clone()).collect())
    }

    /// Calls a tool. `pump` is run each time the surface's wake fires.
    pub fn call(
        &mut self,
        name: &str,
        args: Value,
        pump: &mut dyn FnMut(),
    ) -> Result<LocalReply, LocalError> {
        self.call_as(None, name, args, pump)
    }

    /// As [`LocalClient::call`], with the client naming itself `client` in
    /// the request's `_meta`, as a client without a session does.
    pub fn call_as(
        &mut self,
        client: Option<&str>,
        name: &str,
        args: Value,
        pump: &mut dyn FnMut(),
    ) -> Result<LocalReply, LocalError> {
        let mut params = CallToolRequestParams::new(name.to_string());
        if let Some(client) = client {
            let mut meta = RequestMetaObject::new();
            meta.set_client_info(Implementation::new(client, env!("CARGO_PKG_VERSION")));
            params.meta = Some(meta);
        }
        match args {
            Value::Null => {}
            Value::Object(map) => params = params.with_arguments(map),
            other => {
                return Err(LocalError::Refused(SurfaceError::invalid_argument(
                    format!("tool arguments are a JSON object, not `{other}`"),
                )));
            }
        }
        let result = pumped(
            &self.runtime,
            &self.wake.notify,
            self.client.call_tool(params),
            pump,
        );
        let result = result.map_err(LocalError::of)?;
        let mut text = String::new();
        let mut images = Vec::new();
        for block in &result.content {
            if let Some(t) = block.as_text() {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&t.text);
            } else if let Some(i) = block.as_image() {
                let mime = match i.mime_type.as_str() {
                    "image/png" => "image/png",
                    "image/jpeg" => "image/jpeg",
                    _ => "application/octet-stream",
                };
                images.push(ToolImage {
                    mime,
                    data: base64_decode(&i.data).unwrap_or_default(),
                });
            }
        }
        Ok(LocalReply {
            is_error: result.is_error == Some(true),
            structured: result.structured_content.unwrap_or(Value::Null),
            text,
            images,
        })
    }

    /// Reads a resource: its text parts, joined.
    pub fn read_resource(
        &mut self,
        uri: &str,
        pump: &mut dyn FnMut(),
    ) -> Result<String, LocalError> {
        let result = pumped(
            &self.runtime,
            &self.wake.notify,
            self.client
                .read_resource(ReadResourceRequestParams::new(uri)),
            pump,
        );
        let result = result.map_err(LocalError::of)?;
        let mut text = String::new();
        for part in &result.contents {
            if let ResourceContents::TextResourceContents { text: t, .. } = part {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(t);
            }
        }
        Ok(text)
    }
}

/// Runs `work` to its end, running `pump` each time the surface's wake
/// fires.
///
/// Assumes rmcp's `call_tool` and `read_resource` futures are lazy: they
/// are built before `block_on`, outside the runtime's context, so one that
/// spawned or sent its request as it was built would find no runtime to
/// do it on.
fn pumped<F: std::future::Future>(
    runtime: &tokio::runtime::Runtime,
    wake: &Notify,
    work: F,
    pump: &mut dyn FnMut(),
) -> F::Output {
    runtime.block_on(async {
        tokio::pin!(work);
        loop {
            tokio::select! {
                biased;
                out = &mut work => break out,
                () = wake.notified() => {
                    pump();
                    // A pump that asks to be run again at once makes
                    // this loop always ready. Without a turn of the
                    // runtime in between, no timer would ever fire and
                    // a surface that waits with a timeout would wait
                    // for ever.
                    tokio::task::yield_now().await;
                }
            }
        }
    })
}
