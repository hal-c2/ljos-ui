//! `Texture`: an image a material samples, loaded when a view first draws
//! with it.

use std::cell::RefCell;

use ljos::item::ljos_3d::{self, PixelEncoding, TextureImage, TexturePixels, Tiling};
use ljos::item::{self, ImageEncoding, ImageHandle, kurbo};
use ljos::prelude::*;

use crate::Object3D;
use crate::source::EngineSource;

use ljos::item::ljos_3d::qml::defaults as d;
pub(crate) use ljos::item::ljos_3d::qml::enums::{filter, tiling};

fn texture_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "MappingMode",
            vec![("UV", 0), ("Environment", 1), ("LightProbe", 2)],
        ),
        (
            "TilingMode",
            vec![
                ("ClampToEdge", i64::from(tiling::CLAMP_TO_EDGE)),
                ("MirroredRepeat", i64::from(tiling::MIRRORED_REPEAT)),
                ("Repeat", i64::from(tiling::REPEAT)),
            ],
        ),
        (
            "Filter",
            vec![
                ("None", i64::from(filter::NONE)),
                ("Nearest", i64::from(filter::NEAREST)),
                ("Linear", i64::from(filter::LINEAR)),
            ],
        ),
    ]
}

/// `Texture`: the picture at `source`, laid over a mesh's UV coordinates.
///
/// The picture is read when a view first draws a material that has it,
/// through the image cache, so a picture an `Image` shows is not decoded
/// twice. Until it is ready the material draws as if it had no map; the
/// views that drew it are painted again when it arrives.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D",
    name = "Texture",
    extends = Object3D,
    enums = texture_enums
)]
pub struct Texture {
    #[qml(base)]
    pub object3d: Object3D,
    #[qml(notify)]
    pub source: Url,
    /// An item drawn into the texture, live, instead of `source`.
    #[qml(notify, object = "Item")]
    pub source_item: Option<ObjectId>,
    /// Pixels given as data instead of `source`.
    #[qml(notify, object = "TextureData")]
    pub texture_data: Option<ObjectId>,
    #[qml(notify)]
    pub scale_u: f64,
    #[qml(notify)]
    pub scale_v: f64,
    #[qml(notify)]
    pub position_u: f64,
    #[qml(notify)]
    pub position_v: f64,
    /// Degrees, about the pivot.
    #[qml(notify, rename = "rotationUV")]
    pub rotation_uv: f64,
    #[qml(notify)]
    pub pivot_u: f64,
    #[qml(notify)]
    pub pivot_v: f64,
    #[qml(notify)]
    pub flip_u: bool,
    #[qml(notify)]
    pub flip_v: bool,
    /// Which of a mesh's two UV sets the texture is laid out by.
    #[qml(notify, rename = "indexUV")]
    pub index_uv: i32,
    /// Kept: a material reads its maps by UV and a light probe as one.
    #[qml(notify, enumeration = "MappingMode")]
    pub mapping_mode: i32,
    #[qml(notify, enumeration = "TilingMode")]
    pub tiling_mode_horizontal: i32,
    #[qml(notify, enumeration = "TilingMode")]
    pub tiling_mode_vertical: i32,
    /// Kept: no texture here has depth.
    #[qml(notify, enumeration = "TilingMode")]
    pub tiling_mode_depth: i32,
    #[qml(notify, enumeration = "Filter")]
    pub min_filter: i32,
    #[qml(notify, enumeration = "Filter")]
    pub mag_filter: i32,
    #[qml(notify, enumeration = "Filter")]
    pub mip_filter: i32,
    #[qml(notify)]
    pub generate_mipmaps: bool,
    /// Kept: pictures here are read top row first, as Qt orients them.
    #[qml(notify)]
    pub auto_orientation: bool,
    #[qml(skip)]
    loaded: RefCell<Loaded>,
}

impl Default for Texture {
    fn default() -> Self {
        Texture {
            object3d: Object3D::default(),
            source: Url::default(),
            source_item: None,
            texture_data: None,
            scale_u: d::texture::SCALE_U,
            scale_v: d::texture::SCALE_V,
            position_u: d::texture::POSITION_U,
            position_v: d::texture::POSITION_V,
            rotation_uv: d::texture::ROTATION_UV,
            pivot_u: d::texture::PIVOT_U,
            pivot_v: d::texture::PIVOT_V,
            flip_u: d::texture::FLIP_U,
            flip_v: d::texture::FLIP_V,
            index_uv: d::texture::INDEX_UV,
            mapping_mode: 0,
            tiling_mode_horizontal: d::texture::TILING_MODE,
            tiling_mode_vertical: d::texture::TILING_MODE,
            tiling_mode_depth: d::texture::TILING_MODE,
            min_filter: d::texture::MIN_FILTER,
            mag_filter: d::texture::MAG_FILTER,
            mip_filter: d::texture::MIP_FILTER,
            generate_mipmaps: d::texture::GENERATE_MIPMAPS,
            auto_orientation: d::texture::AUTO_ORIENTATION,
            loaded: RefCell::new(Loaded::default()),
        }
    }
}

/// What has come of reading `url`.
#[derive(Debug, Default)]
struct Loaded {
    url: String,
    state: State,
}

#[derive(Debug, Default)]
enum State {
    #[default]
    Idle,
    /// Being read; the views to paint again when it is ready.
    Reading(Vec<ObjectId>),
    /// One image for every scene, so the renderer's upload of it is shared
    /// and kept while any scene has it.
    Ready(TextureImage),
    /// Could not be read: drawn without, and not read again until the
    /// source changes.
    Failed,
}

/// A decoded picture as the 3D renderer reads it.
#[derive(Debug)]
struct ImagePixels(ImageHandle);

impl ImagePixels {
    fn encoding_of(image: &ImageHandle) -> PixelEncoding {
        match image.encoding() {
            ImageEncoding::Premultiplied => PixelEncoding::Srgb8Premultiplied,
            ImageEncoding::Straight => PixelEncoding::Srgb8Straight,
            ImageEncoding::Rgbe => PixelEncoding::Rgbe,
        }
    }
}

impl TexturePixels for ImagePixels {
    fn id(&self) -> u64 {
        self.0.id()
    }

    fn size(&self) -> [u32; 2] {
        [self.0.width(), self.0.height()]
    }

    fn encoding(&self) -> PixelEncoding {
        Self::encoding_of(&self.0)
    }

    fn opaque(&self) -> bool {
        self.0.is_opaque()
    }

    fn with_pixels(&self, f: &mut dyn FnMut(&[u8])) {
        f(&self.0.data_to_upload());
    }

    fn uploaded(&self) {
        // Made again from the file if a renderer ever needs them again.
        self.0.let_go();
    }
}

/// A picture of an item as the 3D renderer reads it. Kept after the upload,
/// unlike a file's: nothing could make it again but the next picture.
#[derive(Debug)]
pub(crate) struct ItemPixels(pub(crate) ImageHandle);

impl TexturePixels for ItemPixels {
    fn id(&self) -> u64 {
        self.0.id()
    }

    fn size(&self) -> [u32; 2] {
        [self.0.width(), self.0.height()]
    }

    fn encoding(&self) -> PixelEncoding {
        ImagePixels::encoding_of(&self.0)
    }

    fn opaque(&self) -> bool {
        self.0.is_opaque()
    }

    fn with_pixels(&self, f: &mut dyn FnMut(&[u8])) {
        f(&self.0.data_to_upload());
    }
}

/// The texture `obj` as the view `view` draws it now: `None` while its
/// picture is not ready (the view is painted again when it is), or when it
/// has none. Tracked.
pub(crate) fn read(e: &Engine, view: ObjectId, obj: ObjectId) -> Option<ljos_3d::Texture> {
    ljos_3d::qml::texture(&EngineSource::new(e), view, obj)
}

/// The pixels the texture `obj` shows in `view`: its `sourceItem`'s
/// picture, its `textureData` or its loaded `source`. Tracked.
pub(crate) fn image_of(e: &Engine, view: ObjectId, obj: ObjectId) -> Option<TextureImage> {
    if !e.is::<Texture>(obj) {
        return None;
    }
    match item::prop(e, obj, Texture::SOURCE_ITEM) {
        // "If this property is set, the source property is ignored."
        Some(src) => {
            let picture = item::item_picture(e, view, src, item::PictureArea::Bounds)?;
            Some(TextureImage::new(ItemPixels(picture.image)))
        }
        None => {
            // "If this property is set, the source property is ignored."
            if let Some(data) = item::prop(e, obj, Texture::TEXTURE_DATA) {
                return crate::texture_data::image(e, data);
            }
            let source = item::prop(e, obj, Texture::SOURCE);
            // A source a file wrote is relative to that file, as an Image's.
            let url = e.resolve_url(obj, source.0.as_str());
            image(e, view, obj, &url)
        }
    }
}

/// The `sourceItem` of the texture `obj` and the point of it the texture
/// shows at `uv` of a mesh, where a press there lands. Untracked.
pub(crate) fn item_at(e: &Engine, obj: ObjectId, uv: [f32; 2]) -> Option<(ObjectId, kurbo::Point)> {
    e.untrack(|| {
        let src = item::prop(e, obj, Texture::SOURCE_ITEM)?;
        let [r0, r1] = ljos_3d::qml::uv_transform(&EngineSource::new(e), obj).rows();
        let at = |r: [f32; 3], tiling| {
            let t = f64::from(r[0] * uv[0] + r[1] * uv[1] + r[2]);
            // As the sampler reads it: the image the point shows.
            match ljos_3d::qml::tiling_mode(item::prop(e, obj, tiling)) {
                Tiling::ClampToEdge => t.clamp(0.0, 1.0),
                Tiling::Repeat => t.rem_euclid(1.0),
                Tiling::MirroredRepeat => 1.0 - (t.rem_euclid(2.0) - 1.0).abs(),
            }
        };
        let w: f64 = item::prop(e, src, item::Item::WIDTH);
        let h: f64 = item::prop(e, src, item::Item::HEIGHT);
        Some((
            src,
            kurbo::Point::new(
                at(r0, Texture::TILING_MODE_HORIZONTAL) * w,
                at(r1, Texture::TILING_MODE_VERTICAL) * h,
            ),
        ))
    })
}

/// The picture at `url`, read the first time a view asks for it.
fn image(e: &Engine, view: ObjectId, obj: ObjectId, url: &str) -> Option<TextureImage> {
    if url.is_empty() {
        return None;
    }
    let start = e
        .with(obj, |t: &Texture| {
            let mut loaded = t.loaded.borrow_mut();
            if loaded.url != url {
                *loaded = Loaded {
                    url: url.to_owned(),
                    state: State::Idle,
                };
            }
            match &mut loaded.state {
                State::Ready(image) => return Err(Some(image.clone())),
                State::Failed => return Err(None),
                State::Reading(views) => {
                    if !views.contains(&view) {
                        views.push(view);
                    }
                    return Err(None);
                }
                State::Idle => {}
            }
            Ok(())
        })
        .ok()?;
    if let Err(ready) = start {
        return ready;
    }
    // A small local picture is there for the first frame, as an Image's.
    let now = item::texture_now(e, url);
    let state = match now {
        Some(Ok(image)) => State::Ready(TextureImage::new(ImagePixels(image))),
        Some(Err(ref err)) => {
            e.warn(format!("QtQuick3D Texture: {url}: {err}"));
            State::Failed
        }
        None => State::Reading(vec![view]),
    };
    let ready = match &state {
        State::Ready(image) => Some(image.clone()),
        _ => None,
    };
    let reading = matches!(state, State::Reading(_));
    let _ = e.with(obj, |t: &Texture| t.loaded.borrow_mut().state = state);
    if !reading {
        return ready;
    }
    let wanted = url.to_owned();
    item::load_texture(e, url, move |e, result| {
        let failed = result.as_ref().err().map(ToString::to_string);
        let state = match result {
            Ok(image) => State::Ready(TextureImage::new(ImagePixels(image))),
            Err(_) => State::Failed,
        };
        let views = e
            .with(obj, |t: &Texture| {
                let mut loaded = t.loaded.borrow_mut();
                // A reply for a source since replaced is not this one's.
                if loaded.url != wanted {
                    return None;
                }
                match std::mem::replace(&mut loaded.state, state) {
                    State::Reading(views) => Some(views),
                    _ => Some(Vec::new()),
                }
            })
            .ok()
            .flatten();
        let Some(views) = views else {
            return;
        };
        if let Some(err) = failed {
            e.warn(format!("QtQuick3D Texture: {wanted}: {err}"));
        }
        for view in views {
            item::repaint(e, view);
        }
    });
    None
}
