//! The inspector in a scenario (ARCHITECTURE.md §19.4): the one a
//! development build has, asked by a real MCP client in this process, and
//! for the transport scenarios served on a loopback socket to a client
//! that knows nothing but HTTP.
//!
//! The scenario's thread is the application's UI thread. It runs for the
//! inspector only when the inspector's wake says so, as a host does: the
//! agent's call hands it the wake, and while a served client waits it
//! sleeps on a bell the wake rings.

use std::{
    fs,
    io::{Read as _, Write as _},
    net::{SocketAddr, TcpStream},
    os::unix::fs::PermissionsExt as _,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

use ljos_dev::mcp::{self, HttpServer, Inspector, InspectorConfig, InspectorSurface, Listen};
use ljos_mcp_core::{
    Cap, CapSet,
    glue::local::{LocalClient, LocalError, LocalWake},
};
use serde_json::{Value, json};

use super::{Live, LjosBackend, Result, failed, level, not_loaded};
use crate::backend::{
    BackendError, LogEntry,
    inspector::{Answer, Ask, ClientAnswer, Line, Refusal, Seen, Token, TokenFile, lines},
};

/// How long a client of the served inspector waits for its answer before
/// the scenario fails instead of hanging.
const CLIENT_PATIENCE: Duration = Duration::from_secs(20);

/// What the inspector's wake rings while a served client waits.
#[derive(Default)]
struct Bell {
    rung: Mutex<bool>,
    ring: Condvar,
}

impl Bell {
    fn ring(&self) {
        *self.rung.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.ring.notify_one();
    }

    /// Whether it was rung, which it then no longer is. Waits up to
    /// `patience` for it.
    fn take(&self, patience: Duration) -> bool {
        let rung = self.rung.lock().unwrap_or_else(|e| e.into_inner());
        let (mut rung, _) = self
            .ring
            .wait_timeout_while(rung, patience, |rung| !*rung)
            .unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut *rung)
    }
}

/// The served inspector. The server is dropped before the directory its
/// token file is in.
struct Served {
    server: HttpServer,
    _dir: tempfile::TempDir,
}

/// What a scenario with an inspector has beside the live engine.
pub(super) struct Rig {
    served: Option<Served>,
    agent: LocalClient,
    inspector: Inspector,
    surface: Arc<InspectorSurface>,
    bell: Arc<Bell>,
    seen: Seen,
    /// The items the agent has been listed, the latest first, for "the
    /// reference of": an agent keeps what it was told.
    listed: Vec<Line>,
    /// The number of the last snapshot and how to ask for its next part.
    epoch: Option<u64>,
    cursor: Option<String>,
    /// Where the log the agent has read ends.
    log_read: Option<u64>,
    /// The token of the start before this one.
    old_token: Option<String>,
}

fn no_inspector() -> BackendError {
    failed("no inspector is connected; start with `Given the inspector is connected`")
}

/// One pass of the UI thread that the inspector's wake asked for: the
/// inspector answers, then the frame a host runs after it.
fn pump(inspector: &mut Inspector, live: &mut Live, fault: &mut Option<BackendError>) {
    inspector.pump(&live.engine, &mut live.scene, &*live.clock);
    let started = Instant::now();
    if let Err(e) = live.settle() {
        fault.get_or_insert(e);
    }
    let mut report = live.scene.take_incubation_report().unwrap_or_default();
    report.elapsed = started.elapsed();
    inspector.record_frame(&report);
}

/// `7\r\nMozilla\r\n0\r\n\r\n` is `Mozilla`.
fn unchunk(mut rest: &str) -> String {
    let mut out = String::new();
    while let Some((size, after)) = rest.split_once("\r\n") {
        match usize::from_str_radix(size.trim(), 16) {
            Ok(n) if n > 0 && after.len() >= n => {
                out.push_str(&after[..n]);
                rest = after[n..].strip_prefix("\r\n").unwrap_or(&after[n..]);
            }
            _ => break,
        }
    }
    out
}

/// One HTTP request to the served inspector, as a program that is not an
/// MCP client library writes it.
fn exchange(
    addr: SocketAddr,
    headers: &[(&str, String)],
    body: &str,
) -> std::io::Result<ClientAnswer> {
    let mut stream = TcpStream::connect(addr)?;
    stream.set_read_timeout(Some(CLIENT_PATIENCE))?;
    let mut request = String::from("POST /mcp HTTP/1.1\r\nConnection: close\r\n");
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
    stream.write_all(request.as_bytes())?;
    let mut raw = String::new();
    stream.read_to_string(&mut raw)?;
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| std::io::Error::other(format!("no status line in {raw:?}")))?;
    let chunked = head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked");
    Ok(ClientAnswer {
        status,
        body: if chunked {
            unchunk(body)
        } else {
            body.to_owned()
        },
    })
}

impl Rig {
    fn connect(redact_all: bool) -> Result<Rig> {
        let local = LocalWake::new();
        let bell = Arc::new(Bell::default());
        let wake = {
            let (local, bell) = (local.hook(), bell.clone());
            Arc::new(move || {
                local();
                bell.ring();
            })
        };
        let cfg = InspectorConfig {
            redact_all,
            virtual_clock: true,
            ..InspectorConfig::default()
        };
        let (inspector, surface) = Inspector::new(cfg, wake);
        let mut agent = LocalClient::connect(surface.clone(), CapSet::of([Cap::Inspect]), &local)
            .map_err(failed)?;
        let seen = Seen {
            tools: agent
                .tools()
                .map_err(failed)?
                .into_iter()
                .map(|t| (t.name, t.read_only))
                .collect(),
            instructions: agent.instructions().unwrap_or_default(),
            ..Seen::default()
        };
        Ok(Rig {
            served: None,
            agent,
            inspector,
            surface,
            bell,
            seen,
            listed: Vec::new(),
            epoch: None,
            cursor: None,
            log_read: None,
            old_token: None,
        })
    }

    /// Makes the application's log the one the inspector reads, and keeps
    /// it going to the scenario's own log as before.
    pub(super) fn attach(&self, live: &Live) {
        let sink = live.log.clone();
        self.inspector.watch_log(&live.engine, move |e| {
            sink.borrow_mut().push(LogEntry {
                level: level(e.level),
                message: e.message.clone(),
            });
        });
    }

    fn serve(&mut self) -> Result<()> {
        // The token file of an application is below its own directory; a
        // scenario has none, so it gets one that is removed with it.
        let dir = tempfile::Builder::new().prefix("ljos-bdd-mcp-").tempdir()?;
        let token_file = dir.path().join("mcp.json");
        let server =
            mcp::serve(self.surface.clone(), Listen::Loopback(0), &token_file).map_err(failed)?;
        let written: Value = serde_json::from_str(&fs::read_to_string(&token_file)?)
            .map_err(|e| failed(format!("the token file is not JSON: {e}")))?;
        self.seen.token_file = Some(TokenFile {
            mode: fs::metadata(&token_file)?.permissions().mode(),
            url: written["url"].as_str().unwrap_or_default().to_owned(),
            loopback: server.addr().is_some_and(|a| a.ip().is_loopback()),
        });
        self.served = Some(Served { server, _dir: dir });
        Ok(())
    }

    fn keep(&mut self, tool: &str, answer: Answer) {
        if answer.refusal.is_none() {
            let v = &answer.structured;
            match tool {
                "app_snapshot" => {
                    self.epoch = v["epoch"].as_u64().or(self.epoch);
                    self.cursor = v["next_cursor"].as_str().map(str::to_owned);
                    // What changed is not a list of what there is.
                    if v.get("delta").is_none() {
                        self.listed.splice(0..0, lines(answer.outline()));
                    }
                }
                "app_find" => {
                    let found: Vec<Line> = v["matches"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|m| Line {
                            role: m["role"].as_str().unwrap_or_default().to_owned(),
                            name: m["name"].as_str().unwrap_or_default().to_owned(),
                            reference: m["ref"].as_str().map(str::to_owned),
                        })
                        .collect();
                    self.listed.splice(0..0, found);
                }
                "app_logs" => self.log_read = v["next_since"].as_u64().or(self.log_read),
                _ => {}
            }
        }
        self.seen.answer = Some(answer);
    }

    fn refusal(e: LocalError) -> Result<Answer> {
        match e {
            LocalError::Refused(e) => Ok(Answer {
                refusal: Some(Refusal {
                    code: e.code.as_str().to_owned(),
                    message: e.message,
                    details: e.details.unwrap_or(Value::Null),
                }),
                ..Answer::default()
            }),
            other => Err(failed(other)),
        }
    }

    fn call(&mut self, live: &mut Live, tool: &str, args: Value) -> Result<()> {
        let mut fault = None;
        let inspector = &mut self.inspector;
        let reply = self
            .agent
            .call(tool, args, &mut || pump(inspector, live, &mut fault));
        if let Some(e) = fault {
            return Err(e);
        }
        let answer = match reply {
            Ok(reply) if reply.is_error => Answer {
                refusal: Some(Refusal {
                    code: reply.error_code().unwrap_or_default().to_owned(),
                    message: reply.error_message().unwrap_or(&reply.text).to_owned(),
                    details: reply
                        .structured
                        .pointer("/error/details")
                        .cloned()
                        .unwrap_or(Value::Null),
                }),
                text: reply.text,
                ..Answer::default()
            },
            Ok(reply) => Answer {
                refusal: None,
                structured: reply.structured,
                text: reply.text,
                picture: reply.images.into_iter().next().map(|i| i.data),
            },
            Err(e) => Self::refusal(e)?,
        };
        self.keep(tool, answer);
        Ok(())
    }

    fn read(&mut self, live: &mut Live, uri: &str) -> Result<()> {
        let mut fault = None;
        let inspector = &mut self.inspector;
        let reply = self
            .agent
            .read_resource(uri, &mut || pump(inspector, live, &mut fault));
        if let Some(e) = fault {
            return Err(e);
        }
        let answer = match reply {
            Ok(text) => Answer {
                text,
                ..Answer::default()
            },
            Err(e) => Self::refusal(e)?,
        };
        self.seen.answer = Some(answer);
        Ok(())
    }

    fn client(
        &mut self,
        live: &mut Live,
        token: Token,
        origin: Option<&str>,
        host: Option<&str>,
    ) -> Result<()> {
        let not_served = || {
            failed(
                "the inspector is not served; use `the inspector is served on the loopback address`",
            )
        };
        if token == Token::OlderStart {
            // The application is started again: the inspector makes a new
            // token and the one a client kept from before opens nothing.
            let mut old = self.served.take().ok_or_else(not_served)?;
            self.old_token = Some(old.server.token().as_str().to_owned());
            old.server.stop();
            drop(old);
            self.serve()?;
        }
        let served = self.served.as_ref().ok_or_else(not_served)?;
        let addr = served
            .server
            .addr()
            .ok_or_else(|| failed("the served inspector has no address"))?;
        let shown = match token {
            Token::Right => Some(served.server.token().as_str().to_owned()),
            Token::Missing => None,
            Token::Wrong => Some("0".repeat(64)),
            Token::OlderStart => self.old_token.clone(),
        };
        let mut headers = vec![
            ("Host", host.map_or_else(|| addr.to_string(), str::to_owned)),
            ("Accept", "application/json, text/event-stream".to_owned()),
            ("Content-Type", "application/json".to_owned()),
        ];
        if let Some(token) = shown {
            headers.push(("Authorization", format!("Bearer {token}")));
        }
        if let Some(origin) = origin {
            headers.push(("Origin", origin.to_owned()));
        }
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": { "name": "app_snapshot", "arguments": {} },
        })
        .to_string();

        let mut fault = None;
        let (bell, inspector) = (self.bell.clone(), &mut self.inspector);
        // The wakes of what came before are not this client's.
        bell.take(Duration::ZERO);
        let answer = std::thread::scope(|threads| {
            let client = threads.spawn(|| exchange(addr, &headers, &body));
            while !client.is_finished() {
                if bell.take(Duration::from_millis(20)) {
                    pump(inspector, live, &mut fault);
                }
            }
            client.join()
        });
        if let Some(e) = fault {
            return Err(e);
        }
        let answer = answer.map_err(|_| failed("the client's thread panicked"))??;
        self.seen.client = Some(answer);
        Ok(())
    }

    /// Whether the application has come to rest: what a host would do is
    /// done (one more pass if the wake asked for one), and nothing asks
    /// for another.
    fn rests(&mut self, live: &mut Live) -> Result<()> {
        if self.bell.take(Duration::ZERO) {
            let mut fault = None;
            pump(&mut self.inspector, live, &mut fault);
            if let Some(e) = fault {
                return Err(e);
            }
        }
        let busy = [
            (
                self.bell.take(Duration::ZERO),
                "the inspector woke it again",
            ),
            (self.inspector.has_work(), "the inspector has work waiting"),
            (live.scene.needs_frame(), "the scene asks for a frame"),
            (live.wake.is_some(), "the scene asks to be woken later"),
        ];
        match busy.iter().find(|(is, _)| *is) {
            Some((_, why)) => Err(failed(format!("the application is not idle: {why}"))),
            None => Ok(()),
        }
    }

    fn ask(&mut self, live: &mut Live, ask: &Ask) -> Result<()> {
        match ask {
            Ask::Connect { .. } | Ask::Serve => Ok(()),
            Ask::Call { tool, args } => self.call(live, tool, args.clone()),
            Ask::NextPart => {
                let cursor = self.cursor.clone().ok_or_else(|| {
                    failed("the last snapshot was whole: there is no next part to ask for")
                })?;
                self.call(live, "app_snapshot", json!({ "cursor": cursor }))
            }
            Ask::ChangedSince => {
                let since = self
                    .epoch
                    .ok_or_else(|| failed("the agent has not requested a snapshot yet"))?;
                self.call(live, "app_snapshot", json!({ "since": since }))
            }
            Ask::InspectRef { role, name } => {
                let reference = self
                    .listed
                    .iter()
                    .find(|l| l.role == *role && l.name == *name)
                    .and_then(|l| l.reference.clone())
                    .ok_or_else(|| {
                        failed(format!(
                            "the agent was not given a reference for a {role} named {name:?}; \
                             it has been listed {:?}",
                            self.listed
                        ))
                    })?;
                self.call(live, "app_inspect", json!({ "target": reference }))
            }
            Ask::LogSince => {
                let since = self
                    .log_read
                    .ok_or_else(|| failed("the agent has not asked for the log yet"))?;
                self.call(live, "app_logs", json!({ "since": since }))
            }
            Ask::Resource(uri) => self.read(live, uri),
            Ask::Client {
                token,
                origin,
                host,
            } => self.client(live, *token, origin.as_deref(), host.as_deref()),
            Ask::Rests => self.rests(live),
            Ask::TreeGivenBack => {
                if live.scene.accessibility_active() {
                    Err(failed(
                        "the accessibility tree is still kept up to date, though nothing reads it",
                    ))
                } else {
                    Ok(())
                }
            }
        }
    }
}

impl LjosBackend {
    pub(super) fn ask_inspector(&mut self, ask: &Ask) -> Result<()> {
        if let Ask::Connect { .. } | Ask::Serve = ask {
            if self.inspector.is_none() || matches!(ask, Ask::Connect { .. }) {
                let redact_all = matches!(ask, Ask::Connect { redact_all: true });
                let rig = Rig::connect(redact_all)?;
                // An application loaded later is attached by its load.
                if let Some(live) = &self.live {
                    rig.attach(live);
                }
                self.inspector = Some(rig);
            }
            if *ask == Ask::Serve {
                self.inspector.as_mut().ok_or_else(no_inspector)?.serve()?;
            }
            return Ok(());
        }
        let rig = self.inspector.as_mut().ok_or_else(no_inspector)?;
        let live = self.live.as_mut().ok_or_else(not_loaded)?;
        rig.ask(live, ask)
    }

    pub(super) fn seen_by_inspector(&self) -> Result<Seen> {
        Ok(self
            .inspector
            .as_ref()
            .ok_or_else(no_inspector)?
            .seen
            .clone())
    }
}

#[cfg(test)]
mod tests {
    use super::unchunk;

    #[test]
    fn a_chunked_body_is_put_together() {
        assert_eq!(unchunk("4\r\nWiki\r\n5\r\npedia\r\n0\r\n\r\n"), "Wikipedia");
        assert_eq!(unchunk(""), "");
    }
}
