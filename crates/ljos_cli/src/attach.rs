//! `ljos attach`: a running application's inspector on stdio, for an MCP
//! client that starts its servers as programs and speaks neither HTTP nor
//! Unix sockets (ARCHITECTURE.md §19.7). Each line read is one JSON-RPC
//! message, posted to the inspector as it is; each message the inspector
//! sends back is one line written. Messages are posted as they come, so a
//! call that waits (`app_watch`) does not hold back the next.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use clap::Args;
use serde_json::{Value as Json, json};
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _};
use tokio::sync::Mutex;

use crate::inspector::{self, Answer, find_token_file, read_token_file};

#[derive(Args)]
pub struct Attach {
    /// The application: the token file its inspector wrote, or a
    /// directory to look for the newest live one in, as `ljos observe`
    /// takes it. Read again for every message, so that an application
    /// started again is found.
    #[arg(default_value = ".")]
    target: PathBuf,
}

/// How long a message may take beyond the wait it asks for.
const PATIENCE: Duration = Duration::from_secs(30);

impl Attach {
    pub fn run(self) -> anyhow::Result<()> {
        let token_file = find_token_file(&self.target)?;
        let target = read_token_file(&token_file)?;
        crate::stderr::line(&format!(
            "ljos: attached to {} through {}",
            target.url,
            token_file.display()
        ));
        tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .context("starting the async runtime")?
            .block_on(bridge(token_file))
    }
}

async fn bridge(token_file: PathBuf) -> anyhow::Result<()> {
    let out = Arc::new(Mutex::new(tokio::io::stdout()));
    let mut lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();
    let mut running = tokio::task::JoinSet::new();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let (out, token_file) = (Arc::clone(&out), token_file.clone());
        running.spawn(async move {
            let said = relay(&token_file, line).await;
            let mut out = out.lock().await;
            for message in said {
                // A closed stdout is a client gone; stdin ends next.
                let _ = out.write_all(message.as_bytes()).await;
                let _ = out.write_all(b"\n").await;
            }
            let _ = out.flush().await;
        });
        // What has ended is let go, so that a long session holds nothing.
        while running.try_join_next().is_some() {}
    }
    // The client has closed stdin: what it asked last is still answered.
    while running.join_next().await.is_some() {}
    Ok(())
}

/// The lines `message` is answered with: the inspector's messages, or a
/// JSON-RPC error when it could not be asked. A notification is answered
/// with nothing.
async fn relay(token_file: &std::path::Path, message: String) -> Vec<String> {
    let parsed: Json = serde_json::from_str(&message).unwrap_or(Json::Null);
    let id = parsed.get("id").cloned();
    let wait = parsed["params"]["arguments"]["wait_ms"]
        .as_u64()
        .unwrap_or(0);
    let got = match read_token_file(token_file) {
        Ok(target) => {
            let limit = Duration::from_millis(wait) + PATIENCE;
            inspector::post(&target, message, limit).await
        }
        Err(why) => Err(format!("{why:#}")),
    };
    answer_lines(id, got)
}

fn answer_lines(id: Option<Json>, got: Result<Answer, String>) -> Vec<String> {
    let why = match got {
        Ok(answer) if (200..300).contains(&answer.status) => {
            let messages: Vec<String> = answer.messages().into_iter().map(str::to_owned).collect();
            if !messages.is_empty() || id.is_none() {
                return messages;
            }
            format!("the inspector answered {} with no message", answer.status)
        }
        Ok(answer) => format!(
            "the inspector answered {}: {}",
            answer.status,
            String::from_utf8_lossy(&answer.body).trim()
        ),
        Err(why) => why,
    };
    match id {
        // -32603: the JSON-RPC code for an error of the server's own.
        Some(id) => vec![
            json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32603, "message": why}})
                .to_string(),
        ],
        None => {
            crate::stderr::line(&format!("ljos attach: {why}"));
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(status: u16, body: &str) -> Result<Answer, String> {
        Ok(Answer {
            status,
            body: body.as_bytes().to_vec(),
        })
    }

    #[test]
    fn the_inspectors_messages_are_written_one_a_line() {
        let ok = r#"{"jsonrpc":"2.0","id":1,"result":{}}"#;
        let streamed = format!("event: message\ndata: {ok}\n\n");
        assert_eq!(answer_lines(Some(json!(1)), answer(200, &streamed)), [ok]);
        // A notification is taken with 202 and nothing to say.
        assert!(answer_lines(None, answer(202, "")).is_empty());
    }

    #[test]
    fn a_request_that_was_not_answered_is_answered_with_an_error() {
        let lines = answer_lines(Some(json!(7)), Err("connection refused".into()));
        let error: Json = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(error["id"], 7);
        assert_eq!(error["error"]["message"], "connection refused");
        let lines = answer_lines(Some(json!("a")), answer(401, "no token"));
        let error: Json = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(error["id"], "a");
        assert!(error["error"]["message"].as_str().unwrap().contains("401"));
    }
}
