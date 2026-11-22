//! Calls made to a started application's inspector as fast as one client
//! makes them, written as plain HTTP so that a refusal is seen as it is
//! and not asked again, as an MCP client would (ARCHITECTURE.md §19.7).
//! The desktop backend fills an application's second with them too, to
//! see what its observer does when it is turned away.

use std::{
    io::{BufRead as _, BufReader, Read as _, Write as _},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
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

/// Every call to an application's inspector turned away, for as long as
/// they are held: its token file names a listener of the test's own, which
/// answers what the inspector answers when its second is full. Nothing
/// frees a place there, so nothing races a call for one. What reads the
/// file for each call, as `ljos observe` and `ljos attach` do, is refused
/// from the first call after [`Refusals::start`] returns.
///
/// The listener outlives [`Refusals::release`]: a wire asks again where it
/// was turned away, and is then turned away there rather than finding no
/// inspector. It ends when this is let go of.
#[derive(Debug)]
pub(crate) struct Refusals {
    token_file: PathBuf,
    /// The file as the application wrote it, while calls are held.
    kept: Option<String>,
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    answering: Option<JoinHandle<()>>,
}

impl Refusals {
    pub(crate) fn start(token_file: &Path) -> Result<Self, String> {
        let said = |e: &dyn std::fmt::Display| format!("{}: {e}", token_file.display());
        let kept = std::fs::read_to_string(token_file).map_err(|e| said(&e))?;
        let mut body: Value = serde_json::from_str(&kept).map_err(|e| said(&e))?;
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| said(&e))?;
        let addr = listener.local_addr().map_err(|e| said(&e))?;
        body["url"] = format!("http://{addr}/mcp").into();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        let answering = std::thread::spawn(move || {
            for stream in listener.incoming() {
                if stopped.load(Ordering::Relaxed) {
                    break;
                }
                if let Ok(stream) = stream {
                    // A caller that left has nothing to be told.
                    let _ = refuse(stream);
                }
            }
        });
        let held = Self {
            token_file: token_file.to_owned(),
            kept: Some(kept),
            addr,
            stop,
            answering: Some(answering),
        };
        replace(token_file, &body.to_string())?;
        Ok(held)
    }

    /// Gives the calls that follow the application's inspector again.
    pub(crate) fn release(&mut self) -> Result<(), String> {
        if let Some(kept) = &self.kept {
            replace(&self.token_file, kept)?;
            self.kept = None;
        }
        Ok(())
    }
}

impl Drop for Refusals {
    fn drop(&mut self) {
        // The file is gone with a scenario's directory that went first.
        let _ = self.release();
        self.stop.store(true, Ordering::Relaxed);
        // A connection is what wakes a listener that waits for one.
        if TcpStream::connect(self.addr).is_ok()
            && let Some(answering) = self.answering.take()
        {
            let _ = answering.join();
        }
    }
}

/// The inspector's answer to a call past its rate (ARCHITECTURE.md §19.7).
fn refuse(mut stream: TcpStream) -> std::io::Result<()> {
    stream.set_read_timeout(Some(WINDOW))?;
    let mut request = BufReader::new(stream.try_clone()?);
    let mut length = 0;
    loop {
        let mut line = String::new();
        if request.read_line(&mut line)? == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    // Closed on what is unread, the connection is reset and the answer lost.
    std::io::copy(&mut request.take(length), &mut std::io::sink())?;
    let body = serde_json::json!({ "error": {
        "code": "rate_limited",
        "message": "too many calls in one second; wait a moment and ask again",
    } })
    .to_string();
    write!(
        stream,
        "HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\n\
         Retry-After: 1\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
}

/// Replaces the file in one step, its user's alone as the inspector's is:
/// a caller reads the one text or the other.
fn replace(path: &Path, text: &str) -> Result<(), String> {
    let said = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let dir = path.parent().ok_or_else(|| said(&"no directory"))?;
    let mut file = tempfile::NamedTempFile::new_in(dir).map_err(|e| said(&e))?;
    file.write_all(text.as_bytes()).map_err(|e| said(&e))?;
    file.persist(path).map_err(|e| said(&e))?;
    Ok(())
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
    fn held_calls_are_turned_away_until_released_and_the_old_address_after() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("mcp-1.json");
        let written = r#"{"url":"http://127.0.0.1:9/mcp","token":"t","pid":1}"#;
        std::fs::write(&file, written).unwrap();

        let mut held = Refusals::start(&file).unwrap();
        let (addr, token) = target(&file).unwrap();
        assert_eq!((addr, token.as_str()), (held.addr, "t"));
        for id in 0..3 {
            let called = call(addr, &token, id).unwrap();
            assert_eq!(called.status, 429);
            assert!(called.head.contains("retry-after: 1"), "{}", called.head);
        }

        held.release().unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), written);
        assert_eq!(call(addr, &token, 3).unwrap().status, 429);
    }

    #[test]
    fn held_calls_let_go_of_early_are_given_back() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("mcp-1.json");
        let written = r#"{"url":"http://127.0.0.1:9/mcp","token":"t","pid":1}"#;
        std::fs::write(&file, written).unwrap();

        let held = Refusals::start(&file).unwrap();
        let stop = Arc::clone(&held.stop);
        drop(held);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), written);
        assert_eq!(
            Arc::strong_count(&stop),
            1,
            "the listener goes on after what started it let go of it"
        );
    }
}
