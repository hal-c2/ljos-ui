//! How a layout shares its size among rows or columns.
//!
//! One axis at a time: a list of [`Track`]s (the columns of a row layout,
//! the rows of a column layout, either of a grid) and the room there is.
//! The rules were worked out by experiment with `qml6` (Qt 6.11) and are
//! pinned by `features/layouts/*.feature`; nothing here reads the engine.

/// One row or column: the sizes it can take.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Track {
    pub min: f64,
    pub pref: f64,
    pub max: f64,
    /// `Layout.horizontalStretchFactor` of what fills it; below 1: none.
    pub stretch: i32,
    /// False for a row or column nothing sits in: it takes no size and no
    /// spacing.
    pub used: bool,
    /// Only spanning items cover it and none starts in it. While such a
    /// track can grow, every track grows by the same amount.
    pub open: bool,
}

impl Track {
    const EMPTY: Track = Track {
        min: 0.0,
        pref: 0.0,
        max: 0.0,
        stretch: -1,
        used: false,
        open: false,
    };
}

/// What one item asks of the rows or columns it covers, margins included.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Span {
    pub start: usize,
    pub len: usize,
    pub min: f64,
    pub pref: f64,
    pub max: f64,
    pub stretch: i32,
}

/// The smallest, preferred and largest size of a whole axis.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Total {
    pub min: f64,
    pub pref: f64,
    pub max: f64,
}

/// Qt places on whole pixels, rounding a half up.
pub(crate) fn round_half_up(v: f64) -> f64 {
    (v + 0.5).floor()
}

/// The tracks `count` rows or columns need for `items`.
///
/// A track is as small as its largest minimum, prefers its largest
/// preferred size and can grow as far as its most generous item. Columns
/// that only spanning items cover count as one track per run. An item that
/// spans several tracks and does not fit makes them grow as [`distribute`]
/// would. With `uniform` every used track gets the same box.
pub(crate) fn tracks(count: usize, items: &[Span], spacing: f64, uniform: bool) -> Vec<Track> {
    let mut t = vec![Track::EMPTY; count];
    let mut covered = vec![false; count];
    let mut starts = vec![false; count];
    // A spanning item without a maximum lets the tracks under it grow.
    let mut unbounded = vec![false; count];
    for s in items {
        let end = (s.start + s.len).min(count);
        for i in s.start..end {
            covered[i] = true;
            unbounded[i] |= s.len > 1 && s.max.is_infinite();
        }
        if let Some(b) = starts.get_mut(s.start) {
            *b = true;
        }
        if s.len == 1
            && let Some(k) = t.get_mut(s.start)
        {
            k.used = true;
            k.min = k.min.max(s.min);
            k.pref = k.pref.max(s.pref);
            k.max = k.max.max(s.max);
            k.stretch = k.stretch.max(s.stretch);
        }
    }
    // A run of columns with no item of their own is one track, kept in the
    // run's first column. A column where a spanning item starts begins a
    // new run, so that two such items side by side do not overlap.
    let own: Vec<bool> = t.iter().map(|k| k.used).collect();
    let mut run: Option<usize> = None;
    for i in 0..count {
        if own[i] {
            run = None;
        } else if covered[i] {
            let first = match run {
                Some(first) if !starts[i] => first,
                _ => {
                    t[i].used = true;
                    t[i].open = !starts[i];
                    run = Some(i);
                    i
                }
            };
            if unbounded[i] {
                t[first].max = f64::INFINITY;
            }
        }
    }
    // Short spans first: a long one then sees what they already asked for.
    let mut spanning: Vec<&Span> = items.iter().filter(|s| s.len > 1).collect();
    spanning.sort_by_key(|s| s.len);
    for s in spanning {
        let end = (s.start + s.len).min(count);
        let at: Vec<usize> = (s.start..end).filter(|i| t[*i].used).collect();
        if at.is_empty() {
            continue;
        }
        let gaps = spacing * (at.len() - 1) as f64;
        widen_pref(&mut t, &at, &own, s.pref - gaps);
        widen_min(&mut t, &at, s.min - gaps);
    }
    for k in &mut t {
        k.pref = k.pref.max(k.min);
        k.max = k.max.max(k.pref);
    }
    if uniform {
        let used = || t.iter().filter(|k| k.used);
        let min = used().map(|k| k.min).fold(0.0, f64::max);
        let pref = used().map(|k| k.pref).fold(0.0, f64::max);
        // The least generous item decides how far every cell may grow.
        let max = used().map(|k| k.max).fold(f64::INFINITY, f64::min).max(min);
        for k in t.iter_mut().filter(|k| k.used) {
            k.min = min;
            k.max = max;
            k.pref = pref.clamp(min, max);
        }
    }
    t
}

/// Makes the tracks `at` prefer `want` together: they grow as they would
/// when laid out in that room, except that a track with no item of its
/// `own` takes whatever is asked of it.
fn widen_pref(tracks: &mut [Track], at: &[usize], own: &[bool], want: f64) {
    let have: f64 = at.iter().map(|i| tracks[*i].pref).sum();
    if want <= have {
        return;
    }
    let part: Vec<Track> = at
        .iter()
        .map(|i| {
            let mut k = tracks[*i];
            if !own[*i] {
                k.max = f64::INFINITY;
            }
            k
        })
        .collect();
    let mut sizes: Vec<f64> = part.iter().map(|k| k.pref).collect();
    let left = fill_up(&part, &mut sizes, want - have);
    let n = at.len() as f64;
    for (i, size) in at.iter().zip(sizes) {
        // Nothing could take it (no track has a size at all): equal parts.
        tracks[*i].pref = size + left / n;
    }
}

/// Makes the tracks `at` at least `want` wide together. Each takes the
/// part of `want` its preferred size is of the whole, never less than its
/// own minimum and never more than its preferred size.
fn widen_min(tracks: &mut [Track], at: &[usize], want: f64) {
    let have: f64 = at.iter().map(|i| tracks[*i].min).sum();
    if want <= have {
        return;
    }
    let weight: f64 = at.iter().map(|i| tracks[*i].pref).sum();
    let n = at.len() as f64;
    for i in at {
        let k = &mut tracks[*i];
        let share = if weight > 0.0 {
            want * k.pref / weight
        } else {
            want / n
        };
        k.min = k.min.max(share).min(k.pref.max(k.min));
    }
}

/// The size of the whole axis: the used tracks and the spacing between
/// them.
pub(crate) fn total(tracks: &[Track], spacing: f64) -> Total {
    let used = tracks.iter().filter(|k| k.used).count();
    let gaps = spacing * used.saturating_sub(1) as f64;
    let sum = |f: fn(&Track) -> f64| tracks.iter().filter(|k| k.used).map(f).sum::<f64>() + gaps;
    Total {
        min: sum(|k| k.min),
        pref: sum(|k| k.pref),
        max: sum(|k| k.max),
    }
}

/// The size of every track when the axis is `size` long.
///
/// - No more room than the minimums: every track at its minimum (the
///   content sticks out).
/// - Less than the preferred sizes: tracks give up room between preferred
///   and minimum; the more a track can give, the larger its share.
/// - More: tracks that can grow share the surplus in proportion to their
///   preferred sizes, or by stretch factor if any is set, up to their
///   maximum. What is left when none can grow widens every cell in
///   proportion to its maximum, and the items are aligned inside.
pub(crate) fn distribute(tracks: &[Track], spacing: f64, size: f64) -> Vec<f64> {
    let used = || tracks.iter().filter(|k| k.used);
    let avail = size - spacing * used().count().saturating_sub(1) as f64;
    let min: f64 = used().map(|k| k.min).sum();
    let pref: f64 = used().map(|k| k.pref).sum();
    let value = |f: fn(&Track) -> f64| -> Vec<f64> {
        tracks
            .iter()
            .map(|k| if k.used { f(k) } else { 0.0 })
            .collect()
    };
    if avail <= min {
        return value(|k| k.min);
    }
    if avail < pref {
        return shrink(tracks, avail, min, pref);
    }
    let mut sizes = value(|k| k.pref);
    if avail > pref {
        fill_up(tracks, &mut sizes, avail - pref);
    }
    sizes
}

/// Adds `surplus` to `sizes`, which start at the preferred sizes. Returns
/// what no track could take.
fn fill_up(tracks: &[Track], sizes: &mut [f64], mut surplus: f64) -> f64 {
    let stretch = stretch_weights(tracks);
    if stretch.iter().any(|w| *w > 0.0) {
        surplus = grow(tracks, sizes, &stretch, surplus);
    }
    let even = tracks.iter().any(|k| k.used && k.open && k.max > k.pref);
    // A track with no preferred size still gets a share.
    let by_size: Vec<f64> = tracks
        .iter()
        .map(|k| if even { 1.0 } else { k.pref.max(1.0) })
        .collect();
    surplus = grow(tracks, sizes, &by_size, surplus);
    let max: f64 = tracks.iter().filter(|k| k.used).map(|k| k.max).sum();
    if surplus > 0.0 && max > 0.0 && max.is_finite() {
        for (s, k) in sizes.iter_mut().zip(tracks).filter(|(_, k)| k.used) {
            *s += surplus * k.max / max;
        }
        return 0.0;
    }
    surplus
}

/// Sizes between the minimums and the preferred sizes.
///
/// A track that can give `r` of the `R` everyone can give keeps
/// `r * t^(r / R)` of it, `t` being the share of `R` there is room for,
/// scaled so that the whole fits. Two tracks preferring 100 and 50 with no
/// minimum get 78 and 42 of 120, and 36 and 24 of 60, as in Qt.
fn shrink(tracks: &[Track], avail: f64, min: f64, pref: f64) -> Vec<f64> {
    let give = pref - min;
    let room = avail - min;
    let t = room / give;
    let kept: Vec<f64> = tracks
        .iter()
        .map(|k| {
            let r = if k.used { k.pref - k.min } else { 0.0 };
            if r > 0.0 { r * t.powf(r / give) } else { 0.0 }
        })
        .collect();
    let sum: f64 = kept.iter().sum();
    tracks
        .iter()
        .zip(kept)
        .map(|(k, f)| {
            if !k.used {
                0.0
            } else if sum > 0.0 {
                k.min + room * f / sum
            } else {
                k.min
            }
        })
        .collect()
}

/// The share of the surplus each stretching track takes.
///
/// With factors `s` and preferred sizes `p`, and `q` the largest `p / s`,
/// a track weighs `3 s q - 2 p`. That reproduces Qt for two tracks (50 and
/// 50 with factors 1 and 2 share 180 as 66 and 114); with three or more
/// tracks of different ratios Qt is not linear and Ljós only comes close.
fn stretch_weights(tracks: &[Track]) -> Vec<f64> {
    let stretching = |k: &Track| k.used && k.stretch > 0 && k.max > k.pref;
    let q = tracks
        .iter()
        .filter(|k| stretching(k))
        .map(|k| k.pref / f64::from(k.stretch))
        .fold(0.0, f64::max);
    tracks
        .iter()
        .map(|k| {
            if !stretching(k) {
                0.0
            } else if q > 0.0 {
                3.0 * f64::from(k.stretch) * q - 2.0 * k.pref
            } else {
                f64::from(k.stretch)
            }
        })
        .collect()
}

/// Shares `surplus` by `weights` among tracks below their maximum; a track
/// that reaches it drops out and the others share the rest. Returns what
/// nobody could take.
fn grow(tracks: &[Track], sizes: &mut [f64], weights: &[f64], mut surplus: f64) -> f64 {
    let mut active: Vec<usize> = (0..tracks.len())
        .filter(|i| tracks[*i].used && weights[*i] > 0.0 && tracks[*i].max > sizes[*i])
        .collect();
    while surplus > 0.0 && !active.is_empty() {
        let weight: f64 = active.iter().map(|i| weights[*i]).sum();
        let full: Vec<usize> = active
            .iter()
            .copied()
            .filter(|i| sizes[*i] + surplus * weights[*i] / weight >= tracks[*i].max)
            .collect();
        if full.is_empty() {
            for i in &active {
                sizes[*i] += surplus * weights[*i] / weight;
            }
            return 0.0;
        }
        for i in &full {
            surplus -= tracks[*i].max - sizes[*i];
            sizes[*i] = tracks[*i].max;
        }
        active.retain(|i| !full.contains(i));
    }
    surplus.max(0.0)
}

/// Where every track starts and ends, on whole pixels.
pub(crate) fn edges(tracks: &[Track], sizes: &[f64], spacing: f64) -> Vec<(f64, f64)> {
    let mut at = 0.0;
    tracks
        .iter()
        .zip(sizes)
        .map(|(k, size)| {
            if !k.used {
                return (round_half_up(at), round_half_up(at));
            }
            let edge = (round_half_up(at), round_half_up(at + size));
            at += size + spacing;
            edge
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill(min: f64, pref: f64, max: f64) -> Track {
        Track {
            min,
            pref,
            max,
            stretch: -1,
            used: true,
            open: false,
        }
    }

    fn fixed(size: f64) -> Track {
        fill(size, size, size)
    }

    fn rounded(tracks: &[Track], spacing: f64, size: f64) -> Vec<(f64, f64)> {
        edges(tracks, &distribute(tracks, spacing, size), spacing)
    }

    #[test]
    fn surplus_goes_to_filling_tracks_by_preferred_size() {
        let t = [
            fill(0.0, 100.0, f64::INFINITY),
            fill(0.0, 50.0, f64::INFINITY),
        ];
        assert_eq!(rounded(&t, 5.0, 400.0), vec![(0.0, 263.0), (268.0, 400.0)]);
    }

    #[test]
    fn a_track_stops_at_its_maximum() {
        let t = [fill(0.0, 50.0, 80.0), fill(0.0, 0.0, f64::INFINITY)];
        assert_eq!(rounded(&t, 5.0, 300.0), vec![(0.0, 80.0), (85.0, 300.0)]);
    }

    #[test]
    fn fixed_tracks_widen_their_cells_by_maximum() {
        let t = [fixed(40.0), fixed(20.0), fixed(30.0)];
        assert_eq!(
            rounded(&t, 5.0, 300.0),
            vec![(0.0, 129.0), (134.0, 198.0), (203.0, 300.0)]
        );
    }

    #[test]
    fn shrinking_takes_more_from_the_track_that_can_give_more() {
        let t = [
            fill(0.0, 100.0, f64::INFINITY),
            fill(0.0, 50.0, f64::INFINITY),
        ];
        for (size, a) in [(120.0, 78.0), (100.0, 64.0), (60.0, 36.0), (20.0, 10.0)] {
            assert_eq!(rounded(&t, 0.0, size), vec![(0.0, a), (a, size)], "{size}");
        }
    }

    #[test]
    fn below_the_minimum_tracks_keep_it() {
        let t = [fixed(80.0), fill(40.0, 60.0, f64::INFINITY)];
        assert_eq!(distribute(&t, 5.0, 50.0), vec![80.0, 40.0]);
    }

    #[test]
    fn stretch_factors_weigh_the_surplus() {
        let pair = |a: i32, b: i32| {
            let mut t = [
                fill(0.0, 50.0, f64::INFINITY),
                fill(0.0, 50.0, f64::INFINITY),
            ];
            t[0].stretch = a;
            t[1].stretch = b;
            rounded(&t, 0.0, 180.0)[0].1
        };
        assert_eq!(pair(1, 1), 90.0);
        assert_eq!(pair(1, 2), 66.0);
        assert_eq!(pair(1, 3), 60.0);
        assert_eq!(pair(2, 3), 73.0);
        assert_eq!(pair(1, -1), 130.0);
        assert_eq!(pair(0, 2), 50.0);
    }

    #[test]
    fn a_spanning_item_widens_its_tracks_by_preferred_size() {
        let span = |start, len, size| Span {
            start,
            len,
            min: size,
            pref: size,
            max: size,
            stretch: -1,
        };
        let t = tracks(
            2,
            &[span(0, 1, 10.0), span(1, 1, 30.0), span(0, 2, 50.0)],
            0.0,
            false,
        );
        assert_eq!((t[0].pref, t[1].pref), (12.5, 37.5));
        assert_eq!((t[0].min, t[1].min), (12.5, 37.5));
    }

    #[test]
    fn uniform_cells_take_the_least_generous_maximum() {
        let span = |start, min, pref, max| Span {
            start,
            len: 1,
            min,
            pref,
            max,
            stretch: -1,
        };
        let t = tracks(
            3,
            &[
                span(0, 0.0, 10.0, f64::INFINITY),
                span(1, 20.0, 20.0, 20.0),
                span(2, 0.0, 30.0, f64::INFINITY),
            ],
            0.0,
            true,
        );
        assert_eq!(total(&t, 0.0).pref, 60.0);
        assert_eq!(
            rounded(&t, 0.0, 300.0),
            vec![(0.0, 100.0), (100.0, 200.0), (200.0, 300.0)]
        );
    }

    #[test]
    fn an_unused_track_takes_no_size_and_no_spacing() {
        let t = [fixed(10.0), Track::EMPTY, fixed(10.0)];
        assert_eq!(total(&t, 5.0).pref, 25.0);
        assert_eq!(
            rounded(&t, 5.0, 25.0),
            vec![(0.0, 10.0), (15.0, 15.0), (15.0, 25.0)]
        );
    }

    /// A single item filling (`max` infinite) or fixed at `pref`.
    fn item(start: usize, pref: f64, fills: bool) -> Span {
        Span {
            start,
            len: 1,
            min: if fills { 0.0 } else { pref },
            pref,
            max: if fills { f64::INFINITY } else { pref },
            stretch: -1,
        }
    }

    fn wide(start: usize, len: usize, size: f64) -> Span {
        Span {
            start,
            len,
            min: size,
            pref: size,
            max: size,
            stretch: -1,
        }
    }

    fn prefs(t: &[Track]) -> Vec<f64> {
        t.iter().map(|k| (k.pref * 100.0).round() / 100.0).collect()
    }

    #[test]
    fn a_spanning_item_grows_the_tracks_that_can_grow() {
        // Only the filling column takes what the wide item needs.
        let t = tracks(
            2,
            &[item(0, 10.0, true), item(1, 30.0, false), wide(0, 2, 100.0)],
            0.0,
            false,
        );
        assert_eq!(prefs(&t), vec![70.0, 30.0]);
        // Two filling columns share it by preferred size around a fixed one.
        let t = tracks(
            3,
            &[
                item(0, 10.0, true),
                item(1, 30.0, false),
                item(2, 20.0, true),
                wide(0, 3, 200.0),
            ],
            0.0,
            false,
        );
        assert_eq!(prefs(&t), vec![56.67, 30.0, 113.33]);
        // A maximum stops a column; the other takes the rest.
        let mut capped = item(1, 30.0, true);
        capped.max = 50.0;
        let t = tracks(
            2,
            &[item(0, 10.0, true), capped, wide(0, 2, 100.0)],
            0.0,
            false,
        );
        assert_eq!(prefs(&t), vec![50.0, 50.0]);
    }

    #[test]
    fn columns_only_a_spanning_item_covers_are_one_track() {
        // Columns 1 and 2 hold nothing of their own: one track, one gap,
        // and what the wide item needs is shared evenly.
        let t = tracks(3, &[item(0, 10.0, true), wide(0, 3, 100.0)], 5.0, false);
        assert_eq!(prefs(&t), vec![52.5, 42.5, 0.0]);
        assert!(t[1].used && !t[2].used);
        assert_eq!(total(&t, 5.0).pref, 100.0);
        // A small spanning item adds one gap, however many columns it has.
        let t = tracks(4, &[item(0, 10.0, false), wide(0, 4, 5.0)], 5.0, false);
        assert_eq!(total(&t, 5.0).pref, 15.0);
        // Alone it is just as wide as it is.
        let t = tracks(3, &[wide(0, 3, 3.0)], 5.0, false);
        assert_eq!(total(&t, 5.0).pref, 3.0);
        // Fixed columns leave everything to the one in between.
        let t = tracks(
            3,
            &[
                item(0, 10.0, false),
                item(2, 20.0, false),
                wide(0, 3, 100.0),
            ],
            0.0,
            false,
        );
        assert_eq!(prefs(&t), vec![10.0, 70.0, 20.0]);
    }

    #[test]
    fn a_column_where_a_spanning_item_starts_is_a_track_of_its_own() {
        // The short item starts in column 1, which then shares like a
        // column that prefers no width: by weight 1 against 10 and 20.
        let t = tracks(
            3,
            &[
                item(0, 10.0, true),
                item(2, 20.0, true),
                wide(0, 3, 200.0),
                wide(1, 2, 1.0),
            ],
            0.0,
            false,
        );
        assert_eq!(prefs(&t), vec![64.84, 5.48, 129.68]);
    }

    #[test]
    fn a_track_under_a_bounded_spanning_item_does_not_grow_further() {
        let items = [item(0, 10.0, true), wide(0, 2, 100.0)];
        let t = tracks(2, &items, 0.0, false);
        assert_eq!(prefs(&t), vec![55.0, 45.0]);
        assert_eq!(distribute(&t, 0.0, 300.0), vec![255.0, 45.0]);
        // Under a spanning item that fills, both grow by the same amount.
        let mut filling = wide(0, 2, 100.0);
        filling.max = f64::INFINITY;
        let t = tracks(2, &[item(0, 10.0, true), filling], 0.0, false);
        assert_eq!(distribute(&t, 0.0, 300.0), vec![155.0, 145.0]);
    }

    #[test]
    fn a_spanning_minimum_is_shared_by_preferred_size() {
        let mut big = wide(0, 2, 150.0);
        big.min = 100.0;
        big.max = f64::INFINITY;
        let t = tracks(
            2,
            &[item(0, 10.0, true), item(1, 30.0, true), big],
            0.0,
            false,
        );
        assert_eq!((t[0].min, t[1].min), (25.0, 75.0));
        assert_eq!(prefs(&t), vec![37.5, 112.5]);
        assert_eq!(rounded(&t, 0.0, 125.0), vec![(0.0, 33.0), (33.0, 125.0)]);
    }
}
