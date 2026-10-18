//! Small pictures share textures.
//!
//! Each texture a frame samples is a draw of its own: a clock face with
//! three hands and a centre was five, and a driver checks its state anew
//! for every draw when the frame is handed over, which cost more than
//! everything else the frame did. Pictures up to [`MAX_SIDE`] a side are
//! put on shared pages instead, so the quads of a run of them are one mesh
//! on one texture.
//!
//! A page is cut into shelves, rows as high as the first picture put in
//! them. A place is not given out again: the page goes once no picture on
//! it is held any more, and pictures that come back go on another.

use std::sync::Arc;

use egui::{Color32, ColorImage, Context, Rect as ERect, TextureHandle, TextureId, TextureOptions};

/// The side of a page, in pixels: 1 MiB of texture.
pub(crate) const PAGE: u32 = 512;

/// Pictures with a side past this get textures of their own: a page holds
/// at least four.
pub(crate) const MAX_SIDE: u32 = PAGE / 2;

/// Where a picture lies on a page. Holding it keeps the page.
#[derive(Debug)]
pub(crate) struct Place {
    pub(crate) texture: TextureId,
    /// The picture's pixels, as fractions of the page.
    pub(crate) uv: ERect,
    _page: Arc<()>,
}

#[derive(Default)]
pub(crate) struct ImageAtlas {
    pages: Vec<Page>,
}

struct Page {
    handle: TextureHandle,
    smooth: bool,
    shelves: Vec<Shelf>,
    /// The top of what no shelf takes yet.
    free_y: u32,
    /// One more than the pictures on it that are held.
    held: Arc<()>,
}

struct Shelf {
    y: u32,
    height: u32,
    /// The left of what is free on it.
    x: u32,
}

impl ImageAtlas {
    /// Puts `pixels` on a page sampled as `options` say, or `None` where
    /// they do not go on one: too big, or a page would be bigger than the
    /// GPU takes.
    pub(crate) fn place(
        &mut self,
        ctx: &Context,
        pixels: &ColorImage,
        options: TextureOptions,
        max_side: usize,
    ) -> Option<Place> {
        let [w, h] = pixels.size.map(|s| s as u32);
        if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE || (PAGE as usize) > max_side {
            return None;
        }
        let smooth = options.magnification == egui::TextureFilter::Linear;
        // A pixel of each edge again around it: sampled across its edge, the
        // picture reads itself, as a texture of its own clamped there does.
        let (gw, gh) = (w + 2, h + 2);
        let (page, x, y) = match self
            .pages
            .iter_mut()
            .enumerate()
            .filter(|(_, p)| p.smooth == smooth)
            .find_map(|(i, p)| p.room(gw, gh).map(|(x, y)| (i, x, y)))
        {
            Some(found) => found,
            None => {
                let mut page = Page::new(ctx, smooth, options);
                let (x, y) = page.room(gw, gh).expect("a picture fits an empty page");
                self.pages.push(page);
                (self.pages.len() - 1, x, y)
            }
        };
        let page = &mut self.pages[page];
        page.handle
            .set_partial([x as usize, y as usize], with_edges(pixels), options);
        let side = PAGE as f32;
        Some(Place {
            texture: page.handle.id(),
            uv: ERect::from_min_max(
                egui::pos2((x + 1) as f32 / side, (y + 1) as f32 / side),
                egui::pos2((x + 1 + w) as f32 / side, (y + 1 + h) as f32 / side),
            ),
            _page: Arc::clone(&page.held),
        })
    }

    /// Lets the pages go that hold no picture any more.
    pub(crate) fn sweep(&mut self) {
        self.pages.retain(|p| Arc::strong_count(&p.held) > 1);
    }

    /// How many pages there are.
    pub(crate) fn pages(&self) -> usize {
        self.pages.len()
    }
}

impl Page {
    fn new(ctx: &Context, smooth: bool, options: TextureOptions) -> Page {
        let side = PAGE as usize;
        let blank = ColorImage::filled([side, side], Color32::TRANSPARENT);
        Page {
            handle: crate::b0::upload(ctx, "ljos-image-atlas", blank, options),
            smooth,
            shelves: Vec::new(),
            free_y: 0,
            held: Arc::new(()),
        }
    }

    /// Where a `w` by `h` picture goes on this page, taken for it.
    fn room(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        // The lowest shelf it fits on, so tall shelves are not filled with
        // flat pictures.
        let shelf = self
            .shelves
            .iter_mut()
            .filter(|s| s.height >= h && PAGE - s.x >= w)
            .min_by_key(|s| s.height);
        if let Some(s) = shelf {
            let at = (s.x, s.y);
            s.x += w;
            return Some(at);
        }
        if PAGE - self.free_y < h {
            return None;
        }
        let y = self.free_y;
        self.free_y += h;
        self.shelves.push(Shelf { y, height: h, x: w });
        Some((0, y))
    }
}

/// `pixels` with each edge pixel once more outside it.
fn with_edges(pixels: &ColorImage) -> ColorImage {
    let [w, h] = pixels.size;
    let (gw, gh) = (w + 2, h + 2);
    let mut out = Vec::with_capacity(gw * gh);
    for gy in 0..gh {
        let row = &pixels.pixels[gy.saturating_sub(1).min(h - 1) * w..][..w];
        out.push(row[0]);
        out.extend_from_slice(row);
        out.push(row[w - 1]);
    }
    ColorImage::new([gw, gh], out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(w: usize, h: usize) -> ColorImage {
        let pixels = (0..w * h)
            .map(|i| Color32::from_rgb(i as u8, (i / w) as u8, 7))
            .collect();
        ColorImage::new([w, h], pixels)
    }

    fn options() -> TextureOptions {
        TextureOptions::LINEAR
    }

    #[test]
    fn the_edges_are_repeated_around_a_picture() {
        let p = picture(3, 2);
        let e = with_edges(&p);
        assert_eq!(e.size, [5, 4]);
        let at = |x: usize, y: usize| e.pixels[y * 5 + x];
        let src = |x: usize, y: usize| p.pixels[y * 3 + x];
        assert_eq!(at(0, 0), src(0, 0));
        assert_eq!(at(4, 3), src(2, 1));
        assert_eq!(at(2, 0), src(1, 0));
        assert_eq!(at(0, 2), src(0, 1));
        assert_eq!(at(2, 2), src(1, 1));
    }

    #[test]
    fn small_pictures_share_a_page_and_big_ones_do_not_go_on_one() {
        let ctx = Context::default();
        let mut atlas = ImageAtlas::default();
        let a = atlas
            .place(&ctx, &picture(16, 16), options(), 4096)
            .unwrap();
        let b = atlas
            .place(&ctx, &picture(200, 240), options(), 4096)
            .unwrap();
        assert_eq!(a.texture, b.texture);
        assert_eq!(atlas.pages(), 1);
        assert!(!a.uv.intersects(b.uv), "{:?} {:?}", a.uv, b.uv);
        assert!(
            atlas
                .place(&ctx, &picture(300, 10), options(), 4096)
                .is_none()
        );
        // A GPU that takes no page.
        assert!(atlas.place(&ctx, &picture(4, 4), options(), 256).is_none());
    }

    #[test]
    fn a_place_is_the_picture_s_pixels_inside_its_edges() {
        let ctx = Context::default();
        let mut atlas = ImageAtlas::default();
        let a = atlas
            .place(&ctx, &picture(10, 20), options(), 4096)
            .unwrap();
        let side = PAGE as f32;
        assert_eq!(a.uv.min, egui::pos2(1.0 / side, 1.0 / side));
        assert_eq!(a.uv.max, egui::pos2(11.0 / side, 21.0 / side));
    }

    #[test]
    fn pictures_sampled_apart_are_on_pages_apart() {
        let ctx = Context::default();
        let mut atlas = ImageAtlas::default();
        let a = atlas.place(&ctx, &picture(8, 8), options(), 4096).unwrap();
        let b = atlas
            .place(&ctx, &picture(8, 8), TextureOptions::NEAREST, 4096)
            .unwrap();
        assert_ne!(a.texture, b.texture);
    }

    #[test]
    fn a_full_page_is_followed_by_another_and_an_unheld_one_goes() {
        let ctx = Context::default();
        let mut atlas = ImageAtlas::default();
        let held: Vec<Place> = (0..5)
            .map(|_| {
                atlas
                    .place(&ctx, &picture(250, 250), options(), 4096)
                    .unwrap()
            })
            .collect();
        assert_eq!(atlas.pages(), 2);
        assert_ne!(held[0].texture, held[4].texture);
        let last = held[4].texture;
        drop(held);
        atlas.sweep();
        assert_eq!(atlas.pages(), 0);
        let again = atlas.place(&ctx, &picture(8, 8), options(), 4096).unwrap();
        assert_ne!(again.texture, last, "a page that went is not used again");
    }
}
