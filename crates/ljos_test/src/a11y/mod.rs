//! The accessibility audit (ARCHITECTURE.md section 14, "Audit lints").
//!
//! [`audit`] looks at a mounted scene the way an assistive technology and
//! a keyboard meet it, and reports what would leave a user stuck:
//!
//! | Code | What is wrong |
//! |---|---|
//! | `a11y-unnamed-interactive` | Something a user acts on has no name. |
//! | `a11y-unnamed-image` | A picture is neither named nor left out. |
//! | `a11y-duplicate-name` | Two things to act on, side by side, have one name. |
//! | `a11y-target-size` | Something to press is smaller than a finger or a pointer needs. |
//! | `a11y-tab-trap` | Tab goes round without coming back to where it began. |
//! | `a11y-tab-unreachable` | Something takes the keyboard and Tab never gets there. |
//! | `a11y-contrast` | Text is too close in colour to what is behind it. |
//! | `a11y-set-size` | A row does not say which row it is, among rows that do. |
//!
//! The codes are stable: a test may allow one by name. Each finding names
//! the item and the QML file and line that made it.
//!
//! The audit reads and presses nothing: the scene is as it was afterwards,
//! except for the one frame it may run to have something to read.

mod checks;
mod contrast;
mod tab;
#[cfg(test)]
mod tests;

use std::fmt;

use ljos_a11y::{NodeData, NodeKey, role_word};
use ljos_engine::{Engine, ObjectId};
use ljos_quick::QuickScene;
use ljos_runtime::Time;

/// What a finding is about. [`Code::as_str`] is the stable name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Code {
    UnnamedInteractive,
    UnnamedImage,
    DuplicateName,
    TargetSize,
    TabTrap,
    TabUnreachable,
    Contrast,
    SetSize,
}

impl Code {
    pub const ALL: [Code; 8] = [
        Code::UnnamedInteractive,
        Code::UnnamedImage,
        Code::DuplicateName,
        Code::TargetSize,
        Code::TabTrap,
        Code::TabUnreachable,
        Code::Contrast,
        Code::SetSize,
    ];

    /// The name tests and tools know the check by. The first two are the
    /// codes of the static lints of `ljos check` for the same mistake.
    pub fn as_str(self) -> &'static str {
        match self {
            Code::UnnamedInteractive => "a11y-unnamed-interactive",
            Code::UnnamedImage => "a11y-unnamed-image",
            Code::DuplicateName => "a11y-duplicate-name",
            Code::TargetSize => "a11y-target-size",
            Code::TabTrap => "a11y-tab-trap",
            Code::TabUnreachable => "a11y-tab-unreachable",
            Code::Contrast => "a11y-contrast",
            Code::SetSize => "a11y-set-size",
        }
    }

    pub fn from_name(name: &str) -> Option<Code> {
        Code::ALL.into_iter().find(|c| c.as_str() == name)
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One thing the audit found.
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub code: Code,
    /// The item, when it is still there to ask.
    pub item: Option<ObjectId>,
    /// `file:line:column` of the QML that made the item; `None` for an
    /// item made in Rust.
    pub place: Option<String>,
    /// What is wrong and what to do, starting with what the item is.
    pub message: String,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(place) = &self.place {
            write!(f, "{place}: ")?;
        }
        write!(f, "[{}] {}", self.code, self.message)
    }
}

/// What an audit found, in the order of the table above.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub findings: Vec<Finding>,
}

impl Report {
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }

    /// The findings of one check.
    pub fn of(&self, code: Code) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(move |f| f.code == code)
    }

    /// The codes found, each once.
    pub fn codes(&self) -> Vec<Code> {
        let mut codes: Vec<Code> = self.findings.iter().map(|f| f.code).collect();
        codes.sort();
        codes.dedup();
        codes
    }

    /// The report without the findings of the checks a test allows.
    pub fn without(mut self, allowed: &[Code]) -> Report {
        self.findings.retain(|f| !allowed.contains(&f.code));
        self
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

/// What the audit holds an application to.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    /// The least width and height of something to press, in logical
    /// pixels: 24 with a pointer, 44 under a finger.
    pub target_size: f64,
    /// The least contrast of text with what is behind it.
    pub contrast: f64,
    /// The least contrast of text of [`Options::large_text`] and more,
    /// which is as easy to read with less.
    pub contrast_large: f64,
    /// The font size, in logical pixels, from which text is large.
    pub large_text: f64,
    /// The colour of the window where nothing is painted. `None`: white,
    /// or black when the colour scheme is dark.
    pub backdrop: Option<[u8; 3]>,
}

impl Options {
    /// A pointer and a keyboard.
    pub fn desktop() -> Self {
        Options {
            target_size: 24.0,
            contrast: 4.5,
            contrast_large: 3.0,
            large_text: 24.0,
            backdrop: None,
        }
    }

    /// A finger.
    pub fn mobile() -> Self {
        Options {
            target_size: 44.0,
            ..Options::desktop()
        }
    }
}

impl Default for Options {
    fn default() -> Self {
        Options::desktop()
    }
}

/// What the checks share.
struct Cx<'a> {
    engine: &'a Engine,
    scene: &'a QuickScene,
    options: &'a Options,
    out: Vec<Finding>,
}

impl Cx<'_> {
    fn report(&mut self, code: Code, item: Option<ObjectId>, message: String) {
        let place = item.and_then(|i| place_of(self.engine, i));
        self.out.push(Finding {
            code,
            item,
            place,
            message,
        });
    }

    /// The item behind a node.
    fn item(&self, key: NodeKey) -> Option<ObjectId> {
        self.scene.accessible_item(key)
    }
}

/// `file:line:column` of the QML that made the item.
fn place_of(engine: &Engine, item: ObjectId) -> Option<String> {
    let at = engine.declared_at(item)?;
    let at = at.trim_end().trim_end_matches(':');
    (!at.is_empty()).then(|| at.to_string())
}

/// A node as a message names it: `button "Save"`.
fn node_words(data: &NodeData) -> String {
    let role = role_word(data.role);
    if data.name.is_empty() {
        format!("the {role} with no name")
    } else {
        format!("the {role} {:?}", data.name)
    }
}

/// An item with no node as a message names it: by its `objectName` or
/// `id`, else by its type.
fn item_words(engine: &Engine, item: ObjectId) -> String {
    let ty = engine
        .meta_of(item)
        .map(|m| m.name.to_string())
        .unwrap_or_else(|| "item".into());
    let name = engine.read_by_name(item, "objectName").to_js_string();
    if name.is_empty() {
        format!("the {ty}")
    } else {
        format!("the {ty} {:?}", name.as_str())
    }
}

/// Audits the scene as it is at `now` (the host's clock, as for
/// [`QuickScene::frame`]).
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
    let mut cx = Cx {
        engine,
        scene,
        options,
        out: Vec::new(),
    };
    checks::names(&mut cx);
    checks::images(&mut cx);
    checks::duplicates(&mut cx);
    checks::target_size(&mut cx);
    tab::walk(&mut cx);
    contrast::text(&mut cx);
    checks::sets(&mut cx);
    let findings = cx.out;
    if !was_active {
        scene.set_accessibility_active(engine, false);
    }
    Report { findings }
}
