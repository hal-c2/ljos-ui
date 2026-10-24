//! What one run of a case shows, on either side, and the categories its
//! warnings fall in.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};

use regex::Regex;
use serde_json::Value;

/// A case as the runners see it: one QML file whose root has the
/// observation and step functions (see [`crate::bindings::print`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rendered {
    pub qml: String,
    pub steps: usize,
    /// Observed keys whose values are not compared.
    pub unobserved: BTreeSet<String>,
    /// The declared type of each observed key (`int`, `list<real>`).
    pub types: BTreeMap<String, String>,
}

/// What a case did on one side.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// The file did not load: a component error, with its text.
    Rejected(String),
    /// The program crashed, hung or panicked while running the case.
    Crashed(String),
    /// Phase 0 is the load, phase `i` the `i`th step.
    Ran(Vec<Phase>),
}

/// The observations after one phase.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Phase {
    /// `"id.prop"` to the value's serialised form.
    pub obs: BTreeMap<String, Value>,
    /// The step's own result (`{"$u":1}` for nothing, `{"$err":name}` when
    /// it threw); `None` for the load.
    pub result: Option<Value>,
    /// The warnings logged during the phase, in order.
    pub warnings: Vec<String>,
}

impl Phase {
    /// The phase from the JSON strings the case's functions return.
    ///
    /// # Errors
    /// When either is not the JSON the case's helpers write.
    pub fn parse(obs: &str, result: Option<&str>, warnings: Vec<String>) -> Result<Self, String> {
        let obs: BTreeMap<String, Value> =
            serde_json::from_str(obs).map_err(|e| format!("bad observation {obs:?}: {e}"))?;
        let result = result
            .map(|r| serde_json::from_str(r).map_err(|e| format!("bad step result {r:?}: {e}")))
            .transpose()?;
        Ok(Self {
            obs,
            result,
            warnings,
        })
    }
}

/// What a warning is about, which is what the two runtimes are compared on:
/// their wording differs and is not the point.
#[must_use]
pub fn category(message: &str) -> &'static str {
    static KINDS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
        [
            (r"(?i)binding loop", "binding-loop"),
            (r"(?i)read-?only", "readonly"),
            (r"TypeError", "TypeError"),
            (r"ReferenceError", "ReferenceError"),
            (r"RangeError", "RangeError"),
            (r"SyntaxError", "SyntaxError"),
            (r"URIError", "URIError"),
            (r"(?i)(unable|cannot|can't) (to )?assign", "assign"),
            // Qt's words for a value a value type cannot be made from.
            (r"Could not find any constructor for value type", "assign"),
            (r"should be coerced to void", "coerce-void"),
            (r"\bError\b", "Error"),
        ]
        .into_iter()
        .map(|(re, kind)| (Regex::new(re).expect("the patterns are valid"), kind))
        .collect()
    });
    KINDS
        .iter()
        .find(|(re, _)| re.is_match(message))
        .map_or("other", |(_, kind)| kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warnings_fall_in_categories_by_what_they_say() {
        let qt = "file:///x/Case.qml:5:5: QML Item: Binding loop detected for property \"a\"";
        assert_eq!(category(qt), "binding-loop");
        assert_eq!(category("Case.qml:3: binding loop on a"), "binding-loop");
        assert_eq!(
            category("Case.qml:2: Unable to assign QString to int"),
            "assign"
        );
        assert_eq!(
            category("Case.qml:4: TypeError: Cannot read property 'x' of undefined"),
            "TypeError"
        );
        assert_eq!(category("something else"), "other");
    }

    #[test]
    fn a_phase_reads_the_helpers_json() {
        let p = Phase::parse(r#"{"root.a":1}"#, Some(r#"{"$u":1}"#), vec![]).unwrap();
        assert_eq!(p.obs["root.a"], Value::from(1));
        assert!(Phase::parse("nope", None, vec![]).is_err());
    }
}
