//! The design rules, once. A static lint (`ljos check`, `qml_lint`), the
//! runtime audit (`ljos audit`, `app_audit`, the BDD steps), the MCP design
//! tools and `docs/design/` all name a rule by the code in [`RULES`] and
//! read its limits from [`limits`], so a finding means the same thing in a
//! terminal, in an agent's answer and in the guideline it links to.
//!
//! A rule's severity is a promise about confidence, not about importance:
//! an `Error` is something the check can tell for certain is wrong for a
//! user, and it fails `ljos check` and `ljos audit`; a `Warning` is very
//! likely wrong and needs a reason to keep (`// ljos-allow(code): why`);
//! a `Hint` is advice.

use serde::Serialize;

pub mod limits;

/// How sure a check is that a finding hurts a user.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Hint,
}

/// Where a rule can be told: from the source alone, only from the running
/// scene, or from either (the source catches the plain cases early, the
/// scene the rest).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Check {
    Static,
    Runtime,
    Both,
}

/// What part of an interface a rule is about; also the guideline file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Area {
    A11y,
    Color,
    Text,
    Layout,
    Focus,
    Motion,
    Content,
}

impl Area {
    /// The guideline page under `docs/design/`.
    pub const fn page(self) -> &'static str {
        match self {
            Area::A11y => "accessibility.md",
            Area::Color => "color.md",
            Area::Text => "typography.md",
            Area::Layout => "layout.md",
            Area::Focus => "interaction.md",
            Area::Motion => "motion.md",
            Area::Content => "content.md",
        }
    }
}

/// One rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Rule {
    /// What a finding is tagged with: `[motion-ease-in-enter]`.
    pub code: &'static str,
    pub area: Area,
    pub severity: Severity,
    pub check: Check,
    /// What is wrong where the rule finds something.
    pub rule: &'static str,
    /// What to write so that it no longer is.
    pub fix: &'static str,
}

impl Rule {
    /// The guideline section: `docs/design/<page>#<code>`.
    pub fn guide(&self) -> String {
        format!("docs/design/{}#{}", self.area.page(), self.code)
    }
}

/// The rule with this code.
pub fn rule(code: &str) -> Option<&'static Rule> {
    RULES.iter().find(|r| r.code == code)
}

const fn r(
    code: &'static str,
    area: Area,
    severity: Severity,
    check: Check,
    rule: &'static str,
    fix: &'static str,
) -> Rule {
    Rule {
        code,
        area,
        severity,
        check,
        rule,
        fix,
    }
}

use Area::*;
use Check::*;
use Severity::*;

/// Every rule. The order is the order of the guidelines.
pub const RULES: &[Rule] = &[
    // Accessibility: the rules `ljos check` and the audit had before this
    // table; their codes do not change.
    r(
        "a11y-unnamed-interactive",
        A11y,
        Error,
        Both,
        "Something reacts to a click, a tap or a key and has no accessible name.",
        "Set `Accessible.name` (and `Accessible.role` on a plain item), or `Accessible.ignored: true` if it is decoration.",
    ),
    r(
        "a11y-unnamed-icon-button",
        A11y,
        Error,
        Both,
        "A button shows only an icon and has no `text`, `action` or accessible name.",
        "Give it `text` with `display: AbstractButton.IconOnly`, or `Accessible.name`.",
    ),
    r(
        "a11y-unnamed-image",
        A11y,
        Warning,
        Both,
        "An image is neither named nor left out of the accessibility tree.",
        "Set `Accessible.name`, or `Accessible.ignored: true` if it is decoration.",
    ),
    r(
        "a11y-duplicate-name",
        A11y,
        Warning,
        Runtime,
        "Two actionable siblings have the same accessible name.",
        "Name each by what it does (`Retry upload`, not `Retry`).",
    ),
    r(
        "a11y-ambiguous-name",
        A11y,
        Warning,
        Runtime,
        "Two actionable items in one window share a name but do different things.",
        "Name each by what it acts on (`More options for Commit`).",
    ),
    r(
        "a11y-icon-button-no-tooltip",
        A11y,
        Warning,
        Both,
        "A button shows only an icon and has no tooltip, so a sighted user cannot learn what it does.",
        "Set `ToolTip.text` (and `ToolTip.visible: hovered`), using the same words as its name.",
    ),
    r(
        "a11y-target-size",
        A11y,
        Error,
        Runtime,
        "A pointer target is smaller than the minimum for the platform and has no spacing to make up for it.",
        "Grow the target (not the drawing): `implicitWidth`/`implicitHeight`, padding, or a larger `MouseArea` around the visual.",
    ),
    r(
        "a11y-tab-trap",
        A11y,
        Error,
        Runtime,
        "Tab walks into a part of the interface and cannot leave it.",
        "Let Tab and Shift+Tab leave the part; a modal popup keeps focus but closes on Escape.",
    ),
    r(
        "a11y-tab-unreachable",
        A11y,
        Error,
        Runtime,
        "Something the pointer can operate is never reached by Tab.",
        "Use a control, or set `activeFocusOnTab: true` and handle `Keys.onReturnPressed`/`onSpacePressed`.",
    ),
    r(
        "a11y-set-size",
        A11y,
        Warning,
        Runtime,
        "A row of a virtualised list does not tell its position in the list.",
        "Use a view's delegate; it tells `position_in_set` and `size_of_set` itself.",
    ),
    // Colour.
    r(
        "color-text-contrast",
        Color,
        Error,
        Both,
        "Text does not stand out enough from what is behind it (WCAG 2.2 1.4.3), in at least one colour scheme or contrast setting.",
        "Take text and surface from one palette or token pair made for each other (`palette.text` on `palette.base`), never a tint of the surface.",
    ),
    r(
        "color-nontext-contrast",
        Color,
        Warning,
        Runtime,
        "A boundary, icon, focus ring or state indicator a user needs stands out less than 3:1 from what is next to it (WCAG 2.2 1.4.11).",
        "Use the palette's border or `mid` roles for edges and the text colour for icons.",
    ),
    r(
        "color-state-only",
        Color,
        Warning,
        Runtime,
        "A control's states (on and off, selected or not) differ only by hue or tint.",
        "Change shape, fill, an icon or weight as well, and say the state in its accessible name or state.",
    ),
    r(
        "color-literal",
        Color,
        Hint,
        Static,
        "A colour is written as a literal in a component rather than taken from the palette or a theme.",
        "Read `palette.*`, the style's attached colours, or a theme singleton, so dark mode and high contrast follow.",
    ),
    r(
        "color-scheme-ignored",
        Color,
        Warning,
        Static,
        "A component draws its own surfaces with fixed colours and no binding to the palette or colour scheme, so one scheme will be unreadable.",
        "Bind colours to `palette.*` or to `Application.styleHints.colorScheme`.",
    ),
    r(
        "color-translucent-surface",
        Color,
        Warning,
        Runtime,
        "A popup, menu or dialog lets the content behind it show through enough to compete with its own text.",
        "Make the surface opaque (alpha at least the limit), or put a blur behind it.",
    ),
    r(
        "color-surface-merge",
        Color,
        Warning,
        Runtime,
        "A floating or adjacent surface is drawn in nearly the colour behind it, with no border or shadow, so its edge cannot be seen.",
        "Raise it a step on the surface scale, or give it a border or a shadow.",
    ),
    // Text.
    r(
        "text-clipped",
        Text,
        Error,
        Runtime,
        "Text is cut off, by its parent's clip, a sibling or the window edge, without eliding or wrapping, at some font scale or window size.",
        "Let the text wrap (`wrapMode`) or elide (`elide`, and keep the full text reachable), and let its container grow with `implicitHeight`.",
    ),
    r(
        "text-elided-hidden",
        Text,
        Warning,
        Runtime,
        "Text is elided and the full text is nowhere a user can get it.",
        "Show the full text in a `ToolTip` while elided (`truncated`), or in `Accessible.description`.",
    ),
    r(
        "text-unbounded-dynamic",
        Text,
        Warning,
        Static,
        "Text from a binding sits in a box of bounded width with neither `elide` nor `wrapMode`.",
        "Set `elide: Text.ElideRight` (and a tooltip), or `wrapMode: Text.Wrap`.",
    ),
    r(
        "text-raw-markup",
        Text,
        Warning,
        Runtime,
        "Plain text shows markup or an internal envelope as characters (backticks, `**`, `<tag …>`).",
        "Render it as rich text or Markdown (`textFormat`), or strip the markup before showing it.",
    ),
    // Layout.
    r(
        "layout-offscreen",
        Layout,
        Error,
        Runtime,
        "Something a user can operate lies partly or wholly outside the window, at some window size or font scale, and nothing scrolls it into view.",
        "Give the layout minimum sizes, let it wrap or scroll, or move the control into an overflow menu when space runs out.",
    ),
    r(
        "layout-occluded",
        Layout,
        Error,
        Runtime,
        "Something a user can operate is covered by another item drawn above it.",
        "Place the covering item (a toast, a banner, a popup) where it covers nothing it does not own, or move it out of the way.",
    ),
    r(
        "layout-overlap",
        Layout,
        Warning,
        Runtime,
        "Two pieces of text or two controls that are not meant to stack are drawn over each other.",
        "Let a layout place them rather than anchors or fixed positions that ignore each other's size.",
    ),
    r(
        "layout-under-minimum",
        Layout,
        Warning,
        Runtime,
        "An item is laid out smaller than its own minimum or implicit size, so its content slides or squeezes.",
        "Give the window or the parent layout a minimum size that adds up, or collapse the part when it no longer fits.",
    ),
    r(
        "layout-popup-fit",
        Layout,
        Warning,
        Runtime,
        "A popup or dialog is much larger than what it holds, or too small to show it.",
        "Size it from its content (`implicitWidth`/`implicitHeight`), with a maximum and a scroll, not a fixed size.",
    ),
    r(
        "layout-fixed-size",
        Layout,
        Warning,
        Static,
        "A popup, dialog or pane with content that changes has a literal width and height.",
        "Bind its size to its content's implicit size, within a maximum.",
    ),
    r(
        "layout-misaligned",
        Layout,
        Hint,
        Runtime,
        "Controls in one column start a few pixels apart, or controls of one kind in a form have different widths.",
        "Put them in one layout and let it align them; give controls of a kind one width.",
    ),
    r(
        "layout-anchors-in-layout",
        Layout,
        Error,
        Static,
        "An item directly inside a layout sets anchors, which the layout overrides.",
        "Use `Layout.*` properties, or wrap the item in a plain `Item`.",
    ),
    r(
        "layout-spacing-scale",
        Layout,
        Hint,
        Static,
        "A spacing, margin or padding is not on the spacing scale.",
        "Use a step of the scale (multiples of the base unit).",
    ),
    // Focus and interaction.
    r(
        "focus-indicator-missing",
        Focus,
        Error,
        Runtime,
        "A control that takes keyboard focus looks the same with and without it (WCAG 2.2 2.4.7).",
        "Draw `visualFocus` (a ring or a fill change) in the control's background.",
    ),
    r(
        "focus-order",
        Focus,
        Warning,
        Runtime,
        "Tab moves against the reading order (back up the window, or across columns and back).",
        "Order the items in the file as they are read, or set `KeyNavigation.tab`.",
    ),
    r(
        "focus-popup-stranded",
        Focus,
        Warning,
        Runtime,
        "A modal popup or dialog is open with keyboard focus outside it, or Escape does not close it.",
        "Give it `focus: true` and a `closePolicy` with `Popup.CloseOnEscape`.",
    ),
    r(
        "focus-pointer-only",
        Focus,
        Warning,
        Static,
        "An action is bound to a pointer handler on a plain item, with no key handler, so a keyboard cannot do it.",
        "Use a control (`Button`, `ItemDelegate`), or add `activeFocusOnTab`, `Keys` handlers and `Accessible.onPressAction`.",
    ),
    r(
        "focus-hover-only",
        Focus,
        Warning,
        Static,
        "Something is shown only while the pointer hovers, so touch and keyboard users never see it.",
        "Show it on `hovered || activeFocus` (or `visualFocus`), and make its action reachable another way.",
    ),
    // Motion.
    r(
        "motion-ease-in-enter",
        Motion,
        Error,
        Both,
        "Something appears or responds with an ease-in curve: it starts slow, so the interface feels late.",
        "Use an ease-out curve for anything entering or answering the user; keep ease-in for exits only.",
    ),
    r(
        "motion-default-easing",
        Motion,
        Warning,
        Static,
        "An animation moves, scales or fades with no easing, which is linear: a mechanical start and stop.",
        "Set `easing.type` (ease-out to enter, in-out to move on screen) or use the motion tokens.",
    ),
    r(
        "motion-duration",
        Motion,
        Warning,
        Both,
        "An interface animation is longer than its kind allows, or so short it reads as a flicker.",
        "Use the duration for its kind and frequency (the motion tokens), or 0 for something done many times a day.",
    ),
    r(
        "motion-scale-from-zero",
        Motion,
        Error,
        Both,
        "Something grows from scale 0, which looks like it comes from nowhere.",
        "Start at a scale near 1 together with an opacity fade.",
    ),
    r(
        "motion-bounce",
        Motion,
        Warning,
        Static,
        "A control or panel moves with a bounce, elastic or back curve.",
        "Use ease-out or a critically damped spring; keep bounce for rare, celebratory moments.",
    ),
    r(
        "motion-layout-property",
        Motion,
        Warning,
        Static,
        "An animation changes a size or anchor margin, which lays the interface out again every frame.",
        "Animate `scale`, `opacity` or a transform, and set the size once.",
    ),
    r(
        "motion-exit-slower",
        Motion,
        Warning,
        Static,
        "Something leaves as slowly as or more slowly than it came.",
        "Make exits shorter than entrances.",
    ),
    r(
        "motion-stagger",
        Motion,
        Warning,
        Both,
        "Items of a group start one after another with too long a step, or the last waits too long.",
        "Keep each step and the whole stagger within the limits.",
    ),
    r(
        "motion-frequent",
        Motion,
        Warning,
        Both,
        "Something done many times a day (selection, tab switch, focus, a shortcut) animates for longer than the limit.",
        "Make it instant, or keep it within the limit for frequent actions.",
    ),
    r(
        "motion-timer-driven",
        Motion,
        Warning,
        Static,
        "A repeating `Timer` moves or fades something by hand, past reduced motion and the animation clock.",
        "Use an animation or `Behavior`; they honour reduced motion and stop when done.",
    ),
    r(
        "motion-dropped-frames",
        Motion,
        Warning,
        Runtime,
        "An animation dropped frames while it ran.",
        "Profile with `app_frames`; animate cheap properties and avoid relayout while it runs.",
    ),
    // Content.
    r(
        "content-inconsistent-words",
        Content,
        Hint,
        Runtime,
        "One window names the same action two ways (`Retry` and `Try again`).",
        "Pick one word for one action across the interface.",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique_kebab_case_and_start_with_their_area() {
        let mut seen = std::collections::HashSet::new();
        for rule in RULES {
            assert!(seen.insert(rule.code), "{} twice", rule.code);
            assert!(
                rule.code
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{} is not kebab-case",
                rule.code
            );
            let prefix = match rule.area {
                A11y => "a11y-",
                Color => "color-",
                Text => "text-",
                Layout => "layout-",
                Focus => "focus-",
                Motion => "motion-",
                Content => "content-",
            };
            assert!(
                rule.code.starts_with(prefix),
                "{} is not {prefix}",
                rule.code
            );
        }
    }

    #[test]
    fn a_rule_is_found_by_its_code() {
        let r = rule("motion-ease-in-enter").unwrap();
        assert_eq!(r.severity, Error);
        assert_eq!(r.guide(), "docs/design/motion.md#motion-ease-in-enter");
        assert!(rule("no-such-rule").is_none());
    }
}
