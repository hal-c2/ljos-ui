//! A window drawn on the processor draws again only where two frames
//! differ: the old picture with those parts drawn again must be the new
//! picture drawn whole.

use std::sync::Arc;

use ljos_render::kurbo::{Affine, Rect, Size};
use ljos_render::{
    ClipSpec, DisplayListBuilder, FrameSnapshot, FrameView, NodeId, Paint, PixelRect, RasterConfig,
    RenderTree, Rgba, RgbaImage, TextMode, VelloCpuReference, frame_damage,
};

const BG: RasterConfig = RasterConfig {
    background: Rgba::from_premul(0.0, 0.0, 0.5, 1.0),
    text: TextMode::Skip,
};

const RED: Rgba = Rgba::from_premul(1.0, 0.0, 0.0, 1.0);
const GREEN: Rgba = Rgba::from_premul(0.0, 1.0, 0.0, 1.0);
const YELLOW: Rgba = Rgba::from_premul(1.0, 1.0, 0.0, 1.0);

struct Scene {
    tree: RenderTree,
    vp: Rect,
    frame: u64,
}

impl Scene {
    fn new(w: f64, h: f64) -> Self {
        let mut tree = RenderTree::new();
        let r = tree.root();
        tree.set_size(r, Size::new(w, h));
        Self {
            tree,
            vp: Rect::new(0.0, 0.0, w, h),
            frame: 0,
        }
    }

    fn node(&mut self, parent: Option<NodeId>, x: f64, y: f64, w: f64, h: f64) -> NodeId {
        let p = parent.unwrap_or(self.tree.root());
        let n = self.tree.add_child(p);
        self.tree.set_pos(n, x, y);
        self.tree.set_size(n, Size::new(w, h));
        n
    }

    fn fill(&mut self, id: NodeId, color: Rgba) {
        let size = self.tree.node(id).unwrap().size();
        let mut b = DisplayListBuilder::new();
        b.rounded_rect(
            Rect::from_origin_size((0.0, 0.0), size),
            [6.0; 4],
            Paint::Solid(color),
            None,
        );
        self.tree.set_content(id, Some(b.finish_arc()));
    }

    fn compose(&mut self) -> Arc<FrameSnapshot> {
        self.frame += 1;
        self.tree.compose(self.vp, self.frame)
    }
}

fn whole(r: &mut VelloCpuReference, snap: &FrameSnapshot, scale: f32) -> RgbaImage {
    r.render(&FrameView::new(snap, scale)).unwrap()
}

/// Draws `before` whole, then only the damaged parts of `now` over it, and
/// checks that against `now` drawn whole. Returns the parts.
fn redraw(scene: &mut Scene, change: impl FnOnce(&mut Scene), scale: f32) -> Vec<PixelRect> {
    redraw_with(VelloCpuReference::new(BG), scene, change, scale)
}

/// [`redraw`] with `screen` drawing the parts.
fn redraw_with(
    mut screen: VelloCpuReference,
    scene: &mut Scene,
    change: impl FnOnce(&mut Scene),
    scale: f32,
) -> Vec<PixelRect> {
    let mut r = VelloCpuReference::new(BG);
    let before = scene.compose();
    let mut picture = whole(&mut r, &before, scale);
    change(scene);
    let now = scene.compose();
    let (w, h) = (picture.width, picture.height);
    let parts = frame_damage(&before, &now, scale, w, h);
    let view = FrameView::new(&now, scale);
    for part in &parts {
        let pixels = screen.render_area(&view, *part).unwrap();
        for row in 0..part.height {
            let from = (row * part.width * 4) as usize;
            let to = (((part.y + row) * w + part.x) * 4) as usize;
            let n = (part.width * 4) as usize;
            picture.data[to..to + n].copy_from_slice(&pixels[from..from + n]);
        }
    }
    let want = whole(&mut r, &now, scale);
    if let Some(i) = picture
        .data
        .chunks(4)
        .zip(want.data.chunks(4))
        .position(|(a, b)| a.iter().zip(b).any(|(a, b)| a.abs_diff(*b) > 1))
    {
        let (x, y) = (i as u32 % w, i as u32 / w);
        panic!(
            "pixel ({x}, {y}) is {:?} drawn in parts {parts:?}, {:?} drawn whole",
            picture.pixel(x, y),
            want.pixel(x, y)
        );
    }
    parts
}

fn two_boxes() -> (Scene, NodeId, NodeId) {
    let mut s = Scene::new(200.0, 120.0);
    let a = s.node(None, 10.0, 10.0, 40.0, 40.0);
    s.fill(a, RED);
    let b = s.node(None, 150.0, 70.0, 40.0, 40.0);
    s.fill(b, GREEN);
    (s, a, b)
}

#[test]
fn nothing_changed_is_no_damage() {
    let (mut s, _, _) = two_boxes();
    assert!(redraw(&mut s, |_| {}, 1.0).is_empty());
}

#[test]
fn a_move_damages_where_it_was_and_where_it_is() {
    let (mut s, a, _) = two_boxes();
    let parts = redraw(&mut s, |s| s.tree.set_pos(a, 100.0, 10.0), 1.0);
    assert_eq!(parts.len(), 2, "{parts:?}");
    let covered: u64 = parts.iter().map(PixelRect::area).sum();
    assert!(covered < 200 * 120 / 2, "{parts:?}");
}

#[test]
fn a_new_colour_damages_only_the_box() {
    let (mut s, a, _) = two_boxes();
    let parts = redraw(&mut s, |s| s.fill(a, YELLOW), 2.0);
    assert_eq!(parts.len(), 1, "{parts:?}");
    assert!(parts[0].width <= 84 && parts[0].height <= 84, "{parts:?}");
}

#[test]
fn opacity_removal_and_rotation_are_drawn_again() {
    let (mut s, a, b) = two_boxes();
    redraw(&mut s, |s| s.tree.set_opacity(a, 0.4), 1.0);
    redraw(
        &mut s,
        |s| {
            s.tree
                .set_local(b, Affine::translate((150.0, 70.0)) * Affine::rotate(0.26))
        },
        1.5,
    );
    redraw(
        &mut s,
        |s| {
            s.tree.remove(a).unwrap();
        },
        1.0,
    );
}

#[test]
fn a_box_raised_over_another_is_drawn_on_top() {
    let mut s = Scene::new(100.0, 100.0);
    let a = s.node(None, 10.0, 10.0, 50.0, 50.0);
    s.fill(a, RED);
    let b = s.node(None, 30.0, 30.0, 50.0, 50.0);
    s.fill(b, GREEN);
    let parts = redraw(&mut s, |s| s.tree.set_z(a, 1.0), 1.0);
    assert!(!parts.is_empty());
}

#[test]
fn rows_scrolled_in_a_clip_are_drawn_where_they_now_are() {
    let mut s = Scene::new(200.0, 200.0);
    let view = s.node(None, 20.0, 20.0, 160.0, 120.0);
    s.tree
        .set_clip(view, Some(ClipSpec::bounds(Size::new(160.0, 120.0))));
    let content = s.node(Some(view), 0.0, 0.0, 160.0, 800.0);
    let colours = [RED, GREEN, YELLOW];
    for i in 0..20 {
        let row = s.node(Some(content), 0.0, f64::from(i) * 40.0, 160.0, 40.0);
        s.fill(row, colours[i as usize % 3]);
    }
    let parts = redraw(&mut s, |s| s.tree.set_pos(content, 0.0, -61.0), 1.0);
    // All of it lies within the clip.
    for p in &parts {
        assert!(p.y + 2 >= 20 && p.y + p.height <= 142, "{parts:?}");
    }
    redraw(&mut s, |s| s.tree.set_pos(content, 0.0, -23.5), 1.25);
}

#[test]
fn the_screens_settings_draw_the_same_parts() {
    let (mut s, a, _) = two_boxes();
    let screen = VelloCpuReference::new(BG).for_screen(2);
    redraw_with(screen, &mut s, |s| s.tree.set_pos(a, 77.25, 31.5), 1.0);
}
