//! What the audit found, and the report of it.

use std::fmt;
use std::fmt::Write as _;

use ljos_design::{Rule, Severity};
use ljos_engine::ObjectId;
use serde_json::{Value, json};

use crate::matrix::MatrixPoint;

/// Where an item is on the window, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Bounds {
    fn json(self) -> Value {
        let n = |v: f64| (v * 10.0).round() / 10.0;
        json!({"x": n(self.x), "y": n(self.y), "width": n(self.width), "height": n(self.height)})
    }
}

/// One thing the audit found.
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    /// The rule's code, as `ljos_design` has it: `layout-offscreen`.
    pub code: &'static str,
    pub rule: &'static Rule,
    /// The item, when it is still there to ask.
    pub item: Option<ObjectId>,
    /// `file:line:column` of the QML that made the item; `None` for an
    /// item made in Rust.
    pub place: Option<String>,
    /// The item's accessible role (`button`) and name, when it has a node.
    pub role: Option<String>,
    pub name: Option<String>,
    /// A locator `app_find` takes to find the item again.
    pub locator: Option<String>,
    pub bounds: Option<Bounds>,
    /// What was measured against what the rule allows:
    /// `2.9:1 < 4.5:1`, `text needs 212 px, has 160`.
    pub measured: Option<String>,
    /// What is wrong and what to do, starting with what the item is.
    pub message: String,
    /// Where in the matrix it shows; empty for an audit of the scene as
    /// it is.
    pub points: Vec<MatrixPoint>,
}

impl Finding {
    pub fn severity(&self) -> Severity {
        self.rule.severity
    }

    /// The finding for a tool or a UI to list and highlight.
    pub fn json(&self) -> Value {
        let mut v = json!({
            "code": self.code,
            "severity": self.rule.severity,
            "area": self.rule.area,
            "message": self.message,
            "fix": self.rule.fix,
            "guide": self.rule.guide(),
        });
        let o = v.as_object_mut().expect("an object");
        let mut put = |k: &str, x: Option<Value>| {
            if let Some(x) = x {
                o.insert(k.into(), x);
            }
        };
        put("role", self.role.clone().map(Value::from));
        put("name", self.name.clone().map(Value::from));
        put("locator", self.locator.clone().map(Value::from));
        put("source", self.place.clone().map(Value::from));
        put("bounds", self.bounds.map(Bounds::json));
        put("measured", self.measured.clone().map(Value::from));
        if !self.points.is_empty() {
            o.insert(
                "points".into(),
                self.points.iter().map(|p| p.to_string()).collect(),
            );
        }
        v
    }
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(place) = &self.place {
            write!(f, "{place}: ")?;
        }
        write!(f, "[{}] {}", self.code, self.message)
    }
}

/// What an audit found, each rule's findings in the order the rules run.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub findings: Vec<Finding>,
    /// The points of the matrix the scene was audited at.
    pub points: Vec<MatrixPoint>,
    /// How many findings `max_findings` left out.
    pub left_out: usize,
    /// The modules standing in as empty Items (`QtGraphs`): what they
    /// show is not audited, and nothing is held against what lies on it.
    pub unseen: Vec<String>,
    /// The scene's clock when the audit was done: `now`, or later when
    /// the acting checks let animations run. The scene was last framed at
    /// this time; a host that frames it with a clock of its own moves that
    /// clock here first, so that time does not run backwards for it.
    pub until: ljos_runtime::Time,
    /// The codes asked for that were not checked because the audit was
    /// not let act ([`crate::Options::act`]), sorted: no finding of one
    /// says nothing about it.
    pub not_checked: Vec<&'static str>,
}

impl Report {
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }

    /// The findings of one rule.
    pub fn of<'a>(&'a self, code: &'a str) -> impl Iterator<Item = &'a Finding> + 'a {
        self.findings.iter().filter(move |f| f.code == code)
    }

    /// The codes found, each once, sorted.
    pub fn codes(&self) -> Vec<&'static str> {
        let mut codes: Vec<&'static str> = self.findings.iter().map(|f| f.code).collect();
        codes.sort_unstable();
        codes.dedup();
        codes
    }

    /// The report without the findings of the rules a test allows.
    pub fn without(mut self, allowed: &[&str]) -> Report {
        self.findings.retain(|f| !allowed.contains(&f.code));
        self
    }

    /// How many findings have this severity.
    pub fn count(&self, severity: Severity) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity() == severity)
            .count()
    }

    /// The report for a tool: the findings, how many of each severity,
    /// and the points audited.
    pub fn json(&self) -> Value {
        json!({
            "findings": self.findings.iter().map(Finding::json).collect::<Vec<_>>(),
            "counts": {
                "error": self.count(Severity::Error),
                "warning": self.count(Severity::Warning),
                "hint": self.count(Severity::Hint),
            },
            "points": self.points.iter().map(|p| p.to_string()).collect::<Vec<_>>(),
            "leftOut": self.left_out,
            "unseen": self.unseen,
            "notChecked": self.not_checked,
        })
    }
}

/// A severity as a report writes it.
pub fn severity_word(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Hint => "hint",
    }
}

impl Report {
    /// The report for a person: the findings of each rule under the
    /// rule, with its severity, what to do and where the guide says
    /// more; each finding at its `file:line`, with what was measured and
    /// the points it shows at. `strip` is taken off the front of each
    /// place (the project's directory).
    pub fn by_rule(&self, strip: &str) -> String {
        let mut out = String::new();
        let mut codes: Vec<&'static str> = Vec::new();
        for f in &self.findings {
            if !codes.contains(&f.code) {
                codes.push(f.code);
            }
        }
        for code in codes {
            let mut of = self.of(code).peekable();
            let Some(first) = of.peek() else { continue };
            let rule = first.rule;
            let _ = writeln!(
                out,
                "{code} ({}): {}\n  fix: {}\n  guide: {}",
                severity_word(rule.severity),
                rule.rule,
                rule.fix,
                rule.guide()
            );
            for f in of {
                let place = f
                    .place
                    .as_deref()
                    .map(|p| p.strip_prefix(strip).unwrap_or(p).trim_start_matches('/'))
                    .unwrap_or("(no source)");
                let _ = writeln!(out, "  {place}: {}", f.message);
                let mut more = Vec::new();
                if let Some(m) = &f.measured {
                    more.push(m.clone());
                }
                if let Some(l) = &f.locator {
                    more.push(l.clone());
                }
                if !f.points.is_empty() {
                    let points: Vec<String> = f.points.iter().map(ToString::to_string).collect();
                    more.push(format!("at {}", points.join("; ")));
                }
                if !more.is_empty() {
                    let _ = writeln!(out, "    {}", more.join(" | "));
                }
            }
        }
        if !self.unseen.is_empty() {
            let _ = writeln!(
                out,
                "Not audited: what the items of {} show; Ljós does not have the module, and \
                 they stand in as empty Items.",
                self.unseen.join(", ")
            );
        }
        if !self.not_checked.is_empty() {
            let _ = writeln!(
                out,
                "Not checked: {}; these checks press keys, toggle controls and run the \
                 application's handlers, which this audit was not asked to do.",
                self.not_checked.join(", ")
            );
        }
        if self.left_out > 0 {
            let _ = writeln!(out, "({} more not shown)", self.left_out);
        }
        out
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for finding in &self.findings {
            writeln!(f, "{finding}")?;
        }
        Ok(())
    }
}
