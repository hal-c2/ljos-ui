//! Reading the runtime's type names the way the compiler needs.

/// Whether a type name, as QML or as the runtime writes it, is a colour.
pub(crate) fn is_color(type_name: &str) -> bool {
    matches!(type_name, "color" | "QColor")
}
