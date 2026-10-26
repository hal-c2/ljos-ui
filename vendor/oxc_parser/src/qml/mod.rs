//! QML mode.
//!
//! QML is a small declarative shell (about twenty productions) around
//! JavaScript. This module parses the shell with the same lexer and cursor as
//! the JavaScript parser and calls straight into it for every binding
//! expression, script block and function, so embedded JavaScript ends exactly
//! where the JavaScript grammar says it ends and all spans are file offsets.

pub mod ast;
mod recovery;

use oxc_allocator::ArenaVec;
use oxc_ast::ast::{Comment, Expression, IdentifierName, ObjectExpression, ObjectPropertyKind};
use oxc_diagnostics::Diagnostics;
use oxc_span::Span;

use crate::{
    ParserImpl,
    config::ParserConfig,
    context::{Context, StatementContext},
    diagnostics::{self, ParserDiagnostic},
    js::FunctionKind,
    lexer::Kind,
};

use ast::*;
use recovery::Sync;

/// Return value of [`Parser::parse_qml`](crate::Parser::parse_qml).
pub struct QmlParserReturn<'a> {
    /// Every import, pragma and member that parsed; units that did not are
    /// left out and have a diagnostic (see `recovery.rs`). Only when
    /// [`panicked`](Self::panicked) is set is the root object missing, and then
    /// it is empty with a type name of `""`.
    pub document: QmlDocument<'a>,
    pub comments: ArenaVec<'a, Comment>,
    pub diagnostics: Diagnostics,
    /// The parser hit a syntax error it could not recover from (the root
    /// object has no usable type name) and stopped early.
    pub panicked: bool,
}

impl<'a, C: ParserConfig> ParserImpl<'a, C> {
    pub(crate) fn parse_qml(mut self) -> QmlParserReturn<'a> {
        let document = self.parse_qml_document();

        let mut panicked = false;
        if let Some(fatal_error) = self.fatal_error.take() {
            panicked = true;
            self.errors.truncate(fatal_error.errors_len);
            self.errors.push(fatal_error.error);
        }
        let mut diagnostics = self
            .lexer
            .errors
            .drain(..)
            .chain(self.errors.drain(..))
            .map(ParserDiagnostic::into_diagnostic)
            .collect::<Vec<_>>();
        // Recovery re-lexes the text it skips, so a lexer error can appear
        // twice (once as the member's fatal error); and the two lists are in
        // no particular order. Report each once, in source order.
        let at = |d: &oxc_diagnostics::OxcDiagnostic| d.labels.first().map_or(0, |l| l.offset());
        diagnostics.sort_by_key(at);
        diagnostics.dedup_by(|a, b| at(a) == at(b) && a.message == b.message);
        let diagnostics = diagnostics.into_iter().collect::<Diagnostics>();
        let comments = self.lexer.trivia_builder.comments;

        QmlParserReturn { document, comments, diagnostics, panicked }
    }

    fn parse_qml_document(&mut self) -> QmlDocument<'a> {
        self.lexer.qml = true;
        self.token = self.lexer.first_token();
        let start = self.cur_start();

        let mut pragmas = ArenaVec::new_in(&self.ast);
        let mut imports = ArenaVec::new_in(&self.ast);
        while self.fatal_error.is_none() {
            if self.at(Kind::Import) {
                imports.extend(self.qml_recover(Sync::Line, |p| p.parse_qml_import()));
            } else if self.qml_at_word("pragma") && self.qml_next_is_name() {
                pragmas.extend(self.qml_recover(Sync::Line, |p| p.parse_qml_pragma()));
            } else {
                break;
            }
        }

        let root_start = self.cur_start();
        let root_type = self.parse_qml_qualified_name();
        let mut root = self.parse_qml_object_rest(root_start, root_type, true);
        if self.fatal_error.is_some() {
            root.span = Span::empty(root_start);
        }
        if !self.at(Kind::Eof) {
            let span = self.cur_token().span();
            self.qml_error_here("A QML document has exactly one root object", span);
        }
        // With nothing parsed there is no token before `start`; keep the span
        // well-formed rather than ending before it begins.
        let span = Span::new(start, self.prev_token_end.max(start));
        QmlDocument { span, pragmas, imports, root }
    }

    fn parse_qml_pragma(&mut self) -> QmlPragma<'a> {
        let start = self.cur_start();
        self.bump_any();
        let name = self.parse_identifier_name().name.as_str();
        let value = if self.eat(Kind::Colon) {
            Some(self.parse_identifier_name().name.as_str())
        } else {
            None
        };
        self.bump(Kind::Semicolon);
        QmlPragma { span: self.end_span(start), name, value }
    }

    fn parse_qml_import(&mut self) -> QmlImport<'a> {
        let start = self.cur_start();
        self.bump_any();
        let source = if self.at(Kind::Str) {
            QmlImportSource::Path(self.parse_literal_string().value.as_str())
        } else {
            QmlImportSource::Module(self.parse_qml_import_name())
        };
        // A version is only a version when it is on the import's own line;
        // otherwise a number would be the start of something else entirely.
        let version = if self.cur_kind().is_number() && !self.cur_token().is_on_new_line() {
            let text = self.cur_src();
            self.bump_any();
            Some(text)
        } else {
            None
        };
        let alias = if self.eat(Kind::As) {
            self.qml_expect_same_line("An import alias is on the import's own line");
            Some(self.parse_identifier_name())
        } else {
            None
        };
        self.bump(Kind::Semicolon);
        QmlImport { span: self.end_span(start), source, version, alias }
    }

    /// A module URI. Unlike other names it must not run onto the next line:
    /// `import QtQuick.` followed by `Item {` would otherwise swallow the root
    /// object's type, and recovery would have no root to resume at.
    fn parse_qml_import_name(&mut self) -> QmlQualifiedName<'a> {
        let start = self.cur_start();
        let mut parts = ArenaVec::new_in(&self.ast);
        parts.push(self.parse_qml_name().name.as_str());
        while self.fatal_error.is_none() && self.eat(Kind::Dot) {
            self.qml_expect_same_line("A module name continues on the import's own line");
            parts.push(self.parse_qml_name().name.as_str());
        }
        QmlQualifiedName { span: self.end_span(start), parts }
    }

    fn qml_expect_same_line(&mut self, message: &'static str) {
        if self.cur_token().is_on_new_line() {
            let span = self.cur_token().span();
            self.set_fatal_error(diagnostics::qml(message, span));
        }
    }

    fn parse_qml_qualified_name(&mut self) -> QmlQualifiedName<'a> {
        let start = self.cur_start();
        let mut parts = ArenaVec::new_in(&self.ast);
        parts.push(self.parse_qml_name().name.as_str());
        while self.fatal_error.is_none() && self.eat(Kind::Dot) {
            parts.push(self.parse_qml_name().name.as_str());
        }
        QmlQualifiedName { span: self.end_span(start), parts }
    }

    /// QML's keywords are contextual: `property`, `signal`, `on`, `default` and
    /// the JavaScript keywords are all usable as names.
    fn parse_qml_name(&mut self) -> IdentifierName<'a> {
        self.parse_identifier_name()
    }

    fn parse_qml_object(&mut self) -> QmlObject<'a> {
        let start = self.cur_start();
        let type_name = self.parse_qml_qualified_name();
        self.parse_qml_object_rest(start, type_name, false)
    }

    fn parse_qml_object_rest(
        &mut self,
        start: u32,
        type_name: QmlQualifiedName<'a>,
        is_root: bool,
    ) -> QmlObject<'a> {
        let on = if self.qml_at_word("on") {
            self.bump_any();
            Some(self.parse_qml_qualified_name())
        } else {
            None
        };
        let opening_span = self.cur_token().span();
        if is_root && !self.at(Kind::LCurly) {
            // The members of a file whose root lost its `{` are still worth
            // having. A nested object cannot do this: its siblings would be
            // taken for its members.
            self.qml_error_here("Expected `{` after the root object's type", opening_span);
        } else {
            self.expect(Kind::LCurly);
        }
        let mut members = ArenaVec::new_in(&self.ast);
        // Not `has_fatal_error`: that is also true at an `Undetermined` token
        // (an unterminated string), which a member's recovery should skip.
        while !self.at(Kind::RCurly) && !self.at(Kind::Eof) && self.fatal_error.is_none() {
            members.extend(self.qml_recover(Sync::Member, |p| p.parse_qml_member()).flatten());
        }
        if self.at(Kind::RCurly) {
            self.bump_any();
        } else if self.fatal_error.is_none() {
            // Unclosed at the end of the file. The object keeps its members.
            let found = self.cur_kind().to_str();
            let error = diagnostics::expect_closing(
                Kind::RCurly.to_str(),
                found,
                self.cur_token().span(),
                opening_span,
            );
            self.error(error);
        }
        QmlObject { span: self.end_span(start), type_name, on, members }
    }

    fn parse_qml_member(&mut self) -> Option<QmlMember<'a>> {
        let start = self.cur_start();
        match self.cur_kind() {
            Kind::Semicolon => {
                // Qt's grammar has no empty member: `Item {};`, `f() {};`
                // and `x: 1;;` do not load (checked on qml6).
                let span = self.cur_token().span();
                self.error(diagnostics::unexpected_token(span));
                self.bump_any();
                return None;
            }
            Kind::Function => {
                let function =
                    self.parse_function_impl(start, /* async */ false, FunctionKind::Declaration);
                return Some(QmlMember::Function(function));
            }
            Kind::Enum if self.qml_next_is_name() => {
                return Some(QmlMember::Enum(self.parse_qml_enum()));
            }
            Kind::At => {
                let span = self.cur_token().span();
                self.set_fatal_error(diagnostics::qml("QML annotations are not supported yet", span));
                return None;
            }
            _ => {}
        }

        // A declaration keyword only starts a declaration when a name follows:
        // `property: 1` and `signal.x: 1` are ordinary bindings.
        if self.qml_next_is_name() {
            if self.qml_at_word("signal") {
                return Some(QmlMember::Signal(self.parse_qml_signal()));
            }
            if self.qml_at_word("component") {
                return Some(QmlMember::InlineComponent(self.parse_qml_inline_component()));
            }
            if self.qml_at_word("property")
                || self.qml_at_word("required")
                || matches!(self.cur_kind(), Kind::Default | Kind::Readonly)
            {
                return Some(QmlMember::Property(self.parse_qml_property()));
            }
        }

        let name = self.parse_qml_qualified_name();
        if self.eat(Kind::Colon) {
            let value = self.parse_qml_binding_value(true);
            return Some(QmlMember::Binding(QmlBinding { span: self.end_span(start), name, value }));
        }
        Some(QmlMember::Object(self.parse_qml_object_rest(start, name, false)))
    }

    fn parse_qml_property(&mut self) -> QmlPropertyDeclaration<'a> {
        let start = self.cur_start();
        let (mut is_default, mut is_required, mut is_readonly) = (false, false, false);
        loop {
            if !self.qml_next_is_name() {
                break;
            }
            if self.at(Kind::Default) {
                is_default = true;
            } else if self.at(Kind::Readonly) {
                is_readonly = true;
            } else if self.qml_at_word("required") {
                is_required = true;
            } else {
                break;
            }
            self.bump_any();
        }

        let type_name = if self.qml_at_word("property") && self.qml_next_is_name() {
            self.bump_any();
            Some(self.parse_qml_type_name())
        } else {
            None
        };
        let name = self.parse_qml_name();
        // A declared property takes a list of objects only when its type is
        // a list: after `property var p:` the `[` starts an array, and
        // `[Item {}]` is an error at the `{`, as in Qt.
        let objects = type_name.as_ref().is_none_or(|type_name| type_name.is_list);
        let value =
            if self.eat(Kind::Colon) { Some(self.parse_qml_binding_value(objects)) } else { None };
        self.bump(Kind::Semicolon);
        QmlPropertyDeclaration {
            span: self.end_span(start),
            is_default,
            is_required,
            is_readonly,
            type_name,
            name,
            value,
        }
    }

    fn parse_qml_type_name(&mut self) -> QmlTypeName<'a> {
        let start = self.cur_start();
        let name = self.parse_qml_qualified_name();
        if name.as_simple() == Some("list") && self.eat(Kind::LAngle) {
            let element = self.parse_qml_qualified_name();
            self.expect(Kind::RAngle);
            return QmlTypeName { span: self.end_span(start), name: element, is_list: true };
        }
        QmlTypeName { span: self.end_span(start), name, is_list: false }
    }

    fn parse_qml_signal(&mut self) -> QmlSignalDeclaration<'a> {
        let start = self.cur_start();
        self.bump_any();
        let name = self.parse_qml_name();
        let mut params = ArenaVec::new_in(&self.ast);
        if self.eat(Kind::LParen) {
            while !self.at(Kind::RParen) && !self.has_fatal_error() {
                params.push(self.parse_qml_signal_parameter());
                if !self.eat(Kind::Comma) {
                    break;
                }
            }
            self.expect(Kind::RParen);
        }
        self.bump(Kind::Semicolon);
        QmlSignalDeclaration { span: self.end_span(start), name, params }
    }

    /// `name: type` (current style), `type name` (legacy style) or a bare name.
    fn parse_qml_signal_parameter(&mut self) -> QmlSignalParameter<'a> {
        let typed_after = self.lookahead(|p| {
            p.bump_any();
            p.at(Kind::Colon)
        });
        if typed_after {
            let name = self.parse_qml_name();
            self.bump_any();
            return QmlSignalParameter { name, type_name: Some(self.parse_qml_type_name()) };
        }
        let bare = self.lookahead(|p| {
            p.bump_any();
            matches!(p.cur_kind(), Kind::Comma | Kind::RParen)
        });
        if bare {
            return QmlSignalParameter { name: self.parse_qml_name(), type_name: None };
        }
        let type_name = self.parse_qml_type_name();
        QmlSignalParameter { name: self.parse_qml_name(), type_name: Some(type_name) }
    }

    fn parse_qml_enum(&mut self) -> QmlEnumDeclaration<'a> {
        let start = self.cur_start();
        self.bump_any();
        let name = self.parse_qml_name();
        let opening_span = self.cur_token().span();
        self.expect(Kind::LCurly);
        let mut members = ArenaVec::new_in(&self.ast);
        let mut next = 0;
        while !self.at(Kind::RCurly) && !self.has_fatal_error() {
            let name = self.parse_qml_name();
            let value = if self.eat(Kind::Eq) {
                let negative = self.eat(Kind::Minus);
                let span = self.cur_token().span();
                let text = self.cur_src();
                let written = match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
                    Some(digits) => i64::from_str_radix(digits, 16).ok(),
                    None => text.parse().ok(),
                };
                let Some(written) = written.filter(|_| self.cur_kind().is_number()) else {
                    self.set_fatal_error(diagnostics::qml("An enum's value is a whole number", span));
                    break;
                };
                self.bump_any();
                if negative { -written } else { written }
            } else {
                next
            };
            next = value + 1;
            members.push(QmlEnumMember { name, value });
            if !self.eat(Kind::Comma) {
                break;
            }
        }
        self.expect_closing(Kind::RCurly, opening_span);
        QmlEnumDeclaration { span: self.end_span(start), name, members }
    }

    fn parse_qml_inline_component(&mut self) -> QmlInlineComponent<'a> {
        let start = self.cur_start();
        self.bump_any();
        let name = self.parse_qml_name();
        self.expect(Kind::Colon);
        let object = self.parse_qml_object();
        QmlInlineComponent { span: self.end_span(start), name, object }
    }

    /// `objects`: whether `[` may start a list of objects.
    fn parse_qml_binding_value(&mut self, objects: bool) -> QmlBindingValue<'a> {
        match self.cur_kind() {
            Kind::LCurly => self.parse_qml_braced_value(),
            Kind::If | Kind::Switch | Kind::Try | Kind::With => {
                QmlBindingValue::Statement(self.context_add(Context::Return, |p| {
                    p.parse_statement_list_item(StatementContext::StatementList)
                }))
            }
            Kind::LBrack if objects && self.qml_at_object_list() => {
                self.bump_any();
                let mut objects = ArenaVec::new_in(&self.ast);
                while !self.at(Kind::RBrack) && !self.has_fatal_error() {
                    objects.push(self.parse_qml_object());
                    if !self.eat(Kind::Comma) {
                        break;
                    }
                }
                // No `;` after it: a property declaration takes one, a
                // binding does not (`delegate: Item {};` does not load).
                self.expect(Kind::RBrack);
                QmlBindingValue::Objects(objects)
            }
            _ if self.qml_at_object() => {
                let object = self.parse_qml_object();
                QmlBindingValue::Object(object)
            }
            _ => self.parse_qml_expression_value(),
        }
    }

    fn parse_qml_expression_value(&mut self) -> QmlBindingValue<'a> {
        let expression = self.parse_expr();
        self.asi();
        QmlBindingValue::Expression(expression)
    }

    /// `name: { ... }` is a script block, valued as the last statement it
    /// runs, unless it starts as JSON does: `{ "key":` or `{ 1:`. Then it is
    /// an object, and every key of it a string or a number. Qt tells the two
    /// apart by those first tokens alone (Qt 6.11.2, by running qml6):
    /// `{}` is an empty block, `{ k: 1 }` a block with a label, and
    /// `{ k: 1, j: 2 }` no script at all.
    fn parse_qml_braced_value(&mut self) -> QmlBindingValue<'a> {
        let keyed = self.lookahead(|p| {
            p.bump_any();
            let literal = p.at(Kind::Str) || p.cur_kind().is_number();
            p.bump_any();
            literal && p.at(Kind::Colon)
        });
        if !keyed {
            return QmlBindingValue::Statement(
                self.context_add(Context::Return, |p| p.parse_block_statement()),
            );
        }
        let start = self.cur_start();
        self.expect(Kind::LCurly);
        let mut properties = ArenaVec::new_in(&self.ast);
        while !self.at(Kind::RCurly) && !self.has_fatal_error() {
            if !self.at(Kind::Str) && !self.cur_kind().is_number() {
                self.expect(Kind::Decimal);
                break;
            }
            let property = self.context_add(Context::In, |p| p.parse_object_literal_element());
            properties.push(ObjectPropertyKind::ObjectProperty(property));
            if !self.eat(Kind::Comma) {
                break;
            }
        }
        self.expect(Kind::RCurly);
        self.asi();
        QmlBindingValue::Expression(Expression::ObjectExpression(ObjectExpression::boxed(
            self.end_span(start),
            properties,
            self,
        )))
    }

    /// At `Type {` or `Ns.Type {`, where the brace is on the same line.
    fn qml_at_object(&mut self) -> bool {
        if !self.cur_kind().is_identifier_name() {
            return false;
        }
        self.lookahead(|p| {
            p.bump_any();
            while p.eat(Kind::Dot) {
                if !p.cur_kind().is_identifier_name() {
                    return false;
                }
                p.bump_any();
            }
            p.at(Kind::LCurly) && !p.cur_token().is_on_new_line()
        })
    }

    /// At `[` followed by an object: a list of objects, not an array literal.
    fn qml_at_object_list(&mut self) -> bool {
        self.lookahead(|p| {
            p.bump_any();
            p.qml_at_object()
        })
    }

    fn qml_at_word(&self, word: &str) -> bool {
        self.at(Kind::Ident) && self.cur_src() == word
    }

    /// The token after the current one is a name on the same line.
    fn qml_next_is_name(&mut self) -> bool {
        self.lookahead(|p| {
            p.bump_any();
            p.cur_kind().is_identifier_name() && !p.cur_token().is_on_new_line()
        })
    }
}
