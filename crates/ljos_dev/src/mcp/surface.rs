//! The server's side of the inspector: what runs on the thread that talks
//! to clients.
//!
//! It never touches the application. A call is checked here (is there such
//! a tool, may this client use it, are the arguments right), put into the
//! bounded inbox of the UI thread, and awaited. The inbox's wake hook is
//! the only thing that makes the UI thread run for the inspector, so a
//! connected client that asks nothing costs the application nothing.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use ljos_mcp_core::{
    CallCtx, Cap, CapSet, ErrorCode, McpSurface, ResourceContent, ResourceDef, ResourceFuture,
    ServerMeta, SurfaceError, ToolFuture, ToolRegistry, ToolReply,
};
use ljos_runtime::UiHandle;
use serde_json::{Value, json};
use tokio::sync::oneshot;

use super::args::{self, INSTRUCTIONS, Kind, RESERVED, Resource, SERVER_NAME};

/// Requests that may wait for the UI thread at one time (§19.5). One more
/// is answered `busy`.
pub const INBOX_CAPACITY: usize = 16;
/// Snapshots that may be in the making at one time (§19.7).
pub const MAX_SNAPSHOT_JOBS: usize = 2;

/// What this inspector grants, whatever a client was started with.
pub(crate) fn granted() -> CapSet {
    CapSet::of([Cap::Inspect])
}

/// One of the [`MAX_SNAPSHOT_JOBS`] places, held until the snapshot is
/// answered or given up.
pub(crate) struct SlotGuard(Arc<AtomicUsize>);

impl SlotGuard {
    fn take(slots: &Arc<AtomicUsize>) -> Result<SlotGuard, SurfaceError> {
        let taken = slots.fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            (n < MAX_SNAPSHOT_JOBS).then_some(n + 1)
        });
        match taken {
            Ok(_) => Ok(SlotGuard(slots.clone())),
            Err(_) => Err(SurfaceError::new(
                ErrorCode::Busy,
                format!(
                    "{MAX_SNAPSHOT_JOBS} snapshots are already being written. Wait for one of \
                     them to be answered, then ask again"
                ),
            )),
        }
    }
}

impl Drop for SlotGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

impl fmt::Debug for SlotGuard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SlotGuard")
    }
}

pub(crate) type Answer = Result<ToolReply, SurfaceError>;

/// One request on its way to the UI thread.
#[derive(Debug)]
pub(crate) struct DevReq {
    pub(crate) kind: Kind,
    pub(crate) reply: oneshot::Sender<Answer>,
    /// The client has stopped waiting after this; answering later is work
    /// for nobody.
    pub(crate) deadline: Instant,
    pub(crate) slot: Option<SlotGuard>,
    /// The size each copy of the answer (text and JSON) may have.
    pub(crate) per_copy: usize,
}

/// The inspector as an MCP server sees it. `Send + Sync`: it holds the
/// sending end of the UI thread's inbox and nothing of the application.
pub struct InspectorSurface {
    registry: ToolRegistry,
    handle: UiHandle<DevReq>,
    snapshots: Arc<AtomicUsize>,
    timeout: Duration,
}

impl fmt::Debug for InspectorSurface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InspectorSurface")
            .field("tools", &self.registry.len())
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

fn read_only_refusal(tool: &str, needs: Cap) -> SurfaceError {
    SurfaceError::new(
        ErrorCode::CapabilityDenied,
        format!(
            "tool `{tool}` needs the `{needs}` capability, and this inspector is read-only: it \
             grants `inspect` and nothing else, whatever it was started with. It has: {}",
            args::TOOLS.join(", ")
        ),
    )
    .with_details(json!({ "required": needs.as_str(), "granted": ["inspect"] }))
}

impl InspectorSurface {
    pub(crate) fn new(handle: UiHandle<DevReq>, timeout: Duration) -> Self {
        InspectorSurface {
            registry: args::registry(),
            handle,
            snapshots: Arc::new(AtomicUsize::new(0)),
            timeout,
        }
    }

    /// What a caller may do here: `inspect` if it has it, nothing else.
    fn narrowed(ctx: &CallCtx) -> CallCtx {
        CallCtx {
            caps: if ctx.caps.contains(Cap::Inspect) {
                granted()
            } else {
                CapSet::EMPTY
            },
        }
    }

    async fn ask(&self, kind: Kind, per_copy: usize) -> Answer {
        let slot = if kind.is_snapshot() {
            Some(SlotGuard::take(&self.snapshots)?)
        } else {
            None
        };
        let (reply, answer) = oneshot::channel();
        let request = DevReq {
            kind,
            reply,
            deadline: Instant::now() + self.timeout,
            slot,
            per_copy,
        };
        if let Err(e) = self.handle.send(request) {
            return Err(if e.is_full() {
                SurfaceError::new(
                    ErrorCode::Busy,
                    format!(
                        "{INBOX_CAPACITY} requests are already waiting for the application's UI \
                         thread. Nothing was done; ask again in a moment"
                    ),
                )
            } else {
                SurfaceError::new(
                    ErrorCode::AppFaulted,
                    "the application is no longer running, so there is nothing to inspect",
                )
            });
        }
        match tokio::time::timeout(self.timeout, answer).await {
            Ok(Ok(answer)) => answer,
            // The UI thread dropped the request without answering: the
            // application went away while it was queued.
            Ok(Err(_)) => Err(SurfaceError::new(
                ErrorCode::AppFaulted,
                "the application stopped before it answered",
            )),
            Err(_) => Err(SurfaceError::new(
                ErrorCode::Timeout,
                format!(
                    "the application's UI thread did not answer within {} ms. It may be busy or \
                     blocked; `app_logs` may say why once it answers again",
                    self.timeout.as_millis()
                ),
            )),
        }
    }
}

impl McpSurface for InspectorSurface {
    fn meta(&self) -> ServerMeta {
        ServerMeta {
            name: SERVER_NAME,
            version: env!("CARGO_PKG_VERSION"),
            instructions: INSTRUCTIONS.to_string(),
        }
    }

    fn tools(&self) -> &ToolRegistry {
        &self.registry
    }

    fn call_tool<'a>(&'a self, name: &'a str, ctx: &'a CallCtx, args: Value) -> ToolFuture<'a> {
        Box::pin(async move {
            // A tool that acts is refused by name, before its arguments are
            // read: there is no request it could become (`Kind`).
            if let Some((_, needs)) = RESERVED.iter().find(|(tool, _)| *tool == name) {
                return Err(read_only_refusal(name, *needs));
            }
            if !args::TOOLS.contains(&name) {
                // Says which tools there are.
                return args::parse(name, Value::Null).map(|_| ToolReply::from(Value::Null));
            }
            let def = self.registry.check(name, &Self::narrowed(ctx))?;
            let per_copy = def.per_copy_bytes();
            let kind = args::parse(name, args)?;
            self.ask(kind, per_copy).await
        })
    }

    fn resources(&self, ctx: &CallCtx) -> Vec<ResourceDef> {
        if !ctx.caps.contains(Cap::Inspect) {
            return Vec::new();
        }
        Resource::ALL
            .into_iter()
            .map(|r| ResourceDef {
                uri: r.uri().to_string(),
                name: r.name().to_string(),
                description: r.description().to_string(),
                mime_type: r.mime(),
            })
            .collect()
    }

    fn read_resource_async<'a>(&'a self, uri: &'a str, ctx: &'a CallCtx) -> ResourceFuture<'a> {
        Box::pin(async move {
            let Some(resource) = Resource::of(uri) else {
                let all: Vec<&str> = Resource::ALL.into_iter().map(Resource::uri).collect();
                return Err(SurfaceError::not_found(format!(
                    "no resource `{uri}`. This inspector has: {}",
                    all.join(", ")
                )));
            };
            if !ctx.caps.contains(Cap::Inspect) {
                return Err(SurfaceError::new(
                    ErrorCode::CapabilityDenied,
                    format!("resource `{uri}` needs the `inspect` capability"),
                ));
            }
            let reply = self
                .ask(
                    Kind::Resource(resource),
                    ljos_mcp_core::DEFAULT_RESULT_BYTES / 2,
                )
                .await?;
            let text = match reply.text {
                Some(text) => text,
                None => reply.structured.to_string(),
            };
            Ok(vec![ResourceContent {
                uri: uri.to_string(),
                mime_type: resource.mime(),
                text,
            }])
        })
    }
}
