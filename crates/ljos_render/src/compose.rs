//! Compose (frame pipeline step 9) and hit testing.
//!
//! Compose turns the tree into an immutable [`FrameSnapshot`]: world
//! transforms, effective opacity, paint order, clip stack and culling. Hit
//! testing reads the same snapshot in reverse, so paint, hit-testing and
//! accessibility order cannot drift apart (section 7.1, "Paint order").

use std::sync::Arc;

use vello_cpu::kurbo::{Affine, Point, Rect, RoundedRect, Shape, Size};

use crate::display_list::DisplayList;
use crate::tree::{ClipSpec, Dirty, NodeId, RenderTree};

/// Index into [`FrameSnapshot::clips`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClipId(pub u32);

/// One clip in the snapshot's clip stack. `parent` links outward.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipEntry {
    /// Enclosing clip.
    pub parent: Option<ClipId>,
    /// Local-to-world transform of the clipping node.
    pub transform: Affine,
    /// Clip rectangle in the clipping node's local space.
    pub rect: Rect,
    /// Corner radii (tl, tr, br, bl).
    pub radii: [f32; 4],
    /// World-space bounding box, already intersected with every enclosing clip
    /// and the viewport. This is what culling and B0 use.
    pub bbox: Rect,
    /// True when `transform` is translate-and-scale only, i.e. `rect` maps to
    /// an axis-aligned world rectangle. B0 can only clip those exactly.
    pub axis_aligned: bool,
    /// The clip of one node's own drawing ([`RenderTree::set_content_clip`]):
    /// only that node's item is in it, and hit testing passes over it.
    pub content: bool,
}

impl ClipEntry {
    /// True when this clip is rounded.
    pub fn is_rounded(&self) -> bool {
        self.radii.iter().any(|r| *r > 0.0)
    }

    /// Does the world point survive this single clip (not its ancestors)?
    pub fn contains(&self, p: Point) -> bool {
        let local = self.transform.inverse() * p;
        if !self.is_rounded() {
            return rect_contains(self.rect, local);
        }
        if !rect_contains(self.rect, local) {
            return false;
        }
        let rr = RoundedRect::from_rect(
            self.rect,
            (
                f64::from(self.radii[0]),
                f64::from(self.radii[1]),
                f64::from(self.radii[2]),
                f64::from(self.radii[3]),
            ),
        );
        rr.contains(local)
    }
}

fn rect_contains(r: Rect, p: Point) -> bool {
    p.x >= r.x0 && p.x < r.x1 && p.y >= r.y0 && p.y < r.y1
}

/// True for translate and non-negative-scale only transforms.
pub fn is_axis_aligned(a: Affine) -> bool {
    let c = a.as_coeffs();
    c[1] == 0.0 && c[2] == 0.0
}

/// `a.transform_rect_bbox(r)`, without the four corners when `a` only
/// scales and moves: what nearly every node in a scene is.
#[inline]
pub(crate) fn bbox_of(a: Affine, r: Rect) -> Rect {
    let [sx, ky, kx, sy, tx, ty] = a.as_coeffs();
    if ky != 0.0 || kx != 0.0 {
        return a.transform_rect_bbox(r);
    }
    let (x0, x1) = (sx * r.x0 + tx, sx * r.x1 + tx);
    let (y0, y1) = (sy * r.y0 + ty, sy * r.y1 + ty);
    Rect::new(x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1))
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x0 <= b.x1 && b.x0 <= a.x1 && a.y0 <= b.y1 && b.y0 <= a.y1
}

/// One painted node, in paint order.
#[derive(Clone, Debug)]
pub struct PaintItem {
    /// The node.
    pub node: NodeId,
    /// Local-to-world transform.
    pub world: Affine,
    /// Opacity multiplied down from the root.
    pub opacity: f32,
    /// Node size (the hit rectangle is `(0, 0, w, h)`).
    pub size: Size,
    /// Display list in local coordinates, `None` for content-less nodes.
    pub list: Option<Arc<DisplayList>>,
    /// Innermost clip, including the node's own.
    pub clip: Option<ClipId>,
    /// Whether hit testing may return the node.
    pub hit_testable: bool,
}

/// Counters from one compose, for tests and `LJOS_VISUALIZE=changes`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ComposeStats {
    /// Nodes entered (not pruned, not culled before entering).
    pub visited: u32,
    /// Subtrees skipped because `visible: false` or opacity 0.
    pub pruned: u32,
    /// Subtrees skipped because they lie outside the viewport or a clip.
    pub culled: u32,
    /// Nodes whose world transform and effective opacity were recomputed.
    pub world_recomputed: u32,
    /// Subtrees copied from the previous snapshot because nothing in them
    /// changed. Their nodes count as visited, pruned or culled as before.
    pub reused: u32,
}

/// Immutable output of steps 8-9. The seam where a render thread would attach.
#[derive(Debug)]
pub struct FrameSnapshot {
    /// Frame number given to the compose that built this snapshot. When compose
    /// returns a reused snapshot (`Arc::ptr_eq`), this is the older frame.
    pub frame: u64,
    /// Viewport in world (logical pixel) coordinates.
    pub viewport: Rect,
    /// Nodes in paint order, back to front.
    pub items: Vec<PaintItem>,
    /// The clip stack.
    pub clips: Vec<ClipEntry>,
    /// What compose did.
    pub stats: ComposeStats,
}

impl FrameSnapshot {
    /// Node ids in paint order. Accessibility order uses the same sequence.
    pub fn order(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.items.iter().map(|i| i.node)
    }

    /// Look up a clip.
    pub fn clip(&self, id: ClipId) -> &ClipEntry {
        &self.clips[id.0 as usize]
    }

    /// Does `p` survive the whole clip chain starting at `clip`?
    pub fn clip_chain_contains(&self, clip: Option<ClipId>, p: Point) -> bool {
        let mut cur = clip;
        while let Some(id) = cur {
            let c = self.clip(id);
            if !c.contains(p) {
                return false;
            }
            cur = c.parent;
        }
        true
    }

    /// Total number of display-list ops, for stats.
    pub fn op_count(&self) -> usize {
        self.items
            .iter()
            .filter_map(|i| i.list.as_ref())
            .map(|l| l.ops().len())
            .sum()
    }
}

/// Nodes under `point` (world coordinates), topmost first.
///
/// Pruned and culled nodes are not in the snapshot, so they cannot be hit.
/// Clips are honoured exactly, rotated ones included.
pub fn hit_test(snapshot: &FrameSnapshot, point: Point) -> impl Iterator<Item = NodeId> + '_ {
    snapshot.items.iter().rev().filter_map(move |item| {
        if !item.hit_testable {
            return None;
        }
        let local = item.world.inverse() * point;
        let hit_rect = Rect::from_origin_size((0.0, 0.0), item.size);
        if !rect_contains(hit_rect, local) {
            return None;
        }
        // The clip of the node's own drawing is not of where it is.
        let clip = item
            .clip
            .map(|c| snapshot.clip(c))
            .filter(|c| c.content)
            .map_or(item.clip, |c| c.parent);
        snapshot
            .clip_chain_contains(clip, point)
            .then_some(item.node)
    })
}

/// The clip of a node's own drawing, `spec` inside the clip it sits in,
/// or the clip it sits in when it has none.
fn content_clip(
    walk: &mut Walk,
    here: &Parent,
    world: Affine,
    spec: Option<ClipSpec>,
) -> Option<ClipId> {
    let Some(spec) = spec else {
        return here.clip;
    };
    let wb = bbox_of(world, spec.rect);
    let (x0, y0) = (wb.x0.max(here.clip_bbox.x0), wb.y0.max(here.clip_bbox.y0));
    // Nothing of it is left: empty, not inverted.
    let bbox = Rect::new(
        x0,
        y0,
        wb.x1.min(here.clip_bbox.x1).max(x0),
        wb.y1.min(here.clip_bbox.y1).max(y0),
    );
    let id = ClipId(walk.clips.len() as u32);
    walk.clips.push(ClipEntry {
        parent: here.clip,
        transform: world,
        rect: spec.rect,
        radii: spec.radii,
        bbox,
        axis_aligned: is_axis_aligned(world),
        content: true,
    });
    Some(id)
}

/// Cached output of the last compose.
#[derive(Debug)]
pub(crate) struct CachedSnapshot {
    pub(crate) epoch: u64,
    viewport: Rect,
    snapshot: Arc<FrameSnapshot>,
}

struct Walk<'a> {
    items: Vec<PaintItem>,
    clips: Vec<ClipEntry>,
    stats: ComposeStats,
    frame: u64,
    /// The previous snapshot and the epoch it was composed at.
    old: Option<(&'a FrameSnapshot, u64)>,
    epoch: u64,
}

/// Where a node's subtree sat in a snapshot, and what it was composed
/// against there. A subtree with no change since, under the same world,
/// clip and `drawn`, is the same run of items, so it is copied.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Span {
    /// The epoch of the compose that wrote it, 0 for never.
    epoch: u64,
    items: u32,
    items_len: u32,
    clips: u32,
    clips_len: u32,
    /// The clip the subtree sat in.
    outer: Option<ClipId>,
    clip_bbox: Rect,
    drawn: bool,
    visited: u32,
    pruned: u32,
    culled: u32,
}

#[derive(Clone, Copy)]
struct Parent {
    world: Affine,
    stamp: u64,
    /// Whether the nodes below paint ([`RenderTree::set_drawn`]).
    drawn: bool,
    opacity: f32,
    clip: Option<ClipId>,
    clip_bbox: Rect,
}

impl RenderTree {
    /// Pipeline step 9. Cheap when nothing changed (the previous `Arc` comes
    /// back); otherwise O(visible nodes), because culled and pruned subtrees are
    /// skipped whole and unchanged subtrees reuse their cached world data.
    pub fn compose(&mut self, viewport: Rect, frame: u64) -> Arc<FrameSnapshot> {
        if let Some(c) = &self.cache
            && c.epoch == self.epoch
            && c.viewport == viewport
        {
            let snap = Arc::clone(&c.snapshot);
            // Still on screen this frame, so keep the LRU clock moving.
            for item in &snap.items {
                if let Some(n) = self.nodes.get_mut(item.node) {
                    n.last_visible_frame = frame;
                }
            }
            return snap;
        }

        let root = self.root;
        self.refresh_bounds(root);
        // About as many as last time: a frame that moves every node pays
        // for its list once, not for each doubling on the way to it.
        let prev = self.cache.take();
        let (items, clips) = prev
            .as_ref()
            .map_or((0, 0), |c| (c.snapshot.items.len(), c.snapshot.clips.len()));
        let mut walk = Walk {
            items: Vec::with_capacity(items),
            clips: Vec::with_capacity(clips),
            stats: ComposeStats::default(),
            frame,
            old: prev.as_ref().map(|c| (&*c.snapshot, c.epoch)),
            epoch: self.epoch,
        };
        let top = Parent {
            world: Affine::IDENTITY,
            stamp: 0,
            drawn: true,
            opacity: 1.0,
            clip: None,
            clip_bbox: viewport,
        };
        self.compose_node(root, &top, &mut walk);
        let snapshot = Arc::new(FrameSnapshot {
            frame,
            viewport,
            items: walk.items,
            clips: walk.clips,
            stats: walk.stats,
        });
        self.cache = Some(CachedSnapshot {
            epoch: self.epoch,
            viewport,
            snapshot: Arc::clone(&snapshot),
        });
        snapshot
    }

    /// What `node` and the nodes below it paint, in `node`'s own
    /// coordinates and cut to its bounds, as though it were the root: its
    /// own transform, opacity and visibility do not apply, those of the
    /// nodes below it do. What an effect that draws an item sees of it.
    /// Leaves the tree's cached snapshot and world data alone.
    pub fn compose_subtree(&mut self, node: NodeId) -> Option<FrameSnapshot> {
        let size = self.nodes.get(node)?.size;
        let viewport = Rect::from_origin_size((0.0, 0.0), size);
        let mut walk = Walk {
            items: Vec::new(),
            clips: Vec::new(),
            stats: ComposeStats::default(),
            frame: 0,
            old: None,
            epoch: 0,
        };
        let top = Parent {
            world: Affine::IDENTITY,
            stamp: 0,
            drawn: true,
            opacity: 1.0,
            clip: None,
            clip_bbox: viewport,
        };
        self.subtree_node(node, top, true, &mut walk);
        Some(FrameSnapshot {
            frame: 0,
            viewport,
            items: walk.items,
            clips: walk.clips,
            stats: walk.stats,
        })
    }

    fn subtree_node(&mut self, id: NodeId, parent: Parent, root: bool, walk: &mut Walk) {
        let Some(n) = self.nodes.get(id) else {
            return;
        };
        let (world, opacity) = match root {
            true => (parent.world, parent.opacity),
            false if n.is_pruned() => return,
            false => (parent.world * n.local, parent.opacity * n.opacity),
        };
        let mut here = Parent {
            world,
            stamp: 0,
            drawn: true,
            opacity,
            clip: parent.clip,
            clip_bbox: parent.clip_bbox,
        };
        if let Some(spec) = n.clip {
            let wb = bbox_of(world, spec.rect);
            let inter = Rect::new(
                wb.x0.max(parent.clip_bbox.x0),
                wb.y0.max(parent.clip_bbox.y0),
                wb.x1.min(parent.clip_bbox.x1),
                wb.y1.min(parent.clip_bbox.y1),
            );
            if inter.x0 > inter.x1 || inter.y0 > inter.y1 {
                return;
            }
            let cid = ClipId(walk.clips.len() as u32);
            walk.clips.push(ClipEntry {
                parent: parent.clip,
                transform: world,
                rect: spec.rect,
                radii: spec.radii,
                bbox: inter,
                axis_aligned: is_axis_aligned(world),
                content: false,
            });
            here.clip = Some(cid);
            here.clip_bbox = inter;
        }
        self.ensure_order(id);
        let n = &self.nodes[id];
        let order = n.paint_order.clone();
        let neg = n.neg_count;
        let item = PaintItem {
            node: id,
            world,
            opacity,
            size: n.size,
            list: n.content.clone(),
            clip: content_clip(walk, &here, world, n.content_clip),
            hit_testable: false,
        };
        for c in &order[..neg] {
            self.subtree_node(*c, here, false, walk);
        }
        walk.items.push(item);
        for c in &order[neg..] {
            self.subtree_node(*c, here, false, walk);
        }
    }

    fn compose_node(&mut self, id: NodeId, parent: &Parent, walk: &mut Walk) {
        let n = &self.nodes[id];
        if n.is_pruned() {
            walk.stats.pruned += 1;
            return;
        }
        let span = n.span;
        if let Some((old, at)) = walk.old
            && span.epoch == at
            && n.changed <= at
            && n.parent_stamp == parent.stamp
            && span.drawn == parent.drawn
            && span.outer.is_some() == parent.clip.is_some()
            && span.clip_bbox == parent.clip_bbox
        {
            self.reuse(id, old, span, parent, walk);
            return;
        }
        let (items, clips) = (walk.items.len(), walk.clips.len());
        let before = walk.stats;
        self.compose_fresh(id, parent, walk);
        self.nodes[id].span = Span {
            epoch: walk.epoch,
            items: items as u32,
            items_len: (walk.items.len() - items) as u32,
            clips: clips as u32,
            clips_len: (walk.clips.len() - clips) as u32,
            outer: parent.clip,
            clip_bbox: parent.clip_bbox,
            drawn: parent.drawn,
            visited: walk.stats.visited - before.visited,
            pruned: walk.stats.pruned - before.pruned,
            culled: walk.stats.culled - before.culled,
        };
    }

    /// Copy `id`'s subtree from the previous snapshot, its clips renumbered.
    fn reuse(
        &mut self,
        id: NodeId,
        old: &FrameSnapshot,
        span: Span,
        parent: &Parent,
        walk: &mut Walk,
    ) {
        let (from, len) = (span.clips, span.clips_len);
        let to = walk.clips.len() as u32;
        // A clip inside the run moves with it; any other is the one the
        // subtree sits in.
        let clip = |c: Option<ClipId>| match c {
            Some(ClipId(i)) if i.wrapping_sub(from) < len => Some(ClipId(i - from + to)),
            _ => parent.clip,
        };
        let items = span.items as usize..(span.items + span.items_len) as usize;
        let at = walk.items.len() as u32;
        for item in &old.items[items] {
            if let Some(n) = self.nodes.get_mut(item.node) {
                n.last_visible_frame = walk.frame;
            }
            walk.items.push(PaintItem {
                clip: clip(item.clip),
                ..item.clone()
            });
        }
        for c in &old.clips[from as usize..(from + len) as usize] {
            walk.clips.push(ClipEntry {
                parent: clip(c.parent),
                ..*c
            });
        }
        walk.stats.visited += span.visited;
        walk.stats.pruned += span.pruned;
        walk.stats.culled += span.culled;
        walk.stats.reused += 1;
        self.nodes[id].span = Span {
            epoch: walk.epoch,
            items: at,
            clips: to,
            outer: parent.clip,
            ..span
        };
    }

    fn compose_fresh(&mut self, id: NodeId, parent: &Parent, walk: &mut Walk) {
        // One look at the node for all of it: a frame that moves two
        // hundred bars composes each of them afresh.
        let n = &mut self.nodes[id];
        // Reuse the cached world data unless this node or an ancestor changed.
        let needs = n.dirty.intersects(Dirty::TRANSFORM | Dirty::OPACITY)
            || n.parent_stamp != parent.stamp
            || n.stamp == 0;
        if needs {
            n.world = parent.world * n.local;
            n.effective_opacity = parent.opacity * n.opacity;
            n.stamp = self.next_stamp;
            self.next_stamp += 1;
            n.parent_stamp = parent.stamp;
            n.dirty.remove(Dirty::TRANSFORM | Dirty::OPACITY);
            walk.stats.world_recomputed += 1;
        }
        n.dirty.remove(Dirty::VISIBLE | Dirty::LAYER);
        let world = n.world;
        let opacity = n.effective_opacity;
        let stamp = n.stamp;

        // Container-level cull: the subtree box must touch the viewport and every clip.
        let bbox = bbox_of(world, n.subtree_bounds);
        if !overlaps(bbox, parent.clip_bbox) {
            walk.stats.culled += 1;
            return;
        }

        let mut here = Parent {
            world,
            stamp,
            drawn: parent.drawn && n.drawn,
            opacity,
            clip: parent.clip,
            clip_bbox: parent.clip_bbox,
        };
        if let Some(spec) = n.clip {
            let aligned = is_axis_aligned(world);
            let wb = bbox_of(world, spec.rect);
            let inter = Rect::new(
                wb.x0.max(parent.clip_bbox.x0),
                wb.y0.max(parent.clip_bbox.y0),
                wb.x1.min(parent.clip_bbox.x1),
                wb.y1.min(parent.clip_bbox.y1),
            );
            n.dirty.remove(Dirty::CLIP);
            if inter.x0 > inter.x1 || inter.y0 > inter.y1 {
                walk.stats.culled += 1;
                return;
            }
            let cid = ClipId(walk.clips.len() as u32);
            walk.clips.push(ClipEntry {
                parent: parent.clip,
                transform: world,
                rect: spec.rect,
                radii: spec.radii,
                bbox: inter,
                axis_aligned: aligned,
                content: false,
            });
            here.clip = Some(cid);
            here.clip_bbox = inter;
        } else {
            n.dirty.remove(Dirty::CLIP);
        }

        walk.stats.visited += 1;
        n.last_visible_frame = walk.frame;

        // A node with no children has no order to keep or walk.
        if n.children.is_empty() && n.paint_order.is_empty() {
            n.dirty.remove(Dirty::ORDER);
            let clip = content_clip(walk, &here, world, n.content_clip);
            walk.items.push(PaintItem {
                node: id,
                world,
                opacity,
                size: n.size,
                list: n.content.clone().filter(|_| here.drawn),
                clip,
                hit_testable: n.hit_testable,
            });
            return;
        }

        self.ensure_order(id);
        let order = std::mem::take(&mut self.nodes[id].paint_order);
        let neg = self.nodes[id].neg_count;
        for c in &order[..neg] {
            self.compose_node(*c, &here, walk);
        }
        let n = &self.nodes[id];
        let clip = content_clip(walk, &here, world, n.content_clip);
        walk.items.push(PaintItem {
            node: id,
            world,
            opacity,
            size: n.size,
            list: n.content.clone().filter(|_| here.drawn),
            clip,
            hit_testable: n.hit_testable,
        });
        for c in &order[neg..] {
            self.compose_node(*c, &here, walk);
        }
        self.nodes[id].paint_order = order;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ClipSpec, DisplayListBuilder, Rgba};

    /// Two trees take the same changes; one composes from its previous
    /// snapshot, the other from nothing. They must agree item for item.
    #[test]
    fn a_snapshot_made_from_the_last_is_the_one_made_from_nothing() {
        let mut seed = 0x2545_f491_u64;
        let mut next = move |n: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % n
        };
        let mut trees = [RenderTree::new(), RenderTree::new()];
        let mut ids = vec![trees[0].root()];
        for i in 0..40 {
            let parent = ids[next(ids.len() as u64) as usize];
            for t in &mut trees {
                let n = t.add_child(parent);
                t.set_pos(n, (i % 7) as f64 * 9.0, (i % 5) as f64 * 11.0);
                t.set_size(n, Size::new(20.0, 15.0));
            }
            ids.push(
                trees[1]
                    .node(parent)
                    .unwrap()
                    .children()
                    .last()
                    .copied()
                    .unwrap(),
            );
        }
        let list = {
            let mut b = DisplayListBuilder::new();
            b.fill_rect(Rect::new(0.0, 0.0, 8.0, 8.0), Rgba::BLACK);
            b.finish_arc()
        };
        let mut reused = 0;
        for frame in 1..400 {
            let changes = 1 + next(3);
            for _ in 0..changes {
                let id = ids[1 + next(ids.len() as u64 - 1) as usize];
                let (k, v) = (next(9), next(100) as f64);
                for t in &mut trees {
                    match k {
                        0 => t.set_pos(id, v, v / 2.0),
                        1 => t.set_opacity(id, (v / 50.0) as f32),
                        2 => t.set_visible(id, v > 20.0),
                        3 => t.set_clip(
                            id,
                            (v > 50.0).then(|| ClipSpec::rect(Rect::new(0.0, 0.0, v, v))),
                        ),
                        4 => t.set_z(id, (v as f32 - 50.0) / 25.0),
                        5 => t.set_drawn(id, v > 30.0),
                        6 => t.set_content(id, (v > 40.0).then(|| Arc::clone(&list))),
                        7 => t.set_local(
                            id,
                            Affine::rotate(v / 30.0).then_translate((v, 3.0).into()),
                        ),
                        _ => t.set_hit_testable(id, v > 50.0),
                    }
                }
            }
            let vp = if frame % 50 < 45 {
                Rect::new(0.0, 0.0, 100.0, 80.0)
            } else {
                Rect::new(5.0, 0.0, 90.0, 90.0)
            };
            trees[1].cache = None;
            let a = trees[0].compose(vp, frame);
            let b = trees[1].compose(vp, frame);
            reused += a.stats.reused;
            assert_eq!(a.items.len(), b.items.len(), "frame {frame}");
            for (x, y) in a.items.iter().zip(&b.items) {
                assert_eq!(
                    (x.node, x.world, x.opacity, x.size, x.clip, x.hit_testable),
                    (y.node, y.world, y.opacity, y.size, y.clip, y.hit_testable),
                    "frame {frame}"
                );
                assert_eq!(
                    x.list.as_ref().map(Arc::as_ptr),
                    y.list.as_ref().map(Arc::as_ptr)
                );
            }
            assert_eq!(a.clips, b.clips, "frame {frame}");
            let s = |s: ComposeStats| (s.visited, s.pruned, s.culled);
            assert_eq!(s(a.stats), s(b.stats), "frame {frame}");
            for id in &ids {
                assert_eq!(
                    trees[0].node(*id).unwrap().last_visible_frame(),
                    trees[1].node(*id).unwrap().last_visible_frame()
                );
            }
        }
        assert!(reused > 1000, "{reused} subtrees reused");
    }
}
