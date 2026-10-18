//! Whether a pass changed the picture in its window, and where.
//!
//! A pass runs for every pointer event, timer and message, and most of
//! them change nothing that is seen: the pointer moves over items that do
//! not react to it, a click lands where nothing takes it, an animation
//! moves something out of sight. Rasterising, submitting and presenting
//! the same picture again is most of what a frame costs (all of it on a
//! software rasteriser), so a window keeps what it last showed and is
//! only drawn when that changes. When it does, a painter that can keep
//! the rest of the window (the GL painter) draws only where it changed: a
//! logo animating over a still background costs the logo, not the window,
//! and two things moving in opposite corners cost the two, not all that
//! lies between them.

use std::sync::Arc;

use egui::Rect;
use egui::epaint::{ClippedShape, CornerRadius, Shape};
use ljos_render::b0::ShaderPaint;

/// What a window last showed.
#[derive(Debug, Default)]
pub(crate) struct LastPicture {
    /// `None` until something is shown, and after the platform may have
    /// lost it (a new surface, a new size, an exposed window).
    shown: Option<Shown>,
}

#[derive(Debug)]
struct Shown {
    shapes: Vec<ClippedShape>,
    pixels_per_point: f32,
    clear: egui::Color32,
}

impl LastPicture {
    /// What drawing `shapes` would change in what the window shows. Shapes
    /// the same as the last picture's, in the same order, draw the same
    /// pixels, so only where the others lie (before and now) can differ:
    /// each of those places is one of the areas.
    pub(crate) fn change(
        &self,
        shapes: &[ClippedShape],
        pixels_per_point: f32,
        clear: egui::Color32,
    ) -> Change {
        let Some(shown) = &self.shown else {
            return Change::All;
        };
        if shown.pixels_per_point != pixels_per_point || shown.clear != clear {
            return Change::All;
        }
        let (before, now) = (&shown.shapes[..], shapes);
        let mut areas = Vec::new();
        let mut add = |shape: &ClippedShape| {
            let area = extent(shape, pixels_per_point);
            if area.is_positive() {
                areas.push(area);
            }
        };
        if before.len() == now.len() {
            for (a, b) in before.iter().zip(now) {
                if !same_clipped(a, b) {
                    add(a);
                    add(b);
                }
            }
        } else {
            // Something came or went: what lies between the shapes both
            // start with and those both end with.
            let head = before
                .iter()
                .zip(now)
                .take_while(|(a, b)| same_clipped(a, b))
                .count();
            let (before, now) = (&before[head..], &now[head..]);
            let tail = before
                .iter()
                .rev()
                .zip(now.iter().rev())
                .take_while(|(a, b)| same_clipped(a, b))
                .count();
            let changed = before[..before.len() - tail]
                .iter()
                .chain(&now[..now.len() - tail]);
            changed.for_each(add);
        }
        if areas.is_empty() {
            return Change::None;
        }
        match scrolled(before, now, pixels_per_point) {
            // Moving what the window shows costs about as much as drawing
            // a quarter of it: a scroll is taken only where it spares more.
            Some(scroll) if covered(&scroll.areas) + scroll.rect.area() / 4.0 < covered(&areas) => {
                Change::Scroll(scroll)
            }
            _ => Change::Area(areas),
        }
    }

    /// The window now shows `shapes`.
    pub(crate) fn keep(
        &mut self,
        shapes: &[ClippedShape],
        pixels_per_point: f32,
        clear: egui::Color32,
    ) {
        match &mut self.shown {
            // Into the same allocation: this runs in every drawn frame.
            Some(shown) => {
                shown.shapes.clear();
                shown.shapes.extend_from_slice(shapes);
                shown.pixels_per_point = pixels_per_point;
                shown.clear = clear;
            }
            None => {
                self.shown = Some(Shown {
                    shapes: shapes.to_vec(),
                    pixels_per_point,
                    clear,
                });
            }
        }
    }

    /// What the window shows is not known: the next pass draws.
    pub(crate) fn forget(&mut self) {
        self.shown = None;
    }
}

/// What drawing a picture changes in the window, in points.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Change {
    None,
    /// Inside these areas, which may overlap.
    Area(Vec<Rect>),
    /// What one clip showed, moved: a painter that keeps what the window
    /// showed there may move it and draw only the areas.
    Scroll(Scroll),
    All,
}

/// The content of `rect` (on whole device pixels) moved `by` (whole device
/// pixels) from the last picture: moved there, it is the picture again
/// except inside `areas` (which may overlap, and take in what the move
/// brings into view). Outside `rect`, only `areas` change.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Scroll {
    pub(crate) rect: Rect,
    pub(crate) by: egui::Vec2,
    pub(crate) areas: Vec<Rect>,
}

/// How far ahead in the other picture a shape's moved twin is looked for,
/// past those that came into view or went out of it.
const AHEAD: usize = 64;

/// Whether the change from `before` to `now` is the content of one clip
/// moved by whole pixels (a list or a page scrolled), and what else then
/// changed. The content of a moving Flickable is drawn on whole pixels
/// (`flickable-whole-pixels.feature`), so each of its shapes is the same
/// as before, moved by the same distance, with the same clip; some come
/// into view and some go out. What does not move with them but lies over
/// or under them (a scroll bar, a header) is drawn again where it is and
/// where the move took its pixels.
fn scrolled(before: &[ClippedShape], now: &[ClippedShape], ppp: f32) -> Option<Scroll> {
    let head = before
        .iter()
        .zip(now)
        .take_while(|(a, b)| same_clipped(a, b))
        .count();
    let tail = before[head..]
        .iter()
        .rev()
        .zip(now[head..].iter().rev())
        .take_while(|(a, b)| same_clipped(a, b))
        .count();
    let (a, b) = (
        &before[head..before.len() - tail],
        &now[head..now.len() - tail],
    );
    // The clip and the distance: those the first shapes moved by most
    // often. Rows that look the same (a list's separators) are each
    // other moved by a row too, so the few most often found are each
    // tried, and the one that leaves least to draw is taken.
    let mut found: Vec<((Rect, egui::Vec2), usize)> = Vec::new();
    for x in a.iter().take(VOTERS) {
        for y in b.iter().take(AHEAD) {
            if x.clip_rect != y.clip_rect {
                continue;
            }
            let Some(by) = shift(&x.shape, &y.shape, ppp) else {
                continue;
            };
            match found.iter_mut().find(|(f, _)| *f == (x.clip_rect, by)) {
                Some((_, n)) => *n += 1,
                None => found.push(((x.clip_rect, by), 1)),
            }
        }
    }
    found.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
    found
        .iter()
        .take(TRIED)
        .filter_map(|&((rect, by), _)| moved_in(before, now, (head, tail), rect, by, ppp))
        .min_by(|p, q| covered(&p.areas).total_cmp(&covered(&q.areas)))
}

/// How many of the first shapes that changed say what moved.
const VOTERS: usize = 8;

/// How many of the clips and distances they found most often are tried.
const TRIED: usize = 3;

/// What else changes when the content of `rect` moved `by` from `before`
/// to `now`, which start with `head` shapes and end with `tail` that are
/// the same.
fn moved_in(
    before: &[ClippedShape],
    now: &[ClippedShape],
    (head, tail): (usize, usize),
    rect: Rect,
    by: egui::Vec2,
    ppp: f32,
) -> Option<Scroll> {
    let (a, b) = (
        &before[head..before.len() - tail],
        &now[head..now.len() - tail],
    );
    let on_pixels = |v: f32| ((v * ppp).round() - v * ppp).abs() < 1e-3;
    if !rect.is_finite()
        || ![rect.min.x, rect.min.y, rect.max.x, rect.max.y]
            .into_iter()
            .all(on_pixels)
        || by.x.abs() >= rect.width()
        || by.y.abs() >= rect.height()
    {
        return None;
    }
    let moved = |x: &ClippedShape, y: &ClippedShape| {
        let clipped = (x.clip_rect == rect && y.clip_rect == rect)
            || (y.clip_rect == x.clip_rect.translate(by)
                && rect.contains_rect(x.clip_rect)
                && rect.contains_rect(y.clip_rect));
        clipped && moved_by(&x.shape, &y.shape, by)
    };
    let mut areas = Vec::new();
    let mut add = |area: Rect| {
        if area.is_positive() {
            areas.push(area);
        }
    };
    // What comes into view.
    let mut moved_rect = rect.translate(by);
    if by.x > 0.0 {
        add(Rect::from_x_y_ranges(
            rect.min.x..=moved_rect.min.x,
            rect.y_range(),
        ));
    } else if by.x < 0.0 {
        add(Rect::from_x_y_ranges(
            moved_rect.max.x..=rect.max.x,
            rect.y_range(),
        ));
    }
    moved_rect = moved_rect.intersect(rect);
    if by.y > 0.0 {
        add(Rect::from_x_y_ranges(
            rect.x_range(),
            rect.min.y..=moved_rect.min.y,
        ));
    } else if by.y < 0.0 {
        add(Rect::from_x_y_ranges(
            rect.x_range(),
            moved_rect.max.y..=rect.max.y,
        ));
    }
    // A shape that went: where it was, and where the move took it.
    let went = |s: &ClippedShape, add: &mut dyn FnMut(Rect)| {
        let at = extent(s, ppp);
        add(at);
        add(at.translate(by).intersect(rect));
    };
    // One that stays where it is while what lies in the clip moves: where
    // it is, and where the move took its pixels, unless it is the same
    // everywhere in the clip (a background).
    let stays = |s: &ClippedShape, add: &mut dyn FnMut(Rect)| {
        let at = extent(s, ppp);
        if at.intersects(rect) && !even_over(s, rect) {
            add(at.intersect(rect));
            add(at.translate(by).intersect(rect));
        }
    };
    for s in before[..head].iter().chain(&before[before.len() - tail..]) {
        stays(s, &mut add);
    }
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if moved(&a[i], &b[j]) {
            (i, j) = (i + 1, j + 1);
            continue;
        }
        if same_clipped(&a[i], &b[j]) {
            stays(&a[i], &mut add);
            (i, j) = (i + 1, j + 1);
            continue;
        }
        // Shapes came into view before the next that moved, or went out
        // of it: whichever skips fewer.
        let came = (1..AHEAD).find(|&k| b.get(j + k).is_some_and(|y| moved(&a[i], y)));
        let gone = (1..AHEAD).find(|&k| a.get(i + k).is_some_and(|x| moved(x, &b[j])));
        match (came, gone) {
            (Some(k), g) if g.is_none_or(|g| k <= g) => {
                b[j..j + k].iter().for_each(|s| add(extent(s, ppp)));
                j += k;
            }
            (_, Some(k)) => {
                a[i..i + k].iter().for_each(|s| went(s, &mut add));
                i += k;
            }
            _ => {
                went(&a[i], &mut add);
                add(extent(&b[j], ppp));
                (i, j) = (i + 1, j + 1);
            }
        }
    }
    a[i..].iter().for_each(|s| went(s, &mut add));
    b[j..].iter().for_each(|s| add(extent(s, ppp)));
    Some(Scroll { rect, by, areas })
}

/// How far `b` lies from `a` when it is `a` moved by whole pixels, not
/// by none.
fn shift(a: &Shape, b: &Shape, ppp: f32) -> Option<egui::Vec2> {
    let by = match (a, b) {
        (Shape::Mesh(a), Shape::Mesh(b)) => b.vertices.first()?.pos - a.vertices.first()?.pos,
        (Shape::Callback(_), _) | (_, Shape::Callback(_)) => return None,
        _ => b.visual_bounding_rect().min - a.visual_bounding_rect().min,
    };
    let px = (by * ppp).round();
    if px == egui::Vec2::ZERO || ((by * ppp) - px).length() > 1e-3 {
        return None;
    }
    let by = px / ppp;
    moved_by(a, b, by).then_some(by)
}

/// Whether `b` is `a` moved by `by`. A mesh's vertices are placed again
/// in each frame, so they may be a rounding apart.
fn moved_by(a: &Shape, b: &Shape, by: egui::Vec2) -> bool {
    match (a, b) {
        (Shape::Mesh(a), Shape::Mesh(b)) => {
            a.texture_id == b.texture_id
                && a.indices == b.indices
                && a.vertices.len() == b.vertices.len()
                && a.vertices.iter().zip(&b.vertices).all(|(u, v)| {
                    u.color == v.color
                        && u.uv == v.uv
                        && (v.pos - u.pos - by).abs().max_elem() < 1e-3
                })
        }
        (Shape::Vec(a), Shape::Vec(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| moved_by(a, b, by))
        }
        (Shape::Callback(_), _) | (_, Shape::Callback(_)) => false,
        (a, b) => {
            let mut a = a.clone();
            a.translate(by);
            a == *b
        }
    }
}

/// Whether `s` draws the same over all of `rect`: a filled rectangle with
/// square corners that covers it.
fn even_over(s: &ClippedShape, rect: Rect) -> bool {
    match &s.shape {
        Shape::Rect(r) => {
            r.corner_radius == CornerRadius::ZERO
                && r.stroke.is_empty()
                && r.fill_texture_id() == egui::TextureId::default()
                && r.blur_width == 0.0
                && r.rect.contains_rect(rect)
                && s.clip_rect.contains_rect(rect)
        }
        _ => false,
    }
}

/// The pixels `areas` cover, counted again where they overlap.
fn covered(areas: &[Rect]) -> f32 {
    areas.iter().map(|a| a.area()).sum()
}

/// How long the steps of a painted pass took on this thread, where the
/// painter times them apart (the inspector's `app_metrics`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Steps {
    pub(crate) upload: Option<std::time::Duration>,
    pub(crate) draw: Option<std::time::Duration>,
    pub(crate) present: Option<std::time::Duration>,
}

/// What a painter draws of a picture.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Frame {
    /// Nothing: the window shows it already (the pictures handed over are
    /// still taken).
    Nothing,
    /// These areas (in points, on whole pixels, none overlapping another);
    /// what lies outside them in the window's buffer is the picture
    /// already.
    #[cfg_attr(
        not(gl_painter),
        expect(dead_code, reason = "only the GL painter draws part of a window")
    )]
    Part(Vec<Rect>),
    Whole,
}

/// Where `shape` can draw: its edges are smoothed a pixel out.
fn extent(shape: &ClippedShape, pixels_per_point: f32) -> Rect {
    shape
        .shape
        .visual_bounding_rect()
        .expand(1.0 / pixels_per_point)
        .intersect(shape.clip_rect)
}

/// Draws `shapes` only inside `areas`, which do not overlap: a shape is
/// drawn once in each area it reaches, its clip rectangle cut to it, and
/// not at all if it reaches none. Each area gets its shapes in their
/// order. A shape wholly inside one area keeps its own clip rectangle, so
/// shapes that shared one are still drawn together.
///
/// A shape is compared only with the areas whose tops lie within the
/// tallest area's height above it and its bottom, found by a binary search
/// over the areas by their tops: two hundred shapes over two hundred
/// areas are not forty thousand comparisons.
pub(crate) fn clip_to(shapes: &mut Vec<ClippedShape>, areas: &[Rect], pixels_per_point: f32) {
    let mut by_top: Vec<usize> = (0..areas.len()).collect();
    by_top.sort_by(|&a, &b| areas[a].min.y.total_cmp(&areas[b].min.y));
    let tallest = areas.iter().map(Rect::height).fold(0.0, f32::max);
    let mut near = Vec::new();
    let all = std::mem::take(shapes);
    shapes.reserve(all.len());
    for s in all {
        let reach = extent(&s, pixels_per_point);
        near.clear();
        if reach.min.y <= reach.max.y {
            let from = by_top.partition_point(|&a| areas[a].min.y < reach.min.y - tallest);
            let to = by_top.partition_point(|&a| areas[a].min.y <= reach.max.y);
            near.extend_from_slice(&by_top[from..to.max(from)]);
            // In the order the areas were given.
            near.sort_unstable();
        } else {
            near.extend(0..areas.len());
        }
        count_compared(near.len());
        let near = near.iter().map(|&a| areas[a]);
        if near.clone().any(|area| area.contains_rect(reach)) {
            shapes.push(s);
            continue;
        }
        let mut inside = near
            .map(|area| s.clip_rect.intersect(area))
            .filter(|clip| clip.is_positive() && reach.intersect(*clip).is_positive())
            .peekable();
        while let Some(clip_rect) = inside.next() {
            if inside.peek().is_none() {
                // The last one takes the shape itself.
                shapes.push(ClippedShape {
                    clip_rect,
                    shape: s.shape,
                });
                break;
            }
            shapes.push(ClippedShape {
                clip_rect,
                shape: s.shape.clone(),
            });
        }
    }
}

#[cfg(test)]
thread_local! {
    static COMPARED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn count_compared(_n: usize) {
    #[cfg(test)]
    COMPARED.set(COMPARED.get() + _n);
}

fn same_clipped(a: &ClippedShape, b: &ClippedShape) -> bool {
    a.clip_rect == b.clip_rect && same_shape(&a.shape, &b.shape)
}

/// The scene's meshes are cached per node and handed out as the same
/// `Arc` while the node is unchanged, so most are told apart or found
/// equal without looking at their vertices.
fn same_shape(a: &Shape, b: &Shape) -> bool {
    match (a, b) {
        (Shape::Mesh(a), Shape::Mesh(b)) => Arc::ptr_eq(a, b) || a == b,
        (Shape::Vec(a), Shape::Vec(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_shape(a, b))
        }
        // B0 hands out a shader effect's paint as the same `Arc` while
        // nothing it draws with changes.
        (Shape::Callback(a), Shape::Callback(b))
            if a.callback.is::<ShaderPaint>() && b.callback.is::<ShaderPaint>() =>
        {
            a.rect == b.rect && Arc::ptr_eq(&a.callback, &b.callback)
        }
        // What another callback paints is its own; it may differ with the
        // same callback, so a window with one is always drawn.
        (Shape::Callback(_), _) | (_, Shape::Callback(_)) => false,
        (a, b) => a == b,
    }
}

#[cfg(test)]
mod tests {
    use egui::epaint::{CornerRadius, Mesh, RectShape};
    use egui::{Color32, Pos2, vec2};

    use super::*;

    fn rect(x: f32) -> ClippedShape {
        ClippedShape {
            clip_rect: Rect::EVERYTHING,
            shape: Shape::Rect(RectShape::filled(
                Rect::from_min_size(Pos2::new(x, 0.0), vec2(10.0, 10.0)),
                CornerRadius::ZERO,
                Color32::RED,
            )),
        }
    }

    fn mesh(mesh: &Arc<Mesh>) -> ClippedShape {
        ClippedShape {
            clip_rect: Rect::EVERYTHING,
            shape: Shape::Mesh(Arc::clone(mesh)),
        }
    }

    /// Counts the frames drawn for a run of passes, as the shell decides.
    fn drawn(passes: &[Vec<ClippedShape>]) -> usize {
        let mut last = LastPicture::default();
        let mut frames = 0;
        for shapes in passes {
            if last.change(shapes, 1.0, Color32::WHITE) != Change::None {
                frames += 1;
                last.keep(shapes, 1.0, Color32::WHITE);
            }
        }
        frames
    }

    #[test]
    fn passes_that_change_nothing_draw_one_frame() {
        // A pointer moving over items that do not react: ten passes, the
        // same picture.
        let passes = vec![vec![rect(0.0), rect(20.0)]; 10];
        assert_eq!(drawn(&passes), 1);
    }

    #[test]
    fn every_pass_that_moves_something_is_drawn() {
        let passes: Vec<_> = (0..10).map(|i| vec![rect(i as f32)]).collect();
        assert_eq!(drawn(&passes), 10);
    }

    #[test]
    fn a_shape_more_or_less_is_a_change() {
        let passes = vec![vec![rect(0.0)], vec![rect(0.0), rect(1.0)], vec![rect(0.0)]];
        assert_eq!(drawn(&passes), 3);
    }

    #[test]
    fn cached_meshes_are_the_same_picture() {
        let m = Arc::new(Mesh::default());
        let again = Arc::new(Mesh::default());
        let passes = vec![vec![mesh(&m)], vec![mesh(&m)], vec![mesh(&again)]];
        assert_eq!(
            drawn(&passes),
            1,
            "equal meshes, shared or not, are no change"
        );
    }

    fn shader(paint: &Arc<ShaderPaint>) -> ClippedShape {
        ClippedShape {
            clip_rect: Rect::EVERYTHING,
            shape: Shape::Callback(egui::PaintCallback {
                rect: Rect::from_min_size(Pos2::ZERO, vec2(10.0, 10.0)),
                callback: Arc::clone(paint) as _,
            }),
        }
    }

    #[test]
    fn a_shader_effect_unchanged_is_the_same_picture() {
        use ljos_render::kurbo::{Affine, Rect as KRect};
        use ljos_render::ljos_shader::{Program, default_fragment, default_vertex};
        use ljos_render::{Cull, ShaderOp};
        let program = Arc::new(Program::new(default_vertex(), default_fragment()).unwrap());
        let paint = || {
            Arc::new(ShaderPaint {
                op: Arc::new(ShaderOp {
                    program: Arc::clone(&program),
                    uniforms: Vec::new(),
                    textures: Vec::new(),
                    grid: (1, 1),
                    blending: true,
                    cull: Cull::None,
                }),
                rect: KRect::new(0.0, 0.0, 10.0, 10.0),
                to_screen: Affine::IDENTITY,
                opacity: 1.0,
                ppp: 1.0,
                textures: Vec::new(),
            })
        };
        let (a, b) = (paint(), paint());
        let passes = vec![vec![shader(&a)], vec![shader(&a)], vec![shader(&b)]];
        assert_eq!(
            drawn(&passes),
            2,
            "a new paint is drawn, the same one is not"
        );
    }

    /// Where `rect(x)` draws, its smoothed edge with it.
    fn at(x: f32) -> Rect {
        Rect::from_min_size(Pos2::new(x - 1.0, -1.0), vec2(12.0, 12.0))
    }

    fn change(before: &[ClippedShape], now: &[ClippedShape]) -> Change {
        let mut last = LastPicture::default();
        last.keep(before, 1.0, Color32::WHITE);
        last.change(now, 1.0, Color32::WHITE)
    }

    #[test]
    fn only_where_a_shape_moved_from_and_to_is_drawn_again() {
        let before = [rect(0.0), rect(20.0), rect(40.0)];
        assert_eq!(change(&before, &before), Change::None);
        let now = [rect(0.0), rect(25.0), rect(40.0)];
        assert_eq!(
            change(&before, &now),
            Change::Area(vec![at(20.0), at(25.0)])
        );
    }

    #[test]
    fn a_shape_that_comes_or_goes_changes_only_where_it_lies() {
        let before = [rect(0.0), rect(40.0)];
        let now = [rect(0.0), rect(20.0), rect(40.0)];
        assert_eq!(change(&before, &now), Change::Area(vec![at(20.0)]));
        assert_eq!(change(&now, &before), Change::Area(vec![at(20.0)]));
    }

    #[test]
    fn a_shape_changes_only_what_its_clip_lets_through() {
        let before = [rect(0.0)];
        let mut now = [rect(5.0)];
        now[0].clip_rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(12.0, 12.0));
        let now_seen = Rect::from_min_max(Pos2::new(4.0, 0.0), Pos2::new(12.0, 11.0));
        assert_eq!(change(&before, &now), Change::Area(vec![at(0.0), now_seen]));
    }

    #[test]
    fn another_scale_background_or_first_picture_changes_everything() {
        let mut last = LastPicture::default();
        assert_eq!(last.change(&[rect(0.0)], 1.0, Color32::WHITE), Change::All);
        last.keep(&[rect(0.0)], 1.0, Color32::WHITE);
        assert_eq!(last.change(&[rect(0.0)], 2.0, Color32::WHITE), Change::All);
        assert_eq!(last.change(&[rect(0.0)], 1.0, Color32::BLACK), Change::All);
    }

    #[test]
    fn two_shapes_far_apart_change_only_where_they_are() {
        let before = [rect(0.0), rect(20.0), rect(40.0), rect(60.0)];
        let now = [rect(1.0), rect(20.0), rect(40.0), rect(61.0)];
        let Change::Area(areas) = change(&before, &now) else {
            panic!("a change");
        };
        assert!(
            areas.iter().all(|a| !a.intersects(at(20.0).shrink(1.5))),
            "what lies between them is not drawn: {areas:?}"
        );
    }

    #[test]
    fn shapes_drawn_into_an_area_keep_to_it() {
        let mut shapes = vec![rect(0.0), rect(20.0), rect(40.0)];
        let area = Rect::from_min_max(Pos2::new(15.0, 0.0), Pos2::new(35.0, 10.0));
        clip_to(&mut shapes, &[area], 1.0);
        assert_eq!(shapes.len(), 1, "the shapes outside it are not drawn");
        assert_eq!(shapes[0].clip_rect, area);
    }

    #[test]
    fn a_shape_is_drawn_in_each_area_it_reaches_in_paint_order() {
        let wide = ClippedShape {
            clip_rect: Rect::EVERYTHING,
            shape: Shape::Rect(RectShape::filled(
                Rect::from_min_size(Pos2::ZERO, vec2(100.0, 10.0)),
                CornerRadius::ZERO,
                Color32::BLUE,
            )),
        };
        let mut shapes = vec![wide, rect(0.0), rect(80.0)];
        let left = Rect::from_min_max(Pos2::ZERO, Pos2::new(10.0, 10.0));
        let right = Rect::from_min_max(Pos2::new(80.0, 0.0), Pos2::new(90.0, 10.0));
        clip_to(&mut shapes, &[left, right], 1.0);
        let clips: Vec<Rect> = shapes.iter().map(|s| s.clip_rect).collect();
        assert_eq!(
            clips,
            [left, right, left, right],
            "the wide one in both, under the ones on top of it"
        );
    }

    #[test]
    fn shapes_wholly_inside_an_area_keep_their_clip() {
        let mut shapes = vec![rect(0.0), rect(20.0), rect(40.0)];
        let area = Rect::from_min_max(Pos2::new(-5.0, -5.0), Pos2::new(35.0, 15.0));
        clip_to(&mut shapes, &[area], 1.0);
        let clips: Vec<Rect> = shapes.iter().map(|s| s.clip_rect).collect();
        assert_eq!(clips, [Rect::EVERYTHING; 2]);
    }

    #[test]
    fn each_shape_is_compared_only_with_the_areas_near_it() {
        // Two hundred bars, one a row, each with the area it moved in.
        let bar = |row: f32| ClippedShape {
            clip_rect: Rect::EVERYTHING,
            shape: Shape::Rect(RectShape::filled(
                Rect::from_min_size(Pos2::new(row * 3.0 % 700.0, row * 6.0), vec2(12.0, 3.0)),
                CornerRadius::ZERO,
                Color32::RED,
            )),
        };
        let mut shapes: Vec<_> = (0..200).map(|r| bar(r as f32)).collect();
        let areas: Vec<_> = shapes
            .iter()
            .map(|s| extent(s, 1.0).expand2(vec2(2.0, 0.0)))
            .collect();
        COMPARED.set(0);
        clip_to(&mut shapes, &areas, 1.0);
        assert_eq!(shapes.len(), 200, "each in its own area");
        assert!(shapes.iter().all(|s| s.clip_rect == Rect::EVERYTHING));
        assert!(
            COMPARED.get() <= 200 * 3,
            "{} comparisons, not 40 000",
            COMPARED.get()
        );
    }

    const LIST: Rect = Rect {
        min: Pos2::ZERO,
        max: Pos2::new(100.0, 100.0),
    };

    /// Row `i` of a list in `LIST`, at `y`.
    fn row(i: u8, y: f32) -> ClippedShape {
        ClippedShape {
            clip_rect: LIST,
            shape: Shape::Rect(RectShape::filled(
                Rect::from_min_size(Pos2::new(0.0, y), vec2(100.0, 10.0)),
                CornerRadius::ZERO,
                Color32::from_rgb(i * 20, 0, 0),
            )),
        }
    }

    fn rows(from: f32) -> Vec<ClippedShape> {
        (0..10)
            .map(|i| row(i, from + f32::from(i) * 20.0))
            .collect()
    }

    fn scroll(before: &[ClippedShape], now: &[ClippedShape]) -> Scroll {
        match change(before, now) {
            Change::Scroll(scroll) => scroll,
            other => panic!("a scroll, not {other:?}"),
        }
    }

    #[test]
    fn a_list_moved_by_whole_pixels_is_a_scroll_that_draws_what_comes_into_view() {
        let s = scroll(&rows(0.0), &rows(-3.0));
        assert_eq!((s.rect, s.by), (LIST, vec2(0.0, -3.0)));
        let strip = Rect::from_min_max(Pos2::new(0.0, 97.0), Pos2::new(100.0, 100.0));
        assert_eq!(s.areas, [strip]);
    }

    #[test]
    fn a_row_that_comes_into_view_is_drawn() {
        let now = &rows(-3.0)[..6];
        let s = scroll(&rows(0.0)[..5], now);
        assert!(
            s.areas.contains(&extent(&now[5], 1.0)),
            "the row that came: {:?}",
            s.areas
        );
        let mut gone = rows(-3.0);
        gone.remove(0);
        let s = scroll(&rows(0.0), &gone);
        let went = extent(&row(0, 0.0), 1.0);
        assert!(s.areas.contains(&went), "where the row was: {:?}", s.areas);
        assert!(
            s.areas
                .contains(&went.translate(vec2(0.0, -3.0)).intersect(LIST)),
            "where the move took it: {:?}",
            s.areas
        );
    }

    #[test]
    fn what_stays_over_a_moving_list_is_drawn_where_it_is_and_where_it_was_moved() {
        let bar = ClippedShape {
            clip_rect: Rect::EVERYTHING,
            shape: Shape::Rect(RectShape::filled(
                Rect::from_min_max(Pos2::new(95.0, 10.0), Pos2::new(100.0, 30.0)),
                CornerRadius::ZERO,
                Color32::BLUE,
            )),
        };
        let (mut before, mut now) = (rows(0.0), rows(-3.0));
        before.push(bar.clone());
        now.push(bar.clone());
        let s = scroll(&before, &now);
        let at = extent(&bar, 1.0);
        assert!(s.areas.contains(&at.intersect(LIST)), "{:?}", s.areas);
        let moved = at.translate(vec2(0.0, -3.0)).intersect(LIST);
        assert!(s.areas.contains(&moved), "{:?}", s.areas);
    }

    #[test]
    fn a_background_under_a_moving_list_is_not_drawn_again() {
        let back = ClippedShape {
            clip_rect: Rect::EVERYTHING,
            shape: Shape::Rect(RectShape::filled(
                Rect::from_min_max(Pos2::ZERO, Pos2::new(200.0, 200.0)),
                CornerRadius::ZERO,
                Color32::WHITE,
            )),
        };
        let (mut before, mut now) = (vec![back.clone()], vec![back]);
        before.extend(rows(0.0));
        now.extend(rows(-3.0));
        assert_eq!(scroll(&before, &now).areas.len(), 1, "only the strip");
    }

    #[test]
    fn a_move_between_pixels_is_no_scroll() {
        assert!(matches!(change(&rows(0.0), &rows(-0.5)), Change::Area(_)));
    }

    #[test]
    fn one_small_thing_moving_in_a_large_clip_is_drawn_where_it_moves() {
        let dot = |x: f32| ClippedShape {
            clip_rect: LIST,
            shape: Shape::Rect(RectShape::filled(
                Rect::from_min_size(Pos2::new(x, 50.0), vec2(4.0, 4.0)),
                CornerRadius::ZERO,
                Color32::RED,
            )),
        };
        assert!(matches!(
            change(&[dot(10.0)], &[dot(12.0)]),
            Change::Area(_)
        ));
    }

    #[test]
    fn a_forgotten_picture_is_drawn_again() {
        let mut last = LastPicture::default();
        last.keep(&[], 1.0, Color32::WHITE);
        assert_eq!(last.change(&[], 1.0, Color32::WHITE), Change::None);
        last.forget();
        assert_eq!(last.change(&[], 1.0, Color32::WHITE), Change::All);
    }
}
