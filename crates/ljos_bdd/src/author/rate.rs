//! Calls made to a started application's inspector as fast as one client
//! makes them, written as plain HTTP so that a refusal is seen as it is
//! and not asked again, as an MCP client would (ARCHITECTURE.md §19.7).
//! The desktop backend fills an application's second with them too, to
//! see what its observer does when it is turned away.

use std::{
    io::{Read as _, Write as _},
    net::{SocketAddr, TcpStream},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use serde_json::Value;

/// The inspector counts the calls of the last second.
const WINDOW: Duration = Duration::from_secs(1);

/// What one call was answered.
#[derive(Clone, Debug)]
pub(crate) struct Called {
    pub status: u16,
    /// The answer's header lines, lower-cased.
    pub head: String,
}

/// Calls made on a thread of their own for as long as they are to keep
/// the inspector's second full. Let go of, they stop and are waited for:
/// a step that failed before [`Flood::finish`] leaves none behind to turn
/// away the calls of the next.
#[derive(Debug)]
pub(crate) struct Flood {
    stop: Arc<AtomicBool>,
    calls: Option<JoinHandle<Result<(), String>>>,
}

impl Flood {
    pub(crate) fn start(
        until: Instant,
        mut call: impl FnMut() -> Result<(), String> + Send + 'static,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        let calls = std::thread::spawn(move || {
            while Instant::now() < until && !stopped.load(Ordering::Relaxed) {
                call()?;
                // A pause between them keeps the loopback's ports to a few
                // thousand.
                std::thread::sleep(Duration::from_micros(500));
            }
            Ok(())
        });
        Self {
            stop,
            calls: Some(calls),
        }
    }

    /// Waits for the last of the calls.
    pub(crate) fn finish(mut self) -> Result<(), String> {
        match self.calls.take().map(JoinHandle::join) {
            Some(Ok(made)) => made,
            Some(Err(_)) => {
                Err("the calls that kept the application's second full panicked".into())
            }
            None => Ok(()),
        }
    }
}

impl Drop for Flood {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(calls) = self.calls.take() {
            // What they came to was for `finish` to say.
            let _ = calls.join();
        }
    }
}

/// What a burst of calls came to.
#[derive(Clone, Debug)]
pub(super) struct Burst {
    pub calls: Vec<Called>,
    pub took: Duration,
}

/// The address and the token the inspector's token file names.
pub(crate) fn target(token_file: &Path) -> Result<(SocketAddr, String), String> {
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

/// One `ping`, answered or refused.
pub(crate) fn call(addr: SocketAddr, token: &str, id: usize) -> Result<Called, String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flood_let_go_of_early_has_ended_when_it_is_gone() {
        let held = Arc::new(());
        let by_call = Arc::clone(&held);
        let flood = Flood::start(Instant::now() + Duration::from_secs(20), move || {
            let _ = &by_call;
            Ok(())
        });
        let let_go = Instant::now();
        drop(flood);
        assert_eq!(
            Arc::strong_count(&held),
            1,
            "the calls go on after what started them let go of them"
        );
        assert!(
            let_go.elapsed() < Duration::from_secs(10),
            "letting go of the calls waited for their end"
        );
    }

    #[test]
    fn a_flood_waited_for_says_what_its_last_call_said() {
        let flood = Flood::start(Instant::now() + Duration::from_secs(20), || {
            Err("refused".into())
        });
        assert_eq!(flood.finish(), Err("refused".to_owned()));
    }
}
