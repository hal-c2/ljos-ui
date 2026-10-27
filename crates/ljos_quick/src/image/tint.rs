//! One colour laid over a picture: every pixel takes the colour and keeps
//! its own alpha, which is how an icon is drawn in the colour of the text
//! beside it.
//!
//! `Image` has no such property in QML. A type derived from it that has
//! one (the icon of a control) registers how to read it; the tinted
//! pixels are made once for each picture and colour, and are dropped with
//! the last display list that shows them, unless they are among the few
//! made last.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

use ljos_engine::{Color, Engine, MetaRef, ObjectId};
use ljos_render::{ImageHandle, WeakImage};
use rustc_hash::FxHashMap;

use crate::by_type::ByType;

/// The colour an item of a registered type lays over its picture just
/// now; none shows the picture as it is. Read inside the scene's paint
/// effect, so what it reads is followed.
pub type ImageTint = fn(&Engine, ObjectId) -> Option<Color>;

#[derive(Default)]
struct Tints {
    types: ByType<ImageTint>,
    /// By the picture's identity and the colour's bytes.
    made: RefCell<FxHashMap<(u64, [u8; 4]), WeakImage>>,
    /// How many `made` kept at its last sweep.
    swept: Cell<usize>,
    /// The last few made, held: a colour that goes back and forth (a
    /// button that is pressed and let go, dimmed and lit again) finds its
    /// picture again after the display list that showed it is gone.
    recent: RefCell<VecDeque<ImageHandle>>,
}

/// How many [`Tints::recent`] holds.
const RECENT: usize = 4;

/// Makes items of type `meta` (and of the types derived from it) paint
/// their picture in the colour `tint` gives. A later registration wins.
pub fn register_image_tint(engine: &Engine, meta: MetaRef, tint: ImageTint) {
    engine.extension::<Tints>().types.push(meta, tint);
}

/// `image` as `obj` paints it: tinted when its type registered a tint
/// that gives a colour.
pub(crate) fn tinted(engine: &Engine, obj: ObjectId, image: ImageHandle) -> ImageHandle {
    let tint = engine.extension::<Tints>().types.of(engine, obj);
    match tint.and_then(|t| t(engine, obj)) {
        Some(colour) => tinted_in(engine, image, colour),
        None => image,
    }
}

/// `image` laid over in `colour`, made once for each picture and colour
/// while a display list shows it or it is among the last few made.
fn tinted_in(engine: &Engine, image: ImageHandle, colour: Color) -> ImageHandle {
    let tints = engine.extension::<Tints>();
    let rgba = colour.to_rgba_f().map(|c| (c * 255.0).round() as u8);
    let key = (image.id(), rgba);
    let mut made = tints.made.borrow_mut();
    if let Some(done) = made.get(&key).and_then(WeakImage::upgrade) {
        return done;
    }
    // The pictures that are gone would otherwise stay in the table for
    // as long as the engine lives. Swept when it has doubled, so that
    // making many does not walk it each time.
    if made.len() >= 2 * tints.swept.get().max(16) {
        made.retain(|_, w| w.is_alive());
        tints.swept.set(made.len());
    }
    let out = lay_over(&image, rgba);
    made.insert(key, out.downgrade());
    let mut recent = tints.recent.borrow_mut();
    if recent.len() == RECENT {
        recent.pop_front();
    }
    recent.push_back(out.clone());
    out
}

/// The picture with the colour of every pixel replaced; the pixel's alpha
/// scales the colour's.
fn lay_over(image: &ImageHandle, [r, g, b, a]: [u8; 4]) -> ImageHandle {
    let scale = |channel: u8, alpha: u32| ((u32::from(channel) * alpha + 127) / 255) as u8;
    let mut data = Vec::with_capacity(image.data().len());
    for px in image.data().as_chunks::<4>().0 {
        // Premultiplied: each colour byte is the colour times the alpha.
        let alpha = u32::from(scale(a, u32::from(px[3])));
        data.extend_from_slice(&[
            scale(r, alpha),
            scale(g, alpha),
            scale(b, alpha),
            alpha as u8,
        ]);
    }
    ImageHandle::from_premultiplied(image.width(), image.height(), data)
        .unwrap_or_else(|_| image.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tint_replaces_the_colour_and_keeps_the_alpha() {
        let half = ImageHandle::from_straight(1, 1, vec![255, 0, 0, 128]).expect("one pixel");
        let green = lay_over(&half, [0, 255, 0, 255]);
        assert_eq!(green.pixel(0, 0), [0, 128, 0, 128]);
        let faint = lay_over(&half, [0, 255, 0, 128]);
        assert_eq!(faint.pixel(0, 0), [0, 64, 0, 64]);
    }

    /// As with every other per-type registry, a later registration wins.
    #[test]
    fn the_last_tint_registered_wins() {
        use ljos_engine::QmlType;

        let e = Engine::new();
        crate::register(&e);
        let meta = || MetaRef::Static(crate::Image::static_meta());
        register_image_tint(&e, meta(), |_, _| Some(Color::from_rgba8(255, 0, 0, 255)));
        register_image_tint(&e, meta(), |_, _| Some(Color::from_rgba8(0, 0, 255, 255)));
        let image = e.create_native(crate::Image::default(), None).unwrap();
        let white = ImageHandle::from_straight(1, 1, vec![255, 255, 255, 255]).expect("one pixel");
        assert_eq!(tinted(&e, image, white).pixel(0, 0), [0, 0, 255, 255]);
    }
}
