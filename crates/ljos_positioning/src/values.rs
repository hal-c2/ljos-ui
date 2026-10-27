//! The value types of `QtPositioning`: `geoCoordinate`, `geoRectangle`,
//! `geoCircle`, `geoPath`, `geoPolygon` and `geoShape`.
//!
//! Each is a Rust struct (what native code reads and writes, and a field
//! type of a native object) and a [`CustomType`] (what QML and scripts
//! see). The members are listed in the order qml6 lists their keys.

use ljos::ljos_engine::{ConvertError, CustomMember, CustomType, CustomValue, QmlValue, TypeTag};
use ljos::prelude::*;

use crate::geo::{self, GeoCoordinate};

/// `type` of a shape: `QGeoShape::ShapeType`.
mod shape_type {
    pub const UNKNOWN: i32 = 0;
    pub const RECTANGLE: i32 = 1;
    pub const CIRCLE: i32 = 2;
    pub const PATH: i32 = 3;
    pub const POLYGON: i32 = 4;
}

/// A Rust value of one of the types: how it is read from and made into a
/// [`CustomValue`].
pub trait GeoValue: Sized + Default {
    fn ty() -> &'static dyn CustomType;
    fn from_custom(v: &CustomValue) -> Self;
    fn to_custom(&self) -> CustomValue;

    /// `v`, when it is one of this type or something its properties take
    /// as one (a shape holding it).
    fn from_any(v: &Value) -> Option<Self> {
        let v = match v {
            Value::Custom(c) if c.is(Self::ty()) => v.clone(),
            _ => Self::ty().coerce(v)?,
        };
        match &v {
            Value::Custom(c) => Some(Self::from_custom(c)),
            _ => None,
        }
    }

    fn value(&self) -> Value {
        self.to_custom().into()
    }
}

macro_rules! qml_value {
    ($t:ty) => {
        impl QmlValue for $t {
            fn type_tag() -> TypeTag {
                TypeTag::Custom(Atom::intern(<$t as GeoValue>::ty().name()))
            }
            fn to_value(&self) -> Value {
                self.value()
            }
            fn from_value(v: &Value) -> Result<Self, ConvertError> {
                match v {
                    Value::Undefined => Ok(Self::default()),
                    _ => <$t as GeoValue>::from_any(v).ok_or_else(|| {
                        ConvertError::new(format!("{v:?}"), <$t as GeoValue>::ty().name())
                    }),
                }
            }
        }
    };
}

/// The number in `v`, or `or` when it is left undefined.
pub(crate) fn number_or(v: &Value, or: f64) -> f64 {
    match v {
        Value::Undefined => or,
        v => v.to_number(),
    }
}

fn real(v: &Value) -> f64 {
    number_or(v, f64::NAN)
}

/// A coordinate in a field or an argument; nowhere when it is not one.
pub fn coordinate_of(v: &Value) -> GeoCoordinate {
    GeoCoordinate::from_any(v).unwrap_or_default()
}

/// The coordinates of a list.
pub fn coordinates_of(v: &Value) -> Vec<GeoCoordinate> {
    v.as_list()
        .map(|items| items.iter().map(coordinate_of).collect())
        .unwrap_or_default()
}

fn coordinates_value(cs: &[GeoCoordinate]) -> Value {
    Value::list(cs.iter().map(GeoValue::value).collect())
}

fn arg(args: &[Value], i: usize) -> &Value {
    const NONE: &Value = &Value::Undefined;
    args.get(i).unwrap_or(NONE)
}

/// What a method left out is given.
fn arg_or(args: &[Value], i: usize, or: f64) -> f64 {
    number_or(arg(args, i), or)
}

// ------------------------------------------------------------ geoCoordinate

#[derive(Debug)]
pub struct CoordinateType;
pub static COORDINATE: CoordinateType = CoordinateType;

impl GeoValue for GeoCoordinate {
    fn ty() -> &'static dyn CustomType {
        &COORDINATE
    }
    fn from_custom(v: &CustomValue) -> Self {
        GeoCoordinate::new(real(v.field(0)), real(v.field(1)), real(v.field(2)))
    }
    fn to_custom(&self) -> CustomValue {
        CustomValue::new(
            &COORDINATE,
            vec![
                Value::Real(self.latitude),
                Value::Real(self.longitude),
                Value::Real(self.altitude),
            ],
        )
    }
}
qml_value!(GeoCoordinate);

impl CustomType for CoordinateType {
    fn name(&self) -> &'static str {
        "geoCoordinate"
    }
    fn module(&self) -> &'static str {
        "QtPositioning"
    }
    fn members(&self) -> &'static [CustomMember] {
        const M: &[CustomMember] = &[
            CustomMember::stored("latitude", "real"),
            CustomMember::stored("longitude", "real"),
            CustomMember::stored("altitude", "real"),
            CustomMember::computed("isValid", "bool"),
        ];
        M
    }
    fn methods(&self) -> &'static [&'static str] {
        &["distanceTo", "azimuthTo", "atDistanceAndAzimuth"]
    }
    fn default_fields(&self) -> Vec<Value> {
        vec![Value::Real(f64::NAN); 3]
    }
    fn computed(&self, v: &CustomValue, name: &str) -> Value {
        match name {
            "isValid" => Value::Bool(GeoCoordinate::from_custom(v).is_valid()),
            _ => Value::Undefined,
        }
    }
    fn text(&self, v: &CustomValue) -> String {
        GeoCoordinate::from_custom(v).text()
    }
    fn call(&self, v: &CustomValue, method: &str, args: &[Value]) -> Result<Value, String> {
        let c = GeoCoordinate::from_custom(v);
        Ok(match method {
            "distanceTo" => Value::Real(c.distance_to(&coordinate_of(arg(args, 0)))),
            "azimuthTo" => Value::Real(c.azimuth_to(&coordinate_of(arg(args, 0)))),
            "atDistanceAndAzimuth" => c
                .at_distance_and_azimuth(arg_or(args, 0, 0.0), arg_or(args, 1, 0.0))
                .value(),
            _ => return Err(format!("{method} is not a function")),
        })
    }
}

// ------------------------------------------------------------- geoRectangle

/// From its top left corner to its bottom right; it may cross the date
/// line, when the left is east of the right.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GeoRectangle {
    pub top_left: GeoCoordinate,
    pub bottom_right: GeoCoordinate,
}

impl GeoRectangle {
    pub fn new(top_left: GeoCoordinate, bottom_right: GeoCoordinate) -> Self {
        Self {
            top_left,
            bottom_right,
        }
    }

    /// `width` by `height` degrees around `center`.
    pub fn around(center: GeoCoordinate, width: f64, height: f64) -> Self {
        let mut r = Self::new(center, center);
        r.top_left.altitude = f64::NAN;
        r.bottom_right.altitude = f64::NAN;
        r.set_size(width, height);
        r
    }

    /// The smallest that holds all of `cs`.
    pub fn bounding(cs: &[GeoCoordinate]) -> Self {
        let valid: Vec<&GeoCoordinate> = cs.iter().filter(|c| c.is_valid()).collect();
        if valid.is_empty() {
            return Self::default();
        }
        let fold = |f: fn(f64, f64) -> f64, of: fn(&GeoCoordinate) -> f64, start: f64| {
            valid.iter().map(|c| of(c)).fold(start, f)
        };
        let north = fold(f64::max, |c| c.latitude, -90.0);
        let south = fold(f64::min, |c| c.latitude, 90.0);
        let west = fold(f64::min, |c| c.longitude, 180.0);
        let east = fold(f64::max, |c| c.longitude, -180.0);
        Self::new(
            GeoCoordinate::new(north, west, f64::NAN),
            GeoCoordinate::new(south, east, f64::NAN),
        )
    }

    pub fn is_valid(&self) -> bool {
        self.top_left.is_valid()
            && self.bottom_right.is_valid()
            && self.top_left.latitude >= self.bottom_right.latitude
    }

    pub fn is_empty(&self) -> bool {
        !self.is_valid() || self.width() == 0.0 || self.height() == 0.0
    }

    /// Degrees from west to east, across the date line if need be.
    pub fn width(&self) -> f64 {
        if !self.is_valid() {
            return f64::NAN;
        }
        let w = self.bottom_right.longitude - self.top_left.longitude;
        if w < 0.0 { w + 360.0 } else { w }
    }

    pub fn height(&self) -> f64 {
        if !self.is_valid() {
            return f64::NAN;
        }
        self.top_left.latitude - self.bottom_right.latitude
    }

    pub fn center(&self) -> GeoCoordinate {
        if !self.is_valid() {
            return GeoCoordinate::default();
        }
        GeoCoordinate::new(
            (self.top_left.latitude + self.bottom_right.latitude) / 2.0,
            geo::wrap_longitude(self.top_left.longitude + self.width() / 2.0),
            f64::NAN,
        )
    }

    pub fn bottom_left(&self) -> GeoCoordinate {
        GeoCoordinate::new(
            self.bottom_right.latitude,
            self.top_left.longitude,
            f64::NAN,
        )
    }

    pub fn top_right(&self) -> GeoCoordinate {
        GeoCoordinate::new(
            self.top_left.latitude,
            self.bottom_right.longitude,
            f64::NAN,
        )
    }

    /// The edges are inside.
    pub fn contains(&self, c: &GeoCoordinate) -> bool {
        if !self.is_valid() || !c.is_valid() {
            return false;
        }
        let (west, east) = (self.top_left.longitude, self.bottom_right.longitude);
        let across = if west <= east {
            (west..=east).contains(&c.longitude)
        } else {
            c.longitude >= west || c.longitude <= east
        };
        across && (self.bottom_right.latitude..=self.top_left.latitude).contains(&c.latitude)
    }

    /// Moved so its centre is `center`; its size stays.
    pub fn set_center(&mut self, center: GeoCoordinate) {
        let (w, h) = (self.width(), self.height());
        self.top_left = center;
        self.bottom_right = center;
        self.top_left.altitude = f64::NAN;
        self.bottom_right.altitude = f64::NAN;
        self.set_size(w, h);
    }

    /// `width` by `height` degrees, around the centre it has.
    fn set_size(&mut self, width: f64, height: f64) {
        let c = if self.top_left == self.bottom_right {
            self.top_left
        } else {
            self.center()
        };
        let north = (c.latitude + height / 2.0).min(90.0);
        let south = (c.latitude - height / 2.0).max(-90.0);
        self.top_left = GeoCoordinate::new(
            north,
            geo::wrap_longitude(c.longitude - width / 2.0),
            f64::NAN,
        );
        self.bottom_right = GeoCoordinate::new(
            south,
            geo::wrap_longitude(c.longitude + width / 2.0),
            f64::NAN,
        );
    }

    pub fn translated(&self, dlat: f64, dlon: f64) -> Self {
        let mut r = *self;
        let center = self.center();
        r.set_center(GeoCoordinate::new(
            center.latitude + dlat,
            geo::wrap_longitude(center.longitude + dlon),
            f64::NAN,
        ));
        r
    }

    /// The smallest that holds this and `other`.
    pub fn united(&self, other: &GeoRectangle) -> Self {
        match (self.is_valid(), other.is_valid()) {
            (false, _) => *other,
            (_, false) => *self,
            _ => Self::bounding(&[
                self.top_left,
                self.bottom_right,
                other.top_left,
                other.bottom_right,
            ]),
        }
    }

    fn text(&self) -> String {
        format!(
            "QGeoRectangle({{{}, {}}}, {{{}, {}}})",
            geo::g(self.top_left.latitude),
            geo::g(self.top_left.longitude),
            geo::g(self.bottom_right.latitude),
            geo::g(self.bottom_right.longitude)
        )
    }
}

#[derive(Debug)]
pub struct RectangleType;
pub static RECTANGLE: RectangleType = RectangleType;

impl GeoValue for GeoRectangle {
    fn ty() -> &'static dyn CustomType {
        &RECTANGLE
    }
    fn from_custom(v: &CustomValue) -> Self {
        Self::new(coordinate_of(v.field(1)), coordinate_of(v.field(0)))
    }
    fn to_custom(&self) -> CustomValue {
        CustomValue::new(
            &RECTANGLE,
            vec![self.bottom_right.value(), self.top_left.value()],
        )
    }
}
qml_value!(GeoRectangle);

/// What every shape has.
const SHAPE_MEMBERS: [CustomMember; 4] = [
    CustomMember::computed("type", "int"),
    CustomMember::computed("isValid", "bool"),
    CustomMember::computed("isEmpty", "bool"),
    CustomMember::computed("center", "geoCoordinate"),
];

impl CustomType for RectangleType {
    fn name(&self) -> &'static str {
        "geoRectangle"
    }
    fn module(&self) -> &'static str {
        "QtPositioning"
    }
    fn members(&self) -> &'static [CustomMember] {
        const M: &[CustomMember] = &[
            SHAPE_MEMBERS[0],
            SHAPE_MEMBERS[1],
            SHAPE_MEMBERS[2],
            SHAPE_MEMBERS[3],
            CustomMember::computed("bottomLeft", "geoCoordinate"),
            CustomMember::stored("bottomRight", "geoCoordinate"),
            CustomMember::stored("topLeft", "geoCoordinate"),
            CustomMember::computed("topRight", "geoCoordinate"),
            CustomMember::computed("height", "real"),
            CustomMember::computed("width", "real"),
        ];
        M
    }
    fn methods(&self) -> &'static [&'static str] {
        &["contains", "boundingGeoRectangle", "translated", "united"]
    }
    fn default_fields(&self) -> Vec<Value> {
        GeoRectangle::default().to_custom().fields().to_vec()
    }
    fn computed(&self, v: &CustomValue, name: &str) -> Value {
        let r = GeoRectangle::from_custom(v);
        match name {
            "bottomLeft" => r.bottom_left().value(),
            "topRight" => r.top_right().value(),
            "height" => Value::Real(r.height()),
            "width" => Value::Real(r.width()),
            _ => Shape::Rectangle(r).member(name),
        }
    }
    fn text(&self, v: &CustomValue) -> String {
        GeoRectangle::from_custom(v).text()
    }
    fn with(&self, v: &CustomValue, name: &str, to: &Value) -> Option<CustomValue> {
        let mut r = GeoRectangle::from_custom(v);
        match name {
            "topLeft" => r.top_left = coordinate_of(to),
            "bottomRight" => r.bottom_right = coordinate_of(to),
            "bottomLeft" => {
                let c = coordinate_of(to);
                r.bottom_right.latitude = c.latitude;
                r.top_left.longitude = c.longitude;
            }
            "topRight" => {
                let c = coordinate_of(to);
                r.top_left.latitude = c.latitude;
                r.bottom_right.longitude = c.longitude;
            }
            "center" => r.set_center(coordinate_of(to)),
            "width" => r.set_size(to.to_number(), r.height()),
            "height" => r.set_size(r.width(), to.to_number()),
            _ => return None,
        }
        Some(r.to_custom())
    }
    fn call(&self, v: &CustomValue, method: &str, args: &[Value]) -> Result<Value, String> {
        let r = GeoRectangle::from_custom(v);
        match method {
            "translated" => Ok(r
                .translated(arg_or(args, 0, 0.0), arg_or(args, 1, 0.0))
                .value()),
            "united" => Ok(r
                .united(&GeoRectangle::from_any(arg(args, 0)).unwrap_or_default())
                .value()),
            _ => Shape::Rectangle(r).call(method, args),
        }
    }
    fn coerce(&self, v: &Value) -> Option<Value> {
        match Shape::of(v)? {
            Shape::Rectangle(r) => Some(r.value()),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------- geoCircle

/// The coordinates within `radius` metres of `center`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeoCircle {
    pub center: GeoCoordinate,
    pub radius: f64,
}

impl Default for GeoCircle {
    fn default() -> Self {
        Self {
            center: GeoCoordinate::default(),
            radius: -1.0,
        }
    }
}

impl GeoCircle {
    pub fn is_valid(&self) -> bool {
        self.center.is_valid() && self.radius >= 0.0
    }

    pub fn is_empty(&self) -> bool {
        !self.is_valid() || self.radius == 0.0
    }

    pub fn contains(&self, c: &GeoCoordinate) -> bool {
        self.is_valid() && c.is_valid() && self.center.distance_to(c) <= self.radius
    }

    pub fn bounding(&self) -> GeoRectangle {
        if !self.is_valid() {
            return GeoRectangle::default();
        }
        let dlat = (self.radius / geo::EARTH_RADIUS).to_degrees();
        let north = self.center.latitude + dlat;
        let south = self.center.latitude - dlat;
        if north >= 90.0 || south <= -90.0 {
            return GeoRectangle::new(
                GeoCoordinate::new(north.min(90.0), -180.0, f64::NAN),
                GeoCoordinate::new(south.max(-90.0), 180.0, f64::NAN),
            );
        }
        let dlon = dlat / self.center.latitude.to_radians().cos();
        if dlon >= 180.0 {
            return GeoRectangle::new(
                GeoCoordinate::new(north, -180.0, f64::NAN),
                GeoCoordinate::new(south, 180.0, f64::NAN),
            );
        }
        GeoRectangle::new(
            GeoCoordinate::new(
                north,
                geo::wrap_longitude(self.center.longitude - dlon),
                f64::NAN,
            ),
            GeoCoordinate::new(
                south,
                geo::wrap_longitude(self.center.longitude + dlon),
                f64::NAN,
            ),
        )
    }

    fn text(&self) -> String {
        format!(
            "QGeoCircle({{{}, {}}}, {})",
            geo::g(self.center.latitude),
            geo::g(self.center.longitude),
            geo::g(self.radius)
        )
    }
}

#[derive(Debug)]
pub struct CircleType;
pub static CIRCLE: CircleType = CircleType;

impl GeoValue for GeoCircle {
    fn ty() -> &'static dyn CustomType {
        &CIRCLE
    }
    fn from_custom(v: &CustomValue) -> Self {
        Self {
            center: coordinate_of(v.field(0)),
            radius: real(v.field(1)),
        }
    }
    fn to_custom(&self) -> CustomValue {
        CustomValue::new(&CIRCLE, vec![self.center.value(), Value::Real(self.radius)])
    }
}
qml_value!(GeoCircle);

impl CustomType for CircleType {
    fn name(&self) -> &'static str {
        "geoCircle"
    }
    fn module(&self) -> &'static str {
        "QtPositioning"
    }
    fn members(&self) -> &'static [CustomMember] {
        const M: &[CustomMember] = &[
            SHAPE_MEMBERS[0],
            SHAPE_MEMBERS[1],
            SHAPE_MEMBERS[2],
            CustomMember::stored("center", "geoCoordinate"),
            CustomMember::stored("radius", "real"),
        ];
        M
    }
    fn methods(&self) -> &'static [&'static str] {
        &["contains", "boundingGeoRectangle", "translated"]
    }
    fn default_fields(&self) -> Vec<Value> {
        GeoCircle::default().to_custom().fields().to_vec()
    }
    fn computed(&self, v: &CustomValue, name: &str) -> Value {
        Shape::Circle(GeoCircle::from_custom(v)).member(name)
    }
    fn text(&self, v: &CustomValue) -> String {
        GeoCircle::from_custom(v).text()
    }
    fn call(&self, v: &CustomValue, method: &str, args: &[Value]) -> Result<Value, String> {
        let c = GeoCircle::from_custom(v);
        match method {
            "translated" => {
                let mut moved = c;
                moved.center.latitude += arg_or(args, 0, 0.0);
                moved.center.longitude =
                    geo::wrap_longitude(moved.center.longitude + arg_or(args, 1, 0.0));
                Ok(moved.value())
            }
            _ => Shape::Circle(c).call(method, args),
        }
    }
    fn coerce(&self, v: &Value) -> Option<Value> {
        match Shape::of(v)? {
            Shape::Circle(c) => Some(c.value()),
            _ => None,
        }
    }
}

// ------------------------------------------------------------------ geoPath

/// Coordinates one after another, `width` metres wide.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GeoPath {
    pub path: Vec<GeoCoordinate>,
    pub width: f64,
}

/// Metres along `cs` from the `from`th to the `to`th; `to` past the last
/// (or negative) goes on round to the first, as qml6's `length()` does.
fn length_of(cs: &[GeoCoordinate], from: f64, to: f64) -> f64 {
    let n = cs.len();
    if n < 2 {
        return 0.0;
    }
    let from = (from.max(0.0) as usize).min(n);
    let to = if to < 0.0 || to as usize >= n {
        n
    } else {
        to as usize
    };
    (from..to)
        .map(|i| cs[i].distance_to(&cs[(i + 1) % n]))
        .sum()
}

impl GeoPath {
    pub fn is_valid(&self) -> bool {
        !self.path.is_empty()
    }

    /// Near the path: within half its width of a segment.
    pub fn contains(&self, c: &GeoCoordinate) -> bool {
        if !c.is_valid() {
            return false;
        }
        let reach = self.width / 2.0;
        match self.path.as_slice() {
            [] => false,
            [only] => only.distance_to(c) <= reach,
            cs => cs
                .windows(2)
                .any(|w| geo::distance_to_segment(c, &w[0], &w[1]) <= reach.max(1e-6)),
        }
    }

    fn text(&self) -> String {
        let items: Vec<String> = self.path.iter().map(|c| format!("{},", c.text())).collect();
        format!("QGeoPath([ {} ])", items.join(" "))
    }
}

#[derive(Debug)]
pub struct PathType;
pub static PATH: PathType = PathType;

impl GeoValue for GeoPath {
    fn ty() -> &'static dyn CustomType {
        &PATH
    }
    fn from_custom(v: &CustomValue) -> Self {
        Self {
            path: coordinates_of(v.field(0)),
            width: number_or(v.field(1), 0.0),
        }
    }
    fn to_custom(&self) -> CustomValue {
        CustomValue::new(
            &PATH,
            vec![coordinates_value(&self.path), Value::Real(self.width)],
        )
    }
}
qml_value!(GeoPath);

/// What a path and a polygon both have: their coordinates by index.
fn coordinate_methods(cs: &[GeoCoordinate], method: &str, args: &[Value]) -> Option<Value> {
    Some(match method {
        "size" => Value::Int(cs.len() as i32),
        "coordinateAt" => {
            let i = arg_or(args, 0, -1.0);
            match i >= 0.0 && (i as usize) < cs.len() {
                true => cs[i as usize].value(),
                false => GeoCoordinate::default().value(),
            }
        }
        "containsCoordinate" => {
            let c = coordinate_of(arg(args, 0));
            Value::Bool(cs.contains(&c))
        }
        "length" => Value::Real(length_of(cs, arg_or(args, 0, 0.0), arg_or(args, 1, -1.0))),
        _ => return None,
    })
}

impl CustomType for PathType {
    fn name(&self) -> &'static str {
        "geoPath"
    }
    fn module(&self) -> &'static str {
        "QtPositioning"
    }
    fn members(&self) -> &'static [CustomMember] {
        const M: &[CustomMember] = &[
            SHAPE_MEMBERS[0],
            SHAPE_MEMBERS[1],
            SHAPE_MEMBERS[2],
            SHAPE_MEMBERS[3],
            CustomMember::stored("path", "list"),
            CustomMember::stored("width", "real"),
        ];
        M
    }
    fn methods(&self) -> &'static [&'static str] {
        &[
            "contains",
            "boundingGeoRectangle",
            "length",
            "size",
            "coordinateAt",
            "containsCoordinate",
        ]
    }
    fn default_fields(&self) -> Vec<Value> {
        GeoPath::default().to_custom().fields().to_vec()
    }
    fn computed(&self, v: &CustomValue, name: &str) -> Value {
        Shape::Path(GeoPath::from_custom(v)).member(name)
    }
    fn text(&self, v: &CustomValue) -> String {
        GeoPath::from_custom(v).text()
    }
    fn with(&self, v: &CustomValue, name: &str, to: &Value) -> Option<CustomValue> {
        let mut p = GeoPath::from_custom(v);
        match name {
            "path" => p.path = coordinates_of(to),
            "width" => p.width = to.to_number(),
            _ => return None,
        }
        Some(p.to_custom())
    }
    fn call(&self, v: &CustomValue, method: &str, args: &[Value]) -> Result<Value, String> {
        let p = GeoPath::from_custom(v);
        match coordinate_methods(&p.path, method, args) {
            Some(out) => Ok(out),
            None => Shape::Path(p).call(method, args),
        }
    }
    fn coerce(&self, v: &Value) -> Option<Value> {
        match Shape::of(v)? {
            Shape::Path(p) => Some(p.value()),
            _ => None,
        }
    }
}

// --------------------------------------------------------------- geoPolygon

/// The area a closed path goes round.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GeoPolygon {
    pub perimeter: Vec<GeoCoordinate>,
}

impl GeoPolygon {
    /// Three corners at least.
    pub fn is_valid(&self) -> bool {
        self.perimeter.len() >= 3
    }

    /// Inside by the even-odd rule, on the plane of latitudes and
    /// longitudes.
    pub fn contains(&self, c: &GeoCoordinate) -> bool {
        if !self.is_valid() || !c.is_valid() {
            return false;
        }
        let ps = &self.perimeter;
        let mut inside = false;
        let mut j = ps.len() - 1;
        for i in 0..ps.len() {
            let (a, b) = (&ps[i], &ps[j]);
            if (a.latitude > c.latitude) != (b.latitude > c.latitude) {
                let lon = (b.longitude - a.longitude) * (c.latitude - a.latitude)
                    / (b.latitude - a.latitude)
                    + a.longitude;
                if c.longitude < lon {
                    inside = !inside;
                }
            }
            j = i;
        }
        inside
    }

    fn text(&self) -> String {
        let items: Vec<String> = self.perimeter.iter().map(GeoCoordinate::text).collect();
        format!("QGeoPolygon([ {} ])", items.join("; "))
    }
}

#[derive(Debug)]
pub struct PolygonType;
pub static POLYGON: PolygonType = PolygonType;

impl GeoValue for GeoPolygon {
    fn ty() -> &'static dyn CustomType {
        &POLYGON
    }
    fn from_custom(v: &CustomValue) -> Self {
        Self {
            perimeter: coordinates_of(v.field(0)),
        }
    }
    fn to_custom(&self) -> CustomValue {
        CustomValue::new(&POLYGON, vec![coordinates_value(&self.perimeter)])
    }
}
qml_value!(GeoPolygon);

impl CustomType for PolygonType {
    fn name(&self) -> &'static str {
        "geoPolygon"
    }
    fn module(&self) -> &'static str {
        "QtPositioning"
    }
    fn members(&self) -> &'static [CustomMember] {
        const M: &[CustomMember] = &[
            SHAPE_MEMBERS[0],
            SHAPE_MEMBERS[1],
            SHAPE_MEMBERS[2],
            SHAPE_MEMBERS[3],
            CustomMember::stored("perimeter", "list"),
        ];
        M
    }
    fn methods(&self) -> &'static [&'static str] {
        &[
            "contains",
            "boundingGeoRectangle",
            "length",
            "size",
            "coordinateAt",
            "containsCoordinate",
        ]
    }
    fn default_fields(&self) -> Vec<Value> {
        GeoPolygon::default().to_custom().fields().to_vec()
    }
    fn computed(&self, v: &CustomValue, name: &str) -> Value {
        Shape::Polygon(GeoPolygon::from_custom(v)).member(name)
    }
    fn text(&self, v: &CustomValue) -> String {
        GeoPolygon::from_custom(v).text()
    }
    fn with(&self, v: &CustomValue, name: &str, to: &Value) -> Option<CustomValue> {
        let _ = v;
        (name == "perimeter").then(|| {
            GeoPolygon {
                perimeter: coordinates_of(to),
            }
            .to_custom()
        })
    }
    fn call(&self, v: &CustomValue, method: &str, args: &[Value]) -> Result<Value, String> {
        let p = GeoPolygon::from_custom(v);
        match coordinate_methods(&p.perimeter, method, args) {
            Some(out) => Ok(out),
            None => Shape::Polygon(p).call(method, args),
        }
    }
    fn coerce(&self, v: &Value) -> Option<Value> {
        match Shape::of(v)? {
            Shape::Polygon(p) => Some(p.value()),
            _ => None,
        }
    }
}

// ----------------------------------------------------------------- geoShape

/// Any of the shapes, or none.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Shape {
    #[default]
    Unknown,
    Rectangle(GeoRectangle),
    Circle(GeoCircle),
    Path(GeoPath),
    Polygon(GeoPolygon),
}

impl Shape {
    /// The shape `v` is or holds.
    pub fn of(v: &Value) -> Option<Shape> {
        let Value::Custom(c) = v else {
            return None;
        };
        Some(if c.is(&RECTANGLE) {
            Shape::Rectangle(GeoRectangle::from_custom(c))
        } else if c.is(&CIRCLE) {
            Shape::Circle(GeoCircle::from_custom(c))
        } else if c.is(&PATH) {
            Shape::Path(GeoPath::from_custom(c))
        } else if c.is(&POLYGON) {
            Shape::Polygon(GeoPolygon::from_custom(c))
        } else if c.is(&SHAPE) {
            match c.field(0) {
                Value::Undefined | Value::Null => Shape::Unknown,
                held => Shape::of(held)?,
            }
        } else {
            return None;
        })
    }

    pub fn type_id(&self) -> i32 {
        match self {
            Shape::Unknown => shape_type::UNKNOWN,
            Shape::Rectangle(_) => shape_type::RECTANGLE,
            Shape::Circle(_) => shape_type::CIRCLE,
            Shape::Path(_) => shape_type::PATH,
            Shape::Polygon(_) => shape_type::POLYGON,
        }
    }

    pub fn is_valid(&self) -> bool {
        match self {
            Shape::Unknown => false,
            Shape::Rectangle(r) => r.is_valid(),
            Shape::Circle(c) => c.is_valid(),
            Shape::Path(p) => p.is_valid(),
            Shape::Polygon(p) => p.is_valid(),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Shape::Unknown => true,
            Shape::Rectangle(r) => r.is_empty(),
            Shape::Circle(c) => c.is_empty(),
            Shape::Path(p) => p.path.is_empty(),
            Shape::Polygon(p) => p.perimeter.is_empty(),
        }
    }

    pub fn bounding(&self) -> GeoRectangle {
        match self {
            Shape::Unknown => GeoRectangle::default(),
            Shape::Rectangle(r) => *r,
            Shape::Circle(c) => c.bounding(),
            Shape::Path(p) => GeoRectangle::bounding(&p.path),
            Shape::Polygon(p) => GeoRectangle::bounding(&p.perimeter),
        }
    }

    pub fn center(&self) -> GeoCoordinate {
        match self {
            Shape::Circle(c) => c.center,
            _ => self.bounding().center(),
        }
    }

    pub fn contains(&self, c: &GeoCoordinate) -> bool {
        match self {
            Shape::Unknown => false,
            Shape::Rectangle(r) => r.contains(c),
            Shape::Circle(circle) => circle.contains(c),
            Shape::Path(p) => p.contains(c),
            Shape::Polygon(p) => p.contains(c),
        }
    }

    fn member(&self, name: &str) -> Value {
        match name {
            "type" => Value::Int(self.type_id()),
            "isValid" => Value::Bool(self.is_valid()),
            "isEmpty" => Value::Bool(self.is_empty()),
            "center" => self.center().value(),
            _ => Value::Undefined,
        }
    }

    fn call(&self, method: &str, args: &[Value]) -> Result<Value, String> {
        match method {
            "contains" => Ok(Value::Bool(self.contains(&coordinate_of(arg(args, 0))))),
            "boundingGeoRectangle" => Ok(self.bounding().value()),
            _ => Err(format!("{method} is not a function")),
        }
    }

    /// As the shape value itself: a rectangle as a `geoRectangle`.
    pub fn held_value(&self) -> Value {
        match self {
            // Null, not undefined: a face with an undefined field reads
            // back as a plain object.
            Shape::Unknown => Value::Null,
            Shape::Rectangle(r) => r.value(),
            Shape::Circle(c) => c.value(),
            Shape::Path(p) => p.value(),
            Shape::Polygon(p) => p.value(),
        }
    }
}

#[derive(Debug)]
pub struct ShapeType;
pub static SHAPE: ShapeType = ShapeType;

impl GeoValue for Shape {
    fn ty() -> &'static dyn CustomType {
        &SHAPE
    }
    fn from_custom(v: &CustomValue) -> Self {
        Shape::of(&Value::from(v.clone())).unwrap_or_default()
    }
    fn to_custom(&self) -> CustomValue {
        CustomValue::new(&SHAPE, vec![self.held_value()])
    }
}
qml_value!(Shape);

impl CustomType for ShapeType {
    fn name(&self) -> &'static str {
        "geoShape"
    }
    fn module(&self) -> &'static str {
        "QtPositioning"
    }
    fn members(&self) -> &'static [CustomMember] {
        const M: &[CustomMember] = &[
            SHAPE_MEMBERS[0],
            SHAPE_MEMBERS[1],
            SHAPE_MEMBERS[2],
            SHAPE_MEMBERS[3],
            // The shape it holds, which a script does not list.
            CustomMember::stored("__ljos_shape", "var").hidden(),
        ];
        M
    }
    fn methods(&self) -> &'static [&'static str] {
        &["contains", "boundingGeoRectangle"]
    }
    fn default_fields(&self) -> Vec<Value> {
        vec![Value::Null]
    }
    fn computed(&self, v: &CustomValue, name: &str) -> Value {
        Shape::from_custom(v).member(name)
    }
    fn text(&self, v: &CustomValue) -> String {
        format!("QGeoShape({})", Shape::from_custom(v).type_id())
    }
    fn with(&self, _v: &CustomValue, _name: &str, _to: &Value) -> Option<CustomValue> {
        None
    }
    fn call(&self, v: &CustomValue, method: &str, args: &[Value]) -> Result<Value, String> {
        Shape::from_custom(v).call(method, args)
    }
    fn coerce(&self, v: &Value) -> Option<Value> {
        Shape::of(v).map(|s| s.value())
    }
}

/// Every type, for [`crate::register`].
pub fn all() -> [&'static dyn CustomType; 6] {
    [&COORDINATE, &RECTANGLE, &CIRCLE, &PATH, &POLYGON, &SHAPE]
}
