//! `MultiEffect` (QtQuick.Effects): draws its source item with
//! post-processing applied.
//!
//! What is painted today is the mask, and the source unchanged when no
//! effect is enabled — both only when the source (and the mask) are
//! `Image` items, whose pixels this item composes on the Engine thread
//! at paint time (a cached picture per content change). There is no
//! offscreen rendering of item subtrees in Ljós yet, so a source that is
//! not an `Image` paints nothing; blur, shadow and the colour
//! adjustments (brightness, contrast, saturation, colorization) are
//! declared properties that store and notify but are not painted. The
//! source and the mask are not drawn in their own places while an
//! effect uses them (the scene hides them, as Qt's effect renders them
//! only as textures); a hidden source still paints through its effect,
//! which is also what Qt does when the source can be used as a texture
//! directly.
//!
//! The mask model follows the documentation: the mask's alpha is kept
//! between `maskThresholdMin` and `maskThresholdMax` (outside that
//! window the source is masked away, unless `maskInverted`), scaled by
//! the alpha itself, with `maskSpreadAtMin`/`maskSpreadAtMax` softening
//! the two edges. Qt's software scene graph — what the BDD Qt backend
//! runs — paints no MultiEffect at all (measured), so the painting is
//! pinned on Ljós only in features/items/multi-effect.feature; the
//! property defaults there are verified on qml6.

use ljos_engine::{Color, Engine, ObjectId, QmlObject, Rect, Value, qml_impl};
use ljos_render::{DisplayListBuilder, DrawOp, ImageHandle, kurbo};

use crate::image::ImagePaint;
use crate::item::Item;

/// `MultiEffect` (QtQuick.Effects).
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Effects", name = "MultiEffect", extends = Item)]
pub struct MultiEffect {
    #[qml(base)]
    pub item: Item,
    /// The item drawn with the effect applied.
    #[qml(notify)]
    pub source: Option<ObjectId>,
    /// Kept; padding is not changed (no blur or shadow is painted).
    #[qml(notify)]
    pub auto_padding_enabled: bool,
    /// Kept; see `auto_padding_enabled`.
    #[qml(notify)]
    pub padding_rect: Rect,
    /// Kept; not painted.
    #[qml(notify)]
    pub brightness: f64,
    /// Kept; not painted.
    #[qml(notify)]
    pub contrast: f64,
    /// Kept; not painted.
    #[qml(notify)]
    pub saturation: f64,
    /// Kept; not painted.
    #[qml(notify)]
    pub colorization: f64,
    /// Kept; not painted.
    #[qml(notify)]
    pub colorization_color: Color,
    /// Kept; no blur is painted.
    #[qml(notify)]
    pub blur_enabled: bool,
    /// Kept; no blur is painted.
    #[qml(notify)]
    pub blur: f64,
    /// Kept; no blur is painted.
    #[qml(notify)]
    pub blur_max: i32,
    /// Kept; no blur is painted.
    #[qml(notify)]
    pub blur_multiplier: f64,
    /// Kept; no shadow is painted.
    #[qml(notify)]
    pub shadow_enabled: bool,
    /// Kept; no shadow is painted.
    #[qml(notify)]
    pub shadow_opacity: f64,
    /// Kept; no shadow is painted.
    #[qml(notify)]
    pub shadow_blur: f64,
    /// Kept; no shadow is painted.
    #[qml(notify)]
    pub shadow_horizontal_offset: f64,
    /// Kept; no shadow is painted.
    #[qml(notify)]
    pub shadow_vertical_offset: f64,
    /// Kept; no shadow is painted.
    #[qml(notify)]
    pub shadow_color: Color,
    /// Kept; no shadow is painted.
    #[qml(notify)]
    pub shadow_scale: f64,
    /// Applies the mask.
    #[qml(notify)]
    pub mask_enabled: bool,
    /// The item whose alpha is the mask.
    #[qml(notify)]
    pub mask_source: Option<ObjectId>,
    /// Mask alphas below this mask the source away.
    #[qml(notify)]
    pub mask_threshold_min: f64,
    /// Softness of the mask edge at the minimum threshold.
    #[qml(notify)]
    pub mask_spread_at_min: f64,
    /// Mask alphas above this mask the source away.
    #[qml(notify)]
    pub mask_threshold_max: f64,
    /// Softness of the mask edge at the maximum threshold.
    #[qml(notify)]
    pub mask_spread_at_max: f64,
    /// Masks away between the thresholds instead of outside them.
    #[qml(notify)]
    pub mask_inverted: bool,
    /// The source's rectangle in this item; kept at the item's bounds.
    #[qml(notify, readonly)]
    pub item_rect: Rect,
    /// Empty: there is no custom shader.
    #[qml(notify, readonly)]
    pub fragment_shader: String,
    /// Empty: there is no custom shader.
    #[qml(notify, readonly)]
    pub vertex_shader: String,
    /// False: no proxy source is made (an `Image` is used directly).
    #[qml(notify, readonly)]
    pub has_proxy_source: bool,
}

impl Default for MultiEffect {
    fn default() -> Self {
        Self {
            item: Item::default(),
            source: None,
            auto_padding_enabled: true,
            padding_rect: Rect::default(),
            brightness: 0.0,
            contrast: 0.0,
            saturation: 0.0,
            colorization: 0.0,
            colorization_color: Color {
                r: 65535,
                g: 0,
                b: 0,
                a: 65535,
            },
            blur_enabled: false,
            blur: 0.0,
            blur_max: 32,
            blur_multiplier: 0.0,
            shadow_enabled: false,
            shadow_opacity: 1.0,
            shadow_blur: 1.0,
            shadow_horizontal_offset: 0.0,
            shadow_vertical_offset: 0.0,
            shadow_color: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 65535,
            },
            shadow_scale: 1.0,
            mask_enabled: false,
            mask_source: None,
            mask_threshold_min: 0.0,
            mask_spread_at_min: 0.0,
            mask_threshold_max: 1.0,
            mask_spread_at_max: 0.0,
            mask_inverted: false,
            item_rect: Rect::default(),
            fragment_shader: String::new(),
            vertex_shader: String::new(),
            has_proxy_source: false,
        }
    }
}

#[qml_impl]
impl MultiEffect {
    /// Never emitted: the shader does not change (there is none).
    #[qml(signal)]
    fn shader_changed();
}

/// Registers module `QtQuick.Effects`.
pub(crate) fn register(engine: &Engine) {
    engine.register_module(ljos_engine::module!("QtQuick.Effects" 1.0 => [MultiEffect]));
}

/// What a `MultiEffect` paints: the composited picture, made when its
/// paint inputs change.
#[derive(Clone, Debug)]
pub struct EffectPaint {
    image: Option<ImageHandle>,
}

impl EffectPaint {
    /// Reads the effect's properties and the pixels of its source (and
    /// mask), all tracked, and composes the picture.
    pub fn read(e: &Engine, obj: ObjectId) -> Self {
        let source = obj_prop(e, obj, MultiEffect::SOURCE.index);
        let image = source.and_then(|src| {
            let source_paint = ImagePaint::read(e, src);
            let pixels = source_paint.image()?.clone();
            if !e.read(obj, MultiEffect::MASK_ENABLED.index).to_boolean() {
                return Some(pixels);
            }
            let mask = obj_prop(e, obj, MultiEffect::MASK_SOURCE.index)?;
            let mask_paint = ImagePaint::read(e, mask);
            let mask = mask_paint.image()?;
            let num = |p| e.read(obj, p).to_number();
            masked(
                &pixels,
                mask,
                Mask {
                    min: num(MultiEffect::MASK_THRESHOLD_MIN.index),
                    spread_min: num(MultiEffect::MASK_SPREAD_AT_MIN.index),
                    max: num(MultiEffect::MASK_THRESHOLD_MAX.index),
                    spread_max: num(MultiEffect::MASK_SPREAD_AT_MAX.index),
                    inverted: e.read(obj, MultiEffect::MASK_INVERTED.index).to_boolean(),
                },
            )
        });
        Self { image }
    }

    pub fn paint(&self, size: kurbo::Size, out: &mut DisplayListBuilder) {
        let Some(image) = &self.image else { return };
        if size.width <= 0.0 || size.height <= 0.0 {
            return;
        }
        out.push(DrawOp::Image {
            image: image.clone(),
            src: image.rect(),
            dst: kurbo::Rect::from_origin_size((0.0, 0.0), size),
            tile: None,
            mirror: [false; 2],
            smooth: true,
        });
    }
}

fn obj_prop(e: &Engine, obj: ObjectId, prop: ljos_engine::PropIndex) -> Option<ObjectId> {
    match e.read(obj, prop) {
        Value::Object(id) => Some(id),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug)]
struct Mask {
    min: f64,
    spread_min: f64,
    max: f64,
    spread_max: f64,
    inverted: bool,
}

impl Mask {
    /// The factor a mask alpha of `a` (0..=1) scales the source by:
    /// the alpha itself inside the threshold window, nothing outside
    /// it, each edge softened over its spread.
    fn factor(self, a: f64) -> f64 {
        let lo = edge(a, self.min, self.spread_min);
        // At the maximum itself the pixel is kept; only above it is it
        // masked away, so the upper edge only bites past `max`.
        let hi = if self.spread_max <= 0.0 {
            if a > self.max { 0.0 } else { 1.0 }
        } else {
            1.0 - edge(a, self.max, self.spread_max)
        };
        let inside = lo.min(hi).clamp(0.0, 1.0);
        let inside = if self.inverted { 1.0 - inside } else { inside };
        a * inside
    }
}

/// 0 below `at - spread/2`, 1 above `at + spread/2`, smooth between;
/// a step at `at` when the spread is 0 (at the threshold itself the
/// pixel is kept).
fn edge(a: f64, at: f64, spread: f64) -> f64 {
    let spread = spread.clamp(0.0, 1.0);
    if spread <= 0.0 {
        return if a >= at { 1.0 } else { 0.0 };
    }
    let t = ((a - (at - spread / 2.0)) / spread).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The source scaled per pixel by the mask, sampled to the source's
/// size. `None` only if the composed picture cannot be made (a size
/// the image type refuses).
fn masked(source: &ImageHandle, mask: &ImageHandle, m: Mask) -> Option<ImageHandle> {
    let (w, h) = (source.width(), source.height());
    let (mw, mh) = (mask.width(), mask.height());
    if w == 0 || h == 0 || mw == 0 || mh == 0 {
        return None;
    }
    let src = source.data();
    let msk = mask.data();
    let mut out = Vec::with_capacity(src.len());
    for y in 0..h {
        let my = y * mh / h;
        for x in 0..w {
            let mx = x * mw / w;
            let a = f64::from(msk[(my as usize * mw as usize + mx as usize) * 4 + 3]) / 255.0;
            let f = m.factor(a);
            let i = (y as usize * w as usize + x as usize) * 4;
            // Premultiplied in, premultiplied out: one scale for all four.
            for c in 0..4 {
                out.push((f64::from(src[i + c]) * f).round() as u8);
            }
        }
    }
    ImageHandle::from_premultiplied(w, h, out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_window_keeps_the_alpha() {
        let m = Mask {
            min: 0.0,
            spread_min: 0.0,
            max: 1.0,
            spread_max: 0.0,
            inverted: false,
        };
        assert_eq!(m.factor(0.0), 0.0);
        assert_eq!(m.factor(0.5), 0.5);
        assert_eq!(m.factor(1.0), 1.0);
    }

    #[test]
    fn outside_the_window_is_masked_away() {
        let m = Mask {
            min: 0.6,
            spread_min: 0.0,
            max: 0.4,
            spread_max: 0.0,
            inverted: false,
        };
        assert_eq!(
            m.factor(0.5),
            0.0,
            "below the minimum and above the maximum"
        );
        let wide = Mask { max: 1.0, ..m };
        assert_eq!(wide.factor(0.5), 0.0, "below the minimum alone");
        assert_eq!(wide.factor(0.6), 0.6, "at the minimum, kept");
    }

    #[test]
    fn a_composite_scales_premultiplied_pixels() {
        let red = ImageHandle::filled(2, 1, [255, 0, 0, 255]).unwrap();
        let half = ImageHandle::filled(1, 1, [255, 255, 255, 128]).unwrap();
        let m = Mask {
            min: 0.0,
            spread_min: 0.0,
            max: 1.0,
            spread_max: 0.0,
            inverted: false,
        };
        let out = masked(&red, &half, m).unwrap();
        assert_eq!(out.pixel(0, 0), [128, 0, 0, 128]);
        assert_eq!(out.pixel(1, 0), [128, 0, 0, 128]);
    }
}
