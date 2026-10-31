//! `TextureData`: pixels an application gives a `Texture` as bytes, as
//! `Geometry` gives a model its mesh.
//!
//! The bytes are read into the pixels the renderer uploads once, when
//! they are committed ([`TextureData::update`]); every commit is new
//! pixels under a new id, so the renderer uploads each revision once.

use ljos::item::ljos_3d::TextureImage;
use ljos::item::ljos_3d::texture_data::{self as data, format};
use ljos::item::{self};
use ljos::prelude::*;

use crate::Object3D;

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
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D",
    name = "TextureData",
    extends = Object3D,
    enums = data_enums,
    uncreatable
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

    /// The pixels, rows top first, tightly packed in the format.
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
    e.with(obj, |t: &TextureData| t.image.clone()).ok().flatten()
}
