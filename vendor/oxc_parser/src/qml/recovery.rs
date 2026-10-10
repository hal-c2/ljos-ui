//! Member-level error recovery for QML mode.
//!
//! The JavaScript parser reports most syntax errors as *fatal*: it records the
//! error, moves the lexer to the end of the file and unwinds with dummy nodes.
//! That is right for JavaScript and wrong for a UI toolkit, where one typo
//! must not lose the rest of the file (hot reload, the language server and
//! `ljos check` all need a partial tree).
//!
//! So QML mode parses each unit that has a natural boundary (an object member,
//! an import, a pragma) under [`ParserImpl::qml_recover`]. When the unit ends
//! in a fatal error, the parser:
//!
//! 1. takes the fatal error and keeps it as an ordinary diagnostic;
//! 2. rewinds to the start of the unit, which also discards any half-built
//!    state of the failed attempt;
//! 3. skips tokens to a synchronisation point (see [`Sync`]);
//! 4. carries on with the next unit.
//!
//! The failed unit is left out of the tree. Rewinding and re-scanning, rather
//! than skipping from wherever the error happened, is what makes the skip
//! independent of how far the JavaScript parser got before it gave up.
//!
//! Termination: every recovery consumes at least the unit's first token, and
//! every later step of the skip consumes a token or stops.

use oxc_span::Span;

use crate::{ParserConfig as Config, ParserImpl, lexer::Kind};

/// Where a recovery stops skipping.
#[derive(Clone, Copy)]
pub(super) enum Sync {
    /// An object member. Skips whole bracket groups, and stops after a `;` or
    /// before a `}` that closes the enclosing object, or before a token that
    /// starts a member on a new line.
    Member,
    /// An import or pragma. Stops after a `;` or before the next line.
    Line,
}

/// A bracket the skip has seen and not yet seen closed.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Open {
    Paren,
    Bracket,
    Brace,
    /// A `${` inside a template literal: closed by a `}` that the lexer has to
    /// be told to read as the rest of the template.
    Template,
}

impl<'a, C: Config> ParserImpl<'a, C> {
    /// Runs `parse`. If it ends in a fatal error, records the error, skips to
    /// a synchronisation point and returns `None`.
    pub(super) fn qml_recover<T>(
        &mut self,
        sync: Sync,
        parse: impl FnOnce(&mut Self) -> T,
    ) -> Option<T> {
        // The error-recovering checkpoint keeps lexer errors that the failed
        // attempt may have popped (`set_unexpected` turns one into the fatal
        // error).
        let checkpoint = self.checkpoint_with_error_recovery();
        let start = self.cur_start();
        let value = parse(self);
        let Some(fatal) = self.fatal_error.take() else {
            return Some(value);
        };

        self.rewind(checkpoint);
        // After the rewind, which would truncate it away.
        self.errors.push(fatal.error);
        // Speculative state of the failed attempt, keyed by source offset.
        self.state.cover_initialized_name.retain(|at, _| *at < start);
        self.state.trailing_commas.retain(|at, _| *at < start);
        self.state.not_parenthesized_arrow.retain(|at| *at < start);
        match sync {
            Sync::Member => self.qml_skip_member(start),
            Sync::Line => self.qml_skip_line(),
        }
        None
    }

    /// Skips the rest of a broken import or pragma.
    fn qml_skip_line(&mut self) {
        self.bump_any();
        while !self.at(Kind::Eof) {
            if self.at(Kind::Semicolon) {
                self.bump_any();
                return;
            }
            if self.cur_token().is_on_new_line() {
                return;
            }
            self.bump_any();
        }
    }

    /// Skips a broken object member that started at `member_start`.
    fn qml_skip_member(&mut self, member_start: u32) {
        let column = self.qml_column(member_start);
        let mut open: Vec<Open> = Vec::new();
        let mut first = true;
        while !self.at(Kind::Eof) {
            let kind = self.cur_kind();
            // `first` keeps the progress guarantee: the member's own first
            // token is always consumed, whatever it is.
            if !first {
                let on_new_line = self.cur_token().is_on_new_line();
                if open.is_empty() {
                    if kind == Kind::RCurly {
                        // Closes the enclosing object.
                        return;
                    }
                    if on_new_line && self.qml_at_member_start() {
                        return;
                    }
                } else if on_new_line
                    && self.qml_column(self.cur_start()) <= column
                    && self.qml_at_member_start()
                {
                    // Brackets that were never closed would swallow the rest
                    // of the object. A member that starts no deeper than the
                    // broken one is a sibling, so give the brackets up.
                    return;
                }
            }
            first = false;

            match kind {
                Kind::LParen => open.push(Open::Paren),
                Kind::LBrack => open.push(Open::Bracket),
                Kind::LCurly => open.push(Open::Brace),
                Kind::TemplateHead => open.push(Open::Template),
                Kind::RParen | Kind::RBrack => {
                    let want = if kind == Kind::RParen { Open::Paren } else { Open::Bracket };
                    // A closer ends the innermost bracket of its kind, and
                    // whatever was left open inside it (`{ foo( }`). A closer
                    // with no opener is stray and skipped.
                    if let Some(at) = open.iter().rposition(|o| *o == want) {
                        open.truncate(at);
                    }
                }
                Kind::RCurly => {
                    let Some(at) =
                        open.iter().rposition(|o| matches!(o, Open::Brace | Open::Template))
                    else {
                        // Not inside a group of ours: the enclosing object's.
                        return;
                    };
                    if open[at] == Open::Template {
                        // The lexer read `}`; the rest of the template is
                        // string text.
                        self.re_lex_template_substitution_tail();
                        if self.cur_kind() == Kind::TemplateMiddle {
                            open.truncate(at + 1);
                        } else {
                            open.truncate(at);
                        }
                    } else {
                        open.truncate(at);
                    }
                }
                Kind::Semicolon if open.is_empty() => {
                    self.bump_any();
                    return;
                }
                _ => {}
            }
            self.bump_any();
        }
    }

    /// The current token is a name followed by what starts a member: `name:`,
    /// `Type {`, `a.b`, or a declaration keyword and a name. Only used on the
    /// first token of a line, to find where the next member starts.
    fn qml_at_member_start(&mut self) -> bool {
        let kind = self.cur_kind();
        if !kind.is_identifier_name() {
            return false;
        }
        if matches!(kind, Kind::Function | Kind::Enum) {
            return true;
        }
        let declaration = matches!(kind, Kind::Default | Kind::Readonly)
            || (kind == Kind::Ident
                && matches!(self.cur_src(), "property" | "required" | "signal" | "component"));
        self.lookahead(|p| {
            p.bump_any();
            match p.cur_kind() {
                Kind::Colon | Kind::LCurly | Kind::Dot => true,
                next => declaration && next.is_identifier_name() && !p.cur_token().is_on_new_line(),
            }
        })
    }

    /// Bytes between the start of `offset`'s line and `offset`. Indentation is
    /// compared as bytes: a file that mixes tabs and spaces gets a worse
    /// recovery, never a wrong tree.
    fn qml_column(&self, offset: u32) -> u32 {
        let before = &self.source_text.as_bytes()[..offset as usize];
        let line_start = before.iter().rposition(|b| *b == b'\n').map_or(0, |at| at + 1);
        offset - line_start as u32
    }

    /// Records that `what` was expected, without stopping the parse.
    pub(super) fn qml_error_here(&mut self, message: &'static str, span: Span) {
        if self.fatal_error.is_none() {
            self.error(crate::diagnostics::qml(message, span));
        }
    }
}
