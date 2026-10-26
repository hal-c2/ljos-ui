//! Layout and text that does not fit (`docs/design/layout.md`,
//! `docs/design/typography.md`).

use ljos_design::limits;
use qml_syntax::{ast::QmlObject, js::Expression};

use super::{Bound, Found, Frame, bindings, bound, children, class, is_singleton};

/// What a layout sets on its children itself.
const ANCHORS: &[&str] = &[
    "anchors.fill",
    "anchors.centerIn",
    "anchors.left",
    "anchors.right",
    "anchors.top",
    "anchors.bottom",
    "anchors.horizontalCenter",
    "anchors.verticalCenter",
    "anchors.baseline",
];

/// What makes a text's width bounded.
const BOUNDED_WIDTH: &[&str] = &[
    "width",
    "anchors.fill",
    "Layout.fillWidth",
    "Layout.maximumWidth",
    "Layout.preferredWidth",
];

/// What keeps text in its box.
const FITS: &[&str] = &[
    "elide",
    "wrapMode",
    "clip",
    "fontSizeMode",
    "maximumLineCount",
    "textFormat",
];

/// Spacing, margins and padding, by name.
fn is_spacing(name: &str) -> bool {
    let last = name.rsplit('.').next().unwrap_or(name);
    last == "spacing"
        || last.ends_with("Spacing")
        || last == "margins"
        || last.ends_with("Margin")
        || last == "padding"
        || last.ends_with("Padding")
}

pub(super) fn object<'a>(found: &mut Found<'_, '_, '_, 'a>, chain: &[Frame<'_, 'a>]) {
    anchors_in_layout(found, chain);
    let frame = chain.last().expect("the object itself");
    if found.is(frame, class::POPUP) || found.is(frame, class::PANE) {
        fixed_size(found, frame);
    }
    if found.is(frame, class::TEXT) {
        unbounded(found, frame);
    }
    if found.is(frame, class::MENU) {
        menu_ungrouped(found, frame);
    }
}

pub(super) fn file<'a>(found: &mut Found<'_, '_, '_, 'a>, root: &QmlObject<'a>) {
    let singleton = is_singleton(found.cx.source);
    spacing(found, root);
    if !singleton {
        font_literal(found, root);
    }
}

fn anchors_in_layout<'a>(found: &mut Found<'_, '_, '_, 'a>, chain: &[Frame<'_, 'a>]) {
    let [.., parent, frame] = chain else { return };
    // A Repeater's delegate is laid out by the layout the Repeater is in;
    // any other object a property holds is not the layout's child.
    let layout = if found.is(parent, class::REPEATER)
        && matches!(frame.via.as_deref(), None | Some("delegate"))
    {
        match chain.len().checked_sub(3) {
            Some(i) => &chain[i],
            None => return,
        }
    } else if frame.via.is_none() {
        parent
    } else {
        return;
    };
    if !found.is(layout, class::LAYOUT) || found.is(layout, class::STACK_LAYOUT) {
        return;
    }
    // A `Repeater` or a non-visual object sits in a layout without being laid
    // out, and so does an item that names another `parent`.
    if !found.is(frame, class::ITEM)
        || found.is(frame, class::REPEATER)
        || bound(frame.object, &["parent"]).is_some()
    {
        return;
    }
    if let Some((name, b)) = bindings(frame.object)
        .into_iter()
        .find(|(n, _)| ANCHORS.contains(&n.as_str()))
    {
        found.report(
            b.at,
            "layout-anchors-in-layout",
            format!(
                "`{name}` on `{}` directly in `{}`: the layout places it and overrides the \
                 anchor; use `Layout.*` (`Layout.fillWidth`, `Layout.alignment`), or wrap it in \
                 an `Item`",
                found.name(frame),
                found.name(layout)
            ),
        );
    }
}

/// Whether `object` holds content whose size is not known when it is written.
fn dynamic_content(found: &Found<'_, '_, '_, '_>, object: &QmlObject<'_>) -> bool {
    children(object).into_iter().any(|(child, via)| {
        if via
            .as_deref()
            .is_some_and(|v| v != "contentItem" && v != "contentData")
        {
            return false;
        }
        let frame = Frame {
            object: child,
            index: found.cx.tree.index_of(child),
            via: None,
        };
        let changes = found.is(&frame, class::VIEW)
            || found.is(&frame, class::REPEATER)
            || found.is(&frame, class::LOADER)
            || found.is(&frame, class::TEXT)
                && bound(child, &["text"]).is_some_and(|b| !is_literal(&b));
        changes || dynamic_content(found, child)
    })
}

/// A value written out, a `qsTr` of one included: what it says is known.
fn is_literal(b: &Bound<'_, '_>) -> bool {
    match b.expression().map(|e| e.without_parentheses()) {
        Some(Expression::StringLiteral(_) | Expression::NumericLiteral(_)) => true,
        Some(Expression::TemplateLiteral(t)) => t.expressions.is_empty(),
        Some(Expression::CallExpression(c)) => {
            matches!(&c.callee, Expression::Identifier(id) if id.name.starts_with("qsTr") || id.name == "qsTrId")
                && c.arguments.len() == 1
                && matches!(
                    c.arguments[0].as_expression(),
                    Some(Expression::StringLiteral(_))
                )
        }
        _ => false,
    }
}

fn fixed_size<'a>(found: &mut Found<'_, '_, '_, 'a>, frame: &Frame<'_, 'a>) {
    if found.is(frame, class::MENU)
        || found.is(frame, class::TOOL_TIP)
        || found.is(frame, class::DRAWER)
    {
        return;
    }
    let (Some(width), Some(height)) = (
        bound(frame.object, &["width"]),
        bound(frame.object, &["height"]),
    ) else {
        return;
    };
    let (Some(w), Some(h)) = (width.number(), height.number()) else {
        return;
    };
    if !dynamic_content(found, frame.object) {
        return;
    }
    found.report(
        width.at,
        "layout-fixed-size",
        format!(
            "`{}` is {w}×{h} whatever it holds, and what it holds changes: larger text or a \
             longer translation is cut off; bind its size to the content's implicit size, \
             within a maximum",
            found.name(frame)
        ),
    );
}

fn unbounded<'a>(found: &mut Found<'_, '_, '_, 'a>, frame: &Frame<'_, 'a>) {
    let Some(text) = bound(frame.object, &["text"]) else {
        return;
    };
    if is_literal(&text) {
        return;
    }
    let all = bindings(frame.object);
    let has = |name: &str| all.iter().any(|(n, _)| n == name);
    let fill_width = all
        .iter()
        .any(|(n, b)| n == "Layout.fillWidth" && b.boolean() == Some(true));
    let bounded = BOUNDED_WIDTH
        .iter()
        .filter(|n| **n != "Layout.fillWidth")
        .any(|n| has(n))
        || fill_width
        || has("anchors.left") && has("anchors.right");
    if !bounded || FITS.iter().any(|n| has(n)) {
        return;
    }
    found.report(
        text.at,
        "text-unbounded-dynamic",
        format!(
            "`{}` shows text from a binding in a box of bounded width, with neither `elide` \
             nor `wrapMode`: a long value runs out of the box; set `elide: Text.ElideRight` (and \
             a tooltip) or `wrapMode: Text.Wrap`",
            found.name(frame)
        ),
    );
}

fn menu_ungrouped<'a>(found: &mut Found<'_, '_, '_, 'a>, frame: &Frame<'_, 'a>) {
    let mut items = 0usize;
    for (child, via) in children(frame.object) {
        if via.as_deref().is_some_and(|v| v != "contentData") {
            continue;
        }
        let f = Frame {
            object: child,
            index: found.cx.tree.index_of(child),
            via: None,
        };
        if found.is(&f, class::MENU_SEPARATOR) || found.is(&f, class::MENU) {
            return;
        }
        // A Repeater or an Instantiator makes items the source cannot count.
        if !(found.is(&f, class::ACTION) || found.is(&f, class::ITEM))
            || found.is(&f, class::REPEATER)
        {
            return;
        }
        items += 1;
    }
    if items > limits::MENU_MAX_UNGROUPED {
        found.report(
            frame.object.type_name.span.start,
            "layout-menu-ungrouped",
            format!(
                "this menu holds {items} items and no separator or submenu (more than {}); \
                 group related items with `MenuSeparator` and move rare ones into a submenu",
                limits::MENU_MAX_UNGROUPED
            ),
        );
    }
}

fn spacing<'a>(found: &mut Found<'_, '_, '_, 'a>, object: &QmlObject<'a>) {
    let unit = f64::from(limits::SPACING_UNIT_PX);
    for (name, b) in bindings(object) {
        if !is_spacing(&name) {
            continue;
        }
        let Some(value) = b.number() else { continue };
        // Below the unit is a hairline adjustment, not a step of the scale.
        if value.abs() < unit || (value / unit).fract() == 0.0 {
            continue;
        }
        found.report(
            b.at,
            "layout-spacing-scale",
            format!(
                "`{name}: {value}` is not on the spacing scale; use a multiple of {unit} \
                 ({} or {})",
                (value / unit).floor() * unit,
                (value / unit).ceil() * unit
            ),
        );
    }
    for (child, _) in children(object) {
        spacing(found, child);
    }
}

fn font_literal<'a>(found: &mut Found<'_, '_, '_, 'a>, object: &QmlObject<'a>) {
    let frame = Frame {
        object,
        index: found.cx.tree.index_of(object),
        via: None,
    };
    if (found.is(&frame, class::TEXT)
        || found.is(&frame, class::TEXT_INPUT)
        || found.is(&frame, class::CONTROL))
        && let Some(b) = bound(object, &["font", "family"])
        && let Some(family) = b.string()
    {
        found.report(
            b.at,
            "text-font-literal",
            format!(
                "`font.family: \"{family}\"` stops this text following the application's \
                 font; leave it unset, or bind it to the theme's font"
            ),
        );
    }
    for (child, _) in children(object) {
        font_literal(found, child);
    }
}
