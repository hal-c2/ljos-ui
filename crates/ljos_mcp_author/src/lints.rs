//! The lint rules, once: their codes, how serious each is, and who reports
//! it. `qml_lint` reports all of them; `ljos check` reports the ones a
//! reader of the application pays for. Both read this table, so a finding
//! has the same code in a terminal, in an agent's answer and in an editor.
//!
//! The design rules (accessibility, colour, text, layout, focus, motion)
//! are `ljos_design`'s, where the runtime audit and the guidelines read
//! them too; this table takes the ones the source can tell and maps their
//! severity. The translation rules are about the text, not the design, and
//! stay here; so do the declarative rules (`decl-*`), which are about how
//! the code says what it does, not about what a user sees
//! (`docs/declarative.md`).

use std::sync::LazyLock;

use ljos_design::{Check, RULES, Rule};

use crate::Severity;

/// One rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    /// The design rule it is, if it is one.
    pub design: Option<&'static Rule>,
}

impl Lint {
    /// The guideline section of a design rule (`docs/design/<page>#<code>`)
    /// or of a declarative one (`docs/declarative.md#<code>`).
    pub fn guide(&self) -> Option<String> {
        self.design.map(Rule::guide).or_else(|| {
            DECLARATIVE
                .iter()
                .any(|l| l.code == self.code)
                .then(|| format!("docs/declarative.md#{}", self.code))
        })
    }
}

pub const QSTR_NON_LITERAL: Lint = Lint {
    code: "i18n-qstr-non-literal",
    severity: Severity::Warning,
    rule: "`qsTr` is given something that is not a text written out, which no tool can \
           extract for translators.",
    fix: "Give `qsTr` a string literal; put what varies in with `.arg()`.",
    in_check: true,
    design: None,
};

pub const UNTRANSLATED_STRING: Lint = Lint {
    code: "i18n-untranslated-string",
    severity: Severity::Hint,
    rule: "A text the user sees is written out without `qsTr`, so it cannot be translated.",
    fix: "Wrap it in `qsTr(\"…\")`.",
    in_check: false,
    design: None,
};

pub const MIRROR_HANDLER: Lint = Lint {
    code: "decl-mirror-handler",
    severity: Severity::Hint,
    rule: "A change handler only copies a value into a property nothing else writes, which a \
           binding says.",
    fix: "Bind the property and drop the handler: `onLevelChanged: shown = level` is \
          `shown: level`.",
    in_check: false,
    design: None,
};

pub const MODE_BY_HANDLERS: Lint = Lint {
    code: "decl-mode-by-handlers",
    severity: Severity::Hint,
    rule: "Two or more handlers each set the same group of properties to values of their own: \
           modes switched by hand.",
    fix: "Make each mode a state and let the handlers name it: `states: [State { name: \
          \"edit\"; PropertyChanges { list.opacity: 0; editor.opacity: 1 } }]`, then \
          `onClicked: root.state = \"edit\"`, or give each state a `when:`.",
    in_check: false,
    design: None,
};

pub const IMPERATIVE_ANIMATION: Lint = Lint {
    code: "decl-imperative-animation",
    severity: Severity::Hint,
    rule: "Script starts animations one after another, or starts opposite animations of one \
           property from different handlers.",
    fix: "Chain them in one animation, `SequentialAnimation { id: pulse; NumberAnimation { … } \
          NumberAnimation { … } }`, or animate a change of state: `transitions: Transition { \
          NumberAnimation { property: \"opacity\" } }`, or `Behavior on opacity { \
          NumberAnimation { } }`.",
    in_check: false,
    design: None,
};

pub const VISIBILITY_JUGGLING: Lint = Lint {
    code: "decl-visibility-juggling",
    severity: Severity::Hint,
    rule: "One item's `visible` or `opacity` is set to values written out from several handlers.",
    fix: "Bind it to what decides it, `visible: drawer.opened`, or make the shown and hidden \
          states and give them a transition: `states: State { name: \"shown\"; PropertyChanges \
          { banner.visible: true } }`.",
    in_check: false,
    design: None,
};

/// The declarative rules: script where a declaration says the same, which
/// the engine compiles and the motion and focus rules can read.
pub const DECLARATIVE: &[Lint] = &[
    MIRROR_HANDLER,
    MODE_BY_HANDLERS,
    IMPERATIVE_ANIMATION,
    VISIBILITY_JUGGLING,
];

/// The design rules the static lints implement. A rule whose check is
/// `static` or `both` and that is not here has no static check yet: its
/// fixtures are pending (`crates/ljos_design/fixtures/README.md`).
pub const IMPLEMENTED: &[&str] = &[
    "a11y-unnamed-interactive",
    "a11y-unnamed-icon-button",
    "a11y-unnamed-image",
    "a11y-icon-button-no-tooltip",
    "color-text-contrast",
    "color-literal",
    "color-scheme-ignored",
    "text-unbounded-dynamic",
    "text-font-literal",
    "layout-fixed-size",
    "layout-anchors-in-layout",
    "layout-spacing-scale",
    "layout-menu-ungrouped",
    "focus-pointer-only",
    "focus-hover-only",
    "motion-ease-in-enter",
    "motion-default-easing",
    "motion-scale-from-zero",
    "motion-duration",
    "motion-bounce",
    "motion-layout-property",
    "motion-exit-slower",
    "motion-stagger",
    "motion-frequent",
    "motion-timer-driven",
    "motion-reduced-ignored",
    "motion-flash",
    "motion-endless",
    "motion-origin",
    "a11y-placeholder-label",
    "content-vague-action",
    "content-menu-ellipsis",
    "layout-nested-radius",
    "focus-destructive-undo",
    "content-duplicate-control",
    "lint-allow-without-reason",
    "lint-allow-unused",
];

pub(crate) fn severity(severity: ljos_design::Severity) -> Severity {
    match severity {
        ljos_design::Severity::Error => Severity::Error,
        ljos_design::Severity::Warning => Severity::Warning,
        ljos_design::Severity::Hint => Severity::Hint,
    }
}

fn from_design(rule: &'static Rule) -> Lint {
    let severity = severity(rule.severity);
    Lint {
        code: rule.code,
        severity,
        rule: rule.rule,
        fix: rule.fix,
        // A hint is advice for whoever writes the file, not a fault of
        // the build.
        in_check: severity != Severity::Hint,
        design: Some(rule),
    }
}

static TABLE: LazyLock<Vec<Lint>> = LazyLock::new(|| {
    RULES
        .iter()
        .filter(|r| r.check != Check::Runtime && IMPLEMENTED.contains(&r.code))
        .map(from_design)
        .chain([QSTR_NON_LITERAL, UNTRANSLATED_STRING])
        .chain(DECLARATIVE.iter().copied())
        .collect()
});

/// Every rule the lints report, in the order they are documented.
pub fn lints() -> &'static [Lint] {
    &TABLE
}

/// The rule with this code.
pub fn lint(code: &str) -> Option<&'static Lint> {
    lints().iter().find(|l| l.code == code)
}

/// The design rule with this code, which the lints implement. A code that
/// is not one is a mistake in the lints, not in the file.
pub(crate) fn design(code: &str) -> &'static Lint {
    lint(code)
        .filter(|l| l.design.is_some())
        .unwrap_or_else(|| panic!("`{code}` is not an implemented design rule"))
}

/// `ljos://lints`: the table as a page.
pub(crate) fn markdown() -> String {
    let mut out = String::from(
        "# Lint rules\n\n\
         What `qml_lint` reports, and which of it `ljos check` reports too. A finding is \
         tagged with its code wherever it is reported. A design rule links to its guideline \
         (`docs/design/<page>#<code>`).\n\n\
         An `error` is certain and fails `ljos check` (`--lint=warn` makes it a warning, for \
         code that is not yours). A `warning` is kept with a reason: \
         `// ljos-allow(<code>): <why>` on the finding's line, the line above, or the first \
         line of the object it is in. A `hint` is advice, from `qml_lint` alone.\n\n\
         A declarative rule (`decl-*`) finds script that a binding, a state or a transition \
         says better, and links to `docs/declarative.md#<code>`.\n\n\
         | Code | Severity | Reported by | Rule | Fix | Guide |\n|---|---|---|---|---|---|\n",
    );
    for lint in lints() {
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
        let guide = lint.guide().unwrap_or_default();
        out.push_str(&format!(
            "| `{}` | {severity} | {by} | {} | {} | {guide} |\n",
            lint.code, lint.rule, lint.fix
        ));
    }
    let pending: Vec<&Rule> = RULES
        .iter()
        .filter(|r| r.check != Check::Runtime && !IMPLEMENTED.contains(&r.code))
        .collect();
    if !pending.is_empty() {
        out.push_str(
            "\nDesign rules the source could tell that have no static check yet (the runtime \
             audit may):\n\n",
        );
        for rule in pending {
            out.push_str(&format!(
                "- `{}`: {} ({})\n",
                rule.code,
                rule.rule,
                rule.guide()
            ));
        }
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
    fn codes_are_unique_and_the_design_ones_are_design_rules() {
        let mut codes: Vec<&str> = lints().iter().map(|l| l.code).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), lints().len());
        for code in IMPLEMENTED {
            let rule = ljos_design::rule(code).unwrap_or_else(|| panic!("{code} is no rule"));
            assert_ne!(rule.check, Check::Runtime, "{code} is runtime only");
            assert_eq!(design(code).severity, severity(rule.severity));
        }
    }

    #[test]
    fn the_page_names_every_rule_its_guide_and_who_reports_it() {
        let page = markdown();
        for lint in lints() {
            assert!(page.contains(lint.code), "{}", lint.code);
            if let Some(guide) = lint.guide() {
                assert!(page.contains(&guide), "{guide}");
            }
        }
        for rule in RULES.iter().filter(|r| r.check != Check::Runtime) {
            assert!(page.contains(&rule.guide()), "{}", rule.code);
        }
        assert!(page.contains("ljos check"));
        assert!(page.contains("ljos-allow"));
    }

    #[test]
    fn a_declarative_rule_is_a_hint_with_a_section_of_its_own() {
        let page = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/declarative.md"),
        )
        .unwrap();
        for lint in DECLARATIVE {
            assert!(lint.code.starts_with("decl-"), "{}", lint.code);
            assert_eq!(lint.severity, Severity::Hint);
            assert!(!lint.in_check);
            assert_eq!(
                lint.guide(),
                Some(format!("docs/declarative.md#{}", lint.code))
            );
            assert!(
                page.contains(&format!("<a id=\"{}\"></a>", lint.code)),
                "docs/declarative.md has no section for {}",
                lint.code
            );
        }
    }
}
