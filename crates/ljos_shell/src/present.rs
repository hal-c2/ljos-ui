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

use egui::emath::TSTransform;
use egui::epaint::{ClippedPrimitive, ClippedShape, CornerRadius, Mesh, Primitive, Shape};
use egui::{Rect, TextureId};
use ljos_render::b0::ShaderPaint;
#[cfg(feature = "quick3d")]
use ljos_render::scene3d::View3dPaint;

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
        // Moving what the window shows costs about as much as drawing a
        // quarter of it: a scroll is taken only where it spares more.
        match scrolled(before, now, pixels_per_point, covered(&areas)) {
            Some(scroll) => Change::Scroll(scroll),
            None => Change::Area(areas),
        }
    }

    /// [`LastPicture::change`] for a painter that draws the window whole
    /// whatever changed: only whether something did, so it stops at the
    /// first shape that differs and works out no areas.
    pub(crate) fn whole_change(
        &self,
        shapes: &[ClippedShape],
        pixels_per_point: f32,
        clear: egui::Color32,
    ) -> Change {
        let Some(shown) = &self.shown else {
            return Change::All;
        };
        let same = shown.pixels_per_point == pixels_per_point
            && shown.clear == clear
            && shown.shapes.len() == shapes.len()
            && shown
                .shapes
                .iter()
                .zip(shapes)
                .all(|(a, b)| same_clipped(a, b));
        if same { Change::None } else { Change::All }
    }

    /// The window now shows `shapes`, which are kept as they are and lent
    /// back to be drawn: copied, they were a clone and a drop of every
    /// shape in every drawn frame.
    pub(crate) fn keep(
        &mut self,
        shapes: Vec<ClippedShape>,
        pixels_per_point: f32,
        clear: egui::Color32,
    ) -> &[ClippedShape] {
        &self
            .shown
            .insert(Shown {
                shapes,
                pixels_per_point,
                clear,
            })
            .shapes
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

impl Change {
    /// For a painter that draws the whole window or nothing.
    pub(crate) fn whole(self) -> Change {
        match self {
            Change::None => Change::None,
            _ => Change::All,
        }
    }

    /// This change, and the pictures drawn again in `areas` too.
    pub(crate) fn with(self, areas: Vec<Rect>) -> Change {
        if areas.is_empty() {
            return self;
        }
        match self {
            Change::None => Change::Area(areas),
            Change::Area(mut a) => {
                a.extend(areas);
                Change::Area(a)
            }
            Change::Scroll(mut s) => {
                s.areas.extend(areas);
                Change::Scroll(s)
            }
            Change::All => Change::All,
        }
    }

    /// Where the picture changed, as boxes `[x, y, width, height]` in
    /// points; `None` for all of it.
    #[cfg(feature = "mcp")]
    pub(crate) fn boxes(&self) -> Option<Vec<[f64; 4]>> {
        let of = |r: &Rect| {
            [
                f64::from(r.min.x),
                f64::from(r.min.y),
                f64::from(r.width()),
                f64::from(r.height()),
            ]
        };
        match self {
            Change::None => Some(Vec::new()),
            Change::Area(areas) => Some(areas.iter().map(of).collect()),
            Change::Scroll(s) => Some(std::iter::once(&s.rect).chain(&s.areas).map(of).collect()),
            Change::All => None,
        }
    }
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
/// A scroll is only found where it leaves less to draw than `drawn`, the
/// pixels the shapes that changed cover: one that does not is given up on
/// as soon as that is so, as most are where everything moves differently.
fn scrolled(before: &[ClippedShape], now: &[ClippedShape], ppp: f32, drawn: f32) -> Option<Scroll> {
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
        if scroll_cost(x.clip_rect) >= drawn {
            continue;
        }
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
    // Each one tried after a scroll is found has to leave less to draw
    // than it does, and is given up on once it does not: the shapes of a
    // row moved by a row are compared no further than that.
    let mut best: Option<Scroll> = None;
    for &((rect, by), _) in found.iter().take(TRIED) {
        let mut budget = drawn - scroll_cost(rect);
        if let Some(b) = &best {
            budget = budget.min(covered(&b.areas));
        }
        if let Some(s) = moved_in(before, now, (head, tail), rect, by, ppp, budget) {
            best = Some(s);
        }
    }
    best
}

/// What moving the pixels of `rect` costs, as pixels drawn.
fn scroll_cost(rect: Rect) -> f32 {
    rect.area() / 4.0
}

/// How many of the first shapes that changed say what moved.
const VOTERS: usize = 8;

/// How many of the clips and distances they found most often are tried.
const TRIED: usize = 3;

/// What else changes when the content of `rect` moved `by` from `before`
/// to `now`, which start with `head` shapes and end with `tail` that are
/// the same. `None` as well once that covers `budget` pixels or more.
fn moved_in(
    before: &[ClippedShape],
    now: &[ClippedShape],
    (head, tail): (usize, usize),
    rect: Rect,
    by: egui::Vec2,
    ppp: f32,
    budget: f32,
) -> Option<Scroll> {
    if budget <= 0.0 {
        return None;
    }
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
    let spent = std::cell::Cell::new(0.0f32);
    let mut add = |area: Rect| {
        if area.is_positive() {
            spent.set(spent.get() + area.area());
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
        if spent.get() >= budget {
            return None;
        }
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
        // What went is taken only where fewer went than came: no further
        // is looked.
        let gone =
            (1..came.unwrap_or(AHEAD)).find(|&k| a.get(i + k).is_some_and(|x| moved(x, &b[j])));
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
    (spent.get() < budget).then_some(Scroll { rect, by, areas })
}

/// How far `b` lies from `a` when it is `a` moved by whole pixels, not
/// by none.
fn shift(a: &Shape, b: &Shape, ppp: f32) -> Option<egui::Vec2> {
    if std::mem::discriminant(a) != std::mem::discriminant(b) {
        return None;
    }
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
            // Most pairs looked at are not the same shape: the cheap
            // differences first, the indices last.
            a.texture_id == b.texture_id
                && a.vertices.len() == b.vertices.len()
                && a.indices.len() == b.indices.len()
                && a.vertices.iter().zip(&b.vertices).all(|(u, v)| {
                    u.color == v.color
                        && u.uv == v.uv
                        && (v.pos - u.pos - by).abs().max_elem() < 1e-3
                })
                && a.indices == b.indices
        }
        (Shape::Vec(a), Shape::Vec(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| moved_by(a, b, by))
        }
        (Shape::Callback(_), _) | (_, Shape::Callback(_)) => false,
        // Moved, a shape is still its kind; and moving a copy of a mesh
        // copies its vertices.
        (Shape::Mesh(_), _) | (_, Shape::Mesh(_)) => false,
        (a, b) if std::mem::discriminant(a) != std::mem::discriminant(b) => false,
        (a, b) => {
            // Most shapes looked at are not the other moved: told by their
            // kind and bounds, before a copy is moved to be compared.
            if std::mem::discriminant(a) != std::mem::discriminant(b)
                || (b.visual_bounding_rect().min - a.visual_bounding_rect().min - by)
                    .abs()
                    .max_elem()
                    > 0.5
            {
                return false;
            }
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
    /// The GPU time of 3D views read back during the pass: an earlier
    /// pass's drawing, as nothing waits for the GPU.
    pub(crate) gpu_3d: Option<std::time::Duration>,
    /// Whether the pass moved the pixels of a scrolled clip rather than
    /// drawing them.
    pub(crate) moved: bool,
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
    // The clip rectangle is in the space the transform maps to.
    (shape.transform_after_tessellation * bounds(&shape.shape))
        .expand(1.0 / pixels_per_point)
        .intersect(shape.clip_rect)
}

/// `shape.visual_bounding_rect()`, a mesh's in plain comparisons:
/// `f32::min` minds a NaN at each vertex, and the texts of a scrolled list
/// were most of what the GL painter spent finding where shapes draw. A NaN
/// is passed over here as there.
pub(crate) fn bounds(shape: &Shape) -> Rect {
    let Shape::Mesh(mesh) = shape else {
        return shape.visual_bounding_rect();
    };
    let (mut x0, mut y0) = (f32::INFINITY, f32::INFINITY);
    let (mut x1, mut y1) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
    for v in &mesh.vertices {
        let egui::Pos2 { x, y } = v.pos;
        x0 = if x < x0 { x } else { x0 };
        y0 = if y < y0 { y } else { y0 };
        x1 = if x > x1 { x } else { x1 };
        y1 = if y > y1 { y } else { y1 };
    }
    Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y1))
}

/// Where `shapes`, the same as before, show something else: they sample
/// texels that a frame hands over anew. Each of `changed` is a texture and
/// the texels it changes, as fractions of its size (`None`: any). An atlas
/// adds a glyph or a picture where nothing sampled it yet, so the text
/// and pictures already shown stay as they are: only a texture changed
/// under them makes them draw again, not one that grew beside them.
pub(crate) fn resampled(
    shapes: &[ClippedShape],
    changed: &[(TextureId, Option<Rect>)],
    pixels_per_point: f32,
) -> Vec<Rect> {
    if changed.is_empty() {
        return Vec::new();
    }
    shapes
        .iter()
        .filter(|s| samples(&s.shape, changed))
        .map(|s| extent(s, pixels_per_point))
        .filter(|area| area.is_positive())
        .collect()
}

/// The texels of a `size` picture that a `part` handed over at `at`
/// changes, as fractions of it, and half a texel more: filtered, a quad
/// reads that far past its own.
pub(crate) fn texels(at: [usize; 2], part: [usize; 2], size: [usize; 2]) -> Rect {
    let f = |x: usize, by: f32, side: usize| (x as f32 + by) / side as f32;
    Rect::from_min_max(
        egui::pos2(f(at[0], -0.5, size[0]), f(at[1], -0.5, size[1])),
        egui::pos2(
            f(at[0] + part[0], 0.5, size[0]),
            f(at[1] + part[1], 0.5, size[1]),
        ),
    )
}

/// Whether `shape` samples any of the texels of `changed`.
fn samples(shape: &Shape, changed: &[(TextureId, Option<Rect>)]) -> bool {
    // Not only touching: a glyph filled beside one shown, past the texel
    // of padding between them, is not read by it.
    let overlap = |a: Rect, b: Rect| {
        a.min.x < b.max.x && b.min.x < a.max.x && a.min.y < b.max.y && b.min.y < a.max.y
    };
    let reads = |id: TextureId, uv: Rect| {
        changed
            .iter()
            .any(|&(t, texels)| t == id && texels.is_none_or(|texels| overlap(texels, uv)))
    };
    match shape {
        // Triangle by triangle: one mesh draws a run of glyphs from all
        // over the atlas.
        Shape::Mesh(mesh) => {
            changed.iter().any(|&(t, _)| t == mesh.texture_id)
                && mesh.indices.as_chunks::<3>().0.iter().any(|tri| {
                    let uv = tri.iter().fold(Rect::NOTHING, |r, &i| {
                        r.union(Rect::from_pos(mesh.vertices[i as usize].uv))
                    });
                    reads(mesh.texture_id, uv)
                })
        }
        Shape::Vec(shapes) => shapes.iter().any(|s| samples(s, changed)),
        // What a callback samples is not known.
        Shape::Callback(_) => true,
        Shape::Rect(rect) => match &rect.brush {
            Some(brush) => reads(brush.fill_texture_id, brush.uv),
            None => reads(TextureId::default(), Rect::from_pos(egui::epaint::WHITE_UV)),
        },
        // egui's glyphs, anywhere in its atlas.
        Shape::Text(_) => reads(TextureId::default(), Rect::EVERYTHING),
        // Tessellated in flat colour, from the white texel of egui's atlas.
        _ => reads(TextureId::default(), Rect::from_pos(egui::epaint::WHITE_UV)),
    }
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
                shapes.push(ClippedShape { clip_rect, ..s });
                break;
            }
            shapes.push(ClippedShape {
                clip_rect,
                ..s.clone()
            });
        }
    }
}

/// The primitives of `shapes`, as [`egui::Context::tessellate`] makes
/// them, but with a mesh handed over as it is: the tessellator walks every
/// vertex of one for its bounds and then copies it, and nearly all of a
/// scene's shapes are meshes (two hundred bars are made one, each image is
/// one). Meshes one after another in one clip rectangle and texture are
/// still one primitive; the other shapes go to egui in runs.
pub(crate) fn tessellate(
    ctx: &egui::Context,
    shapes: Vec<ClippedShape>,
    pixels_per_point: f32,
) -> Vec<ClippedPrimitive> {
    let mut out = Vec::with_capacity(shapes.len());
    // At most one of the two holds anything.
    let mut rest = Vec::new();
    let mut run: Vec<Arc<Mesh>> = Vec::new();
    let mut run_clip = Rect::NOTHING;
    for s in shapes {
        // A mesh moved after tessellation is egui's to move.
        let clip_rect = s.clip_rect;
        let mesh = match s.shape {
            Shape::Mesh(mesh) if s.transform_after_tessellation == TSTransform::IDENTITY => mesh,
            shape => {
                end_run(&mut out, &mut run, run_clip);
                rest.push(ClippedShape { shape, ..s });
                continue;
            }
        };
        if !clip_rect.is_positive() || mesh.is_empty() {
            continue;
        }
        if !rest.is_empty() {
            out.extend(ctx.tessellate(std::mem::take(&mut rest), pixels_per_point));
        }
        if run
            .first()
            .is_some_and(|m| run_clip != clip_rect || m.texture_id != mesh.texture_id)
        {
            end_run(&mut out, &mut run, run_clip);
        }
        run_clip = clip_rect;
        run.push(mesh);
    }
    end_run(&mut out, &mut run, run_clip);
    if !rest.is_empty() {
        out.extend(ctx.tessellate(rest, pixels_per_point));
    }
    out
}

/// Makes the meshes of `run` one primitive, and leaves `run` empty. One
/// mesh nothing else holds is moved, not copied.
fn end_run(out: &mut Vec<ClippedPrimitive>, run: &mut Vec<Arc<Mesh>>, clip_rect: Rect) {
    let mesh = match run.len() {
        0 => return,
        1 => Arc::unwrap_or_clone(run.pop().expect("one mesh")),
        _ => {
            let mut mesh = Mesh::with_texture(run[0].texture_id);
            mesh.reserve_vertices(run.iter().map(|m| m.vertices.len()).sum());
            mesh.indices
                .reserve(run.iter().map(|m| m.indices.len()).sum());
            for m in run.drain(..) {
                mesh.append_ref(&m);
            }
            mesh
        }
    };
    out.push(ClippedPrimitive {
        clip_rect,
        primitive: Primitive::Mesh(mesh),
    });
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
        // A 3D view's scene is handed out as the same `Arc` while nothing
        // in it changes, in a new callback each frame.
        #[cfg(feature = "quick3d")]
        (Shape::Callback(a), Shape::Callback(b))
            if a.callback.is::<View3dPaint>() && b.callback.is::<View3dPaint>() =>
        {
            let (va, vb) = (
                a.callback.downcast_ref::<View3dPaint>(),
                b.callback.downcast_ref::<View3dPaint>(),
            );
            a.rect == b.rect
                && va.zip(vb).is_some_and(|(va, vb)| {
                    va.key == vb.key
                        && va.size_px == vb.size_px
                        && Arc::ptr_eq(&va.scene, &vb.scene)
                })
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
        ClippedShape::new(
            Rect::EVERYTHING,
            Shape::Rect(RectShape::filled(
                Rect::from_min_size(Pos2::new(x, 0.0), vec2(10.0, 10.0)),
                CornerRadius::ZERO,
                Color32::RED,
            )),
        )
    }

    fn mesh(mesh: &Arc<Mesh>) -> ClippedShape {
        ClippedShape::new(Rect::EVERYTHING, Shape::Mesh(Arc::clone(mesh)))
    }

    #[test]
    fn a_mesh_is_bounded_where_epaint_bounds_it() {
        let at = |pos: [(f32, f32); 4]| {
            let mut m = Mesh::default();
            for (x, y) in pos {
                m.colored_vertex(Pos2::new(x, y), Color32::RED);
            }
            Shape::mesh(m)
        };
        let shapes = [
            Shape::mesh(Mesh::default()),
            at([(3.5, -2.0), (1.0, 7.25), (-4.0, 0.0), (2.0, 2.0)]),
            at([(f32::NAN, 1.0), (2.0, f32::NAN), (5.0, 6.0), (-1.0, 3.0)]),
            at([(f32::NAN, f32::NAN); 4]),
            at([(-0.0, 0.0), (0.0, -0.0), (1e30, -1e30), (0.5, 0.5)]),
            rect(4.0).shape,
        ];
        for shape in &shapes {
            assert_eq!(bounds(shape), shape.visual_bounding_rect(), "{shape:?}");
        }
    }

    /// Counts the frames drawn for a run of passes, as the shell decides.
    fn drawn(passes: &[Vec<ClippedShape>]) -> usize {
        let mut last = LastPicture::default();
        let mut frames = 0;
        for shapes in passes {
            if last.change(shapes, 1.0, Color32::WHITE) != Change::None {
                frames += 1;
                last.keep(shapes.to_vec(), 1.0, Color32::WHITE);
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

    #[test]
    fn a_shape_of_another_kind_never_moved() {
        let mut m = Mesh::default();
        m.add_colored_rect(
            Rect::from_min_size(Pos2::ZERO, vec2(10.0, 10.0)),
            Color32::RED,
        );
        let m = Shape::Mesh(Arc::new(m));
        let r = rect(5.0).shape;
        let by = vec2(5.0, 0.0);
        assert!(!moved_by(&m, &r, by));
        assert!(!moved_by(&r, &m, by));
        assert!(!moved_by(&r, &Shape::Noop, by));
        assert!(moved_by(&rect(0.0).shape, &r, by));
        let mut moved = m.clone();
        moved.translate(by);
        assert!(moved_by(&m, &moved, by));
    }

    fn shader(paint: &Arc<ShaderPaint>) -> ClippedShape {
        ClippedShape::new(
            Rect::EVERYTHING,
            Shape::Callback(egui::PaintCallback {
                rect: Rect::from_min_size(Pos2::ZERO, vec2(10.0, 10.0)),
                callback: Arc::clone(paint) as _,
            }),
        )
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

    #[cfg(feature = "quick3d")]
    #[test]
    fn a_3d_view_whose_scene_did_not_change_is_the_same_picture() {
        use ljos_render::scene3d::Scene;
        let view = |scene: &Arc<Scene>, size_px| {
            ClippedShape::new(
                Rect::EVERYTHING,
                Shape::Callback(egui::PaintCallback {
                    rect: Rect::from_min_size(Pos2::ZERO, vec2(10.0, 10.0)),
                    // B0 hands out a new paint each frame, the scene in it
                    // the same while it is unchanged.
                    callback: Arc::new(View3dPaint {
                        key: 1,
                        scene: Arc::clone(scene),
                        size_px,
                    }),
                }),
            )
        };
        let (a, b) = (Arc::new(Scene::new()), Arc::new(Scene::new()));
        let passes = vec![
            vec![view(&a, [10, 10])],
            vec![view(&a, [10, 10])],
            vec![view(&b, [10, 10])],
            vec![view(&b, [20, 20])],
        ];
        assert_eq!(
            drawn(&passes),
            3,
            "a new scene or size is drawn, the same one is not"
        );
    }

    /// Where `rect(x)` draws, its smoothed edge with it.
    fn at(x: f32) -> Rect {
        Rect::from_min_size(Pos2::new(x - 1.0, -1.0), vec2(12.0, 12.0))
    }

    fn change(before: &[ClippedShape], now: &[ClippedShape]) -> Change {
        let mut last = LastPicture::default();
        last.keep(before.to_vec(), 1.0, Color32::WHITE);
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
    fn a_painter_that_draws_whole_is_told_only_whether_something_changed() {
        let before = [rect(0.0), rect(20.0)];
        let mut last = LastPicture::default();
        assert_eq!(last.whole_change(&before, 1.0, Color32::WHITE), Change::All);
        last.keep(before.to_vec(), 1.0, Color32::WHITE);
        assert_eq!(
            last.whole_change(&before, 1.0, Color32::WHITE),
            Change::None
        );
        for now in [&[rect(0.0), rect(25.0)][..], &[rect(0.0)], &before[..]] {
            let whole = if now == &before[..] {
                Change::None
            } else {
                Change::All
            };
            assert_eq!(last.whole_change(now, 1.0, Color32::WHITE), whole);
        }
        assert_eq!(last.whole_change(&before, 2.0, Color32::WHITE), Change::All);
        assert_eq!(last.whole_change(&before, 1.0, Color32::BLACK), Change::All);
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
        last.keep(vec![rect(0.0)], 1.0, Color32::WHITE);
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
        let wide = ClippedShape::new(
            Rect::EVERYTHING,
            Shape::Rect(RectShape::filled(
                Rect::from_min_size(Pos2::ZERO, vec2(100.0, 10.0)),
                CornerRadius::ZERO,
                Color32::BLUE,
            )),
        );
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
        let bar = |row: f32| {
            ClippedShape::new(
                Rect::EVERYTHING,
                Shape::Rect(RectShape::filled(
                    Rect::from_min_size(Pos2::new(row * 3.0 % 700.0, row * 6.0), vec2(12.0, 3.0)),
                    CornerRadius::ZERO,
                    Color32::RED,
                )),
            )
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
        ClippedShape::new(
            LIST,
            Shape::Rect(RectShape::filled(
                Rect::from_min_size(Pos2::new(0.0, y), vec2(100.0, 10.0)),
                CornerRadius::ZERO,
                Color32::from_rgb(i * 20, 0, 0),
            )),
        )
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

    /// A run of glyphs of a 512-texel atlas filled at `xs`, 10 texels a
    /// side, drawn as one mesh of quads side by side from `at`.
    fn run(atlas: TextureId, xs: &[usize], at: f32) -> ClippedShape {
        let mut m = Mesh::with_texture(atlas);
        for (i, &x) in xs.iter().enumerate() {
            let uv = texels([x, 0], [10, 10], [512, 512]).shrink(0.5 / 512.0);
            let on = Pos2::new(at + 10.0 * i as f32, 0.0);
            m.add_rect_with_uv(
                Rect::from_min_size(on, vec2(10.0, 10.0)),
                uv,
                Color32::BLACK,
            );
        }
        mesh(&Arc::new(m))
    }

    #[test]
    fn a_glyph_filled_beside_text_shown_draws_none_of_it_again() {
        let atlas = TextureId::Managed(7);
        let shown = [
            run(atlas, &[0, 100], 0.0),
            run(atlas, &[11], 20.0),
            rect(40.0),
        ];
        // The next glyph past the texel of padding after the second run's,
        // and so between the first run's two.
        let beside = [(atlas, Some(texels([22, 0], [10, 10], [512, 512])))];
        assert_eq!(resampled(&shown, &beside, 1.0), Vec::<Rect>::new());
        // Written over the second, it is drawn again, and only it.
        let over = [(atlas, Some(texels([12, 2], [4, 4], [512, 512])))];
        assert_eq!(resampled(&shown, &over, 1.0), vec![extent(&shown[1], 1.0)]);
        // A picture handed over whole is drawn again wherever it is shown;
        // another picture's is not.
        let whole = [(atlas, None), (TextureId::Managed(8), None)];
        assert_eq!(resampled(&shown, &whole, 1.0).len(), 2);
    }

    #[test]
    fn flat_colour_is_drawn_again_only_when_the_white_texel_is() {
        let shown = [rect(0.0)];
        let font = TextureId::default();
        let glyph = [(font, Some(texels([40, 0], [10, 10], [2048, 64])))];
        assert!(resampled(&shown, &glyph, 1.0).is_empty());
        let white = [(font, Some(texels([0, 0], [2, 2], [2048, 64])))];
        assert_eq!(resampled(&shown, &white, 1.0).len(), 1);
    }

    #[test]
    fn what_is_drawn_again_is_added_to_a_scroll_as_to_any_change() {
        let area = Rect::from_min_size(Pos2::new(5.0, 5.0), vec2(4.0, 4.0));
        assert_eq!(Change::None.with(vec![area]), Change::Area(vec![area]));
        assert_eq!(Change::None.with(Vec::new()), Change::None);
        let scroll = Scroll {
            rect: Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0)),
            by: vec2(-3.0, 0.0),
            areas: vec![],
        };
        assert_eq!(
            Change::Scroll(scroll.clone()).with(vec![area]),
            Change::Scroll(Scroll {
                areas: vec![area],
                ..scroll
            })
        );
        assert_eq!(Change::All.with(vec![area]), Change::All);
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
        let bar = ClippedShape::new(
            Rect::EVERYTHING,
            Shape::Rect(RectShape::filled(
                Rect::from_min_max(Pos2::new(95.0, 10.0), Pos2::new(100.0, 30.0)),
                CornerRadius::ZERO,
                Color32::BLUE,
            )),
        );
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
        let back = ClippedShape::new(
            Rect::EVERYTHING,
            Shape::Rect(RectShape::filled(
                Rect::from_min_max(Pos2::ZERO, Pos2::new(200.0, 200.0)),
                CornerRadius::ZERO,
                Color32::WHITE,
            )),
        );
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
        let dot = |x: f32| {
            ClippedShape::new(
                LIST,
                Shape::Rect(RectShape::filled(
                    Rect::from_min_size(Pos2::new(x, 50.0), vec2(4.0, 4.0)),
                    CornerRadius::ZERO,
                    Color32::RED,
                )),
            )
        };
        assert!(matches!(
            change(&[dot(10.0)], &[dot(12.0)]),
            Change::Area(_)
        ));
    }

    #[test]
    fn a_forgotten_picture_is_drawn_again() {
        let mut last = LastPicture::default();
        last.keep(Vec::new(), 1.0, Color32::WHITE);
        assert_eq!(last.change(&[], 1.0, Color32::WHITE), Change::None);
        last.forget();
        assert_eq!(last.change(&[], 1.0, Color32::WHITE), Change::All);
    }
}
