//! The only module that names `rmcp`. Written against rmcp 3.5.1.

use std::sync::Arc;

use rmcp::{
    ErrorData, RoleServer, ServerHandler, ServiceExt,
    model::{
        self, CallToolRequestParams, CallToolResponse, CallToolResult, GetPromptRequestParams,
        GetPromptResponse, GetPromptResult, ListPromptsResult, ListResourceTemplatesResult,
        ListResourcesResult, ListToolsResult, PaginatedRequestParams, Prompt, PromptArgument,
        PromptMessage, ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult,
        ResourceContents, Role, ServerCapabilities, ServerConfig,
    },
    service::{MaybeSendFuture, RequestContext},
    transport::IntoTransport,
};
use serde_json::Value;

use crate::{CallCtx, CapSet, ErrorCode, McpSurface, SurfaceError, ToolDef};

/// Serves a surface to one client with a fixed capability grant.
pub struct RmcpServer<S: McpSurface> {
    surface: Arc<S>,
    caps: CapSet,
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
        Self { surface, caps }
    }

    fn ctx(&self) -> CallCtx {
        CallCtx { caps: self.caps }
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

fn protocol_error(e: &SurfaceError) -> ErrorData {
    match e.code {
        ErrorCode::NotFound => ErrorData::resource_not_found(e.message.clone(), e.details.clone()),
        ErrorCode::InvalidArgument | ErrorCode::PathDenied => {
            ErrorData::invalid_params(e.message.clone(), e.details.clone())
        }
        _ => ErrorData::internal_error(e.message.clone(), e.details.clone()),
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

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResponse, ErrorData>> + MaybeSendFuture + '_ {
        let args = request.arguments.map_or(Value::Null, Value::Object);
        let (out, _) = self.surface.tools().call(&request.name, &self.ctx(), args);
        let result = match out {
            // Structured content plus a text mirror, for clients that only read text.
            Ok(v) => CallToolResult::structured(v),
            Err(e) => CallToolResult::structured_error(e.to_value()),
        };
        std::future::ready(Ok(result.into()))
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

    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ReadResourceResponse, ErrorData>> + MaybeSendFuture + '_ {
        let out = self
            .surface
            .read_resource(&request.uri, &self.ctx())
            .map(|contents| {
                ReadResourceResult::new(
                    contents
                        .into_iter()
                        .map(|c| ResourceContents::text(c.text, c.uri).with_mime_type(c.mime_type))
                        .collect(),
                )
                .into()
            })
            .map_err(|e| protocol_error(&e));
        std::future::ready(out)
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
