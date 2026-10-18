//! `app_watch`: what changed in the application, told when it changes.
//!
//! The transport has no channel the server may speak on by itself, so a
//! client that wants to follow the application asks and is answered when
//! there is something to tell. The inspector counts what the host already
//! tells it (a frame run, a pass drawn, a line logged, a reload) in one
//! `watch` cell, and a waiting call sleeps on that cell on the server's
//! thread. The UI thread is never woken for a watch: an application at
//! rest runs nothing, and a client waiting on it costs one sleeping task.
//!
//! A client can also follow a few items: each pass that changed the
//! picture where one of them lies counts for it. Only while some client
//! watches items does the host work out where a pass changed the picture,
//! and the items it looks at are the ones asked for in the last minute.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ljos_mcp_core::ToolReply;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::watch;

pub const WATCH: &str = "\
Wait until the app changes: frames run, passes drawn (`paints`), log lines, reloads, `traces`, or \
passes that drew the `items` given by handle. Give the `counts` of the last answer as `since`; it \
answers when a `topics` count differs, else after `wait_ms`. `settle_ms` gathers a burst.";

/// The longest a watch waits.
pub const MAX_WAIT_MS: u64 = 30_000;
/// The longest a watch gathers a burst.
pub const MAX_SETTLE_MS: u64 = 5_000;
pub(crate) const DEFAULT_WAIT_MS: u64 = 10_000;
/// The most items a watch follows, and the most followed at once.
pub const MAX_ITEMS: usize = 16;
/// How long an item is followed after a watch last asked about it.
const KEEP_ITEM: Duration = Duration::from_secs(60);

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
    /// Events `app_trace` wrote down.
    Traces,
    /// Passes that changed the picture where one of the watch's `items`
    /// lies.
    Items,
}

impl Topic {
    const ALL: [Topic; 6] = [
        Topic::Frames,
        Topic::Paints,
        Topic::Logs,
        Topic::Reloads,
        Topic::Traces,
        Topic::Items,
    ];

    fn name(self) -> &'static str {
        match self {
            Topic::Frames => "frames",
            Topic::Paints => "paints",
            Topic::Logs => "logs",
            Topic::Reloads => "reloads",
            Topic::Traces => "traces",
            Topic::Items => "items",
        }
    }
}

/// The counts a client was last told; a topic left out is not compared.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
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
    #[serde(default)]
    pub traces: Option<u64>,
    /// By handle; an item left out is not compared.
    #[serde(default)]
    pub items: Option<BTreeMap<String, u64>>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WatchArgs {
    /// The `counts` of the last answer. Without them the counts are
    /// answered at once.
    #[serde(default)]
    pub since: Option<Since>,
    /// Wait for these only: `frames`, `paints`, `logs`, `reloads`,
    /// `traces`, `items`. All by default.
    #[serde(default)]
    pub topics: Vec<Topic>,
    /// Items to follow, by handle (`@12.3`, as `app_objects` gives them),
    /// at most 16: how many passes changed the picture where each lies,
    /// counted from when it was first watched.
    #[serde(default)]
    pub items: Vec<String>,
    /// How long to wait at most, in milliseconds (default 10000, at most
    /// 30000).
    #[serde(default)]
    pub wait_ms: Option<u64>,
    /// After the first change, wait this much longer so that a burst is
    /// one answer (at most 5000).
    #[serde(default)]
    pub settle_ms: Option<u64>,
    /// The name this client gave its traces (`app_trace`'s `holder`):
    /// they last while it watches.
    #[serde(default)]
    pub holder: Option<String>,
}

/// How many of each thing happened since the inspector attached.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    pub(crate) frames: u64,
    pub(crate) paints: u64,
    pub(crate) logs: u64,
    pub(crate) reloads: u64,
    pub(crate) traces: u64,
    /// Passes counted for any watched item: it moves so that a waiting
    /// watch looks at its own items again.
    pub(crate) items: u64,
}

impl Counts {
    /// `None` for the items, which are counted one by one.
    fn of(&self, topic: Topic) -> Option<u64> {
        match topic {
            Topic::Frames => Some(self.frames),
            Topic::Paints => Some(self.paints),
            Topic::Logs => Some(self.logs),
            Topic::Reloads => Some(self.reloads),
            Topic::Traces => Some(self.traces),
            Topic::Items => None,
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
            Topic::Traces => self.traces,
            Topic::Items => None,
        }
    }
}

/// An item a watch asked about.
#[derive(Debug)]
struct Followed {
    handle: String,
    drawn: u64,
    asked: Instant,
}

/// Written on the UI thread, read by the server's. A count moves with
/// something the host does anyway, so keeping it costs a store per frame.
#[derive(Debug)]
pub(crate) struct ChangeFeed {
    tx: watch::Sender<Counts>,
    /// Written by watches on the server's thread, counted on the UI
    /// thread after a pass.
    followed: Mutex<Vec<Followed>>,
}

impl ChangeFeed {
    pub(crate) fn new() -> Arc<ChangeFeed> {
        let (tx, _) = watch::channel(Counts::default());
        Arc::new(ChangeFeed {
            tx,
            followed: Mutex::new(Vec::new()),
        })
    }

    fn followed(&self) -> std::sync::MutexGuard<'_, Vec<Followed>> {
        self.followed.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A watch asks about `handles`: they are followed for a while from
    /// now. Those no watch asked about for a minute are let go, and the
    /// ones asked about longest ago when there would be too many.
    fn follow(&self, handles: &[String], now: Instant) {
        let mut followed = self.followed();
        followed.retain(|f| now.duration_since(f.asked) < KEEP_ITEM);
        for h in handles {
            match followed.iter_mut().find(|f| &f.handle == h) {
                Some(f) => f.asked = now,
                None => followed.push(Followed {
                    handle: h.clone(),
                    drawn: 0,
                    asked: now,
                }),
            }
        }
        if followed.len() > MAX_ITEMS {
            followed.sort_by_key(|f| std::cmp::Reverse(f.asked));
            followed.truncate(MAX_ITEMS);
        }
    }

    pub(crate) fn follows_any(&self) -> bool {
        !self.followed().is_empty()
    }

    /// The handles of the items followed now, for the UI thread to look
    /// up after a pass. Empty when no watch asked for any.
    pub(crate) fn followed_items(&self) -> Vec<String> {
        self.followed().iter().map(|f| f.handle.clone()).collect()
    }

    /// A pass changed the picture where these items lie.
    pub(crate) fn drawn_again(&self, handles: &[String]) {
        if handles.is_empty() {
            return;
        }
        for f in self.followed().iter_mut() {
            if handles.contains(&f.handle) {
                f.drawn += 1;
            }
        }
        self.tx.send_modify(|c| c.items += 1);
    }

    /// How often each of `handles` was drawn again; 0 for one not
    /// followed.
    fn drawn(&self, handles: &[String]) -> BTreeMap<String, u64> {
        let followed = self.followed();
        handles
            .iter()
            .map(|h| {
                let n = followed
                    .iter()
                    .find(|f| &f.handle == h)
                    .map_or(0, |f| f.drawn);
                (h.clone(), n)
            })
            .collect()
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

    pub(crate) fn trace(&self) {
        self.tx.send_modify(|c| c.traces += 1);
    }

    #[cfg(test)]
    fn counts(&self) -> Counts {
        *self.tx.borrow()
    }
}

/// The topics of `wanted` whose count is not the one in `since`. The
/// items moved when one given in `since` was drawn a different number of
/// times.
fn changed(
    now: &Counts,
    drawn: &BTreeMap<String, u64>,
    since: &Since,
    wanted: &[Topic],
) -> Vec<Topic> {
    wanted
        .iter()
        .copied()
        .filter(|t| match (now.of(*t), &since.items) {
            (Some(count), _) => since.of(*t).is_some_and(|was| was != count),
            (None, Some(was)) => drawn
                .iter()
                .any(|(h, n)| was.get(h).is_some_and(|w| w != n)),
            (None, None) => false,
        })
        .collect()
}

/// Answers `app_watch`, on the server's thread.
pub(crate) async fn watch(feed: &ChangeFeed, args: &WatchArgs) -> ToolReply {
    let wanted: Vec<Topic> = if args.topics.is_empty() {
        Topic::ALL.to_vec()
    } else {
        args.topics.clone()
    };
    if !args.items.is_empty() {
        feed.follow(&args.items, Instant::now());
    }
    let mut rx = feed.tx.subscribe();
    let mut waited = false;
    let mut settled = false;
    if let Some(since) = &args.since {
        // Takes the counts copied out, so that the channel is not held
        // while the items are looked at.
        let differs = |c: Counts| !changed(&c, &feed.drawn(&args.items), since, &wanted).is_empty();
        // An application that changes every frame has changed again by
        // the time a client asks anew. The burst is still gathered, or a
        // client following it would ask as fast as answers come.
        let mut came = differs(*rx.borrow_and_update());
        if !came {
            waited = true;
            let wait = Duration::from_millis(args.wait_ms.unwrap_or(DEFAULT_WAIT_MS));
            let waiting = async {
                loop {
                    match rx.changed().await {
                        Ok(()) if differs(*rx.borrow_and_update()) => return true,
                        Ok(()) => {}
                        // The application is gone.
                        Err(_) => return false,
                    }
                }
            };
            came = tokio::time::timeout(wait, waiting).await.unwrap_or(false);
        }
        if came && let Some(settle) = args.settle_ms.filter(|s| *s > 0) {
            // Sleeps without looking: what comes meanwhile is in the
            // counts read after.
            settled = true;
            tokio::time::sleep(Duration::from_millis(settle)).await;
        }
    }
    let now = *feed.tx.borrow();
    let drawn = feed.drawn(&args.items);
    let moved = match &args.since {
        Some(since) => changed(&now, &drawn, since, &wanted),
        None => Vec::new(),
    };
    let names: Vec<&str> = moved.iter().map(|t| t.name()).collect();
    let mut text = format!(
        "frames {}, paints {}, logs {}, reloads {}, traces {}",
        now.frames, now.paints, now.logs, now.reloads, now.traces
    );
    for (h, n) in &drawn {
        let _ = write!(text, ", {h} drawn {n}");
    }
    if args.since.is_none() {
        text.push_str(" (the counts to give as `since`)");
    } else if names.is_empty() {
        text.push_str("; nothing changed");
    } else {
        let _ = write!(text, "; changed: {}", names.join(", "));
    }
    let mut counts = json!({
        "frames": now.frames,
        "paints": now.paints,
        "logs": now.logs,
        "reloads": now.reloads,
        "traces": now.traces,
    });
    if !args.items.is_empty() {
        counts["items"] = json!(drawn);
    }
    ToolReply::from(json!({
        "counts": counts,
        "changed": names,
        "waited": waited,
        "settled": settled,
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
        assert_eq!(reply.structured["settled"], json!(false));
    }

    #[test]
    fn a_change_already_there_is_gathered_for_the_settle() {
        let feed = ChangeFeed::new();
        feed.paint();
        let args = WatchArgs {
            since: Some(Since {
                paints: Some(0),
                ..Since::default()
            }),
            settle_ms: Some(30),
            ..WatchArgs::default()
        };
        let reply = run(async {
            let painter = async {
                tokio::time::sleep(Duration::from_millis(5)).await;
                feed.paint();
            };
            let (reply, ()) = tokio::join!(watch(&feed, &args), painter);
            reply
        });
        assert_eq!(reply.structured["waited"], json!(false));
        assert_eq!(reply.structured["settled"], json!(true));
        // The pass drawn while it settled is in the counts.
        assert_eq!(reply.structured["counts"]["paints"], json!(2));
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

    fn items(handles: &[&str]) -> Vec<String> {
        handles.iter().map(|h| (*h).to_owned()).collect()
    }

    #[test]
    fn a_watch_of_an_item_wakes_only_when_that_item_is_drawn_again() {
        let feed = ChangeFeed::new();
        let args = WatchArgs {
            items: items(&["@1.1"]),
            since: Some(Since {
                items: Some([("@1.1".to_owned(), 0)].into()),
                ..Since::default()
            }),
            topics: vec![Topic::Items],
            wait_ms: Some(5_000),
            ..WatchArgs::default()
        };
        let reply = run(async {
            let painter = async {
                tokio::time::sleep(Duration::from_millis(10)).await;
                // Followed by another watch, drawn: not this one's item.
                feed.follow(&items(&["@2.1"]), Instant::now());
                feed.drawn_again(&items(&["@2.1"]));
                tokio::time::sleep(Duration::from_millis(10)).await;
                feed.drawn_again(&items(&["@1.1", "@2.1"]));
            };
            let (reply, ()) = tokio::join!(watch(&feed, &args), painter);
            reply
        });
        assert_eq!(reply.structured["changed"], json!(["items"]));
        assert_eq!(reply.structured["waited"], json!(true));
        assert_eq!(reply.structured["counts"]["items"], json!({"@1.1": 1}));
    }

    #[test]
    fn items_are_followed_while_asked_about_and_at_most_sixteen() {
        let feed = ChangeFeed::new();
        let start = Instant::now();
        let first: Vec<String> = (0..MAX_ITEMS).map(|n| format!("@{n}.1")).collect();
        feed.follow(&first, start);
        feed.follow(&items(&["@0.1"]), start + Duration::from_secs(1));
        // One more than there is room for: the one asked about longest
        // ago goes, not the one asked about again.
        feed.follow(&items(&["@99.1"]), start + Duration::from_secs(2));
        let now = feed.followed_items();
        assert_eq!(now.len(), MAX_ITEMS);
        assert!(now.contains(&"@0.1".to_owned()) && now.contains(&"@99.1".to_owned()));
        // A minute without being asked about lets an item go.
        feed.follow(
            &items(&["@99.1"]),
            start + KEEP_ITEM + Duration::from_millis(1500),
        );
        assert_eq!(feed.followed_items(), items(&["@99.1"]));
    }
}
