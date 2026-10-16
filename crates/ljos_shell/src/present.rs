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
//! logo animating over a still background costs the logo, not the window.

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
    /// pixels, so only where the others lie (before and now) can differ.
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
        let mut area = Rect::NOTHING;
        if before.len() == now.len() {
            for (a, b) in before.iter().zip(now) {
                if !same_clipped(a, b) {
                    area |= extent(a);
                    area |= extent(b);
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
            for shape in changed {
                area |= extent(shape);
            }
        }
        if area.is_positive() {
            Change::Area(area)
        } else {
            Change::None
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
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Change {
    None,
    Area(Rect),
    All,
}

/// What a painter draws of a picture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Frame {
    /// Nothing: the window shows it already (the pictures handed over are
    /// still taken).
    Nothing,
    /// The area (in points, on whole pixels); what lies outside it in the
    /// window's buffer is the picture already.
    Part(Rect),
    Whole,
}

/// Where `shape` can draw.
fn extent(shape: &ClippedShape) -> Rect {
    shape
        .shape
        .visual_bounding_rect()
        .intersect(shape.clip_rect)
}

/// Draws `shapes` only inside `area`: every clip rectangle is cut to it,
/// and what then has none is left out.
pub(crate) fn clip_to(shapes: &mut Vec<ClippedShape>, area: Rect) {
    shapes.retain_mut(|s| {
        s.clip_rect = s.clip_rect.intersect(area);
        s.clip_rect.is_positive() && extent(s).is_positive()
    });
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

    fn at(x: f32) -> Rect {
        Rect::from_min_size(Pos2::new(x, 0.0), vec2(10.0, 10.0))
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
        assert_eq!(change(&before, &now), Change::Area(at(20.0) | at(25.0)));
    }

    #[test]
    fn a_shape_that_comes_or_goes_changes_only_where_it_lies() {
        let before = [rect(0.0), rect(40.0)];
        let now = [rect(0.0), rect(20.0), rect(40.0)];
        assert_eq!(change(&before, &now), Change::Area(at(20.0)));
        assert_eq!(change(&now, &before), Change::Area(at(20.0)));
    }

    #[test]
    fn a_shape_changes_only_what_its_clip_lets_through() {
        let before = [rect(0.0)];
        let mut now = [rect(5.0)];
        now[0].clip_rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(12.0, 12.0));
        let changed = Rect::from_min_max(Pos2::ZERO, Pos2::new(12.0, 10.0));
        assert_eq!(change(&before, &now), Change::Area(changed));
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
    fn shapes_drawn_into_an_area_keep_to_it() {
        let mut shapes = vec![rect(0.0), rect(20.0), rect(40.0)];
        clip_to(
            &mut shapes,
            Rect::from_min_max(Pos2::new(15.0, 0.0), Pos2::new(35.0, 10.0)),
        );
        assert_eq!(shapes.len(), 1, "the shapes outside it are not drawn");
        assert_eq!(
            shapes[0].clip_rect,
            Rect::from_min_max(Pos2::new(15.0, 0.0), Pos2::new(35.0, 10.0))
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
