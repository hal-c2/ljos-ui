//! Reading the runtime's type names the way the compiler needs.

use qml_syntax::js::{Expression, UnaryOperator};

/// Whether a type name, as QML or as the runtime writes it, is a colour.
pub(crate) fn is_color(type_name: &str) -> bool {
    matches!(type_name, "color" | "QColor")
}

/// What a property holds, as far as what may be written for it goes.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Held {
    Number,
    Int,
    Bool,
    String,
    Url,
    Color,
    Point,
    Size,
    Rect,
    Date,
    List,
    /// One object, of the class named.
    Object(String),
}

impl Held {
    /// What a value type, as QML or as the runtime names it, holds. None
    /// for what takes anything written (`var`, `font`, a vector) or what
    /// the file is not checked for.
    pub(crate) fn of_value(type_name: &str) -> Option<Held> {
        // Ljós's own table writes `QtQml.real`.
        let name = type_name.rsplit('.').next().unwrap_or(type_name);
        Some(match name {
            "double" | "real" | "float" | "qreal" => Held::Number,
            "int" => Held::Int,
            "bool" => Held::Bool,
            "QString" | "string" => Held::String,
            "QUrl" | "url" => Held::Url,
            "QColor" | "color" => Held::Color,
            "QPointF" | "QPoint" | "point" => Held::Point,
            "QSizeF" | "QSize" | "size" => Held::Size,
            "QRectF" | "QRect" | "rect" => Held::Rect,
            "QDateTime" | "date" => Held::Date,
            _ => return None,
        })
    }

    /// What qml6 says of `written` given to a property that holds this,
    /// where it does not load the file for it.
    pub(crate) fn refuses(&self, written: &Written) -> Option<String> {
        use Written::{Bool, Null, Number, Text};
        let expected = |kind: &str| Some(format!("Invalid property assignment: {kind} expected"));
        match (self, written) {
            (Held::Number, Text(_) | Bool | Null) => expected("number"),
            (Held::Int, Text(_) | Bool | Null) => expected("int"),
            (Held::Int, Number(n)) if !is_int(*n) => expected("int"),
            (Held::Bool, Text(_) | Number(_) | Null) => expected("boolean"),
            (Held::String, Number(_) | Bool | Null) => expected("string"),
            (Held::Url, Number(_) | Bool | Null) => expected("url"),
            // A string that is no colour is the colour check's.
            (Held::Color, Number(_) | Bool) => expected("color"),
            (Held::Point, Number(_) | Bool | Null) => expected("point"),
            (Held::Point, Text(text)) if point(text).is_none() => expected("point"),
            (Held::Size, Number(_) | Bool | Null) => expected("size"),
            (Held::Size, Text(text)) if size(text).is_none() => expected("size"),
            // qml6 says "point" of a rect too.
            (Held::Rect, Number(_) | Bool | Null) => expected("point"),
            (Held::Rect, Text(text)) if rect(text).is_none() => expected("point"),
            // Which strings are dates is Qt's to say, and it takes more
            // than one way of writing one: only what is no string is
            // refused here.
            (Held::Date, Number(_) | Bool | Null) => expected("datetime"),
            (Held::List, _) => Some("Cannot assign primitives to lists".into()),
            (Held::Object(class), Text(_) | Number(_) | Bool) => Some(format!(
                "Invalid property assignment: unsupported type \"{class}*\""
            )),
            _ => None,
        }
    }
}

/// A value written in the file as it is, not an expression to run.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Written {
    Number(f64),
    Text(String),
    Bool,
    Null,
}

impl Written {
    /// What `expression` is, where it is a value written as it is. A minus
    /// before a number is part of it; anything else, `(5)` or `+"3"`, is
    /// an expression, which qml6 leaves to the run.
    pub(crate) fn of(expression: &Expression) -> Option<Written> {
        Some(match expression {
            Expression::NumericLiteral(number) => Written::Number(number.value),
            Expression::UnaryExpression(unary)
                if unary.operator == UnaryOperator::UnaryNegation =>
            {
                let Expression::NumericLiteral(number) = &unary.argument else {
                    return None;
                };
                Written::Number(-number.value)
            }
            Expression::StringLiteral(string) => Written::Text(string.value.to_string()),
            Expression::TemplateLiteral(template) if template.expressions.is_empty() => {
                Written::Text(template.quasis.first()?.value.cooked.as_ref()?.to_string())
            }
            Expression::BooleanLiteral(_) => Written::Bool,
            Expression::NullLiteral(_) => Written::Null,
            _ => return None,
        })
    }
}

fn is_int(n: f64) -> bool {
    n.fract() == 0.0 && (f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&n)
}

fn number(text: &str) -> Option<f64> {
    text.trim().parse().ok()
}

/// `"x,y"`.
fn point(text: &str) -> Option<(f64, f64)> {
    let (x, y) = text.split_once(',')?;
    Some((number(x)?, number(y)?))
}

/// `"wxh"`.
fn size(text: &str) -> Option<(f64, f64)> {
    let (w, h) = text.split_once('x')?;
    Some((number(w)?, number(h)?))
}

/// `"x,y,wxh"`.
fn rect(text: &str) -> Option<()> {
    let (x, rest) = text.split_once(',')?;
    let (y, size_text) = rest.split_once(',')?;
    number(x)?;
    number(y)?;
    size(size_text)?;
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_of_points_sizes_and_rects_are_read_as_qml6_reads_them() {
        assert!(point("1,2").is_some());
        assert!(point(" 1 , 2 ").is_some());
        assert!(point("1.5,-2").is_some());
        assert!(point("1,2,").is_none());
        assert!(point("x").is_none());
        assert!(size("1x2").is_some());
        assert!(size("1 x 2").is_some());
        assert!(size("1x").is_none());
        assert!(size("1,2").is_none());
        assert!(rect("1,2,3x4.5").is_some());
        assert!(rect("1,2,3,4").is_none());
    }

    #[test]
    fn an_int_is_a_whole_number_that_fits_in_32_bits() {
        assert!(is_int(5.0));
        assert!(is_int(-2147483648.0));
        assert!(!is_int(2147483648.0));
        assert!(!is_int(2.5));
        assert!(!is_int(f64::NAN));
    }
}
