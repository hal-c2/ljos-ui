//! The in-process client against a surface that answers on the caller's
//! own thread, as the live inspector does.
#![cfg(feature = "client")]

use std::sync::{Arc, Mutex};

use ljos_mcp_core::glue::local::{LocalClient, LocalError, LocalWake};
use ljos_mcp_core::{
    CallCtx, Cap, CapSet, ErrorCode, McpSurface, NoArgs, ResourceContent, ResourceDef, ServerMeta,
    SurfaceError, ToolAnnotations, ToolDef, ToolFuture, ToolRegistry, ToolReply,
};
use serde_json::{Value, json};
use tokio::sync::oneshot;

type Ask = (String, oneshot::Sender<Result<ToolReply, SurfaceError>>);

/// Hands every call to whoever pumps, and wakes them.
struct Deferred {
    tools: ToolRegistry,
    asked: Arc<Mutex<Vec<Ask>>>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl McpSurface for Deferred {
    fn meta(&self) -> ServerMeta {
        ServerMeta {
            name: "deferred",
            version: "0",
            instructions: String::from("Ask and wait."),
        }
    }
    fn tools(&self) -> &ToolRegistry {
        &self.tools
    }
    fn resources(&self, _: &CallCtx) -> Vec<ResourceDef> {
        vec![ResourceDef {
            uri: "demo://notes".into(),
            name: "notes".into(),
            description: "Some notes.".into(),
            mime_type: "text/plain",
        }]
    }
    fn read_resource(&self, uri: &str, _: &CallCtx) -> Result<Vec<ResourceContent>, SurfaceError> {
        match uri {
            "demo://notes" => Ok(vec![ResourceContent {
                uri: uri.to_string(),
                mime_type: "text/plain",
                text: "a note".into(),
            }]),
            "demo://slow" => Err(SurfaceError::new(ErrorCode::Busy, "ask again")
                .with_details(serde_json::json!({ "queued": 16 }))),
            _ => Err(SurfaceError::not_found(format!("no resource `{uri}`"))),
        }
    }
    fn call_tool<'a>(&'a self, name: &'a str, ctx: &'a CallCtx, _: Value) -> ToolFuture<'a> {
        Box::pin(async move {
            self.tools.check(name, ctx)?;
            let (tx, rx) = oneshot::channel();
            self.asked.lock().unwrap().push((name.to_string(), tx));
            (self.wake)();
            rx.await.unwrap_or_else(|_| {
                Err(SurfaceError::new(ErrorCode::AppFaulted, "nobody answered"))
            })
        })
    }
}

fn rig(caps: CapSet) -> (LocalClient, Arc<Mutex<Vec<Ask>>>) {
    let wake = LocalWake::new();
    let asked = Arc::new(Mutex::new(Vec::new()));
    let tools = ToolRegistry::new()
        .declare(
            ToolDef::new::<NoArgs>("look", "Looks.")
                .annotated(ToolAnnotations::READ_ONLY)
                .requires(Cap::Inspect),
        )
        .declare(ToolDef::new::<NoArgs>("picture", "Draws.").requires(Cap::Inspect))
        .declare(ToolDef::new::<NoArgs>("touch", "Touches.").requires(Cap::Input));
    let surface = Arc::new(Deferred {
        tools,
        asked: asked.clone(),
        wake: wake.hook(),
    });
    (LocalClient::connect(surface, caps, &wake).unwrap(), asked)
}

fn answer(asked: &Mutex<Vec<Ask>>) {
    for (name, reply) in asked.lock().unwrap().drain(..) {
        let out = match name.as_str() {
            "look" => Ok(ToolReply::from(json!({ "seen": 3 })).with_text("- window [e1]")),
            "picture" => Ok(ToolReply::from(json!({ "width": 1 }))
                .with_image("image/png", vec![0x89, b'P', b'N', b'G', 0, 255])),
            other => Err(SurfaceError::new(
                ErrorCode::Internal,
                format!("no `{other}`"),
            )),
        };
        let _ = reply.send(out);
    }
}

#[test]
fn the_caller_answers_its_own_call_when_it_is_woken() {
    let (mut client, asked) = rig(CapSet::of([Cap::Inspect]));
    assert_eq!(client.instructions().as_deref(), Some("Ask and wait."));
    let tools = client.tools().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["look", "picture"]);
    assert_eq!(tools[0].read_only, Some(true));
    assert_eq!(tools[0].description, "Looks.");

    let mut pumps = 0;
    let reply = client
        .call("look", json!({}), &mut || {
            pumps += 1;
            answer(&asked);
        })
        .unwrap();
    assert_eq!(pumps, 1, "one wake, one pump: nothing spins");
    assert!(!reply.is_error);
    assert_eq!(reply.structured, json!({ "seen": 3 }));
    assert_eq!(reply.text, "- window [e1]");
    assert_eq!(reply.error_code(), None);
}

#[test]
fn pictures_come_back_as_bytes() {
    let (mut client, asked) = rig(CapSet::of([Cap::Inspect]));
    let reply = client
        .call("picture", Value::Null, &mut || answer(&asked))
        .unwrap();
    assert_eq!(reply.images.len(), 1);
    assert_eq!(reply.images[0].mime, "image/png");
    assert_eq!(reply.images[0].data, [0x89, b'P', b'N', b'G', 0, 255]);
    assert_eq!(reply.structured, json!({ "width": 1 }));
}

#[test]
fn refusals_carry_their_code_and_never_reach_the_pump() {
    let (mut client, asked) = rig(CapSet::of([Cap::Inspect]));
    let mut pumped = false;
    let denied = client
        .call("touch", json!({}), &mut || pumped = true)
        .unwrap();
    assert!(denied.is_error);
    assert_eq!(denied.error_code(), Some("capability_denied"));
    assert!(
        denied.error_message().unwrap().contains("input"),
        "{denied:?}"
    );
    let unknown = client
        .call("fly", json!({}), &mut || pumped = true)
        .unwrap();
    assert_eq!(unknown.error_code(), Some("not_found"));
    assert!(!pumped);
    assert!(asked.lock().unwrap().is_empty());
    // Arguments that are not an object never leave the client.
    let e = client
        .call("look", json!([1]), &mut || pumped = true)
        .unwrap_err();
    assert!(
        matches!(e, LocalError::Refused(ref r) if r.code == ErrorCode::InvalidArgument),
        "{e}"
    );
}

#[test]
fn resources_are_listed_and_read() {
    let (mut client, _) = rig(CapSet::of([Cap::Inspect]));
    assert_eq!(client.resources().unwrap(), ["demo://notes"]);
    assert_eq!(
        client.read_resource("demo://notes", &mut || {}).unwrap(),
        "a note"
    );
    match client.read_resource("demo://secrets", &mut || {}) {
        Err(LocalError::Refused(r)) => assert_eq!(r.code, ErrorCode::NotFound),
        other => panic!("{other:?}"),
    }
    // A refusal the protocol has no number for keeps its code and details.
    match client.read_resource("demo://slow", &mut || {}) {
        Err(LocalError::Refused(r)) => {
            assert_eq!(r.code, ErrorCode::Busy);
            assert_eq!(r.message, "ask again");
            assert_eq!(r.details, Some(serde_json::json!({ "queued": 16 })));
        }
        other => panic!("{other:?}"),
    }
}
