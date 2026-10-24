//! How the value types look from JS: `point`, `size`, `rect`, the vectors,
//! `quaternion` and `font`.
//!
//! A QML value type is a value. Two rects that hold the same are `===`,
//! `indexOf` finds one in a list, and each still has members. JS compares
//! objects by identity only, so each kind keeps one frozen object per
//! distinct value that something still holds, the way the JS host does for
//! colours. The table has weak references and is swept when it has doubled
//! since the last sweep; nothing runs for it between uses.
//!
//! One object standing for every use of a value has nowhere to keep a
//! write, so `area.x = 5` is compiled as a write through
//! `__ljos_vt(object, "area")`: that asks the value for itself with one
//! member changed (`__ljos_with`) and assigns the result where the value
//! was read from. For a property that is the write-back Qt does. A variable
//! whose members are written and whose value was read from a place keeps
//! that place beside it (`__ljos_at`), and a write to a member goes back
//! there too (`__ljos_vp`), as it does in Qt.
//!
//! Text is made here the way [`crate::value_types`] makes it in Rust; the
//! two must give the same.

use ljos_js::rquickjs::Object;
use ljos_js::{JsFault, JsHost, JsValue, ValueKind};

use crate::atom::Atom;
use crate::value::{Font, Value};
use crate::value_types::{Quaternion, Vector2d, Vector3d, Vector4d};

/// The kinds, in the order [`define`] defines them. A host numbers its
/// kinds from zero, and these are the only ones the engine's host has.
const POINT: ValueKind = ValueKind::nth(0);
const SIZE: ValueKind = ValueKind::nth(1);
const RECT: ValueKind = ValueKind::nth(2);
const VECTOR2D: ValueKind = ValueKind::nth(3);
const VECTOR3D: ValueKind = ValueKind::nth(4);
const VECTOR4D: ValueKind = ValueKind::nth(5);
const QUATERNION: ValueKind = ValueKind::nth(6);
const FONT: ValueKind = ValueKind::nth(7);
/// The wheel's whole-number point: last, so the kinds before it keep
/// their numbers.
const INT_POINT: ValueKind = ValueKind::nth(8);

const ORDER: [(&str, ValueKind); 9] = [
    ("point", POINT),
    ("size", SIZE),
    ("rect", RECT),
    ("vector2d", VECTOR2D),
    ("vector3d", VECTOR3D),
    ("vector4d", VECTOR4D),
    ("quaternion", QUATERNION),
    ("font", FONT),
    ("intPoint", INT_POINT),
];

/// Evaluates [`FACES`] and tells the host about each kind. Runs before the
/// prelude, which puts the constructors on `Qt`.
pub(crate) fn define(host: &JsHost) -> Result<(), JsFault> {
    host.eval_script("value-types", FACES)?;
    for (name, kind) in ORDER {
        let got = host.define_value_kind(&format!("__ljos_faces.{name}"))?;
        // Nothing else defines kinds on this host; were that to change,
        // every value would cross as the wrong type.
        assert_eq!(got, kind, "value kinds are defined in a fixed order");
    }
    font_base_changed(host)
}

/// Makes the font `Qt.font({...})` starts from the engine's default one
/// ([`Font::default`]) as it is now.
pub(crate) fn font_base_changed(host: &JsHost) -> Result<(), JsFault> {
    let base = to_js(&Value::Font(std::sync::Arc::default())).unwrap_or_default();
    host.enter("value-types", host.config().slice, |scope| {
        let faces: Object = scope.globals().get("__ljos_faces")?;
        let font: Object = faces.get("font")?;
        font.set("dpi", ljos_runtime::Platform::CURRENT.logical_dpi())?;
        font.set("base", scope.to_value(&base)?)
    })
}

fn numbers(kind: ValueKind, of: &[f64]) -> JsValue {
    JsValue::Typed(kind, of.iter().map(|n| JsValue::Number(*n)).collect())
}

/// A map a font keeps (`features`) as JS takes it.
fn handle(v: &Value) -> JsValue {
    match v {
        Value::Js(h) => JsValue::Handle(h.0.clone()),
        _ => JsValue::Undefined,
    }
}

/// A value type as JS sees it; `None` for every other value.
pub(crate) fn to_js(v: &Value) -> Option<JsValue> {
    let f = f64::from;
    Some(match v {
        Value::Point(p) => numbers(POINT, &[p.x, p.y]),
        Value::IntPoint(p) => numbers(INT_POINT, &[f64::from(p.x), f64::from(p.y)]),
        Value::Size(s) => numbers(SIZE, &[s.width, s.height]),
        Value::Rect(r) => numbers(RECT, &[r.x, r.y, r.width, r.height]),
        Value::Vector2d(p) => numbers(VECTOR2D, &[f(p.x), f(p.y)]),
        Value::Vector3d(p) => numbers(VECTOR3D, &[f(p.x), f(p.y), f(p.z)]),
        Value::Vector4d(p) => numbers(VECTOR4D, &[f(p.x), f(p.y), f(p.z), f(p.w)]),
        Value::Quaternion(q) => numbers(QUATERNION, &[f(q.scalar), f(q.x), f(q.y), f(q.z)]),
        Value::Font(font) => JsValue::Typed(
            FONT,
            vec![
                JsValue::String(font.family.to_string()),
                JsValue::Number(font.point_size),
                JsValue::Number(f64::from(font.pixel_size)),
                JsValue::Number(f64::from(font.weight)),
                JsValue::Bool(font.italic),
                JsValue::Bool(font.underline),
                JsValue::Bool(font.strikeout),
                JsValue::Number(font.letter_spacing),
                JsValue::Number(font.word_spacing),
                JsValue::Bool(font.scale_with_system),
                JsValue::Number(f64::from(font.capitalization)),
                JsValue::Number(f64::from(font.hinting_preference)),
                JsValue::String(font.style_name.to_string()),
                JsValue::Bool(font.overline),
                JsValue::Bool(font.kerning),
                JsValue::Bool(font.prefer_shaping),
                JsValue::Bool(font.context_font_merging),
                JsValue::Bool(font.prefer_typo_line_metrics),
                handle(&font.features),
                handle(&font.variable_axes),
            ],
        ),
        _ => return None,
    })
}

/// A map of a font that came back from JS.
fn kept(v: Option<&JsValue>) -> Value {
    match v {
        Some(JsValue::Handle(r)) => Value::Js(crate::value::JsHandle(r.clone())),
        _ => Value::Undefined,
    }
}

/// What a [`JsValue::Typed`] that came back from JS holds.
pub(crate) fn from_js(kind: ValueKind, fields: &[JsValue]) -> Value {
    let n = |i: usize| match fields.get(i) {
        Some(JsValue::Number(n)) => *n,
        _ => 0.0,
    };
    let s = |i: usize| n(i) as f32;
    let b = |i: usize| matches!(fields.get(i), Some(JsValue::Bool(true)));
    match kind {
        POINT => Value::point(n(0), n(1)),
        INT_POINT => Value::int_point(n(0) as i32, n(1) as i32),
        SIZE => Value::size(n(0), n(1)),
        RECT => Value::rect(n(0), n(1), n(2), n(3)),
        VECTOR2D => Value::Vector2d(Vector2d { x: s(0), y: s(1) }),
        VECTOR3D => Value::Vector3d(Vector3d {
            x: s(0),
            y: s(1),
            z: s(2),
        }),
        VECTOR4D => Value::Vector4d(std::sync::Arc::new(Vector4d {
            x: s(0),
            y: s(1),
            z: s(2),
            w: s(3),
        })),
        QUATERNION => Value::Quaternion(std::sync::Arc::new(Quaternion {
            scalar: s(0),
            x: s(1),
            y: s(2),
            z: s(3),
        })),
        FONT => Value::Font(std::sync::Arc::new(Font {
            family: match fields.first() {
                Some(JsValue::String(family)) => family.as_str().into(),
                _ => Font::default().family,
            },
            point_size: n(1),
            pixel_size: n(2) as i32,
            weight: n(3) as i32,
            italic: b(4),
            underline: b(5),
            strikeout: b(6),
            letter_spacing: n(7),
            word_spacing: n(8),
            scale_with_system: b(9),
            capitalization: n(10) as i32,
            hinting_preference: n(11) as i32,
            style_name: match fields.get(12) {
                Some(JsValue::String(name)) => name.as_str().into(),
                _ => Atom::EMPTY,
            },
            overline: b(13),
            kerning: !matches!(fields.get(14), Some(JsValue::Bool(false))),
            prefer_shaping: !matches!(fields.get(15), Some(JsValue::Bool(false))),
            context_font_merging: b(16),
            prefer_typo_line_metrics: b(17),
            features: kept(fields.get(18)),
            variable_axes: kept(fields.get(19)),
        })),
        _ => Value::Undefined,
    }
}

/// Defines `__ljos_faces` (one `{ make, proto, fields }` per kind, and `qt`:
/// the constructors that go on `Qt`), `__ljos_vt`, `__ljos_vl`, `__ljos_at`
/// and `__ljos_vp`.
pub(crate) const FACES: &str = r##"
(function () {
  "use strict";
  const g = globalThis;
  const f32 = Math.fround;
  const hide = (o, name, value) =>
    Object.defineProperty(o, name, { value, writable: true, configurable: true });

  // A number as C's `%g` writes it, which is how Qt writes the numbers of a
  // value type: six digits that count, an exponent past them.
  const g6 = (v) => {
    v = Number(v);
    if (v !== v) return "nan";
    if (v === Infinity) return "inf";
    if (v === -Infinity) return "-inf";
    if (v === 0) return "0";
    const sci = v.toExponential(5);
    const at = sci.indexOf("e");
    const exp = Number(sci.slice(at + 1));
    const trim = (s) => (s.indexOf(".") < 0 ? s : s.replace(/\.?0+$/, ""));
    if (exp < -4 || exp >= 6) {
      const e = Math.abs(exp);
      return trim(sci.slice(0, at)) + "e" + (exp < 0 ? "-" : "+") + (e < 10 ? "0" : "") + e;
    }
    return trim(v.toFixed(5 - exp));
  };

  // One object per distinct value that something still holds, found by
  // its fields one after another in maps of maps. A number is its own key:
  // writing it out to look it up cost more than the rest of making a
  // point. A map a font holds is keyed by what it holds: two fonts with
  // the same features are one value, and two with different ones are two.
  const keyOf = (v) => (typeof v === "object" && v !== null ? JSON.stringify(v) : v);
  const interned = (build) => {
    const live = new Map();
    let size = 0;
    let sweepAt = 256;
    // Drops what nothing holds any more; how many are left.
    const sweep = (level, depth) => {
      let left = 0;
      for (const [k, v] of level) {
        const n = depth === 0 ? (v.deref() === undefined ? 0 : 1) : sweep(v, depth - 1);
        if (n === 0) level.delete(k);
        left += n;
      }
      return left;
    };
    return (vals) => {
      const last = vals.length - 1;
      let level = live;
      for (let i = 0; i < last; ++i) {
        const v = vals[i];
        const key = typeof v === "object" ? keyOf(v) : v;
        let next = level.get(key);
        if (next === undefined) {
          next = new Map();
          level.set(key, next);
        }
        level = next;
      }
      const v = vals[last];
      const key = typeof v === "object" ? keyOf(v) : v;
      const ref = level.get(key);
      if (ref !== undefined) {
        const known = ref.deref();
        if (known !== undefined) return known;
      } else {
        size += 1;
      }
      const made = Object.freeze(build(vals));
      level.set(key, new WeakRef(made));
      if (size >= sweepAt) {
        size = sweep(live, last);
        sweepAt = Math.max(256, size * 2);
      }
      return made;
    };
  };

  // `read[i]` turns what a script gave for `fields[i]` into what the type
  // holds. `fill(o, vals)` writes the members a script sees, in the order
  // Qt lists them.
  const kind = (fields, read, fill, text) => {
    const proto = {};
    // Made by `new`, every value of a kind shares one shape, which
    // `Object.create` and a fill do not give.
    function Made(vals) { fill(this, vals); }
    Made.prototype = proto;
    const intern = interned((vals) => new Made(vals));
    // From a list or `arguments`: spreading them costs as much as the
    // rest of making a point.
    const of = (vals) => {
      const got = [];
      for (let i = 0; i < fields.length; ++i) got.push(read[i](vals[i]));
      return intern(got);
    };
    const make = (...vals) => of(vals);
    hide(proto, "toString", function toString() { return text(this); });
    // This value with one member changed; `undefined` when it has no such
    // member to write, which a script cannot tell from a write that worked.
    hide(proto, "__ljos_with", function (member, value) {
      const i = fields.indexOf(member);
      if (i < 0) return undefined;
      const vals = fields.map((f) => this[f]);
      vals[i] = value;
      return of(vals);
    });
    return { make, of, proto, fields };
  };

  // Adding zero makes the one zero there is of the two JS has: they print
  // alike and would otherwise be two values that look the same.
  const double = (v) => Number(v) + 0;
  const single = (v) => f32(Number(v)) + 0;
  const plain = (fields) => (o, vals) => {
    for (let i = 0; i < fields.length; ++i) o[fields[i]] = vals[i];
  };
  const named = (name, fields) => (o) => name + "(" + fields.map((f) => g6(o[f])).join(", ") + ")";
  const numeric = (name, fields, read) =>
    kind(fields, fields.map(() => read), plain(fields), named(name, fields));

  const point = numeric("QPointF", ["x", "y"], double);
  // A wheel delta: whole numbers, written in full, not `%g`.
  const int32 = (v) => { v = Math.trunc(Number(v)); return v !== v ? 0 : v + 0; };
  const intPoint = kind(["x", "y"], [int32, int32], plain(["x", "y"]),
    (o) => "QPoint(" + o.x + ", " + o.y + ")");
  const size = numeric("QSizeF", ["width", "height"], double);

  const rectFields = ["x", "y", "width", "height"];
  const rect = kind(rectFields, rectFields.map(() => double), (o, [x, y, width, height]) => {
    o.x = x; o.y = y; o.width = width; o.height = height;
    o.left = x; o.right = x + width; o.top = y; o.bottom = y + height;
  }, named("QRectF", rectFields));
  {
    const change = rect.proto.__ljos_with;
    hide(rect.proto, "__ljos_with", function (member, value) {
      // The edges are worked out from the four numbers; Qt refuses a write
      // to one in these words.
      if (member === "left" || member === "right" || member === "top" || member === "bottom") {
        throw new Error("Cannot assign double to double");
      }
      return change.call(this, member, value);
    });
  }

  const vector2d = numeric("QVector2D", ["x", "y"], single);
  const vector3d = numeric("QVector3D", ["x", "y", "z"], single);
  const vector4d = numeric("QVector4D", ["x", "y", "z", "w"], single);
  const quaternion = numeric("QQuaternion", ["scalar", "x", "y", "z"], single);

  // Vector arithmetic is done in single precision, as the vectors are kept.
  const vectors = [vector2d, vector3d, vector4d];
  const refuse = () => {
    throw new TypeError("Passing incompatible arguments to C++ functions from JavaScript is not allowed.");
  };
  const count = (n, least) => {
    if (n < least) throw new Error("Insufficient arguments");
  };
  for (const face of vectors) {
    const fields = face.fields;
    const of = (v) => fields.map((f) => v[f]);
    // Another vector is taken as one of this size: cut short, or with
    // zeros where it has no number.
    const same = (v) => {
      if (v === null || typeof v !== "object") refuse();
      const p = Object.getPrototypeOf(v);
      if (p === face.proto) return of(v);
      if (!vectors.some((other) => other.proto === p)) refuse();
      return fields.map((f) => (v[f] === undefined ? 0 : v[f]));
    };
    const each = (a, b, op) => face.make(...a.map((x, i) => op(x, b[i])));
    const dot = (a, b) => a.reduce((sum, x, i) => f32(sum + f32(x * b[i])), 0);
    const methods = {
      plus(v) { count(arguments.length, 1); return each(of(this), same(v), (x, y) => x + y); },
      minus(v) { count(arguments.length, 1); return each(of(this), same(v), (x, y) => x - y); },
      times(v) {
        if (arguments.length < 1) {
          throw new Error("Unable to determine callable overload.");
        }
        if (v !== null && typeof v === "object") return each(of(this), same(v), (x, y) => x * y);
        const by = f32(Number(v));
        return face.make(...of(this).map((x) => x * by));
      },
      dotProduct(v) { count(arguments.length, 1); return dot(of(this), same(v)); },
      length() {
        const a = of(this);
        if (a.length !== 3) return f32(Math.sqrt(a.reduce((sum, x) => sum + x * x, 0)));
        // Three numbers are measured in single precision and scaled by the
        // largest, which is where qml6's last digit comes from: (1, 2, 3) is
        // 3.741657257080078 there, one step below the nearest to sqrt(14).
        const most = Math.max(...a.map(Math.abs));
        if (most === 0) return 0;
        const sum = a.map((x) => f32(Math.abs(x) / most))
          .reduce((sum, x) => f32(sum + f32(x * x)), 0);
        return f32(most * f32(Math.sqrt(sum)));
      },
      normalized() {
        const a = of(this);
        const length = Math.sqrt(a.reduce((sum, x) => sum + x * x, 0));
        return length === 0 ? this : face.make(...a.map((x) => x / length));
      },
      // Without a margin Qt also calls two numbers equal when they differ
      // by a hundred-thousandth of the smaller one.
      fuzzyEquals(v, epsilon) {
        count(arguments.length, 1);
        const a = of(this);
        const b = same(v);
        const loose = epsilon === undefined;
        const margin = loose ? f32(0.00001) : Math.abs(f32(Number(epsilon)));
        return a.every((x, i) => {
          const d = Math.abs(f32(x - b[i]));
          return d <= margin
            || (loose && f32(d * 100000) <= Math.min(Math.abs(x), Math.abs(b[i])));
        });
      },
    };
    for (const name of Object.keys(methods)) hide(face.proto, name, methods[name]);
    for (const [name, other] of [["toVector2d", vector2d], ["toVector3d", vector3d], ["toVector4d", vector4d]]) {
      if (other === face) continue;
      hide(face.proto, name, function () {
        return other.make(...other.fields.map((f) => (this[f] === undefined ? 0 : this[f])));
      });
    }
  }
  hide(vector3d.proto, "crossProduct", function crossProduct(v) {
    count(arguments.length, 1);
    if (v === null || typeof v !== "object" || Object.getPrototypeOf(v) !== vector3d.proto) refuse();
    return vector3d.make(
      this.y * v.z - this.z * v.y,
      this.z * v.x - this.x * v.z,
      this.x * v.y - this.y * v.x);
  });

  // A font crosses as these, in this order (see `to_js` beside this text).
  // The two sizes cross as they were asked for and are kept out of sight;
  // a script reads each size in both units, as Qt gives them.
  const fontFields = ["family", "__ljos_pointSize", "__ljos_pixelSize", "weight", "italic", "underline",
    "strikeout", "letterSpacing", "wordSpacing", "scaleWithSystem", "capitalization",
    "hintingPreference", "styleName", "overline", "kerning", "preferShaping", "contextFontMerging",
    "preferTypoLineMetrics", "features", "variableAxes"];
  // Qt's answer for a map never given: an empty one.
  const noMap = Object.freeze({});
  const map = (v) => (v === undefined || v === null || v === noMap ? undefined : v);
  const whole = (v) => { v = Math.trunc(Number(v)); return v !== v ? 0 : v + 0; };
  const flag = (v) => !!v;
  const font = kind(fontFields,
    [String, double, whole, whole, flag, flag, flag, double, double, flag, whole, whole,
      String, flag, flag, flag, flag, flag, map, map],
    (o, [family, pointSize, pixelSize, weight, italic, underline, strikeout, letterSpacing, wordSpacing,
      scaleWithSystem, capitalization, hintingPreference, styleName, overline, kerning, preferShaping,
      contextFontMerging, preferTypoLineMetrics, features, variableAxes]) => {
      o.family = family;
      o.bold = weight > 500;
      o.weight = weight;
      o.italic = italic;
      o.underline = underline;
      o.strikeout = strikeout;
      o.pointSize = pixelSize > 0 ? pixelSize * 72 / font.dpi : pointSize;
      o.pixelSize = pixelSize > 0 ? pixelSize : pointSize > 0 ? Math.trunc(pointSize * font.dpi / 72) : -1;
      hide(o, "__ljos_pointSize", pointSize);
      hide(o, "__ljos_pixelSize", pixelSize);
      o.capitalization = capitalization;
      o.letterSpacing = letterSpacing;
      o.wordSpacing = wordSpacing;
      o.hintingPreference = hintingPreference;
      o.styleName = styleName;
      o.overline = overline;
      o.kerning = kerning;
      o.preferShaping = preferShaping;
      o.contextFontMerging = contextFontMerging;
      o.preferTypoLineMetrics = preferTypoLineMetrics;
      o.features = features === undefined ? noMap : features;
      o.variableAxes = variableAxes === undefined ? noMap : variableAxes;
      // Ours, so not one of the keys a script finds when it lists a font's:
      // those stay Qt's.
      hide(o, "scaleWithSystem", scaleWithSystem);
    },
    // Every setting a font has in Qt; the ones a QML font here cannot
    // change are written as Qt has them by default.
    (o) => "QFont(" + [o.family, o.__ljos_pointSize, o.__ljos_pixelSize, 5, o.weight, +o.italic, +o.underline,
      +o.strikeout, 0, 0, o.capitalization, o.letterSpacing !== 0 ? 1 : 0, o.letterSpacing, o.wordSpacing,
      0, 1, "", 0, 0].join(",") + ")");
  {
    const change = font.proto.__ljos_with;
    hide(font.proto, "__ljos_with", function (member, value) {
      if (member === "bold") return change.call(this, "weight", value ? 700 : 400);
      // One size, as `Value::with_member` keeps it.
      if (member === "pointSize") {
        return this.__ljos_pixelSize > 0 ? this : change.call(this, "__ljos_pointSize", value);
      }
      if (member === "pixelSize") {
        const next = change.call(this, "__ljos_pixelSize", value);
        return next.__ljos_pixelSize > 0 ? change.call(next, "__ljos_pointSize", -1) : next;
      }
      if (typeof member === "string" && member.startsWith("__ljos_")) return undefined;
      return change.call(this, member, value);
    });
  }
  // What `Qt.font` knows by name. A font here keeps the first thirteen.
  const fontKeys = ["family", "bold", "weight", "italic", "underline", "strikeout", "pointSize",
    "pixelSize", "letterSpacing", "wordSpacing", "scaleWithSystem", "capitalization",
    "hintingPreference", "styleName", "overline", "kerning", "preferShaping", "features", "variableAxes",
    "contextFontMerging", "preferTypoLineMetrics"];
  // The screen's logical DPI a point is taken at: 72 on macOS, 96
  // elsewhere. The engine sets it before any font is made.
  font.dpi = 96;
  // `font.base`, the font a `Qt.font` call starts from, is set by the engine.
  const makeFont = function (settings) {
    if (arguments.length < 1) throw new Error("Insufficient arguments");
    if (arguments.length > 1) throw new Error("Too many arguments");
    if (settings === null || typeof settings !== "object") {
      throw new Error("Qt.font(): Invalid arguments");
    }
    let out = font.base;
    let known = false;
    for (const key of fontKeys) {
      if (settings[key] === undefined) continue;
      known = true;
      const next = out.__ljos_with(key, settings[key]);
      if (next !== undefined) out = next;
    }
    if (!known) {
      throw new Error("Qt.font(): Invalid argument: no valid font subproperties specified");
    }
    return out;
  };

  const exactly = (face) => {
    const n = face.fields.length;
    return function () {
      if (arguments.length < n) throw new Error("Insufficient arguments");
      if (arguments.length > n) throw new Error("Too many arguments");
      return face.of(arguments);
    };
  };

  g.__ljos_faces = {
    point, size, rect, vector2d, vector3d, vector4d, quaternion, font, intPoint,
    qt: {
      point: exactly(point), size: exactly(size), rect: exactly(rect),
      vector2d: exactly(vector2d), vector3d: exactly(vector3d), vector4d: exactly(vector4d),
      quaternion: exactly(quaternion), font: makeFont,
    },
  };

  // `object.name.member = v`, compiled: what `object.name` is, or, for a
  // value, a stand-in whose writes go back to `object.name`. The stand-in
  // is not the value itself, which is frozen and shared.
  const standIn = (first, put) => {
    let now = first;
    return new Proxy({}, {
      get(_, member) { return now[member]; },
      set(_, member, v) {
        const next = now.__ljos_with(member, v);
        if (next !== undefined) { put(next); now = next; }
        return true;
      },
    });
  };
  const isValue = (v) => v !== null && typeof v === "object" && v.__ljos_with !== undefined;
  g.__ljos_vt = function (object, name) {
    const now = object[name];
    return isValue(now) ? standIn(now, (next) => { object[name] = next; }) : now;
  };
  // The same for a variable, which only its own script can assign to.
  g.__ljos_vl = function (now, put) {
    return isValue(now) ? standIn(now, put) : now;
  };
  // `const p = object.name`, where the script writes a member of `p`: in Qt
  // `p` names the place, so a write starts from what is there now and goes
  // back there as well as to the variable. `v` is what the variable was
  // last given; were it given something else, the place is not its own.
  g.__ljos_at = function (object, name) {
    return { object, name, v: object[name] };
  };
  g.__ljos_vp = function (now, at, put) {
    if (!isValue(now)) return now;
    if (at === null || at === undefined || at.v !== now) return standIn(now, put);
    const there = at.object[at.name];
    return standIn(isValue(there) ? there : now, (next) => {
      at.object[at.name] = next;
      // What the place made of it, as a property may change what it is given.
      const back = at.object[at.name];
      at.v = isValue(back) ? back : next;
      put(at.v);
    });
  };
})();
"##;

/// `Qt.lighter` and its like, and writes to a channel of a colour. Runs
/// after the prelude, whose `Qt.rgba`, `Qt.hsva` and `Qt.hsla` round the
/// way Qt does.
///
/// Qt works on sixteen bits of hue, saturation and value here, cutting
/// where it divides, and these do the same so that the channels come out
/// as Qt's do.
pub(crate) const COLORS: &str = r##"
(function () {
  "use strict";
  const g = globalThis;
  const Qt = g.Qt;
  const { rgba, hsva, hsla } = Qt;
  const named = g.__ljos_colorOf;
  const U = 65535;
  const make = g.__ljos_color;
  const proto = Object.getPrototypeOf(make(0, 0, 0, 0));
  const isColor = (v) => v !== null && typeof v === "object" && Object.getPrototypeOf(v) === proto;
  // A colour, or the colour a string names; `null` for anything else.
  const colorOf = (v) => (isColor(v) ? v : typeof v === "string" ? named(v) : null);
  const percent = (factor, usual) => {
    const f = factor === undefined ? usual : Number(factor);
    // The nearest whole hundredths: 1.15 is 115 though it falls short.
    return Math.round(f * 100);
  };
  const hsv = (c) => [
    c.hsvHue < 0 ? 0 : Math.round(c.hsvHue * 36000),
    Math.round(c.hsvSaturation * U),
    Math.round(c.hsvValue * U),
  ];
  // Back to red, green and blue in single precision at every step, which
  // is what decides the last bit where a channel falls half way between
  // two (found by comparing every mix of precisions with qml6's results).
  const f32 = Math.fround;
  const fromHsv = (H, S, V, c) => {
    const h = f32(H / 6000);
    const i = Math.floor(h);
    const f = f32(h - i);
    const s = f32(S / U);
    const v = f32(V / U);
    const ch = (x) => Math.round(f32(f32(v * x) * U));
    const p = ch(f32(1 - s));
    const q = ch(f32(1 - f32(s * f)));
    const t = ch(f32(1 - f32(s * f32(1 - f))));
    const a = Math.round(c.a * U);
    switch (i) {
      case 0: return make(V, t, p, a);
      case 1: return make(q, V, p, a);
      case 2: return make(p, V, t, a);
      case 3: return make(p, q, V, a);
      case 4: return make(t, p, V, a);
      default: return make(V, p, q, a);
    }
  };
  // An invalid colour stays invalid through these, as in qml6.
  const lighter = (c, by) => {
    if (!(by > 0) || !c.valid) return c;
    if (by < 100) return darker(c, Math.trunc(10000 / by));
    let [h, s, v] = hsv(c);
    v = Math.trunc(v * by / 100);
    if (v > U) {
      // What does not fit in the value comes off the saturation.
      s = Math.max(0, s - (v - U));
      v = U;
    }
    return fromHsv(h, s, v, c);
  };
  const darker = (c, by) => {
    if (!(by > 0) || !c.valid) return c;
    if (by < 100) return lighter(c, Math.trunc(10000 / by));
    const [h, s, v] = hsv(c);
    return fromHsv(h, s, Math.trunc(v * 100 / by), c);
  };
  Qt.lighter = function (color, factor) {
    const c = colorOf(color);
    return c === null ? null : lighter(c, percent(factor, 1.5));
  };
  Qt.darker = function (color, factor) {
    const c = colorOf(color);
    return c === null ? null : darker(c, percent(factor, 2));
  };
  Qt.tint = function (base, tint) {
    const b = colorOf(base);
    const t = colorOf(tint);
    if (b === null || t === null) return null;
    const a = t.a;
    if (a === 1 || !t.valid) return t;
    if (a === 0) return b;
    const rest = 1 - a;
    return rgba(t.r * a + b.r * rest, t.g * a + b.g * rest, t.b * a + b.b * rest, a + b.a * rest);
  };
  Qt.alpha = function (color, alpha) {
    const c = colorOf(color);
    if (c === null) return null;
    if (c.valid) return rgba(c.r, c.g, c.b, Number(alpha));
    // Qt sets the alpha of an invalid colour and leaves it invalid.
    const n = Number(alpha);
    const a = Number.isNaN(n) ? 0 : Math.min(1, Math.max(0, n));
    const ch = (v) => Math.round(v * U);
    return make(ch(c.r), ch(c.g), ch(c.b), Math.round(Math.fround(a) * U), false);
  };
  // Qt reads the argument as a string, whatever it is: a colour reads as
  // its own name, `undefined` and `null` as "" (seen in qml6).
  Qt.color = function (name) {
    if (arguments.length < 1) throw new Error("Insufficient arguments");
    if (arguments.length > 1) throw new Error("Too many arguments");
    if (isColor(name)) return name;
    const text = name === undefined || name === null ? "" : String(name);
    const c = named(text);
    if (c === null) throw new Error('"' + text + '" is not a valid color name');
    return c;
  };
  Qt.colorEqual = function (a, b) {
    const one = (v) => {
      if (isColor(v)) return v;
      if (typeof v !== "string") throw new Error("Qt.colorEqual(): Invalid arguments");
      const c = named(v);
      if (c === null) throw new Error("Qt.colorEqual(): Invalid color name");
      return c;
    };
    if (arguments.length !== 2) throw new Error("Qt.colorEqual(): Invalid arguments");
    return one(a) === one(b);
  };

  // A colour has the same four of its own: `c.lighter(2)` is
  // `Qt.lighter(c, 2)`, seen in qml6.
  for (const [name, f] of [
    ["lighter", function (factor) { return Qt.lighter(this, factor); }],
    ["darker", function (factor) { return Qt.darker(this, factor); }],
    ["alpha", function (value) { return Qt.alpha(this, value); }],
    ["tint", function (tint) { return Qt.tint(this, tint); }],
  ]) {
    Object.defineProperty(proto, name, { value: f, writable: true, configurable: true });
  }

  // `rect.color.a = 0.5`, compiled, asks the colour for itself with one
  // channel changed (see `__ljos_vt`).
  Object.defineProperty(proto, "__ljos_with", {
    value: function (member, value) {
      const c = this;
      const pick = (name) => (name === member ? value : c[name]);
      switch (member) {
        case "r": case "g": case "b": case "a":
          return rgba(pick("r"), pick("g"), pick("b"), pick("a"));
        case "hsvHue": case "hsvSaturation": case "hsvValue":
          return hsva(pick("hsvHue"), pick("hsvSaturation"), pick("hsvValue"), c.a);
        case "hslHue": case "hslSaturation": case "hslLightness":
          return hsla(pick("hslHue"), pick("hslSaturation"), pick("hslLightness"), c.a);
        case "valid":
          throw new TypeError("Cannot assign to read-only property \"valid\"");
        default:
          return undefined;
      }
    },
    writable: true, configurable: true,
  });
})();
"##;
