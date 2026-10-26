//! `app_audit`: the design audit (`ljos_audit`) on the running
//! application, as it is or under a matrix of settings.
//!
//! The audit runs on the UI thread between frames the host shows: a point
//! of the matrix is put in, settled, looked at and taken out again before
//! the host draws, so the user sees nothing of it, and the application is
//! as it was afterwards, a check that fails included.
//!
//! Each finding in the JSON is what a UI needs to list it and draw a box
//! where it is: the rule (code, severity, fix, guide), the item's role,
//! name and a locator `app_find` and the acting tools take, its QML
//! `file:line`, its bounds in the window, what was measured, and the
//! points it shows at.

use ljos_audit::{Matrix, Options};
use ljos_engine::{Engine, ObjectId};
use ljos_mcp_core::{SurfaceError, ToolReply};
use ljos_quick::QuickScene;
use ljos_runtime::Time;
use schemars::JsonSchema;
use serde::Deserialize;

pub const AUDIT: &str = "\
The design audit of what the app shows: off-window, covered, cut or faint text, small targets, \
popups that do not fit, unnamed controls. Each finding: rule, fix, role, locator, file:line, \
bounds. `matrix`: current, full or points; all is put back. `act`: also press and toggle, \
running app handlers.";

/// The most findings an answer lists unless told otherwise.
const DEFAULT_MAX: usize = 100;
/// The most it lists at all.
const MOST: usize = 500;

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuditArgs {
    /// Only findings about this item and what is inside it: a locator or
    /// a reference, as `app_find` takes. The rest of the window is still
    /// looked at (what covers the item may lie outside it).
    #[serde(default)]
    pub root: Option<String>,
    /// The settings to audit under: `current` (the app as it is, the
    /// default), `full` (also light and dark at normal and high contrast,
    /// larger text and the small window), or points separated by `;`,
    /// each `scheme=light|dark contrast=normal|high font-scale=<n>
    /// size=<w>x<h>|small reduced-motion=true|false`.
    #[serde(default)]
    pub matrix: Option<String>,
    /// Only these rules, by code (`layout-offscreen`) or area (`a11y`,
    /// `color`, `text`, `layout`, `focus`, `motion`, `content`).
    #[serde(default)]
    pub rules: Vec<String>,
    /// Hold what can be pressed to the size a finger needs, not a
    /// pointer's.
    #[serde(default)]
    pub touch: bool,
    /// The most findings to list (default 100, at most 500); the answer
    /// says how many more there were.
    #[serde(default)]
    pub max_findings: Option<usize>,
    /// Also run the checks that act (focus each stop, press Escape,
    /// toggle controls, open popups): they run the application's own
    /// handlers, whose effects are real. Needs the `input` capability.
    /// Off, the answer's `notChecked` names the codes left out.
    #[serde(default)]
    pub act: bool,
}

impl AuditArgs {
    /// The options, or what is wrong with the arguments.
    pub(crate) fn options(&self) -> Result<Options, SurfaceError> {
        let bad = |why: String| SurfaceError::invalid_argument(format!("`app_audit`: {why}"));
        let matrix: Matrix = self
            .matrix
            .as_deref()
            .unwrap_or("current")
            .parse()
            .map_err(|e| bad(format!("{e}")))?;
        let max = self.max_findings.unwrap_or(DEFAULT_MAX);
        if max == 0 {
            return Err(bad("`max_findings` is at least 1".into()));
        }
        let base = if self.touch {
            Options::mobile()
        } else {
            Options::desktop()
        };
        let options = Options {
            matrix,
            max_findings: Some(max.min(MOST)),
            // A running application's handlers do real things: acting is
            // its user's to ask for.
            act: self.act,
            ..base
        };
        options.only(&self.rules).map_err(|e| bad(format!("{e}")))
    }
}

/// Audits the scene, only what is inside `root` when it is given, and
/// says the time the scene was last framed at ([`ljos_audit::Report::until`]).
pub(crate) fn audit(
    engine: &Engine,
    scene: &mut QuickScene,
    now: Time,
    mut options: Options,
    root: Option<ObjectId>,
) -> (ToolReply, Time) {
    options.within = root;
    let report = ljos_audit::audit(engine, scene, now, &options);
    let value = report.json();
    let counts = format!(
        "{} error(s), {} warning(s), {} hint(s)",
        value["counts"]["error"], value["counts"]["warning"], value["counts"]["hint"]
    );
    let points: Vec<String> = report.points.iter().map(ToString::to_string).collect();
    let mut text = report.by_rule("");
    if report.is_clean() {
        text.push_str("Nothing found. ");
    }
    text.push_str(&format!("{counts} at {}", points.join("; ")));
    (ToolReply::from(value).with_text(text), report.until)
}
