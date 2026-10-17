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
use egui::epaint::{ClippedShape, Shape};

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
            Change::None
        } else {
            Change::Area(areas)
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
    All,
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
        // What a callback paints is its own; it may differ with the same
        // callback, so a window with one is always drawn.
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

    #[test]
    fn a_forgotten_picture_is_drawn_again() {
        let mut last = LastPicture::default();
        last.keep(&[], 1.0, Color32::WHITE);
        assert_eq!(last.change(&[], 1.0, Color32::WHITE), Change::None);
        last.forget();
        assert_eq!(last.change(&[], 1.0, Color32::WHITE), Change::All);
    }
}
