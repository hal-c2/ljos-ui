//! Which of a frame's draws are drawn together.
//!
//! A thousand cubes that differ only in where they are and in their colour
//! are one mesh drawn a thousand times with the same pipeline, maps and
//! uniforms but for those two. Drawn one by one, each costs a bind group,
//! buffers and a draw call to submit; drawn as the copies of one instanced
//! draw, with each one's place and colour in its copy's row, they cost one.
//! What is drawn together is decided by a key the caller gives each draw:
//! the same key, the same draw. A draw with no key is drawn on its own.

use std::hash::Hash;

use rustc_hash::FxHashMap;

use crate::math::{Mat4, Vec3};

/// What a frame's batching works in, kept from one frame to the next: the
/// lists of a thousand draws are made once, and each frame's in the room
/// the last one left. A key may stand for more than draws alike, as a hash
/// does: a draw is put with the first of its key's group only where
/// `alike` says the two are, and drawn alone where not.
pub(crate) struct Batching<K> {
    extents: Vec<f32>,
    first_of: FxHashMap<K, usize>,
    index: Index,
    /// The first draw of the group each draw is drawn in.
    first: Vec<usize>,
    /// The draws group by group, the groups where their first draws are
    /// and each one's draws in the order they came.
    members: Vec<usize>,
    /// Where each group begins in `members`, and past the last, its end.
    starts: Vec<usize>,
}

impl<K> Default for Batching<K> {
    fn default() -> Self {
        Batching {
            extents: Vec::new(),
            first_of: FxHashMap::default(),
            index: Index::new(1.0),
            first: Vec::new(),
            members: Vec::new(),
            starts: Vec::new(),
        }
    }
}

/// The box a model takes in the scene, which none of its points is
/// outside: where two models may meet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Reach {
    least: [f32; 3],
    most: [f32; 3],
    /// Whether the model is a closed surface round what its box holds, as
    /// a cube is: what is on its box's face is then seen from outside only
    /// where no other model's box is against it.
    solid: bool,
}

impl Reach {
    /// The box `(least, most)` of a mesh in model space, placed by `m`.
    pub(crate) fn placed(m: &Mat4, (least, most): (Vec3, Vec3), solid: bool) -> Reach {
        let mut r = Reach {
            least: [f32::INFINITY; 3],
            most: [f32::NEG_INFINITY; 3],
            solid,
        };
        for corner in 0..8 {
            let pick = |bit: u32, lo: f32, hi: f32| if corner & bit == 0 { lo } else { hi };
            let p = Vec3::new(
                pick(1, least.x, most.x),
                pick(2, least.y, most.y),
                pick(4, least.z, most.z),
            );
            let p = m.transform_point(p);
            for (axis, v) in [p.x, p.y, p.z].into_iter().enumerate() {
                r.least[axis] = r.least[axis].min(v);
                r.most[axis] = r.most[axis].max(v);
            }
        }
        r
    }

    /// The box round spheres, `(centre, radius)`: no closed surface's.
    pub(crate) fn round(spheres: impl IntoIterator<Item = (Vec3, f32)>) -> Reach {
        let mut r = Reach {
            least: [f32::INFINITY; 3],
            most: [f32::NEG_INFINITY; 3],
            solid: false,
        };
        for (c, radius) in spheres {
            for (axis, v) in [c.x, c.y, c.z].into_iter().enumerate() {
                r.least[axis] = r.least[axis].min(v - radius);
                r.most[axis] = r.most[axis].max(v + radius);
            }
        }
        r
    }

    /// Whether the two may show a point in one place, where the depth test
    /// lets the one drawn last be seen. Two closed surfaces whose boxes only
    /// touch meet where neither is seen, inside what they hold together; a
    /// flat box, as a rectangle's, is all face and meets what it touches.
    pub(crate) fn meets(&self, o: &Reach) -> bool {
        (0..3).all(|axis| {
            let (a0, a1, b0, b1) = (
                self.least[axis],
                self.most[axis],
                o.least[axis],
                o.most[axis],
            );
            // A box that is no number is one nothing can be told apart from.
            if ![a0, a1, b0, b1].iter().all(|v| v.is_finite()) {
                return true;
            }
            let flat = a0 == a1 || b0 == b1;
            if self.solid && o.solid && !flat {
                a0 < b1 && b0 < a1
            } else {
                a0 <= b1 && b0 <= a1
            }
        })
    }

    fn extent(&self) -> f32 {
        (0..3)
            .map(|a| self.most[a] - self.least[a])
            .fold(0.0, f32::max)
    }
}

impl<K: Eq + Hash + Copy> Batching<K> {
    /// The first draw of the group each draw is drawn in, by index: a draw
    /// is put with the first of its key's group where `alike` says the two
    /// are. A group is drawn where its first draw is, so the draws it takes
    /// in come before those between them, and its rows come in their
    /// declaration order (`order`), which may not be the order they came
    /// in. Where two models that come in one order may meet
    /// ([`Reach::meets`]), the depth test shows the later where they are as
    /// deep: a draw that would be drawn before one it came after and may
    /// meet starts a group of its own key, and the other groups go on. A
    /// draw with no key is drawn alone and may be the one seen where it is,
    /// so no group reaches across it.
    pub(crate) fn firsts(
        &mut self,
        keys: &[Option<K>],
        alike: impl Fn(usize, usize) -> bool,
        reach: &[Reach],
        order: &[u32],
    ) -> &[usize] {
        let n = keys.len();
        // Cells about one model across, so a model is in a few and a cell
        // holds a few: the median keeps one large floor from setting it.
        let extents = &mut self.extents;
        extents.clear();
        extents.extend(
            (0..n)
                .filter(|&i| keys[i].is_some())
                .map(|i| reach[i].extent())
                .filter(|e| e.is_finite() && *e > 0.0),
        );
        // A power of two, so a scene that turns or grows a little keeps its
        // cells, and the room their lists have, from frame to frame: two
        // boxes that meet share a cell however large.
        let cell = if extents.is_empty() {
            1.0
        } else {
            let mid = extents.len() / 2;
            extents
                .select_nth_unstable_by(mid, f32::total_cmp)
                .1
                .log2()
                .round()
                .exp2()
        };
        let index = &mut self.index;
        index.begin(cell);
        let first_of = &mut self.first_of;
        first_of.clear();
        let group = &mut self.first;
        group.clear();
        group.extend(0..n);
        for i in 0..n {
            let Some(key) = keys[i] else {
                // What came before it is looked at no more: each group's
                // first comes after it, and the index is read from there.
                first_of.clear();
                continue;
            };
            match first_of.get(&key).copied() {
                Some(f) if alike(f, i) => {
                    // What came before it and is drawn after it: one of
                    // its group declared later, or a group drawn after its
                    // own. A draw is in a group no later than its own, so
                    // only those since `f` can be either.
                    let reordered = index.any_near(&reach[i], f, |j| {
                        let ahead = if group[j] == f {
                            order[j] > order[i]
                        } else {
                            group[j] > f
                        };
                        ahead && reach[i].meets(&reach[j])
                    });
                    // Every pair it would turn round is one with a draw
                    // before it, all of which the index holds: one set
                    // apart leaves the others' groups whole.
                    if reordered {
                        first_of.insert(key, i);
                    } else {
                        group[i] = f;
                    }
                }
                Some(_) => {}
                None => {
                    first_of.insert(key, i);
                }
            }
            index.add(i, &reach[i]);
        }
        group
    }

    /// Puts the draws in the groups of the firsts [`Batching::firsts`]
    /// found, for [`Batching::members`]: the groups where their firsts
    /// are, each one's draws in the order they came.
    pub(crate) fn group(&mut self) {
        let n = self.first.len();
        // Counted at their first, then laid out in the firsts' order: no
        // list of its own for each group.
        self.starts.clear();
        self.starts.resize(n + 1, 0);
        for &f in &self.first {
            self.starts[f + 1] += 1;
        }
        // Where the group of each first begins; one of a draw that is no
        // group's first is empty.
        let mut at = 0;
        for f in 0..n {
            let count = self.starts[f + 1];
            self.starts[f + 1] = at;
            at += count;
        }
        self.members.clear();
        self.members.resize(n, 0);
        for (i, &f) in self.first.iter().enumerate() {
            let next = &mut self.starts[f + 1];
            self.members[*next] = i;
            *next += 1;
        }
        // `starts[f + 1]` is now where `f`'s group ends; those of the
        // draws that are no group's first are where the one before ended.
        let mut kept = 0;
        for f in 0..n {
            if self.first[f] == f {
                self.starts[kept + 1] = self.starts[f + 1];
                kept += 1;
            }
        }
        self.starts.truncate(kept + 1);
    }

    /// How many groups [`Batching::group`] put the draws in.
    pub(crate) fn len(&self) -> usize {
        self.starts.len().saturating_sub(1)
    }

    /// The draws of group `g`, which [`Batching::group`] put in the order
    /// they came; a caller may put them in another.
    pub(crate) fn members(&mut self, g: usize) -> &mut [usize] {
        &mut self.members[self.starts[g]..self.starts[g + 1]]
    }
}

/// A frame's draws by the cells of space their boxes are in; a box in more
/// cells than a few is looked at for every draw. The cells are kept from
/// frame to frame with the room their lists have, and a cell is the
/// frame's once a draw of it is added: the models of a scene whose camera
/// moves are in the cells they were in.
struct Index {
    cell: f32,
    /// Each cell's draws, and the frame they are of.
    cells: FxHashMap<[i64; 3], (u64, Vec<usize>)>,
    /// The frame the draws added are of.
    frame: u64,
    /// How many of the cells the frame added draws to.
    live: usize,
    large: Vec<usize>,
    all: Vec<usize>,
}

impl Index {
    /// Cells across, at most, of a box kept in its cells.
    const ACROSS: i64 = 4;

    fn new(cell: f32) -> Self {
        Index {
            cell,
            cells: FxHashMap::default(),
            // A cell made is of frame 0: of none.
            frame: 1,
            live: 0,
            large: Vec::new(),
            all: Vec::new(),
        }
    }

    /// Starts a frame's draws, in cells `cell` across: those of the frames
    /// before are left out.
    fn begin(&mut self, cell: f32) {
        if self.cell != cell {
            self.cells.clear();
        } else if self.cells.len() > 2 * self.live + 1024 {
            // Cells no frame adds to any more are let go of once they are
            // most of those kept: a scene that moves leaves cells behind.
            let last = self.frame;
            self.cells.retain(|_, (frame, _)| *frame == last);
        }
        self.cell = cell;
        self.frame += 1;
        self.live = 0;
        self.large.clear();
        self.all.clear();
    }

    /// The cells `r` is in, `None` for a box in too many to keep it in.
    fn span(&self, r: &Reach) -> Option<([i64; 3], [i64; 3])> {
        let mut lo = [0i64; 3];
        let mut hi = [0i64; 3];
        for axis in 0..3 {
            let (a, b) = (r.least[axis] / self.cell, r.most[axis] / self.cell);
            if !(a.is_finite() && b.is_finite()) {
                return None;
            }
            // Two boxes that meet have a point in common, whose cell is
            // in both: a touch on a cell's edge too.
            // Far out, the cells are i64's ends, which no span between
            // holds.
            (lo[axis], hi[axis]) = (a.floor() as i64, b.floor() as i64);
            if hi[axis]
                .checked_sub(lo[axis])
                .is_none_or(|d| d > Self::ACROSS)
            {
                return None;
            }
        }
        Some((lo, hi))
    }

    fn add(&mut self, i: usize, r: &Reach) {
        self.all.push(i);
        match self.span(r) {
            None => self.large.push(i),
            Some((lo, hi)) => {
                for x in lo[0]..=hi[0] {
                    for y in lo[1]..=hi[1] {
                        for z in lo[2]..=hi[2] {
                            let (frame, list) = self.cells.entry([x, y, z]).or_default();
                            if *frame != self.frame {
                                *frame = self.frame;
                                list.clear();
                                self.live += 1;
                            }
                            list.push(i);
                        }
                    }
                }
            }
        }
    }

    /// Whether `found` holds for one of the draws from `since` on whose
    /// boxes may meet `r`'s, which it is asked of from the last added, some
    /// more than once. Each list is in the order the draws were added, so
    /// it is read from its end and left at the first before `since`: what
    /// came long ago is not looked at again and again.
    fn any_near(&self, r: &Reach, since: usize, mut found: impl FnMut(usize) -> bool) -> bool {
        let mut from = |list: &[usize]| {
            list.iter()
                .rev()
                .copied()
                .take_while(|&j| j >= since)
                .any(&mut found)
        };
        let Some((lo, hi)) = self.span(r) else {
            return from(&self.all);
        };
        for x in lo[0]..=hi[0] {
            for y in lo[1]..=hi[1] {
                for z in lo[2]..=hi[2] {
                    if let Some((frame, list)) = self.cells.get(&[x, y, z])
                        && *frame == self.frame
                        && from(list)
                    {
                        return true;
                    }
                }
            }
        }
        from(&self.large)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Quat;

    fn firsts<K: Eq + Hash + Copy>(
        keys: &[Option<K>],
        alike: impl Fn(usize, usize) -> bool,
        reach: &[Reach],
        order: &[u32],
    ) -> Vec<usize> {
        Batching::default()
            .firsts(keys, alike, reach, order)
            .to_vec()
    }

    /// The groups of draws far apart, which come in their declaration
    /// order.
    fn groups<K: Eq + Hash + Copy>(
        keys: &[Option<K>],
        alike: impl Fn(usize, usize) -> bool,
    ) -> Vec<Vec<usize>> {
        let reach: Vec<Reach> = (0..keys.len()).map(|i| cube(9.0 * i as f32, 0.0)).collect();
        let order: Vec<u32> = (0..keys.len() as u32).collect();
        let mut batching = Batching::default();
        batching.firsts(keys, alike, &reach, &order);
        batching.group();
        (0..batching.len())
            .map(|g| batching.members(g).to_vec())
            .collect()
    }

    #[test]
    fn draws_with_one_key_are_one_group_where_the_first_of_them_is() {
        let (a, b) = (Some('a'), Some('b'));
        assert_eq!(
            groups(&[a, b, a, b, a], |_, _| true),
            [vec![0, 2, 4], vec![1, 3]]
        );
        assert_eq!(
            groups(&[a, b, a, None, b, None, a], |_, _| true),
            [vec![0, 2], vec![1], vec![3], vec![4], vec![5], vec![6]]
        );
    }

    #[test]
    fn the_room_a_frame_leaves_finds_the_next_s_groups_as_new_room_does() {
        // A row of cubes, every other one alike, which a frame later have
        // moved, some of them against others.
        let keys: Vec<Option<u8>> = (0..40).map(|i| Some((i % 3) as u8)).collect();
        let order: Vec<u32> = (0..40).rev().collect();
        let at = |shift: f32| -> Vec<Reach> {
            (0..40)
                .map(|i| cube(3.0 * i as f32 + shift * (i % 5) as f32, (i % 2) as f32))
                .collect()
        };
        let mut kept = Batching::default();
        for shift in [0.0, 0.7, 1.5, 0.0, 40.0] {
            let reach = at(shift);
            let found = kept.firsts(&keys, |_, _| true, &reach, &order).to_vec();
            assert_eq!(found, firsts(&keys, |_, _| true, &reach, &order), "{shift}");
        }
    }

    fn cube(x: f32, z: f32) -> Reach {
        Reach::placed(
            &Mat4::from_trs(Vec3::new(x, 0.0, z), Quat::IDENTITY, Vec3::ONE),
            (Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0)),
            true,
        )
    }

    fn square(x: f32, z: f32) -> Reach {
        Reach::placed(
            &Mat4::from_trs(Vec3::new(x, 0.0, z), Quat::IDENTITY, Vec3::ONE),
            (Vec3::new(-1.0, -1.0, 0.0), Vec3::new(1.0, 1.0, 0.0)),
            false,
        )
    }

    #[test]
    fn boxes_meet_where_they_share_more_than_a_face_of_two_solids() {
        assert!(cube(0.0, 0.0).meets(&cube(1.5, 0.0)));
        assert!(!cube(0.0, 0.0).meets(&cube(3.0, 0.0)));
        // Two cubes side by side: what they touch with is inside them.
        assert!(!cube(0.0, 0.0).meets(&cube(2.0, 0.0)));
        // A square on a cube's face is seen there.
        assert!(square(0.0, 1.0).meets(&cube(0.0, 0.0)));
        assert!(square(0.5, 0.0).meets(&square(0.0, 0.0)));
        // Nor is a box round spheres a closed surface.
        let ball = Reach::round([(Vec3::new(2.0, 0.0, 0.0), 1.0)]);
        assert!(ball.meets(&cube(0.0, 0.0)));
        let lost = Reach::round([(Vec3::new(f32::NAN, 0.0, 0.0), 1.0)]);
        assert!(lost.meets(&cube(100.0, 0.0)));
    }

    #[test]
    fn a_draw_that_would_go_before_one_it_meets_is_set_apart() {
        let keys = [Some('a'), Some('b'), Some('a')];
        // B and C as deep and side by side, overlapping on the screen;
        // A, alike C, nearer and to one side.
        let reach = [cube(-5.0, 2.0), cube(0.0, 0.0), cube(0.5, 0.0)];
        assert_eq!(firsts(&keys, |_, _| true, &reach, &[0, 1, 2]), [0, 1, 2]);
        // Apart from B, C joins A.
        let reach = [cube(-5.0, 2.0), cube(0.0, 0.0), cube(5.0, 0.0)];
        assert_eq!(firsts(&keys, |_, _| true, &reach, &[0, 1, 2]), [0, 1, 0]);
    }

    #[test]
    fn a_draw_set_apart_leaves_the_other_groups_whole() {
        // C is set apart from A as above; D, apart from all, still joins B.
        let keys = [Some('a'), Some('b'), Some('a'), Some('b')];
        let reach = [
            cube(-5.0, 2.0),
            cube(0.0, 0.0),
            cube(0.5, 0.0),
            cube(9.0, 0.0),
        ];
        assert_eq!(
            firsts(&keys, |_, _| true, &reach, &[0, 1, 2, 3]),
            [0, 1, 2, 1]
        );
    }

    #[test]
    fn a_draw_is_set_apart_where_a_group_s_rows_would_turn_two_that_meet_round() {
        // Alike, in the depth order 0, 1; declared 1, 0.
        let keys = [Some('a'), Some('a')];
        let reach = [cube(0.0, 0.0), cube(0.5, 0.0)];
        assert_eq!(firsts(&keys, |_, _| true, &reach, &[1, 0]), [0, 1]);
        assert_eq!(firsts(&keys, |_, _| true, &reach, &[0, 1]), [0, 0]);
        let reach = [cube(0.0, 0.0), cube(5.0, 0.0)];
        assert_eq!(firsts(&keys, |_, _| true, &reach, &[1, 0]), [0, 0]);
    }

    #[test]
    fn a_group_drawn_before_another_s_first_leaves_the_order_as_it_was() {
        // b's group is drawn first, so b's second still comes before a's.
        let keys = [Some('b'), Some('a'), Some('b'), Some('a')];
        let reach = [
            cube(9.0, 0.0),
            cube(-9.0, 0.0),
            cube(0.0, 0.0),
            cube(0.5, 0.0),
        ];
        assert_eq!(
            firsts(&keys, |_, _| true, &reach, &[0, 1, 2, 3]),
            [0, 1, 0, 1]
        );
    }

    #[test]
    fn a_large_box_is_looked_at_for_every_draw() {
        let floor = Reach::placed(
            &Mat4::IDENTITY,
            (
                Vec3::new(-100.0, -1.0, -100.0),
                Vec3::new(100.0, 1.0, 100.0),
            ),
            true,
        );
        let keys = [Some('a'), Some('b'), Some('a')];
        let reach = [cube(-50.0, 0.0), floor, cube(50.0, 0.0)];
        assert_eq!(firsts(&keys, |_, _| true, &reach, &[0, 1, 2]), [0, 1, 2]);
        // The floor's group is drawn first: the cubes stay after it.
        let keys = [Some('a'), Some('b'), Some('b')];
        let reach = [floor, cube(-50.0, 0.0), cube(-50.5, 0.0)];
        assert_eq!(firsts(&keys, |_, _| true, &reach, &[0, 1, 2]), [0, 1, 1]);
        let keys = [Some('b'), Some('a'), Some('b')];
        assert_eq!(firsts(&keys, |_, _| true, &reach, &[0, 1, 2]), [0, 1, 2]);
    }

    #[test]
    fn the_index_leaves_what_came_before_the_group_s_first() {
        let mut index = Index::new(2.0);
        let floor = Reach::placed(
            &Mat4::IDENTITY,
            (
                Vec3::new(-100.0, -1.0, -100.0),
                Vec3::new(100.0, 1.0, 100.0),
            ),
            true,
        );
        for i in 0..6 {
            let r = if i % 3 == 0 { floor } else { cube(0.0, 0.0) };
            index.add(i, &r);
        }
        let near = |r: &Reach, since: usize| {
            let mut near = Vec::new();
            index.any_near(r, since, |j| {
                near.push(j);
                false
            });
            near.sort_unstable();
            near.dedup();
            near
        };
        assert_eq!(near(&cube(0.0, 0.0), 2), [2, 3, 4, 5]);
        assert_eq!(near(&floor, 4), [4, 5]);
    }

    #[test]
    fn a_box_far_out_is_looked_at_for_every_draw() {
        // Its cells are i64's ends, a span no subtraction holds.
        let far = Reach::placed(
            &Mat4::IDENTITY,
            (Vec3::new(-1e30, -1.0, -1.0), Vec3::new(1e30, 1.0, 1.0)),
            true,
        );
        let keys = [Some('a'), Some('b'), Some('a')];
        let reach = [cube(-5.0, 0.0), far, cube(5.0, 0.0)];
        assert_eq!(firsts(&keys, |_, _| true, &reach, &[0, 1, 2]), [0, 1, 2]);
    }

    #[test]
    fn no_group_reaches_across_a_draw_without_a_key() {
        let keys = [Some('a'), None, Some('a')];
        let reach = [cube(0.0, 0.0), cube(9.0, 0.0), cube(-9.0, 0.0)];
        assert_eq!(firsts(&keys, |_, _| true, &reach, &[0, 1, 2]), [0, 1, 2]);
    }

    #[test]
    fn a_draw_without_a_key_is_drawn_alone() {
        assert_eq!(groups(&[None::<u8>, None], |_, _| true), [vec![0], vec![1]]);
    }

    #[test]
    fn a_draw_its_key_s_first_is_not_alike_is_drawn_alone() {
        let keys = [Some(1), Some(1), Some(1)];
        assert_eq!(groups(&keys, |a, b| a + b != 1), [vec![0, 2], vec![1]]);
    }
}
