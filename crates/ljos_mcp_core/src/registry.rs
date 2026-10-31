use std::{collections::HashSet, sync::Arc};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};

use crate::{
    CapSet, DEFAULT_RESULT_BYTES, ErrorCode, HARD_RESULT_BYTES, SurfaceError, caps::Cap,
    size::cap_result,
};

/// Descriptions stay short so a tool list costs few tokens (§19.6).
const MAX_DESCRIPTION_CHARS: usize = 300;

/// Hints only (§19.7): the server enforces [`Cap`]s, clients may not trust these.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ToolAnnotations {
    pub read_only: bool,
    pub destructive: bool,
    pub idempotent: bool,
    pub open_world: bool,
}

impl ToolAnnotations {
    /// Pure analysis: reads, never writes, repeatable, closed world.
    pub const READ_ONLY: Self = Self {
        read_only: true,
        destructive: false,
        idempotent: true,
        open_world: false,
    };
    /// Writes, but additively and repeatably.
    pub const WRITE: Self = Self {
        read_only: false,
        destructive: false,
        idempotent: true,
        open_world: false,
    };
}

/// Empty argument object for tools that take none.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoArgs {}

#[derive(Clone, Debug)]
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Arc<Map<String, Value>>,
    pub output_schema: Option<Arc<Map<String, Value>>>,
    /// `None` means any client may call it.
    pub required_cap: Option<Cap>,
    pub annotations: ToolAnnotations,
    /// Per-tool budget for the whole response; clamped to
    /// [`crate::HARD_RESULT_BYTES`]. The glue sends the structured value twice
    /// (as `structuredContent` and as a text mirror), so each copy gets half.
    pub max_result_bytes: usize,
}

fn schema_object<T: JsonSchema>() -> Arc<Map<String, Value>> {
    let Value::Object(mut map) = schemars::schema_for!(T).to_value() else {
        unreachable!("a schemars root schema is always an object")
    };
    // The meta-schema URI adds tokens and no information for clients.
    map.remove("$schema");
    Arc::new(map)
}

impl ToolDef {
    /// `I` is the argument type; its schema becomes `inputSchema`.
    ///
    /// # Panics
    /// If `description` exceeds 300 characters: that is a programming error
    /// caught by the first test run, not a runtime condition.
    pub fn new<I: JsonSchema>(name: &'static str, description: &'static str) -> Self {
        assert!(
            description.chars().count() <= MAX_DESCRIPTION_CHARS,
            "tool `{name}` description exceeds {MAX_DESCRIPTION_CHARS} characters"
        );
        Self {
            name,
            description,
            input_schema: schema_object::<I>(),
            output_schema: None,
            required_cap: None,
            annotations: ToolAnnotations::READ_ONLY,
            max_result_bytes: DEFAULT_RESULT_BYTES,
        }
    }

    #[must_use]
    pub fn output<O: JsonSchema>(mut self) -> Self {
        self.output_schema = Some(schema_object::<O>());
        self
    }

    #[must_use]
    pub fn requires(mut self, cap: Cap) -> Self {
        self.required_cap = Some(cap);
        self
    }

    #[must_use]
    pub fn annotated(mut self, annotations: ToolAnnotations) -> Self {
        self.annotations = annotations;
        self
    }

    #[must_use]
    pub fn max_result_bytes(mut self, bytes: usize) -> Self {
        self.max_result_bytes = bytes;
        self
    }

    /// The budget of one copy of the answer: the model's text and the
    /// structured value each get half of the tool's budget.
    pub fn per_copy_bytes(&self) -> usize {
        self.max_result_bytes.min(HARD_RESULT_BYTES) / 2
    }
}

/// Per-call context: who is calling and with which rights.
#[derive(Clone, Debug)]
pub struct CallCtx {
    pub caps: CapSet,
    /// The name the client gave itself: in the request's `_meta`
    /// (`io.modelcontextprotocol/clientInfo`), else when it connected.
    pub client: Option<String>,
    /// Which connection the call came in on. Two clients may give the same
    /// name, so a surface that must tell writers apart (to keep one's edit
    /// from being taken for another's) uses this, never `client`.
    pub connection: ConnectionId,
}

impl CallCtx {
    /// A call from a client that gave no name, on a connection of its own.
    pub fn new(caps: CapSet) -> Self {
        CallCtx {
            caps,
            client: None,
            connection: ConnectionId::next(),
        }
    }
}

/// Names one connection for as long as the process runs. Ids are handed out
/// from a counter, so none repeats and none is derived from anything a client
/// said; they mean nothing across processes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConnectionId(u64);

impl ConnectionId {
    /// A fresh id, different from every one handed out before.
    pub fn next() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        ConnectionId(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }

    /// The number, for a log line or a map key a surface keeps outside Rust.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// A tool's result before capping. `Ok` values are JSON objects (anything
/// else is wrapped as `{"value": …}`).
pub type ToolOutput = Result<Value, SurfaceError>;
pub type ToolHandler = Arc<dyn Fn(&CallCtx, Value) -> ToolOutput + Send + Sync>;

/// Adapts a typed function into a [`ToolHandler`]: arguments are deserialised
/// (failures become `invalid_argument`) and the output serialised.
pub fn typed<I, O, F>(f: F) -> ToolHandler
where
    I: DeserializeOwned,
    O: Serialize,
    F: Fn(&CallCtx, I) -> Result<O, SurfaceError> + Send + Sync + 'static,
{
    Arc::new(move |ctx, args| {
        // Clients may omit `arguments` for a no-arg tool.
        let args = if args.is_null() {
            Value::Object(Map::new())
        } else {
            args
        };
        let input: I = serde_json::from_value(args)
            .map_err(|e| SurfaceError::invalid_argument(e.to_string()))?;
        let out = f(ctx, input)?;
        serde_json::to_value(out).map_err(|e| SurfaceError::new(ErrorCode::Internal, e.to_string()))
    })
}

/// The refusal for a tool the caller has no right to: it names the missing
/// capability, so the reader knows what to ask the person at the keyboard for.
pub fn denied(name: &str, cap: Cap, ctx: &CallCtx) -> SurfaceError {
    SurfaceError::new(
        ErrorCode::CapabilityDenied,
        format!("tool `{name}` needs the `{cap}` capability"),
    )
    .with_details(serde_json::json!({ "required": cap, "granted": ctx.caps }))
}

#[derive(Debug, thiserror::Error)]
pub enum ToolRegistryError {
    #[error("tool `{0}` registered twice")]
    Duplicate(&'static str),
}

struct Entry {
    def: ToolDef,
    handler: ToolHandler,
}

/// Tools in registration order. The order is the wire order, so prompt caches
/// hit across sessions; nothing here sorts or hashes.
#[derive(Default)]
pub struct ToolRegistry {
    entries: Vec<Entry>,
    names: HashSet<&'static str>,
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.entries.iter().map(|e| e.def.name))
            .finish()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &mut self,
        def: ToolDef,
        handler: ToolHandler,
    ) -> Result<(), ToolRegistryError> {
        if !self.names.insert(def.name) {
            return Err(ToolRegistryError::Duplicate(def.name));
        }
        self.entries.push(Entry { def, handler });
        Ok(())
    }

    /// Builder form of [`register`](Self::register) for static tool sets.
    ///
    /// # Panics
    /// On a duplicate name, which is a bug in the (static) registrant.
    #[must_use]
    pub fn with(mut self, def: ToolDef, handler: ToolHandler) -> Self {
        self.register(def, handler)
            .unwrap_or_else(|e| panic!("{e}"));
        self
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Every tool, in registration order.
    pub fn all(&self) -> impl Iterator<Item = &ToolDef> {
        self.entries.iter().map(|e| &e.def)
    }

    /// Tools the caller may use, in registration order.
    pub fn visible(&self, caps: CapSet) -> impl Iterator<Item = &ToolDef> {
        self.all()
            .filter(move |d| d.required_cap.is_none_or(|c| caps.contains(c)))
    }

    /// Registers a tool whose surface runs it itself (it overrides
    /// [`crate::McpSurface::call_tool`]): the registry lists it and checks
    /// the caller's capabilities, and has no handler to dispatch to.
    ///
    /// # Panics
    /// On a duplicate name, which is a bug in the (static) registrant.
    #[must_use]
    pub fn declare(self, def: ToolDef) -> Self {
        let name = def.name;
        self.with(
            def,
            Arc::new(move |_, _| {
                Err(SurfaceError::new(
                    ErrorCode::Internal,
                    format!("tool `{name}` is run by its surface, not by the registry"),
                ))
            }),
        )
    }

    /// The tool `name`, if the caller may use it: the first half of
    /// [`call`](Self::call), for surfaces that run their tools themselves.
    ///
    /// The capability check uses the full registry, so a hidden tool answers
    /// `capability_denied` rather than pretending not to exist.
    pub fn check(&self, name: &str, ctx: &CallCtx) -> Result<&ToolDef, SurfaceError> {
        let Some(entry) = self.entries.iter().find(|e| e.def.name == name) else {
            return Err(SurfaceError::not_found(format!("unknown tool `{name}`")));
        };
        let def = &entry.def;
        if let Some(cap) = def.required_cap
            && !ctx.caps.contains(cap)
        {
            return Err(denied(name, cap, ctx));
        }
        Ok(def)
    }

    /// Runs a tool: capability check, dispatch, response capping.
    ///
    /// The capability check uses the full registry, so a hidden tool answers
    /// `capability_denied` rather than pretending not to exist.
    pub fn call(&self, name: &str, ctx: &CallCtx, args: Value) -> (ToolOutput, Option<&ToolDef>) {
        let Some(entry) = self.entries.iter().find(|e| e.def.name == name) else {
            return (
                Err(SurfaceError::not_found(format!("unknown tool `{name}`"))),
                None,
            );
        };
        let def = &entry.def;
        if let Some(cap) = def.required_cap
            && !ctx.caps.contains(cap)
        {
            return (Err(denied(name, cap, ctx)), Some(def));
        }
        let out = (entry.handler)(ctx, args).map(|v| cap_result(v, def.per_copy_bytes()));
        (out, Some(def))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[derive(Deserialize, JsonSchema)]
    struct Echo {
        text: String,
    }

    fn registry() -> ToolRegistry {
        ToolRegistry::new()
            .with(
                ToolDef::new::<Echo>("echo", "Echo text back."),
                typed(|_, a: Echo| Ok(json!({ "text": a.text }))),
            )
            .with(
                ToolDef::new::<NoArgs>("write", "Needs fs.").requires(Cap::Fs),
                typed(|_, _: NoArgs| Ok(json!({}))),
            )
    }

    #[test]
    fn order_is_registration_order() {
        let r = registry();
        assert_eq!(
            r.all().map(|d| d.name).collect::<Vec<_>>(),
            ["echo", "write"]
        );
        assert_eq!(r.visible(CapSet::EMPTY).count(), 1);
        assert_eq!(r.visible(CapSet::of([Cap::Fs])).count(), 2);
    }

    #[test]
    fn duplicates_are_rejected() {
        let mut r = registry();
        let err = r.register(
            ToolDef::new::<NoArgs>("echo", "dup"),
            typed(|_, _: NoArgs| Ok(json!({}))),
        );
        assert!(matches!(err, Err(ToolRegistryError::Duplicate("echo"))));
    }

    #[test]
    fn denial_names_the_missing_capability() {
        let r = registry();
        let ctx = CallCtx::new(CapSet::EMPTY);
        let (out, _) = r.call("write", &ctx, Value::Null);
        let err = out.unwrap_err();
        assert_eq!(err.code, ErrorCode::CapabilityDenied);
        assert_eq!(err.details.unwrap()["required"], "fs");
    }

    #[test]
    fn bad_arguments_are_invalid_argument() {
        let r = registry();
        let ctx = CallCtx::new(CapSet::EMPTY);
        let (out, _) = r.call("echo", &ctx, json!({"nope": 1}));
        assert_eq!(out.unwrap_err().code, ErrorCode::InvalidArgument);
    }

    #[test]
    fn unknown_tool_is_not_found() {
        let r = registry();
        let (out, def) = r.call("x", &CallCtx::new(CapSet::EMPTY), Value::Null);
        assert_eq!(out.unwrap_err().code, ErrorCode::NotFound);
        assert!(def.is_none());
    }

    #[test]
    fn schemas_are_objects_without_meta_schema_uri() {
        let d = ToolDef::new::<NoArgs>("n", "d");
        assert_eq!(d.input_schema["type"], "object");
        assert!(!d.input_schema.contains_key("$schema"));
    }
}
