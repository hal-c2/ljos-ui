//! The members of Qt's `Qt` global, each where Ljós's API has it
//! (docs/DIALECT.md §4.3.1): `Qt.point(…)` is `point(…)`, `Qt.callLater(…)`
//! `later(…)`, `Qt.darker(c, 2)` `Color.of(c).darker(2)`, `Qt.platform`
//! `App.platform`, `Qt.AlignLeft` `Align.Left`, and `Qt.red` where it goes to
//! a `color` is `"red"`.
//!
//! A file of Ljós's API has no `Qt`, so a file is lowered only when every
//! member of `Qt` it uses is lowered; one that keeps a member stays Qt's
//! whole and says which ([`Members::settle`]). A member is lowered only where
//! the name it becomes can be nothing else: the new name is a free name of
//! the lowered file, found after everything the file has in sight, so
//! anything in sight of that name would take it.
//! DECISION: the pass cannot see every name in sight, so a name is free
//! only when it is no other word of the file (outside a property's type), no
//! member of a type the file's objects are, no name a file beside it
//! declares on the root it is built on, and, for a name in lower case, the
//! file has no object whose type the pass does not know and no delegate,
//! whose model's roles would come first. A capitalised name (`App`) is never
//! a member or a role, but may be a type of a directory the file imports by
//! path, which the pass cannot see, so under such an import it is not free.
//! A project lowers a file again when a file of a name the pass gives comes
//! beside it or goes ([`introduced`]).

use std::ops::Range;

use qt_types::{Type, ljos_enums};

use crate::{Cause, Hider, Kept};

const APP: &str = "App";
const COLOR: &str = "Color";
const FONT: &str = "Font";
const LOCALE: &str = "Locale";

/// The makers `Ljos.Core` calls as `Qt` does.
const SAME: &[&str] = &[
    "point",
    "size",
    "rect",
    "vector2d",
    "vector3d",
    "vector4d",
    "quaternion",
    "matrix4x4",
    "rgba",
    "hsla",
    "hsva",
];

/// The members `App` has under Qt's name.
const ON_APP: &[&str] = &[
    "platform",
    "openUrlExternally",
    "quit",
    "exit",
    "inputMethod",
];

/// A colour's own methods `Qt` takes the colour of first:
/// `Qt.darker(c, 2)` is `Color.of(c).darker(2)`.
const OF_COLOUR: &[(&str, &str)] = &[
    ("darker", "darker"),
    ("lighter", "lighter"),
    ("tint", "tint"),
    ("alpha", "alpha"),
    ("colorEqual", "equals"),
];

/// Qt's colours by name, as a `color` is written in Ljós's API: the name
/// where the SVG colour of it is Qt's, and the number otherwise
/// (`Qt.green` is `#00ff00`, the SVG `green` `#008000`).
const COLOURS: &[(&str, &str)] = &[
    ("black", "\"black\""),
    ("white", "\"white\""),
    ("red", "\"red\""),
    ("blue", "\"blue\""),
    ("cyan", "\"cyan\""),
    ("magenta", "\"magenta\""),
    ("yellow", "\"yellow\""),
    ("transparent", "\"transparent\""),
    ("green", "\"#00ff00\""),
    ("darkGray", "\"#808080\""),
    ("gray", "\"#a0a0a4\""),
    ("lightGray", "\"#c0c0c0\""),
    ("darkRed", "\"#800000\""),
    ("darkGreen", "\"#008000\""),
    ("darkBlue", "\"#000080\""),
    ("darkCyan", "\"#008080\""),
    ("darkMagenta", "\"#800080\""),
    ("darkYellow", "\"#808000\""),
];

/// Where a member of `Qt` is in Ljós's API, as `ljos check` names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Where {
    /// `text` (`Align.Left`, `later`, `App.platform`) of `module`
    /// (`Ljos.Ui`, `Ljos.Core`).
    Home { text: String, module: &'static str },
    /// A colour of Qt's, which is its name where it goes to a `color`.
    Colour { text: &'static str },
    /// Ljós's API has none yet.
    Nowhere,
}

/// Where `Qt.<member>` is in Ljós's API.
pub fn where_is(member: &str) -> Where {
    let core = |text: String| Where::Home {
        text,
        module: "Ljos.Core",
    };
    if let Some(home) = ljos_enums::home(member) {
        return Where::Home {
            text: home.to_string(),
            module: "Ljos.Ui",
        };
    }
    if let Some((_, text)) = COLOURS.iter().find(|(name, _)| *name == member) {
        return Where::Colour { text };
    }
    if SAME.contains(&member) {
        return core(member.to_owned());
    }
    if ON_APP.contains(&member) {
        return core(format!("{APP}.{member}"));
    }
    if let Some((_, method)) = OF_COLOUR.iter().find(|(name, _)| *name == member) {
        return core(format!("{COLOR}.of(…).{method}"));
    }
    core(
        match member {
            "callLater" => "later",
            "resolvedUrl" => "url",
            "binding" => "bind",
            "color" => "Color.of",
            "font" => "Font.of",
            "fontFamilies" => "Font.families",
            "locale" => "Locale.current",
            "formatDate" => "Locale.current.formatDate",
            "formatTime" => "Locale.current.formatTime",
            "formatDateTime" => "Locale.current.formatDateTime",
            "application" => "App",
            "uiLanguage" => "App.language",
            "styleHints" => "App.hints",
            _ => return Where::Nowhere,
        }
        .to_owned(),
    )
}

/// The name lowering `Qt.member` brings into the file that a file beside it
/// could have: a file of that name beside it would take it, so the project
/// lowers the file again when one comes or goes.
pub(crate) fn introduced(member: &str) -> Option<&'static str> {
    if let Some(home) = ljos_enums::home(member) {
        return Some(home.ty.split_once('.').map_or(home.ty, |(ty, _)| ty));
    }
    if ON_APP.contains(&member) || matches!(member, "application" | "uiLanguage" | "styleHints") {
        return Some(APP);
    }
    if member == "color" || OF_COLOUR.iter().any(|(name, _)| *name == member) {
        return Some(COLOR);
    }
    match member {
        "font" | "fontFamilies" => Some(FONT),
        "locale" | "formatDate" | "formatTime" | "formatDateTime" => Some(LOCALE),
        _ => None,
    }
}

/// A call of a member of `Qt`, as [`Members::qt`] is told of it.
#[derive(Clone, Debug)]
pub(crate) struct Call {
    /// The span of each argument; `None` for a spread.
    pub(crate) arguments: Vec<Option<Range<usize>>>,
    /// Whether each argument is written as a string.
    pub(crate) texts: Vec<bool>,
    /// The whole call, `Qt.darker(c, 2)`.
    pub(crate) span: Range<usize>,
}

/// One `Qt.member` of the file.
#[derive(Debug)]
struct Use {
    /// Where `Qt` is, which a kept file points at.
    at: usize,
    member: String,
    lowered: Result<Lowered, Cause>,
}

/// What lowering one member changes, and the name it brings in.
#[derive(Debug)]
struct Lowered {
    changes: Vec<(Range<usize>, String)>,
    /// The name it becomes, which must be free; none for a colour's name.
    name: Option<&'static str>,
}

/// What the pass gathers, over the file, to lower its members of `Qt`.
#[derive(Debug, Default)]
pub(crate) struct Members {
    uses: Vec<Use>,
    /// The types of the file's objects, whose members are in sight.
    types: Vec<&'static Type>,
    /// Where the types of the properties the file declares are written:
    /// `point` of `property point at` is no name in sight.
    typed: Vec<Range<usize>>,
    /// The names of the properties and functions the file declares.
    declared: Vec<String>,
    /// The values that go to a `color`.
    colours: Vec<Range<usize>>,
    /// The file has a delegate, whose model's roles are in sight.
    delegate: bool,
    /// The file has an object whose type the pass does not know.
    unknown: bool,
}

impl Members {
    /// Whether `Qt` at `at` is a use already told of: `Qt.application` of
    /// `Qt.application.name`, or the callee of a call told of.
    pub(crate) fn has(&self, at: usize) -> bool {
        self.uses.iter().any(|u| u.at == at)
    }

    /// `Qt.member`, read as `whole`, at `qt` (the span of `Qt`) and `at`
    /// (that of the member's name), and the call it is the callee of.
    pub(crate) fn qt(
        &mut self,
        whole: Range<usize>,
        qt: Range<usize>,
        member: &str,
        at: Range<usize>,
        call: Option<Call>,
    ) {
        if self.has(qt.start) {
            return;
        }
        let lowered = self.lowered(&whole, &qt, member, &at, call.as_ref());
        self.uses.push(Use {
            at: qt.start,
            member: member.to_owned(),
            lowered,
        });
    }

    fn lowered(
        &self,
        whole: &Range<usize>,
        qt: &Range<usize>,
        member: &str,
        at: &Range<usize>,
        call: Option<&Call>,
    ) -> Result<Lowered, Cause> {
        let named = |text: String, name: &'static str| Lowered {
            changes: vec![(qt.start..at.end, text)],
            name: Some(name),
        };
        if let Some(home) = ljos_enums::home(member) {
            let ty = home.ty.split_once('.').map_or(home.ty, |(ty, _)| ty);
            return Ok(named(home.to_string(), ty));
        }
        if let Some((_, text)) = COLOURS.iter().find(|(name, _)| *name == member) {
            return if self.colours.contains(whole) {
                Ok(Lowered {
                    changes: vec![(whole.clone(), (*text).to_owned())],
                    name: None,
                })
            } else {
                Err(Cause::QtColourNumber {
                    name: member.to_owned(),
                })
            };
        }
        if let Some(name) = SAME.iter().find(|name| **name == member) {
            return Ok(named((*name).to_owned(), name));
        }
        if ON_APP.contains(&member) {
            return Ok(Lowered {
                changes: vec![(qt.clone(), APP.to_owned())],
                name: Some(APP),
            });
        }
        if let Some((_, method)) = OF_COLOUR.iter().find(|(name, _)| *name == member) {
            // `Qt.darker(c, 2)` is `Color.of(c).darker(2)`: what is written
            // between the two is left, so a member of `Qt` in `c` is
            // lowered too.
            let Some(call) = call else {
                return Err(Cause::QtNoHome {
                    member: member.to_owned(),
                });
            };
            let arguments: Option<Vec<Range<usize>>> = call.arguments.iter().cloned().collect();
            let Some(arguments) = arguments.filter(|a| !a.is_empty()) else {
                return Err(Cause::QtNoHome {
                    member: member.to_owned(),
                });
            };
            let first = &arguments[0];
            let rest = arguments.get(1).map_or(call.span.end - 1, |a| a.start);
            return Ok(Lowered {
                changes: vec![
                    (qt.start..first.start, format!("{COLOR}.of(")),
                    (first.end..rest, format!(").{method}(")),
                ],
                name: Some(COLOR),
            });
        }
        Ok(match member {
            "callLater" => named("later".into(), "later"),
            "binding" => named("bind".into(), "bind"),
            "color" => named(format!("{COLOR}.of"), COLOR),
            "font" => named(format!("{FONT}.of"), FONT),
            "fontFamilies" => named(format!("{FONT}.families"), FONT),
            // A locale's `formatDate` reads a number as a Locale.FormatType,
            // where `Qt.formatDate` reads it as a Qt.DateFormat, and takes
            // no locale: only a format written as a string, or none, keeps
            // its meaning.
            "formatDate" | "formatTime" | "formatDateTime" => match call {
                Some(call)
                    if call.arguments.len() == 1 && call.arguments[0].is_some()
                        || call.arguments.len() == 2
                            && call.arguments[0].is_some()
                            && call.texts[1] =>
                {
                    named(format!("{LOCALE}.current.{member}"), LOCALE)
                }
                _ => {
                    return Err(Cause::QtFormatNotText {
                        member: member.to_owned(),
                    });
                }
            },
            "application" => named(APP.into(), APP),
            "uiLanguage" => named(format!("{APP}.language"), APP),
            "styleHints" => named(format!("{APP}.hints"), APP),
            // Qt 6 resolves `Qt.resolvedUrl(u, ctx)` against the file of
            // ctx's component, and `url` takes one argument; a spread or the
            // bare function may carry a context.
            "resolvedUrl" => match call {
                Some(call) if matches!(call.arguments.as_slice(), [Some(_)]) => {
                    named("url".into(), "url")
                }
                Some(_) => return Err(Cause::QtResolvedAgainst),
                None => {
                    return Err(Cause::QtNoHome {
                        member: member.to_owned(),
                    });
                }
            },
            "locale" => match call {
                Some(call) if call.arguments.is_empty() => Lowered {
                    changes: vec![(call.span.clone(), format!("{LOCALE}.current"))],
                    name: Some(LOCALE),
                },
                Some(call) if matches!(call.arguments.as_slice(), [Some(_)]) => {
                    named(format!("{LOCALE}.named"), LOCALE)
                }
                _ => {
                    return Err(Cause::QtNoHome {
                        member: member.to_owned(),
                    });
                }
            },
            _ => {
                return Err(Cause::QtNoHome {
                    member: member.to_owned(),
                });
            }
        })
    }

    /// `Qt` read alone, not as `Qt.member`, which has no home.
    pub(crate) fn bare(&mut self, qt: Range<usize>) {
        if self.has(qt.start) {
            return;
        }
        self.uses.push(Use {
            at: qt.start,
            member: String::new(),
            lowered: Err(Cause::QtNoHome {
                member: String::new(),
            }),
        });
    }

    pub(crate) fn object_type(&mut self, ty: Option<&'static Type>) {
        match ty {
            Some(ty) => self.types.push(ty),
            None => self.unknown = true,
        }
    }

    pub(crate) fn delegate(&mut self) {
        self.delegate = true;
    }

    pub(crate) fn typed(&mut self, range: Range<usize>) {
        self.typed.push(range);
    }

    pub(crate) fn declared(&mut self, name: &str) {
        self.declared.push(name.to_owned());
    }

    /// `value` goes to a `color`: `Qt.red` there is the colour.
    pub(crate) fn colour(&mut self, value: Range<usize>) {
        self.colours.push(value);
    }

    /// The changes that lower every member of `Qt` of the file, or why the
    /// file is kept: the first member, in the file's order, that has no
    /// home or whose name is not free. `beside` says what beside the file
    /// may take a name ([`Scope::may_be_beside`](crate::scope::Scope::may_be_beside)).
    pub(crate) fn settle(
        mut self,
        source: &str,
        beside: impl Fn(&str) -> Option<Hider>,
    ) -> Result<Settled, Kept> {
        self.uses.sort_by_key(|u| u.at);
        let changed: Vec<Range<usize>> = self
            .uses
            .iter()
            .filter_map(|u| u.lowered.as_ref().ok())
            .flat_map(|l| l.changes.iter().map(|(range, _)| range.clone()))
            .collect();
        let hider = |name: &str| -> Option<Hider> {
            let capitalised = name.starts_with(|c: char| c.is_ascii_uppercase());
            if let Some(by) = beside(name) {
                return Some(by);
            }
            if !capitalised {
                if self.declared.iter().any(|d| d == name) {
                    return Some(Hider::Own);
                }
                if self.types.iter().any(|ty| {
                    ty.property(name).is_some() || ty.has_method(name) || ty.signal(name).is_some()
                }) {
                    return Some(Hider::Member);
                }
                if self.delegate {
                    return Some(Hider::Role);
                }
                if self.unknown {
                    return Some(Hider::UnknownType);
                }
            }
            let elsewhere = words(source, name).any(|at| {
                !changed.iter().any(|c| c.contains(&at))
                    && !self.typed.iter().any(|t| t.contains(&at))
            });
            elsewhere.then_some(Hider::Word)
        };
        let mut changes = Vec::new();
        let mut core = false;
        for u in &self.uses {
            let cause = match &u.lowered {
                Err(cause) => Some(cause.clone()),
                Ok(lowered) => lowered.name.and_then(|name| {
                    hider(name).map(|by| Cause::QtHomeHidden {
                        member: u.member.clone(),
                        name,
                        by,
                    })
                }),
            };
            if let Some(cause) = cause {
                return Err(Kept { cause, at: u.at });
            }
            if let Ok(lowered) = &u.lowered {
                changes.extend(lowered.changes.iter().cloned());
                core |= lowered.name.is_some() && ljos_enums::home(&u.member).is_none();
            }
        }
        Ok(Settled { changes, core })
    }
}

/// The changes that lower a file's members of `Qt`.
#[derive(Debug, Default)]
pub(crate) struct Settled {
    pub(crate) changes: Vec<(Range<usize>, String)>,
    /// Some lower to a name of `Ljos.Core`, which the file then imports.
    pub(crate) core: bool,
}

/// Where `name` is a whole word of `source`, as byte offsets.
fn words<'a>(source: &'a str, name: &'a str) -> impl Iterator<Item = usize> + 'a {
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    source.match_indices(name).filter_map(move |(at, _)| {
        let before = source[..at].chars().next_back();
        let after = source[at + name.len()..].chars().next();
        (!before.is_some_and(is_word) && !after.is_some_and(is_word)).then_some(at)
    })
}
