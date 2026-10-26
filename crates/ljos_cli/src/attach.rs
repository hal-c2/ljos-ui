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
use tokio::io::{AsyncBufRead, AsyncBufReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::sync::Mutex;

use crate::inspector::{self, Answer, Trust, find_token_file, read_token_file};

#[derive(Args)]
pub struct Attach {
    /// The application: the token file its inspector wrote, or a
    /// directory to look for the newest live one in, as `ljos observe`
    /// takes it. Read again for every message, so that an application
    /// started again is found.
    #[arg(default_value = ".")]
    target: PathBuf,
    /// Lets a token file named as the target give an address beyond this
    /// machine (an inspector started with `--mcp-bind`) or a socket of
    /// another user's. A file found in a directory never may.
    #[arg(long)]
    remote: bool,
}

/// How long a message may take beyond the wait it asks for.
const PATIENCE: Duration = Duration::from_secs(30);

impl Attach {
    pub fn run(self) -> anyhow::Result<()> {
        let (token_file, trust) = find_token_file(&self.target)?;
        let trust = Trust {
            remote: self.remote,
            ..trust
        };
        let target = read_token_file(&token_file, trust)?;
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
            .block_on(bridge(token_file, trust))
    }
}

async fn bridge(token_file: PathBuf, trust: Trust) -> anyhow::Result<()> {
    bridge_io(
        tokio::io::BufReader::new(tokio::io::stdin()),
        tokio::io::stdout(),
        move |line| {
            let token_file = token_file.clone();
            async move { relay(&token_file, trust, line).await }
        },
    )
    .await
}

// DECISION: use the observer's 16-call limit, including responses waiting
// for stdout. Acquire a slot before reading so neither slow peers nor a
// client that stops reading can grow a queue of requests or responses.
async fn bridge_io<R, W, F, Fut>(mut input: R, output: W, relay: F) -> anyhow::Result<()>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin + Send + 'static,
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Vec<String>> + Send + 'static,
{
    let out = Arc::new(Mutex::new(output));
    let mut running = tokio::task::JoinSet::new();
    loop {
        while running.try_join_next().is_some() {}
        if running.len() >= inspector::IN_FLIGHT {
            running.join_next().await;
        }
        let Some(line) = read_line(&mut input).await? else {
            break;
        };
        if line.trim().is_empty() {
            continue;
        }
        let out = Arc::clone(&out);
        let response = relay(line);
        running.spawn(async move {
            let said = response.await;
            let mut out = out.lock().await;
            for message in said {
                out.write_all(message.as_bytes()).await?;
                out.write_all(b"\n").await?;
            }
            out.flush().await
        });
    }
    // The client has closed stdin: what it asked last is still answered.
    while running.join_next().await.is_some() {}
    Ok(())
}

// DECISION: stdin JSON-RPC lines have the same 1 MiB cap as HTTP bodies.
// Stop the relay on overflow, without draining an unending hostile line.
async fn read_line<R: AsyncBufRead + Unpin>(input: &mut R) -> std::io::Result<Option<String>> {
    let mut line = Vec::new();
    loop {
        let available = input.fill_buf().await?;
        if available.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                decode_line(line).map(Some)
            };
        }
        let end = available.iter().position(|b| *b == b'\n');
        let count = end.unwrap_or(available.len());
        if count > inspector::MAX_BODY_BYTES.saturating_sub(line.len()) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "JSON-RPC line exceeds 1 MiB",
            ));
        }
        line.extend_from_slice(&available[..count]);
        input.consume(count + usize::from(end.is_some()));
        if end.is_some() {
            return decode_line(line).map(Some);
        }
    }
}

fn decode_line(mut line: Vec<u8>) -> std::io::Result<String> {
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    String::from_utf8(line).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

// DECISION: app_watch allows one hour; arbitrary wait_ms must not extend
// relay retention beyond that protocol maximum plus transport patience.
fn request_limit(parsed: &Json) -> Duration {
    let wait = parsed["params"]["arguments"]["wait_ms"]
        .as_u64()
        .unwrap_or(0)
        .min(3_600_000);
    Duration::from_millis(wait)
        .checked_add(PATIENCE)
        .unwrap_or(PATIENCE)
}

/// The lines `message` is answered with: the inspector's messages, or a
/// JSON-RPC error when it could not be asked. A notification is answered
/// with nothing.
async fn relay(token_file: &std::path::Path, trust: Trust, message: String) -> Vec<String> {
    let parsed: Json = serde_json::from_str(&message).unwrap_or(Json::Null);
    let id = parsed.get("id").cloned();
    let limit = request_limit(&parsed);
    let got = match read_token_file(token_file, trust) {
        Ok(target) => inspector::post(&target, message, limit).await,
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

    #[tokio::test]
    async fn stdin_lines_are_bounded_including_without_a_newline() {
        for suffix in ["", "\n"] {
            let input = format!("{}{suffix}", "x".repeat(inspector::MAX_BODY_BYTES + 1));
            let mut input = input.as_bytes();
            assert_eq!(
                read_line(&mut input).await.unwrap_err().kind(),
                std::io::ErrorKind::InvalidData
            );
        }
        let text = "x".repeat(inspector::MAX_BODY_BYTES);
        let mut input = text.as_bytes();
        assert_eq!(read_line(&mut input).await.unwrap(), Some(text));
        let mut input = b"one\r\ntwo\nlast".as_slice();
        for expected in ["one", "two", "last"] {
            assert_eq!(
                read_line(&mut input).await.unwrap().as_deref(),
                Some(expected)
            );
        }
        assert!(read_line(&mut input).await.unwrap().is_none());
    }

    #[test]
    fn wait_retention_is_clamped_without_overflow() {
        let limit = |wait| request_limit(&json!({"params":{"arguments":{"wait_ms":wait}}}));
        assert_eq!(request_limit(&Json::Null), PATIENCE);
        assert_eq!(limit(50), PATIENCE + Duration::from_millis(50));
        assert_eq!(limit(3_600_000), Duration::from_secs(3630));
        assert_eq!(limit(u64::MAX), Duration::from_secs(3630));
    }

    #[tokio::test]
    async fn slow_targets_bound_tasks_and_backpressure_input() {
        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        let (started, mut calls) = tokio::sync::mpsc::unbounded_channel();
        let input = "request\n".repeat(inspector::IN_FLIGHT + 1);
        let held = Arc::clone(&gate);
        let bridge = tokio::spawn(async move {
            bridge_io(input.as_bytes(), tokio::io::sink(), move |_| {
                let gate = Arc::clone(&held);
                let started = started.clone();
                async move {
                    started.send(()).unwrap();
                    gate.acquire().await.unwrap().forget();
                    Vec::new()
                }
            })
            .await
        });
        for _ in 0..inspector::IN_FLIGHT {
            calls.recv().await.unwrap();
        }
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        assert!(calls.try_recv().is_err());
        gate.add_permits(inspector::IN_FLIGHT + 1);
        bridge.await.unwrap().unwrap();
        assert_eq!(calls.recv().await, Some(()));
        assert_eq!(calls.recv().await, None);
    }

    #[tokio::test]
    async fn blocked_stdout_holds_slots_and_valid_traffic_is_written() {
        use tokio::io::AsyncReadExt as _;
        let (output, mut client) = tokio::io::duplex(1);
        let (started, mut calls) = tokio::sync::mpsc::unbounded_channel();
        let request = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}).to_string();
        let response = json!({"jsonrpc": "2.0", "id": 1, "result": {"tools": []}}).to_string();
        let expected = format!("{response}\n").repeat(inspector::IN_FLIGHT + 1);
        let input = format!("{request}\n").repeat(inspector::IN_FLIGHT + 1);
        let bridge = tokio::spawn(async move {
            bridge_io(input.as_bytes(), output, move |line| {
                let parsed: Json = serde_json::from_str(&line).unwrap();
                assert_eq!(parsed["method"], "tools/list");
                started.send(()).unwrap();
                let response = response.clone();
                async move { answer_lines(Some(json!(1)), answer(200, &response)) }
            })
            .await
        });
        for _ in 0..inspector::IN_FLIGHT {
            calls.recv().await.unwrap();
        }
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        assert!(calls.try_recv().is_err());
        let mut received = String::new();
        client.read_to_string(&mut received).await.unwrap();
        bridge.await.unwrap().unwrap();
        assert_eq!(received, expected);
    }

    fn answer(status: u16, body: &str) -> Result<Answer, String> {
        Ok(Answer {
            status,
            body: body.as_bytes().to_vec(),
            retry_after: None,
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
