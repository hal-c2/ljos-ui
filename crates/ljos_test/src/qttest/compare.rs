//! What `compare` and `fuzzyCompare` call the same, and how a value is
//! written in a report. The rules are what Qt's runner was seen to do
//! (`features/tooling/qttest-checks.feature`).

use ljos_engine::{Color, Engine, Value};

use super::host::Host;

/// How far two numbers may be apart and still be the same to `compare`.
const TOLERANCE: f64 = 1e-5;

/// Arrays and plain objects are compared member by member, with the rules
/// of `compare` for what they hold.
const DEEP_EQUAL: &str = r"(function () {
    function same(a, b) {
        if (a === b)
            return true;
        if (typeof a !== typeof b)
            return false;
        if (typeof a === 'number')
            return (isNaN(a) && isNaN(b)) || Math.abs(a - b) <= 1e-5;
        if (typeof a !== 'object' || a === null || b === null)
            return false;
        if (a instanceof Date || b instanceof Date)
            return a instanceof Date && b instanceof Date && a.getTime() === b.getTime();
        var i;
        if (Array.isArray(a) !== Array.isArray(b))
            return false;
        if (Array.isArray(a)) {
            if (a.length !== b.length)
                return false;
            for (i = 0; i < a.length; ++i) {
                if (!same(a[i], b[i]))
                    return false;
            }
            return true;
        }
        var ka = Object.keys(a), kb = Object.keys(b);
        if (ka.length !== kb.length)
            return false;
        for (i = 0; i < ka.length; ++i) {
            if (!Object.prototype.hasOwnProperty.call(b, ka[i]) || !same(a[ka[i]], b[ka[i]]))
                return false;
        }
        return true;
    }
    return same;
})()";

const TO_STRING: &str = "(function (v) { return String(v); })";

fn text(v: &Value) -> Option<&str> {
    match v {
        Value::String(s) | Value::Url(s) => Some(s),
        _ => None,
    }
}

/// Whether `v` is something a string is compared with by what it prints
/// as: a colour is the same as `"#ff0000"`.
fn prints_as_itself(v: &Value) -> bool {
    !matches!(
        v,
        Value::Undefined
            | Value::Null
            | Value::Bool(_)
            | Value::Int(_)
            | Value::Real(_)
            | Value::Enum(_)
            | Value::String(_)
            | Value::Url(_)
            | Value::List(_)
    )
}

/// `compare`'s sameness.
pub(crate) fn same(engine: &Engine, host: &Host, a: &Value, b: &Value) -> bool {
    if let (Some(x), Some(y)) = (a.as_number(), b.as_number()) {
        return x == y || (x.is_nan() && y.is_nan()) || (x - y).abs() <= TOLERANCE;
    }
    match (a, b) {
        (Value::List(x), Value::List(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y.iter())
                    .all(|(p, q)| same(engine, host, p, q))
        }
        (Value::Js(_), Value::Js(_)) => host
            .call(engine, DEEP_EQUAL, &[a.clone(), b.clone()])
            .is_some_and(|v| v.to_boolean()),
        _ => match (text(a), text(b)) {
            (Some(x), Some(y)) => x == y,
            (Some(x), None) if prints_as_itself(b) => x == printed(engine, host, b),
            (None, Some(y)) if prints_as_itself(a) => printed(engine, host, a) == y,
            _ => a.strict_equals(b),
        },
    }
}

/// A value as a report shows it.
pub(crate) fn printed(engine: &Engine, host: &Host, v: &Value) -> String {
    match v {
        // What `String` makes of an array, in brackets: an array in an
        // array has none, and nothing stands for `null`.
        Value::List(items) => format!("[{}]", joined(engine, host, items)),
        _ => plain(engine, host, v),
    }
}

fn joined(engine: &Engine, host: &Host, items: &[Value]) -> String {
    let parts: Vec<String> = items
        .iter()
        .map(|v| match v {
            Value::Undefined | Value::Null => String::new(),
            Value::List(inner) => joined(engine, host, inner),
            other => plain(engine, host, other),
        })
        .collect();
    parts.join(",")
}

fn plain(engine: &Engine, host: &Host, v: &Value) -> String {
    match v {
        Value::Js(_) => host
            .call(engine, TO_STRING, std::slice::from_ref(v))
            .map_or_else(
                || "[object Object]".to_owned(),
                |s| engine.to_js_string(&s).to_string(),
            ),
        other => engine.to_js_string(other).to_string(),
    }
}

fn colour(v: &Value) -> Option<Color> {
    match v {
        Value::Color(c) => Some(*c),
        Value::String(s) => Color::parse(s),
        _ => None,
    }
}

/// `fuzzyCompare`'s sameness: two colours channel by channel, two numbers
/// as single-precision numbers, each within `delta`. Anything else is not
/// the same.
pub(crate) fn fuzzy_same(a: &Value, b: &Value, delta: f64) -> bool {
    if matches!(a, Value::Color(_)) || matches!(b, Value::Color(_)) {
        let (Some(x), Some(y)) = (colour(a), colour(b)) else {
            return false;
        };
        return x
            .to_rgba8()
            .iter()
            .zip(y.to_rgba8())
            .all(|(p, q)| (f64::from(*p) - f64::from(q)).abs() <= delta);
    }
    match (a.as_number(), b.as_number()) {
        // Qt compares floats: 0.1 + 0.2 is 0.3 to it.
        #[allow(clippy::cast_possible_truncation)]
        (Some(x), Some(y)) => f64::from((x as f32 - y as f32).abs()) <= delta,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_are_compared_channel_by_channel() {
        let red = Value::Color(Color::parse("#ff0000").expect("a colour"));
        assert!(fuzzy_same(&red, &Value::string("#fd0102"), 2.0));
        assert!(!fuzzy_same(&red, &Value::string("#fc0000"), 2.0));
        assert!(!fuzzy_same(&red, &Value::string("no colour"), 255.0));
        assert!(!fuzzy_same(&red, &Value::Int(1), 255.0));
    }

    #[test]
    fn numbers_are_compared_within_the_delta() {
        assert!(fuzzy_same(&Value::Real(1.0), &Value::Real(1.5), 0.5));
        assert!(!fuzzy_same(&Value::Real(1.0), &Value::Real(1.6), 0.5));
        assert!(fuzzy_same(&Value::Int(3), &Value::Real(3.0), 0.0));
        assert!(!fuzzy_same(&Value::string("a"), &Value::string("a"), 1.0));
    }
}
