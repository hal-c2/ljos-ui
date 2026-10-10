//! QML mode.
//!
//! QML is a small declarative shell (about twenty productions) around
//! JavaScript. This module parses the shell with the same lexer and cursor as
//! the JavaScript parser and calls straight into it for every binding
//! expression, script block and function, so embedded JavaScript ends exactly
//! where the JavaScript grammar says it ends and all spans are file offsets.

pub mod ast;

use oxc_allocator::ArenaVec;
use oxc_ast::ast::{Comment, Expression, IdentifierName, ObjectPropertyKind, PropertyKey};
use oxc_diagnostics::Diagnostics;

use crate::{
    ParserImpl,
    config::ParserConfig,
    context::{Context, StatementContext},
    diagnostics::{self, ParserDiagnostic},
    js::FunctionKind,
    lexer::Kind,
};

use ast::*;

/// Return value of [`Parser::parse_qml`](crate::Parser::parse_qml).
pub struct QmlParserReturn<'a> {
    /// Incomplete when [`panicked`](Self::panicked) is set.
    pub document: QmlDocument<'a>,
    pub comments: ArenaVec<'a, Comment>,
    pub diagnostics: Diagnostics,
    /// The parser hit an unrecoverable syntax error and stopped early.
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
        let diagnostics = self
            .lexer
            .errors
            .drain(..)
            .chain(self.errors.drain(..))
            .map(ParserDiagnostic::into_diagnostic)
            .collect::<Diagnostics>();
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
                imports.push(self.parse_qml_import());
            } else if self.qml_at_word("pragma") && self.qml_next_is_name() {
                pragmas.push(self.parse_qml_pragma());
            } else {
                break;
            }
        }

        let root = self.parse_qml_object();
        if self.fatal_error.is_none() && !self.at(Kind::Eof) {
            let span = self.cur_token().span();
            self.set_fatal_error(diagnostics::qml("A QML document has exactly one root object", span));
        }
        QmlDocument { span: self.end_span(start), pragmas, imports, root }
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
            QmlImportSource::Module(self.parse_qml_qualified_name())
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
        let alias = if self.eat(Kind::As) { Some(self.parse_identifier_name()) } else { None };
        self.bump(Kind::Semicolon);
        QmlImport { span: self.end_span(start), source, version, alias }
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
        self.parse_qml_object_rest(start, type_name)
    }

    fn parse_qml_object_rest(
        &mut self,
        start: u32,
        type_name: QmlQualifiedName<'a>,
    ) -> QmlObject<'a> {
        let on = if self.qml_at_word("on") {
            self.bump_any();
            Some(self.parse_qml_qualified_name())
        } else {
            None
        };
        let opening_span = self.cur_token().span();
        self.expect(Kind::LCurly);
        let mut members = ArenaVec::new_in(&self.ast);
        while !self.at(Kind::RCurly) && !self.has_fatal_error() {
            if let Some(member) = self.parse_qml_member() {
                members.push(member);
            }
        }
        self.expect_closing(Kind::RCurly, opening_span);
        QmlObject { span: self.end_span(start), type_name, on, members }
    }

    fn parse_qml_member(&mut self) -> Option<QmlMember<'a>> {
        let start = self.cur_start();
        match self.cur_kind() {
            Kind::Semicolon => {
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
            let value = self.parse_qml_binding_value();
            return Some(QmlMember::Binding(QmlBinding { span: self.end_span(start), name, value }));
        }
        Some(QmlMember::Object(self.parse_qml_object_rest(start, name)))
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
        let value =
            if self.eat(Kind::Colon) { Some(self.parse_qml_binding_value()) } else { None };
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
        self.bump(Kind::Semicolon);
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

    fn parse_qml_binding_value(&mut self) -> QmlBindingValue<'a> {
        match self.cur_kind() {
            Kind::LCurly => self.parse_qml_braced_value(),
            Kind::If | Kind::Switch | Kind::Try | Kind::With => {
                QmlBindingValue::Statement(self.context_add(Context::Return, |p| {
                    p.parse_statement_list_item(StatementContext::StatementList)
                }))
            }
            Kind::LBrack if self.qml_at_object_list() => {
                self.bump_any();
                let mut objects = ArenaVec::new_in(&self.ast);
                while !self.at(Kind::RBrack) && !self.has_fatal_error() {
                    objects.push(self.parse_qml_object());
                    if !self.eat(Kind::Comma) {
                        break;
                    }
                }
                self.expect(Kind::RBrack);
                self.bump(Kind::Semicolon);
                QmlBindingValue::Objects(objects)
            }
            _ if self.qml_at_object() => {
                let object = self.parse_qml_object();
                self.bump(Kind::Semicolon);
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

    /// `name: { ... }` is an object literal when it parses as one and a script
    /// block otherwise, which is how Qt's own parser resolves the ambiguity.
    /// `{ a }` and `{ a = b }` are both to JavaScript's grammar, the second
    /// until it turns out not to be a pattern; in QML they are blocks. So is
    /// `{ if (a) { } }`, which to JavaScript's grammar is also an object with
    /// a method named `if`.
    fn parse_qml_braced_value(&mut self) -> QmlBindingValue<'a> {
        let checkpoint = self.checkpoint_with_error_recovery();
        let errors_before = self.errors_count();
        let start = self.cur_token().span().start;
        let value = self.parse_qml_expression_value();
        let initialized = self.state.cover_initialized_name.keys().any(|at| *at >= start);
        let is_literal = match &value {
            QmlBindingValue::Expression(Expression::ObjectExpression(object)) => {
                object.properties.is_empty()
                    || object.properties.iter().any(|property| match property {
                        ObjectPropertyKind::ObjectProperty(property) => {
                            !property.shorthand && !(property.method && Self::qml_names_statement(&property.key))
                        }
                        ObjectPropertyKind::SpreadProperty(_) => true,
                    })
            }
            _ => true,
        };
        if self.fatal_error.is_none() && self.errors_count() == errors_before && !initialized && is_literal {
            return value;
        }
        self.state.cover_initialized_name.retain(|at, _| *at < start);
        self.rewind(checkpoint);
        QmlBindingValue::Statement(
            self.context_add(Context::Return, |p| p.parse_block_statement()),
        )
    }

    /// A method by the name of a statement that is written as a call and a
    /// block: `if (a) { }`.
    fn qml_names_statement(key: &PropertyKey<'a>) -> bool {
        matches!(key, PropertyKey::StaticIdentifier(name) if matches!(name.name.as_str(), "if" | "while" | "with" | "switch" | "for"))
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
