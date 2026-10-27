//! Frame times and input latency from what the compositor reported.

use super::{Frames, LONG_KEPT};

/// The value at quantile `q` of sorted `values`.
fn quantile(values: &[f64], q: f64) -> Option<f64> {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let i = ((values.len().checked_sub(1)?) as f64 * q).round() as usize;
    values.get(i).copied()
}

pub(super) fn frames(times: &[u64], start: u64, seconds: f64, inputs: &[u64]) -> Frames {
    #[allow(clippy::cast_precision_loss)]
    let ms = |ns: u64| ns as f64 / 1e6;
    let mut long: Vec<[f64; 2]> = times
        .windows(2)
        .map(|w| {
            [
                ms(w[1].saturating_sub(start)),
                ms(w[1].saturating_sub(w[0])),
            ]
        })
        .collect();
    let mut gaps: Vec<f64> = long.iter().map(|l| l[1]).collect();
    gaps.sort_by(f64::total_cmp);
    long.sort_by(|a, b| b[1].total_cmp(&a[1]));
    long.truncate(LONG_KEPT);
    long.sort_by(|a, b| a[0].total_cmp(&b[0]));
    // An input after the last frame was never answered while counting.
    #[allow(clippy::cast_precision_loss)]
    let waits_ms: Vec<Option<f64>> = inputs
        .iter()
        .map(|&sent| {
            let i = times.partition_point(|&t| t <= sent);
            times.get(i).map(|&t| (t - sent) as f64 / 1e6)
        })
        .collect();
    let mut waits: Vec<f64> = waits_ms.iter().flatten().copied().collect();
    waits.sort_by(f64::total_cmp);
    #[allow(clippy::cast_precision_loss)]
    let fps = if seconds > 0.0 {
        times.len() as f64 / seconds
    } else {
        0.0
    };
    Frames {
        count: times.len(),
        fps,
        p50_ms: quantile(&gaps, 0.5),
        p95_ms: quantile(&gaps, 0.95),
        max_ms: gaps.last().copied(),
        latency_p50_ms: quantile(&waits, 0.5),
        latency_p95_ms: quantile(&waits, 0.95),
        latency_max_ms: waits.last().copied(),
        committed: None,
        long,
        waits_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_intervals() {
        let f = frames(&[0, 16_000_000, 33_000_000, 100_000_000], 0, 1.0, &[]);
        assert_eq!(f.long[2], [100.0, 67.0]);
        assert_eq!(f.count, 4);
        assert_eq!(f.max_ms, Some(67.0));
        assert_eq!(f.p50_ms, Some(17.0));
        assert_eq!(f.latency_p50_ms, None);
        let none = frames(&[], 0, 1.0, &[]);
        assert_eq!((none.count, none.p50_ms), (0, None));
    }

    #[test]
    fn input_to_frame() {
        // Inputs at 5, 20 and 40 ms; frames at 16, 33 and 100 ms; one
        // input after the last frame is not answered.
        let ms = 1_000_000;
        let f = frames(
            &[16 * ms, 33 * ms, 100 * ms],
            0,
            1.0,
            &[5 * ms, 20 * ms, 40 * ms, 120 * ms],
        );
        assert_eq!(f.latency_p50_ms, Some(13.0));
        assert_eq!(f.latency_max_ms, Some(60.0));
    }
}
