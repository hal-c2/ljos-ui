//! `import Fixtures`: native types an application would register, for
//! scenarios about what the engine does with such types. No scenario that
//! does not import the module sees them.

use ljos::prelude::*;
use ljos_engine::{
    Engine, EngineError, LogLevel, ObjectId, QmlObject, ScopedEvent, Version, qml_singleton,
};
use ljos_quick3d::{ComponentType, Geometry, PrimitiveType, Semantic};

/// One for each window (`features/language/singletons-per-window.feature`).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Fixtures", name = "Document")]
#[qml_singleton(scope = "window")]
pub(super) struct Document {
    #[qml(notify)]
    pub title: String,
}

/// One for the engine.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Fixtures", name = "Session")]
#[qml_singleton]
pub(super) struct Session {
    #[qml(notify)]
    pub title: String,
}

/// Setters that do more than store their value, as a WRITE function in Qt
/// may (`features/language/property-setter-writes-others.feature`).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Fixtures", name = "Browser", extends = QtObject)]
pub(super) struct Browser {
    #[qml(base)]
    object: QtObject,
    /// Clears `error` on every write and counts it in `visits`.
    #[qml(notify, write = Self::keep_root)]
    root_path: String,
    #[qml(notify)]
    error: String,
    /// Kept at 3 or below by a setter of its own.
    #[qml(notify, write = Self::keep_visits)]
    visits: i32,
    /// Clears `error` on every write.
    #[qml(notify, write = Self::keep_zoom)]
    zoom: f64,
    /// Tries to write itself, and logs what it is told.
    #[qml(notify, write = Self::keep_depth)]
    depth: i32,
}

impl Browser {
    fn keep_root(cx: &Cx, path: String) -> Option<String> {
        cx.set(Self::ERROR, String::new()).ok()?;
        let visits = cx.with(|b: &Self| b.visits).ok()?;
        cx.set(Self::VISITS, visits + 1).ok()?;
        Some(path)
    }

    fn keep_visits(&self, v: i32) -> Option<i32> {
        Some(v.min(3))
    }

    fn keep_zoom(cx: &Cx, zoom: f64) -> Option<f64> {
        cx.set(Self::ERROR, String::new()).ok()?;
        Some(zoom)
    }

    fn keep_depth(cx: &Cx, depth: i32) -> Option<i32> {
        if let Err(e) = cx.set(Self::DEPTH, depth + 1) {
            cx.engine.log(LogLevel::Warning, "qml", e.to_string());
        }
        Some(depth)
    }
}

/// A geometry an application provides (`features/quick3d/custom-geometry.feature`):
/// a flat disc of `corners` corners and `radius`, around its origin and
/// facing +Z, as a triangle fan with no indices and no normals.
#[derive(Debug, QmlObject)]
#[qml(module = "Fixtures", name = "Fan", extends = Geometry)]
pub(super) struct Fan {
    #[qml(base)]
    geometry: Geometry,
    #[qml(notify, change = Self::rebuild)]
    corners: i32,
    #[qml(notify, change = Self::rebuild)]
    radius: f64,
}

impl Default for Fan {
    fn default() -> Self {
        Fan {
            geometry: Geometry::default(),
            corners: 6,
            radius: 50.0,
        }
    }
}

impl Fan {
    fn rebuild(cx: &Cx, _: PropIndex) {
        let (Ok(corners), Ok(radius)) = (cx.get(Self::CORNERS), cx.get(Self::RADIUS)) else {
            return;
        };
        let rim = (0..corners.max(0)).flat_map(|k| {
            let a = std::f32::consts::TAU * k as f32 / corners as f32;
            let (s, c) = a.sin_cos();
            [radius as f32 * c, radius as f32 * s, 0.0]
        });
        let mut data: Vec<f32> = vec![0.0; 3];
        data.extend(rim);
        // The fan runs round the rim and shuts on its first corner.
        if corners >= 3 {
            data.extend([radius as f32, 0.0, 0.0]);
        }
        let _ = cx.with_mut(|f: &mut Fan| {
            let g = &mut f.geometry;
            g.clear();
            g.set_vertex_data(data.iter().flat_map(|v| v.to_le_bytes()).collect());
            g.set_stride(12);
            g.add_attribute(Semantic::Position, 0, ComponentType::F32);
            g.set_primitive_type(PrimitiveType::TriangleFan);
        });
        Geometry::update(cx.engine, cx.this);
    }
}

/// A `Document` the application made itself, titled `title`.
pub(super) fn document(engine: &Engine, title: &str) -> Result<ObjectId, EngineError> {
    engine.create_native(
        Document {
            title: title.to_owned(),
        },
        None,
    )
}

/// The title of the `Document` `obj`.
pub(super) fn title(engine: &Engine, obj: ObjectId) -> Result<String, EngineError> {
    engine.with::<Document, _>(obj, |d| d.title.clone())
}

pub(super) fn register(engine: &Engine) {
    engine.register_singleton::<Document>("Fixtures", Version::new(1, 0));
    engine.register_singleton::<Session>("Fixtures", Version::new(1, 0));
    engine.register_module(ljos::module!("Fixtures" 1.0 => [Browser, Fan]));
}

/// What a scenario reads in the log: when a window's `Document` is made
/// and when it goes.
pub(super) fn log_documents(engine: &Engine) {
    engine.on_scoped_singleton(|e, s| {
        if s.is::<Document>() {
            let said = match s.event {
                ScopedEvent::Made => "Document made",
                ScopedEvent::Dropped => "Document dropped",
            };
            e.log(LogLevel::Info, "", said);
        }
    });
}
