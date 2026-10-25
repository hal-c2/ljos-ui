//! The arithmetic of places on the Earth, as Qt Positioning does it: a
//! sphere of the Earth's mean radius, great circles on it, and the Web
//! Mercator projection.

use std::f64::consts::PI;

/// The radius of the sphere distances are measured on, in metres: what
/// qml6's distances come out at.
pub const EARTH_RADIUS: f64 = 6_371_007.2;

/// A latitude and a longitude in degrees and an altitude in metres; any
/// of them NaN when it is not known.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeoCoordinate {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: f64,
}

impl Default for GeoCoordinate {
    /// Nowhere: what a `geoCoordinate` property starts as.
    fn default() -> Self {
        Self::new(f64::NAN, f64::NAN, f64::NAN)
    }
}

impl GeoCoordinate {
    pub const fn new(latitude: f64, longitude: f64, altitude: f64) -> Self {
        Self {
            latitude,
            longitude,
            altitude,
        }
    }

    /// On the Earth: the latitude within ±90 and the longitude within
    /// ±180. The altitude may be unknown.
    pub fn is_valid(&self) -> bool {
        (-90.0..=90.0).contains(&self.latitude) && (-180.0..=180.0).contains(&self.longitude)
    }

    /// Metres along a great circle to `to`; 0 when either is nowhere. The
    /// altitudes play no part.
    pub fn distance_to(&self, to: &GeoCoordinate) -> f64 {
        if !self.is_valid() || !to.is_valid() {
            return 0.0;
        }
        let (p1, p2) = (self.latitude.to_radians(), to.latitude.to_radians());
        let dp = p2 - p1;
        let dl = (to.longitude - self.longitude).to_radians();
        let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
        2.0 * a.sqrt().atan2((1.0 - a).sqrt()) * EARTH_RADIUS
    }

    /// The bearing to `to` in degrees from north, in [0, 360); 0 when
    /// either is nowhere.
    pub fn azimuth_to(&self, to: &GeoCoordinate) -> f64 {
        if !self.is_valid() || !to.is_valid() {
            return 0.0;
        }
        let (p1, p2) = (self.latitude.to_radians(), to.latitude.to_radians());
        let dl = (to.longitude - self.longitude).to_radians();
        let y = dl.sin() * p2.cos();
        let x = p1.cos() * p2.sin() - p1.sin() * p2.cos() * dl.cos();
        let deg = y.atan2(x).to_degrees();
        if deg < 0.0 { deg + 360.0 } else { deg }
    }

    /// Where `distance` metres along a great circle towards `azimuth`
    /// degrees ends; the altitude stays. Nowhere from nowhere.
    pub fn at_distance_and_azimuth(&self, distance: f64, azimuth: f64) -> GeoCoordinate {
        if !self.is_valid() {
            return GeoCoordinate::default();
        }
        let d = distance / EARTH_RADIUS;
        let t = azimuth.to_radians();
        let p1 = self.latitude.to_radians();
        let l1 = self.longitude.to_radians();
        let p2 = (p1.sin() * d.cos() + p1.cos() * d.sin() * t.cos()).asin();
        let l2 = l1 + (t.sin() * d.sin() * p1.cos()).atan2(d.cos() - p1.sin() * p2.sin());
        GeoCoordinate::new(
            p2.to_degrees(),
            wrap_longitude(l2.to_degrees()),
            self.altitude,
        )
    }

    /// Degrees, minutes and seconds with the hemisphere, and the altitude
    /// when it is known: `64° 8' 47.8" N, 21° 56' 33.4" W, 10m`. Nowhere
    /// is the empty text.
    pub fn text(&self) -> String {
        if !self.is_valid() {
            return String::new();
        }
        let mut out = format!(
            "{}, {}",
            dms(self.latitude, 'N', 'S'),
            dms(self.longitude, 'E', 'W')
        );
        if !self.altitude.is_nan() {
            out.push_str(&format!(", {}m", g(self.altitude)));
        }
        out
    }
}

/// A longitude brought back into [-180, 180].
pub fn wrap_longitude(lon: f64) -> f64 {
    if (-180.0..=180.0).contains(&lon) {
        return lon;
    }
    let l = (lon + 180.0).rem_euclid(360.0) - 180.0;
    if l == -180.0 && lon > 0.0 { 180.0 } else { l }
}

/// One angle as `D° M' S.s"` and its hemisphere; none at 0. The seconds
/// are rounded to a tenth and carried into the minutes and degrees.
fn dms(v: f64, plus: char, minus: char) -> String {
    let a = v.abs();
    let mut deg = a.floor();
    let minutes = (a - deg) * 60.0;
    let mut min = minutes.floor();
    let mut sec = ((minutes - min) * 60.0 * 10.0).round() / 10.0;
    if sec >= 60.0 {
        sec -= 60.0;
        min += 1.0;
    }
    if min >= 60.0 {
        min -= 60.0;
        deg += 1.0;
    }
    let hemisphere = match v {
        v if v > 0.0 => format!(" {plus}"),
        v if v < 0.0 => format!(" {minus}"),
        _ => String::new(),
    };
    format!("{deg}° {min}' {sec:.1}\"{hemisphere}")
}

/// A number as C's `%g` writes it: six significant digits, the shorter
/// of the fixed and the exponent forms, no trailing zeros; `nan`.
pub fn g(v: f64) -> String {
    const P: i32 = 6;
    if v.is_nan() {
        return "nan".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "inf" } else { "-inf" }.into();
    }
    if v == 0.0 {
        return if v.is_sign_negative() { "-0" } else { "0" }.into();
    }
    let sci = format!("{:.*e}", (P - 1) as usize, v);
    let (mantissa, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    if !(-4..P).contains(&exp) {
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{}e{sign}{:02}", trim_zeros(mantissa), exp.abs())
    } else {
        let decimals = (P - 1 - exp).max(0) as usize;
        trim_zeros(&format!("{v:.decimals$}")).to_string()
    }
}

fn trim_zeros(s: &str) -> &str {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.')
    } else {
        s
    }
}

/// Web Mercator: `x` and `y` from 0 to 1, `y` growing southwards.
pub fn to_mercator(c: &GeoCoordinate) -> (f64, f64) {
    let x = (c.longitude + 180.0) / 360.0;
    let lat = c.latitude.to_radians();
    let y = 0.5 - (PI / 4.0 + lat / 2.0).tan().ln() / (2.0 * PI);
    (x, y)
}

/// The coordinate at a Web Mercator point, at altitude 0.
pub fn from_mercator(x: f64, y: f64) -> GeoCoordinate {
    let lon = x.rem_euclid(1.0) * 360.0 - 180.0;
    let lat = (2.0 * (PI * (1.0 - 2.0 * y)).exp().atan() - PI / 2.0).to_degrees();
    GeoCoordinate::new(lat, lon, 0.0)
}

/// Metres from `p` to the great-circle segment from `a` to `b`.
pub fn distance_to_segment(p: &GeoCoordinate, a: &GeoCoordinate, b: &GeoCoordinate) -> f64 {
    let ab = a.distance_to(b);
    let ap = a.distance_to(p);
    if ab == 0.0 {
        return ap;
    }
    let d13 = ap / EARTH_RADIUS;
    let t13 = a.azimuth_to(p).to_radians();
    let t12 = a.azimuth_to(b).to_radians();
    let xt = (d13.sin() * (t13 - t12).sin()).clamp(-1.0, 1.0).asin();
    let along = (d13.cos() / xt.cos()).clamp(-1.0, 1.0).acos() * EARTH_RADIUS;
    let ahead = (t13 - t12).cos() >= 0.0;
    if ahead && along <= ab {
        (xt * EARTH_RADIUS).abs()
    } else {
        ap.min(b.distance_to(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g_writes_numbers_as_printf_does() {
        assert_eq!(g(10.0), "10");
        assert_eq!(g(-3.0), "-3");
        assert_eq!(g(1.5), "1.5");
        assert_eq!(g(200000.0), "200000");
        assert_eq!(g(123456789.0), "1.23457e+08");
        assert_eq!(g(0.123456789), "0.123457");
        assert_eq!(g(1e-5), "1e-05");
        assert_eq!(g(f64::NAN), "nan");
    }

    #[test]
    fn text_carries_rounded_seconds() {
        assert_eq!(
            GeoCoordinate::new(89.99999, 0.0, f64::NAN).text(),
            "90° 0' 0.0\" N, 0° 0' 0.0\""
        );
        assert_eq!(
            GeoCoordinate::new(64.1466, -21.9426, 10.0).text(),
            "64° 8' 47.8\" N, 21° 56' 33.4\" W, 10m"
        );
    }

    #[test]
    fn distances_match_qt() {
        let o = GeoCoordinate::new(0.0, 0.0, f64::NAN);
        let d = o.distance_to(&GeoCoordinate::new(0.0, 1.0, f64::NAN));
        assert!((d - 111_195.052_308_264_87).abs() < 1e-6, "{d}");
        assert_eq!(o.azimuth_to(&GeoCoordinate::new(0.0, 1.0, 0.0)), 90.0);
        assert_eq!(o.azimuth_to(&GeoCoordinate::new(0.0, -1.0, 0.0)), 270.0);
    }

    #[test]
    fn mercator_round_trips() {
        let (x, y) = to_mercator(&GeoCoordinate::new(45.0, 90.0, 0.0));
        assert_eq!(x, 0.75);
        assert!((y - 0.359_725_036_915_205).abs() < 1e-12);
        let c = from_mercator(0.25, 0.4);
        assert!((c.latitude - 33.841_220_320_476_78).abs() < 1e-9);
        assert_eq!(c.longitude, -90.0);
    }
}
