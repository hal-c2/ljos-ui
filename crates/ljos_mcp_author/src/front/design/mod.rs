//! The design lints: the rules of `ljos_design` that the source alone can
//! tell (ARCHITECTURE.md §14, "Design guardrails"). Each rule reads its code,
//! severity and limits from `ljos_design`; `crate::lints::IMPLEMENTED` lists
//! the ones that are here.
//!
//! As the rest of the front end, a rule reports only what it can tell from
//! the file with confidence: where a value is not written out, or a context
//! could mean either, it says nothing and leaves the case to the runtime
//! audit. Each has good fixtures close to its bad ones
//! (`crates/ljos_design/fixtures/<code>/`).

mod content;
mod focus;
mod layout;
mod motion;

use qml_front::{QtType, types::Kind};
use qml_syntax::{
    GetSpan,
    ast::{QmlBinding, QmlBindingValue, QmlMember, QmlObject},
    js::{Expression, UnaryOperator},
};

use super::{diagnostic, lint::Cx};
use crate::{Diagnostic, lints};

/// An object with what it sits in.
pub(super) struct Frame<'n, 'a> {
    pub object: &'n QmlObject<'a>,
    pub index: Option<usize>,
    /// The property it is the value of (`enter`, `background`, `delegate`),
    /// or `None` for one in its parent's default property.
    pub via: Option<String>,
}

pub(super) fn run<'a>(cx: &Cx<'_, 'a>, root: &QmlObject<'a>, out: &mut Vec<Diagnostic>) {
    let mut found = Found { cx, out };
    let mut chain = Vec::new();
    walk(&mut found, root, None, &mut chain);
    for member in &root.members {
        if let QmlMember::InlineComponent(inline) = member {
            walk(&mut found, &inline.object, None, &mut Vec::new());
        }
    }
    file_rules(&mut found, root);
}

fn walk<'n, 'a>(
    found: &mut Found<'_, '_, '_, 'a>,
    object: &'n QmlObject<'a>,
    via: Option<String>,
    chain: &mut Vec<Frame<'n, 'a>>,
) {
    chain.push(Frame {
        object,
        index: found.cx.tree.index_of(object),
        via,
    });
    object_rules(found, chain);
    for (child, via) in children(object) {
        walk(found, child, via, chain);
    }
    chain.pop();
}

/// The rules about one object, the last of `chain`.
fn object_rules<'a>(found: &mut Found<'_, '_, '_, 'a>, chain: &[Frame<'_, 'a>]) {
    motion::object(found, chain);
    layout::object(found, chain);
    focus::object(found, chain);
}

/// The rules about a file as a whole.
fn file_rules<'a>(found: &mut Found<'_, '_, '_, 'a>, root: &QmlObject<'a>) {
    layout::file(found, root);
    content::file(found, root);
}

/// Where the findings go.
pub(super) struct Found<'f, 'c, 'x, 'a> {
    pub cx: &'f Cx<'c, 'a>,
    pub out: &'x mut Vec<Diagnostic>,
}

impl<'a> Found<'_, '_, '_, 'a> {
    pub fn report(&mut self, at: u32, code: &str, message: String) {
        let lint = lints::design(code);
        self.out.push(diagnostic(
            self.cx.path,
            self.cx.source,
            at,
            lint.severity,
            lint.code,
            message,
        ));
    }

    /// The builtin type an object comes down to, a component followed to
    /// the type it is built on.
    pub fn base(&self, frame: &Frame<'_, 'a>) -> Option<QtType> {
        let kind: &Kind<QtType> = self.cx.tree.objects[frame.index?].kind.as_ref()?;
        self.cx.types.base(kind)
    }

    pub fn is(&self, frame: &Frame<'_, 'a>, classes: &[&str]) -> bool {
        self.base(frame).is_some_and(|ty| ty.0.is_a(classes))
    }

    /// The object's name as written (`NumberAnimation`, `Controls.Button`).
    pub fn name<'s>(&self, frame: &Frame<'s, 'a>) -> String {
        frame.object.type_name.parts.join(".")
    }

    /// The source text of a span.
    pub fn text(&self, span: qml_syntax::Span) -> &str {
        self.cx
            .source
            .get(span.start as usize..span.end as usize)
            .unwrap_or_default()
    }
}

/// The types the rules are about, each by its class in Qt's table and in
/// Ljós's own (`qt_types::Type::is_a`).
pub(super) mod class {
    pub const ANIMATION: &[&str] = &["QQuickAbstractAnimation", "QtQuick.Animation"];
    pub const PROPERTY_ANIMATION: &[&str] =
        &["QQuickPropertyAnimation", "QtQuick.PropertyAnimation"];
    pub const ANIMATOR: &[&str] = &["QQuickAnimator", "QtQuick.Animator"];
    /// Animations that follow a curve of their own, not `easing`.
    pub const SELF_EASED: &[&str] = &[
        "QQuickSpringAnimation",
        "QQuickSmoothedAnimation",
        "QtQuick.SpringAnimation",
        "QtQuick.SmoothedAnimation",
    ];
    pub const COLOR_ANIMATION: &[&str] = &["QQuickColorAnimation", "QtQuick.ColorAnimation"];
    pub const PAUSE: &[&str] = &["QQuickPauseAnimation", "QtQuick.PauseAnimation"];
    pub const BEHAVIOR: &[&str] = &["QQuickBehavior", "QtQuick.Behavior"];
    pub const TRANSITION: &[&str] = &["QQuickTransition", "QtQuick.Transition"];
    pub const POPUP: &[&str] = &["QQuickPopup", "QtQuick.Templates.Popup"];
    pub const DRAWER: &[&str] = &["QQuickDrawer", "QtQuick.Templates.Drawer"];
    pub const MENU: &[&str] = &["QQuickMenu", "QtQuick.Templates.Menu"];
    pub const TOOL_TIP: &[&str] = &["QQuickToolTip", "QtQuick.Templates.ToolTip"];
    pub const PANE: &[&str] = &["QQuickPane", "QtQuick.Templates.Pane"];
    pub const LAYOUT: &[&str] = &["QQuickLayout", "QtQuick.Layouts.Layout"];
    pub const STACK_LAYOUT: &[&str] = &["QQuickStackLayout", "QtQuick.Layouts.StackLayout"];
    pub const TIMER: &[&str] = &["QQmlTimer", "QtQml.Timer"];
    pub const TEXT: &[&str] = &["QQuickText", "QtQuick.Text"];
    pub const TEXT_INPUT: &[&str] = &[
        "QQuickTextInput",
        "QQuickTextEdit",
        "QtQuick.TextInput",
        "QtQuick.TextEdit",
    ];
    pub const ITEM: &[&str] = &["QQuickItem", "QtQuick.Item"];
    pub const ABSTRACT_BUTTON: &[&str] =
        &["QQuickAbstractButton", "QtQuick.Templates.AbstractButton"];
    /// Buttons that show their text beside the icon in a list or a menu.
    pub const TEXT_ROW_BUTTON: &[&str] = &[
        "QQuickMenuItem",
        "QQuickMenuBarItem",
        "QQuickItemDelegate",
        "QtQuick.Templates.MenuItem",
        "QtQuick.Templates.MenuBarItem",
        "QtQuick.Templates.ItemDelegate",
    ];
    pub const MENU_SEPARATOR: &[&str] = &["QQuickMenuSeparator", "QtQuick.Templates.MenuSeparator"];
    pub const ACTION: &[&str] = &["QQuickAction", "QtQuick.Templates.Action"];
    pub const CONTROL: &[&str] = &["QQuickControl", "QtQuick.Templates.Control"];
    pub const MOUSE_AREA: &[&str] = &["QQuickMouseArea", "QtQuick.MouseArea"];
    pub const TAP_HANDLER: &[&str] = &["QQuickTapHandler", "QtQuick.TapHandler"];
    pub const VIEW: &[&str] = &[
        "QQuickItemView",
        "QQuickPathView",
        "QQuickTableView",
        "QtQuick.ItemView",
        "QtQuick.PathView",
        "QtQuick.TableView",
    ];
    pub const REPEATER: &[&str] = &["QQuickRepeater", "QtQuick.Repeater"];
    pub const LOADER: &[&str] = &["QQuickLoader", "QtQuick.Loader"];
    pub const PROGRESS: &[&str] = &[
        "QQuickBusyIndicator",
        "QQuickProgressBar",
        "QtQuick.Templates.BusyIndicator",
        "QtQuick.Templates.ProgressBar",
    ];
    pub const IMAGE: &[&str] = &["QQuickImage", "QtQuick.Image"];
}

/// Every object written in `object` with the property it is the value of.
pub(super) fn children<'o, 'a>(
    object: &'o QmlObject<'a>,
) -> Vec<(&'o QmlObject<'a>, Option<String>)> {
    let mut out = Vec::new();
    push_children(object, "", &mut out);
    out
}

fn push_children<'o, 'a>(
    object: &'o QmlObject<'a>,
    group: &str,
    out: &mut Vec<(&'o QmlObject<'a>, Option<String>)>,
) {
    for member in &object.members {
        match member {
            QmlMember::Object(child) if qml_front::scope::is_group(child) => {
                let name = format!("{group}{}.", child.type_name.parts.join("."));
                push_children(child, &name, out);
            }
            QmlMember::Object(child) => out.push((child, None)),
            QmlMember::Binding(QmlBinding { name, value, .. }) => {
                let name = format!("{group}{}", name.parts.join("."));
                push_value(value, &name, out);
            }
            QmlMember::Property(p) => {
                if let Some(value) = &p.value {
                    push_value(value, p.name.name.as_str(), out);
                }
            }
            _ => {}
        }
    }
}

fn push_value<'o, 'a>(
    value: &'o QmlBindingValue<'a>,
    name: &str,
    out: &mut Vec<(&'o QmlObject<'a>, Option<String>)>,
) {
    match value {
        QmlBindingValue::Object(o) => out.push((o, Some(name.to_owned()))),
        QmlBindingValue::Objects(os) => out.extend(os.iter().map(|o| (o, Some(name.to_owned())))),
        _ => {}
    }
}

/// A binding as written: its value and where its name starts.
#[derive(Clone, Copy)]
pub(super) struct Bound<'o, 'a> {
    pub value: &'o QmlBindingValue<'a>,
    pub at: u32,
}

impl<'o, 'a> Bound<'o, 'a> {
    pub fn expression(&self) -> Option<&'o Expression<'a>> {
        match self.value {
            QmlBindingValue::Expression(e) => Some(e),
            _ => None,
        }
    }

    pub fn number(&self) -> Option<f64> {
        self.expression().and_then(number)
    }

    pub fn string(&self) -> Option<&'o str> {
        match self.expression()? {
            Expression::StringLiteral(s) => Some(s.value.as_str()),
            Expression::TemplateLiteral(t) if t.expressions.is_empty() => {
                t.quasis.first()?.value.cooked.as_ref().map(|c| c.as_str())
            }
            _ => None,
        }
    }

    pub fn boolean(&self) -> Option<bool> {
        match self.expression()? {
            Expression::BooleanLiteral(b) => Some(b.value),
            _ => None,
        }
    }

    /// `Easing.OutCubic` → `("Easing", "OutCubic")`.
    pub fn enumeration(&self) -> Option<(&'o str, &'o str)> {
        match self.expression()?.without_parentheses() {
            Expression::StaticMemberExpression(m) => match &m.object {
                Expression::Identifier(id) => Some((id.name.as_str(), m.property.name.as_str())),
                _ => None,
            },
            _ => None,
        }
    }

    /// The span of the value, for its text.
    pub fn span(&self) -> Option<qml_syntax::Span> {
        self.expression().map(GetSpan::span)
    }
}

/// A number written out, negative ones too.
pub(super) fn number(e: &Expression<'_>) -> Option<f64> {
    match e.without_parentheses() {
        Expression::NumericLiteral(n) => Some(n.value),
        Expression::UnaryExpression(u) if u.operator == UnaryOperator::UnaryNegation => {
            number(&u.argument).map(|n| -n)
        }
        _ => None,
    }
}

/// The binding `path` on `object`, among its bindings and in group notation
/// (`easing { type: … }`, `anchors { fill: parent }`) alike.
pub(super) fn bound<'o, 'a>(object: &'o QmlObject<'a>, path: &[&str]) -> Option<Bound<'o, 'a>> {
    for member in &object.members {
        match member {
            QmlMember::Binding(QmlBinding { name, value, .. })
                if name.parts.iter().copied().eq(path.iter().copied()) =>
            {
                return Some(Bound {
                    value,
                    at: name.span.start,
                });
            }
            QmlMember::Object(child) if child.on.is_none() => {
                let parts = &child.type_name.parts;
                if parts.len() < path.len()
                    && parts
                        .iter()
                        .copied()
                        .eq(path[..parts.len()].iter().copied())
                    && let Some(found) = bound(child, &path[parts.len()..])
                {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

/// Every binding of `object` with its dotted name (`anchors.fill`,
/// `easing.type`), group notation flattened.
pub(super) fn bindings<'o, 'a>(object: &'o QmlObject<'a>) -> Vec<(String, Bound<'o, 'a>)> {
    let mut out = Vec::new();
    push_bindings(object, "", &mut out);
    out
}

fn push_bindings<'o, 'a>(
    object: &'o QmlObject<'a>,
    group: &str,
    out: &mut Vec<(String, Bound<'o, 'a>)>,
) {
    for member in &object.members {
        match member {
            QmlMember::Binding(QmlBinding { name, value, .. }) => out.push((
                format!("{group}{}", name.parts.join(".")),
                Bound {
                    value,
                    at: name.span.start,
                },
            )),
            QmlMember::Object(child) if child.on.is_none() && qml_front::scope::is_group(child) => {
                let name = format!("{group}{}.", child.type_name.parts.join("."));
                push_bindings(child, &name, out);
            }
            _ => {}
        }
    }
}

/// Whether the file is a `pragma Singleton`: a theme, whose literals are
/// the point.
pub(super) fn is_singleton(source: &str) -> bool {
    source
        .lines()
        .map(str::trim)
        .take_while(|l| !l.ends_with('{'))
        .any(|l| l.starts_with("pragma Singleton"))
}
