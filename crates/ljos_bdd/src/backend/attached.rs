//! The inspector of a running application, reached through `ljos attach`
//! (ARCHITECTURE.md §19.4): what the `@desktop` and `@android` backends ask
//! the application, and how they wait for it to rest. The application is a
//! process of this computer or of a phone; either says how it is when the
//! way in to it ends.

use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead as _, BufReader, Write as _};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ljos_child::Reaped;
use serde_json::{Value, json};

use super::inspector::{Answer, Refusal};
use super::{BackendError, Result};

/// The longest the application may take to see input, or to come to rest
/// before what one reading of it costs is measured.
pub(super) const SEEN: Duration = Duration::from_secs(10);
/// How long a watch gathers the rests of a busy application: at most ten
/// watches a second, half of what the inspector answers.
const BURST_MS: u64 = 100;
/// The most calls sent in any second: the inspector answers twenty and
/// refuses the rest for a second; two spare cover the time to deliver one.
const CALLS_PER_SECOND: usize = 18;
/// How much of a child's stderr is kept to say why it ended.
const STDERR_TAIL: usize = 8 * 1024;

/// The process an inspector is served from, as far as the way in to it
/// can tell.
pub(super) trait Reached: std::fmt::Debug {
    /// How it is: running, or how it ended.
    fn state(&mut self) -> String;
    /// Ends it, when it is the backend's to end.
    fn end(&mut self);
}

impl Reached for Reaped {
    fn state(&mut self) -> String {
        match self.try_wait() {
            Ok(Some(status)) => format!("ended ({status})"),
            Ok(None) => "still running".to_owned(),
            Err(e) => format!("not known ({e})"),
        }
    }

    fn end(&mut self) {
        let _ = self.stop();
    }
}

/// The running application and the ways in to it.
#[derive(Debug)]
pub(super) struct Running<P: Reached = Reaped> {
    pub(super) app: P,
    attach: Reaped,
    /// The last of what `ljos attach` wrote to its stderr, kept so that
    /// its end can say why.
    attach_stderr: Arc<Mutex<Vec<u8>>>,
    to_app: ChildStdin,
    from_app: BufReader<ChildStdout>,
    next_id: u64,
    /// When the calls of the last second were sent.
    sent: VecDeque<Instant>,
    /// Where its window's content is, in the layout's pixels.
    pub(super) rect: (i32, i32, i32, i32),
}

impl<P: Reached> Running<P> {
    /// Talks to the inspector of `app` through `ljos attach`.
    pub(super) fn attach(
        ljos: &Path,
        token: &Path,
        app: P,
        rect: (i32, i32, i32, i32),
    ) -> Result<Self> {
        let mut attach = Reaped::spawn(
            Command::new(ljos)
                .args(["attach".as_ref(), token.as_os_str()])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped()),
        )?;
        let attach_stderr = Arc::new(Mutex::new(Vec::new()));
        // Drained as it comes, so the pipe never fills, and bounded.
        if let Some(mut stderr) = attach.stderr.take() {
            let tail = Arc::clone(&attach_stderr);
            std::thread::spawn(move || {
                let mut chunk = [0u8; 1024];
                while let Ok(n) = std::io::Read::read(&mut stderr, &mut chunk) {
                    if n == 0 {
                        break;
                    }
                    let mut tail = tail.lock().unwrap_or_else(|e| e.into_inner());
                    tail.extend_from_slice(&chunk[..n]);
                    let over = tail.len().saturating_sub(STDERR_TAIL);
                    tail.drain(..over);
                }
            });
        }
        let to_app = attach.stdin.take().expect("piped");
        let from_app = BufReader::new(attach.stdout.take().expect("piped"));
        Ok(Running {
            app,
            attach,
            attach_stderr,
            to_app,
            from_app,
            next_id: 0,
            sent: VecDeque::new(),
            rect,
        })
    }

    /// What the process has done so far, by kind: its window's passes and
    /// frames, its jobs and messages (`app_metrics`), and what was sent to
    /// each of its mailboxes (`app_processes`).
    pub(super) fn work(&mut self) -> Result<BTreeMap<String, u64>> {
        let metrics = self.ask("app_metrics", json!({}))?;
        let n = |v: &Value| v.as_u64().unwrap_or(0);
        let paint = &metrics["paint"];
        let mut work = BTreeMap::from([
            ("frames".to_owned(), n(&metrics["frames"]["count"])),
            (
                "passes".to_owned(),
                n(&paint["presented"]) + n(&paint["unchanged"]) + n(&paint["lost"]),
            ),
            ("jobs".to_owned(), n(&metrics["jobs"]["polled"])),
            ("messages".to_owned(), n(&metrics["messages"]["drained"])),
        ]);
        let processes = self.ask("app_processes", json!({"kind": "mailbox"}))?;
        for mailbox in processes["mailboxes"].as_array().into_iter().flatten() {
            let name = mailbox["name"].as_str().unwrap_or("?");
            work.insert(format!("sent to {name}"), n(&mailbox["sent"]));
        }
        Ok(work)
    }

    pub(super) fn ask(&mut self, tool: &str, arguments: Value) -> Result<Value> {
        let answer = self.call(tool, arguments)?;
        match answer.refusal {
            Some(r) => Err(BackendError::Failed(format!("{tool}: {}", r.message))),
            None => Ok(answer.structured),
        }
    }

    /// Waits until a call more keeps to the inspector's calls per second.
    /// Steps that no longer wait for a while send them faster than that.
    pub(super) fn pace(&mut self) {
        let second = Duration::from_secs(1);
        if self.sent.len() >= CALLS_PER_SECOND
            && let Some(first) = self.sent.pop_front()
        {
            std::thread::sleep((first + second).saturating_duration_since(Instant::now()));
        }
        let now = Instant::now();
        while self
            .sent
            .front()
            .is_some_and(|t| now.duration_since(*t) >= second)
        {
            self.sent.pop_front();
        }
        self.sent.push_back(now);
    }

    /// A tool's answer as an agent sees it, a refusal among them.
    pub(super) fn call(&mut self, tool: &str, arguments: Value) -> Result<Answer> {
        self.pace();
        self.next_id += 1;
        let id = self.next_id;
        let message = json!({
            "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": {"name": tool, "arguments": arguments},
        });
        writeln!(self.to_app, "{message}")?;
        self.to_app.flush()?;
        let mut line = String::new();
        loop {
            line.clear();
            if self.from_app.read_line(&mut line)? == 0 {
                return Err(BackendError::Failed(self.went_away()));
            }
            let answer: Value = serde_json::from_str(&line)
                .map_err(|e| BackendError::Failed(format!("the inspector said {line:?}: {e}")))?;
            if answer["id"] != id {
                continue;
            }
            if let Some(error) = answer.get("error") {
                return Err(BackendError::Failed(format!(
                    "{tool}: {}",
                    error["message"]
                )));
            }
            let result = &answer["result"];
            let text = result["content"][0]["text"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            if result["isError"] == true {
                let error = &result["structuredContent"]["error"];
                return Ok(Answer {
                    refusal: Some(Refusal {
                        code: error["code"].as_str().unwrap_or_default().to_owned(),
                        message: error["message"].as_str().unwrap_or(&text).to_owned(),
                        details: error["details"].clone(),
                    }),
                    text,
                    ..Answer::default()
                });
            }
            let picture = result["content"].as_array().and_then(|content| {
                content
                    .iter()
                    .find(|c| c["type"] == "image")
                    .and_then(|c| ljos_mcp_core::base64_decode(c["data"].as_str()?))
            });
            return Ok(Answer {
                structured: result["structuredContent"].clone(),
                text,
                picture,
                ..Answer::default()
            });
        }
    }

    /// Why the way in ended: how `ljos attach` and the process behind it
    /// ended, and what attach said last.
    pub(super) fn went_away(&mut self) -> String {
        let mut said = String::from("the application's inspector went away");
        let ended = |child: &mut Child| match child.try_wait() {
            Ok(Some(status)) => format!("ended ({status})"),
            Ok(None) => "still running".to_owned(),
            Err(e) => format!("not known ({e})"),
        };
        // Attach is given a moment to be reaped: its pipe closed first.
        let until = Instant::now() + Duration::from_secs(1);
        while matches!(self.attach.try_wait(), Ok(None)) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
        let attach = ended(&mut self.attach);
        said.push_str(&format!(
            ": `ljos attach` {attach}; the process it reaches {}",
            self.app.state()
        ));
        let tail = self.attach_stderr.lock().unwrap_or_else(|e| e.into_inner());
        let tail = String::from_utf8_lossy(&tail);
        if tail.trim().is_empty() {
            said.push_str("; attach said nothing");
        } else {
            said.push_str(&format!("; attach said:\n{}", tail.trim_end()));
        }
        said
    }

    /// How many frames the UI thread has run and how many times it has
    /// rested, to wait from with [`Running::turned_after`].
    pub(super) fn turns(&mut self) -> Result<(u64, u64)> {
        let told = self.ask("app_watch", json!({"topics": ["frames", "rests"]}))?;
        let count = |k: &str| told["counts"][k].as_u64().unwrap_or(0);
        Ok((count("frames"), count("rests")))
    }

    /// Waits until the UI thread has run a frame or rested anew since
    /// `turns`, or until `deadline`: whether it did. A rest is counted
    /// only when it says something new, and a window told of the focus
    /// can go back to the same rest it had, so the frame it ran for it
    /// counts too.
    pub(super) fn turned_after(
        &mut self,
        (frames, rests): (u64, u64),
        deadline: Instant,
    ) -> Result<bool> {
        let left = deadline.saturating_duration_since(Instant::now());
        let wait_ms = u64::try_from(left.as_millis()).unwrap_or(u64::MAX).max(1);
        let told = self.ask(
            "app_watch",
            json!({
                "since": {"frames": frames, "rests": rests},
                "topics": ["frames", "rests"], "wait_ms": wait_ms,
            }),
        )?;
        let past = |k: &str, was: u64| told["counts"][k].as_u64().is_some_and(|n| n > was);
        Ok(past("frames", frames) || past("rests", rests))
    }

    /// The UI thread's last rest (`app_watch`'s `rest`), and the count of
    /// rests to wait from.
    pub(super) fn rest(&mut self) -> Result<(u64, Value)> {
        let told = self.ask("app_watch", json!({"topics": ["rests"]}))?;
        Ok((
            told["counts"]["rests"].as_u64().unwrap_or(0),
            told["rest"].clone(),
        ))
    }

    /// Waits until the UI thread rests as `done` wants, or until
    /// `deadline`: whether it did. The watch waits on the inspector's
    /// thread, so the application runs no pass for it.
    pub(super) fn rest_until(
        &mut self,
        deadline: Instant,
        done: impl Fn(&Value) -> bool,
    ) -> Result<bool> {
        let (mut count, mut rest) = self.rest()?;
        let mut settle_ms = 0;
        loop {
            if done(&rest) {
                return Ok(true);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(false);
            }
            let wait_ms = u64::try_from(left.as_millis()).unwrap_or(u64::MAX).max(1);
            let told = self.ask(
                "app_watch",
                json!({
                    "since": {"rests": count}, "topics": ["rests"],
                    "wait_ms": wait_ms, "settle_ms": settle_ms,
                }),
            )?;
            // An animating application rests every frame. Once a rest has
            // not been the one wanted, the rests of a burst are gathered so
            // the watches stay under the inspector's calls per second.
            settle_ms = BURST_MS;
            count = told["counts"]["rests"].as_u64().unwrap_or(count);
            rest = told["rest"].clone();
        }
    }

    /// Waits until no pass of the UI thread is due: what was asked of it
    /// has run, and what that asked for after.
    pub(super) fn settle(&mut self) -> Result<()> {
        self.rest_until(Instant::now() + SEEN, |rest| {
            rest["wake_in_ms"].as_u64() != Some(0)
        })?;
        // An application that keeps animating never rests; what it costs
        // is then measured as it goes.
        Ok(())
    }

    /// Waits until the application rests with its window's last frame
    /// presented: no pass due, and none owed to the window. Under load
    /// the window system lets a window draw late, and the thread rests
    /// meanwhile with only that stall's wake due, so a rest alone can
    /// come before the frame (`features/window/cpu-drawing.feature`).
    pub(super) fn presented(&mut self, deadline: Instant) -> Result<()> {
        self.rest_until(deadline, |rest| {
            rest["wake_in_ms"].as_u64() != Some(0) && rest["frame_owed"] == Value::Bool(false)
        })?;
        // A window that keeps drawing owes a frame at every rest; the
        // compositor's picture is then never still, which `grab` says.
        Ok(())
    }

    pub(super) fn eval(&mut self, object: &str, source: &str) -> Result<Value> {
        let answer = self.ask(
            "app_eval",
            json!({"source": source, "target": format!("#{object}")}),
        )?;
        Ok(answer["value"].clone())
    }
}

impl<P: Reached> Drop for Running<P> {
    fn drop(&mut self) {
        // Only the two processes this backend started.
        // The way in before what it leads to.
        let _ = self.attach.stop();
        self.app.end();
    }
}
