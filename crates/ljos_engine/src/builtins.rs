//! Types every engine has.

use crate::atom::Atom;

/// `QtObject` (module `QtQml`): the root of every type, and the native part
/// of QML types that do not build on a visual type.
#[derive(Debug, Default, crate::QmlObject)]
#[qml(module = "QtQml", name = "QtObject")]
pub struct QtObject {
    #[qml(notify)]
    pub object_name: Atom,
}
