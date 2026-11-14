//! Qt's positioners and line layouts lowered to `Ljos.Layout`'s one
//! arrangement (`Row`, `Column`, `Grid`), and Qt's `Flow` to Ljós's
//! (docs/DIALECT.md §4.6).
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
//! - a size the file gives that is not a whole number: `RowLayout` rounds
//!   it up, Ljós's `Row` does not;
//! - a layout with nothing to fill it that may be sized along its line:
//!   `RowLayout` spreads the room, Ljós's `Row` leaves it at the end. The
//!   file may size it, or, for the root of the file, of an inline component
//!   or of a `Component`, or an object given as a value, whatever uses or
//!   holds it may, which the file does not show;
//! - a layout in a layout that is sized, bounded or told to fill where
//!   none of its children can grow;
//! - an anchored child, and `uniformCellSizes`.
//!
//! An implicit size that may be a fraction (a `Text`'s) is rounded up by
//! both, as Ljós's box is a layout when a child has a `Layout`; the pass
//! gives a child of a `ColumnLayout` one where none says anything through
//! it. A `RowLayout` or `ColumnLayout` in a layout grows along an axis
//! where one of its children fills with no maximum, as Ljós's box does
//! when it is told to fill, and is held to its children's minimum, as
//! Ljós's is; the pass writes the fill out.
//!
//! A `GridLayout` is a `Grid` held to the same on both axes, with Qt's
//! `columnSpacing` and `rowSpacing` of 5 written out. It is kept where Qt's
//! reads it otherwise: a count across its flow (Qt's ignores it), a child
//! placed in a cell of its own (`Layout.row`, `column`, `rowSpan`,
//! `columnSpan`), `layoutDirection`, or uniform cells.
//!
//! A `Row` positioner is lowered unless a child says something through
//! `Layout.` (Qt's ignores it, Ljós's would not), a child sets its own
//! position across the row (Qt's leaves it, Ljós's sets it), a child is
//! anchored, or the file uses what only a positioner has (`add`, `move`,
//! `populate`, `Positioner`, `effectiveLayoutDirection`, `forceLayout`). A
//! child that is a file of the project or an inline component is held to
//! the same by what its root binds.
//!
//! A `Grid` positioner is held to the same, but for a child's own position,
//! which both set on both axes. It is lowered unless it sets what Ljós's
//! `Grid` has not (`horizontalItemAlignment`, `verticalItemAlignment`,
//! `layoutDirection`), or Qt's would count room Ljós's does not: the
//! spacing after a line of children it does not fill, that of a row or
//! column it leaves empty, and a child beyond its rows and columns, which
//! Qt's leaves where it is. Where the pass cannot count the children (a
//! `Repeater`, a `visible` it cannot read, an item that may have no size)
//! it keeps the file if a count could tell the two apart. Qt's `Grid` has
//! four columns unless told; the pass writes them out, as Ljós's `Grid` is
//! one line unless told.
//!
//! Ljós's `Flow` is Qt's; a `Flow` is held to a positioner's rules so that
//! what Ljós's arrangement would read differently is not written to it.

use std::ops::Range;

use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use oxc_syntax::operator::BinaryOperator;
use qml_edit::Change;
use qml_syntax::ast::{QmlBinding, QmlBindingValue, QmlMember, QmlObject};
use qt_types::Type;

use crate::cause::{Differs, Layout};
use crate::rules::{File, Rule};
use crate::{Kept, Ty};

/// The rule of what the pass lowers to an arrangement.
pub(crate) struct ArrangeRule;

pub(crate) const ARRANGE: ArrangeRule = ArrangeRule;

/// Each type of Qt's an arrangement is lowered from: its module, its name
/// and its shape.
const SHAPES: &[(&str, &str, Shape)] = &[
    ("QtQuick", "Row", Shape::Positioner { vertical: false }),
    ("QtQuick", "Column", Shape::Positioner { vertical: true }),
    ("QtQuick", "Grid", Shape::Grid),
    ("QtQuick", "Flow", Shape::Flow),
    (
        "QtQuick.Layouts",
        "RowLayout",
        Shape::Layout { vertical: false },
    ),
    (
        "QtQuick.Layouts",
        "ColumnLayout",
        Shape::Layout { vertical: true },
    ),
    ("QtQuick.Layouts", "GridLayout", Shape::GridLayout),
];

impl Rule for ArrangeRule {
    fn owns(&self) -> &'static [&'static str] {
        // The names of `SHAPES`, which a test holds equal.
        &[
            "Row",
            "Column",
            "Grid",
            "Flow",
            "RowLayout",
            "ColumnLayout",
            "GridLayout",
        ]
    }

    fn object(
        &self,
        object: &QmlObject<'_>,
        on: Ty,
        a_root: bool,
        file: &dyn File,
    ) -> Result<Vec<Change>, Kept> {
        // An object of a type built on one is the type's own: its file
        // says how it arranges.
        let Ty::Exact(ty) = on else {
            return Ok(Vec::new());
        };
        match Shape::of(ty) {
            Some(shape) => arrange(file.source(), object, shape, a_root, file),
            None => Ok(Vec::new()),
        }
    }
}

/// What of Qt's an object is that the pass lowers to an arrangement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    /// `Row` or `Column` of QtQuick.
    Positioner { vertical: bool },
    /// `RowLayout` or `ColumnLayout` of QtQuick.Layouts.
    Layout { vertical: bool },
    /// `Grid` of QtQuick, the positioner.
    Grid,
    /// `Flow` of QtQuick.
    Flow,
    /// `GridLayout` of QtQuick.Layouts.
    GridLayout,
}

impl Shape {
    fn of(ty: &Type) -> Option<Shape> {
        SHAPES
            .iter()
            .find(|(module, name, _)| *module == ty.module && *name == ty.name)
            .map(|(_, _, shape)| *shape)
    }

    fn vertical(self) -> bool {
        match self {
            Shape::Positioner { vertical } | Shape::Layout { vertical } => vertical,
            Shape::Grid | Shape::Flow | Shape::GridLayout => false,
        }
    }

    /// Whether Qt's places its children, as a positioner, and does not size
    /// them.
    fn positions(self) -> bool {
        !matches!(self, Shape::Layout { .. } | Shape::GridLayout)
    }

    /// What Ljós calls it.
    fn ljos(self) -> &'static str {
        match self {
            Shape::Grid | Shape::GridLayout => "Grid",
            Shape::Flow => "Flow",
            _ if self.vertical() => "Column",
            _ => "Row",
        }
    }

    fn qt(self) -> &'static str {
        match self {
            Shape::Positioner { vertical: false } => "Row",
            Shape::Positioner { vertical: true } => "Column",
            Shape::Layout { vertical: false } => "RowLayout",
            Shape::Layout { vertical: true } => "ColumnLayout",
            Shape::Grid => "Grid",
            Shape::Flow => "Flow",
            Shape::GridLayout => "GridLayout",
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

/// The edits that lower `object`, a `shape`, or why its file is kept.
fn arrange(
    source: &str,
    object: &QmlObject<'_>,
    shape: Shape,
    a_root: bool,
    types: &dyn File,
) -> Result<Vec<Change>, Kept> {
    let mut a = Arrange {
        source,
        shape,
        a_root,
        types,
        changes: Vec::new(),
    };
    match shape {
        Shape::Positioner { .. } | Shape::Flow => a.positioner(object)?,
        Shape::Grid => {
            a.positioner(object)?;
            a.grid(object)?;
        }
        Shape::Layout { .. } | Shape::GridLayout => a.layout(object)?,
    }
    Ok(a.changes)
}

struct Arrange<'s, 't> {
    source: &'s str,
    shape: Shape,
    /// The root of the file or of an inline component: a file that uses
    /// it may size it.
    a_root: bool,
    types: &'t dyn File,
    changes: Vec<Change>,
}

/// The anchors that place a child on each axis, by the axis: what a
/// positioner along the other axis leaves to it.
const ACROSS: [&[&str]; 2] = [
    &[
        "left",
        "right",
        "horizontalCenter",
        "leftMargin",
        "rightMargin",
        "horizontalCenterOffset",
    ],
    &[
        "top",
        "bottom",
        "verticalCenter",
        "baseline",
        "topMargin",
        "bottomMargin",
        "verticalCenterOffset",
        "baselineOffset",
    ],
];

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
    fn keep(&self, cause: Layout, at: usize) -> Kept {
        Kept {
            cause: cause.into(),
            at,
        }
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
                // Ljós's `Flow` is Qt's, and has them.
                let why = if self.shape == Shape::Flow {
                    Layout::FlowOnly { name }
                } else {
                    Layout::PositionerOnly { name, qt }
                };
                return Err(self.keep(why, binding.span.start as usize));
            }
        }
        for name in POSITIONER_ONLY_NAMES {
            if let Some(at) = self.source.find(name) {
                let name = name.trim_end_matches('.');
                return Err(self.keep(
                    Layout::PositionersUse {
                        name: name.to_owned(),
                    },
                    at,
                ));
            }
        }
        for child in self.laid_out(object)? {
            for (name, binding) in bindings(child) {
                self.placed(&name, None, binding.span.start as usize)?;
            }
            // What a file's root binds places it as much as what the child
            // binds; Qt's positioner ignores both alike.
            let ty = child.type_name.to_string();
            if self.types.qt_type(&ty).is_none()
                && let Some(names) = self.types.root_bindings(&ty)
            {
                let at = child.type_name.span.start as usize;
                for name in names {
                    self.placed(&name, Some(&ty), at)?;
                }
            }
        }
        Ok(())
    }

    /// Why a positioner's child binding `name` keeps the file, if it does;
    /// `root` is the type whose root binds it, when the child does not.
    fn placed(&self, name: &str, root: Option<&str>, at: usize) -> Result<(), Kept> {
        let qt = self.shape.qt();
        let cross = ["x", "y"][1 - self.main()];
        let root = root.map(str::to_owned);
        if name.starts_with("Layout.") {
            let name = name.to_owned();
            return Err(self.keep(Layout::Ignored { name, root, qt }, at));
        }
        if let Some(anchor) = name.strip_prefix("anchors.") {
            // Qt's positioner places a child along it alone: an anchor across
            // it holds there, where Ljós's places the child by its
            // alignment, which rounds a centre where Qt's anchor does not.
            if matches!(self.shape, Shape::Positioner { .. })
                && ACROSS[1 - self.main()].contains(&anchor)
            {
                let (qt, ljos) = (self.shape.qt(), self.shape.ljos());
                let name = name.to_owned();
                return Err(self.keep(Layout::AnchoredAcross { name, qt, ljos }, at));
            }
            return Err(self.anchored(name, at));
        }
        if matches!(self.shape, Shape::Positioner { .. }) && name == cross {
            let name = cross.to_owned();
            return Err(self.keep(Layout::Crossed { name, root, qt }, at));
        }
        Ok(())
    }

    /// A `Grid` positioner, past what every positioner is held to.
    fn grid(&mut self, object: &QmlObject<'_>) -> Result<(), Kept> {
        let own = bindings(object);
        let find = |n: &str| own.iter().find(|(m, _)| m == n).map(|(_, b)| *b);
        for name in [
            "horizontalItemAlignment",
            "verticalItemAlignment",
            "layoutDirection",
        ] {
            if let Some(b) = find(name) {
                return Err(self.keep(
                    Layout::NotYet {
                        name: name.to_owned(),
                        ljos: "Grid",
                    },
                    b.span.start as usize,
                ));
            }
        }
        let at = object.type_name.span.start as usize;
        let count = |name: &str| -> Result<Option<usize>, Kept> {
            let Some(b) = find(name) else {
                return Ok(None);
            };
            match number(&b.value) {
                Some(n) if n >= 0.0 && n.fract() == 0.0 => Ok(Some(n as usize).filter(|n| *n > 0)),
                _ => Err(self.keep(
                    Layout::GridCount {
                        name: name.to_owned(),
                    },
                    b.span.start as usize,
                )),
            }
        };
        let (rows, columns) = (count("rows")?, count("columns")?);
        let top_to_bottom = self.top_to_bottom(&own)?;
        // A gap the pass cannot read is one that may not be 0.
        let gap = |name: &str| match find(name).or_else(|| find("spacing")) {
            None => Some(0.0),
            Some(b) => number(&b.value),
        };
        let gaps = [gap("columnSpacing"), gap("rowSpacing")];
        let (along, across) = if top_to_bottom { (1, 0) } else { (0, 1) };
        let spaced = |axis: usize| gaps[axis] != Some(0.0);
        let shown = self.shown(object);
        // How Qt's `Grid` counts its cells for `n` children: along its
        // flow, then across.
        let given = if top_to_bottom {
            (rows, columns)
        } else {
            (columns, rows)
        };
        let differs = |n: usize| -> Option<Differs> {
            let (line, lines) = match given {
                (Some(l), ls) => (l, ls),
                (None, Some(ls)) => (n.div_ceil(ls).max(1), Some(ls)),
                (None, None) if top_to_bottom => (n.div_ceil(4).max(1), Some(4)),
                (None, None) => (4, None),
            };
            let placed = match (given, lines) {
                ((Some(l), Some(ls)), _) => n.min(l * ls),
                _ => n,
            };
            let a = if top_to_bottom { "column" } else { "row" };
            if matches!(given, (Some(_), Some(_))) && placed < n {
                return Some(Differs::Beyond);
            }
            if spaced(along) && n > 0 && n < line {
                return Some(Differs::SpacingAfter { line: a });
            }
            if let Some(ls) = lines
                && spaced(across)
                && placed.div_ceil(line) < ls
            {
                return Some(Differs::EmptyLine {
                    line: a,
                    lines: ls,
                    children: placed,
                    per_line: line,
                });
            }
            None
        };
        match shown {
            Shown::Between(least, most) => {
                for n in least..=most {
                    if let Some(differs) = differs(n) {
                        let for_some = least != most;
                        return Err(self.keep(Layout::Grid { differs, for_some }, at));
                    }
                }
            }
            Shown::Unknown(why) => {
                if spaced(along) || spaced(across) || matches!(given, (Some(_), Some(_))) {
                    return Err(self.keep(Layout::GridShown { why }, at));
                }
            }
        }
        // Qt's `Grid` has four columns unless told, and a count of 0 tells
        // it nothing (qml6); Ljós's is one line.
        if rows.is_none() && columns.is_none() {
            if let Some(b) = find("columns")
                && let QmlBindingValue::Expression(Expression::NumericLiteral(zero)) = &b.value
            {
                self.changes.push(Change {
                    range: zero.span.start as usize..zero.span.end as usize,
                    replacement: "4".into(),
                });
            } else if let Some(at) = after_brace(self.source, object) {
                self.insert(at, " columns: 4;");
            }
        }
        Ok(())
    }

    /// Whether a `Grid` or `GridLayout` lays its children out column after
    /// column, as far as the pass can read its `flow`.
    fn top_to_bottom(&self, own: &[(String, &QmlBinding<'_>)]) -> Result<bool, Kept> {
        let Some((_, b)) = own.iter().find(|(n, _)| n == "flow") else {
            return Ok(false);
        };
        let qt = self.shape.qt();
        match &b.value {
            QmlBindingValue::Expression(Expression::StaticMemberExpression(m)) if matches!(&m.object, Expression::Identifier(id) if id.name == qt) => {
                match m.property.name.as_str() {
                    "LeftToRight" => Ok(false),
                    "TopToBottom" => Ok(true),
                    _ => Err(self.unread("flow", b)),
                }
            }
            _ => Err(self.unread("flow", b)),
        }
    }

    fn unread(&self, name: &str, b: &QmlBinding<'_>) -> Kept {
        self.keep(
            Layout::GridUnread {
                name: name.to_owned(),
            },
            b.span.start as usize,
        )
    }

    /// How many of `object`'s children a positioner places: one with no
    /// size or not visible it leaves out.
    fn shown(&self, object: &QmlObject<'_>) -> Shown {
        let (mut least, mut most) = (0, 0);
        for member in &object.members {
            let QmlMember::Object(child) = member else {
                continue;
            };
            if is_group(child) {
                continue;
            }
            let name = child.type_name.to_string();
            let ty = self.types.qt_type(&name);
            if ty.is_some_and(|t| t.name == "Repeater") {
                return Shown::Unknown("it has a `Repeater`".into());
            }
            if ty.is_some_and(|t| !is_item(t)) {
                continue;
            }
            let own = bindings(child);
            let value = |n: &str| own.iter().find(|(m, _)| m == n).map(|(_, b)| &b.value);
            let mut maybe = match value("visible") {
                None => false,
                Some(v) => match boolean(v) {
                    Some(true) => false,
                    Some(false) => continue,
                    None => true,
                },
            };
            let size = |axis: usize| value(SIZE[axis]).or_else(|| value(IMPLICIT[axis]));
            if [W, H]
                .iter()
                .any(|a| size(*a).and_then(number) == Some(0.0))
            {
                continue;
            }
            // An item with no size of its own unless given one; a file of the
            // project may be such an item.
            let sizeless = ty.is_none_or(|t| ZERO_IMPLICIT.contains(&t.name));
            if sizeless
                && ![W, H]
                    .iter()
                    .all(|a| size(*a).and_then(number).is_some_and(|n| n > 0.0))
            {
                maybe = true;
            }
            most += 1;
            if !maybe {
                least += 1;
            }
        }
        Shown::Between(least, most)
    }

    fn anchored(&self, name: &str, at: usize) -> Kept {
        self.keep(
            Layout::Anchored {
                name: name.to_owned(),
                qt: self.shape.qt(),
                ljos: self.shape.ljos(),
            },
            at,
        )
    }

    fn layout(&mut self, object: &QmlObject<'_>) -> Result<(), Kept> {
        let ljos = self.shape.ljos();
        let main = self.main();
        let own = bindings(object);
        let uniform = [
            "uniformCellSizes",
            "uniformCellWidths",
            "uniformCellHeights",
        ];
        if let Some((n, b)) = own.iter().find(|(n, _)| uniform.contains(&n.as_str())) {
            return Err(self.keep(
                Layout::NotYet {
                    name: n.clone(),
                    ljos,
                },
                b.span.start as usize,
            ));
        }
        let grid = self.shape == Shape::GridLayout;
        if grid {
            self.grid_layout(&own)?;
        }
        let children = self.laid_out(object)?;
        for child in &children {
            self.child(child)?;
        }
        // A grid has room to spread on both axes, a line along its own.
        let axes = if grid { vec![W, H] } else { vec![main] };
        for axis in axes {
            self.spread(object, &own, &children, axis)?;
        }
        let span = object.type_name.span;
        self.changes.push(Change {
            range: span.start as usize..span.end as usize,
            replacement: ljos.to_string(),
        });
        let has = |n: &str| own.iter().any(|(m, _)| m == n);
        let defaults = if grid {
            ["columnSpacing", "rowSpacing"]
                .into_iter()
                .filter(|n| !has(n))
                .map(|n| format!(" {n}: {LAYOUT_SPACING};"))
                .collect::<String>()
        } else if has("spacing") {
            String::new()
        } else {
            format!(" spacing: {LAYOUT_SPACING};")
        };
        if !defaults.is_empty()
            && let Some(at) = after_brace(self.source, object)
        {
            self.insert(at, defaults);
        }
        Ok(())
    }

    /// What a `GridLayout` says that Ljós's `Grid` would read otherwise.
    fn grid_layout(&self, own: &[(String, &QmlBinding<'_>)]) -> Result<(), Kept> {
        let find = |n: &str| own.iter().find(|(m, _)| m == n).map(|(_, b)| *b);
        if let Some(b) = find("layoutDirection") {
            return Err(self.keep(
                Layout::NotYet {
                    name: "layoutDirection".to_owned(),
                    ljos: "Grid",
                },
                b.span.start as usize,
            ));
        }
        // Qt's reads only the count along its flow; Ljós's reads both.
        let ignored = if self.top_to_bottom(own)? {
            ("columns", "top-to-bottom")
        } else {
            ("rows", "left-to-right")
        };
        if let Some(b) = find(ignored.0) {
            return Err(self.keep(
                Layout::GridLayoutIgnored {
                    name: ignored.0,
                    flow: ignored.1,
                },
                b.span.start as usize,
            ));
        }
        Ok(())
    }

    /// Keeps the file where Qt's layout would spread room along `axis` that
    /// no child fills, which Ljós's leaves at the end.
    fn spread(
        &self,
        object: &QmlObject<'_>,
        own: &[(String, &QmlBinding<'_>)],
        children: &[&QmlObject<'_>],
        axis: usize,
    ) -> Result<(), Kept> {
        let (qt, ljos) = (self.shape.qt(), self.shape.ljos());
        let mut fills = Some(false);
        for child in children {
            let name = child.type_name.to_string();
            let told = match self.line_layout(&name) {
                Some(line) => Some(Some(self.nested_fills(child, line, axis)?)),
                None => bound(child, FILL[axis]).map(boolean),
            };
            match told {
                Some(Some(true)) => fills = fills.map(|_| true),
                Some(None) => fills = None,
                _ => {}
            }
        }
        if fills != Some(true) && (sized(own, axis) || self.a_root) {
            // A root is sized by whatever uses it, which this file does not
            // show.
            return Err(self.keep(
                Layout::Spreads {
                    qt,
                    ljos,
                    root: !sized(own, axis),
                    fill: FILL[axis],
                },
                object.type_name.span.start as usize,
            ));
        }
        Ok(())
    }

    /// One child of a `RowLayout`, `ColumnLayout` or `GridLayout`.
    fn child(&mut self, child: &QmlObject<'_>) -> Result<(), Kept> {
        let (qt, ljos) = (self.shape.qt(), self.shape.ljos());
        let name = child.type_name.to_string();
        let at = child.type_name.span.start as usize;
        let own = bindings(child);
        if let Some((n, b)) = own.iter().find(|(n, _)| n.starts_with("anchors.")) {
            return Err(self.anchored(n, b.span.start as usize));
        }
        if self.shape == Shape::GridLayout {
            let cells = [
                "Layout.row",
                "Layout.column",
                "Layout.rowSpan",
                "Layout.columnSpan",
            ];
            if let Some((n, b)) = own.iter().find(|(n, _)| cells.contains(&n.as_str())) {
                return Err(self.keep(Layout::Cell { name: n.clone() }, b.span.start as usize));
            }
        }
        let says_layout = own.iter().any(|(n, _)| n.starts_with("Layout."));
        // Whether Ljós's box may leave the child out, or keep a size from its
        // content as it is: one whose children have no `Layout` does both.
        let mut needs_layout = false;
        if let Some(line) = self.line_layout(&name) {
            for axis in [W, H] {
                if self.nested_fills(child, line, axis)?
                    && bound(child, FILL[axis]).is_none()
                    && let Some(at) = after_brace(self.source, child)
                {
                    self.insert(at, format!(" {}: true;", FILL[axis]));
                }
            }
        } else if self
            .types
            .qt_type(&name)
            .is_some_and(|t| t.module == "QtQuick.Layouts" && t.name != "Layout")
        {
            // A `GridLayout` or `StackLayout` fills its cell and is bounded by
            // its children otherwise.
            return Err(self.keep(Layout::Fills { name, qt, ljos }, at));
        } else {
            let zero =
                ZERO_IMPLICIT.contains(&name.as_str()) && self.types.qt_type(&name).is_some();
            for axis in [W, H] {
                match self.request(&own, &name, zero, axis)? {
                    Ask::Size(size) => needs_layout |= size == 0.0,
                    Ask::Content => needs_layout = true,
                    Ask::Given => {}
                }
            }
        }
        if !self.shape.vertical() {
            // Qt's `RowLayout` and `GridLayout` centre a child across its row
            // unless it says otherwise; Ljós's `Row` and `Grid` put it at the
            // top.
            match bindings(child)
                .into_iter()
                .find(|(n, _)| n == "Layout.alignment")
            {
                Some((_, binding)) => self.centre(binding)?,
                None => {
                    if let Some(at) = after_brace(self.source, child) {
                        self.insert(at, " Layout.alignment: Align.VCenter;");
                    }
                }
            }
        } else if needs_layout
            && !says_layout
            && let Some(at) = after_brace(self.source, child)
        {
            // Qt's layout counts an empty child and rounds a size from its
            // content up; Ljós's box does both once a child has a `Layout`.
            self.insert(at, " Layout.fillWidth: false;");
        }
        Ok(())
    }

    /// Whether `name` is a `RowLayout` or `ColumnLayout`, and which.
    fn line_layout(&self, name: &str) -> Option<Shape> {
        let t = self.types.qt_type(name)?;
        if t.module != "QtQuick.Layouts" {
            return None;
        }
        match t.name {
            "RowLayout" => Some(Shape::Layout { vertical: false }),
            "ColumnLayout" => Some(Shape::Layout { vertical: true }),
            _ => None,
        }
    }

    /// Whether a `RowLayout` or `ColumnLayout` (`shape`) in a layout grows
    /// along `axis` in Qt's: where one of its children fills with no
    /// maximum. Keeps the file where Ljós's box would read it otherwise:
    /// one sized or bounded by the file, with a child that fills up to a
    /// maximum, or told to fill where none of its children can grow.
    fn nested_fills(
        &self,
        nested: &QmlObject<'_>,
        shape: Shape,
        axis: usize,
    ) -> Result<bool, Kept> {
        let (qt, ljos) = (self.shape.qt(), self.shape.ljos());
        let name = nested.type_name.to_string();
        let fills = |kept: &Self| {
            kept.keep(
                Layout::Fills {
                    name: name.clone(),
                    qt,
                    ljos,
                },
                nested.type_name.span.start as usize,
            )
        };
        let own = bindings(nested);
        let sizes = [
            SIZE[axis],
            IMPLICIT[axis],
            PREFERRED[axis],
            MINIMUM[axis],
            MAXIMUM[axis],
        ];
        if own
            .iter()
            .any(|(n, _)| sizes.contains(&n.as_str()) || n.starts_with("anchors."))
        {
            return Err(fills(self));
        }
        let inner = Arrange {
            source: self.source,
            shape,
            a_root: false,
            types: self.types,
            changes: Vec::new(),
        };
        let mut grows = false;
        for child in inner.laid_out(nested)? {
            // A hidden child takes no room in Qt's layout or in Ljós's box,
            // so it grows neither; one shown and hidden as the file runs
            // grows it only while shown, which no binding says.
            let shown = bound(child, "visible").map(boolean);
            if shown == Some(Some(false)) {
                continue;
            }
            let child_name = child.type_name.to_string();
            let can = match inner.line_layout(&child_name) {
                Some(line) => inner.nested_fills(child, line, axis)?,
                // One that fills up to a maximum lets Qt's layout grow
                // only that far, which a Ljós box that fills does not
                // stop at.
                None => match bound(child, FILL[axis]).map(boolean) {
                    Some(Some(true)) if bound(child, MAXIMUM[axis]).is_none() => true,
                    Some(Some(true)) | Some(None) => return Err(fills(self)),
                    _ => false,
                },
            };
            if can && shown == Some(None) {
                return Err(fills(self));
            }
            grows |= can;
        }
        match bound(nested, FILL[axis]).map(boolean) {
            Some(Some(true)) if !grows => Err(fills(self)),
            Some(Some(told)) => Ok(told && grows),
            Some(None) => Err(fills(self)),
            None => Ok(grows),
        }
    }

    /// The size `child` asks for along `axis`, or why the file is kept.
    fn request(
        &self,
        own: &[(String, &QmlBinding<'_>)],
        name: &str,
        zero: bool,
        axis: usize,
    ) -> Result<Ask, Kept> {
        let qt = self.shape.qt();
        let ljos = self.shape.ljos();
        let whole = |b: &QmlBinding<'_>, what: &str| -> Result<f64, Kept> {
            match number(&b.value) {
                Some(n) if n >= 0.0 && n.fract() == 0.0 => Ok(n),
                _ => Err(self.keep(
                    Layout::NotWhole {
                        what: what.to_owned(),
                        qt,
                        ljos,
                    },
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
            return whole(b, PREFERRED[axis]).map(|_| Ask::Given);
        }
        let implicit = find(IMPLICIT[axis]);
        if let Some(b) = find(SIZE[axis]) {
            let zero_implicit = match implicit {
                Some(i) => number(&i.value) == Some(0.0),
                None => zero,
            };
            if !zero_implicit {
                return Err(self.keep(
                    Layout::NotSize {
                        name: name.to_owned(),
                        qt,
                        ljos,
                        size: SIZE[axis],
                        preferred: PREFERRED[axis],
                    },
                    b.span.start as usize,
                ));
            }
            return whole(b, SIZE[axis]).map(Ask::Size);
        }
        // Both round a size from the content up.
        Ok(match implicit.and_then(|b| number(&b.value)) {
            Some(n) if n.fract() == 0.0 => Ask::Size(n),
            Some(_) => Ask::Content,
            None if implicit.is_none() && zero => Ask::Size(0.0),
            None => Ask::Content,
        })
    }

    /// A `RowLayout` child's own `Layout.alignment`, made to say it is
    /// centred across the row when it says nothing of that.
    fn centre(&mut self, binding: &QmlBinding<'_>) -> Result<(), Kept> {
        let QmlBindingValue::Expression(e) = &binding.value else {
            return Err(self.keep(Layout::AlignmentNoExpression, binding.span.start as usize));
        };
        let qt = self.shape.qt();
        let ljos = self.shape.ljos();
        let span = e.span();
        let flags = flags(e)
            .filter(|_| !self.source[span.start as usize..span.end as usize].contains('\n'));
        let Some(flags) = flags else {
            return Err(self.keep(Layout::AlignmentUnread { qt, ljos }, span.start as usize));
        };
        if flags.contains(&"AlignBaseline") {
            return Err(self.keep(
                Layout::NotYet {
                    name: "Qt.AlignBaseline".to_owned(),
                    ljos,
                },
                span.start as usize,
            ));
        }
        let vertical = ["AlignTop", "AlignBottom", "AlignVCenter", "AlignCenter"];
        if !flags.iter().any(|f| vertical.contains(f)) {
            self.insert(span.end as usize, " | Align.VCenter");
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
                        if self.shape.positions() {
                            // A positioner places any item the same as Ljós's
                            // arrangement does; what it is does not matter.
                            out.push(child);
                            continue;
                        }
                        return Err(self.keep(
                            Layout::FileSize {
                                name,
                                qt: self.shape.qt(),
                            },
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
                        Layout::ChildrenThrough {
                            qt: self.shape.qt(),
                            through: binding.name.to_string(),
                        },
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
                Layout::RepeaterDelegate,
                repeater.type_name.span.start as usize,
            )
        })
    }
}

/// What a layout's child asks for along one axis.
enum Ask {
    /// A size the file gives it, or the pass can read.
    Size(f64),
    /// One that fills or is given: Ljós's box sizes it as Qt's does.
    Given,
    /// A size from its content, which may be a fraction, or nothing.
    Content,
}

/// How many children a positioner places, as far as the pass can tell.
enum Shown {
    Between(usize, usize),
    Unknown(String),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rule_owns_the_names_of_its_shapes() {
        let names: Vec<&str> = SHAPES.iter().map(|(_, name, _)| *name).collect();
        assert_eq!(ARRANGE.owns(), names.as_slice());
    }
}
