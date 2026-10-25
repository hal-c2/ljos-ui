//! `app_frames`' `growth`: the instances and bytes of each component
//! loaded, sampled over time while a client asks, with the components
//! whose instances keep growing marked.
//!
//! A leak shows as a component whose count goes up and does not come
//! back: at every action repeated (a dialog opened and closed, a page
//! visited) or while the application seems to rest. The counts change only
//! where the application runs, so they are sampled where it runs: at a
//! pass of the host, at most once every [`EVERY`], and at each answer.
//! An application at rest is not woken for it. A run of equal samples is
//! kept as its first and its last.
//!
//! Nothing is sampled until a client asks; while nobody does a pass pays
//! one check. The samples are a ring of [`MAX_SAMPLES`].

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::Write as _;
use std::time::{Duration, Instant};

use ljos_engine::Engine;
use ljos_mcp_core::{SurfaceError, ToolReply};
use serde_json::{Value, json};

/// The least time between two samples taken at passes.
pub const EVERY: Duration = Duration::from_secs(1);
/// The most samples kept.
pub const MAX_SAMPLES: usize = 300;
/// The most components one answer lists unless told otherwise.
const DEFAULT_MAX: usize = 60;
/// The most components one answer lists.
const MAX_LISTED: usize = 600;
/// The most points of one component's series in an answer.
const MAX_POINTS: usize = 120;
/// The most clients that hold the sampling at once.
const MAX_HOLDERS: usize = 16;
/// The rises that make a component one that keeps growing.
const RISES: u32 = 2;

struct Component {
    id: String,
    name: String,
    kind: &'static str,
    url: String,
}

struct Sample {
    at: Duration,
    /// By component index: instances and bytes, of those that have any.
    counts: Vec<(u32, u32, u64)>,
}

impl Sample {
    fn of(&self, c: u32) -> (u32, u64) {
        self.counts
            .binary_search_by_key(&c, |&(i, _, _)| i)
            .map_or((0, 0), |at| {
                let (_, n, b) = self.counts[at];
                (n, b)
            })
    }
}

pub(crate) struct Growth {
    started: Instant,
    unheld: bool,
    held: HashSet<String>,
    components: Vec<Component>,
    index: HashMap<String, u32>,
    samples: VecDeque<Sample>,
    last: Option<Instant>,
    dropped: u64,
}

impl Default for Growth {
    fn default() -> Self {
        Growth {
            started: Instant::now(),
            unheld: false,
            held: HashSet::new(),
            components: Vec::new(),
            index: HashMap::new(),
            samples: VecDeque::new(),
            last: None,
            dropped: 0,
        }
    }
}

/// What the samples say of one component.
struct Trend {
    c: u32,
    first: u32,
    last: u32,
    min: u32,
    max: u32,
    rises: u32,
    falls: u32,
    bytes: u64,
}

impl Trend {
    /// Up at least [`RISES`] times and never down: what an action repeated
    /// leaves behind each time.
    fn growing(&self) -> bool {
        self.rises >= RISES && self.falls == 0 && self.last > self.first
    }
}

impl Growth {
    pub(crate) fn recording(&self) -> bool {
        self.unheld || !self.held.is_empty()
    }

    /// Starts or stops sampling for whoever asks. Starting afresh forgets
    /// the samples and takes the first.
    pub(crate) fn ask(
        &mut self,
        engine: &Engine,
        record: bool,
        holder: Option<&str>,
    ) -> Result<(), SurfaceError> {
        let was = self.recording();
        match (record, holder) {
            (true, Some(h)) => {
                if !self.held.contains(h) && self.held.len() >= MAX_HOLDERS {
                    return Err(SurfaceError::invalid_argument(format!(
                        "`app_frames`: {MAX_HOLDERS} clients sample growth already"
                    )));
                }
                self.held.insert(h.to_owned());
            }
            (true, None) => self.unheld = true,
            (false, Some(h)) => {
                self.held.remove(h);
            }
            (false, None) => {
                self.unheld = false;
                self.held.clear();
            }
        }
        if self.recording() && !was {
            self.samples.clear();
            self.dropped = 0;
            self.started = Instant::now();
            self.sample(engine);
        }
        Ok(())
    }

    /// A pass of the host: a sample if one is due.
    pub(crate) fn tick(&mut self, engine: &Engine) {
        if !self.recording() || self.last.is_some_and(|l| l.elapsed() < EVERY) {
            return;
        }
        self.sample(engine);
    }

    fn sample(&mut self, engine: &Engine) {
        let now = Instant::now();
        self.last = Some(now);
        let mut counts: Vec<(u32, u32, u64)> = engine
            .components()
            .into_iter()
            .filter(|c| !c.instances.is_empty())
            .map(|c| {
                let i = match self.index.get(&c.id) {
                    Some(&i) => i,
                    None => {
                        let i = self.components.len() as u32;
                        self.index.insert(c.id.clone(), i);
                        self.components.push(Component {
                            id: c.id.clone(),
                            name: c.name.clone(),
                            kind: c.kind.name(),
                            url: c.url.clone(),
                        });
                        i
                    }
                };
                let n = u32::try_from(c.instances.len()).unwrap_or(u32::MAX);
                (i, n, c.bytes as u64)
            })
            .collect();
        counts.sort_unstable_by_key(|&(i, _, _)| i);
        let at = now.duration_since(self.started);
        // A run of equal samples is its first and its last.
        let n = self.samples.len();
        if n >= 2 && self.samples[n - 1].counts == counts && self.samples[n - 2].counts == counts {
            self.samples[n - 1].at = at;
            return;
        }
        if n == MAX_SAMPLES {
            self.samples.pop_front();
            self.dropped += 1;
        }
        self.samples.push_back(Sample { at, counts });
    }

    fn trends(&self) -> Vec<Trend> {
        let mut seen: Vec<u32> = self
            .samples
            .iter()
            .flat_map(|s| s.counts.iter().map(|&(i, _, _)| i))
            .collect();
        seen.sort_unstable();
        seen.dedup();
        seen.into_iter()
            .map(|c| {
                let mut t = Trend {
                    c,
                    first: 0,
                    last: 0,
                    min: u32::MAX,
                    max: 0,
                    rises: 0,
                    falls: 0,
                    bytes: 0,
                };
                let mut prev = None;
                for s in &self.samples {
                    let (n, b) = s.of(c);
                    match prev {
                        None => t.first = n,
                        Some(p) if n > p => t.rises += 1,
                        Some(p) if n < p => t.falls += 1,
                        Some(_) => {}
                    }
                    prev = Some(n);
                    t.last = n;
                    t.bytes = b;
                    t.min = t.min.min(n);
                    t.max = t.max.max(n);
                }
                t
            })
            .collect()
    }

    /// The series of `c` with at most [`MAX_POINTS`] points, the last
    /// always among them: `[at_ms, instances, bytes]`.
    fn series(&self, c: u32) -> Vec<Value> {
        let n = self.samples.len();
        let step = n.div_ceil(MAX_POINTS).max(1);
        self.samples
            .iter()
            .enumerate()
            .filter(|(i, _)| i % step == 0 || *i + 1 == n)
            .map(|(_, s)| {
                let (count, bytes) = s.of(c);
                json!([s.at.as_millis() as u64, count, bytes])
            })
            .collect()
    }

    /// Answers `app_frames` with `growth`, after a sample of now.
    pub(crate) fn answer(&mut self, engine: &Engine, max: Option<usize>) -> ToolReply {
        if self.recording() {
            self.sample(engine);
        }
        let max = max.unwrap_or(DEFAULT_MAX).min(MAX_LISTED);
        let mut trends = self.trends();
        trends.sort_by(|a, b| {
            b.growing()
                .cmp(&a.growing())
                .then_with(|| {
                    (i64::from(b.last) - i64::from(b.first))
                        .cmp(&(i64::from(a.last) - i64::from(a.first)))
                })
                .then_with(|| b.last.cmp(&a.last))
        });
        let count = trends.len();
        let growing = trends.iter().filter(|t| t.growing()).count();
        trends.truncate(max);
        let window = self.samples.back().map_or(Duration::ZERO, |s| s.at);
        let mut text = String::new();
        if self.recording() {
            let _ = writeln!(
                text,
                "sampling components over {:.1} s ({} samples); {growing} of {count} keep growing",
                window.as_secs_f64(),
                self.samples.len()
            );
        } else {
            text.push_str("not sampling; `growth: true, record: true` starts it\n");
        }
        for t in &trends {
            let c = &self.components[t.c as usize];
            let _ = writeln!(
                text,
                "{}{} {} ({}): {} → {} instances (min {}, max {}, {} up, {} down), {} bytes",
                if t.growing() { "growing " } else { "" },
                c.kind,
                c.name,
                c.id,
                t.first,
                t.last,
                t.min,
                t.max,
                t.rises,
                t.falls,
                t.bytes
            );
        }
        let value = json!({
            "recording": self.recording(),
            "window_ms": window.as_millis() as u64,
            "samples": self.samples.len(),
            "dropped": self.dropped,
            "count": count,
            "growing": growing,
            "components": trends.iter().map(|t| {
                let c = &self.components[t.c as usize];
                json!({
                    "component": c.id,
                    "name": c.name,
                    "kind": c.kind,
                    "url": c.url,
                    "growing": t.growing(),
                    "first": t.first,
                    "last": t.last,
                    "min": t.min,
                    "max": t.max,
                    "rises": t.rises,
                    "falls": t.falls,
                    "bytes": t.bytes,
                    "series": self.series(t.c),
                })
            }).collect::<Vec<_>>(),
        });
        ToolReply::from(value).with_text(text)
    }
}
