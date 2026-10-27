//! Colour: literals, schemes and the contrast of text the source can
//! measure (`docs/design/color.md`). The maths is `ljos_design::color`'s.

use ljos_design::{
    color::{Srgb, Srgba, is_large_text, wcag_contrast},
    limits,
};
use qml_syntax::{ast::QmlObject, js::Expression};

use super::{Bound, Found, Frame, bindings, bound, children, class, is_singleton};

/// Whether a property holds a colour, by its name.
fn is_color(name: &str) -> bool {
    let last = crate::front::last_part(name);
    last == "color" || last.ends_with("Color")
}

/// A colour written out as a string.
fn literal(b: &Bound<'_, '_>) -> Option<Srgba> {
    Srgba::parse(b.string()?).ok()
}

/// An opaque colour written out.
fn opaque(b: &Bound<'_, '_>) -> Option<Srgb> {
    literal(b).filter(|c| c.a >= 1.0).map(Srgba::rgb)
}

pub(super) fn object<'a>(found: &mut Found<'_, '_, '_, 'a>, chain: &[Frame<'_, 'a>]) {
    let [.., parent, frame] = chain else { return };
    if frame.via.is_none() && found.is(frame, class::TEXT) && found.is(parent, class::RECTANGLE) {
        text_contrast(found, parent, frame);
    }
}

pub(super) fn file<'a>(found: &mut Found<'_, '_, '_, 'a>, root: &QmlObject<'a>) {
    // A theme's literals are the point of it.
    if is_singleton(found.cx.source) {
        return;
    }
    literals(found, root);
    scheme_ignored(found, root);
}

/// Whether the colour of `object` changes with a state, which the source
/// cannot follow.
fn has_states(object: &QmlObject<'_>) -> bool {
    bound(object, &["states"]).is_some()
}

fn text_contrast<'a>(
    found: &mut Found<'_, '_, '_, 'a>,
    surface: &Frame<'_, 'a>,
    text: &Frame<'_, 'a>,
) {
    let Some(written) = bound(text.object, &["color"]) else {
        return;
    };
    let (Some(fg), Some(bg)) = (
        opaque(&written),
        bound(surface.object, &["color"]).and_then(|b| opaque(&b)),
    ) else {
        return;
    };
    if bound(surface.object, &["gradient"]).is_some()
        || has_states(surface.object)
        || has_states(text.object)
    {
        return;
    }
    let px = bound(text.object, &["font", "pixelSize"])
        .and_then(|b| b.number())
        .or_else(|| {
            // A point is 4/3 of a pixel at the 96 dpi CSS and Qt take.
            bound(text.object, &["font", "pointSize"])
                .and_then(|b| b.number())
                .map(|pt| pt * 4.0 / 3.0)
        })
        .unwrap_or(0.0) as f32;
    let bold = bound(text.object, &["font", "bold"]).and_then(|b| b.boolean()) == Some(true)
        || bound(text.object, &["font", "weight"])
            .and_then(|b| b.enumeration())
            .is_some_and(|(_, w)| matches!(w, "Bold" | "ExtraBold" | "Black"));
    let weight = if bold { limits::BOLD_WEIGHT } else { 400 };
    let large = is_large_text(px, weight);
    let needed = if large {
        limits::LARGE_TEXT_CONTRAST
    } else {
        limits::TEXT_CONTRAST
    };
    let ratio = wcag_contrast(fg, bg);
    if ratio < needed {
        found.report(
            written.at,
            "color-text-contrast",
            format!(
                "text {} on {} is {ratio:.2}:1, below the {needed}:1 {} text needs; take text \
                 and surface from a pair made for each other (`palette.text` on `palette.base`)",
                fg.to_hex(),
                bg.to_hex(),
                if large { "large" } else { "body" },
            ),
        );
    }
}

fn literals<'a>(found: &mut Found<'_, '_, '_, 'a>, object: &QmlObject<'a>) {
    for (name, b) in bindings(object) {
        if !is_color(&name) {
            continue;
        }
        let Some(c) = literal(&b) else { continue };
        if c.a == 0.0 {
            continue;
        }
        found.report(
            b.at,
            "color-literal",
            format!(
                "`{name}: \"{}\"` is a colour written out, which dark mode and high contrast \
                 cannot change; read `palette.*`, the style's colours or a theme singleton",
                b.string().unwrap_or_default()
            ),
        );
    }
    for (child, _) in children(object) {
        literals(found, child);
    }
}

/// Whether a colour expression follows the scheme: reads the palette,
/// the scheme itself, a style's colours or a theme singleton (`Theme.x`).
fn follows_scheme(text: &str) -> bool {
    let mut rest = text;
    while let Some(i) = rest.find(|c: char| c.is_alphabetic() || c == '_') {
        rest = &rest[i..];
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        let word = &rest[..end];
        rest = &rest[end..];
        if matches!(
            word,
            "palette" | "colorScheme" | "styleHints" | "Material" | "Universal"
        ) {
            return true;
        }
        let member = rest.starts_with('.');
        if member && word != "Qt" && word.starts_with(|c: char| c.is_uppercase()) {
            return true;
        }
    }
    false
}

/// The opaque colours written out in any branch of `e`.
fn written(e: &Expression<'_>, out: &mut Vec<String>) {
    match e.without_parentheses() {
        Expression::StringLiteral(s) => {
            if Srgba::parse(s.value.as_str()).is_ok_and(|c| c.a >= 1.0) {
                out.push(s.value.to_string());
            }
        }
        Expression::ConditionalExpression(c) => {
            written(&c.consequent, out);
            written(&c.alternate, out);
        }
        Expression::LogicalExpression(l) => {
            written(&l.left, out);
            written(&l.right, out);
        }
        _ => {}
    }
}

/// A surface filled with a colour written out, in every branch the
/// condition picks from, when nothing in that binding follows the scheme:
/// the other scheme gets the same fill under its own text.
fn scheme_ignored<'a>(found: &mut Found<'_, '_, '_, 'a>, object: &QmlObject<'a>) {
    let frame = Frame {
        object,
        index: found.cx.tree.index_of(object),
        via: None,
    };
    if found.is(&frame, class::RECTANGLE)
        && let Some(b) = bound(object, &["color"])
        && let (Some(e), Some(span)) = (b.expression(), b.span())
    {
        let mut fills = Vec::new();
        written(e, &mut fills);
        if !fills.is_empty() && !follows_scheme(found.text(span)) {
            found.report(
                b.at,
                "color-scheme-ignored",
                format!(
                    "this surface is filled with {} whatever the colour scheme, so the text \
                     the scheme colours on it is unreadable in one of them; bind it to \
                     `palette.*` (or a theme's colour that follows \
                     `Application.styleHints.colorScheme`)",
                    fills
                        .iter()
                        .map(|f| format!("`{f}`"))
                        .collect::<Vec<_>>()
                        .join(" or ")
                ),
            );
        }
    }
    for (child, _) in children(object) {
        scheme_ignored(found, child);
    }
}
