//! `TextureData`: pixels an application gives a `Texture` as bytes, as
//! `Geometry` gives a model its mesh.
//!
//! The bytes are read into the pixels the renderer uploads once, when
//! they are committed ([`TextureData::update`]); every commit is new
//! pixels under a new id, so the renderer uploads each revision once.

use std::sync::Arc;

use ljos::item::ljos_3d::texture_data::{self as data, format};
use ljos::item::ljos_3d::{PixelEncoding, TextureImage, TexturePixels};
use ljos::item::{self};
use ljos::prelude::*;

use crate::Object3D;
use crate::texture::ImagePixels;

pub(crate) fn data_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "Format",
        format::NAMES
            .iter()
            .enumerate()
            .map(|(i, n)| (*n, i as i64))
            .collect(),
    )]
}

/// `TextureData` (QtQuick3D): what every kind of texture data is. An
/// application sets its data from Rust and calls [`TextureData::update`].
///
/// QML can declare one and call the same functions, which Qt's QML cannot:
/// what stands in for an application's C++ subclass has its data to give.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D",
    name = "TextureData",
    extends = Object3D,
    enums = data_enums
)]
pub struct TextureData {
    #[qml(base)]
    pub object3d: Object3D,
    /// How many times the data was committed: what a texture's drawing
    /// tracks.
    #[qml(notify, readonly)]
    pub revision: i32,
    #[qml(skip)]
    data: data::TextureData,
    #[qml(skip)]
    image: Option<TextureImage>,
    /// Counts `setImageFileData` calls: only the last one's picture is shown.
    #[qml(skip)]
    file_reads: u64,
}

impl Default for TextureData {
    fn default() -> Self {
        TextureData {
            object3d: Object3D::default(),
            revision: 0,
            data: data::TextureData {
                format: format::RGBA8,
                ..data::TextureData::default()
            },
            image: None,
            file_reads: 0,
        }
    }
}

const CATEGORY: &str = "qt.quick3d.texturedata";

impl TextureData {
    pub fn set_size(&mut self, width: u32, height: u32) {
        self.data.width = width;
        self.data.height = height;
    }

    /// More than 0 makes a volume, which is not drawn here.
    pub fn set_depth(&mut self, depth: u32) {
        self.data.depth = depth;
    }

    /// A `TextureData.Format` value; `None` is RGBA8.
    pub fn set_format(&mut self, format: i32) {
        self.data.format = format;
    }

    pub fn set_has_transparency(&mut self, has: bool) {
        self.data.has_transparency = has;
    }

    /// The pixels, tightly packed in the format. The first row is the
    /// bottom of the picture, as `1 - UV0.y` in a Qt shader expects.
    pub fn set_texture_data(&mut self, bytes: Vec<u8>) {
        self.data.bytes = bytes;
    }

    /// Commits what was set to the texture data `obj` (an object whose
    /// base is a `TextureData`): the textures that show it are drawn
    /// again. Data that cannot be drawn is reported and draws nothing.
    pub fn update(e: &Engine, obj: ObjectId) {
        let read = e.with(obj, |t: &TextureData| t.data.image());
        match read {
            Ok(Ok(image)) => commit(e, obj, image),
            Ok(Err(err)) => {
                e.log(LogLevel::Warning, CATEGORY, format!("TextureData: {err}"));
                commit(e, obj, None);
            }
            Err(err) => e.warn(format!("QtQuick3D TextureData: {err}")),
        }
    }
}

#[qml_impl]
impl TextureData {
    #[qml(invokable, name = "setSize")]
    fn set_size_in_qml(cx: &Cx, size: Value) {
        let Value::Size(size) = size else {
            return cx
                .engine
                .warn("QtQuick3D TextureData: setSize takes a size".to_owned());
        };
        let (w, h) = (size.width.max(0.0) as u32, size.height.max(0.0) as u32);
        let _ = cx
            .engine
            .with_mut(cx.this, |t: &mut TextureData| t.set_size(w, h));
    }

    #[qml(invokable, name = "setDepth")]
    fn set_depth_in_qml(cx: &Cx, depth: i32) {
        let depth = depth.max(0) as u32;
        let _ = cx
            .engine
            .with_mut(cx.this, |t: &mut TextureData| t.set_depth(depth));
    }

    #[qml(invokable, name = "setFormat")]
    fn set_format_in_qml(cx: &Cx, format: i32) {
        let _ = cx
            .engine
            .with_mut(cx.this, |t: &mut TextureData| t.set_format(format));
    }

    #[qml(invokable, name = "setHasTransparency")]
    fn set_has_transparency_in_qml(cx: &Cx, has: bool) {
        let _ = cx
            .engine
            .with_mut(cx.this, |t: &mut TextureData| t.set_has_transparency(has));
    }

    #[qml(invokable, name = "setTextureData")]
    fn set_texture_data_in_qml(cx: &Cx, data: Value) {
        let bytes = cx.engine.js_bytes(&data).unwrap_or_default();
        let _ = cx
            .engine
            .with_mut(cx.this, |t: &mut TextureData| t.set_texture_data(bytes));
    }

    #[qml(invokable, name = "update")]
    fn update_in_qml(cx: &Cx) {
        TextureData::update(cx.engine, cx.this);
    }

    /// The bytes of an image file: its size, format and pixels replace what
    /// was set when they have been read, off the UI thread, as a screenful of
    /// pictures read here would hold up every frame. A Ljós addition: in Qt,
    /// an application's C++ reads a file with QImage and calls the setters,
    /// so the file's first row counts as the data's first.
    #[qml(invokable, name = "setImageFileData")]
    fn set_image_file_data(cx: &Cx, data: Value) {
        let (e, me) = (cx.engine, cx.this);
        let Ok(read) = e.with_mut(me, |t: &mut TextureData| {
            t.file_reads += 1;
            t.file_reads
        }) else {
            return;
        };
        let bytes: Arc<[u8]> = e.js_bytes(&data).unwrap_or_default().into();
        item::decode_texture(e, bytes, move |e, got| {
            // Gone, or given another file since: none of this is wanted.
            if e.with(me, |t: &TextureData| t.file_reads).ok() != Some(read) {
                return;
            }
            match got {
                Ok(image) => commit(e, me, Some(TextureImage::new(FileRows(ImagePixels(image))))),
                Err(err) => {
                    e.log(LogLevel::Warning, CATEGORY, format!("TextureData: {err}"));
                    commit(e, me, None);
                }
            }
        });
    }
}

/// A decoded file as data: its first row is the bottom of the picture.
#[derive(Debug)]
struct FileRows(ImagePixels);

impl TexturePixels for FileRows {
    fn id(&self) -> u64 {
        self.0.id()
    }

    fn size(&self) -> [u32; 2] {
        self.0.size()
    }

    fn encoding(&self) -> PixelEncoding {
        self.0.encoding()
    }

    fn opaque(&self) -> bool {
        self.0.opaque()
    }

    fn with_pixels(&self, f: &mut dyn FnMut(&[u8])) {
        let row = self.size()[0] as usize * 4;
        self.0.with_pixels(&mut |pixels| {
            // Nothing once let go, and a zero-width chunk would panic.
            if row == 0 || pixels.is_empty() {
                return f(pixels);
            }
            let flipped: Vec<u8> = pixels.rchunks_exact(row).flatten().copied().collect();
            f(&flipped);
        });
    }

    fn uploaded(&self) {
        self.0.uploaded();
    }
}

/// Shows `image` from now on, as a commit does.
pub(crate) fn commit(e: &Engine, obj: ObjectId, image: Option<TextureImage>) {
    let _ = e.with_mut(obj, |t: &mut TextureData| {
        t.image = image;
        t.revision = t.revision.wrapping_add(1);
    });
    if let Err(err) = e.notify(obj, TextureData::REVISION.index) {
        e.warn(format!("QtQuick3D TextureData: {err}"));
    }
}

/// The pixels the texture data `obj` was last committed as. Tracked.
pub(crate) fn image(e: &Engine, obj: ObjectId) -> Option<TextureImage> {
    if !e.is::<TextureData>(obj) {
        return None;
    }
    item::prop(e, obj, TextureData::REVISION);
    e.with(obj, |t: &TextureData| t.image.clone())
        .ok()
        .flatten()
}
