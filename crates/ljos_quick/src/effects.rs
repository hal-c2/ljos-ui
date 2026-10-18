//! `MultiEffect` (`QtQuick.Effects`): draws its source item with
//! post-processing applied.
//!
//! Reference:
//!   <https://doc.qt.io/qt-6/qml-qtquick-effects-multieffect.html>
//!   <https://doc.qt.io/qt-6/qml-qt5compat-graphicaleffects-brightnesscontrast.html>
//!
//! The MultiEffect page gives ranges, defaults and the padding rules but
//! describes the colour adjustments, blur and shadow only qualitatively
//! and does not state an order. Where it is silent, Ljós uses the
//! standard formulation named below; the Ljós-only scenarios in
//! `features/items/multi-effect.feature` pin these exact numbers.
//!
//! Pipeline order (Ljós' choice; the docs only say all effects combine
//! into one pass):
//! 1. Sample the source item (`Image`, `Canvas`, or `Rectangle`) and, when
//!    `maskEnabled` is true, compute the mask factor from `maskSource`'s
//!    alpha channel using `maskThresholdMin`, `maskSpreadAtMin`,
//!    `maskThresholdMax`, `maskSpreadAtMax`, and `maskInverted`. The mask
//!    only scales alpha and the adjustments only touch straight colour, so
//!    their relative order does not change the result.
//! 2. Apply colour adjustments in straight-alpha sRGB space in the order:
//!    - `brightness` (`[-1, 1]`, documented range): additive shift,
//!      $c' = \text{clamp}(c + \text{brightness}, 0, 1)$.
//!    - `contrast` (`[-1, 1]`, documented range): midpoint scaling around
//!      $0.5$. BrightnessContrast's page says the decrease is linear and the
//!      increase follows a non-linear curve for very high contrast at the top
//!      of the range; Ljós uses slope $k = 1 + \text{contrast}$ for
//!      $\text{contrast} \le 0$ and $k = 1 / (1 - \text{contrast})$ above.
//!    - `saturation` (`[-1, \infty)`, `-1` = "totally desaturated"): chroma
//!      scaling around Rec. 709 luma $Y = 0.2126 r + 0.7152 g + 0.0722 b$,
//!      $c' = \text{clamp}(Y + (c - Y)(1 + \text{saturation}), 0, 1)$.
//!    - `colorization` (`[0, 1]`) and `colorizationColor` (documented as an
//!      RGBA colour): linear blend towards `colorizationColor.rgb` weighted by
//!      $t = \text{clamp}(\text{colorization}, 0, 1) \cdot \text{colorizationColor}.a$.
//!
//!    Re-premultiply by the masked alpha.
//! 3. Compute the padded canvas (`itemRect`): when `autoPaddingEnabled` is
//!    true and `blurEnabled` or `shadowEnabled` is true, each side grows by
//!    $\max(\text{blurMax}, 0) \cdot \max(1 + \text{blurMultiplier}, 0)$
//!    (#autoPaddingEnabled-prop: "padded automatically based on blurMax and
//!    blurMultiplier"; verified on qml6 by the @qt-parity itemRect scenario),
//!    plus the manual `paddingRect` (`left = x`, `top = y`, `right = width`,
//!    `bottom = height`), which "is always added to the size".
//! 4. Blur the masked, colour-adjusted foreground on the padded canvas with a
//!    two-pass separable 1D Gaussian of radius
//!    $R_{\text{fg}} = \text{clamp}(\text{blur}, 0, 1) \cdot \text{blurMax} \cdot (1 + \text{blurMultiplier})$
//!    (#blurMax-prop: "the maximum pixel radius that blur with value 1.0 will
//!    reach"; #blurMultiplier-prop: 0.0 is "not multiplied"). The kernel shape
//!    is not documented: Ljós uses $\sigma = R / 2$, truncated at $R$ and
//!    renormalised, at the item's own pixel resolution.
//! 5. When `shadowEnabled` is true, form the shadow alpha from the masked
//!    source alpha scaled by `shadowScale` around the item centre
//!    (#shadowScale-prop) and shifted by `(shadowHorizontalOffset,
//!    shadowVerticalOffset)`, blur it with the same kernel at radius
//!    $R_{\text{sh}} = \text{clamp}(\text{shadowBlur}, 0, 1) \cdot \text{blurMax} \cdot (1 + \text{blurMultiplier})$
//!    (#shadowBlur-prop), tint by `shadowColor` with its alpha multiplied by
//!    `shadowOpacity` (#shadowOpacity-prop), and composite the foreground over
//!    the shadow using Porter-Duff source-over. The foreground blur does not
//!    blur the shadow; each has its own radius.
//!
//! Performance: `EffectPaint::read` runs inside the scene's reactive
//! `content_effect`, so a static `MultiEffect` schedules no frames and
//! recomputes no pixels after its initial build; it reruns only when a
//! property it read (its own, or its source's/mask's paint inputs) changes.
//! Results are additionally memoized in an engine-wide [`Effects`] cache
//! keyed by source/mask image IDs and the exact effect parameters, so two
//! effects with the same inputs share one picture.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use ljos_engine::{Color, Cx, Engine, EngineError, ObjectId, QmlObject, Rect, Value, qml_impl};
use ljos_render::{
    DisplayListBuilder, DrawOp, ImageHandle, RasterConfig, RenderTree, VelloCpuReference,
    WeakImage, kurbo,
};

use crate::canvas::{Canvas, CanvasPicture};
use crate::image::ImagePaint;
use crate::item::{Item, is_a};
use crate::rectangle::{Rectangle, RectanglePaint, to_rgba};

/// `MultiEffect` (`QtQuick.Effects`).
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Effects", name = "MultiEffect", extends = Item)]
pub struct MultiEffect {
    #[qml(base)]
    pub item: Item,
    /// The item drawn with the effect applied.
    #[qml(notify)]
    pub source: Option<ObjectId>,
    /// Pads `itemRect` by `blurMax * (1 + blurMultiplier)` when blur or shadow is enabled.
    #[qml(notify)]
    pub auto_padding_enabled: bool,
    /// Additional manual padding `(left = x, top = y, right = width, bottom = height)`.
    #[qml(notify)]
    pub padding_rect: Rect,
    /// Brightness shift in `-1.0..=1.0`.
    #[qml(notify)]
    pub brightness: f64,
    /// Contrast adjustment in `-1.0..=1.0`.
    #[qml(notify)]
    pub contrast: f64,
    /// Saturation adjustment (`-1.0` = greyscale, `0.0` = unchanged, `> 0.0` = boosted).
    #[qml(notify)]
    pub saturation: f64,
    /// Colorization blend factor in `0.0..=1.0`.
    #[qml(notify)]
    pub colorization: f64,
    /// Target colour for `colorization` (default `#ff0000`).
    #[qml(notify)]
    pub colorization_color: Color,
    /// Enables the foreground Gaussian blur.
    #[qml(notify)]
    pub blur_enabled: bool,
    /// Normalized blur strength in `0.0..=1.0`.
    #[qml(notify)]
    pub blur: f64,
    /// Maximum blur radius in pixels when `blur == 1.0` (default `32`).
    #[qml(notify)]
    pub blur_max: i32,
    /// Multiplier applied as `1.0 + blurMultiplier` to the blur radius and auto-padding.
    #[qml(notify)]
    pub blur_multiplier: f64,
    /// Enables the drop shadow under the source.
    #[qml(notify)]
    pub shadow_enabled: bool,
    /// Opacity multiplier for the drop shadow in `0.0..=1.0`.
    #[qml(notify)]
    pub shadow_opacity: f64,
    /// Normalized blur strength for the drop shadow in `0.0..=1.0` (default `1.0`).
    #[qml(notify)]
    pub shadow_blur: f64,
    /// Horizontal offset of the drop shadow in pixels.
    #[qml(notify)]
    pub shadow_horizontal_offset: f64,
    /// Vertical offset of the drop shadow in pixels.
    #[qml(notify)]
    pub shadow_vertical_offset: f64,
    /// Colour of the drop shadow (default `#000000`).
    #[qml(notify)]
    pub shadow_color: Color,
    /// Scale factor of the drop shadow around the item centre (default `1.0`).
    #[qml(notify)]
    pub shadow_scale: f64,
    /// Applies the alpha mask from `maskSource`.
    #[qml(notify)]
    pub mask_enabled: bool,
    /// The item whose alpha channel masks the source.
    #[qml(notify)]
    pub mask_source: Option<ObjectId>,
    /// Mask alphas below this threshold mask the source away.
    #[qml(notify)]
    pub mask_threshold_min: f64,
    /// Softness of the mask edge at the minimum threshold.
    #[qml(notify)]
    pub mask_spread_at_min: f64,
    /// Mask alphas above this threshold mask the source away.
    #[qml(notify)]
    pub mask_threshold_max: f64,
    /// Softness of the mask edge at the maximum threshold.
    #[qml(notify)]
    pub mask_spread_at_max: f64,
    /// Masks away between the thresholds instead of outside them.
    #[qml(notify)]
    pub mask_inverted: bool,
    /// Padded geometry of the effect relative to this item.
    #[qml(notify, readonly)]
    pub item_rect: Rect,
    /// Diagnostic fragment shader name (matches `qml6`'s default `"multieffect_c0"`).
    #[qml(notify, readonly)]
    pub fragment_shader: String,
    /// Diagnostic vertex shader name (matches `qml6`'s default `"multieffect_c"`).
    #[qml(notify, readonly)]
    pub vertex_shader: String,
    /// False: source items are sampled directly without an intermediate proxy item.
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
            colorization_color: Color::rgba16(65535, 0, 0, 65535),
            blur_enabled: false,
            blur: 0.0,
            blur_max: 32,
            blur_multiplier: 0.0,
            shadow_enabled: false,
            shadow_opacity: 1.0,
            shadow_blur: 1.0,
            shadow_horizontal_offset: 0.0,
            shadow_vertical_offset: 0.0,
            shadow_color: Color::rgba16(0, 0, 0, 65535),
            shadow_scale: 1.0,
            mask_enabled: false,
            mask_source: None,
            mask_threshold_min: 0.0,
            mask_spread_at_min: 0.0,
            mask_threshold_max: 1.0,
            mask_spread_at_max: 0.0,
            mask_inverted: false,
            item_rect: Rect::default(),
            fragment_shader: "multieffect_c0".into(),
            vertex_shader: "multieffect_c".into(),
            has_proxy_source: false,
        }
    }
}

#[qml_impl]
impl MultiEffect {
    /// Emitted when the diagnostic shader name changes.
    #[qml(signal)]
    fn shader_changed();

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let obj = cx.this;
        cx.engine
            .bind_initial(obj, MultiEffect::ITEM_RECT.index, move |e| {
                Value::Rect(Arc::new(compute_item_rect(e, obj)))
            })?;
        Ok(())
    }
}

/// Registers module `QtQuick.Effects`.
pub(crate) fn register(engine: &Engine) {
    engine.register_module(ljos_engine::module!("QtQuick.Effects" 1.0 => [MultiEffect]));
}

/// Padding on each side `(left, top, right, bottom)` in item coordinates.
///
/// Reference: <https://doc.qt.io/qt-6/qml-qtquick-effects-multieffect.html#itemRect-prop>
/// and `#paddingRect-prop`.
fn compute_padding(e: &Engine, obj: ObjectId) -> (f64, f64, f64, f64) {
    let auto_enabled = e
        .read(obj, MultiEffect::AUTO_PADDING_ENABLED.index)
        .to_boolean();
    let blur_enabled = e.read(obj, MultiEffect::BLUR_ENABLED.index).to_boolean();
    let shadow_enabled = e.read(obj, MultiEffect::SHADOW_ENABLED.index).to_boolean();
    let blur_max = e.read(obj, MultiEffect::BLUR_MAX.index).to_number();
    let blur_mult = e.read(obj, MultiEffect::BLUR_MULTIPLIER.index).to_number();
    let pad_rect = match e.read(obj, MultiEffect::PADDING_RECT.index) {
        Value::Rect(r) => *r,
        _ => Rect::default(),
    };
    let auto_pad = if auto_enabled && (blur_enabled || shadow_enabled) {
        blur_max.max(0.0) * (1.0 + blur_mult).max(0.0)
    } else {
        0.0
    };
    (
        auto_pad + pad_rect.x,
        auto_pad + pad_rect.y,
        auto_pad + pad_rect.width,
        auto_pad + pad_rect.height,
    )
}

/// Computes `itemRect` from the item's `width`, `height`, `autoPaddingEnabled`,
/// `blurEnabled`, `shadowEnabled`, `blurMax`, `blurMultiplier`, and `paddingRect`.
fn compute_item_rect(e: &Engine, obj: ObjectId) -> Rect {
    let w = e.read(obj, Item::WIDTH.index).to_number().max(0.0);
    let h = e.read(obj, Item::HEIGHT.index).to_number().max(0.0);
    let (pl, pt, pr, pb) = compute_padding(e, obj);
    Rect {
        x: -pl,
        y: -pt,
        width: (w + pl + pr).max(0.0),
        height: (h + pt + pb).max(0.0),
    }
}

/// Engine-wide cache of composed `MultiEffect` images, keyed by input image IDs
/// and quantized effect parameters so static or repeated effects do not
/// recompute pixels.
#[derive(Default)]
struct Effects(RefCell<HashMap<EffectKey, WeakImage>>);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct EffectKey {
    source_id: u64,
    mask_id: Option<u64>,
    width: u32,
    height: u32,
    pad: [u32; 4],
    mask: Option<[u64; 5]>,
    adjust: [u64; 5],
    colorization_rgba: [u16; 4],
    fg_blur_bits: u64,
    shadow: Option<ShadowKey>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ShadowKey {
    blur_bits: u64,
    opacity_bits: u64,
    offset_x_bits: u64,
    offset_y_bits: u64,
    scale_bits: u64,
    color_rgba: [u16; 4],
}

/// What a `MultiEffect` paints: the composited picture and its destination
/// rectangle in item coordinates (including any padding).
#[derive(Clone, Debug)]
pub struct EffectPaint {
    image: Option<ImageHandle>,
    pad: [f64; 4],
    inputs: Option<Arc<EffectInputs>>,
}

/// What an effect was given besides its source's pixels.
#[derive(Clone, Debug)]
struct EffectInputs {
    source: ObjectId,
    mask_img: Option<ImageHandle>,
    mask_params: Option<Mask>,
    adjust: ColourAdjust,
    col_c: Color,
    fg_blur_radius: f64,
    shadow: Option<ShadowParams>,
    item_w: f64,
    item_h: f64,
    pad_px: [u32; 4],
    pad: [f64; 4],
}

impl EffectPaint {
    /// Reads the effect's properties and the pixels of its source (and
    /// mask), all tracked, and composes the picture.
    pub fn read(e: &Engine, obj: ObjectId) -> Self {
        let item_w = e.read(obj, Item::WIDTH.index).to_number();
        let item_h = e.read(obj, Item::HEIGHT.index).to_number();
        let (pl, pt, pr, pb) = compute_padding(e, obj);
        let pad_px = [
            pl.round().max(0.0) as u32,
            pt.round().max(0.0) as u32,
            pr.round().max(0.0) as u32,
            pb.round().max(0.0) as u32,
        ];
        let pad = [
            f64::from(pad_px[0]),
            f64::from(pad_px[1]),
            f64::from(pad_px[2]),
            f64::from(pad_px[3]),
        ];

        let num = |p: ljos_engine::PropIndex| e.read(obj, p).to_number();
        let flag = |p: ljos_engine::PropIndex| e.read(obj, p).to_boolean();
        let color = |p: ljos_engine::PropIndex| match e.read(obj, p) {
            Value::Color(c) => c,
            _ => Color::TRANSPARENT,
        };

        let mask_enabled = flag(MultiEffect::MASK_ENABLED.index);
        let mask_params = mask_enabled.then(|| Mask {
            min: num(MultiEffect::MASK_THRESHOLD_MIN.index),
            spread_min: num(MultiEffect::MASK_SPREAD_AT_MIN.index),
            max: num(MultiEffect::MASK_THRESHOLD_MAX.index),
            spread_max: num(MultiEffect::MASK_SPREAD_AT_MAX.index),
            inverted: flag(MultiEffect::MASK_INVERTED.index),
        });

        let col_c = color(MultiEffect::COLORIZATION_COLOR.index);
        let adjust = ColourAdjust {
            brightness: num(MultiEffect::BRIGHTNESS.index),
            contrast: num(MultiEffect::CONTRAST.index),
            saturation: num(MultiEffect::SATURATION.index),
            colorization: num(MultiEffect::COLORIZATION.index),
            colorization_color: [
                f64::from(col_c.r) / 65535.0,
                f64::from(col_c.g) / 65535.0,
                f64::from(col_c.b) / 65535.0,
                f64::from(col_c.a) / 65535.0,
            ],
        };

        let blur_enabled = flag(MultiEffect::BLUR_ENABLED.index);
        let blur = num(MultiEffect::BLUR.index);
        let blur_max = num(MultiEffect::BLUR_MAX.index).max(0.0);
        let blur_mult = (1.0 + num(MultiEffect::BLUR_MULTIPLIER.index)).max(0.0);
        let fg_blur_radius = if blur_enabled {
            blur.clamp(0.0, 1.0) * blur_max * blur_mult
        } else {
            0.0
        };

        let shadow_enabled = flag(MultiEffect::SHADOW_ENABLED.index);
        let shadow_opacity = num(MultiEffect::SHADOW_OPACITY.index);
        let shadow_blur = num(MultiEffect::SHADOW_BLUR.index);
        let shadow_hx = num(MultiEffect::SHADOW_HORIZONTAL_OFFSET.index);
        let shadow_vy = num(MultiEffect::SHADOW_VERTICAL_OFFSET.index);
        let shadow_col = color(MultiEffect::SHADOW_COLOR.index);
        let shadow_scale = num(MultiEffect::SHADOW_SCALE.index);
        let shadow = shadow_enabled.then(|| ShadowParams {
            blur_radius: shadow_blur.clamp(0.0, 1.0) * blur_max * blur_mult,
            opacity: shadow_opacity.clamp(0.0, 1.0),
            offset_x: shadow_hx,
            offset_y: shadow_vy,
            scale: shadow_scale,
            color: shadow_col,
        });

        let source_obj = obj_prop(e, obj, MultiEffect::SOURCE.index);
        let mask_obj = if mask_enabled {
            obj_prop(e, obj, MultiEffect::MASK_SOURCE.index)
        } else {
            None
        };
        let source_img = source_obj.and_then(|src| read_item_image(e, src));
        let mask_img = if mask_enabled {
            let Some(m) = mask_obj.and_then(|m| read_item_image(e, m)) else {
                return Self::none(None);
            };
            Some(m)
        } else {
            None
        };
        let Some(source) = source_obj else {
            return Self::none(None);
        };
        let inputs = Arc::new(EffectInputs {
            source,
            mask_img,
            mask_params,
            adjust,
            col_c,
            fg_blur_radius,
            shadow,
            item_w,
            item_h,
            pad_px,
            pad,
        });
        match source_img {
            Some(img) => Self::finish(e, &inputs, img),
            // An item with nothing of its own to sample is drawn by the
            // scene, from its subtree ([`EffectPaint::with_source`]).
            None => Self::none(Some(inputs)),
        }
    }

    fn none(inputs: Option<Arc<EffectInputs>>) -> Self {
        Self {
            image: None,
            pad: [0.0; 4],
            inputs,
        }
    }

    /// The item it draws, when it has one.
    pub(crate) fn source(&self) -> Option<ObjectId> {
        self.inputs.as_ref().map(|i| i.source)
    }

    /// The same effect drawn from `source_img`, a picture of the source
    /// item with all it shows: its children are part of what the effect
    /// draws, as they are of the texture Qt renders the item into.
    pub(crate) fn with_source(&self, e: &Engine, source_img: ImageHandle) -> Self {
        match &self.inputs {
            Some(inputs) => Self::finish(e, inputs, source_img),
            None => self.clone(),
        }
    }

    fn finish(e: &Engine, inputs: &Arc<EffectInputs>, source_img: ImageHandle) -> Self {
        let EffectInputs {
            mask_img,
            mask_params,
            adjust,
            col_c,
            fg_blur_radius,
            shadow,
            item_w,
            item_h,
            pad_px,
            pad,
            ..
        } = (**inputs).clone();
        let inputs = Some(Arc::clone(inputs));

        let has_spatial = fg_blur_radius > 0.0 || shadow.is_some() || pad_px.iter().any(|&p| p > 0);

        // Fast path: nothing enabled at all -> draw the source image directly.
        if !has_spatial && mask_params.is_none() && adjust.is_identity() {
            return Self {
                image: Some(source_img),
                pad: [0.0; 4],
                inputs,
            };
        }

        let target_w = if has_spatial {
            item_w.round().max(0.0) as u32
        } else {
            source_img.width()
        };
        let target_h = if has_spatial {
            item_h.round().max(0.0) as u32
        } else {
            source_img.height()
        };
        if target_w == 0 || target_h == 0 {
            return Self::none(inputs);
        }

        let key = EffectKey {
            source_id: source_img.id(),
            mask_id: mask_img.as_ref().map(ImageHandle::id),
            width: target_w,
            height: target_h,
            pad: if has_spatial { pad_px } else { [0; 4] },
            mask: mask_params.map(|m| {
                [
                    m.min.to_bits(),
                    m.spread_min.to_bits(),
                    m.max.to_bits(),
                    m.spread_max.to_bits(),
                    u64::from(m.inverted),
                ]
            }),
            adjust: [
                adjust.brightness.to_bits(),
                adjust.contrast.to_bits(),
                adjust.saturation.to_bits(),
                adjust.colorization.to_bits(),
                u64::from(!adjust.is_identity()),
            ],
            colorization_rgba: [col_c.r, col_c.g, col_c.b, col_c.a],
            fg_blur_bits: fg_blur_radius.to_bits(),
            shadow: shadow.as_ref().map(|s| ShadowKey {
                blur_bits: s.blur_radius.to_bits(),
                opacity_bits: s.opacity.to_bits(),
                offset_x_bits: s.offset_x.to_bits(),
                offset_y_bits: s.offset_y.to_bits(),
                scale_bits: s.scale.to_bits(),
                color_rgba: [s.color.r, s.color.g, s.color.b, s.color.a],
            }),
        };

        let cache = e.extension::<Effects>();
        if let Some(hit) = cache.0.borrow().get(&key).and_then(WeakImage::upgrade) {
            return Self {
                image: Some(hit),
                pad: if has_spatial { pad } else { [0.0; 4] },
                inputs,
            };
        }

        let composed = compose_effect(
            &source_img,
            mask_img.as_ref(),
            mask_params,
            adjust,
            (target_w, target_h),
            if has_spatial { pad_px } else { [0; 4] },
            fg_blur_radius,
            shadow,
        );

        if let Some(img) = &composed {
            let mut map = cache.0.borrow_mut();
            map.retain(|_, v| v.is_alive());
            map.insert(key, img.downgrade());
        }

        Self {
            image: composed,
            pad: if has_spatial { pad } else { [0.0; 4] },
            inputs,
        }
    }

    pub fn paint(&self, size: kurbo::Size, out: &mut DisplayListBuilder) {
        let Some(image) = &self.image else { return };
        if size.width <= 0.0 || size.height <= 0.0 {
            return;
        }
        let [pl, pt, pr, pb] = self.pad;
        let dst = kurbo::Rect::new(-pl, -pt, size.width + pr, size.height + pb);
        if dst.width() <= 0.0 || dst.height() <= 0.0 {
            return;
        }
        out.push(DrawOp::Image {
            image: image.clone(),
            src: image.rect(),
            dst,
            tile: None,
            mirror: [false; 2],
            smooth: true,
            opacity: 1.0,
            tint: None,
        });
    }
}

fn obj_prop(e: &Engine, obj: ObjectId, prop: ljos_engine::PropIndex) -> Option<ObjectId> {
    match e.read(obj, prop) {
        Value::Object(id) => Some(id),
        _ => None,
    }
}

/// Extracts an [`ImageHandle`] from a source or mask item (`Image`, `Canvas`,
/// or `Rectangle`), tracking its reactive paint inputs.
/// The pixels of a composed item and what it shows (from
/// [`RenderTree::compose_subtree`]), one per logical pixel.
pub(crate) fn picture(snap: &ljos_render::FrameSnapshot) -> Option<ImageHandle> {
    picture_at(snap, 1.0)
}

/// The same, `scale` pixels per logical pixel.
pub(crate) fn picture_at(snap: &ljos_render::FrameSnapshot, scale: f32) -> Option<ImageHandle> {
    if snap.viewport.width() < 1.0 || snap.viewport.height() < 1.0 {
        return None;
    }
    let mut r = VelloCpuReference::new(RasterConfig::default());
    let rgba = r.render(&ljos_render::FrameView::new(snap, scale)).ok()?;
    ImageHandle::from_straight(rgba.width, rgba.height, rgba.data).ok()
}

pub(crate) fn read_item_image(e: &Engine, item: ObjectId) -> Option<ImageHandle> {
    if is_a::<crate::image::Image>(e, item) {
        return ImagePaint::read(e, item).image().cloned();
    }
    if is_a::<Canvas>(e, item) {
        let mut b = DisplayListBuilder::new();
        CanvasPicture::read(e, item).paint(kurbo::Size::ZERO, &mut b);
        return b.finish().ops().iter().find_map(|op| match op {
            DrawOp::Image { image, .. } => Some(image.clone()),
            _ => None,
        });
    }
    if is_a::<Rectangle>(e, item) {
        let w = e.read(item, Item::WIDTH.index).to_number().round().max(0.0) as u32;
        let h = e
            .read(item, Item::HEIGHT.index)
            .to_number()
            .round()
            .max(0.0) as u32;
        if w == 0 || h == 0 {
            return None;
        }
        let rp = RectanglePaint::read(e, item);
        if rp.radii.iter().all(|&r| r <= 0.0) && rp.border_width <= 0.0 && rp.gradient.is_none() {
            return ImageHandle::filled(w, h, to_rgba(rp.color).to_premul_u8()).ok();
        }
        let size = kurbo::Size::new(f64::from(w), f64::from(h));
        let mut tree = RenderTree::new();
        let mut b = DisplayListBuilder::new();
        rp.paint(size, &mut b);
        tree.set_content(tree.root(), Some(b.finish_arc()));
        let snap = tree.compose(kurbo::Rect::from_origin_size((0.0, 0.0), size), 0);
        let mut r = VelloCpuReference::new(RasterConfig::default());
        let rgba = r.render(&ljos_render::FrameView::new(&snap, 1.0)).ok()?;
        return ImageHandle::from_straight(rgba.width, rgba.height, rgba.data).ok();
    }
    ImagePaint::read(e, item).image().cloned()
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

/// Colour adjustment parameters (`brightness`, `contrast`, `saturation`,
/// `colorization` / `colorizationColor`).
///
/// Reference: <https://doc.qt.io/qt-6/qml-qtquick-effects-multieffect.html>
#[derive(Clone, Copy, Debug)]
struct ColourAdjust {
    brightness: f64,
    contrast: f64,
    saturation: f64,
    colorization: f64,
    colorization_color: [f64; 4],
}

impl Default for ColourAdjust {
    fn default() -> Self {
        Self {
            brightness: 0.0,
            contrast: 0.0,
            saturation: 0.0,
            colorization: 0.0,
            colorization_color: [1.0, 0.0, 0.0, 1.0],
        }
    }
}

impl ColourAdjust {
    fn is_identity(self) -> bool {
        self.brightness == 0.0
            && self.contrast == 0.0
            && self.saturation == 0.0
            && (self.colorization <= 0.0 || self.colorization_color[3] <= 0.0)
    }

    /// Applies `brightness` -> `contrast` -> `saturation` -> `colorization` to
    /// straight-alpha sRGB channels in `0.0..=1.0`.
    fn apply(self, r: f64, g: f64, b: f64) -> (f64, f64, f64) {
        // 1. Brightness: the docs give only the range (#brightness-prop);
        //    an additive shift is the standard formulation.
        let mut r = (r + self.brightness).clamp(0.0, 1.0);
        let mut g = (g + self.brightness).clamp(0.0, 1.0);
        let mut b = (b + self.brightness).clamp(0.0, 1.0);

        // 2. Contrast: BrightnessContrast#contrast-prop says decrease is
        //    linear and increase a non-linear curve reaching very high
        //    contrast at the top; 1/(1 - c) is the curve Ljós picks.
        if self.contrast != 0.0 {
            let c = self.contrast.clamp(-1.0, 1.0);
            let k = if c <= 0.0 {
                1.0 + c
            } else {
                1.0 / (1.0 - c).max(1e-6)
            };
            r = ((r - 0.5) * k + 0.5).clamp(0.0, 1.0);
            g = ((g - 0.5) * k + 0.5).clamp(0.0, 1.0);
            b = ((b - 0.5) * k + 0.5).clamp(0.0, 1.0);
        }

        // 3. Saturation: #saturation-prop gives -1 as "totally desaturated"
        //    and no upper bound; chroma scaling around Rec. 709 luma is the
        //    standard formulation (not Qt 5 Desaturate's plain RGB average).
        if self.saturation != 0.0 {
            let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            let s = (1.0 + self.saturation).max(0.0);
            r = (luma + (r - luma) * s).clamp(0.0, 1.0);
            g = (luma + (g - luma) * s).clamp(0.0, 1.0);
            b = (luma + (b - luma) * s).clamp(0.0, 1.0);
        }

        // 4. Colorization: linear blend towards colorizationColor weighted by
        //    colorization * colorizationColor.a (#colorization-prop gives the
        //    0..1 range, #colorizationColor-prop calls it an RGBA colour).
        let t = self.colorization.clamp(0.0, 1.0) * self.colorization_color[3].clamp(0.0, 1.0);
        if t > 0.0 {
            r = (r + (self.colorization_color[0] - r) * t).clamp(0.0, 1.0);
            g = (g + (self.colorization_color[1] - g) * t).clamp(0.0, 1.0);
            b = (b + (self.colorization_color[2] - b) * t).clamp(0.0, 1.0);
        }

        (r, g, b)
    }
}

#[derive(Clone, Copy, Debug)]
struct ShadowParams {
    blur_radius: f64,
    opacity: f64,
    offset_x: f64,
    offset_y: f64,
    scale: f64,
    color: Color,
}

/// Applies the mask and colour adjustments to `source`, returning a
/// premultiplied RGBA8 image at `source`'s dimensions.
fn mask_and_adjust(
    source: &ImageHandle,
    mask: Option<&ImageHandle>,
    mask_params: Option<Mask>,
    adjust: ColourAdjust,
) -> Option<ImageHandle> {
    let (w, h) = (source.width(), source.height());
    if w == 0 || h == 0 {
        return None;
    }
    let mask_pair = match (mask, mask_params) {
        (Some(m), Some(mp)) => {
            if m.width() == 0 || m.height() == 0 {
                return None;
            }
            Some((m.data(), m.width(), m.height(), mp))
        }
        _ => None,
    };

    if mask_pair.is_none() && adjust.is_identity() {
        return Some(source.clone());
    }

    let src = source.data();
    let mut out = Vec::with_capacity(src.len());
    let identity_colour = adjust.is_identity();

    for y in 0..h {
        for x in 0..w {
            let mask_f = match mask_pair {
                Some((ref msk, mw, mh, m)) => {
                    let mx = x * mw / w;
                    let my = y * mh / h;
                    let a =
                        f64::from(msk[(my as usize * mw as usize + mx as usize) * 4 + 3]) / 255.0;
                    m.factor(a)
                }
                None => 1.0,
            };

            let i = (y as usize * w as usize + x as usize) * 4;
            let pa = src[i + 3];
            if pa == 0 || mask_f <= 0.0 {
                out.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }

            if identity_colour {
                for c in 0..4 {
                    out.push((f64::from(src[i + c]) * mask_f).round() as u8);
                }
            } else {
                let inv_a = 1.0 / f64::from(pa);
                let r = (f64::from(src[i]) * inv_a).clamp(0.0, 1.0);
                let g = (f64::from(src[i + 1]) * inv_a).clamp(0.0, 1.0);
                let b = (f64::from(src[i + 2]) * inv_a).clamp(0.0, 1.0);
                let (r, g, b) = adjust.apply(r, g, b);
                let out_a = (f64::from(pa) / 255.0) * mask_f;
                let scale = out_a * 255.0;
                out.push((r * scale).round().clamp(0.0, 255.0) as u8);
                out.push((g * scale).round().clamp(0.0, 255.0) as u8);
                out.push((b * scale).round().clamp(0.0, 255.0) as u8);
                out.push(scale.round().clamp(0.0, 255.0) as u8);
            }
        }
    }
    ImageHandle::from_premultiplied(w, h, out).ok()
}

/// Composes the full `MultiEffect` pipeline (mask, colour adjustments, padded
/// canvas, foreground Gaussian blur, and drop shadow).
#[allow(clippy::too_many_arguments)]
fn compose_effect(
    source: &ImageHandle,
    mask: Option<&ImageHandle>,
    mask_params: Option<Mask>,
    adjust: ColourAdjust,
    target_size: (u32, u32),
    pad: [u32; 4],
    fg_blur_radius: f64,
    shadow: Option<ShadowParams>,
) -> Option<ImageHandle> {
    let adjusted = mask_and_adjust(source, mask, mask_params, adjust)?;
    let has_spatial = fg_blur_radius > 0.0 || shadow.is_some() || pad.iter().any(|&p| p > 0);
    if !has_spatial {
        return Some(adjusted);
    }

    let (w, h) = (target_size.0 as usize, target_size.1 as usize);
    let base = adjusted.resized(target_size.0, target_size.1)?;
    let base_data = base.data();

    let [pl, pt, pr, pb] = [
        pad[0] as usize,
        pad[1] as usize,
        pad[2] as usize,
        pad[3] as usize,
    ];
    let cw = w.checked_add(pl)?.checked_add(pr)?;
    let ch = h.checked_add(pt)?.checked_add(pb)?;
    if cw == 0 || ch == 0 {
        return None;
    }

    // Place the (masked, colour-adjusted) source at `(pl, pt)` inside the
    // `cw x ch` padded canvas.
    let mut fg = vec![0u8; cw * ch * 4];
    for y in 0..h {
        let src_row = &base_data[y * w * 4..(y + 1) * w * 4];
        let dst_off = ((pt + y) * cw + pl) * 4;
        fg[dst_off..dst_off + w * 4].copy_from_slice(src_row);
    }

    if fg_blur_radius > 0.0 {
        fg = gaussian_blur_rgba(&fg, cw, ch, fg_blur_radius);
    }

    if let Some(sh) = shadow
        && sh.opacity > 0.0
        && sh.color.a > 0
        && sh.scale > 0.0
    {
        let mut sh_alpha = vec![0.0f32; cw * ch];
        let center_x = pl as f64 + w as f64 * 0.5;
        let center_y = pt as f64 + h as f64 * 0.5;
        let inv_scale = 1.0 / sh.scale;

        for cy in 0..ch {
            let py = cy as f64 + 0.5;
            let sy = (py - sh.offset_y - center_y) * inv_scale + h as f64 * 0.5;
            if sy < 0.0 || sy >= h as f64 {
                continue;
            }
            let sy_i = sy.floor() as usize;
            for cx in 0..cw {
                let px = cx as f64 + 0.5;
                let sx = (px - sh.offset_x - center_x) * inv_scale + w as f64 * 0.5;
                if sx < 0.0 || sx >= w as f64 {
                    continue;
                }
                let sx_i = sx.floor() as usize;
                let a = base_data[(sy_i * w + sx_i) * 4 + 3];
                sh_alpha[cy * cw + cx] = f32::from(a) / 255.0;
            }
        }

        if sh.blur_radius > 0.0 {
            sh_alpha = gaussian_blur_alpha(&sh_alpha, cw, ch, sh.blur_radius);
        }

        let col_r = f64::from(sh.color.r) / 65535.0;
        let col_g = f64::from(sh.color.g) / 65535.0;
        let col_b = f64::from(sh.color.b) / 65535.0;
        let col_a = (f64::from(sh.color.a) / 65535.0) * sh.opacity;

        for (i, a) in sh_alpha.iter().enumerate() {
            let sa = (f64::from(*a) * col_a).clamp(0.0, 1.0);
            if sa <= 0.0 {
                continue;
            }
            let p = i * 4;
            let fa = f64::from(fg[p + 3]) / 255.0;
            let inv_fa = 1.0 - fa;
            let sa_255 = sa * 255.0;
            let sr = col_r * sa_255;
            let sg = col_g * sa_255;
            let sb = col_b * sa_255;

            fg[p] = (f64::from(fg[p]) + sr * inv_fa).round().clamp(0.0, 255.0) as u8;
            fg[p + 1] = (f64::from(fg[p + 1]) + sg * inv_fa)
                .round()
                .clamp(0.0, 255.0) as u8;
            fg[p + 2] = (f64::from(fg[p + 2]) + sb * inv_fa)
                .round()
                .clamp(0.0, 255.0) as u8;
            fg[p + 3] = (f64::from(fg[p + 3]) + sa_255 * inv_fa)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }

    ImageHandle::from_premultiplied(cw as u32, ch as u32, fg).ok()
}

/// Builds a normalized 1D Gaussian kernel of half-width $r = \lceil R \rceil$
/// and standard deviation $\sigma = R / 2$.
fn gaussian_kernel(radius: f64) -> Option<(usize, Vec<f32>)> {
    if !radius.is_finite() || radius <= 0.0 {
        return None;
    }
    let r = (radius.ceil() as usize).min(256);
    if r == 0 {
        return None;
    }
    let sigma = (radius / 2.0).max(0.5);
    let two_sigma_sq = 2.0 * sigma * sigma;
    let mut weights = Vec::with_capacity(2 * r + 1);
    let mut sum = 0.0f64;
    let r_i = r as isize;
    for k in -r_i..=r_i {
        let d = k as f64;
        let w = (-d * d / two_sigma_sq).exp();
        weights.push(w);
        sum += w;
    }
    let norm = weights.into_iter().map(|w| (w / sum) as f32).collect();
    Some((r, norm))
}

/// Two-pass separable 1D Gaussian blur over a premultiplied RGBA8 buffer of
/// size `w x h`, with transparent (`0`) boundary outside `0..w` and `0..h`.
fn gaussian_blur_rgba(src: &[u8], w: usize, h: usize, radius: f64) -> Vec<u8> {
    let Some((r, kernel)) = gaussian_kernel(radius) else {
        return src.to_vec();
    };
    let r_i = r as isize;
    let w_i = w as isize;
    let h_i = h as isize;

    let mut mid = vec![0.0f32; w * h * 4];
    for y in 0..h {
        let row_off = y * w * 4;
        for x in 0..w {
            let mut acc = [0.0f32; 4];
            let k_min = (-r_i).max(-(x as isize));
            let k_max = r_i.min(w_i - 1 - x as isize);
            for k in k_min..=k_max {
                let weight = kernel[(k + r_i) as usize];
                let sx = (x as isize + k) as usize;
                let p = row_off + sx * 4;
                acc[0] += f32::from(src[p]) * weight;
                acc[1] += f32::from(src[p + 1]) * weight;
                acc[2] += f32::from(src[p + 2]) * weight;
                acc[3] += f32::from(src[p + 3]) * weight;
            }
            let dst = row_off + x * 4;
            mid[dst..dst + 4].copy_from_slice(&acc);
        }
    }

    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        let k_min = (-r_i).max(-(y as isize));
        let k_max = r_i.min(h_i - 1 - y as isize);
        for x in 0..w {
            let mut acc = [0.0f32; 4];
            for k in k_min..=k_max {
                let weight = kernel[(k + r_i) as usize];
                let sy = (y as isize + k) as usize;
                let p = (sy * w + x) * 4;
                acc[0] += mid[p] * weight;
                acc[1] += mid[p + 1] * weight;
                acc[2] += mid[p + 2] * weight;
                acc[3] += mid[p + 3] * weight;
            }
            let dst = (y * w + x) * 4;
            out[dst] = acc[0].round().clamp(0.0, 255.0) as u8;
            out[dst + 1] = acc[1].round().clamp(0.0, 255.0) as u8;
            out[dst + 2] = acc[2].round().clamp(0.0, 255.0) as u8;
            out[dst + 3] = acc[3].round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// Two-pass separable 1D Gaussian blur over a single-channel alpha buffer in
/// `0.0..=1.0`, with `0.0` outside `0..w` and `0..h`.
fn gaussian_blur_alpha(src: &[f32], w: usize, h: usize, radius: f64) -> Vec<f32> {
    let Some((r, kernel)) = gaussian_kernel(radius) else {
        return src.to_vec();
    };
    let r_i = r as isize;
    let w_i = w as isize;
    let h_i = h as isize;

    let mut mid = vec![0.0f32; w * h];
    for y in 0..h {
        let row_off = y * w;
        for x in 0..w {
            let mut acc = 0.0f32;
            let k_min = (-r_i).max(-(x as isize));
            let k_max = r_i.min(w_i - 1 - x as isize);
            for k in k_min..=k_max {
                let weight = kernel[(k + r_i) as usize];
                let sx = (x as isize + k) as usize;
                acc += src[row_off + sx] * weight;
            }
            mid[row_off + x] = acc;
        }
    }

    let mut out = vec![0.0f32; w * h];
    for y in 0..h {
        let k_min = (-r_i).max(-(y as isize));
        let k_max = r_i.min(h_i - 1 - y as isize);
        for x in 0..w {
            let mut acc = 0.0f32;
            for k in k_min..=k_max {
                let weight = kernel[(k + r_i) as usize];
                let sy = (y as isize + k) as usize;
                acc += mid[sy * w + x] * weight;
            }
            out[y * w + x] = acc;
        }
    }
    out
}

/// The source scaled per pixel by the mask, sampled to the source's
/// size. `None` only if the composed picture cannot be made (a size
/// the image type refuses).
#[cfg(test)]
fn masked(source: &ImageHandle, mask: &ImageHandle, m: Mask) -> Option<ImageHandle> {
    mask_and_adjust(source, Some(mask), Some(m), ColourAdjust::default())
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

    #[test]
    fn colour_adjustments_follow_documented_formulas() {
        let red = ImageHandle::filled(1, 1, [255, 0, 0, 255]).unwrap();

        // Brightness -0.5 -> #800000, +0.5 -> #ff8080
        let dark = mask_and_adjust(
            &red,
            None,
            None,
            ColourAdjust {
                brightness: -0.5,
                ..ColourAdjust::default()
            },
        )
        .unwrap();
        assert_eq!(dark.pixel(0, 0), [128, 0, 0, 255]);

        let bright = mask_and_adjust(
            &red,
            None,
            None,
            ColourAdjust {
                brightness: 0.5,
                ..ColourAdjust::default()
            },
        )
        .unwrap();
        assert_eq!(bright.pixel(0, 0), [255, 128, 128, 255]);

        // Contrast -0.5 -> #bf4040, -1.0 -> #808080
        let low_c = mask_and_adjust(
            &red,
            None,
            None,
            ColourAdjust {
                contrast: -0.5,
                ..ColourAdjust::default()
            },
        )
        .unwrap();
        assert_eq!(low_c.pixel(0, 0), [191, 64, 64, 255]);

        // Saturation -1.0 -> Rec. 709 luma of red (0.2126 * 255 = 54) -> #363636
        let desat = mask_and_adjust(
            &red,
            None,
            None,
            ColourAdjust {
                saturation: -1.0,
                ..ColourAdjust::default()
            },
        )
        .unwrap();
        assert_eq!(desat.pixel(0, 0), [54, 54, 54, 255]);

        // Colorization 0.5 towards blue (0, 0, 1, 1) -> #800080
        let col = mask_and_adjust(
            &red,
            None,
            None,
            ColourAdjust {
                colorization: 0.5,
                colorization_color: [0.0, 0.0, 1.0, 1.0],
                ..ColourAdjust::default()
            },
        )
        .unwrap();
        assert_eq!(col.pixel(0, 0), [128, 0, 128, 255]);
    }

    #[test]
    fn gaussian_blur_preserves_symmetry_and_centre_peak() {
        let red = ImageHandle::filled(5, 5, [255, 0, 0, 255]).unwrap();
        let blurred = compose_effect(
            &red,
            None,
            None,
            ColourAdjust::default(),
            (5, 5),
            [4, 4, 4, 4],
            2.0,
            None,
        )
        .unwrap();
        assert_eq!((blurred.width(), blurred.height()), (13, 13));
        // Centre (6, 6) stays full red; 4 cardinal inner edges and 4 outer neighbours match.
        assert_eq!(blurred.pixel(6, 6), [255, 0, 0, 255]);
        let inner = blurred.pixel(4, 6);
        assert_eq!(inner, [179, 0, 0, 179]);
        assert_eq!(blurred.pixel(8, 6), inner);
        assert_eq!(blurred.pixel(6, 4), inner);
        assert_eq!(blurred.pixel(6, 8), inner);
        let outer = blurred.pixel(3, 6);
        assert_eq!(outer, [76, 0, 0, 76]);
        assert_eq!(blurred.pixel(9, 6), outer);
        assert_eq!(blurred.pixel(6, 3), outer);
        assert_eq!(blurred.pixel(6, 9), outer);
    }
}
