//! Whether a pass changed the picture in its window.
//!
//! A pass runs for every pointer event, timer and message, and most of
//! them change nothing that is seen: the pointer moves over items that do
//! not react to it, a click lands where nothing takes it, an animation
//! moves something out of sight. Rasterising, submitting and presenting
//! the same picture again is most of what a frame costs (all of it on a
//! software rasteriser), so a window keeps what it last showed and is
//! only drawn when that changes.

use std::sync::Arc;

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
    /// Whether drawing `shapes` would show what the window shows already.
    /// `textures` is whether there are pictures for the painter to take,
    /// which only a drawn frame hands over.
    pub(crate) fn unchanged(
        &self,
        shapes: &[ClippedShape],
        pixels_per_point: f32,
        clear: egui::Color32,
        textures: bool,
    ) -> bool {
        let Some(shown) = &self.shown else {
            return false;
        };
        !textures
            && shown.pixels_per_point == pixels_per_point
            && shown.clear == clear
            && shown.shapes.len() == shapes.len()
            && shown
                .shapes
                .iter()
                .zip(shapes)
                .all(|(a, b)| same_clipped(a, b))
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
    use egui::{Color32, Pos2, Rect, vec2};

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
            if !last.unchanged(shapes, 1.0, Color32::WHITE, false) {
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

    #[test]
    fn new_pictures_for_the_painter_are_drawn() {
        let mut last = LastPicture::default();
        last.keep(&[rect(0.0)], 1.0, Color32::WHITE);
        assert!(last.unchanged(&[rect(0.0)], 1.0, Color32::WHITE, false));
        assert!(!last.unchanged(&[rect(0.0)], 1.0, Color32::WHITE, true));
    }

    #[test]
    fn another_scale_or_background_is_a_change() {
        let mut last = LastPicture::default();
        last.keep(&[rect(0.0)], 1.0, Color32::WHITE);
        assert!(!last.unchanged(&[rect(0.0)], 2.0, Color32::WHITE, false));
        assert!(!last.unchanged(&[rect(0.0)], 1.0, Color32::BLACK, false));
    }

    #[test]
    fn a_forgotten_picture_is_drawn_again() {
        let mut last = LastPicture::default();
        assert!(
            !last.unchanged(&[], 1.0, Color32::WHITE, false),
            "nothing shown yet"
        );
        last.keep(&[], 1.0, Color32::WHITE);
        assert!(last.unchanged(&[], 1.0, Color32::WHITE, false));
        last.forget();
        assert!(!last.unchanged(&[], 1.0, Color32::WHITE, false));
    }
}
