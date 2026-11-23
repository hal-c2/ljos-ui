//! Calls made to a started application's inspector as fast as one client
//! makes them, written as plain HTTP so that a refusal is seen as it is
//! and not asked again, as an MCP client would (ARCHITECTURE.md §19.7).
//! The desktop backend also answers in an application's place with a
//! refusal, to see what its observer does when it is turned away.

use std::{
    io::{BufRead as _, BufReader, Read as _, Write as _},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex, PoisonError,
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
    pub body: String,
}

impl Called {
    /// What a caller reads of a refusal: the status, when to ask again,
    /// what the body is and what it says.
    fn refusal(&self) -> Result<Value, String> {
        let header = |name: &str| {
            self.head.lines().find_map(|line| {
                let (n, value) = line.split_once(':')?;
                (n == name).then(|| value.trim().to_owned())
            })
        };
        let body: Value = serde_json::from_str(&self.body)
            .map_err(|e| format!("a refusal's body is not JSON: {e}: {:?}", self.body))?;
        Ok(serde_json::json!({
            "status": self.status,
            "retry-after": header("retry-after"),
            "content-type": header("content-type"),
            "body": body,
        }))
    }
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
///
/// Started with [`Refusals::start_late`], it keeps one call's second try
/// unanswered until [`Refusals::pass_on`]: that answer comes after whatever
/// the test saw first, however fast or slow either side is.
#[derive(Debug)]
pub(crate) struct Refusals {
    token_file: PathBuf,
    /// The file as the application wrote it, while calls are held.
    kept: Option<String>,
    /// The application's own inspector.
    app: Option<SocketAddr>,
    addr: SocketAddr,
    late: Arc<Late>,
    stop: Arc<AtomicBool>,
    answering: Option<JoinHandle<()>>,
}

impl Refusals {
    pub(crate) fn start(token_file: &Path) -> Result<Self, String> {
        Self::listen(token_file, Late::default(), None)
    }

    /// As [`Refusals::start`], but the first call of `tool` is kept when
    /// it is made again.
    pub(crate) fn start_late(token_file: &Path, tool: &'static str) -> Result<Self, String> {
        let late = Late {
            tool: Some(tool),
            ..Late::default()
        };
        let (app, _) = target(token_file)?;
        Self::listen(token_file, late, Some(app))
    }

    fn listen(token_file: &Path, late: Late, app: Option<SocketAddr>) -> Result<Self, String> {
        let said = |e: &dyn std::fmt::Display| format!("{}: {e}", token_file.display());
        let kept = std::fs::read_to_string(token_file).map_err(|e| said(&e))?;
        let mut body: Value = serde_json::from_str(&kept).map_err(|e| said(&e))?;
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| said(&e))?;
        let addr = listener.local_addr().map_err(|e| said(&e))?;
        body["url"] = format!("http://{addr}/mcp").into();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        let late = Arc::new(late);
        let taking = Arc::clone(&late);
        let answering = std::thread::spawn(move || {
            for stream in listener.incoming() {
                if stopped.load(Ordering::Relaxed) {
                    break;
                }
                if let Ok(stream) = stream {
                    // A caller that left has nothing to be told.
                    let _ = taking.take(stream);
                }
            }
        });
        let held = Self {
            token_file: token_file.to_owned(),
            kept: Some(kept),
            app,
            addr,
            late,
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

    /// Waits until the call that is to be late was turned away.
    pub(crate) fn refused(&self, by: Instant) -> Result<(), String> {
        self.late
            .until(by, |call| call.refused.is_some())
            .ok_or_else(|| "the call to be answered late was never made".to_owned())
    }

    /// Waits until that call was made again and is kept.
    pub(crate) fn came_again(&self, by: Instant) -> Result<(), String> {
        self.late
            .until(by, |call| call.again.is_some())
            .ok_or_else(|| "the call turned away was not made again".to_owned())
    }

    /// Gives the kept call to the application and its caller the answer,
    /// which is to be the application's and no refusal or failure.
    pub(crate) fn pass_on(&self, by: Instant) -> Result<(), String> {
        let said = |e: &dyn std::fmt::Display| format!("passing the late call on: {e}");
        self.came_again(by)?;
        let app = self.app.ok_or("no call is kept to be late")?;
        let (request, mut caller) = self
            .late
            .call()
            .again
            .take()
            .ok_or("the late call was passed on already")?;
        let mut head = format!(
            "{}\r\nHost: {app}\r\nConnection: close\r\n",
            request.head.lines().next().unwrap_or_default()
        );
        for line in request.head.lines().skip(1) {
            let name = line.split(':').next().unwrap_or_default();
            if !name.eq_ignore_ascii_case("host") && !name.eq_ignore_ascii_case("connection") {
                head += line;
                head += "\r\n";
            }
        }
        let mut stream = TcpStream::connect(app).map_err(|e| said(&e))?;
        let left = by.saturating_duration_since(Instant::now());
        stream
            .set_read_timeout(Some(left.max(Duration::from_millis(1))))
            .map_err(|e| said(&e))?;
        stream
            .write_all(&[head.as_bytes(), b"\r\n", &request.body].concat())
            .map_err(|e| said(&e))?;
        let mut answer = Vec::new();
        stream.read_to_end(&mut answer).map_err(|e| said(&e))?;
        let text = String::from_utf8_lossy(&answer);
        if text.split_whitespace().nth(1) != Some("200")
            || !text.contains(r#""result":"#)
            || text.contains(r#""isError":true"#)
        {
            return Err(said(&format!("the application answered {text}")));
        }
        caller.write_all(&answer).map_err(|e| said(&e))
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

/// What a listener of [`Refusals`] keeps of the call that is to be late.
#[derive(Debug, Default)]
struct Late {
    /// The tool whose first call that is; none when every call is refused.
    tool: Option<&'static str>,
    call: Mutex<LateCall>,
    came: Condvar,
}

#[derive(Debug, Default)]
struct LateCall {
    /// Its body when it was turned away. A wire asks again with the same.
    refused: Option<Vec<u8>>,
    /// Its second try, unanswered.
    again: Option<(Request, TcpStream)>,
}

impl Late {
    fn call(&self) -> std::sync::MutexGuard<'_, LateCall> {
        self.call.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn until(&self, by: Instant, seen: impl Fn(&LateCall) -> bool) -> Option<()> {
        let left = by.saturating_duration_since(Instant::now());
        let (call, _) = self
            .came
            .wait_timeout_while(self.call(), left, |call| !seen(call))
            .unwrap_or_else(PoisonError::into_inner);
        seen(&call).then_some(())
    }

    /// Answers a call with the refusal, or keeps it.
    fn take(&self, mut stream: TcpStream) -> std::io::Result<()> {
        stream.set_read_timeout(Some(WINDOW))?;
        let request = read_request(&mut stream)?;
        let Some(tool) = self.tool else {
            return refuse(stream);
        };
        let mut call = self.call();
        match &call.refused {
            Some(first) if *first == request.body && call.again.is_none() => {
                call.again = Some((request, stream));
                self.came.notify_all();
                Ok(())
            }
            None if serde_json::from_slice::<Value>(&request.body)
                .is_ok_and(|body| body["params"]["name"] == tool) =>
            {
                // Known before the caller can have made it again.
                call.refused = Some(request.body);
                self.came.notify_all();
                drop(call);
                refuse(stream)
            }
            _ => {
                drop(call);
                refuse(stream)
            }
        }
    }
}

#[derive(Debug)]
struct Request {
    /// Its first line and header lines.
    head: String,
    body: Vec<u8>,
}

/// Reads a whole call: closed on what is unread, the connection is reset
/// and the answer lost.
fn read_request(stream: &mut TcpStream) -> std::io::Result<Request> {
    let mut reader = BufReader::new(stream);
    let (mut head, mut length) = (String::new(), 0);
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap_or(0);
        }
        head += &line;
    }
    let mut body = Vec::new();
    reader.take(length).read_to_end(&mut body)?;
    Ok(Request { head, body })
}

/// The inspector's answer to a call past its rate (ARCHITECTURE.md §19.7).
fn refuse(mut stream: TcpStream) -> std::io::Result<()> {
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
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((&*raw, ""));
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("call {id} had no status line: {raw:?}"))?;
    Ok(Called {
        status,
        head: head.to_ascii_lowercase(),
        body: body.to_owned(),
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

    /// Whether call `nth`, refused by the application's inspector, reads
    /// as a call [`Refusals`] refuses. What the observer's scenarios show
    /// of a refusal holds of the inspector's only while it does.
    pub(super) fn refused_as_by_listener(&self, nth: usize) -> Result<(), String> {
        let real = nth
            .checked_sub(1)
            .and_then(|at| self.calls.get(at))
            .ok_or_else(|| format!("only {} calls were made, not {nth}", self.calls.len()))?
            .refusal()?;
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let file = dir.path().join("mcp-1.json");
        std::fs::write(&file, r#"{"url":"http://127.0.0.1:9/mcp","token":"t"}"#)
            .map_err(|e| e.to_string())?;
        let held = Refusals::start(&file)?;
        let (addr, token) = target(&file)?;
        let ours = call(addr, &token, nth)?.refusal()?;
        drop(held);
        if ours == real {
            return Ok(());
        }
        Err(format!(
            "the inspector refused call {nth} with {real}, the scenarios' listener refuses \
             with {ours}: the observer's scenarios test a refusal the inspector does not give"
        ))
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
    fn a_call_turned_away_is_answered_by_the_application_only_when_it_is_passed_on() {
        fn posted(addr: SocketAddr, body: &str) -> TcpStream {
            let mut stream = TcpStream::connect(addr).unwrap();
            stream.set_read_timeout(Some(super::super::ANSWER)).unwrap();
            write!(
                stream,
                "POST /mcp HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer t\r\n\
                 Content-Length: {}\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            stream
        }
        fn answered(mut stream: TcpStream) -> String {
            let mut text = String::new();
            stream.read_to_string(&mut text).unwrap();
            text
        }
        let app = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let real = app.local_addr().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("mcp-1.json");
        let written = format!(r#"{{"url":"http://{real}/mcp","token":"t","pid":1}}"#);
        std::fs::write(&file, &written).unwrap();
        let (asked, other) = (
            r#"{"id":1,"params":{"name":"app_inspect"}}"#,
            r#"{"id":2,"params":{"name":"app_objects"}}"#,
        );

        let mut held = Refusals::start_late(&file, "app_inspect").unwrap();
        let (addr, _) = target(&file).unwrap();
        assert!(answered(posted(addr, other)).starts_with("HTTP/1.1 429"));
        assert!(answered(posted(addr, asked)).starts_with("HTTP/1.1 429"));
        let by = Instant::now() + super::super::ANSWER;
        held.refused(by).unwrap();
        held.release().unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), written);

        let mut again = posted(addr, asked);
        held.came_again(by).unwrap();
        // Only the call that is waited for is kept.
        assert!(answered(posted(addr, other)).starts_with("HTTP/1.1 429"));
        app.set_nonblocking(true).unwrap();
        assert!(app.accept().is_err(), "asked before it was passed on");
        again.set_nonblocking(true).unwrap();
        assert!(
            again.read(&mut [0]).is_err(),
            "answered before it was passed on"
        );
        again.set_nonblocking(false).unwrap();

        app.set_nonblocking(false).unwrap();
        let application = std::thread::spawn(move || {
            let (mut stream, _) = app.accept().unwrap();
            let request = read_request(&mut stream).unwrap();
            let body = r#"{"jsonrpc":"2.0","id":1,"result":{}}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            request
        });
        held.pass_on(by).unwrap();
        assert!(answered(again).ends_with(concat!(
            "\r\n\r\n",
            r#"{"jsonrpc":"2.0","id":1,"result":{}}"#
        )));
        let request = application.join().unwrap();
        assert_eq!(request.body, asked.as_bytes());
        assert!(request.head.contains(&format!("Host: {real}")));
        assert!(request.head.contains("Authorization: Bearer t"));
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
