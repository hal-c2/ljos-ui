//! What `grabImage` returns: the pixels an item is shown with.
//!
//! The picture is the part of the window the item covers, drawn by the
//! reference renderer on a white window. Qt draws the item alone; the two
//! are the same picture wherever the item paints every pixel of its own.

use std::path::PathBuf;

use ljos_engine::{Color, Cx, Engine, EngineError, ObjectId, QmlObject, QtObject, Value, qml_impl};
use ljos_quick::{Item, scene_transform};
use ljos_render::kurbo::Rect;
use ljos_render::{RasterConfig, Rgba, RgbaImage};
use ljos_runtime::Clock as _;

use super::host::Host;
use super::input::number;

fn error(text: impl Into<String>) -> EngineError {
    EngineError::Invoke(text.into())
}

/// A picture of an item
/// (doc.qt.io/qt-6/qml-qttest-testcase.html#grabImage-method).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtTest", name = "GrabbedImage", extends = QtObject, uncreatable)]
pub struct GrabbedImage {
    #[qml(base)]
    pub object: QtObject,
    #[qml(readonly)]
    pub width: i32,
    #[qml(readonly)]
    pub height: i32,
    #[qml(readonly)]
    pub size: Value,
    /// `width * height * 4` bytes, straight alpha.
    #[qml(skip)]
    data: Vec<u8>,
}

impl GrabbedImage {
    /// Takes the picture of `item` as it is shown now.
    pub(crate) fn grab(
        engine: &Engine,
        case: ObjectId,
        item: ObjectId,
    ) -> Result<ObjectId, EngineError> {
        let host = Host::of(engine);
        host.settle(engine)?;
        let window = host.with_rig(|rig| {
            let config = RasterConfig {
                background: Rgba::from_straight(1.0, 1.0, 1.0, 1.0),
                ..RasterConfig::default()
            };
            ljos_drive::draw(engine, &mut rig.scene, rig.clock.now(), config)
        })?;
        let window = window.map_err(|e| error(format!("grabImage: {e}")))?;
        let width = engine.get(item, Item::WIDTH)?;
        let height = engine.get(item, Item::HEIGHT)?;
        let area =
            scene_transform(engine, item).transform_rect_bbox(Rect::new(0.0, 0.0, width, height));
        let (data, w, h) = crop(&window, area);
        let native = GrabbedImage {
            width: w,
            height: h,
            size: Value::size(f64::from(w), f64::from(h)),
            data,
            ..GrabbedImage::default()
        };
        engine.create_native_incomplete(native, Some(case))
    }

    /// The pixel at a place a test named. `None` outside the picture.
    fn at(&self, x: &Value, y: &Value) -> Option<[u8; 4]> {
        let (x, y) = (number(x)?.floor(), number(y)?.floor());
        if x < 0.0 || y < 0.0 || x >= f64::from(self.width) || y >= f64::from(self.height) {
            return None;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let i = (y as usize * self.width as usize + x as usize) * 4;
        self.data.get(i..i + 4)?.try_into().ok()
    }

    fn channel(&self, x: &Value, y: &Value, channel: usize) -> i32 {
        self.at(x, y).map_or(0, |p| i32::from(p[channel]))
    }
}

/// The part of `image` that `area` covers, whole pixels.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn crop(image: &RgbaImage, area: Rect) -> (Vec<u8>, i32, i32) {
    let clamp = |v: f64, max: u32| v.round().clamp(0.0, f64::from(max)) as u32;
    let (x0, x1) = (clamp(area.x0, image.width), clamp(area.x1, image.width));
    let (y0, y1) = (clamp(area.y0, image.height), clamp(area.y1, image.height));
    let (w, h) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
    let mut data = Vec::with_capacity(w as usize * h as usize * 4);
    for y in y0..y1 {
        let from = (y as usize * image.width as usize + x0 as usize) * 4;
        data.extend_from_slice(&image.data[from..from + w as usize * 4]);
    }
    (data, w as i32, h as i32)
}

#[qml_impl]
impl GrabbedImage {
    /// The colour of a pixel; `undefined` outside the picture.
    #[qml(invokable)]
    fn pixel(&self, x: Value, y: Value) -> Value {
        self.at(&x, &y).map_or(Value::Undefined, |p| {
            Value::Color(Color::from_rgba8(p[0], p[1], p[2], p[3]))
        })
    }

    #[qml(invokable)]
    fn red(&self, x: Value, y: Value) -> i32 {
        self.channel(&x, &y, 0)
    }

    #[qml(invokable)]
    fn green(&self, x: Value, y: Value) -> i32 {
        self.channel(&x, &y, 1)
    }

    #[qml(invokable)]
    fn blue(&self, x: Value, y: Value) -> i32 {
        self.channel(&x, &y, 2)
    }

    #[qml(invokable)]
    fn alpha(&self, x: Value, y: Value) -> i32 {
        self.channel(&x, &y, 3)
    }

    /// Whether `other` is a picture of the same size with the same pixels.
    #[qml(invokable)]
    fn equals(cx: &Cx, other: Value) -> Result<bool, EngineError> {
        let Some(other) = other.as_object() else {
            return Ok(false);
        };
        let mine = cx.with::<Self, _>(|s| (s.width, s.height, s.data.clone()))?;
        Ok(cx
            .engine
            .with::<Self, _>(other, |o| {
                (o.width, o.height) == (mine.0, mine.1) && o.data == mine.2
            })
            .unwrap_or(false))
    }

    /// Writes the picture as a PNG. A relative path is below the directory
    /// the tests were started in.
    #[qml(invokable)]
    fn save(cx: &Cx, path: Value) -> Result<(), EngineError> {
        let name = match &path {
            Value::String(s) | Value::Url(s) => s.to_string(),
            _ => return Err(error("save: no path was given")),
        };
        let name = name.strip_prefix("file://").unwrap_or(&name);
        let mut file = PathBuf::from(name);
        if file.is_relative() {
            file = Host::of(cx.engine).state.borrow().save_dir.join(file);
        }
        let image = cx.with::<Self, _>(|s| RgbaImage {
            #[allow(clippy::cast_sign_loss)]
            width: s.width as u32,
            #[allow(clippy::cast_sign_loss)]
            height: s.height as u32,
            data: s.data.clone(),
        })?;
        let png = image.to_png().map_err(|e| error(format!("save: {e}")))?;
        std::fs::write(&file, png).map_err(|e| error(format!("save: {}: {e}", file.display())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: u32, height: u32) -> RgbaImage {
        let mut data = Vec::new();
        for y in 0..height {
            for x in 0..width {
                #[allow(clippy::cast_possible_truncation)]
                data.extend_from_slice(&[x as u8, y as u8, 0, 255]);
            }
        }
        RgbaImage {
            width,
            height,
            data,
        }
    }

    #[test]
    fn a_picture_is_the_part_of_the_window_its_item_covers() {
        let (data, w, h) = crop(&image(10, 8), Rect::new(2.0, 3.0, 5.0, 5.0));
        assert_eq!((w, h), (3, 2));
        assert_eq!(&data[..4], &[2, 3, 0, 255]);
        assert_eq!(&data[data.len() - 4..], &[4, 4, 0, 255]);
    }

    #[test]
    fn what_is_outside_the_window_is_not_in_the_picture() {
        let (data, w, h) = crop(&image(10, 8), Rect::new(-5.0, 6.0, 4.0, 20.0));
        assert_eq!((w, h), (4, 2));
        assert_eq!(data.len(), 4 * 2 * 4);
        let (data, w, h) = crop(&image(10, 8), Rect::new(20.0, 20.0, 30.0, 30.0));
        assert_eq!((w, h), (0, 0));
        assert!(data.is_empty());
    }
}
