//! What each 3D frame cost, as the runtime counted it while it drew.
//!
//! Ljós writes a JSON line for every frame it draws a 3D scene in when
//! `LJOS_3D_FRAMES` names a file (`ljos_3d::stats`): what it drew and set,
//! what it uploaded and made, and how long each part took on the CPU. The
//! bench names the file, notes when each span of the run began and ended
//! on the same clock as the lines (microseconds since the epoch), and sums
//! up the lines of each span here. Every number a line has is summed up
//! by its own name, so a counter added to the lines is in the report
//! without a change here.

use std::{collections::BTreeMap, path::Path};

use serde::{Deserialize, Serialize};

/// The environment variable that names the file of lines.
pub const ENV: &str = "LJOS_3D_FRAMES";

/// Fields of a line that say which frame it is, not what it cost.
const NOT_COSTS: &[&str] = &["t_us", "frame"];

/// One number of the frames of a span.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stat {
    pub p50: f64,
    pub p99: f64,
    pub mean: f64,
    pub max: f64,
}

/// The frames drawn in a span of time and each of their numbers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spread {
    /// 3D frames drawn in the span.
    pub frames: usize,
    /// Of them a second.
    pub fps: f64,
    /// By the line's name: `draw_calls`, `encode_ms`, … and `cpu_ms`, the
    /// four CPU parts of a frame added up.
    pub costs: BTreeMap<String, Stat>,
}

/// The 3D frames of one run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frames3d {
    /// Frames drawn in the whole run.
    pub frames: usize,
    /// From the gate opening to the first 3D frame handed to the GPU.
    pub first_ms: Option<f64>,
    /// The first frame's numbers: what starting cost in uploads and
    /// pipelines made.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub first: BTreeMap<String, f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle: Option<Spread>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load: Option<Spread>,
}

/// When a span began and ended, in microseconds since the epoch.
#[derive(Clone, Copy, Debug)]
pub struct Window {
    pub from_us: u64,
    pub to_us: u64,
}

/// Microseconds since the epoch, the lines' clock.
#[must_use]
pub fn now_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_micros()).unwrap_or(u64::MAX))
}

/// The frames of the file at `path`; `None` when there is none, as for a
/// runtime that does not write them or a run that drew no 3D frame.
#[must_use]
pub fn read(
    path: &Path,
    opened_us: u64,
    idle: Option<Window>,
    load: Option<Window>,
) -> Option<Frames3d> {
    let text = std::fs::read_to_string(path).ok()?;
    summarise(&text, opened_us, idle, load)
}

/// The frames of `text`, a line of JSON each; a line cut short by the
/// runtime being stopped is left out.
#[must_use]
pub fn summarise(
    text: &str,
    opened_us: u64,
    idle: Option<Window>,
    load: Option<Window>,
) -> Option<Frames3d> {
    let lines: Vec<(u64, BTreeMap<String, f64>)> = text
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(l).ok())
        .filter_map(|m| {
            let t = m.get("t_us")?.as_u64()?;
            let mut costs: BTreeMap<String, f64> = m
                .iter()
                .filter(|(k, _)| !NOT_COSTS.contains(&k.as_str()))
                .filter_map(|(k, v)| Some((k.clone(), v.as_f64()?)))
                .collect();
            let cpu: f64 = ["sync_ms", "prepare_ms", "encode_ms", "submit_ms"]
                .iter()
                .filter_map(|k| costs.get(*k))
                .sum();
            costs.insert("cpu_ms".to_owned(), cpu);
            Some((t, costs))
        })
        .collect();
    let (t, first) = lines.first()?.clone();
    #[allow(clippy::cast_precision_loss)]
    let first_ms = (t >= opened_us).then(|| (t - opened_us) as f64 / 1000.0);
    let spread = |w: Window| spread(&lines, w);
    Some(Frames3d {
        frames: lines.len(),
        first_ms,
        first,
        idle: idle.map(spread),
        load: load.map(spread),
    })
}

fn spread(lines: &[(u64, BTreeMap<String, f64>)], w: Window) -> Spread {
    let inside: Vec<&BTreeMap<String, f64>> = lines
        .iter()
        .filter(|(t, _)| (w.from_us..w.to_us).contains(t))
        .map(|(_, c)| c)
        .collect();
    let mut by_name: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for costs in &inside {
        for (k, v) in *costs {
            by_name.entry(k.clone()).or_default().push(*v);
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let seconds = w.to_us.saturating_sub(w.from_us) as f64 / 1e6;
    #[allow(clippy::cast_precision_loss)]
    Spread {
        frames: inside.len(),
        fps: if seconds > 0.0 {
            inside.len() as f64 / seconds
        } else {
            0.0
        },
        costs: by_name
            .into_iter()
            .map(|(k, mut v)| {
                v.sort_by(f64::total_cmp);
                let at = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
                let stat = Stat {
                    p50: at(0.5),
                    p99: at(0.99),
                    mean: v.iter().sum::<f64>() / v.len() as f64,
                    max: v[v.len() - 1],
                };
                (k, stat)
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_take_their_own_frames() {
        let text = "\
{\"t_us\":1000500,\"frame\":1,\"draw_calls\":40,\"sync_ms\":0.5,\"prepare_ms\":1.0,\"encode_ms\":0.25,\"submit_ms\":0.25,\"gpu_ms\":null}
{\"t_us\":2000000,\"frame\":2,\"draw_calls\":10,\"sync_ms\":0.1,\"prepare_ms\":0.2,\"encode_ms\":0.1,\"submit_ms\":0.1,\"gpu_ms\":0.5}
{\"t_us\":2500000,\"frame\":3,\"draw_calls\":30,\"sync_ms\":0.1,\"prepare_ms\":0.2,\"encode_ms\":0.1,\"submit_ms\":0.1,\"gpu_ms\":0.7}
{\"t_us\":2600000,\"fra";
        let load = Window {
            from_us: 1_500_000,
            to_us: 3_500_000,
        };
        let f = summarise(text, 1_000_000, None, Some(load)).unwrap();
        assert_eq!(f.frames, 3);
        assert_eq!(f.first_ms, Some(0.5));
        assert_eq!(f.first["draw_calls"], 40.0);
        assert_eq!(f.first["cpu_ms"], 2.0);
        let l = f.load.unwrap();
        assert_eq!((l.frames, l.fps), (2, 1.0));
        assert_eq!(l.costs["draw_calls"].max, 30.0);
        assert_eq!(l.costs["draw_calls"].mean, 20.0);
        assert_eq!(l.costs["gpu_ms"].p50, 0.7);
        assert!(f.idle.is_none());
        assert!(summarise("", 0, None, None).is_none());
    }
}
