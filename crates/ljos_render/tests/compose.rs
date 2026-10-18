//! Compose, paint order, pruning, culling, dirty minimality and hit testing.

use std::sync::Arc;

use ljos_render::kurbo::{Affine, Point, Rect, Size};
use ljos_render::{
    ClipSpec, Dirty, DisplayListBuilder, NodeId, RenderTree, Rgba, TreeError, hit_test,
};

const VP: Rect = Rect::new(0.0, 0.0, 100.0, 100.0);

fn sized(t: &mut RenderTree, parent: NodeId, x: f64, y: f64, w: f64, h: f64) -> NodeId {
    let n = t.add_child(parent);
    t.set_pos(n, x, y);
    t.set_size(n, Size::new(w, h));
    n
}

fn order(t: &mut RenderTree, vp: Rect) -> Vec<NodeId> {
    t.compose(vp, 1).order().collect()
}

#[test]
fn z_and_declaration_index_decide_paint_order() {
    let mut t = RenderTree::new();
    let r = t.root();
    let a = t.add_child(r); // z 0, decl 0
    let b = t.add_child(r); // z -1, decl 1
    let c = t.add_child(r); // z 1, decl 2
    let d = t.add_child(r); // z 0, decl 3
    let e = t.add_child(r); // z -1, decl 4
    let f = t.add_child(r); // z -2, decl 5
    t.set_z(b, -1.0);
    t.set_z(c, 1.0);
    t.set_z(e, -1.0);
    t.set_z(f, -2.0);
    // Negative z first (by z, then declaration), then the node itself, then the rest.
    assert_eq!(order(&mut t, VP), vec![f, b, e, r, a, d, c]);
}

#[test]
fn z_changes_resort_and_declaration_order_breaks_ties() {
    let mut t = RenderTree::new();
    let r = t.root();
    let a = t.add_child(r);
    let b = t.add_child(r);
    assert_eq!(order(&mut t, VP), vec![r, a, b]);
    t.set_z(a, 1.0);
    assert_eq!(order(&mut t, VP), vec![r, b, a]);
    t.set_z(a, 0.0);
    assert_eq!(order(&mut t, VP), vec![r, a, b]);
    // Inserting at the front changes declaration indices.
    let c = t.insert_child(r, 0);
    assert_eq!(order(&mut t, VP), vec![r, c, a, b]);
}

#[test]
fn negative_z_applies_per_parent() {
    let mut t = RenderTree::new();
    let r = t.root();
    let p = t.add_child(r);
    let under = t.add_child(p);
    t.set_z(under, -1.0);
    let over = t.add_child(p);
    let sibling = t.add_child(r);
    // `under` goes below `p` itself but stays above everything declared before `p`.
    assert_eq!(order(&mut t, VP), vec![r, under, p, over, sibling]);
}

#[test]
fn walk_paint_order_matches_snapshot_order() {
    let mut t = RenderTree::new();
    let r = t.root();
    let a = sized(&mut t, r, 0.0, 0.0, 10.0, 10.0);
    let b = sized(&mut t, r, 0.0, 0.0, 10.0, 10.0);
    t.set_z(a, 2.0);
    let snap: Vec<_> = order(&mut t, VP);
    let mut walked = Vec::new();
    t.walk_paint_order(|n| walked.push(n));
    assert_eq!(snap, walked);
    assert_eq!(walked, vec![r, b, a]);
}

#[test]
fn opacity_is_multiplied_down_and_zero_prunes() {
    let mut t = RenderTree::new();
    let r = t.root();
    let p = sized(&mut t, r, 0.0, 0.0, 50.0, 50.0);
    let c = sized(&mut t, p, 0.0, 0.0, 10.0, 10.0);
    t.set_opacity(p, 0.5);
    t.set_opacity(c, 0.5);
    let snap = t.compose(VP, 1);
    let op = |n: NodeId| snap.items.iter().find(|i| i.node == n).unwrap().opacity;
    assert_eq!(op(r), 1.0);
    assert_eq!(op(p), 0.5);
    assert_eq!(op(c), 0.25);

    t.set_opacity(p, 0.0);
    let snap = t.compose(VP, 2);
    let ids: Vec<_> = snap.order().collect();
    assert_eq!(ids, vec![r], "opacity 0 prunes the node and its subtree");
    assert_eq!(snap.stats.pruned, 1);

    t.set_opacity(p, 1.0);
    let snap = t.compose(VP, 3);
    let c_op = snap.items.iter().find(|i| i.node == c).unwrap().opacity;
    assert_eq!(c_op, 0.5, "unpruned subtree gets fresh effective opacity");
}

#[test]
fn invisible_subtrees_are_pruned_from_paint_and_hit_testing() {
    let mut t = RenderTree::new();
    let r = t.root();
    t.set_size(r, Size::new(100.0, 100.0));
    let p = sized(&mut t, r, 10.0, 10.0, 50.0, 50.0);
    let c = sized(&mut t, p, 0.0, 0.0, 50.0, 50.0);
    let snap = t.compose(VP, 1);
    let hits: Vec<_> = hit_test(&snap, Point::new(20.0, 20.0)).collect();
    assert_eq!(hits, vec![c, p, r]);

    t.set_visible(p, false);
    let snap = t.compose(VP, 2);
    assert_eq!(snap.order().collect::<Vec<_>>(), vec![r]);
    let hits: Vec<_> = hit_test(&snap, Point::new(20.0, 20.0)).collect();
    assert_eq!(hits, vec![r]);
    // The node itself still exists with its state intact.
    assert!(t.node(c).unwrap().visible());
}

#[test]
fn nodes_outside_the_viewport_are_culled_with_their_subtree() {
    let mut t = RenderTree::new();
    let r = t.root();
    let inside = sized(&mut t, r, 10.0, 10.0, 20.0, 20.0);
    let outside = sized(&mut t, r, 500.0, 500.0, 20.0, 20.0);
    let child_of_outside = sized(&mut t, outside, 0.0, 0.0, 5.0, 5.0);
    let straddling = sized(&mut t, r, 90.0, 90.0, 50.0, 50.0);
    let snap = t.compose(VP, 1);
    let ids: Vec<_> = snap.order().collect();
    assert!(ids.contains(&inside));
    assert!(ids.contains(&straddling));
    assert!(!ids.contains(&outside));
    assert!(!ids.contains(&child_of_outside));
    assert_eq!(snap.stats.culled, 1, "one subtree skipped, not each node");

    // Moving the container, not the child, brings the child into view.
    t.set_pos(outside, 40.0, 40.0);
    let snap = t.compose(VP, 2);
    let item = snap
        .items
        .iter()
        .find(|i| i.node == child_of_outside)
        .unwrap();
    assert_eq!(item.world, Affine::translate((40.0, 40.0)));
}

#[test]
fn a_child_outside_its_parents_size_is_not_culled_with_it() {
    // Qt items paint outside their own width/height unless clipped.
    let mut t = RenderTree::new();
    let r = t.root();
    let parent = sized(&mut t, r, 0.0, 0.0, 1.0, 1.0);
    let child = sized(&mut t, parent, 50.0, 50.0, 10.0, 10.0);
    let ids = order(&mut t, VP);
    assert!(ids.contains(&child));
}

#[test]
fn clips_cull_children_and_stack() {
    let mut t = RenderTree::new();
    let r = t.root();
    let clipper = sized(&mut t, r, 10.0, 10.0, 30.0, 30.0);
    t.set_clip(clipper, Some(ClipSpec::bounds(Size::new(30.0, 30.0))));
    let inside = sized(&mut t, clipper, 5.0, 5.0, 10.0, 10.0);
    let outside_clip = sized(&mut t, clipper, 60.0, 60.0, 10.0, 10.0); // inside viewport, outside clip
    let snap = t.compose(VP, 1);
    let ids: Vec<_> = snap.order().collect();
    assert!(ids.contains(&inside));
    assert!(!ids.contains(&outside_clip));
    assert_eq!(snap.clips.len(), 1);
    assert_eq!(snap.clips[0].bbox, Rect::new(10.0, 10.0, 40.0, 40.0));
    assert!(snap.clips[0].axis_aligned);
    // The clipping node's own content is clipped too.
    let own = snap.items.iter().find(|i| i.node == clipper).unwrap();
    assert!(own.clip.is_some());

    // Nested clip intersects.
    let inner = sized(&mut t, clipper, 20.0, 0.0, 30.0, 30.0);
    t.set_clip(inner, Some(ClipSpec::bounds(Size::new(30.0, 30.0))));
    let snap = t.compose(VP, 2);
    let inner_clip = snap
        .items
        .iter()
        .find(|i| i.node == inner)
        .unwrap()
        .clip
        .unwrap();
    assert_eq!(
        snap.clip(inner_clip).bbox,
        Rect::new(30.0, 10.0, 40.0, 40.0)
    );
    assert!(snap.clip(inner_clip).parent.is_some());
}

#[test]
fn a_content_clip_clips_only_the_nodes_own_drawing() {
    let mut t = RenderTree::new();
    let r = t.root();
    let field = sized(&mut t, r, 10.0, 10.0, 40.0, 20.0);
    t.set_hit_testable(field, true);
    t.set_content_clip(field, Some(ClipSpec::rect(Rect::new(5.0, 2.0, 35.0, 18.0))));
    let child = sized(&mut t, field, 0.0, 0.0, 40.0, 20.0);
    let snap = t.compose(VP, 1);
    let own = snap.items.iter().find(|i| i.node == field).unwrap();
    let clip = snap.clip(own.clip.unwrap());
    assert!(clip.content);
    assert_eq!(clip.bbox, Rect::new(15.0, 12.0, 45.0, 28.0));
    let below = snap.items.iter().find(|i| i.node == child).unwrap();
    assert_eq!(below.clip, None);
    // Where it does not draw, it is still there to press.
    assert!(hit_test(&snap, Point::new(12.0, 11.0)).any(|n| n == field));

    t.set_content_clip(field, None);
    let snap = t.compose(VP, 2);
    let own = snap.items.iter().find(|i| i.node == field).unwrap();
    assert_eq!(own.clip, None);
}

fn build_scene(t: &mut RenderTree, n: usize) -> Vec<NodeId> {
    let r = t.root();
    t.set_size(r, Size::new(100.0, 100.0));
    (0..n)
        .map(|i| {
            let id = sized(
                t,
                r,
                (i % 10) as f64 * 10.0,
                (i / 10) as f64 * 10.0,
                8.0,
                8.0,
            );
            t.mark_content_dirty(id);
            id
        })
        .collect()
}

#[test]
fn only_content_dirty_rebuilds_display_lists() {
    let mut t = RenderTree::new();
    let nodes = build_scene(&mut t, 100);
    let mut calls = 0;
    let rebuilt = t.rebuild_content(1.0, |cx, b| {
        calls += 1;
        b.fill_rect(Rect::from_origin_size((0.0, 0.0), cx.size), Rgba::WHITE);
    });
    assert_eq!((rebuilt, calls), (100, 100));
    assert_eq!(t.content_rebuilds(), 100);
    let first = t.compose(VP, 1);

    // Transform, opacity, clip, z and visibility changes never rebuild.
    t.set_pos(nodes[3], 1.0, 2.0);
    t.set_local(nodes[4], Affine::rotate(0.3));
    t.set_opacity(nodes[5], 0.5);
    t.set_clip(nodes[6], Some(ClipSpec::bounds(Size::new(4.0, 4.0))));
    t.set_z(nodes[7], 3.0);
    t.set_visible(nodes[8], false);
    let before = t.content_rebuilds();
    let rebuilt = t.rebuild_content(1.0, |_, _| panic!("no node is CONTENT-dirty"));
    assert_eq!(rebuilt, 0);
    assert_eq!(t.content_rebuilds(), before);
    let second = t.compose(VP, 2);
    assert!(!Arc::ptr_eq(&first, &second));
    // The cached list is the very same allocation.
    let list = |s: &ljos_render::FrameSnapshot, n| {
        s.items
            .iter()
            .find(|i| i.node == n)
            .unwrap()
            .list
            .clone()
            .unwrap()
    };
    assert!(Arc::ptr_eq(
        &list(&first, nodes[3]),
        &list(&second, nodes[3])
    ));

    // A content change rebuilds exactly that node.
    t.mark_content_dirty(nodes[3]);
    t.mark_content_dirty(nodes[3]); // idempotent
    let mut hit = Vec::new();
    t.rebuild_content(1.0, |cx, b| {
        hit.push(cx.node);
        b.fill_rect(Rect::new(0.0, 0.0, 1.0, 1.0), Rgba::BLACK);
    });
    assert_eq!(hit, vec![nodes[3]]);
    let third = t.compose(VP, 3);
    assert!(!Arc::ptr_eq(
        &list(&second, nodes[3]),
        &list(&third, nodes[3])
    ));
}

#[test]
fn unchanged_tree_returns_the_same_snapshot() {
    let mut t = RenderTree::new();
    build_scene(&mut t, 10);
    let a = t.compose(VP, 1);
    let b = t.compose(VP, 2);
    assert!(Arc::ptr_eq(&a, &b));
    let c = t.compose(Rect::new(0.0, 0.0, 50.0, 50.0), 3);
    assert!(!Arc::ptr_eq(&a, &c), "a new viewport recomposes");
    // A11y-only changes do not invalidate paint.
    let leaf = t.node(t.root()).unwrap().children()[0];
    t.compose(VP, 4);
    t.mark_a11y_dirty(leaf);
    let d = t.compose(VP, 4);
    let e = t.compose(VP, 5);
    assert!(Arc::ptr_eq(&d, &e));
    assert!(t.node(leaf).unwrap().dirty().contains(Dirty::A11Y));
}

#[test]
fn unchanged_subtrees_reuse_cached_world_data() {
    let mut t = RenderTree::new();
    let r = t.root();
    let p = sized(&mut t, r, 0.0, 0.0, 50.0, 50.0);
    let kids: Vec<_> = (0..5)
        .map(|i| sized(&mut t, p, i as f64, 0.0, 5.0, 5.0))
        .collect();
    let other = sized(&mut t, r, 60.0, 0.0, 10.0, 10.0);
    let other_kids: Vec<_> = (0..5)
        .map(|_| sized(&mut t, other, 0.0, 0.0, 2.0, 2.0))
        .collect();
    let _ = other_kids;
    let s = t.compose(VP, 1);
    assert_eq!(s.stats.world_recomputed, 13);

    // Moving one leaf recomputes just that leaf.
    t.set_pos(kids[2], 9.0, 9.0);
    let s = t.compose(VP, 2);
    assert_eq!(s.stats.world_recomputed, 1);

    // Moving a parent recomputes its subtree only, not its sibling's.
    t.set_pos(p, 3.0, 3.0);
    let s = t.compose(VP, 3);
    assert_eq!(s.stats.world_recomputed, 6);
    let item = s.items.iter().find(|i| i.node == kids[2]).unwrap();
    assert_eq!(item.world, Affine::translate((3.0 + 9.0, 3.0 + 9.0)));
}

#[test]
fn culled_subtrees_catch_up_when_they_return() {
    let mut t = RenderTree::new();
    let r = t.root();
    let far = sized(&mut t, r, 1000.0, 0.0, 20.0, 20.0);
    let leaf = sized(&mut t, far, 1.0, 1.0, 5.0, 5.0);
    assert!(!order(&mut t, VP).contains(&leaf));
    // While the leaf is skipped its ancestor moves twice.
    t.set_pos(far, 500.0, 0.0);
    t.compose(VP, 2);
    t.set_pos(far, 30.0, 0.0);
    let s = t.compose(VP, 3);
    let item = s.items.iter().find(|i| i.node == leaf).unwrap();
    assert_eq!(item.world, Affine::translate((31.0, 1.0)));
}

#[test]
fn last_visible_frame_tracks_emitted_nodes() {
    let mut t = RenderTree::new();
    let r = t.root();
    let a = sized(&mut t, r, 0.0, 0.0, 10.0, 10.0);
    let gone = sized(&mut t, r, 900.0, 0.0, 10.0, 10.0);
    t.compose(VP, 7);
    assert_eq!(t.node(a).unwrap().last_visible_frame(), 7);
    assert_eq!(t.node(gone).unwrap().last_visible_frame(), 0);
    t.compose(VP, 8); // reused snapshot still advances the clock
    assert_eq!(t.node(a).unwrap().last_visible_frame(), 8);
}

#[test]
fn hit_test_returns_topmost_first_and_honours_z() {
    let mut t = RenderTree::new();
    let r = t.root();
    t.set_size(r, Size::new(100.0, 100.0));
    let a = sized(&mut t, r, 0.0, 0.0, 50.0, 50.0);
    let b = sized(&mut t, r, 0.0, 0.0, 50.0, 50.0);
    let p = Point::new(10.0, 10.0);
    let snap = t.compose(VP, 1);
    assert_eq!(hit_test(&snap, p).collect::<Vec<_>>(), vec![b, a, r]);
    t.set_z(a, 1.0);
    let snap = t.compose(VP, 2);
    assert_eq!(hit_test(&snap, p).collect::<Vec<_>>(), vec![a, b, r]);
    t.set_z(a, -1.0);
    let snap = t.compose(VP, 3);
    assert_eq!(hit_test(&snap, p).collect::<Vec<_>>(), vec![b, r, a]);
    assert!(hit_test(&snap, Point::new(75.0, 75.0)).eq([r]));
    assert!(hit_test(&snap, Point::new(150.0, 150.0)).next().is_none());
}

#[test]
fn hit_test_honours_clips_including_rotated_and_rounded() {
    let mut t = RenderTree::new();
    let r = t.root();
    t.set_size(r, Size::new(100.0, 100.0));
    let clipper = sized(&mut t, r, 0.0, 0.0, 20.0, 20.0);
    t.set_clip(clipper, Some(ClipSpec::bounds(Size::new(20.0, 20.0))));
    let big = sized(&mut t, clipper, 0.0, 0.0, 80.0, 80.0);
    let snap = t.compose(VP, 1);
    assert_eq!(hit_test(&snap, Point::new(10.0, 10.0)).next(), Some(big));
    assert_eq!(
        hit_test(&snap, Point::new(50.0, 50.0)).next(),
        Some(r),
        "outside the clip the clipped child cannot be hit"
    );

    // Rounded corner: the pixel in the cut corner is outside.
    t.set_clip(
        clipper,
        Some(ClipSpec::rounded(Rect::new(0.0, 0.0, 20.0, 20.0), 10.0)),
    );
    let snap = t.compose(VP, 2);
    assert_eq!(hit_test(&snap, Point::new(0.5, 0.5)).next(), Some(r));
    assert_eq!(hit_test(&snap, Point::new(10.0, 10.0)).next(), Some(big));

    // Rotated 45 degrees about the origin: only the rotated square is hittable.
    t.set_clip(clipper, Some(ClipSpec::bounds(Size::new(20.0, 20.0))));
    t.set_local(
        clipper,
        Affine::translate((50.0, 10.0)) * Affine::rotate(std::f64::consts::FRAC_PI_4),
    );
    let snap = t.compose(VP, 3);
    assert!(!snap.clips[0].axis_aligned);
    // Inside the diamond (centre of the rotated square).
    assert_eq!(hit_test(&snap, Point::new(50.0, 24.0)).next(), Some(big));
    // Inside the axis-aligned bounding box of the diamond but outside it.
    assert_eq!(hit_test(&snap, Point::new(38.0, 12.0)).next(), Some(r));
}

#[test]
fn structure_errors_and_reparenting() {
    let mut t = RenderTree::new();
    let r = t.root();
    let a = t.add_child(r);
    let b = t.add_child(a);
    assert_eq!(t.remove(r), Err(TreeError::RootImmutable));
    assert_eq!(t.reparent(a, b, None), Err(TreeError::WouldCycle));
    assert_eq!(t.reparent(a, a, None), Err(TreeError::WouldCycle));
    let c = t.add_child(r);
    t.reparent(b, c, None).unwrap();
    assert_eq!(t.node(b).unwrap().parent(), Some(c));
    assert!(t.node(a).unwrap().children().is_empty());
    assert_eq!(order(&mut t, VP), vec![r, a, c, b]);
    assert_eq!(t.remove(c), Ok(2));
    assert!(!t.contains(b));
    assert_eq!(t.remove(b), Err(TreeError::NoSuchNode));
    assert_eq!(order(&mut t, VP), vec![r, a]);
}

#[test]
fn display_list_bounds_extend_the_cull_box() {
    // A shadow reaches beyond the node's size; it must not be culled early.
    let mut t = RenderTree::new();
    let r = t.root();
    let n = sized(&mut t, r, 125.0, 10.0, 10.0, 10.0);
    let mut b = DisplayListBuilder::new();
    b.shadow(
        Rect::new(0.0, 0.0, 10.0, 10.0),
        [0.0; 4],
        20.0,
        0.0,
        Rgba::BLACK,
    );
    t.set_content(n, Some(b.finish_arc()));
    assert!(
        !order(&mut t, VP).contains(&n),
        "node itself is outside the viewport"
    );
    t.set_pos(n, 110.0, 10.0);
    // Still outside by node size, but the shadow's left edge (-20) reaches in.
    assert!(order(&mut t, VP).contains(&n));
}

#[test]
fn a_frame_that_moves_every_node_lists_them_without_growing_the_list() {
    let mut t = RenderTree::new();
    let r = t.root();
    let nodes: Vec<_> = (0..200)
        .map(|i| sized(&mut t, r, 0.0, f64::from(i) * 0.5, 4.0, 0.5))
        .collect();
    let first = t.compose(VP, 1);
    assert_eq!(first.items.len(), 201);
    for (i, &n) in nodes.iter().enumerate() {
        t.set_pos(n, 1.0, i as f64 * 0.5);
    }
    let moved = t.compose(VP, 2);
    assert!(!Arc::ptr_eq(&first, &moved));
    // Sized from the last frame: one allocation, not one per doubling.
    assert_eq!(moved.items.capacity(), moved.items.len());
}
