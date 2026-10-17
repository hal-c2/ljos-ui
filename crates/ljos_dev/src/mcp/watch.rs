//! `app_watch`: what changed in the application, told when it changes.
//!
//! The transport has no channel the server may speak on by itself, so a
//! client that wants to follow the application asks and is answered when
//! there is something to tell. The inspector counts what the host already
//! tells it (a frame run, a pass drawn, a line logged, a reload) in one
//! `watch` cell, and a waiting call sleeps on that cell on the server's
//! thread. The UI thread is never woken for a watch: an application at
//! rest runs nothing, and a client waiting on it costs one sleeping task.

use std::fmt::Write as _;
use std::sync::Arc;
use std::time::Duration;

use ljos_mcp_core::ToolReply;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::watch;

pub const WATCH: &str = "\
Wait until the app changes: frames run, passes drawn (`paints`), log lines, reloads. Give the \
`counts` of the last answer as `since`; it answers when a `topics` count differs, else after \
`wait_ms`. `settle_ms` gathers a burst into one answer. Wakes nothing in the app.";

/// The longest a watch waits.
pub const MAX_WAIT_MS: u64 = 30_000;
/// The longest a watch gathers a burst.
pub const MAX_SETTLE_MS: u64 = 5_000;
const DEFAULT_WAIT_MS: u64 = 10_000;

/// What a watch can wait for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Topic {
    /// Frames the application ran, including ones an inspector's request
    /// asked for.
    Frames,
    /// Passes of the primary window that drew something: the picture
    /// changed.
    Paints,
    /// Lines in the log `app_logs` reads.
    Logs,
    /// Reloads, applied or failed.
    Reloads,
}

impl Topic {
    const ALL: [Topic; 4] = [Topic::Frames, Topic::Paints, Topic::Logs, Topic::Reloads];

    fn name(self) -> &'static str {
        match self {
            Topic::Frames => "frames",
            Topic::Paints => "paints",
            Topic::Logs => "logs",
            Topic::Reloads => "reloads",
        }
    }
}

/// The counts a client was last told; a topic left out is not compared.
#[derive(Clone, Copy, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Since {
    #[serde(default)]
    pub frames: Option<u64>,
    #[serde(default)]
    pub paints: Option<u64>,
    #[serde(default)]
    pub logs: Option<u64>,
    #[serde(default)]
    pub reloads: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WatchArgs {
    /// The `counts` of the last answer. Without them the counts are
    /// answered at once.
    #[serde(default)]
    pub since: Option<Since>,
    /// Wait for these only: `frames`, `paints`, `logs`, `reloads`. All by
    /// default.
    #[serde(default)]
    pub topics: Vec<Topic>,
    /// How long to wait at most, in milliseconds (default 10000, at most
    /// 30000).
    #[serde(default)]
    pub wait_ms: Option<u64>,
    /// After the first change, wait this much longer so that a burst is
    /// one answer (at most 5000).
    #[serde(default)]
    pub settle_ms: Option<u64>,
}

/// How many of each thing happened since the inspector attached.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    pub(crate) frames: u64,
    pub(crate) paints: u64,
    pub(crate) logs: u64,
    pub(crate) reloads: u64,
}

impl Counts {
    fn of(&self, topic: Topic) -> u64 {
        match topic {
            Topic::Frames => self.frames,
            Topic::Paints => self.paints,
            Topic::Logs => self.logs,
            Topic::Reloads => self.reloads,
        }
    }
}

impl Since {
    fn of(&self, topic: Topic) -> Option<u64> {
        match topic {
            Topic::Frames => self.frames,
            Topic::Paints => self.paints,
            Topic::Logs => self.logs,
            Topic::Reloads => self.reloads,
        }
    }
}

/// Written on the UI thread, read by the server's. A count moves with
/// something the host does anyway, so keeping it costs a store per frame.
#[derive(Debug)]
pub(crate) struct ChangeFeed {
    tx: watch::Sender<Counts>,
}

impl ChangeFeed {
    pub(crate) fn new() -> Arc<ChangeFeed> {
        let (tx, _) = watch::channel(Counts::default());
        Arc::new(ChangeFeed { tx })
    }

    pub(crate) fn frame(&self) {
        self.tx.send_modify(|c| c.frames += 1);
    }

    pub(crate) fn paint(&self) {
        self.tx.send_modify(|c| c.paints += 1);
    }

    pub(crate) fn log(&self) {
        self.tx.send_modify(|c| c.logs += 1);
    }

    pub(crate) fn reload(&self) {
        self.tx.send_modify(|c| c.reloads += 1);
    }

    #[cfg(test)]
    fn counts(&self) -> Counts {
        *self.tx.borrow()
    }
}

/// The topics of `wanted` whose count is not the one in `since`.
fn changed(now: &Counts, since: &Since, wanted: &[Topic]) -> Vec<Topic> {
    wanted
        .iter()
        .copied()
        .filter(|t| since.of(*t).is_some_and(|was| was != now.of(*t)))
        .collect()
}

/// Answers `app_watch`, on the server's thread.
pub(crate) async fn watch(feed: &ChangeFeed, args: &WatchArgs) -> ToolReply {
    let wanted: Vec<Topic> = if args.topics.is_empty() {
        Topic::ALL.to_vec()
    } else {
        args.topics.clone()
    };
    let mut rx = feed.tx.subscribe();
    let mut waited = false;
    if let Some(since) = &args.since {
        let differs = |c: &Counts| !changed(c, since, &wanted).is_empty();
        if !differs(&rx.borrow_and_update()) {
            waited = true;
            let wait = Duration::from_millis(args.wait_ms.unwrap_or(DEFAULT_WAIT_MS));
            let waiting = async {
                loop {
                    match rx.changed().await {
                        Ok(()) if differs(&rx.borrow_and_update()) => return true,
                        Ok(()) => {}
                        // The application is gone.
                        Err(_) => return false,
                    }
                }
            };
            let came = tokio::time::timeout(wait, waiting).await.unwrap_or(false);
            if came && let Some(settle) = args.settle_ms.filter(|s| *s > 0) {
                // Sleeps without looking: what comes meanwhile is in the
                // counts read after.
                tokio::time::sleep(Duration::from_millis(settle)).await;
            }
        }
    }
    let now = *feed.tx.borrow();
    let moved = match &args.since {
        Some(since) => changed(&now, since, &wanted),
        None => Vec::new(),
    };
    let names: Vec<&str> = moved.iter().map(|t| t.name()).collect();
    let mut text = format!(
        "frames {}, paints {}, logs {}, reloads {}",
        now.frames, now.paints, now.logs, now.reloads
    );
    if args.since.is_none() {
        text.push_str(" (the counts to give as `since`)");
    } else if names.is_empty() {
        text.push_str("; nothing changed");
    } else {
        let _ = write!(text, "; changed: {}", names.join(", "));
    }
    ToolReply::from(json!({
        "counts": {
            "frames": now.frames,
            "paints": now.paints,
            "logs": now.logs,
            "reloads": now.reloads,
        },
        "changed": names,
        "waited": waited,
    }))
    .with_text(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(f)
    }

    #[test]
    fn a_count_that_moved_is_told_at_once() {
        let feed = ChangeFeed::new();
        feed.log();
        let args = WatchArgs {
            since: Some(Since {
                logs: Some(0),
                ..Since::default()
            }),
            ..WatchArgs::default()
        };
        let reply = run(watch(&feed, &args));
        assert_eq!(reply.structured["changed"], json!(["logs"]));
        assert_eq!(reply.structured["waited"], json!(false));
    }

    #[test]
    fn a_watch_wakes_when_a_count_moves() {
        let feed = ChangeFeed::new();
        let args = WatchArgs {
            since: Some(Since {
                paints: Some(0),
                frames: Some(0),
                ..Since::default()
            }),
            topics: vec![Topic::Paints],
            wait_ms: Some(5_000),
            ..WatchArgs::default()
        };
        let reply = run(async {
            let painter = async {
                tokio::time::sleep(Duration::from_millis(10)).await;
                // A frame is not a topic of this watch; the pass is.
                feed.frame();
                tokio::time::sleep(Duration::from_millis(10)).await;
                feed.paint();
            };
            let (reply, ()) = tokio::join!(watch(&feed, &args), painter);
            reply
        });
        assert_eq!(reply.structured["changed"], json!(["paints"]));
        assert_eq!(reply.structured["waited"], json!(true));
        assert_eq!(feed.counts().frames, 1);
    }
}
