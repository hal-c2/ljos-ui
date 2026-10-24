//! Comparing what Qt and Ljós did with one case.
//!
//! Numbers agree within a few ULPs (the engines round intermediate results
//! differently and neither is wrong), colours whatever the case of their
//! hex digits, warnings by category. Anything else that differs, including
//! a shape this module does not know, is a difference.

use std::collections::BTreeSet;

use serde_json::Value;

use crate::outcome::{Outcome, Phase, category};

/// How far apart two doubles may be, in units in the last place.
const ULPS: u64 = 4;

/// What differs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DiffKind {
    /// A property has another value.
    Value,
    /// A property is observed on one side only.
    Missing,
    /// A step returned another value.
    Result,
    /// A step threw on one side only.
    Throws,
    /// Both threw, with errors of different kinds.
    ErrorKind,
    /// One side warned in a category the other did not.
    Warning,
    /// Qt loaded the file and Ljós did not.
    LjosRejects,
    /// Ljós crashed, hung or panicked.
    LjosCrash,
    /// The sides ran a different number of phases.
    Phases,
}

impl DiffKind {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Value => "value",
            Self::Missing => "missing",
            Self::Result => "result",
            Self::Throws => "throws",
            Self::ErrorKind => "error-kind",
            Self::Warning => "warning",
            Self::LjosRejects => "ljos-rejects",
            Self::LjosCrash => "ljos-crash",
            Self::Phases => "phases",
        }
    }
}

/// One difference between the sides.
#[derive(Clone, Debug, PartialEq)]
pub struct Diff {
    pub kind: DiffKind,
    /// 0 for the load, `i` for the `i`th step.
    pub phase: usize,
    /// The observed property (`"o1.p3"`), the warning category, or empty.
    pub key: String,
    pub qt: Value,
    pub ljos: Value,
}

/// How a pair of outcomes is judged.
#[derive(Debug, PartialEq)]
pub enum Verdict {
    Agree,
    /// Qt did not take the case: a generator bug, not a Ljós one.
    QtRejected(String),
    /// Qt itself crashed or hung on it.
    QtCrashed(String),
    Differ(Vec<Diff>),
}

/// Compares the two sides of one case. `unobserved` keys (the properties of
/// a planted binding loop, whose values neither runtime promises) are left
/// out.
#[must_use]
pub fn compare(qt: &Outcome, ljos: &Outcome, unobserved: &BTreeSet<String>) -> Verdict {
    let (qt, ljos) = match (qt, ljos) {
        (Outcome::Rejected(e), _) => return Verdict::QtRejected(e.clone()),
        (Outcome::Crashed(e), _) => return Verdict::QtCrashed(e.clone()),
        (Outcome::Ran(_), Outcome::Rejected(e)) => {
            return Verdict::Differ(vec![whole(DiffKind::LjosRejects, e)]);
        }
        (Outcome::Ran(_), Outcome::Crashed(e)) => {
            return Verdict::Differ(vec![whole(DiffKind::LjosCrash, e)]);
        }
        (Outcome::Ran(q), Outcome::Ran(l)) => (q, l),
    };
    let mut diffs = Vec::new();
    if qt.len() != ljos.len() {
        diffs.push(Diff {
            kind: DiffKind::Phases,
            phase: 0,
            key: String::new(),
            qt: qt.len().into(),
            ljos: ljos.len().into(),
        });
    }
    for (i, (q, l)) in qt.iter().zip(ljos).enumerate() {
        phase(i, q, l, unobserved, &mut diffs);
    }
    if diffs.is_empty() {
        Verdict::Agree
    } else {
        Verdict::Differ(diffs)
    }
}

fn whole(kind: DiffKind, message: &str) -> Diff {
    Diff {
        kind,
        phase: 0,
        key: String::new(),
        qt: Value::Null,
        ljos: Value::String(message.to_owned()),
    }
}

fn phase(i: usize, q: &Phase, l: &Phase, unobserved: &BTreeSet<String>, out: &mut Vec<Diff>) {
    let diff = |kind, key: &str, qt: &Value, ljos: &Value| Diff {
        kind,
        phase: i,
        key: key.to_owned(),
        qt: qt.clone(),
        ljos: ljos.clone(),
    };
    let keys: BTreeSet<&String> = q.obs.keys().chain(l.obs.keys()).collect();
    for key in keys.into_iter().filter(|k| !unobserved.contains(*k)) {
        match (q.obs.get(key), l.obs.get(key)) {
            (Some(a), Some(b)) if !same(a, b) => out.push(diff(DiffKind::Value, key, a, b)),
            (Some(_), Some(_)) => {}
            (a, b) => out.push(diff(
                DiffKind::Missing,
                key,
                a.unwrap_or(&Value::Null),
                b.unwrap_or(&Value::Null),
            )),
        }
    }
    match (&q.result, &l.result) {
        (Some(a), Some(b)) => match (error_of(a), error_of(b)) {
            (Some(x), Some(y)) if x != y => out.push(diff(DiffKind::ErrorKind, "", a, b)),
            (Some(_), Some(_)) => {}
            (None, None) if !same(a, b) => out.push(diff(DiffKind::Result, "", a, b)),
            (None, None) => {}
            _ => out.push(diff(DiffKind::Throws, "", a, b)),
        },
        (None, None) => {}
        (a, b) => out.push(diff(
            DiffKind::Result,
            "",
            a.as_ref().unwrap_or(&Value::Null),
            b.as_ref().unwrap_or(&Value::Null),
        )),
    }
    let cats = |w: &[String]| -> BTreeSet<&'static str> { w.iter().map(|m| category(m)).collect() };
    let (qc, lc) = (cats(&q.warnings), cats(&l.warnings));
    for c in qc.symmetric_difference(&lc) {
        let text = |w: &[String]| {
            w.iter()
                .find(|m| category(m) == *c)
                .map_or(Value::Null, |m| Value::String(m.clone()))
        };
        out.push(diff(
            DiffKind::Warning,
            c,
            &text(&q.warnings),
            &text(&l.warnings),
        ));
    }
}

/// The error a step result says it threw.
fn error_of(v: &Value) -> Option<&str> {
    v.get("$err").map(|e| e.as_str().unwrap_or("?"))
}

/// Whether two serialised values are the same value.
#[must_use]
pub fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => match (x.as_f64(), y.as_f64()) {
            (Some(x), Some(y)) => close(x, y),
            _ => x == y,
        },
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            if let (Some(Value::String(p)), Some(Value::String(q))) =
                (x.get("$color"), y.get("$color"))
            {
                return p.eq_ignore_ascii_case(q);
            }
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

fn close(x: f64, y: f64) -> bool {
    if x == y {
        return true;
    }
    if x.is_sign_negative() != y.is_sign_negative() {
        return false;
    }
    x.to_bits().abs_diff(y.to_bits()) <= ULPS
}

/// A warning without what changes from case to case: where it is, the
/// names and numbers in it. Two warnings of one shape are, nearly always,
/// one behaviour.
#[must_use]
pub fn message_shape(text: &str) -> String {
    let line = text.lines().next().unwrap_or("");
    // `file:///…/Case.qml:9:5: ` in front, or a URL inside the message.
    let mut rest = line;
    if let Some(at) = rest.find(".qml:") {
        let after = &rest[at + 5..];
        let skipped = after.trim_start_matches(|c: char| c.is_ascii_digit() || c == ':');
        if !line[..at].contains(' ') {
            rest = skipped.trim_start();
        }
    }
    let mut out = String::with_capacity(rest.len());
    let mut words = rest.split(' ').peekable();
    while let Some(w) = words.next() {
        let w = if w.contains("://") { "_" } else { w };
        out.push_str(w);
        if words.peek().is_some() {
            out.push(' ');
        }
    }
    // An error's name is the kind of the warning already.
    if let Some((name, msg)) = out.split_once(": ")
        && name.ends_with("Error")
        && !name.contains(' ')
    {
        out = msg.to_owned();
    }
    let mut shape = String::with_capacity(out.len());
    let mut quote = None;
    let mut digits = false;
    for c in out.chars() {
        match quote {
            Some(q) if c == q => {
                shape.push('_');
                shape.push(c);
                quote = None;
            }
            Some(_) => {}
            None if c == '\'' || c == '"' => {
                shape.push(c);
                quote = Some(c);
            }
            None if c.is_ascii_digit() => {
                if !digits {
                    shape.push('N');
                }
                digits = true;
                continue;
            }
            None => shape.push(c),
        }
        digits = false;
    }
    shape
        .trim_end_matches(':')
        .trim()
        .chars()
        .take(80)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::*;

    fn ran(obs: Value, warnings: &[&str]) -> Outcome {
        let obs: BTreeMap<String, Value> = serde_json::from_value(obs).unwrap();
        Outcome::Ran(vec![Phase {
            obs,
            result: None,
            warnings: warnings.iter().map(|w| (*w).to_owned()).collect(),
        }])
    }

    #[test]
    fn numbers_agree_within_a_few_ulps() {
        let a = 0.1 + 0.2;
        assert!(same(&json!(a), &json!(0.3)));
        assert!(!same(
            &json!(0.5000076293945312),
            &json!(0.5019607843137255)
        ));
        assert!(same(&json!(6), &json!(6.0)));
        assert!(!same(&json!(0.0), &json!(-1e-300)));
    }

    #[test]
    fn colours_agree_whatever_the_case() {
        assert!(same(
            &json!({"$color": "#FF0000"}),
            &json!({"$color": "#ff0000"})
        ));
        assert!(!same(
            &json!({"$color": "#ff0000"}),
            &json!({"$color": "#fe0000"})
        ));
    }

    #[test]
    fn warnings_compare_by_category_not_wording() {
        let q = ran(
            json!({"root.a": 1}),
            &["Case.qml:3: QML Item: Binding loop detected for property \"a\""],
        );
        let l = ran(json!({"root.a": 1}), &["binding loop on root.a"]);
        assert_eq!(compare(&q, &l, &BTreeSet::new()), Verdict::Agree);
        let silent = ran(json!({"root.a": 1}), &[]);
        let Verdict::Differ(d) = compare(&q, &silent, &BTreeSet::new()) else {
            panic!("a missing warning is a difference")
        };
        assert_eq!(
            (d[0].kind, d[0].key.as_str()),
            (DiffKind::Warning, "binding-loop")
        );
    }

    #[test]
    fn a_rejection_by_qt_is_not_a_ljos_difference() {
        let q = Outcome::Rejected("Invalid property assignment".into());
        let l = ran(json!({}), &[]);
        assert!(matches!(
            compare(&q, &l, &BTreeSet::new()),
            Verdict::QtRejected(_)
        ));
    }

    #[test]
    fn unobserved_keys_are_left_out() {
        let q = ran(json!({"root.l0": 1, "root.p": 2}), &[]);
        let l = ran(json!({"root.l0": 9, "root.p": 2}), &[]);
        let skip = BTreeSet::from(["root.l0".to_owned()]);
        assert_eq!(compare(&q, &l, &skip), Verdict::Agree);
        assert!(matches!(
            compare(&q, &l, &BTreeSet::new()),
            Verdict::Differ(_)
        ));
    }

    #[test]
    fn errors_compare_by_kind() {
        let step = |r: Value| {
            Outcome::Ran(vec![Phase {
                obs: BTreeMap::new(),
                result: Some(r),
                warnings: vec![],
            }])
        };
        let none = BTreeSet::new();
        let te = step(json!({"$err": "TypeError"}));
        assert_eq!(compare(&te, &te.clone(), &none), Verdict::Agree);
        let Verdict::Differ(d) = compare(&te, &step(json!({"$err": "Error"})), &none) else {
            panic!()
        };
        assert_eq!(d[0].kind, DiffKind::ErrorKind);
        let Verdict::Differ(d) = compare(&te, &step(json!(1)), &none) else {
            panic!()
        };
        assert_eq!(d[0].kind, DiffKind::Throws);
    }

    #[test]
    fn a_warning_is_shaped_without_its_place_and_names() {
        assert_eq!(
            message_shape(
                "file:///w/c12/Case.qml:9: TypeError: Cannot call method 'toFixed' of undefined"
            ),
            "Cannot call method '_' of undefined"
        );
        assert_eq!(
            message_shape("file:///Case.qml:7:5: Unable to assign QString to int"),
            "Unable to assign QString to int"
        );
        assert_eq!(
            message_shape(
                "QQmlExpression: Expression file:///w/Case.qml:8:5 depends on non-bindable properties:\n  x"
            ),
            "QQmlExpression: Expression _ depends on non-bindable properties"
        );
        assert_eq!(
            message_shape("file:///Case.qml:4: RangeError: Invalid array length 42"),
            "Invalid array length N"
        );
    }
}
