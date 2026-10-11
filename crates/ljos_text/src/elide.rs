//! Where to cut a line that does not fit.

use std::ops::Range;

use crate::style::ElideMode;

/// What replaces the text that is cut away.
pub(crate) const ELLIPSIS: &str = "\u{2026}";

/// The clusters of one line in reading order, with running widths, so the
/// widest cut that fits is found by bisection instead of shaping every
/// candidate.
pub(crate) struct Clusters {
    /// Byte offset where each cluster starts, plus the end of the text.
    bounds: Vec<usize>,
    /// `widths[i]`: total advance of the first `i` clusters.
    widths: Vec<f32>,
}

impl Clusters {
    /// `items` are byte ranges and advances in any order.
    pub(crate) fn new(mut items: Vec<(Range<usize>, f32)>, text_len: usize) -> Self {
        items.retain(|(r, _)| r.start < r.end && r.end <= text_len);
        items.sort_by_key(|(r, _)| r.start);
        let mut bounds = Vec::with_capacity(items.len() + 1);
        let mut widths = Vec::with_capacity(items.len() + 1);
        let mut total = 0.0;
        widths.push(total);
        for (range, advance) in &items {
            bounds.push(range.start);
            total += advance;
            widths.push(total);
        }
        bounds.push(text_len);
        Self { bounds, widths }
    }

    /// Number of clusters.
    pub(crate) fn len(&self) -> usize {
        self.bounds.len() - 1
    }

    /// Clusters kept at the start and at the end when `keep` survive.
    ///
    /// A cut in the middle keeps as many at one end as at the other, as Qt
    /// does: it gives up a cluster on each side rather than leave the ends
    /// uneven, so an odd `keep` counts as the even number below it.
    fn split(mode: ElideMode, keep: usize) -> (usize, usize) {
        match mode {
            ElideMode::Left => (0, keep),
            ElideMode::Middle => (keep / 2, keep / 2),
            ElideMode::Right | ElideMode::None => (keep, 0),
        }
    }

    /// Estimated width of `keep` surviving clusters. An estimate, because the
    /// cut can change kerning and joining at its edges.
    fn kept_width(&self, mode: ElideMode, keep: usize) -> f32 {
        let (head, tail) = Self::split(mode, keep);
        let n = self.len();
        self.widths[head] + (self.widths[n] - self.widths[n - tail])
    }

    /// How many clusters one step of cutting takes away.
    fn step(mode: ElideMode) -> usize {
        match mode {
            ElideMode::Middle => 2,
            _ => 1,
        }
    }

    /// The largest number of clusters whose estimated width is at most `avail`.
    pub(crate) fn fit(&self, mode: ElideMode, avail: f32) -> usize {
        let step = Self::step(mode);
        // Widths grow with the number of kept clusters, so bisect.
        let (mut lo, mut hi) = (0, self.len() / step);
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            if self.kept_width(mode, mid * step) <= avail {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        lo * step
    }

    /// The next smaller cut after `keep` clusters turned out too wide.
    pub(crate) fn fewer(mode: ElideMode, keep: usize) -> usize {
        keep.saturating_sub(Self::step(mode))
    }

    /// How many bytes at the start of the text survive a cut to `keep`
    /// clusters: where the ellipsis goes.
    pub(crate) fn head_end(&self, mode: ElideMode, keep: usize) -> usize {
        let (head, _) = Self::split(mode, keep.min(self.len()));
        self.bounds[head]
    }

    /// `text` with `keep` clusters left and an ellipsis where the rest was.
    pub(crate) fn elided(&self, text: &str, mode: ElideMode, keep: usize) -> String {
        let n = self.len();
        let (head, tail) = Self::split(mode, keep.min(n));
        let head_end = self.bounds[head];
        let tail_start = self.bounds[n - tail].max(head_end);
        let mut out = String::with_capacity(head_end + ELLIPSIS.len() + text.len() - tail_start);
        out.push_str(&text[..head_end]);
        out.push_str(ELLIPSIS);
        out.push_str(&text[tail_start..]);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn abc() -> Clusters {
        // Five clusters, 10 wide each; given out of order on purpose.
        let items = vec![
            (2..3, 10.0),
            (0..1, 10.0),
            (1..2, 10.0),
            (4..5, 10.0),
            (3..4, 10.0),
        ];
        Clusters::new(items, 5)
    }

    #[test]
    fn fit_is_the_widest_cut_that_fits() {
        let c = abc();
        assert_eq!(c.fit(ElideMode::Right, 35.0), 3);
        assert_eq!(c.fit(ElideMode::Right, 30.0), 3);
        assert_eq!(c.fit(ElideMode::Right, 9.0), 0);
        assert_eq!(c.fit(ElideMode::Left, 100.0), 5);
        assert_eq!(c.fit(ElideMode::Middle, 25.0), 2);
    }

    #[test]
    fn a_middle_cut_keeps_both_ends_alike() {
        let c = abc();
        // Room for three clusters, but the third would make the ends uneven.
        assert_eq!(c.fit(ElideMode::Middle, 35.0), 2);
        assert_eq!(c.fit(ElideMode::Middle, 19.0), 0);
        assert_eq!(c.fit(ElideMode::Middle, 100.0), 4);
        assert_eq!(c.elided("abcde", ElideMode::Middle, 2), "a\u{2026}e");
        assert_eq!(c.elided("abcde", ElideMode::Middle, 4), "ab\u{2026}de");
        assert_eq!(Clusters::fewer(ElideMode::Middle, 4), 2);
        assert_eq!(Clusters::fewer(ElideMode::Right, 4), 3);
        assert_eq!(Clusters::fewer(ElideMode::Middle, 1), 0);
    }

    #[test]
    fn each_mode_keeps_its_side() {
        let c = abc();
        assert_eq!(c.elided("abcde", ElideMode::Right, 3), "abc\u{2026}");
        assert_eq!(c.elided("abcde", ElideMode::Left, 3), "\u{2026}cde");
        assert_eq!(c.elided("abcde", ElideMode::Middle, 3), "a\u{2026}e");
        assert_eq!(c.elided("abcde", ElideMode::Middle, 0), "\u{2026}");
        assert_eq!(c.elided("abcde", ElideMode::Right, 5), "abcde\u{2026}");
    }

    #[test]
    fn cuts_fall_on_cluster_edges() {
        // "e" + combining acute is one cluster of 3 bytes.
        let text = "e\u{301}x";
        let c = Clusters::new(vec![(0..3, 8.0), (3..4, 7.0)], text.len());
        assert_eq!(c.elided(text, ElideMode::Right, 1), "e\u{301}\u{2026}");
        assert_eq!(c.elided(text, ElideMode::Left, 1), "\u{2026}x");
    }
}
