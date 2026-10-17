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

use super::args::{self, Act, INSTRUCTIONS, Kind, Resource, SERVER_NAME};
use super::reload::{ReloadFeed, ReloadState};

/// Requests that may wait for the UI thread at one time (§19.5). One more
/// is answered `busy`.
pub const INBOX_CAPACITY: usize = 16;
/// Snapshots that may be in the making at one time (§19.7).
pub const MAX_SNAPSHOT_JOBS: usize = 2;

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
    /// What the connection that asked may do: `app_status` lists it.
    pub(crate) caps: CapSet,
}

/// The inspector as an MCP server sees it. `Send + Sync`: it holds the
/// sending end of the UI thread's inbox and nothing of the application.
pub struct InspectorSurface {
    registry: ToolRegistry,
    handle: UiHandle<DevReq>,
    snapshots: Arc<AtomicUsize>,
    timeout: Duration,
    feed: Arc<ReloadFeed>,
}

impl fmt::Debug for InspectorSurface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InspectorSurface")
            .field("tools", &self.registry.len())
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

/// The refusal of a tool whose capability the connection does not hold. It
/// names the capability and says how to be given it: a client cannot ask
/// for more than it was started with, so the person who starts the
/// application has to.
fn refusal(what: &str, needs: Cap, held: CapSet) -> SurfaceError {
    let grant = held
        .with(needs)
        .iter()
        .map(|c| c.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let have = if held.iter().next().is_none() {
        "none".to_string()
    } else {
        held.iter()
            .map(|c| c.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let never = if needs == Cap::Eval {
        " (`eval` is never implied by another capability; it has to be named)"
    } else {
        ""
    };
    SurfaceError::new(
        ErrorCode::CapabilityDenied,
        format!(
            "{what} needs the `{needs}` capability{never}, and this connection holds: {have}. \
             Start the application with `--mcp={grant}` to grant it"
        ),
    )
    .with_details(json!({
        "required": needs.as_str(),
        "granted": held.iter().map(|c| c.as_str()).collect::<Vec<_>>(),
    }))
}

impl InspectorSurface {
    pub(crate) fn new(handle: UiHandle<DevReq>, timeout: Duration, feed: Arc<ReloadFeed>) -> Self {
        InspectorSurface {
            registry: args::registry(),
            handle,
            snapshots: Arc::new(AtomicUsize::new(0)),
            timeout,
            feed,
        }
    }

    /// Waits until a reload newer than `after` was applied or one has
    /// failed, for at most `wait_ms`. Whether it had to wait.
    ///
    /// It sleeps on the reload feed and on one deadline, and wakes nobody:
    /// the host publishes every outcome it observes, and it observes one
    /// whenever the watcher wakes it. A timer that poked the UI thread
    /// meanwhile would make an idle application run for a client that only
    /// waits.
    async fn wait_for_reload(&self, after: u64, wait_ms: u64) -> bool {
        let mut rx = self.feed.subscribe();
        let start: ReloadState = rx.borrow().clone();
        let arrived = |s: &ReloadState| s.generation > after || s.seq != start.seq;
        if arrived(&start) {
            return false;
        }
        let waiting = async {
            loop {
                match rx.changed().await {
                    Ok(()) if arrived(&rx.borrow()) => return,
                    Ok(()) => {}
                    // The application is gone.
                    Err(_) => return,
                }
            }
        };
        let _ = tokio::time::timeout(Duration::from_millis(wait_ms), waiting).await;
        true
    }

    async fn ask(&self, kind: Kind, per_copy: usize, caps: CapSet) -> Answer {
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
            caps,
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
            if !args::TOOLS.contains(&name) {
                // Says which tools there are.
                return args::parse(name, Value::Null).map(|_| ToolReply::from(Value::Null));
            }
            // The capability is checked before the arguments are read, so
            // that a connection that may not use a tool learns nothing about
            // it but that.
            let def = self.registry.all().find(|d| d.name == name);
            if let Some(needs) = def.and_then(|d| d.required_cap)
                && !ctx.caps.contains(needs)
            {
                return Err(refusal(&format!("tool `{name}`"), needs, ctx.caps));
            }
            let per_copy = def.map_or(ljos_mcp_core::DEFAULT_RESULT_BYTES / 2, |d| {
                d.per_copy_bytes()
            });
            let kind = args::parse(name, args)?;
            if let Kind::Mailboxes(a) = &kind {
                // Read here, so a UI thread that is stuck still shows its
                // inbox filling up.
                return Ok(super::observe::mailboxes(a));
            }
            if let Kind::Act(Act::Batch(steps)) = &kind {
                // Everything a batch will do is allowed before any of it is.
                for (i, (tool, act)) in steps.iter().enumerate() {
                    if !ctx.caps.contains(act.needs()) {
                        return Err(refusal(
                            &format!("step {i} of the batch (`{tool}`)"),
                            act.needs(),
                            ctx.caps,
                        ));
                    }
                }
            }
            let mut waited = None;
            if let Kind::Status(a) = &kind
                && let Some(after) = a.after_reload
            {
                waited = Some(self.wait_for_reload(after, a.wait_ms.unwrap_or(0)).await);
            }
            let mut reply = self.ask(kind, per_copy, ctx.caps).await?;
            if let Some(waited) = waited {
                reply.structured["reload"]["waited"] = json!(waited);
            }
            Ok(reply)
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
                    ctx.caps,
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
