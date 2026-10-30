//! Calls made to a started application's inspector as fast as one client
//! makes them, written as plain HTTP so that a refusal is seen as it is
//! and not asked again, as an MCP client would (ARCHITECTURE.md §19.7).

use std::{
    io::{Read as _, Write as _},
    net::{SocketAddr, TcpStream},
    path::Path,
    time::{Duration, Instant},
};

use serde_json::Value;

/// The inspector counts the calls of the last second.
const WINDOW: Duration = Duration::from_secs(1);

/// What one call was answered.
#[derive(Clone, Debug)]
pub(super) struct Called {
    pub status: u16,
    /// The answer's header lines, lower-cased.
    pub head: String,
}

/// What a burst of calls came to.
#[derive(Clone, Debug)]
pub(super) struct Burst {
    pub calls: Vec<Called>,
    pub took: Duration,
}

/// The address and the token the inspector's token file names.
fn target(token_file: &Path) -> Result<(SocketAddr, String), String> {
    let text = std::fs::read_to_string(token_file)
        .map_err(|e| format!("cannot read {}: {e}", token_file.display()))?;
    let body: Value =
        serde_json::from_str(&text).map_err(|e| format!("the token file is not JSON: {e}"))?;
    let url = body["url"].as_str().unwrap_or_default();
    let addr = url
        .strip_prefix("http://")
        .and_then(|rest| rest.split('/').next())
        .and_then(|authority| authority.parse().ok())
        .ok_or_else(|| format!("the token file names no loopback port: {url:?}"))?;
    let token = body["token"]
        .as_str()
        .ok_or("the token file has no token")?
        .to_owned();
    Ok((addr, token))
}

fn call(addr: SocketAddr, token: &str, id: usize) -> Result<Called, String> {
    let body = format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"ping"}}"#);
    let request = format!(
        "POST /mcp HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\
         Accept: application/json, text/event-stream\r\nContent-Type: application/json\r\n\
         Authorization: Bearer {token}\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let mut stream = TcpStream::connect(addr).map_err(|e| format!("call {id}: {e}"))?;
    stream
        .set_read_timeout(Some(super::ANSWER))
        .map_err(|e| format!("call {id}: {e}"))?;
    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("call {id}: {e}"))?;
    let mut raw = Vec::new();
    stream
        .read_to_end(&mut raw)
        .map_err(|e| format!("call {id}: {e}"))?;
    let raw = String::from_utf8_lossy(&raw);
    let head = raw.split_once("\r\n\r\n").map_or(&*raw, |(h, _)| h);
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("call {id} had no status line: {raw:?}"))?;
    Ok(Called {
        status,
        head: head.to_ascii_lowercase(),
    })
}

/// Makes `count` calls one after the other with the right token. The
/// inspector's second is let pass first, so that what the agent asked
/// when it connected is not counted among them.
pub(super) fn burst(token_file: &Path, count: usize) -> Result<Burst, String> {
    let (addr, token) = target(token_file)?;
    std::thread::sleep(WINDOW);
    let start = Instant::now();
    let calls = (1..=count)
        .map(|id| call(addr, &token, id))
        .collect::<Result<Vec<_>, _>>()?;
    let took = start.elapsed();
    if took >= WINDOW {
        // The inspector counted them over more than one second, which
        // says nothing of how many it answers in one.
        return Err(format!(
            "the {count} calls took {} ms, more than the second they had to be made in",
            took.as_millis()
        ));
    }
    Ok(Burst { calls, took })
}

impl Burst {
    pub(super) fn answered(&self, first: usize) -> Result<(), String> {
        let calls = self
            .calls
            .get(..first)
            .ok_or_else(|| format!("only {} calls were made, not {first}", self.calls.len()))?;
        match calls.iter().position(|c| c.status != 200) {
            None => Ok(()),
            Some(at) => Err(format!(
                "call {} of {first} was answered with status {} after {} ms:\n{}",
                at + 1,
                calls[at].status,
                self.took.as_millis(),
                calls[at].head
            )),
        }
    }

    pub(super) fn refused(&self, nth: usize, status: u16, header: &str) -> Result<(), String> {
        let call = nth
            .checked_sub(1)
            .and_then(|at| self.calls.get(at))
            .ok_or_else(|| format!("only {} calls were made, not {nth}", self.calls.len()))?;
        if call.status != status {
            return Err(format!(
                "call {nth} was answered with status {}, not {status}, after {} ms:\n{}",
                call.status,
                self.took.as_millis(),
                call.head
            ));
        }
        let want = header.to_ascii_lowercase();
        if call.head.lines().any(|l| l.trim() == want) {
            Ok(())
        } else {
            Err(format!("call {nth} has no {header:?}:\n{}", call.head))
        }
    }
}

/// The rate the token file states.
pub(super) fn stated(token_file: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(token_file)
        .map_err(|e| format!("cannot read {}: {e}", token_file.display()))?;
    let body: Value =
        serde_json::from_str(&text).map_err(|e| format!("the token file is not JSON: {e}"))?;
    Ok(body["calls_per_second"].clone())
}
