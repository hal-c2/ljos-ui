//! The retained render tree (ARCHITECTURE.md section 7.1).
//!
//! The tree owns only render-relevant state. Every setter records the narrowest
//! [`Dirty`] flag that explains the change, so that composition and backends can
//! skip work: only `CONTENT` rebuilds a display list; transform, opacity, clip
//! and z changes only re-compose.

use std::sync::Arc;

use bitflags::bitflags;
use slotmap::SlotMap;
use smallvec::SmallVec;
use thiserror::Error;
use vello_cpu::kurbo::{Affine, Rect, Size};

use crate::compose::{CachedSnapshot, Span};
use crate::display_list::{DisplayList, DisplayListBuilder, PaintCx};

slotmap::new_key_type! {
    /// Stable handle to a [`RenderNode`] (index plus generation).
    pub struct NodeId;
}

bitflags! {
    /// Why a node needs attention. See section 7.1 for the meaning of each flag.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct Dirty: u16 {
        /// The display list must be rebuilt.
        const CONTENT   = 1 << 0;
        /// The local transform changed: recompose the subtree.
        const TRANSFORM = 1 << 1;
        /// The node's own opacity changed.
        const OPACITY   = 1 << 2;
        /// The clip changed.
        const CLIP      = 1 << 3;
        /// Children or z changed: re-sort the paint order.
        const ORDER     = 1 << 4;
        /// The subtree AABB is invalid.
        const BOUNDS    = 1 << 5;
        /// The layer spec changed.
        const LAYER     = 1 << 6;
        /// Accessibility data changed. Cleared by the a11y pass, never by compose.
        const A11Y      = 1 << 7;
        /// `visible` changed.
        const VISIBLE   = 1 << 8;
    }
}

/// Clip applied to a node's own content and all its descendants.
///
/// The rectangle is in the node's local space. A transform that is not
/// translate-and-scale makes the clip "rotated", which B0 approximates with its
/// bounding box (R is exact).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipSpec {
    /// Local-space rectangle.
    pub rect: Rect,
    /// Corner radii (tl, tr, br, bl); all zero for a plain rectangle.
    pub radii: [f32; 4],
}

impl ClipSpec {
    /// Plain rectangle.
    pub fn rect(rect: Rect) -> Self {
        Self {
            rect,
            radii: [0.0; 4],
        }
    }

    /// Rectangle with one radius on every corner.
    pub fn rounded(rect: Rect, radius: f32) -> Self {
        Self {
            rect,
            radii: [radius; 4],
        }
    }

    /// The node's own bounds `(0, 0, w, h)`, what QML `clip: true` means.
    pub fn bounds(size: Size) -> Self {
        Self::rect(Rect::from_origin_size((0.0, 0.0), size))
    }

    /// True when any corner is rounded.
    pub fn is_rounded(&self) -> bool {
        self.radii.iter().any(|r| *r > 0.0)
    }
}

/// `layer.*` properties. Placeholder: no backend renders layers yet, B0 and R
/// ignore it with a debug warning.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayerSpec {
    /// `layer.enabled`.
    pub enabled: bool,
    /// `layer.textureSize`.
    pub texture_size: Option<(u16, u16)>,
    /// `layer.smooth`.
    pub smooth: bool,
    /// `layer.mipmap`.
    pub mipmap: bool,
}

/// Tree mutation errors.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum TreeError {
    /// The id does not name a live node.
    #[error("no such render node")]
    NoSuchNode,
    /// The root cannot be removed or reparented.
    #[error("the root node cannot be removed or moved")]
    RootImmutable,
    /// Reparenting would make a node its own ancestor.
    #[error("reparenting would create a cycle")]
    WouldCycle,
}

/// One node. Read through accessors; mutate through [`RenderTree`] setters so
/// the dirty flags stay correct.
#[derive(Debug)]
pub struct RenderNode {
    pub(crate) parent: Option<NodeId>,
    /// Declaration order.
    pub(crate) children: SmallVec<[NodeId; 4]>,
    /// Cached `(z, declaration index)` order of `children`; valid when `ORDER` is clear.
    pub(crate) paint_order: Box<[NodeId]>,
    /// How many leading entries of `paint_order` have `z < 0`.
    pub(crate) neg_count: usize,
    pub(crate) local: Affine,
    pub(crate) world: Affine,
    pub(crate) size: Size,
    /// Local space, includes visible descendants and own content, clipped by `clip`.
    pub(crate) subtree_bounds: Rect,
    pub(crate) opacity: f32,
    /// `opacity` multiplied down from the root.
    pub(crate) effective_opacity: f32,
    pub(crate) clip: Option<ClipSpec>,
    pub(crate) layer: Option<LayerSpec>,
    pub(crate) visible: bool,
    /// Whether it and what is below it paint. One that does not is still
    /// composed, so it is hit tested and an effect can draw it.
    pub(crate) drawn: bool,
    pub(crate) z: f32,
    pub(crate) hit_testable: bool,
    pub(crate) content: Option<Arc<DisplayList>>,
    pub(crate) dirty: Dirty,
    pub(crate) last_visible_frame: u64,
    /// Identity of the world data computed for this node; descendants remember
    /// the stamp they were computed against, which is how unchanged subtrees
    /// reuse their cached world transform and effective opacity.
    pub(crate) stamp: u64,
    pub(crate) parent_stamp: u64,
    /// The epoch of the last change to this node or to one below it.
    pub(crate) changed: u64,
    /// Where this node's subtree sat in the snapshot that last composed it.
    pub(crate) span: Span,
}

impl RenderNode {
    fn new() -> Self {
        Self {
            parent: None,
            children: SmallVec::new(),
            paint_order: Box::new([]),
            neg_count: 0,
            local: Affine::IDENTITY,
            world: Affine::IDENTITY,
            size: Size::ZERO,
            subtree_bounds: Rect::ZERO,
            opacity: 1.0,
            effective_opacity: 1.0,
            clip: None,
            layer: None,
            visible: true,
            drawn: true,
            z: 0.0,
            hit_testable: true,
            content: None,
            // CONTENT is opt-in: items that paint call `mark_content_dirty`.
            // Everything else is stale until the first compose.
            dirty: Dirty::all().difference(Dirty::CONTENT),
            last_visible_frame: 0,
            stamp: 0,
            parent_stamp: 0,
            changed: 0,
            span: Span::default(),
        }
    }

    /// Parent, `None` for the root.
    pub fn parent(&self) -> Option<NodeId> {
        self.parent
    }
    /// Children in declaration order.
    pub fn children(&self) -> &[NodeId] {
        &self.children
    }
    /// Transform to the parent's space.
    pub fn local(&self) -> Affine {
        self.local
    }
    /// Transform to the root's space as of the last compose.
    pub fn world(&self) -> Affine {
        self.world
    }
    /// Node size.
    pub fn size(&self) -> Size {
        self.size
    }
    /// Subtree AABB in local space as of the last compose.
    pub fn subtree_bounds(&self) -> Rect {
        self.subtree_bounds
    }
    /// Own opacity.
    pub fn opacity(&self) -> f32 {
        self.opacity
    }
    /// Opacity multiplied down from the root as of the last compose.
    pub fn effective_opacity(&self) -> f32 {
        self.effective_opacity
    }
    /// Clip, if any.
    pub fn clip(&self) -> Option<&ClipSpec> {
        self.clip.as_ref()
    }
    /// Layer spec, if any.
    pub fn layer(&self) -> Option<&LayerSpec> {
        self.layer.as_ref()
    }
    /// `visible`.
    pub fn visible(&self) -> bool {
        self.visible
    }
    /// `z`.
    pub fn z(&self) -> f32 {
        self.z
    }
    /// Whether hit testing may return this node.
    pub fn hit_testable(&self) -> bool {
        self.hit_testable
    }
    /// Cached display list.
    pub fn content(&self) -> Option<&Arc<DisplayList>> {
        self.content.as_ref()
    }
    /// Pending dirty flags.
    pub fn dirty(&self) -> Dirty {
        self.dirty
    }
    /// Last frame in which compose emitted this node.
    pub fn last_visible_frame(&self) -> u64 {
        self.last_visible_frame
    }
    /// Pruned from paint and hit testing by its own state (`visible: false` or opacity 0).
    pub fn is_pruned(&self) -> bool {
        !self.visible || self.opacity <= 0.0
    }
}

/// Slotmap of [`RenderNode`]s with a single root.
#[derive(Debug)]
pub struct RenderTree {
    pub(crate) nodes: SlotMap<NodeId, RenderNode>,
    pub(crate) root: NodeId,
    /// Bumped by every change that can alter a snapshot.
    pub(crate) epoch: u64,
    pub(crate) next_stamp: u64,
    content_queue: Vec<NodeId>,
    content_rebuilds: u64,
    pub(crate) cache: Option<CachedSnapshot>,
}

impl Default for RenderTree {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderTree {
    /// A tree holding just a root node.
    pub fn new() -> Self {
        let mut nodes = SlotMap::with_key();
        let root = nodes.insert(RenderNode::new());
        Self {
            nodes,
            root,
            epoch: 1,
            next_stamp: 1,
            content_queue: Vec::new(),
            content_rebuilds: 0,
            cache: None,
        }
    }

    /// The root node.
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Number of live nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Never true: the root always exists.
    pub fn is_empty(&self) -> bool {
        false
    }

    /// Does `id` name a live node?
    pub fn contains(&self, id: NodeId) -> bool {
        self.nodes.contains_key(id)
    }

    /// Borrow a node.
    pub fn node(&self, id: NodeId) -> Option<&RenderNode> {
        self.nodes.get(id)
    }

    /// How many display lists were installed with [`Self::set_content`]. Tests use
    /// it to prove that transform and opacity changes never rebuild content.
    pub fn content_rebuilds(&self) -> u64 {
        self.content_rebuilds
    }

    /// A change at `id`: a new epoch, and `id` and its ancestors remember it
    /// so that compose walks down to it and copies what did not change.
    fn touch(&mut self, id: NodeId) {
        self.epoch += 1;
        // Marks made since the last compose reach the root, so the first
        // node found marked has marked ancestors.
        let since = self.cache.as_ref().map_or(0, |c| c.epoch);
        let mut cur = Some(id);
        while let Some(c) = cur {
            let Some(n) = self.nodes.get_mut(c) else {
                return;
            };
            if n.changed > since {
                return;
            }
            n.changed = self.epoch;
            cur = n.parent;
        }
    }

    /// Mark `BOUNDS` on `id` and its ancestors. Invariant: a node with `BOUNDS`
    /// has `BOUNDS` on every ancestor, so we can stop at the first one that has it.
    fn mark_bounds_from(&mut self, id: NodeId) {
        let mut cur = Some(id);
        let mut first = true;
        while let Some(c) = cur {
            let Some(n) = self.nodes.get_mut(c) else {
                return;
            };
            if !first && n.dirty.contains(Dirty::BOUNDS) {
                return;
            }
            first = false;
            n.dirty.insert(Dirty::BOUNDS);
            cur = n.parent;
        }
    }

    fn mark_bounds_of_parent(&mut self, id: NodeId) {
        if let Some(p) = self.nodes.get(id).and_then(|n| n.parent) {
            self.mark_bounds_from(p);
        }
    }

    // ------------------------------------------------------------------ structure

    /// Append a new child to `parent`. Panics if `parent` is stale: that is a bug
    /// in the caller (the Engine owns lifetimes).
    pub fn add_child(&mut self, parent: NodeId) -> NodeId {
        self.insert_child(parent, usize::MAX)
    }

    /// Insert a new child at declaration index `index` (clamped).
    pub fn insert_child(&mut self, parent: NodeId, index: usize) -> NodeId {
        let mut node = RenderNode::new();
        node.parent = Some(parent);
        let id = self.nodes.insert(node);
        let p = self
            .nodes
            .get_mut(parent)
            .expect("insert_child: stale parent id");
        let at = index.min(p.children.len());
        p.children.insert(at, id);
        p.dirty.insert(Dirty::ORDER | Dirty::A11Y);
        self.mark_bounds_from(parent);
        self.touch(parent);
        id
    }

    /// Remove `id` and its whole subtree. Returns how many nodes went away.
    pub fn remove(&mut self, id: NodeId) -> Result<usize, TreeError> {
        if id == self.root {
            return Err(TreeError::RootImmutable);
        }
        let parent = self.nodes.get(id).ok_or(TreeError::NoSuchNode)?.parent;
        if let Some(p) = parent
            && let Some(pn) = self.nodes.get_mut(p)
        {
            pn.children.retain(|c| *c != id);
            pn.dirty.insert(Dirty::ORDER | Dirty::A11Y);
        }
        if let Some(p) = parent {
            self.mark_bounds_from(p);
        }
        let mut stack = vec![id];
        let mut count = 0;
        while let Some(n) = stack.pop() {
            if let Some(node) = self.nodes.remove(n) {
                count += 1;
                stack.extend(node.children);
            }
        }
        if let Some(p) = parent {
            self.touch(p);
        } else {
            self.epoch += 1;
        }
        Ok(count)
    }

    /// Move `id` under `new_parent` at `index` (clamped; `None` appends).
    pub fn reparent(
        &mut self,
        id: NodeId,
        new_parent: NodeId,
        index: Option<usize>,
    ) -> Result<(), TreeError> {
        if id == self.root {
            return Err(TreeError::RootImmutable);
        }
        if !self.nodes.contains_key(id) || !self.nodes.contains_key(new_parent) {
            return Err(TreeError::NoSuchNode);
        }
        let mut cur = Some(new_parent);
        while let Some(c) = cur {
            if c == id {
                return Err(TreeError::WouldCycle);
            }
            cur = self.nodes[c].parent;
        }
        let old_parent = self.nodes[id].parent;
        if let Some(op) = old_parent {
            let o = &mut self.nodes[op];
            o.children.retain(|c| *c != id);
            o.dirty.insert(Dirty::ORDER | Dirty::A11Y);
            self.mark_bounds_from(op);
        }
        let np = &mut self.nodes[new_parent];
        let at = index.unwrap_or(usize::MAX).min(np.children.len());
        np.children.insert(at, id);
        np.dirty.insert(Dirty::ORDER | Dirty::A11Y);
        self.nodes[id].parent = Some(new_parent);
        self.nodes[id]
            .dirty
            .insert(Dirty::TRANSFORM | Dirty::OPACITY);
        self.mark_bounds_from(new_parent);
        if let Some(op) = old_parent {
            self.touch(op);
        }
        // From the new parent: the node may be marked already, its new
        // ancestors not.
        self.touch(new_parent);
        self.nodes[id].changed = self.epoch;
        Ok(())
    }

    // ------------------------------------------------------------------ setters

    fn with_node(&mut self, id: NodeId, f: impl FnOnce(&mut RenderNode) -> bool) -> bool {
        let changed = self.nodes.get_mut(id).is_some_and(f);
        if changed {
            self.touch(id);
        }
        changed
    }

    /// Set the transform to the parent's space (`x`, `y`, rotation, scale ...).
    pub fn set_local(&mut self, id: NodeId, local: Affine) {
        let changed = self.with_node(id, |n| {
            if n.local == local {
                return false;
            }
            n.local = local;
            n.dirty.insert(Dirty::TRANSFORM);
            true
        });
        if changed {
            // The parent's subtree bounds depend on where this child sits.
            self.mark_bounds_of_parent(id);
        }
    }

    /// Set a pure translation (QML `x`, `y`).
    pub fn set_pos(&mut self, id: NodeId, x: f64, y: f64) {
        self.set_local(id, Affine::translate((x, y)));
    }

    /// Set the size (`width`, `height`).
    pub fn set_size(&mut self, id: NodeId, size: Size) {
        let changed = self.with_node(id, |n| {
            if n.size == size {
                return false;
            }
            n.size = size;
            n.dirty.insert(Dirty::A11Y);
            true
        });
        if changed {
            self.mark_bounds_from(id);
        }
    }

    /// Set the node's own opacity (clamped to `0..=1`).
    pub fn set_opacity(&mut self, id: NodeId, opacity: f32) {
        let opacity = opacity.clamp(0.0, 1.0);
        let mut crossed_zero = false;
        self.with_node(id, |n| {
            if n.opacity == opacity {
                return false;
            }
            crossed_zero = (n.opacity <= 0.0) != (opacity <= 0.0);
            n.opacity = opacity;
            n.dirty.insert(Dirty::OPACITY);
            true
        });
        // Bounds only care whether the child is pruned, not how transparent it is.
        if crossed_zero {
            self.mark_bounds_of_parent(id);
        }
    }

    /// Whether `id` and what is below it paint; they stay in hit testing
    /// either way (a `ShaderEffectSource`'s `hideSource`).
    pub fn set_drawn(&mut self, id: NodeId, drawn: bool) {
        self.with_node(id, |n| {
            let changed = n.drawn != drawn;
            n.drawn = drawn;
            changed
        });
    }

    /// Set `visible`.
    pub fn set_visible(&mut self, id: NodeId, visible: bool) {
        let changed = self.with_node(id, |n| {
            if n.visible == visible {
                return false;
            }
            n.visible = visible;
            n.dirty.insert(Dirty::VISIBLE | Dirty::A11Y);
            true
        });
        if changed {
            self.mark_bounds_of_parent(id);
        }
    }

    /// Set or clear the clip.
    pub fn set_clip(&mut self, id: NodeId, clip: Option<ClipSpec>) {
        let changed = self.with_node(id, |n| {
            if n.clip == clip {
                return false;
            }
            n.clip = clip;
            n.dirty.insert(Dirty::CLIP);
            true
        });
        if changed {
            self.mark_bounds_from(id);
        }
    }

    /// Set or clear the layer spec.
    pub fn set_layer(&mut self, id: NodeId, layer: Option<LayerSpec>) {
        self.with_node(id, |n| {
            if n.layer == layer {
                return false;
            }
            n.layer = layer;
            n.dirty.insert(Dirty::LAYER);
            true
        });
    }

    /// Set `z`. Re-sorts the parent's paint order, nothing else.
    pub fn set_z(&mut self, id: NodeId, z: f32) {
        let mut parent = None;
        let changed = self.with_node(id, |n| {
            if n.z == z {
                return false;
            }
            n.z = z;
            parent = n.parent;
            true
        });
        if changed && let Some(p) = parent {
            self.nodes[p].dirty.insert(Dirty::ORDER | Dirty::A11Y);
        }
    }

    /// Whether hit testing may return this node (QML `enabled`-independent; the
    /// input layer decides). Does not change what is painted.
    pub fn set_hit_testable(&mut self, id: NodeId, hit_testable: bool) {
        if let Some(n) = self.nodes.get_mut(id) {
            n.hit_testable = hit_testable;
            // Hit testing reads the snapshot, so it must be rebuilt.
            self.touch(id);
        }
    }

    /// Flag the accessibility data as changed. Does not invalidate snapshots.
    pub fn mark_a11y_dirty(&mut self, id: NodeId) {
        if let Some(n) = self.nodes.get_mut(id) {
            n.dirty.insert(Dirty::A11Y);
        }
    }

    /// Clear flags the caller has consumed (the a11y pass clears `A11Y`).
    pub fn clear_dirty(&mut self, id: NodeId, flags: Dirty) {
        if let Some(n) = self.nodes.get_mut(id) {
            n.dirty.remove(flags);
        }
    }

    // ------------------------------------------------------------------ content

    /// Ask for the display list to be rebuilt on the next [`Self::rebuild_content`].
    pub fn mark_content_dirty(&mut self, id: NodeId) {
        if let Some(n) = self.nodes.get_mut(id)
            && !n.dirty.contains(Dirty::CONTENT)
        {
            n.dirty.insert(Dirty::CONTENT);
            self.content_queue.push(id);
        }
    }

    /// Nodes waiting for a content rebuild.
    pub fn pending_content(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.content_queue.iter().copied().filter(|id| {
            self.nodes
                .get(*id)
                .is_some_and(|n| n.dirty.contains(Dirty::CONTENT))
        })
    }

    /// Install a freshly built display list, clearing `CONTENT`.
    pub fn set_content(&mut self, id: NodeId, content: Option<Arc<DisplayList>>) {
        let Some(n) = self.nodes.get_mut(id) else {
            return;
        };
        n.content = content;
        n.dirty.remove(Dirty::CONTENT);
        self.content_rebuilds += 1;
        self.mark_bounds_from(id);
        self.touch(id);
    }

    /// Sync step 8: for every `CONTENT`-dirty node call `paint` and install the
    /// resulting list. Nothing else in the tree ever triggers a call. Returns the
    /// number of rebuilt lists.
    pub fn rebuild_content(
        &mut self,
        scale_factor: f32,
        mut paint: impl FnMut(&PaintCx, &mut DisplayListBuilder),
    ) -> usize {
        let queue = std::mem::take(&mut self.content_queue);
        let mut rebuilt = 0;
        for id in queue {
            let Some(n) = self.nodes.get(id) else {
                continue;
            };
            if !n.dirty.contains(Dirty::CONTENT) {
                continue;
            }
            let cx = PaintCx {
                node: id,
                size: n.size,
                scale_factor,
            };
            let mut b = DisplayListBuilder::new();
            paint(&cx, &mut b);
            let list = if b.is_empty() {
                None
            } else {
                Some(b.finish_arc())
            };
            self.set_content(id, list);
            rebuilt += 1;
        }
        rebuilt
    }

    // ------------------------------------------------------------------ order

    /// Re-sort `id`'s children if `ORDER` is set: stable by `(z, declaration index)`.
    pub(crate) fn ensure_order(&mut self, id: NodeId) {
        let Some(n) = self.nodes.get(id) else {
            return;
        };
        if !n.dirty.contains(Dirty::ORDER) {
            return;
        }
        let mut keyed: Vec<(f32, usize, NodeId)> = n
            .children
            .iter()
            .enumerate()
            .map(|(i, c)| (self.nodes[*c].z, i, *c))
            .collect();
        keyed.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let neg = keyed.iter().take_while(|k| k.0 < 0.0).count();
        let n = &mut self.nodes[id];
        n.paint_order = keyed.into_iter().map(|k| k.2).collect();
        n.neg_count = neg;
        n.dirty.remove(Dirty::ORDER);
    }

    /// Visit the visible, non-pruned nodes in paint order (back to front), the
    /// same order [`crate::FrameSnapshot`] uses, but without culling. Accessibility
    /// order and tests use it.
    pub fn walk_paint_order(&mut self, mut visit: impl FnMut(NodeId)) {
        let root = self.root;
        self.walk_from(root, &mut visit);
    }

    fn walk_from(&mut self, id: NodeId, visit: &mut impl FnMut(NodeId)) {
        if self.nodes[id].is_pruned() {
            return;
        }
        self.ensure_order(id);
        let order = std::mem::take(&mut self.nodes[id].paint_order);
        let neg = self.nodes[id].neg_count;
        for c in &order[..neg] {
            self.walk_from(*c, visit);
        }
        visit(id);
        for c in &order[neg..] {
            self.walk_from(*c, visit);
        }
        self.nodes[id].paint_order = order;
    }

    // ------------------------------------------------------------------ bounds

    /// Recompute `subtree_bounds` along `BOUNDS`-dirty chains only.
    pub(crate) fn refresh_bounds(&mut self, id: NodeId) {
        if !self.nodes[id].dirty.contains(Dirty::BOUNDS) {
            return;
        }
        // By index: a copy of the children was an allocation per dirty
        // node with more than four, in every frame that moves something.
        let n = &self.nodes[id];
        // The hit rectangle is always part of the bounds, so a content-less node
        // still culls correctly.
        let mut b = Rect::from_origin_size((0.0, 0.0), n.size);
        if let Some(content) = &n.content
            && !content.is_empty()
        {
            b = b.union(content.bounds());
        }
        for i in 0..self.nodes[id].children.len() {
            let c = self.nodes[id].children[i];
            // Checked here: most children of a node that moved one did not.
            if self.nodes[c].dirty.contains(Dirty::BOUNDS) {
                self.refresh_bounds(c);
            }
            let cn = &self.nodes[c];
            if cn.is_pruned() {
                continue;
            }
            b = b.union(crate::compose::bbox_of(cn.local, cn.subtree_bounds));
        }
        let n = &self.nodes[id];
        if let Some(clip) = &n.clip {
            let r = b.intersect(clip.rect);
            // `intersect` of disjoint rects is inverted; make it empty at the origin.
            b = if r.x0 <= r.x1 && r.y0 <= r.y1 {
                r
            } else {
                Rect::ZERO
            };
        }
        let n = &mut self.nodes[id];
        n.subtree_bounds = b;
        n.dirty.remove(Dirty::BOUNDS);
    }
}
