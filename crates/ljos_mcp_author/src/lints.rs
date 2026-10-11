//! The lint rules, once: their codes, how serious each is, and who reports
//! it. `qml_lint` reports all of them; `ljos check` reports the ones a
//! reader of the application pays for. Both read this table, so a finding
//! has the same code in a terminal, in an agent's answer and in an editor.

use schemars::JsonSchema;
use serde::Serialize;

use crate::Severity;

/// One rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Lint {
    /// What a finding is tagged with: `[a11y-unnamed-image]`.
    pub code: &'static str,
    pub severity: Severity,
    /// What is wrong where the rule finds something.
    pub rule: &'static str,
    /// What to write so that it no longer does.
    pub fix: &'static str,
    /// Whether `ljos check` reports it too. A rule it leaves out is advice,
    /// not a fault.
    pub in_check: bool,
}

pub const UNNAMED_INTERACTIVE: Lint = Lint {
    code: "a11y-unnamed-interactive",
    severity: Severity::Warning,
    rule: "A `MouseArea` or a `TapHandler` reacts to a click or a tap, and neither it nor the \
           item it is in has an accessible name.",
    fix: "Set `Accessible.role` and `Accessible.name` on it or on the item it is in, or \
          `Accessible.ignored: true` if it is only decoration.",
    in_check: true,
};

pub const UNNAMED_ICON_BUTTON: Lint = Lint {
    code: "a11y-unnamed-icon-button",
    severity: Severity::Warning,
    rule: "A button shows an icon and has no `text`, no `action` and no accessible name.",
    fix: "Set `Accessible.name` (and a `ToolTip` if sighted users need one).",
    in_check: true,
};

pub const UNNAMED_IMAGE: Lint = Lint {
    code: "a11y-unnamed-image",
    severity: Severity::Warning,
    rule: "An `Image` has no accessible name and is not marked as decoration.",
    fix: "Set `Accessible.name`, or `Accessible.ignored: true` if it is only decoration.",
    in_check: true,
};

pub const QSTR_NON_LITERAL: Lint = Lint {
    code: "i18n-qstr-non-literal",
    severity: Severity::Warning,
    rule: "`qsTr` is given something that is not a text written out, which no tool can \
           extract for translators.",
    fix: "Give `qsTr` a string literal; put what varies in with `.arg()`.",
    in_check: true,
};

pub const UNTRANSLATED_STRING: Lint = Lint {
    code: "i18n-untranslated-string",
    severity: Severity::Hint,
    rule: "A text the user sees is written out without `qsTr`, so it cannot be translated.",
    fix: "Wrap it in `qsTr(\"…\")`.",
    in_check: false,
};

/// Every rule, in the order they are documented.
pub const LINTS: [Lint; 5] = [
    UNNAMED_INTERACTIVE,
    UNNAMED_ICON_BUTTON,
    UNNAMED_IMAGE,
    QSTR_NON_LITERAL,
    UNTRANSLATED_STRING,
];

/// `ljos://lints`: the table as a page.
pub(crate) fn markdown() -> String {
    let mut out = String::from(
        "# Lint rules\n\n\
         What `qml_lint` reports, and which of it `ljos check` reports too. A finding is \
         tagged with its code wherever it is reported.\n\n\
         | Code | Severity | Reported by | Rule | Fix |\n|---|---|---|---|---|\n",
    );
    for lint in LINTS {
        let severity = match lint.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Hint => "hint",
        };
        let by = if lint.in_check {
            "`qml_lint`, `ljos check`"
        } else {
            "`qml_lint`"
        };
        out.push_str(&format!(
            "| `{}` | {severity} | {by} | {} | {} |\n",
            lint.code, lint.rule, lint.fix
        ));
    }
    out.push_str(
        "\nThe rules tell a `Button` from an `Item` by the table of types the build has: Qt's \
         where one was generated, and Ljós's own description of its types where not.\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique_and_say_what_they_are_about() {
        let mut codes: Vec<&str> = LINTS.iter().map(|l| l.code).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), LINTS.len());
        assert!(
            LINTS
                .iter()
                .all(|l| l.code.starts_with("a11y-") || l.code.starts_with("i18n-"))
        );
    }

    #[test]
    fn the_page_names_every_rule_and_who_reports_it() {
        let page = markdown();
        for lint in LINTS {
            assert!(page.contains(lint.code), "{}", lint.code);
        }
        assert!(page.contains("ljos check"));
    }
}
