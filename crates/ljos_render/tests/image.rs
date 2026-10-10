//! Images on R: one pixel test per fill mode, the nine-patch, flips, opacity
//! and sampling. The expected pixels for the fill modes and the nine-patch
//! are what Qt 6.11 paints for the same picture and item size.

use ljos_render::kurbo::{Rect, Size};
use ljos_render::{
    DisplayListBuilder, DrawOp, FrameView, ImageAlign, ImageFit, ImageHandle, Insets, NodeId,
    RasterConfig, RenderTree, Rgba, RgbaImage, TextMode, TileMode, VelloCpuReference, place_image,
};

const WHITE_BG: RasterConfig = RasterConfig {
    background: Rgba::WHITE,
    text: TextMode::Skip,
};

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const YELLOW: [u8; 4] = [255, 255, 0, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];

const CENTRE: [ImageAlign; 2] = [ImageAlign::Center, ImageAlign::Center];

/// 40 x 20: red and green on top, blue and yellow below.
fn quad() -> ImageHandle {
    let mut data = Vec::with_capacity(40 * 20 * 4);
    for y in 0..20 {
        for x in 0..40 {
            data.extend_from_slice(&match (x < 20, y < 10) {
                (true, true) => RED,
                (false, true) => GREEN,
                (true, false) => BLUE,
                (false, false) => YELLOW,
            });
        }
    }
    ImageHandle::from_straight(40, 20, data).unwrap()
}

/// 30 x 30 where every pixel carries its own position: red = 8 x, green = 8 y.
fn grid() -> ImageHandle {
    let mut data = Vec::with_capacity(30 * 30 * 4);
    for y in 0..30u8 {
        for x in 0..30u8 {
            data.extend_from_slice(&[x * 8, y * 8, 100, 255]);
        }
    }
    ImageHandle::from_straight(30, 30, data).unwrap()
}

struct Scene {
    tree: RenderTree,
    vp: Rect,
    r: VelloCpuReference,
}

impl Scene {
    fn new(w: f64, h: f64) -> Self {
        let mut tree = RenderTree::new();
        let root = tree.root();
        tree.set_size(root, Size::new(w, h));
        Self {
            tree,
            vp: Rect::new(0.0, 0.0, w, h),
            r: VelloCpuReference::new(WHITE_BG),
        }
    }

    fn node(&mut self, x: f64, y: f64, w: f64, h: f64) -> NodeId {
        let n = self.tree.add_child(self.tree.root());
        self.tree.set_pos(n, x, y);
        self.tree.set_size(n, Size::new(w, h));
        n
    }

    fn op(&mut self, id: NodeId, op: DrawOp) {
        let mut b = DisplayListBuilder::new();
        b.push(op);
        self.tree.set_content(id, Some(b.finish_arc()));
    }

    fn render(&mut self, scale: f32) -> RgbaImage {
        let snap = self.tree.compose(self.vp, 1);
        self.r.render(&FrameView::new(&snap, scale)).unwrap()
    }
}

/// The op an item of size `area` paints for `image` under `fit`.
fn placed(
    image: &ImageHandle,
    area: (f64, f64),
    fit: ImageFit,
    align: [ImageAlign; 2],
    mirror: [bool; 2],
) -> DrawOp {
    let size = Size::new(f64::from(image.width()), f64::from(image.height()));
    let p = place_image(size, Size::new(area.0, area.1), fit, align, mirror).unwrap();
    DrawOp::Image {
        image: image.clone(),
        src: p.src,
        dst: p.dst,
        tile: p.tile,
        mirror,
        smooth: false,
    }
}

/// Render `image` in an item of size `area` at (10, 10) of a white window
/// with a 10 px margin all round.
fn shot(area: (f64, f64), fit: ImageFit, align: [ImageAlign; 2], mirror: [bool; 2]) -> RgbaImage {
    let image = quad();
    let mut s = Scene::new(area.0 + 20.0, area.1 + 20.0);
    let n = s.node(10.0, 10.0, area.0, area.1);
    s.op(n, placed(&image, area, fit, align, mirror));
    s.render(1.0)
}

/// Pixel at an item coordinate of a [`shot`].
fn at(img: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
    img.pixel(x + 10, y + 10)
}

/// The red channel of row `y`, as source columns of [`grid`].
fn grid_row(img: &RgbaImage, y: u32, x0: u32, n: u32) -> Vec<u8> {
    (x0..x0 + n).map(|x| img.pixel(x, y)[0] / 8).collect()
}

#[test]
fn stretch_fills_the_item() {
    let img = shot((80.0, 60.0), ImageFit::Stretch, CENTRE, [false; 2]);
    assert_eq!(at(&img, 0, 0), RED);
    assert_eq!(at(&img, 39, 29), RED);
    assert_eq!(at(&img, 40, 29), GREEN);
    assert_eq!(at(&img, 39, 30), BLUE);
    assert_eq!(at(&img, 79, 59), YELLOW);
    // Nothing outside the item.
    assert_eq!(img.pixel(9, 10), WHITE);
    assert_eq!(img.pixel(90, 69), WHITE);
    assert_eq!(img.pixel(89, 70), WHITE);
}

#[test]
fn fit_keeps_the_shape_and_centres() {
    // 40 x 20 in 100 x 30 is painted 60 x 30, 20 px in from the left.
    let img = shot((100.0, 30.0), ImageFit::Fit, CENTRE, [false; 2]);
    assert_eq!(at(&img, 19, 15), WHITE);
    assert_eq!(at(&img, 20, 0), RED);
    assert_eq!(at(&img, 49, 14), RED);
    assert_eq!(at(&img, 50, 15), YELLOW);
    assert_eq!(at(&img, 79, 29), YELLOW);
    assert_eq!(at(&img, 80, 15), WHITE);

    let left = shot(
        (100.0, 30.0),
        ImageFit::Fit,
        [ImageAlign::Start, ImageAlign::Center],
        [false; 2],
    );
    assert_eq!(at(&left, 0, 0), RED);
    assert_eq!(at(&left, 59, 29), YELLOW);
    assert_eq!(at(&left, 60, 15), WHITE);
}

#[test]
fn crop_covers_the_item_and_is_cut_at_its_edges() {
    // 40 x 20 in 20 x 20 keeps its size; the middle 20 columns show.
    let img = shot((20.0, 20.0), ImageFit::Crop, CENTRE, [false; 2]);
    assert_eq!(at(&img, 0, 0), RED);
    assert_eq!(at(&img, 9, 9), RED);
    assert_eq!(at(&img, 10, 9), GREEN);
    assert_eq!(at(&img, 9, 10), BLUE);
    assert_eq!(at(&img, 19, 19), YELLOW);
    assert_eq!(img.pixel(9, 15), WHITE);
    assert_eq!(img.pixel(30, 15), WHITE);

    let right = shot(
        (20.0, 20.0),
        ImageFit::Crop,
        [ImageAlign::End, ImageAlign::Center],
        [false; 2],
    );
    assert_eq!(at(&right, 0, 0), GREEN);
    assert_eq!(at(&right, 19, 19), YELLOW);
}

#[test]
fn tile_repeats_from_the_aligned_copy() {
    // One copy sits centred at (30, 15); the others continue from it.
    let img = shot((100.0, 50.0), ImageFit::Tile, CENTRE, [false; 2]);
    assert_eq!(at(&img, 30, 15), RED);
    assert_eq!(at(&img, 69, 34), YELLOW);
    // Left neighbour's right half, upper neighbour's lower half.
    assert_eq!(at(&img, 29, 15), GREEN);
    assert_eq!(at(&img, 30, 14), BLUE);
    assert_eq!(at(&img, 29, 14), YELLOW);
    // The copies are cut at the item: (0, 0) is 10 px across and 5 px down
    // a copy, (99, 49) is 29 px across and 14 px down another.
    assert_eq!(at(&img, 0, 0), RED);
    assert_eq!(at(&img, 99, 49), YELLOW);
    assert_eq!(img.pixel(9, 30), WHITE);
    assert_eq!(img.pixel(110, 30), WHITE);

    let top_left = shot(
        (100.0, 50.0),
        ImageFit::Tile,
        [ImageAlign::Start, ImageAlign::Start],
        [false; 2],
    );
    assert_eq!(at(&top_left, 0, 0), RED);
    assert_eq!(at(&top_left, 40, 20), RED);
    assert_eq!(at(&top_left, 39, 19), YELLOW);
}

#[test]
fn tile_vertically_stretches_sideways() {
    let img = shot((80.0, 50.0), ImageFit::TileVertically, CENTRE, [false; 2]);
    // The copy at y 15..35 is 80 wide.
    assert_eq!(at(&img, 0, 15), RED);
    assert_eq!(at(&img, 39, 24), RED);
    assert_eq!(at(&img, 40, 24), GREEN);
    assert_eq!(at(&img, 79, 34), YELLOW);
    assert_eq!(at(&img, 0, 14), BLUE);
    assert_eq!(at(&img, 79, 35), GREEN);
}

#[test]
fn tile_horizontally_stretches_downwards() {
    let img = shot(
        (100.0, 40.0),
        ImageFit::TileHorizontally,
        CENTRE,
        [false; 2],
    );
    // The copy at x 30..70 is 40 high.
    assert_eq!(at(&img, 30, 0), RED);
    assert_eq!(at(&img, 49, 19), RED);
    assert_eq!(at(&img, 49, 20), BLUE);
    assert_eq!(at(&img, 69, 39), YELLOW);
    assert_eq!(at(&img, 29, 0), GREEN);
    assert_eq!(at(&img, 70, 39), BLUE);
}

#[test]
fn pad_keeps_the_size_and_is_cut_at_the_item() {
    let img = shot((100.0, 50.0), ImageFit::Pad, CENTRE, [false; 2]);
    assert_eq!(at(&img, 29, 25), WHITE);
    assert_eq!(at(&img, 30, 15), RED);
    assert_eq!(at(&img, 69, 34), YELLOW);
    assert_eq!(at(&img, 70, 25), WHITE);
    assert_eq!(at(&img, 50, 14), WHITE);
    assert_eq!(at(&img, 50, 35), WHITE);

    // Smaller than the picture: the middle shows and nothing spills.
    let small = shot((20.0, 10.0), ImageFit::Pad, CENTRE, [false; 2]);
    assert_eq!(at(&small, 0, 0), RED);
    assert_eq!(at(&small, 10, 4), GREEN);
    assert_eq!(at(&small, 9, 5), BLUE);
    assert_eq!(at(&small, 19, 9), YELLOW);
    assert_eq!(small.pixel(9, 15), WHITE);
    assert_eq!(small.pixel(30, 15), WHITE);
    assert_eq!(small.pixel(20, 9), WHITE);
    assert_eq!(small.pixel(20, 20), WHITE);

    let corner = shot(
        (100.0, 50.0),
        ImageFit::Pad,
        [ImageAlign::End, ImageAlign::End],
        [false; 2],
    );
    assert_eq!(at(&corner, 60, 30), RED);
    assert_eq!(at(&corner, 99, 49), YELLOW);
    assert_eq!(at(&corner, 59, 40), WHITE);
}

#[test]
fn mirror_flips_the_content_in_place() {
    let h = shot((80.0, 60.0), ImageFit::Stretch, CENTRE, [true, false]);
    assert_eq!(at(&h, 0, 0), GREEN);
    assert_eq!(at(&h, 79, 0), RED);
    assert_eq!(at(&h, 0, 59), YELLOW);

    let v = shot((80.0, 60.0), ImageFit::Stretch, CENTRE, [false, true]);
    assert_eq!(at(&v, 0, 0), BLUE);
    assert_eq!(at(&v, 79, 59), GREEN);

    // A flipped picture that is cut shows the same middle, flipped.
    let crop = shot((20.0, 20.0), ImageFit::Crop, CENTRE, [true, false]);
    assert_eq!(at(&crop, 0, 0), GREEN);
    assert_eq!(at(&crop, 19, 19), BLUE);

    // Left-aligned crop of a flipped picture shows the picture's right half.
    let left = shot(
        (20.0, 20.0),
        ImageFit::Crop,
        [ImageAlign::Start, ImageAlign::Center],
        [true, false],
    );
    assert_eq!(at(&left, 0, 0), GREEN);
    assert_eq!(at(&left, 19, 19), YELLOW);

    // Tiles stay where they are; each one is flipped.
    let tile = shot((100.0, 50.0), ImageFit::Tile, CENTRE, [true, true]);
    assert_eq!(at(&tile, 30, 15), YELLOW);
    assert_eq!(at(&tile, 69, 34), RED);
    assert_eq!(at(&tile, 29, 15), BLUE);
}

#[test]
fn a_source_rectangle_draws_that_part_of_the_picture() {
    // The lower right quarter only, scaled up four times.
    let draw = |smooth: bool| {
        let mut s = Scene::new(80.0, 40.0);
        let n = s.node(0.0, 0.0, 80.0, 40.0);
        s.op(
            n,
            DrawOp::Image {
                image: quad(),
                src: Rect::new(20.0, 10.0, 40.0, 20.0),
                dst: Rect::new(0.0, 0.0, 80.0, 40.0),
                tile: None,
                mirror: [false; 2],
                smooth,
            },
        );
        s.render(1.0)
    };
    let hard = draw(false);
    for (x, y) in [(0, 0), (79, 0), (0, 39), (79, 39), (40, 20)] {
        assert_eq!(hard.pixel(x, y), YELLOW, "({x}, {y})");
    }
    // Smooth sampling blends into the rest of the picture at the cut, as a
    // cropped picture should look: only the two rows and columns next to
    // the cut differ.
    let soft = draw(true);
    for (x, y) in [(2, 2), (79, 2), (2, 39), (79, 39), (40, 20)] {
        assert_eq!(soft.pixel(x, y), YELLOW, "({x}, {y})");
    }
    assert_ne!(soft.pixel(0, 0), YELLOW);
}

#[test]
fn nearest_keeps_hard_edges_and_smooth_blends_them() {
    // Two pixels, black and white, eight times wider.
    let image = ImageHandle::from_straight(2, 1, vec![0, 0, 0, 255, 255, 255, 255, 255]).unwrap();
    let draw = |smooth: bool| {
        let mut s = Scene::new(16.0, 4.0);
        let n = s.node(0.0, 0.0, 16.0, 4.0);
        s.op(
            n,
            DrawOp::Image {
                image: image.clone(),
                src: image.rect(),
                dst: Rect::new(0.0, 0.0, 16.0, 4.0),
                tile: None,
                mirror: [false; 2],
                smooth,
            },
        );
        s.render(1.0)
    };
    let hard = draw(false);
    let reds: Vec<u8> = (0..16).map(|x| hard.pixel(x, 2)[0]).collect();
    assert_eq!(&reds[..8], &[0; 8]);
    assert_eq!(&reds[8..], &[255; 8]);

    let soft = draw(true);
    let reds: Vec<u8> = (0..16).map(|x| soft.pixel(x, 2)[0]).collect();
    // The ends keep the edge colours (no background bleeds in) and the
    // middle is a ramp.
    assert_eq!(reds[0], 0);
    assert_eq!(reds[15], 255);
    assert!(reds.windows(2).all(|w| w[0] <= w[1]), "{reds:?}");
    assert!(reds[7] > 60 && reds[7] < 128, "{reds:?}");
    assert!(reds[8] > 127 && reds[8] < 195, "{reds:?}");
}

#[test]
fn opacity_and_transparent_pixels_blend_with_what_is_below() {
    // Left half opaque red, right half clear.
    let mut data = Vec::new();
    for _ in 0..4 {
        data.extend_from_slice(&[255, 0, 0, 255, 255, 0, 0, 255]);
        data.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]);
    }
    let image = ImageHandle::from_straight(4, 4, data).unwrap();
    assert!(!image.is_opaque());
    let mut s = Scene::new(8.0, 4.0);
    let a = s.node(0.0, 0.0, 4.0, 4.0);
    let b = s.node(4.0, 0.0, 4.0, 4.0);
    for n in [a, b] {
        s.op(
            n,
            DrawOp::Image {
                image: image.clone(),
                src: image.rect(),
                dst: Rect::new(0.0, 0.0, 4.0, 4.0),
                tile: None,
                mirror: [false; 2],
                smooth: false,
            },
        );
    }
    s.tree.set_opacity(b, 0.5);
    let img = s.render(1.0);
    assert_eq!(img.pixel(0, 0), RED);
    assert_eq!(img.pixel(2, 0), WHITE);
    // Half-opaque red over white.
    let half = img.pixel(4, 0);
    assert_eq!(half[0], 255);
    assert!((126..=129).contains(&half[1]), "{half:?}");
    assert_eq!(half[1], half[2]);
    assert_eq!(img.pixel(6, 0), WHITE);
}

#[test]
fn images_scale_with_the_device_pixel_ratio() {
    let image = quad();
    let mut s = Scene::new(40.0, 20.0);
    let n = s.node(0.0, 0.0, 40.0, 20.0);
    s.op(
        n,
        placed(&image, (40.0, 20.0), ImageFit::Stretch, CENTRE, [false; 2]),
    );
    let img = s.render(2.0);
    assert_eq!((img.width, img.height), (80, 40));
    assert_eq!(img.pixel(39, 19), RED);
    assert_eq!(img.pixel(40, 19), GREEN);
    assert_eq!(img.pixel(39, 20), BLUE);
    assert_eq!(img.pixel(79, 39), YELLOW);
}

fn nine(
    insets: (f32, f32, f32, f32),
    size: (f64, f64),
    tile: [TileMode; 2],
    mirror: bool,
) -> RgbaImage {
    let mut s = Scene::new(size.0, size.1);
    let n = s.node(0.0, 0.0, size.0, size.1);
    s.op(
        n,
        DrawOp::NinePatch {
            image: grid(),
            insets: Insets {
                left: insets.0,
                top: insets.1,
                right: insets.2,
                bottom: insets.3,
            },
            dst: Rect::new(0.0, 0.0, size.0, size.1),
            tile,
            mirror,
            smooth: false,
        },
    );
    s.render(1.0)
}

fn range(a: u8, b: u8) -> Vec<u8> {
    (a..b).collect()
}

#[test]
fn nine_patch_keeps_borders_and_stretches_the_middle() {
    let img = nine(
        (5.0, 6.0, 7.0, 8.0),
        (120.0, 80.0),
        [TileMode::Stretch; 2],
        false,
    );
    // Borders at their own size.
    assert_eq!(grid_row(&img, 0, 0, 5), range(0, 5));
    assert_eq!(grid_row(&img, 0, 113, 7), range(23, 30));
    // The 18 middle columns fill 108 px: six each.
    let mid = grid_row(&img, 40, 5, 108);
    for (i, v) in mid.iter().enumerate() {
        assert_eq!(usize::from(*v), 5 + i / 6, "column {i}");
    }
    // Rows: the top border, then 16 middle rows over 66 px, then the bottom.
    let col = |y: u32| img.pixel(60, y)[1] / 8;
    assert_eq!((0..6).map(col).collect::<Vec<_>>(), range(0, 6));
    assert_eq!((72..80).map(col).collect::<Vec<_>>(), range(22, 30));
    assert_eq!(col(6), 6);
    assert_eq!(col(71), 21);
}

#[test]
fn nine_patch_repeat_starts_at_the_leading_edge() {
    // Qt 6.11: 5 + 4 x 20 + 10 + 5 across, 20 + 13 x 5 + 5 down.
    let img = nine(
        (5.0, 20.0, 5.0, 5.0),
        (100.0, 90.0),
        [TileMode::Repeat; 2],
        false,
    );
    for y in [10, 40] {
        let row = grid_row(&img, y, 0, 100);
        assert_eq!(&row[..5], range(0, 5));
        for tile in 0..4 {
            assert_eq!(
                &row[5 + tile * 20..25 + tile * 20],
                range(5, 25),
                "tile {tile}"
            );
        }
        // The last tile is cut off.
        assert_eq!(&row[85..95], range(5, 15));
        assert_eq!(&row[95..], range(25, 30));
    }
    let col: Vec<u8> = (0..90).map(|y| img.pixel(2, y)[1] / 8).collect();
    assert_eq!(&col[..20], range(0, 20));
    for tile in 0..13 {
        assert_eq!(
            &col[20 + tile * 5..25 + tile * 5],
            range(20, 25),
            "tile {tile}"
        );
    }
    assert_eq!(&col[85..], range(25, 30));
}

#[test]
fn nine_patch_round_squeezes_whole_tiles() {
    // Qt 6.11: 72 px of room for 10 px tiles is eight tiles of 9 px.
    let img = nine(
        (10.0, 10.0, 10.0, 10.0),
        (92.0, 60.0),
        [TileMode::Round, TileMode::Stretch],
        false,
    );
    let row = grid_row(&img, 5, 0, 92);
    assert_eq!(&row[..10], range(0, 10));
    for tile in 0..8 {
        let t = &row[10 + tile * 9..19 + tile * 9];
        assert_eq!(t[0], 10, "tile {tile}: {t:?}");
        assert_eq!(t[8], 19, "tile {tile}: {t:?}");
        assert!(t.windows(2).all(|w| w[0] < w[1]), "tile {tile}: {t:?}");
    }
    assert_eq!(&row[82..], range(20, 30));
}

#[test]
fn nine_patch_mirror_flips_the_picture_not_the_borders() {
    // Qt 6.11: the left border stays 5 wide and shows the picture's right
    // end; the right border stays 10 wide.
    let img = nine(
        (5.0, 5.0, 10.0, 5.0),
        (60.0, 30.0),
        [TileMode::Stretch; 2],
        true,
    );
    let row = grid_row(&img, 15, 0, 60);
    assert_eq!(&row[..5], [29, 28, 27, 26, 25]);
    // 15 middle columns over 45 px: three each, right to left.
    for (i, v) in row[5..50].iter().enumerate() {
        assert_eq!(usize::from(*v), 24 - i / 3, "column {i}");
    }
    assert_eq!(&row[50..], [9, 8, 7, 6, 5, 4, 3, 2, 1, 0]);
}

#[test]
fn nine_patch_smaller_than_its_borders_keeps_them_unscaled() {
    // Qt 6.11: the left border in full, then the outer end of the right one.
    let img = nine(
        (10.0, 10.0, 10.0, 10.0),
        (15.0, 15.0),
        [TileMode::Stretch; 2],
        false,
    );
    let row = grid_row(&img, 2, 0, 15);
    assert_eq!(&row[..10], range(0, 10));
    assert_eq!(&row[10..], range(25, 30));
}

#[test]
fn the_backend_keeps_no_pixels_of_a_dropped_image() {
    let image = quad();
    let mut s = Scene::new(40.0, 20.0);
    let n = s.node(0.0, 0.0, 40.0, 20.0);
    s.op(
        n,
        placed(&image, (40.0, 20.0), ImageFit::Stretch, CENTRE, [false; 2]),
    );
    s.render(1.0);
    s.render(1.0);
    assert_eq!(s.r.cached_images(), 1);
    // R's copy does not count as a holder: only this test and the display
    // list keep the picture alive.
    assert_eq!(image.holders(), 2);

    s.tree.set_content(n, None);
    drop(image);
    s.render(1.0);
    assert_eq!(s.r.cached_images(), 0);
}
