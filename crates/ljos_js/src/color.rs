//! How a colour looks from JS ([`crate::JsValue::Color`]).
//!
//! A QML colour is a value: two reads of one colour are `===`, a colour can
//! be found with `indexOf`, and it is still an object with channels
//! (`color.r`). JS only compares objects by identity, so there is exactly one
//! object per distinct colour that something still holds. The table that
//! finds it keeps weak references, and is swept when it has doubled since
//! the last sweep; nothing runs for it between colour reads.
//!
//! The objects are frozen. One object stands for every use of that colour,
//! so a write to a channel has nowhere to go.

/// Evaluates to `{ make, proto }`: `make(r, g, b, a, valid)` takes 16-bit
/// channels and returns the colour object; `proto` is the prototype that
/// marks one. `valid` is false for a colour never given; Qt gives one no
/// hue, saturation, value or lightness (all 0).
///
/// The members are the ones Qt documents for the `color` value type, in the
/// precision Qt gives them: single-precision floats, a hue in hundredths of
/// a degree, saturation and lightness in 16 bits. A grey has no hue (-1).
pub(crate) const FACE: &str = r##"
(function () {
  "use strict";
  const U = 65535;
  const f = Math.fround;
  const live = new Map();
  let sweepAt = 256;

  const hex = (v) => {
    const n = Math.floor((Math.round(v * U) * 255 + 32767) / U);
    return (n < 16 ? "0" : "") + n.toString(16);
  };
  const proto = {};
  Object.defineProperty(proto, "toString", {
    value: function toString() {
      const rgb = hex(this.r) + hex(this.g) + hex(this.b);
      const a = hex(this.a);
      return a === "ff" ? "#" + rgb : "#" + a + rgb;
    },
    writable: true, configurable: true,
  });

  const hue = (r, g, b, max, delta) => {
    let h;
    if (r === max) h = (g - b) / delta;
    else if (g === max) h = 2 + (b - r) / delta;
    else h = 4 + (r - g) / delta;
    h *= 60;
    if (h < 0) h += 360;
    return (Math.round(h * 100) % 36000) / 36000;
  };
  const q16 = (v) => Math.round(v * U) / U;

  function make(r, g, b, a, valid) {
    const invalid = valid === false;
    const key = String.fromCharCode(r, g, b, a) + (invalid ? "!" : "");
    const ref = live.get(key);
    if (ref !== undefined) {
      const known = ref.deref();
      if (known !== undefined) return known;
    }
    const max = Math.max(r, g, b);
    const min = Math.min(r, g, b);
    const delta = max - min;
    const sum = max + min;
    let h = -1, sv = 0, sl = 0;
    if (invalid) {
      h = 0;
    } else if (delta !== 0) {
      h = f(hue(r, g, b, max, delta));
      sv = f(q16(delta / max));
      sl = f(q16(delta / (sum <= U ? sum : 2 * U - sum)));
    }
    const c = Object.create(proto);
    c.r = f(r / U);
    c.g = f(g / U);
    c.b = f(b / U);
    c.a = f(a / U);
    c.hsvHue = h;
    c.hsvSaturation = sv;
    c.hsvValue = invalid ? 0 : f(max / U);
    c.hslHue = h;
    c.hslSaturation = sl;
    c.hslLightness = invalid ? 0 : f(Math.round(sum / 2) / U);
    c.valid = !invalid;
    Object.freeze(c);
    live.set(key, new WeakRef(c));
    if (live.size >= sweepAt) {
      for (const [k, w] of live) if (w.deref() === undefined) live.delete(k);
      sweepAt = Math.max(256, live.size * 2);
    }
    return c;
  }
  return { make, proto };
})()
"##;
