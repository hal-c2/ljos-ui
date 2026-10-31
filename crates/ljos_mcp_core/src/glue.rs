//! The only module that names `rmcp`. Written against rmcp 3.5.1.

use std::sync::Arc;

use rmcp::{
    ErrorData, RoleServer, ServerHandler, ServiceExt,
    model::{
        self, CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock,
        GetPromptRequestParams, GetPromptResponse, GetPromptResult, ListPromptsResult,
        ListResourceTemplatesResult, ListResourcesResult, ListToolsResult, PaginatedRequestParams,
        Prompt, PromptArgument, PromptMessage, ReadResourceRequestParams, ReadResourceResponse,
        ReadResourceResult, ResourceContents, Role, ServerCapabilities, ServerConfig,
    },
    service::{MaybeSendFuture, RequestContext},
    transport::IntoTransport,
};
use serde_json::Value;

use crate::{
    CallCtx, CapSet, ConnectionId, ErrorCode, McpSurface, SurfaceError, ToolDef, ToolReply,
    base64_encode,
};

#[cfg(feature = "http")]
pub mod http;
#[cfg(feature = "client")]
pub mod local;

/// Serves a surface to one client with a fixed capability grant.
pub struct RmcpServer<S: McpSurface> {
    surface: Arc<S>,
    caps: CapSet,
    /// Fixed when the server is made, so every clone serves the same
    /// connection and every `new` is a different one.
    connection: ConnectionId,
}

impl<S: McpSurface> std::fmt::Debug for RmcpServer<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RmcpServer")
            .field("caps", &self.caps)
            .finish()
    }
}

impl<S: McpSurface> Clone for RmcpServer<S> {
    fn clone(&self) -> Self {
        Self {
            surface: self.surface.clone(),
            caps: self.caps,
            connection: self.connection,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error("MCP handshake failed: {0}")]
    Init(String),
    #[error("MCP session ended with an error: {0}")]
    Join(String),
}

impl<S: McpSurface> RmcpServer<S> {
    pub fn new(surface: Arc<S>, caps: CapSet) -> Self {
        Self {
            surface,
            caps,
            connection: ConnectionId::next(),
        }
    }

    /// A server for calls that come in on `connection`.
    #[cfg(feature = "http")]
    fn on_connection(surface: Arc<S>, caps: CapSet, connection: ConnectionId) -> Self {
        Self {
            surface,
            caps,
            connection,
        }
    }

    fn ctx(&self) -> CallCtx {
        CallCtx {
            caps: self.caps,
            client: None,
            connection: self.connection,
        }
    }

    /// Serves over any rmcp transport (stdio, in-process duplex, worker).
    pub async fn serve<T, E, A>(self, transport: T) -> Result<(), ServeError>
    where
        T: IntoTransport<RoleServer, E, A>,
        E: std::error::Error + Send + Sync + 'static,
    {
        let running = ServiceExt::serve(self, transport)
            .await
            .map_err(|e| ServeError::Init(e.to_string()))?;
        running
            .waiting()
            .await
            .map(|_| ())
            .map_err(|e| ServeError::Join(e.to_string()))
    }

    /// Serves on this process's stdin/stdout until the client disconnects.
    pub async fn serve_stdio(self) -> Result<(), ServeError> {
        self.serve(rmcp::transport::stdio()).await
    }
}

fn tool_to_model(def: &ToolDef) -> model::Tool {
    let mut ann = model::ToolAnnotations::new()
        .read_only(def.annotations.read_only)
        .destructive(def.annotations.destructive)
        .idempotent(def.annotations.idempotent)
        .open_world(def.annotations.open_world);
    ann.title = None;
    let mut tool =
        model::Tool::new(def.name, def.description, def.input_schema.clone()).with_annotations(ann);
    if let Some(out) = &def.output_schema {
        tool = tool.with_raw_output_schema(out.clone());
    }
    tool
}

/// A tool's answer as MCP carries it: the text the model reads first, then
/// the pictures, and the structured value beside them. A refusal is a tool
/// result with `isError`, not a protocol error, so the model can read it and
/// correct itself.
fn tool_result(out: Result<ToolReply, SurfaceError>) -> CallToolResult {
    match out {
        Ok(reply) => {
            if reply.text.is_none() && reply.images.is_empty() {
                // Structured content plus a text mirror, for clients that only read text.
                return CallToolResult::structured(reply.structured);
            }
            let text = reply.text.unwrap_or_else(|| reply.structured.to_string());
            let mut content = vec![ContentBlock::text(text)];
            for image in &reply.images {
                content.push(ContentBlock::image(base64_encode(&image.data), image.mime));
            }
            let mut result = CallToolResult::success(content);
            result.structured_content = Some(reply.structured);
            result
        }
        Err(e) => CallToolResult::structured_error(e.to_value()),
    }
}

/// A refusal where the protocol has no tool result to put it in (reading
/// a resource, getting a prompt). JSON-RPC has three numbers for our fifteen
/// codes, so `data` carries the refusal as a tool would give it: `code`,
/// `message` and `details`. A client that knows the codes reads `busy` or
/// `timeout` there instead of "internal error".
fn protocol_error(e: &SurfaceError) -> ErrorData {
    let data = e.to_value().get_mut("error").map(Value::take);
    match e.code {
        ErrorCode::NotFound => ErrorData::resource_not_found(e.message.clone(), data),
        ErrorCode::InvalidArgument | ErrorCode::PathDenied => {
            ErrorData::invalid_params(e.message.clone(), data)
        }
        _ => ErrorData::internal_error(e.message.clone(), data),
    }
}

impl<S: McpSurface> ServerHandler for RmcpServer<S> {
    fn get_info(&self) -> ServerConfig {
        let meta = self.surface.meta();
        let mut caps = ServerCapabilities::builder().enable_tools().build();
        if !self.surface.resource_templates().is_empty()
            || !self.surface.resources(&self.ctx()).is_empty()
        {
            caps.resources = Some(Default::default());
        }
        if !self.surface.prompts().is_empty() {
            caps.prompts = Some(Default::default());
        }
        ServerConfig::new(caps)
            .with_server_info(model::Implementation::new(meta.name, meta.version))
            .with_instructions(meta.instructions)
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + MaybeSendFuture + '_ {
        let tools = self
            .surface
            .tools()
            .visible(self.caps)
            .map(tool_to_model)
            .collect();
        std::future::ready(Ok(ListToolsResult::with_all_items(tools)))
    }

    fn get_tool(&self, name: &str) -> Option<model::Tool> {
        self.surface
            .tools()
            .visible(self.caps)
            .find(|d| d.name == name)
            .map(tool_to_model)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        // A client may name itself in each request, as one without a
        // session must; else it did when it connected.
        let client = context
            .meta
            .client_info()
            .or_else(|| request.meta.as_ref().and_then(|m| m.client_info()))
            .map(|info| info.name)
            .or_else(|| context.peer.peer_info().map(|p| p.client_info.name.clone()));
        let args = request.arguments.map_or(Value::Null, Value::Object);
        let ctx = CallCtx {
            client,
            ..self.ctx()
        };
        // A client that went away, or cancelled, leaves no call behind: a
        // watch it was waiting on is dropped unanswered, which is what
        // lets go of what that client held at once.
        let out = tokio::select! {
            out = self.surface.call_tool(&request.name, &ctx, args) => out,
            () = context.ct.cancelled() => {
                return Err(ErrorData::internal_error("the request was cancelled", None));
            }
        };
        Ok(tool_result(out).into())
    }

    fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListResourcesResult, ErrorData>> + MaybeSendFuture + '_ {
        let items = self
            .surface
            .resources(&self.ctx())
            .into_iter()
            .map(|r| {
                model::Resource::new(r.uri, r.name)
                    .with_description(r.description)
                    .with_mime_type(r.mime_type)
            })
            .collect();
        std::future::ready(Ok(ListResourcesResult::with_all_items(items)))
    }

    fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListResourceTemplatesResult, ErrorData>> + MaybeSendFuture + '_
    {
        let items = self
            .surface
            .resource_templates()
            .into_iter()
            .map(|t| {
                model::ResourceTemplate::new(t.uri_template, t.name)
                    .with_description(t.description)
                    .with_mime_type(t.mime_type)
            })
            .collect();
        std::future::ready(Ok(ListResourceTemplatesResult::with_all_items(items)))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        let ctx = self.ctx();
        self.surface
            .read_resource_async(&request.uri, &ctx)
            .await
            .map(|contents| {
                ReadResourceResult::new(
                    contents
                        .into_iter()
                        .map(|c| ResourceContents::text(c.text, c.uri).with_mime_type(c.mime_type))
                        .collect(),
                )
                .into()
            })
            .map_err(|e| protocol_error(&e))
    }

    fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListPromptsResult, ErrorData>> + MaybeSendFuture + '_ {
        let items = self
            .surface
            .prompts()
            .into_iter()
            .map(|p| {
                let args = p
                    .arguments
                    .iter()
                    .map(|a| {
                        PromptArgument::new(a.name)
                            .with_description(a.description)
                            .with_required(a.required)
                    })
                    .collect();
                Prompt::new(p.name, Some(p.description), Some(args))
            })
            .collect();
        std::future::ready(Ok(ListPromptsResult::with_all_items(items)))
    }

    fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<GetPromptResponse, ErrorData>> + MaybeSendFuture + '_ {
        let args = request.arguments.unwrap_or_default();
        let out = self
            .surface
            .get_prompt(&request.name, &args)
            .map(|p| {
                GetPromptResult::new(vec![PromptMessage::new_text(Role::User, p.text)])
                    .with_description(p.description)
                    .into()
            })
            .map_err(|e| protocol_error(&e));
        std::future::ready(out)
    }
}
