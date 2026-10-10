//! QML document tree.
//!
//! Only the QML structure gets its own nodes. Every binding value, handler and
//! function is an ordinary Oxc JavaScript node allocated in the same arena, with
//! spans that are offsets into the QML file.

use oxc_allocator::{ArenaBox, ArenaVec};
use oxc_ast::ast::{Expression, Function, IdentifierName, Statement};
use oxc_span::Span;

/// A `.qml` file: pragmas, imports and one root object.
#[derive(Debug)]
pub struct QmlDocument<'a> {
    pub span: Span,
    pub pragmas: ArenaVec<'a, QmlPragma<'a>>,
    pub imports: ArenaVec<'a, QmlImport<'a>>,
    pub root: QmlObject<'a>,
}

/// `pragma Singleton` or `pragma ComponentBehavior: Bound`.
#[derive(Debug)]
pub struct QmlPragma<'a> {
    pub span: Span,
    pub name: &'a str,
    pub value: Option<&'a str>,
}

/// `import QtQuick.Controls 2.15 as C` or `import "lib.js" as Lib`.
#[derive(Debug)]
pub struct QmlImport<'a> {
    pub span: Span,
    pub source: QmlImportSource<'a>,
    pub version: Option<&'a str>,
    pub alias: Option<IdentifierName<'a>>,
}

#[derive(Debug)]
pub enum QmlImportSource<'a> {
    /// A module URI such as `QtQuick.Controls`.
    Module(QmlQualifiedName<'a>),
    /// A directory or JavaScript file, given as a string.
    Path(&'a str),
}

/// A dotted name: a type (`T.Button`), a property path (`font.bold`) or an
/// attached property (`Layout.fillWidth`).
#[derive(Debug)]
pub struct QmlQualifiedName<'a> {
    pub span: Span,
    pub parts: ArenaVec<'a, &'a str>,
}

impl<'a> QmlQualifiedName<'a> {
    /// The single segment of an undotted name.
    pub fn as_simple(&self) -> Option<&'a str> {
        if self.parts.len() == 1 { Some(self.parts[0]) } else { None }
    }

    pub fn last(&self) -> &'a str {
        self.parts[self.parts.len() - 1]
    }
}

impl std::fmt::Display for QmlQualifiedName<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, part) in self.parts.iter().enumerate() {
            if index > 0 {
                f.write_str(".")?;
            }
            f.write_str(part)?;
        }
        Ok(())
    }
}

/// `Type { members }`, a grouped property (`font { bold: true }`) or a
/// property value source (`NumberAnimation on x { }`).
#[derive(Debug)]
pub struct QmlObject<'a> {
    pub span: Span,
    pub type_name: QmlQualifiedName<'a>,
    /// The target of `Type on target { }`.
    pub on: Option<QmlQualifiedName<'a>>,
    pub members: ArenaVec<'a, QmlMember<'a>>,
}

#[derive(Debug)]
pub enum QmlMember<'a> {
    Object(QmlObject<'a>),
    Binding(QmlBinding<'a>),
    Property(QmlPropertyDeclaration<'a>),
    Signal(QmlSignalDeclaration<'a>),
    Function(ArenaBox<'a, Function<'a>>),
    InlineComponent(QmlInlineComponent<'a>),
    Enum(QmlEnumDeclaration<'a>),
}

/// `name: value`, including `id: name` and `onSignal: handler`.
#[derive(Debug)]
pub struct QmlBinding<'a> {
    pub span: Span,
    pub name: QmlQualifiedName<'a>,
    pub value: QmlBindingValue<'a>,
}

#[derive(Debug)]
pub enum QmlBindingValue<'a> {
    Expression(Expression<'a>),
    /// A block, `if`, `switch`, `try` or `with` statement. `return` is allowed.
    Statement(Statement<'a>),
    Object(QmlObject<'a>),
    Objects(ArenaVec<'a, QmlObject<'a>>),
}

/// `[default] [required] [readonly] property Type name[: value]`, or the
/// `required name` form that marks an inherited property.
#[derive(Debug)]
pub struct QmlPropertyDeclaration<'a> {
    pub span: Span,
    pub is_default: bool,
    pub is_required: bool,
    pub is_readonly: bool,
    /// `None` for `required name`.
    pub type_name: Option<QmlTypeName<'a>>,
    pub name: IdentifierName<'a>,
    pub value: Option<QmlBindingValue<'a>>,
}

/// `int`, `var`, `alias`, `T.Button` or `list<Item>`.
#[derive(Debug)]
pub struct QmlTypeName<'a> {
    pub span: Span,
    pub name: QmlQualifiedName<'a>,
    pub is_list: bool,
}

/// `signal picked(int index, name: string)`.
#[derive(Debug)]
pub struct QmlSignalDeclaration<'a> {
    pub span: Span,
    pub name: IdentifierName<'a>,
    pub params: ArenaVec<'a, QmlSignalParameter<'a>>,
}

#[derive(Debug)]
pub struct QmlSignalParameter<'a> {
    pub name: IdentifierName<'a>,
    pub type_name: Option<QmlTypeName<'a>>,
}

/// `component Name: Type { }`.
#[derive(Debug)]
pub struct QmlInlineComponent<'a> {
    pub span: Span,
    pub name: IdentifierName<'a>,
    pub object: QmlObject<'a>,
}

/// `enum Name { First, Second = 4 }`.
#[derive(Debug)]
pub struct QmlEnumDeclaration<'a> {
    pub span: Span,
    pub name: IdentifierName<'a>,
    pub members: ArenaVec<'a, QmlEnumMember<'a>>,
}

#[derive(Debug)]
pub struct QmlEnumMember<'a> {
    pub name: IdentifierName<'a>,
    /// What was written, or one more than the member before.
    pub value: i64,
}
