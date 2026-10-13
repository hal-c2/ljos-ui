//! The real [`FrontEnd`]: `qml_check`, `qml_lint` and `type_lookup` over
//! `qml_syntax`, `qml_front` and the `qt_types` registry.
//!
//! # Principle
//!
//! Agents trust a clean result, so everything here reports only what the
//! front end can decide with confidence. Where it cannot (an import it does
//! not know, a type whose members the Qt table does not carry, a script
//! inside a `Connections`), it stays silent about that object rather than
//! guess. A missed problem is cheaper than a false alarm an agent "fixes".
//!
//! # Diagnostic codes
//!
//! Emitted by `qml_check` and `qml_lint` alike. The ones `qml_front` raises
//! carry its own `Code` (`Code::as_str`), so the strings below are the
//! front end's, not a reading of its messages. Lines and columns are 1-based
//! (columns count characters).
//!
//! | Code | Severity | Meaning |
//! |---|---|---|
//! | `io` | error | The file could not be read as UTF-8 text. |
//! | `syntax` | error | The parser rejected the file. The file is then not analysed further. |
//! | `unknown-module` | warning | An `import` names a module that neither Qt's table nor the project's `qmldir`/CMake files know. Type errors in that file are held back, because the module may provide the type. |
//! | `unknown-type` | error | An object type, or the type an attached property is written on (`Layout.fillWidth`), is not provided by anything the file imports. |
//! | `unknown-property` | error | A binding names a property the object's type definitely does not have. Held back where the type chain is not fully known, including a component whose file did not parse whole. |
//! | `unknown-signal` | error | A handler (`onFoo`) for a signal the object's type definitely does not have (same holds as above). |
//! | `duplicate-id` | error | Two objects of one component share an `id`. |
//! | `unknown-alias-target` | error | A `property alias` points at an id the component does not have. |
//! | `unresolved-name` | warning | A free name in a binding that QML's lookup order cannot find, and that nothing else in the project provides. Warning, not error: a delegate's model role or a name the creating component supplies cannot be seen from one file. |
//! | `semantic` | error | Any other error the front end's analysis raises that has no code of its own yet; the message says what. |
//!
//! # Lint rules (`qml_lint` only)
//!
//! | Code | Severity | Rule |
//! |---|---|---|
//! | `a11y-unnamed-interactive` | warning | A `MouseArea` or `TapHandler` that handles clicks or taps, with no `Accessible.name` or `Accessible.ignored` on it or its parent object. |
//! | `a11y-unnamed-icon-button` | warning | A Controls button (`Button`, `ToolButton`, ...) given an icon and no `text`, `action`, `contentItem` or `Accessible.name`. |
//! | `a11y-unnamed-image` | warning | An `Image` with neither `Accessible.name` nor `Accessible.ignored`, not inside a named parent or a button. |
//! | `i18n-untranslated-string` | hint | A literal string bound to `text`, `title`, `placeholderText`, `ToolTip.text` or `Accessible.name`/`description` and not wrapped in `qsTr`. Literals without a letter are ignored. |
//! | `i18n-qstr-non-literal` | warning | `qsTr(...)` whose first argument is not a literal, which the translation tools cannot extract. |
//!
//! # Known limits
//!
//! Types that Qt writes as QML files (the Controls styles) have no members of
//! their own in the table, only the ones they inherit; an extra property a
//! style adds would be reported as unknown. Names that only a model provides
//! to a delegate (`display`, roles) are seen as unresolved when the delegate
//! is its own file. Both are why those cases are warnings or held back.

mod check;
mod facts;
mod lint;
mod load;
mod lookup;
mod usage;

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use ljos_mcp_core::SurfaceError;

pub use usage::{ApiUse, Resolution, Usage};

use crate::{Diagnostic, FrontEnd, ModuleInfo, Severity, TypeInfo};

/// The analysis behind the authoring tools. It holds only the project root,
/// so a call always sees the files as they are on disk now (an agent edits
/// between calls); nothing is cached.
#[derive(Clone, Debug)]
pub struct QmlFrontEnd {
    root: PathBuf,
}

impl QmlFrontEnd {
    /// `root` is the canonical project directory; module discovery
    /// (`qmldir`, `CMakeLists.txt`) and type keys are relative to it.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Every type and member of a module that the `.qml` files below the
    /// root use (`ljos check --port`). Whether Ljós has them is for
    /// [`api::status`](crate::api::status) to say.
    pub fn api_uses(&self) -> Usage {
        usage::collect(&self.root)
    }

    fn analyse(&self, files: &[PathBuf], lint: bool) -> Vec<Diagnostic> {
        check::run(&self.root, files, lint)
    }
}

impl FrontEnd for QmlFrontEnd {
    fn check(&self, files: &[PathBuf]) -> Result<Vec<Diagnostic>, SurfaceError> {
        Ok(self.analyse(files, false))
    }

    fn lint(&self, files: &[PathBuf]) -> Result<Vec<Diagnostic>, SurfaceError> {
        Ok(self.analyse(files, true))
    }

    fn type_lookup(&self, module: &str, name: &str) -> Result<TypeInfo, SurfaceError> {
        lookup::type_info(module, name)
    }

    fn module_types(&self, module: &str) -> Result<ModuleInfo, SurfaceError> {
        lookup::module_info(module)
    }
}

pub(crate) fn diagnostic(
    path: &Path,
    source: &str,
    offset: u32,
    severity: Severity,
    code: &str,
    message: impl Into<String>,
) -> Diagnostic {
    let (line, column) = qml_syntax::position(source, offset);
    Diagnostic {
        file: path.to_string_lossy().into_owned(),
        line: line as u32,
        column: column as u32,
        severity,
        code: code.to_owned(),
        message: message.into(),
    }
}

/// The candidate closest to `name`, if one is close enough to be a typo:
/// within a third of the name's length (at least one edit).
pub(crate) fn closest<'c>(
    name: &str,
    candidates: impl IntoIterator<Item = &'c str>,
) -> Option<&'c str> {
    let limit = (name.chars().count() / 3).max(1);
    candidates
        .into_iter()
        .filter(|c| *c != name)
        .map(|c| (edit_distance(name, c), c))
        .filter(|(d, _)| *d <= limit)
        // Ties go to the alphabetically first so the answer is stable.
        .min_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(b.1)))
        .map(|(_, c)| c)
}

/// Up to `limit` names closest to `name`, nearest first.
pub(crate) fn nearest<'c>(
    name: &str,
    candidates: impl IntoIterator<Item = &'c str>,
    limit: usize,
) -> Vec<&'c str> {
    let name_lower = name.to_lowercase();
    let mut scored: BTreeSet<(usize, &str)> = BTreeSet::new();
    for c in candidates {
        let c_lower = c.to_lowercase();
        let mut d = edit_distance(&name_lower, &c_lower);
        // A shared prefix or substring is a better hint than a small edit
        // distance on short names.
        if c_lower.contains(&name_lower) || name_lower.contains(&c_lower) {
            d = d.min(1);
        }
        if d <= (name.chars().count() / 2).max(2) {
            scored.insert((d, c));
        }
    }
    scored.into_iter().take(limit).map(|(_, c)| c).collect()
}

/// Optimal string alignment distance: an insertion, deletion, substitution or
/// swap of neighbours each cost one, because swapped letters (`widht`) are the
/// typo people make most.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut rows = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in rows.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in rows[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut d = (rows[i - 1][j] + 1)
                .min(rows[i][j - 1] + 1)
                .min(rows[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d = d.min(rows[i - 2][j - 2] + 1);
            }
            rows[i][j] = d;
        }
    }
    rows[a.len()][b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closest_finds_typos_and_ignores_far_names() {
        assert_eq!(closest("colr", ["color", "radius", "width"]), Some("color"));
        assert_eq!(closest("widht", ["width", "height"]), Some("width"));
        assert_eq!(closest("zzzz", ["color", "radius"]), None);
        // An exact match is not a suggestion.
        assert_eq!(closest("color", ["color"]), None);
    }

    #[test]
    fn nearest_orders_by_distance_and_caps() {
        let got = nearest(
            "Rectangl",
            ["Rectangle", "Text", "RectangleItem", "Image"],
            2,
        );
        assert_eq!(got, ["Rectangle", "RectangleItem"]);
    }
}
