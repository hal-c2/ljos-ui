//! The `font` group: what Ljós calls its members, and which of them keep a
//! file. A font is bound on an object (`font.bold: true`,
//! `font { pixelSize: 14 }`) and read in scripts (`label.font.bold`); both
//! ask here, so the two cannot disagree.

use std::ops::Range;

use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use qml_syntax::ast::QmlBindingValue;

use crate::rules::Rule;
use crate::{Font, Rename, Ty, lookup_rename};

/// The members of a font Ljós calls by another name, wherever the font is.
/// `bold` is a `weight` ([`written`]) and `pointSize` a run-time
/// conversion, so neither is a rename.
pub const FONT_RENAMES: &[(&str, &str)] = &[("pixelSize", "size")];

/// What converts a font's points to its pixels where the file runs, which
/// the lowered file names: `Screen.points(12)`.
pub(crate) const SCREEN: &str = "Screen";

/// The rule of Qt's `font` value type.
pub(crate) struct FontRule;

pub(crate) const FONT: FontRule = FontRule;

impl Rule for FontRule {
    fn owns(&self) -> &'static [&'static str] {
        &["font"]
    }

    fn member(&self, name: &str) -> Option<Rename> {
        Some(match name {
            // A script's value is not followed, so it cannot be made the
            // weight it means.
            "bold" => Rename::No(Font::BoldRead.into()),
            // A size in points is no value of a font in Ljós.
            "pointSize" => Rename::No(Font::PointSize.into()),
            _ => lookup_rename(FONT_RENAMES, name).map_or(Rename::Keep, Rename::To),
        })
    }
}

/// A member of a font the pass lowers, or keeps a file for.
pub(crate) fn is_member(name: &str) -> bool {
    matches!(name, "bold" | "pointSize") || FONT_RENAMES.iter().any(|(old, _)| *old == name)
}

/// What a binding of a font's member `part`, given `value`, is written as.
/// `beside` says whether the same font has another member bound.
pub(crate) fn written(
    part: &str,
    value: &QmlBindingValue<'_>,
    beside: impl Fn(&str) -> bool,
) -> Result<Written, Font> {
    match part {
        // A point is the screen's logical DPI / 72 of a pixel, known only
        // when the file runs (docs/DIALECT.md §4.13): the file converts it.
        // Which of the two sizes wins in Qt depends on the order they are
        // set in.
        "pointSize" if beside("pixelSize") => Err(Font::PointAndPixelSize),
        "pointSize" => {
            let QmlBindingValue::Expression(expression) = value else {
                return Err(Font::PointSizeBlock);
            };
            let span = expression.span();
            Ok(Written {
                name: Some("size".to_owned()),
                value: Value::Around {
                    range: span.start as usize..span.end as usize,
                    before: "Screen.points(",
                    after: ")",
                },
            })
        }
        "pixelSize" if beside("pointSize") => Err(Font::PointAndPixelSize),
        // Which one wins in Qt depends on the order they are set in.
        "bold" if beside("weight") => Err(Font::BoldAndWeight),
        "bold" => Ok(Written {
            name: Some("weight".to_owned()),
            value: weight(value)?,
        }),
        _ => Ok(Written {
            name: lookup_rename(FONT_RENAMES, part),
            value: Value::AsWritten,
        }),
    }
}

/// A font member's binding as Ljós writes it.
pub(crate) struct Written {
    /// The member's Ljós name, where it is not Qt's.
    pub(crate) name: Option<String>,
    pub(crate) value: Value,
}

/// What becomes of a font member's value.
pub(crate) enum Value {
    AsWritten,
    /// The expression at `range` is replaced by `with`.
    Replaced {
        range: Range<usize>,
        with: &'static str,
    },
    /// The expression at `range` is put between `before` and `after`.
    Around {
        range: Range<usize>,
        before: &'static str,
        after: &'static str,
    },
}

/// What `font.bold` is given, as the weight it means: `Font.Bold` (700) for
/// true, `Font.Normal` (400) for false, as in Qt.
fn weight(value: &QmlBindingValue<'_>) -> Result<Value, Font> {
    let QmlBindingValue::Expression(expression) = value else {
        return Err(Font::BoldBlock);
    };
    let span = expression.span();
    let range = span.start as usize..span.end as usize;
    Ok(match expression {
        Expression::BooleanLiteral(literal) => Value::Replaced {
            range,
            with: if literal.value {
                "Font.Bold"
            } else {
                "Font.Normal"
            },
        },
        _ => Value::Around {
            range,
            before: "(",
            after: ") ? Font.Bold : Font.Normal",
        },
    })
}

/// What a script's `object.name` is, `on` being what the pass knows of
/// `object`, when the font rule says; `could_be_font` asks whether an
/// `object` the pass does not know may be a font.
pub(crate) fn read(name: &str, on: Ty, could_be_font: impl FnOnce() -> bool) -> Option<Rename> {
    match on {
        Ty::Font => FONT.member(name),
        // A row's role or a script's object may be called so too. Where an
        // area renames the name as well, `Pass::member` answers `Unknown`
        // off an unknown object too, so this adds only what it leaves.
        Ty::Unknown if is_member(name) && could_be_font() => Some(Rename::Unknown),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_off_font(name: &str) -> Rename {
        read(name, Ty::Font, || unreachable!("a font is known")).expect("a font's member")
    }

    #[test]
    fn a_read_of_a_font_member_is_renamed_or_keeps_the_file() {
        assert!(matches!(read_off_font("pixelSize"), Rename::To(new) if new == "size"));
        assert!(
            matches!(read_off_font("pixelSizeChanged"), Rename::To(new) if new == "sizeChanged")
        );
        assert!(matches!(read_off_font("family"), Rename::Keep));
        assert!(matches!(read_off_font("bold"), Rename::No(c) if c == Font::BoldRead.into()));
        assert!(matches!(read_off_font("pointSize"), Rename::No(c) if c == Font::PointSize.into()));
    }

    #[test]
    fn a_read_off_an_unknown_object_keeps_the_file_only_if_it_may_be_a_font() {
        assert!(matches!(
            read("bold", Ty::Unknown, || true),
            Some(Rename::Unknown)
        ));
        assert!(read("bold", Ty::Unknown, || false).is_none());
        assert!(read("family", Ty::Unknown, || unreachable!()).is_none());
        assert!(read("bold", Ty::Plain, || unreachable!()).is_none());
    }

    #[test]
    fn every_renamed_font_member_is_one_the_pass_lowers() {
        for (old, _) in FONT_RENAMES {
            assert!(is_member(old), "`{old}`");
        }
        assert!(!is_member("pixelSizeChanged") && !is_member("family"));
    }
}
