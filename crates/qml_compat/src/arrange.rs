//! Qt's positioners and line layouts lowered to `Ljos.Ui`'s one
//! arrangement (`Row`, `Column`; docs/DIALECT.md).
//!
//! Ljós's `Row` places a child as Qt's `Row` positioner does when the child
//! says nothing through `Layout.`, and sizes it as Qt's `RowLayout` does when
//! it does, with one size rule. A file is lowered where the two say the same
//! of every child, which the pass can only tell from what the file writes;
//! where it cannot, the file is kept and says which rule it would break.
//!
//! A `RowLayout` differs from Ljós's `Row` in what it does by default, and
//! the pass writes those defaults out: a spacing of 5, and a child centred
//! across the row. Everything else that differs keeps the file:
//!
//! - a child with a width and an implicit width: `RowLayout` takes the
//!   implicit one, Ljós's `Row` the one the file gives;
//! - an implicit size that may be a fraction (a `Text`'s): `RowLayout`
//!   rounds it up, Ljós's `Row` does not;
//! - a layout with nothing to fill it that may be sized along its line:
//!   `RowLayout` spreads the room, Ljós's `Row` leaves it at the end. The
//!   file may size it, or, for the root of the file, of an inline component
//!   or of a `Component`, or an object given as a value, whatever uses or
//!   holds it may, which the file does not show;
//! - a layout in a layout: Qt's fills its cell and is bounded by its
//!   children, Ljós's is a child like any other;
//! - an anchored child, and `uniformCellSizes`.
//!
//! A `Row` positioner is lowered unless a child says something through
//! `Layout.` (Qt's ignores it, Ljós's would not), a child sets its own
//! position across the row (Qt's leaves it, Ljós's sets it), a child is
//! anchored, or the file uses what only a positioner has (`add`, `move`,
//! `populate`, `Positioner`, `effectiveLayoutDirection`, `forceLayout`). A
//! child that is a file of the project or an inline component is held to
//! the same by what its root binds.

use std::ops::Range;

use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use oxc_syntax::operator::BinaryOperator;
use qml_edit::Change;
use qml_syntax::ast::{QmlBinding, QmlBindingValue, QmlMember, QmlObject};
use qt_types::Type;

use crate::Kept;

/// What of Qt's an object is that the pass lowers to an arrangement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shape {
    /// `Row` or `Column` of QtQuick.
    Positioner { vertical: bool },
    /// `RowLayout` or `ColumnLayout` of QtQuick.Layouts.
    Layout { vertical: bool },
}

impl Shape {
    pub(crate) fn of(ty: &Type) -> Option<Shape> {
        match (ty.module, ty.name) {
            ("QtQuick", "Row") => Some(Shape::Positioner { vertical: false }),
            ("QtQuick", "Column") => Some(Shape::Positioner { vertical: true }),
            ("QtQuick.Layouts", "RowLayout") => Some(Shape::Layout { vertical: false }),
            ("QtQuick.Layouts", "ColumnLayout") => Some(Shape::Layout { vertical: true }),
            _ => None,
        }
    }

    fn vertical(self) -> bool {
        match self {
            Shape::Positioner { vertical } | Shape::Layout { vertical } => vertical,
        }
    }

    /// What Ljós calls it.
    fn ljos(self) -> &'static str {
        if self.vertical() { "Column" } else { "Row" }
    }

    fn qt(self) -> &'static str {
        match self {
            Shape::Positioner { vertical: false } => "Row",
            Shape::Positioner { vertical: true } => "Column",
            Shape::Layout { vertical: false } => "RowLayout",
            Shape::Layout { vertical: true } => "ColumnLayout",
        }
    }
}

/// Qt's default spacing of a `RowLayout`, which Ljós's `Row` (0, as a
/// positioner's) is told.
const LAYOUT_SPACING: &str = "5";

/// Items whose implicit size is 0 unless the file gives them one, so that
/// a width the file gives is what `RowLayout` asks for too.
const ZERO_IMPLICIT: &[&str] = &["Item", "Rectangle", "MouseArea"];

/// Members only a positioner has, which Ljós's arrangement does not.
const POSITIONER_ONLY: &[&str] = &[
    "add",
    "move",
    "populate",
    "onPositioningComplete",
    "positioningComplete",
];

/// Names a script or another object reads of a positioner only.
const POSITIONER_ONLY_NAMES: &[&str] = &["effectiveLayoutDirection", "forceLayout", "Positioner."];

/// What the pass knows of the types it meets.
pub(crate) trait Types {
    /// The QtQuick or QtQuick.Layouts type `name` names in the file.
    fn qt_type(&self, name: &str) -> Option<&'static Type>;
    /// For a file of the project or an inline component, the names its
    /// root binds, with those of the roots it is built on; `None` for a
    /// type that is neither.
    fn root_bindings(&self, name: &str) -> Option<Vec<String>>;
}

/// The edits that lower `object`, a `shape`, or why its file is kept.
pub(crate) fn arrange(
    source: &str,
    object: &QmlObject<'_>,
    shape: Shape,
    a_root: bool,
    types: &dyn Types,
) -> Result<Vec<Change>, Kept> {
    let mut a = Arrange {
        source,
        shape,
        a_root,
        types,
        changes: Vec::new(),
    };
    match shape {
        Shape::Positioner { .. } => a.positioner(object)?,
        Shape::Layout { .. } => a.layout(object)?,
    }
    Ok(a.changes)
}

struct Arrange<'s, 't> {
    source: &'s str,
    shape: Shape,
    /// The root of the file or of an inline component: a file that uses
    /// it may size it.
    a_root: bool,
    types: &'t dyn Types,
    changes: Vec<Change>,
}

/// The axes, as an index: width, then height.
const W: usize = 0;
const H: usize = 1;
const SIZE: [&str; 2] = ["width", "height"];
const IMPLICIT: [&str; 2] = ["implicitWidth", "implicitHeight"];
const PREFERRED: [&str; 2] = ["Layout.preferredWidth", "Layout.preferredHeight"];
const FILL: [&str; 2] = ["Layout.fillWidth", "Layout.fillHeight"];
const MINIMUM: [&str; 2] = ["Layout.minimumWidth", "Layout.minimumHeight"];
const MAXIMUM: [&str; 2] = ["Layout.maximumWidth", "Layout.maximumHeight"];

impl Arrange<'_, '_> {
    fn keep(&self, reason: String, at: usize) -> Kept {
        Kept { reason, at }
    }

    fn insert(&mut self, at: usize, text: impl Into<String>) {
        self.changes.push(Change {
            range: at..at,
            replacement: text.into(),
        });
    }

    fn main(&self) -> usize {
        usize::from(self.shape.vertical())
    }

    fn positioner(&mut self, object: &QmlObject<'_>) -> Result<(), Kept> {
        let qt = self.shape.qt();
        for (name, binding) in bindings(object) {
            if POSITIONER_ONLY.contains(&name.as_str()) {
                return Err(self.keep(
                    format!("`{name}` is a member of Qt's `{qt}` that Ljós's has not"),
                    binding.span.start as usize,
                ));
            }
        }
        for name in POSITIONER_ONLY_NAMES {
            if let Some(at) = self.source.find(name) {
                let name = name.trim_end_matches('.');
                return Err(self.keep(
                    format!("it uses `{name}`, which only Qt's positioners have"),
                    at,
                ));
            }
        }
        for child in self.laid_out(object)? {
            for (name, binding) in bindings(child) {
                self.placed(&name, "", binding.span.start as usize)?;
            }
            // What a file's root binds places it as much as what the child
            // binds; Qt's positioner ignores both alike.
            let ty = child.type_name.to_string();
            if self.types.qt_type(&ty).is_none()
                && let Some(names) = self.types.root_bindings(&ty)
            {
                let at = child.type_name.span.start as usize;
                for name in names {
                    self.placed(&name, &format!("`{ty}`'s root sets `{name}`, which "), at)?;
                }
            }
        }
        Ok(())
    }

    /// Why a positioner's child binding `name` keeps the file, if it does;
    /// `whose` leads the reason when the binding is not the child's own.
    fn placed(&self, name: &str, whose: &str, at: usize) -> Result<(), Kept> {
        let qt = self.shape.qt();
        let cross = ["x", "y"][1 - self.main()];
        if name.starts_with("Layout.") {
            let reason = if whose.is_empty() {
                format!("`{name}` is ignored by Qt's `{qt}` and would be honoured by Ljós's")
            } else {
                format!("{whose}is ignored by Qt's `{qt}` and would be honoured by Ljós's")
            };
            return Err(self.keep(reason, at));
        }
        if name.starts_with("anchors.") {
            return Err(self.anchored(name, at));
        }
        if name == cross {
            let reason = if whose.is_empty() {
                format!("Qt's `{qt}` leaves a child's `{cross}` to it, and Ljós's sets it")
            } else {
                format!("{whose}Qt's `{qt}` leaves to it, and Ljós's sets")
            };
            return Err(self.keep(reason, at));
        }
        Ok(())
    }

    fn anchored(&self, name: &str, at: usize) -> Kept {
        self.keep(
            format!(
                "`{name}` on a child of a `{}` is overridden by it, and is an error in Ljós's `{}`",
                self.shape.qt(),
                self.shape.ljos()
            ),
            at,
        )
    }

    fn layout(&mut self, object: &QmlObject<'_>) -> Result<(), Kept> {
        let (qt, ljos) = (self.shape.qt(), self.shape.ljos());
        let main = self.main();
        let own = bindings(object);
        if let Some((_, b)) = own.iter().find(|(n, _)| n == "uniformCellSizes") {
            return Err(self.keep(
                format!("`uniformCellSizes` is not in Ljós's `{ljos}` yet"),
                b.span.start as usize,
            ));
        }
        let children = self.laid_out(object)?;
        let mut fills = Some(false);
        for child in &children {
            match bound(child, FILL[main]).map(boolean) {
                Some(Some(true)) => fills = fills.map(|_| true),
                Some(None) => fills = None,
                _ => {}
            }
        }
        for child in children {
            self.child(child)?;
        }
        if fills != Some(true) && (sized(&own, main) || self.a_root) {
            // A root is sized by whatever uses it, which this file does not
            // show.
            let sized_by = if sized(&own, main) {
                ""
            } else {
                " (a file that uses it may size it)"
            };
            return Err(self.keep(
                format!(
                    "`{qt}` spreads the room no child fills over its children, and Ljós's \
                     `{ljos}` leaves it at the end{sized_by}; give a child `{}: true`",
                    FILL[main]
                ),
                object.type_name.span.start as usize,
            ));
        }
        let span = object.type_name.span;
        self.changes.push(Change {
            range: span.start as usize..span.end as usize,
            replacement: ljos.to_string(),
        });
        if !own.iter().any(|(n, _)| n == "spacing")
            && let Some(at) = after_brace(self.source, object)
        {
            self.insert(at, format!(" spacing: {LAYOUT_SPACING};"));
        }
        Ok(())
    }

    /// One child of a `RowLayout` or `ColumnLayout`.
    fn child(&mut self, child: &QmlObject<'_>) -> Result<(), Kept> {
        let (qt, ljos) = (self.shape.qt(), self.shape.ljos());
        let name = child.type_name.to_string();
        let at = child.type_name.span.start as usize;
        let own = bindings(child);
        if let Some((n, b)) = own.iter().find(|(n, _)| n.starts_with("anchors.")) {
            return Err(self.anchored(n, b.span.start as usize));
        }
        if self
            .types
            .qt_type(&name)
            .and_then(Shape::of)
            .is_some_and(|s| matches!(s, Shape::Layout { .. }))
        {
            return Err(self.keep(
                format!(
                    "a `{name}` in a `{qt}` fills it and is bounded by its children, and \
                     one in Ljós's `{ljos}` is not"
                ),
                at,
            ));
        }
        let zero = ZERO_IMPLICIT.contains(&name.as_str()) && self.types.qt_type(&name).is_some();
        let mut empty = false;
        for axis in [W, H] {
            let size = match self.request(child, &own, &name, zero, axis)? {
                Some(size) => size,
                None => continue,
            };
            empty |= size == 0.0;
        }
        let says_layout = own.iter().any(|(n, _)| n.starts_with("Layout."));
        if !self.shape.vertical() {
            // Qt's `RowLayout` centres a child across the row unless it says
            // otherwise; Ljós's `Row` puts it at the top.
            match bindings(child)
                .into_iter()
                .find(|(n, _)| n == "Layout.alignment")
            {
                Some((_, binding)) => self.centre(binding)?,
                None => {
                    if let Some(at) = after_brace(self.source, child) {
                        self.insert(at, " Layout.alignment: Qt.AlignVCenter;");
                    }
                }
            }
        } else if empty
            && !says_layout
            && let Some(at) = after_brace(self.source, child)
        {
            // Qt's layout counts an empty child, Ljós's (as a positioner)
            // leaves it out unless it is given a `Layout`.
            self.insert(at, " Layout.fillWidth: false;");
        }
        Ok(())
    }

    /// The size `child` asks for along `axis`, when the pass can tell, or
    /// why the file is kept. `None` when it fills or is bounded and asks for
    /// no size of its own the two disagree on.
    fn request(
        &self,
        child: &QmlObject<'_>,
        own: &[(String, &QmlBinding<'_>)],
        name: &str,
        zero: bool,
        axis: usize,
    ) -> Result<Option<f64>, Kept> {
        let qt = self.shape.qt();
        let ljos = self.shape.ljos();
        let whole = |b: &QmlBinding<'_>, what: &str| -> Result<f64, Kept> {
            match number(&b.value) {
                Some(n) if n >= 0.0 && n.fract() == 0.0 => Ok(n),
                _ => Err(self.keep(
                    format!(
                        "`{what}` is not a whole number the pass can read; `{qt}` rounds a \
                         child's size up and Ljós's `{ljos}` does not"
                    ),
                    b.span.start as usize,
                )),
            }
        };
        let find = |n: &str| own.iter().find(|(m, _)| m == n).map(|(_, b)| *b);
        for bound in [MINIMUM[axis], MAXIMUM[axis]] {
            if let Some(b) = find(bound) {
                whole(b, bound)?;
            }
        }
        if let Some(b) = find(PREFERRED[axis]) {
            return whole(b, PREFERRED[axis]).map(Some);
        }
        let implicit = find(IMPLICIT[axis]);
        if let Some(b) = find(SIZE[axis]) {
            let zero_implicit = match implicit {
                Some(i) => number(&i.value) == Some(0.0),
                None => zero,
            };
            if !zero_implicit {
                return Err(self.keep(
                    format!(
                        "`{}` of a `{name}` in a `{qt}` is not its size there, and would be in \
                         Ljós's `{ljos}`; give it `{}`",
                        SIZE[axis], PREFERRED[axis]
                    ),
                    b.span.start as usize,
                ));
            }
            return whole(b, SIZE[axis]).map(Some);
        }
        match implicit {
            Some(b) => whole(b, IMPLICIT[axis]).map(Some),
            None if zero => Ok(Some(0.0)),
            None => Err(self.keep(
                format!(
                    "the {} of a `{name}` may be a fraction, which `{qt}` rounds up and Ljós's \
                     `{ljos}` does not",
                    if axis == W {
                        "implicit width"
                    } else {
                        "implicit height"
                    }
                ),
                child.type_name.span.start as usize,
            )),
        }
    }

    /// A `RowLayout` child's own `Layout.alignment`, made to say it is
    /// centred across the row when it says nothing of that.
    fn centre(&mut self, binding: &QmlBinding<'_>) -> Result<(), Kept> {
        let QmlBindingValue::Expression(e) = &binding.value else {
            return Err(self.keep(
                "`Layout.alignment` is not an expression".into(),
                binding.span.start as usize,
            ));
        };
        let span = e.span();
        let flags = flags(e)
            .filter(|_| !self.source[span.start as usize..span.end as usize].contains('\n'));
        let Some(flags) = flags else {
            return Err(self.keep(
                "`Layout.alignment` is not one the pass can read, and `RowLayout` centres a \
                 child that says nothing of how it is aligned across the row, where Ljós's \
                 `Row` puts it at the top"
                    .into(),
                span.start as usize,
            ));
        };
        if flags.contains(&"AlignBaseline") {
            return Err(self.keep(
                "`Qt.AlignBaseline` is not in Ljós's `Row` yet".into(),
                span.start as usize,
            ));
        }
        let vertical = ["AlignTop", "AlignBottom", "AlignVCenter", "AlignCenter"];
        if !flags.iter().any(|f| vertical.contains(f)) {
            self.insert(span.end as usize, " | Qt.AlignVCenter");
        }
        Ok(())
    }

    /// The items `object` lays out: its children that are items, and for a
    /// `Repeater` among them, its delegate.
    fn laid_out<'o, 'a>(&self, object: &'o QmlObject<'a>) -> Result<Vec<&'o QmlObject<'a>>, Kept> {
        let mut out = Vec::new();
        for member in &object.members {
            match member {
                QmlMember::Object(child) if !is_group(child) => {
                    let name = child.type_name.to_string();
                    let Some(ty) = self.types.qt_type(&name) else {
                        if matches!(self.shape, Shape::Positioner { .. }) {
                            // A positioner places any item the same as Ljós's
                            // arrangement does; what it is does not matter.
                            out.push(child);
                            continue;
                        }
                        return Err(self.keep(
                            format!(
                                "`{name}` is a file of the project, whose size in a `{}` the \
                                 pass does not know",
                                self.shape.qt()
                            ),
                            child.type_name.span.start as usize,
                        ));
                    };
                    if ty.name == "Repeater" {
                        out.push(self.delegate(child)?);
                    } else if is_item(ty) {
                        out.push(child);
                    }
                }
                QmlMember::Binding(binding)
                    if matches!(binding.name.as_simple(), Some("children" | "data")) =>
                {
                    return Err(self.keep(
                        format!(
                            "the children of a `{}` are given through `{}`",
                            self.shape.qt(),
                            binding.name
                        ),
                        binding.span.start as usize,
                    ));
                }
                _ => {}
            }
        }
        Ok(out)
    }

    fn delegate<'o, 'a>(&self, repeater: &'o QmlObject<'a>) -> Result<&'o QmlObject<'a>, Kept> {
        let mut found = None;
        for member in &repeater.members {
            match member {
                QmlMember::Object(o) if !is_group(o) => found = Some(o),
                QmlMember::Binding(b) if b.name.as_simple() == Some("delegate") => {
                    if let QmlBindingValue::Object(o) = &b.value
                        && o.type_name.as_simple() != Some("Component")
                    {
                        found = Some(o);
                    } else {
                        found = None;
                        break;
                    }
                }
                _ => {}
            }
        }
        found.ok_or_else(|| {
            self.keep(
                "the delegate of a `Repeater` is not one the pass can read".into(),
                repeater.type_name.span.start as usize,
            )
        })
    }
}

/// `anchors { }`, `font { }`: a group of the object, not a child.
pub(crate) fn is_group(object: &QmlObject<'_>) -> bool {
    object
        .type_name
        .parts
        .first()
        .is_some_and(|p| p.starts_with(|c: char| c.is_ascii_lowercase()))
}

fn is_item(ty: &'static Type) -> bool {
    let mut at = Some(ty);
    while let Some(t) = at {
        if t.name == "Item" && t.module == "QtQuick" {
            return true;
        }
        at = t.prototype();
    }
    false
}

/// Every binding of `object` by its dotted name, with those of its groups
/// (`anchors { left: … }` as `anchors.left`).
fn bindings<'o, 'a>(object: &'o QmlObject<'a>) -> Vec<(String, &'o QmlBinding<'a>)> {
    let mut out = Vec::new();
    for member in &object.members {
        match member {
            QmlMember::Binding(b) => out.push((b.name.to_string(), b)),
            QmlMember::Object(group) if is_group(group) => {
                for (name, b) in bindings(group) {
                    out.push((format!("{}.{name}", group.type_name), b));
                }
            }
            _ => {}
        }
    }
    out
}

/// The names `object` binds, as [`bindings`] has them.
pub(crate) fn binding_names(object: &QmlObject<'_>) -> Vec<String> {
    bindings(object).into_iter().map(|(n, _)| n).collect()
}

fn bound<'o, 'a>(object: &'o QmlObject<'a>, name: &str) -> Option<&'o QmlBindingValue<'a>> {
    bindings(object)
        .into_iter()
        .find(|(n, _)| n == name)
        .map(|(_, b)| &b.value)
}

/// Whether the layout is given its size along `axis` by the file, so that
/// it can have room its children do not take.
fn sized(own: &[(String, &QmlBinding<'_>)], axis: usize) -> bool {
    let has = |n: &str| own.iter().any(|(m, _)| m == n);
    let (a, b) = if axis == W {
        ("anchors.left", "anchors.right")
    } else {
        ("anchors.top", "anchors.bottom")
    };
    has(SIZE[axis]) || has("anchors.fill") || (has(a) && has(b)) || has(FILL[axis])
}

fn number(value: &QmlBindingValue<'_>) -> Option<f64> {
    match value {
        QmlBindingValue::Expression(Expression::NumericLiteral(n)) => Some(n.value),
        _ => None,
    }
}

fn boolean(value: &QmlBindingValue<'_>) -> Option<bool> {
    match value {
        QmlBindingValue::Expression(Expression::BooleanLiteral(b)) => Some(b.value),
        _ => None,
    }
}

/// `Qt.AlignLeft | Qt.AlignVCenter`: the flags, when that is all it is.
fn flags<'a>(e: &Expression<'a>) -> Option<Vec<&'a str>> {
    match e {
        Expression::StaticMemberExpression(m) => match &m.object {
            Expression::Identifier(id) if id.name == "Qt" => Some(vec![m.property.name.as_str()]),
            _ => None,
        },
        Expression::BinaryExpression(b) if b.operator == BinaryOperator::BitwiseOR => {
            let mut l = flags(&b.left)?;
            l.extend(flags(&b.right)?);
            Some(l)
        }
        Expression::ParenthesizedExpression(p) => flags(&p.expression),
        _ => None,
    }
}

/// Just after the `{` that opens `object`'s members.
fn after_brace(source: &str, object: &QmlObject<'_>) -> Option<usize> {
    let from = object.type_name.span.end as usize;
    let range: Range<usize> = from..object.span.end as usize;
    source.get(range)?.find('{').map(|i| from + i + 1)
}
