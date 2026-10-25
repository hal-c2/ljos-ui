//! The agent's side of ARCHITECTURE.md §19.4 (features/tooling/mcp) and
//! the `top` overlay (§3.4, §19.10). The attributes stay on one line each:
//! `features:lint` reads them.

use cucumber::{given, then, when};
use gherkin::Step;
use regex::Regex;
use serde_json::Value;

use crate::backend::BackendError;
use crate::value;
use crate::world::LjosWorld;

use super::{StepResult, docstring};

//
// The agent's side of ARCHITECTURE.md §19.4 (features/tooling/mcp). The
// attributes stay on one line each: `features:lint` reads them from here.

use crate::backend::inspector::{self, Ask, Seen, Token};

fn inspector_ask(w: &mut LjosWorld, ask: Ask) -> StepResult {
    // An agent of the authoring tools has no application: it asks the
    // tools, and the same steps read what they answer.
    let (author, files) = w.author();
    if author.connected() {
        return author.ask(files, &ask);
    }
    w.each(|b| b.tooling()?.inspector(&ask))
}

fn inspector_call(w: &mut LjosWorld, tool: &str, args: Value) -> StepResult {
    inspector_ask(
        w,
        Ask::Call {
            tool: tool.to_owned(),
            args,
        },
    )
}

fn inspector_seen(w: &mut LjosWorld, check: impl Fn(&Seen) -> Result<(), String>) -> StepResult {
    let (author, _) = w.author();
    if author.connected() {
        return check(author.seen()?);
    }
    w.each(|b| check(&b.tooling()?.inspector_seen()?).map_err(BackendError::Failed))
}

/// A `"""text` or `"""json` doc string as it was meant.
fn inspector_doc(step: &Step) -> Result<String, String> {
    let text = docstring(step)?;
    let text = match text.split_once('\n') {
        Some((first, rest)) if matches!(first.trim(), "text" | "json") => rest,
        _ => &text,
    };
    Ok(inspector::dedent(text))
}

/// A step's quoted text with `\"` for a quote in it. Other backslashes
/// are the text's own (a pattern's `\d`).
pub(super) fn unquoted(text: &str) -> String {
    text.replace("\\\"", "\"")
}

fn inspector_token(words: &str) -> Result<Token, String> {
    match words {
        "the right token" => Ok(Token::Right),
        "no token" => Ok(Token::Missing),
        "a wrong token" => Ok(Token::Wrong),
        "the token of an older start" => Ok(Token::OlderStart),
        other => Err(format!("{other:?} is not a token this step knows")),
    }
}

#[given("the inspector is connected")]
fn inspector_connected(w: &mut LjosWorld) -> StepResult {
    inspector_ask(
        w,
        Ask::Connect {
            caps: None,
            redact_all: false,
        },
    )
}

#[given("the inspector is connected and masks every editable value")]
fn inspector_connected_masking(w: &mut LjosWorld) -> StepResult {
    inspector_ask(
        w,
        Ask::Connect {
            caps: None,
            redact_all: true,
        },
    )
}

#[given(regex = r#"^the inspector is connected with the capabilities "([^"]*)"$"#)]
fn inspector_connected_with(w: &mut LjosWorld, caps: String) -> StepResult {
    let caps = Some(caps);
    inspector_ask(
        w,
        Ask::Connect {
            caps,
            redact_all: false,
        },
    )
}

#[rustfmt::skip]
#[given(regex = r#"^the inspector is connected with the capabilities "([^"]*)" and masks every editable value$"#)]
fn inspector_connected_with_masking(w: &mut LjosWorld, caps: String) -> StepResult {
    let caps = Some(caps);
    inspector_ask(
        w,
        Ask::Connect {
            caps,
            redact_all: true,
        },
    )
}

#[given("the files are watched")]
fn inspector_files_watched(w: &mut LjosWorld) -> StepResult {
    inspector_ask(w, Ask::Watched)
}

#[given(regex = r"^the host says the application has (\d+) windows?$")]
fn inspector_windows(w: &mut LjosWorld, count: usize) -> StepResult {
    inspector_ask(w, Ask::Windows(count))
}

#[given("the inspector is served on a socket")]
fn inspector_served_on_socket(w: &mut LjosWorld) -> StepResult {
    inspector_ask(w, Ask::ServeOnSocket)
}

#[when("the inspector is stopped")]
fn inspector_stopped(w: &mut LjosWorld) -> StepResult {
    inspector_ask(w, Ask::Stop)
}

#[rustfmt::skip]
#[when(regex = r#"^the inspector is asked to listen on "([^"]+)" (with|without) the word for a network$"#)]
fn inspector_listens(w: &mut LjosWorld, address: String, word: String) -> StepResult {
    let lan = word == "with";
    inspector_ask(w, Ask::Listen { address, lan })
}

#[rustfmt::skip]
#[when(regex = r"^the inspector is asked to listen on a socket whose path is (\d+) characters long$")]
fn inspector_listens_on_long_socket(w: &mut LjosWorld, len: usize) -> StepResult {
    inspector_ask(w, Ask::ListenOnSocket { path_len: Some(len) })
}

#[when("the inspector is asked to listen on a socket whose path is as long as the system allows")]
fn inspector_listens_on_longest_socket(w: &mut LjosWorld) -> StepResult {
    inspector_ask(w, Ask::ListenOnSocket { path_len: None })
}

#[given("the inspector is served on the loopback address")]
fn inspector_served(w: &mut LjosWorld) -> StepResult {
    inspector_ask(w, Ask::Serve)
}

#[given("the agent has requested a snapshot")]
#[when("the agent requests a snapshot")]
fn snapshot_requested(w: &mut LjosWorld) -> StepResult {
    inspector_call(w, "app_snapshot", serde_json::json!({}))
}

#[given(regex = r"^the agent has requested a snapshot of at most (\d+) nodes$")]
#[when(regex = r"^the agent requests a snapshot of at most (\d+) nodes$")]
fn snapshot_requested_capped(w: &mut LjosWorld, nodes: u64) -> StepResult {
    inspector_call(w, "app_snapshot", serde_json::json!({ "max_nodes": nodes }))
}

#[when("the agent requests the next part of the snapshot")]
fn snapshot_next_part(w: &mut LjosWorld) -> StepResult {
    inspector_ask(w, Ask::NextPart)
}

#[when(regex = r#"^the agent requests a snapshot below "(.*)"$"#)]
fn snapshot_below(w: &mut LjosWorld, root: String) -> StepResult {
    inspector_call(
        w,
        "app_snapshot",
        serde_json::json!({ "root": unquoted(&root) }),
    )
}

#[when(regex = r"^the agent requests a snapshot (\d+) levels? deep$")]
fn snapshot_deep(w: &mut LjosWorld, depth: u64) -> StepResult {
    inspector_call(w, "app_snapshot", serde_json::json!({ "depth": depth }))
}

#[when("the agent requests what changed since the last snapshot")]
fn snapshot_changes(w: &mut LjosWorld) -> StepResult {
    inspector_ask(w, Ask::ChangedSince)
}

// The locator is everything between the outer quotes, so that a table of
// examples can give `role=button[name="Save"]` as it is typed.
#[given(regex = r#"^the agent has searched for "(.*)"$"#)]
#[when(regex = r#"^the agent searches for "(.*)"$"#)]
fn agent_searches(w: &mut LjosWorld, locator: String) -> StepResult {
    inspector_call(
        w,
        "app_find",
        serde_json::json!({ "locator": unquoted(&locator) }),
    )
}

#[when(regex = r#"^the agent inspects "(.*)"$"#)]
fn agent_inspects(w: &mut LjosWorld, target: String) -> StepResult {
    inspector_call(
        w,
        "app_inspect",
        serde_json::json!({ "target": unquoted(&target) }),
    )
}

#[when(regex = r#"^the agent inspects every property of "(.*)"$"#)]
fn agent_inspects_all(w: &mut LjosWorld, target: String) -> StepResult {
    let args = serde_json::json!({ "target": unquoted(&target), "all": true });
    inspector_call(w, "app_inspect", args)
}

#[when(regex = r#"^the agent inspects the reference of the "([^"]+)" named "([^"]*)"$"#)]
fn agent_inspects_reference(w: &mut LjosWorld, role: String, name: String) -> StepResult {
    inspector_ask(w, Ask::InspectRef { role, name })
}

#[given(regex = r#"^the agent has called the tool "([^"]+)"$"#)]
#[when(regex = r#"^the agent calls the tool "([^"]+)"$"#)]
fn agent_calls(w: &mut LjosWorld, tool: String) -> StepResult {
    inspector_call(w, &tool, serde_json::json!({}))
}

#[given(regex = r#"^the agent has called the tool "([^"]+)" with:$"#)]
#[when(regex = r#"^the agent calls the tool "([^"]+)" with:$"#)]
fn agent_calls_with(w: &mut LjosWorld, tool: String, step: &Step) -> StepResult {
    let args = inspector_doc(step)?;
    let args =
        serde_json::from_str(&args).map_err(|e| format!("the arguments are not JSON: {e}"))?;
    inspector_call(w, &tool, args)
}

#[when(regex = r#"^the agent, naming itself "([^"]+)", calls the tool "([^"]+)" with:$"#)]
fn agent_named_calls_with(
    w: &mut LjosWorld,
    client: String,
    tool: String,
    step: &Step,
) -> StepResult {
    let args = inspector_doc(step)?;
    let args =
        serde_json::from_str(&args).map_err(|e| format!("the arguments are not JSON: {e}"))?;
    inspector_ask(w, Ask::CallAs { client, tool, args })
}

#[when(regex = r#"^the agent calls the tool "([^"]+)" with the answer's "(/[^"]*)" as "([^"]+)"$"#)]
fn agent_calls_with_the_answer(
    w: &mut LjosWorld,
    tool: String,
    pointer: String,
    field: String,
) -> StepResult {
    inspector_ask(
        w,
        Ask::CallWithAnswer {
            tool,
            pointer,
            field,
        },
    )
}

// One line, whatever its length: the feature lint reads attributes line by line.
#[rustfmt::skip]
#[when(regex = r#"^the agent calls the tool "([^"]+)" on the reference of the "([^"]+)" named "([^"]*)"$"#)]
fn agent_calls_on(w: &mut LjosWorld, tool: String, role: String, name: String) -> StepResult {
    let args = serde_json::json!({});
    inspector_ask(
        w,
        Ask::CallOn {
            tool,
            role,
            name,
            args,
        },
    )
}

#[rustfmt::skip]
#[when(regex = r#"^the agent calls the tool "([^"]+)" on the reference of the "([^"]+)" named "([^"]*)" with:$"#)]
fn agent_calls_on_with(w: &mut LjosWorld, tool: String, role: String, name: String, step: &Step) -> StepResult {
    let args = inspector_doc(step)?;
    let args =
        serde_json::from_str(&args).map_err(|e| format!("the arguments are not JSON: {e}"))?;
    inspector_ask(w, Ask::CallOn { tool, role, name, args })
}

#[rustfmt::skip]
#[given(regex = r#"^the agent has clicked the reference of the "([^"]+)" named "([^"]*)"$"#)]
#[when(regex = r#"^the agent clicks the reference of the "([^"]+)" named "([^"]*)"$"#)]
fn agent_clicks_reference(w: &mut LjosWorld, role: String, name: String) -> StepResult {
    inspector_ask(
        w,
        Ask::CallOn {
            tool: "app_click".to_owned(),
            role,
            name,
            args: serde_json::json!({}),
        },
    )
}

#[rustfmt::skip]
#[when(regex = r#"^the agent calls the tool "([^"]+)" with (\d+) steps of "([^"]+)"$"#)]
fn agent_calls_batch(w: &mut LjosWorld, tool: String, count: usize, inner: String) -> StepResult {
    let steps: Vec<Value> = (0..count)
        .map(|_| serde_json::json!({ "tool": inner, "args": { "keys": "Tab" } }))
        .collect();
    inspector_call(w, &tool, serde_json::json!({ "steps": steps }))
}

#[when("the agent asks for the log since its last request")]
fn agent_reads_log_on(w: &mut LjosWorld) -> StepResult {
    inspector_ask(w, Ask::LogSince)
}

#[when(regex = r#"^the agent reads the resource "([^"]+)"$"#)]
fn agent_reads_resource(w: &mut LjosWorld, uri: String) -> StepResult {
    inspector_ask(w, Ask::Resource(uri))
}

// On one line, as the lint reads the patterns.
#[rustfmt::skip]
#[when(regex = r"^a client asks for a snapshot with (the right token|no token|a wrong token|the token of an older start)$")]
fn client_asks(w: &mut LjosWorld, token: String) -> StepResult {
    let token = inspector_token(&token)?;
    inspector_ask(
        w,
        Ask::Client {
            token,
            origin: None,
            host: None,
            socket: false,
        },
    )
}

#[rustfmt::skip]
#[when(regex = r"^a client asks on the socket for a snapshot with (the right token|no token|a wrong token)$")]
fn client_asks_on_socket(w: &mut LjosWorld, token: String) -> StepResult {
    let token = inspector_token(&token)?;
    inspector_ask(
        w,
        Ask::Client {
            token,
            origin: None,
            host: None,
            socket: true,
        },
    )
}

#[rustfmt::skip]
#[when(regex = r#"^a client asks for a snapshot with (the right token|no token|a wrong token) from the origin "([^"]+)"$"#)]
fn client_asks_from(w: &mut LjosWorld, token: String, origin: String) -> StepResult {
    let token = inspector_token(&token)?;
    inspector_ask(
        w,
        Ask::Client {
            token,
            origin: Some(origin),
            host: None,
            socket: false,
        },
    )
}

#[rustfmt::skip]
#[when(regex = r#"^a client asks for a snapshot with (the right token|no token|a wrong token) for the host "([^"]+)"$"#)]
fn client_asks_for_host(w: &mut LjosWorld, token: String, host: String) -> StepResult {
    let token = inspector_token(&token)?;
    inspector_ask(
        w,
        Ask::Client {
            token,
            origin: None,
            host: Some(host),
            socket: false,
        },
    )
}

#[then(regex = r#"^the snapshot has a "([^"]+)" named "([^"]*)" with a reference$"#)]
fn snapshot_has(w: &mut LjosWorld, role: String, name: String) -> StepResult {
    inspector_seen(w, |s| s.outline_has(&role, &name, true))
}

#[then(regex = r#"^the snapshot has no "([^"]+)" named "([^"]*)"$"#)]
fn snapshot_has_no(w: &mut LjosWorld, role: String, name: String) -> StepResult {
    inspector_seen(w, |s| s.outline_has(&role, &name, false))
}

#[then("the snapshot is:")]
fn snapshot_is(w: &mut LjosWorld, step: &Step) -> StepResult {
    let want = inspector_doc(step)?;
    inspector_seen(w, |s| s.outline_is(&want))
}

#[then(regex = r#"^the snapshot contains "(.*)"$"#)]
fn snapshot_contains(w: &mut LjosWorld, text: String) -> StepResult {
    inspector_seen(w, |s| s.outline_contains(&unquoted(&text), true))
}

#[then(regex = r#"^the snapshot does not contain "(.*)"$"#)]
fn snapshot_contains_no(w: &mut LjosWorld, text: String) -> StepResult {
    inspector_seen(w, |s| s.outline_contains(&unquoted(&text), false))
}

#[then(regex = r#"^the snapshot matches "(.*)"$"#)]
fn snapshot_matches(w: &mut LjosWorld, re: String) -> StepResult {
    inspector_seen(w, |s| s.outline_matches(&unquoted(&re), true))
}

#[then(regex = r#"^the snapshot does not match "(.*)"$"#)]
fn snapshot_matches_not(w: &mut LjosWorld, re: String) -> StepResult {
    inspector_seen(w, |s| s.outline_matches(&unquoted(&re), false))
}

#[then(regex = r"^the snapshot lists (\d+) nodes?$")]
fn snapshot_lists(w: &mut LjosWorld, count: u64) -> StepResult {
    inspector_seen(w, |s| s.outline_lists(count))
}

#[then(regex = r"^the snapshot is (complete|cut short)$")]
fn snapshot_cut(w: &mut LjosWorld, how: String) -> StepResult {
    inspector_seen(w, |s| s.outline_cut(how == "cut short"))
}

#[then(regex = r"^the search finds (\d+) nodes?$")]
fn search_finds(w: &mut LjosWorld, count: u64) -> StepResult {
    inspector_seen(w, |s| s.found(count))
}

#[then(regex = r#"^the search finds an? "([^"]+)" named "([^"]*)"$"#)]
fn search_finds_one(w: &mut LjosWorld, role: String, name: String) -> StepResult {
    inspector_seen(w, |s| s.found_one(&role, &name))
}

// The place is a JSON pointer into the answer's structured content and the
// value is JSON, which never starts with the `a` of "at least".
#[then(regex = r#"^the answer's "(/[^"]*)" is ([^a].*)$"#)]
fn answer_is(w: &mut LjosWorld, pointer: String, json: String) -> StepResult {
    let want = value::parse(&json)?;
    inspector_seen(w, |s| s.answer_is(&pointer, &want))
}

#[then(regex = r#"^the answer's "(/[^"]*)" is at least (-?\d+(?:\.\d+)?)$"#)]
fn answer_at_least(w: &mut LjosWorld, pointer: String, least: f64) -> StepResult {
    inspector_seen(w, |s| s.answer_at_least(&pointer, least))
}

#[then(regex = r#"^the answer's "(/[^"]*)" matches "(.*)"$"#)]
fn answer_matches(w: &mut LjosWorld, pointer: String, re: String) -> StepResult {
    inspector_seen(w, |s| s.answer_matches(&pointer, &unquoted(&re)))
}

#[then(regex = r#"^the answer's "(/[^"]*)" has (\d+) entr(?:y|ies)$"#)]
fn answer_has(w: &mut LjosWorld, pointer: String, count: usize) -> StepResult {
    inspector_seen(w, |s| s.answer_has(&pointer, count))
}

#[then(regex = r#"^the answer's "(/[^"]*)" is absent$"#)]
fn answer_absent(w: &mut LjosWorld, pointer: String) -> StepResult {
    inspector_seen(w, |s| s.answer_absent(&pointer))
}

#[then(regex = r"^the answer is at most (\d+) bytes$")]
fn answer_at_most(w: &mut LjosWorld, most: usize) -> StepResult {
    inspector_seen(w, |s| s.answer_at_most(most))
}

#[then(regex = r#"^the answer's text contains "(.*)"$"#)]
fn answer_text_has(w: &mut LjosWorld, text: String) -> StepResult {
    inspector_seen(w, |s| s.answer_text_has(&unquoted(&text)))
}

#[then(regex = r#"^the answer does not contain "(.*)"$"#)]
fn answer_lacks(w: &mut LjosWorld, text: String) -> StepResult {
    inspector_seen(w, |s| s.answer_lacks(&unquoted(&text)))
}

#[then(regex = r#"^the answer has no inspected property "([^"]+)"$"#)]
fn inspected_property_absent(w: &mut LjosWorld, name: String) -> StepResult {
    inspector_seen(w, |s| s.property_absent(&name))
}

#[then(regex = r#"^the inspected property "([^"]+)" is "([^"]+)"$"#)]
fn inspected_property_provenance(
    w: &mut LjosWorld,
    name: String,
    provenance: String,
) -> StepResult {
    inspector_seen(w, |s| {
        s.property_says(&name, "provenance", &Value::from(provenance.as_str()))
    })
}

#[then(regex = r#"^the inspected property "([^"]+)" is assigned by "([^"]+)"$"#)]
fn inspected_property_assigned(w: &mut LjosWorld, name: String, by: String) -> StepResult {
    inspector_seen(w, |s| {
        s.property_says(&name, "assigned_by", &Value::from(by.as_str()))
    })
}

#[then(regex = r#"^the inspected property "([^"]+)" is not assigned by "([^"]+)"$"#)]
fn inspected_property_not_assigned(w: &mut LjosWorld, name: String, by: String) -> StepResult {
    inspector_seen(w, |s| s.property_not_assigned_by(&name, &by))
}

#[then(regex = r#"^the inspected property "([^"]+)" has the value "(.*)"$"#)]
fn inspected_property_value(w: &mut LjosWorld, name: String, text: String) -> StepResult {
    inspector_seen(w, |s| {
        s.property_says(&name, "value", &Value::from(unquoted(&text)))
    })
}

#[then(regex = r#"^the inspected property "([^"]+)" has the number (-?\d+)$"#)]
fn inspected_property_number(w: &mut LjosWorld, name: String, n: i64) -> StepResult {
    inspector_seen(w, |s| s.property_says(&name, "number", &Value::from(n)))
}

#[then(regex = r#"^the inspected property "([^"]+)" has a value that matches "(.*)"$"#)]
fn inspected_property_value_matches(w: &mut LjosWorld, name: String, re: String) -> StepResult {
    inspector_seen(w, |s| s.property_matches(&name, &unquoted(&re)))
}

#[then(regex = r#"^the inspected property "([^"]+)" was written at line (\d+)$"#)]
fn inspected_property_line(w: &mut LjosWorld, name: String, line: u64) -> StepResult {
    inspector_seen(w, |s| s.property_says(&name, "line", &Value::from(line)))
}

#[then(regex = r#"^the inspected property "([^"]+)" is bound at tier (\w+)$"#)]
fn inspected_property_tier(w: &mut LjosWorld, name: String, tier: String) -> StepResult {
    inspector_seen(w, |s| {
        s.property_says(&name, "tier", &Value::from(tier.as_str()))
    })
}

#[then(regex = r#"^the inspected property "([^"]+)" is bound to the expression "(.*)"$"#)]
fn inspected_property_expression(w: &mut LjosWorld, name: String, text: String) -> StepResult {
    inspector_seen(w, |s| {
        s.property_says(&name, "expression", &Value::from(unquoted(&text)))
    })
}

#[then("the socket can be used by its owner alone")]
fn socket_private(w: &mut LjosWorld) -> StepResult {
    inspector_seen(w, Seen::socket_private)
}

#[then("the socket is gone")]
fn socket_gone(w: &mut LjosWorld) -> StepResult {
    inspector_seen(w, Seen::socket_gone)
}

#[then("the token file is gone")]
fn token_file_gone(w: &mut LjosWorld) -> StepResult {
    inspector_seen(w, Seen::token_file_gone)
}

#[then(regex = r#"^the inspector is refused with "(.*)"$"#)]
fn inspector_refused(w: &mut LjosWorld, word: String) -> StepResult {
    inspector_seen(w, |s| s.listening_refused(&unquoted(&word)))
}

#[then("the inspector is served")]
fn inspector_is_served(w: &mut LjosWorld) -> StepResult {
    inspector_seen(w, Seen::listening_served)
}

#[then("the inspector is allowed to listen there")]
fn inspector_allowed(w: &mut LjosWorld) -> StepResult {
    inspector_seen(w, Seen::listening_allowed)
}

#[then(regex = r#"^the warning mentions "(.*)"$"#)]
fn warning_mentions(w: &mut LjosWorld, text: String) -> StepResult {
    inspector_seen(w, |s| s.listening_warned(&unquoted(&text)))
}

#[then(regex = r#"^the agent is refused with "([a-z_]+)"$"#)]
fn agent_refused(w: &mut LjosWorld, code: String) -> StepResult {
    inspector_seen(w, |s| s.refused_with(&code))
}

#[then("the agent is told the reference is stale")]
fn agent_told_stale(w: &mut LjosWorld) -> StepResult {
    inspector_seen(w, |s| s.refused_with("stale_ref"))
}

#[then(regex = r#"^the refusal mentions "(.*)"$"#)]
fn refusal_mentions(w: &mut LjosWorld, word: String) -> StepResult {
    inspector_seen(w, |s| s.refusal_mentions(&unquoted(&word)))
}

#[then(regex = r"^the refusal names (\d+) candidates?$")]
fn refusal_candidates(w: &mut LjosWorld, count: usize) -> StepResult {
    inspector_seen(w, |s| s.refusal_candidates(count))
}

#[then(regex = r#"^the agent is offered the tools "([^"]+)"$"#)]
fn agent_offered(w: &mut LjosWorld, names: String) -> StepResult {
    let names: Vec<&str> = names.split(',').map(str::trim).collect();
    inspector_seen(w, |s| s.offered(&names))
}

#[then("every tool the agent is offered is marked read-only")]
fn tools_read_only(w: &mut LjosWorld) -> StepResult {
    inspector_seen(w, Seen::all_read_only)
}

#[then(regex = r#"^the inspector's instructions match "(.*)"$"#)]
fn instructions_match(w: &mut LjosWorld, re: String) -> StepResult {
    inspector_seen(w, |s| s.instructions_match(&unquoted(&re)))
}

#[then(regex = r"^the answer has a PNG picture (\d+) by (\d+) pixels$")]
fn answer_picture(w: &mut LjosWorld, width: u32, height: u32) -> StepResult {
    inspector_seen(w, |s| s.picture_is(width, height))
}

#[then(regex = r#"^the pixel at (\d+), (\d+) of the picture is "(#[0-9a-fA-F]{6})"$"#)]
fn answer_pixel(w: &mut LjosWorld, x: u32, y: u32, colour: String) -> StepResult {
    inspector_seen(w, |s| s.pixel_is(x, y, &colour))
}

#[then(regex = r#"^the resource contains "(.*)"$"#)]
fn resource_contains(w: &mut LjosWorld, text: String) -> StepResult {
    inspector_seen(w, |s| s.resource_contains(&unquoted(&text)))
}

#[then("the client is answered")]
fn client_answered(w: &mut LjosWorld) -> StepResult {
    inspector_seen(w, Seen::client_answered)
}

#[then(regex = r"^the client is refused with status (\d+)$")]
fn client_refused(w: &mut LjosWorld, status: u16) -> StepResult {
    inspector_seen(w, |s| s.client_refused(status))
}

#[then(regex = r#"^the client's answer contains "(.*)"$"#)]
fn client_contains(w: &mut LjosWorld, text: String) -> StepResult {
    inspector_seen(w, |s| s.client_contains(&unquoted(&text), true))
}

#[then(regex = r#"^the client's answer does not contain "(.*)"$"#)]
fn client_contains_no(w: &mut LjosWorld, text: String) -> StepResult {
    inspector_seen(w, |s| s.client_contains(&unquoted(&text), false))
}

#[then("the token file can be read by its owner alone")]
fn token_file_private(w: &mut LjosWorld) -> StepResult {
    inspector_seen(w, Seen::token_file_private)
}

#[then("the token file names an address on this machine")]
fn token_file_local(w: &mut LjosWorld) -> StepResult {
    inspector_seen(w, Seen::token_file_local)
}

// Nothing is waiting, no frame is asked for and nothing woke the
// application since it last ran: a host would be asleep.
#[then("the application is left idle")]
fn application_idle(w: &mut LjosWorld) -> StepResult {
    inspector_ask(w, Ask::Rests)
}

#[then("the accessibility tree is not being kept")]
fn tree_given_back(w: &mut LjosWorld) -> StepResult {
    inspector_ask(w, Ask::TreeGivenBack)
}

// The `top` overlay (ARCHITECTURE.md §3.4, §19.10). Ljós only.

use crate::backend::top::TopSeen;

fn top_seen(w: &mut LjosWorld, check: impl Fn(&TopSeen) -> Result<(), String>) -> StepResult {
    w.each(|b| check(&b.tooling()?.top_seen()?).map_err(BackendError::Failed))
}

#[when("the top overlay is shown")]
fn top_shown(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.tooling()?.top(true))
}

#[when("the top overlay is hidden")]
fn top_hidden(w: &mut LjosWorld) -> StepResult {
    w.each(|b| b.tooling()?.top(false))
}

#[then("the top overlay shows nothing")]
fn top_shows_nothing(w: &mut LjosWorld) -> StepResult {
    top_seen(w, |seen| match &seen.sheet {
        None => Ok(()),
        Some(sheet) => Err(format!("the top overlay shows {:?}", sheet.title)),
    })
}

#[then(regex = r#"^the top overlay's title is "([^"]*)"$"#)]
fn top_title_is(w: &mut LjosWorld, title: String) -> StepResult {
    top_seen(w, |seen| {
        let sheet = seen.sheet()?;
        if sheet.title == title {
            Ok(())
        } else {
            Err(format!(
                "the top overlay's title is {:?}, expected {title:?}",
                sheet.title
            ))
        }
    })
}

#[then(regex = r#"^the top overlay's title matches "(.+)"$"#)]
fn top_title_matches(w: &mut LjosWorld, pattern: String) -> StepResult {
    let wanted = Regex::new(&pattern).map_err(|e| format!("{pattern:?}: {e}"))?;
    top_seen(w, |seen| {
        let sheet = seen.sheet()?;
        if wanted.is_match(&sheet.title) {
            Ok(())
        } else {
            Err(format!(
                "the top overlay's title is {:?}, which does not match {pattern:?}",
                sheet.title
            ))
        }
    })
}

#[then(regex = r#"^the top overlay lists the lanes "([^"]*)"$"#)]
fn top_lists_lanes(w: &mut LjosWorld, lanes: String) -> StepResult {
    top_seen(w, |seen| {
        let listed = seen.sheet()?.lanes.join(", ");
        if listed == lanes {
            Ok(())
        } else {
            Err(format!(
                "the top overlay lists the lanes {listed:?}, expected {lanes:?}"
            ))
        }
    })
}

#[then(regex = r#"^the top overlay has charged "(.+)"$"#)]
fn top_has_charged(w: &mut LjosWorld, owner: String) -> StepResult {
    let owner = unquoted(&owner);
    top_seen(w, |seen| {
        let charged = &seen.sheet()?.charged;
        if charged.contains(&owner) {
            Ok(())
        } else {
            Err(format!(
                "the top overlay has charged {charged:?}, and not {owner:?}"
            ))
        }
    })
}

#[then("the top overlay has charged nobody")]
fn top_has_charged_nobody(w: &mut LjosWorld) -> StepResult {
    top_seen(w, |seen| {
        let charged = &seen.sheet()?.charged;
        if charged.is_empty() {
            Ok(())
        } else {
            Err(format!("the top overlay has charged {charged:?}"))
        }
    })
}

// A host would be asleep: the scene asks for no frame and for no wake,
// with the overlay over it.
#[then("the top overlay keeps no frame coming")]
fn top_keeps_no_frame_coming(w: &mut LjosWorld) -> StepResult {
    top_seen(w, |seen| {
        seen.sheet()?;
        match &seen.frame_wanted {
            None => Ok(()),
            Some(why) => Err(format!("a frame is wanted: {why}")),
        }
    })
}
