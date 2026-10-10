//! Compose (frame pipeline step 9) and hit testing.
//!
//! Compose turns the tree into an immutable [`FrameSnapshot`]: world
//! transforms, effective opacity, paint order, clip stack and culling. Hit
//! testing reads the same snapshot in reverse, so paint, hit-testing and
//! accessibility order cannot drift apart (section 7.1, "Paint order").

use std::sync::Arc;

use vello_cpu::kurbo::{Affine, Point, Rect, RoundedRect, Shape, Size};

use crate::display_list::DisplayList;
use crate::tree::{Dirty, NodeId, RenderTree};

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
        snapshot
            .clip_chain_contains(item.clip, point)
            .then_some(item.node)
    })
}

/// Cached output of the last compose.
#[derive(Debug)]
pub(crate) struct CachedSnapshot {
    epoch: u64,
    viewport: Rect,
    snapshot: Arc<FrameSnapshot>,
}

struct Walk {
    items: Vec<PaintItem>,
    clips: Vec<ClipEntry>,
    stats: ComposeStats,
    frame: u64,
}

#[derive(Clone, Copy)]
struct Parent {
    world: Affine,
    stamp: u64,
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
        let mut walk = Walk {
            items: Vec::new(),
            clips: Vec::new(),
            stats: ComposeStats::default(),
            frame,
        };
        let top = Parent {
            world: Affine::IDENTITY,
            stamp: 0,
            opacity: 1.0,
            clip: None,
            clip_bbox: viewport,
        };
        self.compose_node(root, top, &mut walk);
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

    fn compose_node(&mut self, id: NodeId, parent: Parent, walk: &mut Walk) {
        if self.nodes[id].is_pruned() {
            walk.stats.pruned += 1;
            return;
        }

        // Reuse the cached world data unless this node or an ancestor changed.
        let n = &self.nodes[id];
        let needs = n.dirty.intersects(Dirty::TRANSFORM | Dirty::OPACITY)
            || n.parent_stamp != parent.stamp
            || n.stamp == 0;
        if needs {
            let stamp = self.next_stamp;
            self.next_stamp += 1;
            let n = &mut self.nodes[id];
            n.world = parent.world * n.local;
            n.effective_opacity = parent.opacity * n.opacity;
            n.stamp = stamp;
            n.parent_stamp = parent.stamp;
            n.dirty.remove(Dirty::TRANSFORM | Dirty::OPACITY);
            walk.stats.world_recomputed += 1;
        }
        let n = &mut self.nodes[id];
        n.dirty.remove(Dirty::VISIBLE | Dirty::LAYER);
        let world = n.world;
        let opacity = n.effective_opacity;
        let stamp = n.stamp;

        // Container-level cull: the subtree box must touch the viewport and every clip.
        let bbox = world.transform_rect_bbox(n.subtree_bounds);
        if !overlaps(bbox, parent.clip_bbox) {
            walk.stats.culled += 1;
            return;
        }

        let mut here = Parent {
            world,
            stamp,
            opacity,
            clip: parent.clip,
            clip_bbox: parent.clip_bbox,
        };
        if let Some(spec) = n.clip {
            let aligned = is_axis_aligned(world);
            let wb = world.transform_rect_bbox(spec.rect);
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
            });
            here.clip = Some(cid);
            here.clip_bbox = inter;
        } else {
            n.dirty.remove(Dirty::CLIP);
        }

        walk.stats.visited += 1;
        n.last_visible_frame = walk.frame;

        self.ensure_order(id);
        let order = std::mem::take(&mut self.nodes[id].paint_order);
        let neg = self.nodes[id].neg_count;
        for c in &order[..neg] {
            self.compose_node(*c, here, walk);
        }
        let n = &self.nodes[id];
        walk.items.push(PaintItem {
            node: id,
            world,
            opacity,
            size: n.size,
            list: n.content.clone(),
            clip: here.clip,
            hit_testable: n.hit_testable,
        });
        for c in &order[neg..] {
            self.compose_node(*c, here, walk);
        }
        self.nodes[id].paint_order = order;
    }
}
