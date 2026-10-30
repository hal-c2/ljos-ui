//! `HeightFieldGeometry`: a terrain from a grey-scale picture.

use ljos::item::{self, ImageHandle};
use ljos::prelude::*;

use super::geometry::{ERROR, Generated, NULL, READY, changed, complete, rebuild, status_enums};
use super::mesh::Built;
use super::shapes;
use crate::node::Geometry;

const LOADING: i32 = 1;

/// The mesh of the picture at `source`: a vertex per pixel, as high as the
/// pixel is light. It is `extents.x` wide and `extents.z` deep, centred, and
/// `extents.y` tall.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D.Helpers",
    name = "HeightFieldGeometry",
    extends = Geometry,
    enums = status_enums
)]
pub struct HeightFieldGeometry {
    #[qml(base)]
    geometry: Geometry,
    #[qml(skip)]
    complete: bool,
    #[qml(notify, readonly, enumeration = "Status")]
    status: i32,
    #[qml(notify, change = changed::<HeightFieldGeometry>)]
    extents: Vector3d,
    #[qml(notify, change = changed::<HeightFieldGeometry>)]
    smooth_shading: bool,
    #[qml(notify, change = source_changed)]
    source: Url,
    /// What the picture gave: its size and a height from 0 to 1 per pixel.
    #[qml(skip)]
    field: Field,
}

#[derive(Debug, Default)]
struct Field {
    cols: u32,
    rows: u32,
    heights: Vec<f32>,
}

impl Default for HeightFieldGeometry {
    fn default() -> Self {
        HeightFieldGeometry {
            geometry: Geometry::default(),
            complete: false,
            status: NULL,
            extents: Vector3d {
                x: 100.0,
                y: 100.0,
                z: 100.0,
            },
            smooth_shading: true,
            source: Url::default(),
            field: Field::default(),
        }
    }
}

impl Generated for HeightFieldGeometry {
    const STATUS: Option<Prop<i32>> = Some(HeightFieldGeometry::STATUS);
    fn geometry(&mut self) -> &mut Geometry {
        &mut self.geometry
    }
    fn complete(&mut self) -> &mut bool {
        &mut self.complete
    }
    fn built(cx: &Cx) -> Option<Built> {
        let e: Vector3d = cx.get(HeightFieldGeometry::EXTENTS).ok()?;
        let smooth = cx.get(HeightFieldGeometry::SMOOTH_SHADING).ok()?;
        cx.with(|t: &HeightFieldGeometry| {
            let f = &t.field;
            shapes::height_field(&f.heights, f.cols, f.rows, [e.x, e.y, e.z], smooth)
        })
        .ok()
    }
}

#[qml_impl]
impl HeightFieldGeometry {
    #[qml(on_complete)]
    fn completed(cx: &Cx) {
        complete::<HeightFieldGeometry>(cx);
        load(cx.engine, cx.this);
    }
}

fn source_changed(cx: &Cx, _: PropIndex) {
    if cx
        .with(|t: &HeightFieldGeometry| t.complete)
        .unwrap_or(false)
    {
        load(cx.engine, cx.this);
    }
}

/// Reads the picture at `source`: at once when it is there to be, else
/// when it has been read, and the mesh is made again then.
fn load(e: &Engine, obj: ObjectId) {
    let source: Url = item::prop(e, obj, HeightFieldGeometry::SOURCE);
    if source.0.is_empty() {
        take(e, obj, Field::default(), NULL);
        return;
    }
    let url = e.resolve_url(obj, source.0.as_str());
    match item::texture_now(e, &url) {
        Some(Ok(image)) => take(e, obj, field_of(&image), READY),
        Some(Err(err)) => failed(e, obj, &url, &err.to_string()),
        None => {
            let _ = e.set(obj, HeightFieldGeometry::STATUS, LOADING);
            let wanted = url.clone();
            item::load_texture(e, &url, move |e, result| {
                // A reply for a source since replaced is not this one's.
                let now: Url = item::prop(e, obj, HeightFieldGeometry::SOURCE);
                if e.resolve_url(obj, now.0.as_str()) != wanted {
                    return;
                }
                match result {
                    Ok(image) => take(e, obj, field_of(&image), READY),
                    Err(err) => failed(e, obj, &wanted, &err.to_string()),
                }
            });
        }
    }
}

fn failed(e: &Engine, obj: ObjectId, url: &str, why: &str) {
    e.warn(format!("HeightFieldGeometry: {url}: {why}"));
    take(e, obj, Field::default(), ERROR);
}

fn take(e: &Engine, obj: ObjectId, field: Field, status: i32) {
    let _ = e.with_mut(obj, |t: &mut HeightFieldGeometry| t.field = field);
    let cx = Cx::new(e, obj);
    rebuild::<HeightFieldGeometry>(&cx);
    // `rebuild` says Ready, or Error for an empty mesh; a missing or bad
    // picture is told apart from a mesh that is empty for its size.
    if status != READY {
        let _ = cx.set(HeightFieldGeometry::STATUS, status);
    }
}

/// The lightness of each pixel, row by row.
fn field_of(image: &ImageHandle) -> Field {
    let (cols, rows) = (image.width(), image.height());
    let mut heights = Vec::with_capacity((cols * rows) as usize);
    for y in 0..rows {
        for x in 0..cols {
            let [r, g, b, _] = image.pixel(x, y);
            heights.push((f32::from(r) + f32::from(g) + f32::from(b)) / (3.0 * 255.0));
        }
    }
    Field {
        cols,
        rows,
        heights,
    }
}
