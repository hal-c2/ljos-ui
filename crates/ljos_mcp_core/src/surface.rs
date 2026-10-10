use crate::{CallCtx, SurfaceError, ToolRegistry};

#[derive(Clone, Debug)]
pub struct ServerMeta {
    pub name: &'static str,
    pub version: &'static str,
    /// Shown to the model at connection time.
    pub instructions: String,
}

#[derive(Clone, Debug)]
pub struct ResourceDef {
    pub uri: String,
    pub name: String,
    pub description: String,
    pub mime_type: &'static str,
}

#[derive(Clone, Debug)]
pub struct ResourceTemplateDef {
    pub uri_template: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub mime_type: &'static str,
}

#[derive(Clone, Debug)]
pub struct ResourceContent {
    pub uri: String,
    pub mime_type: &'static str,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct PromptArg {
    pub name: &'static str,
    pub description: &'static str,
    pub required: bool,
}

#[derive(Clone, Debug)]
pub struct PromptDef {
    pub name: &'static str,
    pub description: &'static str,
    pub arguments: Vec<PromptArg>,
}

#[derive(Clone, Debug)]
pub struct PromptText {
    pub description: String,
    /// One user-role message.
    pub text: String,
}

/// Everything an MCP server offers, independent of the SDK that carries it.
///
/// Handlers are synchronous: M0 tools are quick file and table lookups. Long
/// operations follow the `run_id` pattern (§19.1), not blocking calls.
pub trait McpSurface: Send + Sync + 'static {
    fn meta(&self) -> ServerMeta;
    fn tools(&self) -> &ToolRegistry;

    fn resources(&self, _ctx: &CallCtx) -> Vec<ResourceDef> {
        Vec::new()
    }
    fn resource_templates(&self) -> Vec<ResourceTemplateDef> {
        Vec::new()
    }
    fn read_resource(
        &self,
        uri: &str,
        _ctx: &CallCtx,
    ) -> Result<Vec<ResourceContent>, SurfaceError> {
        Err(SurfaceError::not_found(format!("no resource `{uri}`")))
    }

    fn prompts(&self) -> Vec<PromptDef> {
        Vec::new()
    }
    fn get_prompt(
        &self,
        name: &str,
        _args: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<PromptText, SurfaceError> {
        Err(SurfaceError::not_found(format!("no prompt `{name}`")))
    }
}
