//! The design audit (ARCHITECTURE.md section 14, "Audit lints", and the
//! design guardrails).
//!
//! [`audit`] looks at a mounted scene as it is laid out and drawn, the
//! way a user, an assistive technology and a keyboard meet it, and
//! reports each rule of `ljos_design` whose check is `Runtime` or `Both`
//! that the scene breaks: something to press that is off the window or
//! under something else, text cut off or too faint, a Tab order that
//! traps, a button with no name. [`rules::implemented`] lists the codes it
//! knows; their thresholds and wording are `ljos_design`'s.
//!
//! It runs only when asked (a test, `ljos audit`, the inspector's
//! `app_audit`), and looks at the scene under the settings of a
//! [`Matrix`]: as it is, or also in the other colour scheme, at high
//! contrast, with larger text and in a smaller window. Every setting it
//! changes it puts back; the scene is as it was afterwards, except that
//! what was animating when a full audit began has ended.
//!
//! Each finding names its rule, the item (role, name, a locator, the QML
//! file and line), where it is, what was measured, and the points of the
//! matrix where it shows.

mod apply;
mod cx;
mod finding;
mod matrix;
pub mod rules;
#[cfg(test)]
mod tests;

use ljos_design::limits::{TARGET_MIN_DESKTOP, TARGET_MIN_TOUCH};
use ljos_engine::{Engine, ObjectId};
use ljos_quick::QuickScene;
use ljos_runtime::Time;

pub use finding::{Bounds, Finding, Report, severity_word};
pub use ljos_design::{Area, Rule, Severity};
pub use matrix::{Matrix, MatrixPoint, PointError, PointSize};

/// What an audit was asked that it cannot do.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AuditError {
    #[error(
        "`{0}` is neither a rule the audit checks nor an area (a11y, color, text, layout, \
         focus, motion, content)"
    )]
    UnknownRule(String),
}

/// What the audit holds an application to, and what it looks at.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    /// The least width and height of something to press, in logical
    /// pixels: `ljos_design::limits::TARGET_MIN_DESKTOP` with a pointer,
    /// `TARGET_MIN_TOUCH` under a finger.
    pub target_size: f64,
    /// The colour of the window where nothing is painted. `None`: white,
    /// or black when the colour scheme is dark.
    pub backdrop: Option<[u8; 3]>,
    /// The settings the scene is audited under.
    pub matrix: Matrix,
    /// The rules to check, by code or by area (`layout`); empty for all.
    pub rules: Vec<String>,
    /// At most this many findings are reported; the report says how many
    /// more there were.
    pub max_findings: Option<usize>,
    /// Only findings about this item and what is inside it; the rest of
    /// the scene is still looked at (what covers the item may be outside
    /// it).
    pub within: Option<ObjectId>,
}

impl Options {
    /// A pointer and a keyboard, the scene as it is.
    pub fn desktop() -> Self {
        Options {
            target_size: f64::from(TARGET_MIN_DESKTOP),
            backdrop: None,
            matrix: Matrix::Current,
            rules: Vec::new(),
            max_findings: None,
            within: None,
        }
    }

    /// A finger.
    pub fn mobile() -> Self {
        Options {
            target_size: f64::from(TARGET_MIN_TOUCH),
            ..Options::desktop()
        }
    }

    /// Only these rules or areas. `Err` names one that is neither.
    pub fn only(mut self, rules: &[impl AsRef<str>]) -> Result<Self, AuditError> {
        for r in rules {
            let r = r.as_ref();
            let known = rules::implemented().contains(&r) || area_named(r);
            if !known {
                return Err(AuditError::UnknownRule(r.to_string()));
            }
            self.rules.push(r.to_string());
        }
        Ok(self)
    }

    fn wants(&self, code: &str) -> bool {
        if self.rules.is_empty() {
            return true;
        }
        let area = ljos_design::rule(code).map(|r| area_word(r.area));
        self.rules
            .iter()
            .any(|r| r == code || Some(r.as_str()) == area)
    }
}

impl Default for Options {
    fn default() -> Self {
        Options::desktop()
    }
}

/// An area as serde writes it, and as the codes of all but `meta` (whose
/// rules are about the checks, `lint-*`) begin with it.
pub fn area_word(area: Area) -> &'static str {
    match area {
        Area::A11y => "a11y",
        Area::Color => "color",
        Area::Text => "text",
        Area::Layout => "layout",
        Area::Focus => "focus",
        Area::Motion => "motion",
        Area::Content => "content",
        Area::Meta => "meta",
    }
}

fn area_named(name: &str) -> bool {
    ljos_design::RULES.iter().any(|r| area_word(r.area) == name)
}

/// The findings at the scene as it is now.
fn run_checks(engine: &Engine, scene: &QuickScene, options: &Options) -> Vec<Finding> {
    let mut cx = cx::Cx::new(engine, scene, options);
    for check in rules::CHECKS {
        if check.codes.iter().any(|c| options.wants(c)) {
            (check.run)(&mut cx);
        }
    }
    let mut out = cx.out;
    out.retain(|f| {
        options.wants(f.code)
            && options
                .within
                .is_none_or(|root| f.item.is_some_and(|i| cx::within(engine, i, root)))
    });
    out
}

/// Adds what was found at `point` to `all`: a finding that showed at an
/// earlier point too gets this point added instead of a second copy.
fn merge(all: &mut Vec<Finding>, found: Vec<Finding>, point: MatrixPoint, many: bool) {
    for mut f in found {
        if !many {
            all.push(f);
            continue;
        }
        let same = all.iter_mut().find(|g| {
            g.code == f.code
                && g.item == f.item
                && g.place == f.place
                && (f.item.is_some() || g.message == f.message)
                && !g.points.contains(&point)
        });
        match same {
            Some(g) => g.points.push(point),
            None => {
                f.points = vec![point];
                all.push(f);
            }
        }
    }
}

/// Audits the scene at `now` (the host's clock, as for
/// [`QuickScene::frame`]) at each point of `options.matrix`.
///
/// The accessibility tree is what the audit reads, so it is turned on for
/// the length of the audit when it is off, and a frame is run when the
/// scene has none to show or is waiting for one.
pub fn audit(engine: &Engine, scene: &mut QuickScene, now: Time, options: &Options) -> Report {
    let was_active = scene.accessibility_active();
    if !was_active {
        scene.set_accessibility_active(engine, true);
    }
    if !was_active || scene.needs_frame() || scene.snapshot().is_none() {
        scene.frame(engine, now);
    }
    let points = options.matrix.points();
    // Findings say their points unless the audit was of the scene as it
    // is alone, where there is nothing to tell apart.
    let many = points.len() > 1 || points.first().is_some_and(|p| !p.is_current());
    let mut findings = Vec::new();
    let mut audited = Vec::new();
    let mut held = None;
    // What a check that panics leaves is put back all the same, and the
    // panic goes on.
    let walked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut seen = Vec::new();
        for (i, point) in points.into_iter().enumerate() {
            // The scene as it is needs nothing put in or back.
            if i == 0 && point.is_current() {
                merge(
                    &mut findings,
                    run_checks(engine, scene, options),
                    point,
                    many,
                );
                audited.push(point);
                continue;
            }
            let h = held.get_or_insert_with(|| {
                let h = apply::hold(engine, scene);
                if audited.first().is_some_and(MatrixPoint::is_current) {
                    seen.push(h.current());
                }
                h
            });
            let to = apply::resolve(engine, scene, h, &point);
            if seen.contains(&to) {
                continue;
            }
            seen.push(to);
            apply::apply(engine, scene, now, h, &to);
            merge(
                &mut findings,
                run_checks(engine, scene, options),
                point,
                many,
            );
            audited.push(point);
        }
    }));
    if let Some(h) = held {
        apply::restore(engine, scene, now, h);
    }
    if !was_active {
        scene.set_accessibility_active(engine, false);
    }
    if let Err(panic) = walked {
        std::panic::resume_unwind(panic);
    }
    let mut left_out = 0;
    if let Some(max) = options.max_findings
        && findings.len() > max
    {
        left_out = findings.len() - max;
        findings.truncate(max);
    }
    Report {
        findings,
        points: audited,
        left_out,
    }
}
