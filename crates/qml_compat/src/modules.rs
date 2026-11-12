//! The Qt modules a file may import, one row each, and what the pass makes
//! of each: the Ljós module it lowers to, what becomes of its import line,
//! and where a name it has is looked up. What the pass does with an import
//! of Qt's is read from here, not written once per module where it is used.

use std::cell::Cell;

use qt_types::Module;

use crate::{CONTROLS, CORE, Cause, LAYOUT, MODELS_MODULE, MODULE, QT_DIALOGS};

/// A module of Ljós's that a lowered file imports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ljos {
    Ui,
    Core,
    Models,
    Layout,
    Controls,
}

impl Ljos {
    /// In the order a shared line names them: `import Ljos.Ui; import
    /// Ljos.Core; import Ljos.Models; import Ljos.Layout`.
    pub(crate) const ALL: [Ljos; 5] = [
        Ljos::Ui,
        Ljos::Core,
        Ljos::Models,
        Ljos::Layout,
        Ljos::Controls,
    ];

    pub(crate) fn uri(self) -> &'static str {
        match self {
            Ljos::Ui => MODULE,
            Ljos::Core => CORE,
            Ljos::Models => MODELS_MODULE,
            Ljos::Layout => LAYOUT,
            Ljos::Controls => CONTROLS,
        }
    }

    /// The module whose line this one's is, when no Qt import of the file
    /// lowers to it: a file of `QtQuick` alone has `Ljos.Core` on
    /// `Ljos.Ui`'s line, and one of `QtQml` alone `Ljos.Ui` on
    /// `Ljos.Core`'s. Ljós's controls are only ever on a line of their own.
    fn shares(self) -> Option<Ljos> {
        match self {
            Ljos::Ui => Some(Ljos::Core),
            Ljos::Core | Ljos::Models | Ljos::Layout => Some(Ljos::Ui),
            Ljos::Controls => None,
        }
    }

    /// Whether the first import that lowers to it takes its line, rather
    /// than the last: a second import of Qt's controls is the first again.
    fn first_takes_the_line(self) -> bool {
        self == Ljos::Controls
    }
}

/// A Qt module, by what the pass does with an import of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Qt {
    /// `QtQuick`, which has `QtQml`'s types too.
    Quick,
    Qml,
    Models,
    Layouts,
    Shapes,
    Window,
    Controls,
    Dialogs,
}

impl Qt {
    const COUNT: usize = 8;

    /// The Ljós module its types lower to.
    pub(crate) fn lowers_to(self) -> Ljos {
        ROWS.iter()
            .find(|row| row.qt == self)
            .map_or(Ljos::Ui, |row| row.lowers_to)
    }

    /// The imports that bring its types: `QtQuick` has `QtQml`'s.
    pub(crate) fn brought_by(self) -> &'static [Qt] {
        match self {
            Qt::Qml => &[Qt::Quick, Qt::Qml],
            Qt::Quick => &[Qt::Quick],
            Qt::Models => &[Qt::Models],
            Qt::Layouts => &[Qt::Layouts],
            Qt::Shapes => &[Qt::Shapes],
            Qt::Window => &[Qt::Window],
            Qt::Controls => &[Qt::Controls],
            Qt::Dialogs => &[Qt::Dialogs],
        }
    }
}

/// What becomes of an import of a Qt module the file imports under a name
/// (`import QtQuick as Q`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UnderName {
    /// The file is kept: its types are named through the name, which the
    /// lowered file would have to say of Ljós's module.
    Keeps,
    /// It is any module's import, which the pass does not lower.
    IsAnother,
}

/// What the pass reads of a Qt module's description.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Described {
    /// Read before the imports are, and the file is kept without it: every
    /// name of a file is looked up there.
    Always,
    /// Read where the file imports it; without it, its types are not known.
    WhereImported,
    /// Read where the file imports it, with Ljós's description of the
    /// module it lowers to; without either the file is kept.
    WithLjos,
    /// Not read: its types are another module's.
    No,
}

/// One Qt module, and what the pass makes of it.
#[derive(Debug)]
pub(crate) struct Row {
    pub uri: &'static str,
    pub qt: Qt,
    pub under_name: UnderName,
    /// Whether it is lowered only where the program starts with the style
    /// [`CONTROLS`] draws (`QtQuick.Controls`, whose style is chosen when
    /// the program runs).
    pub styled: bool,
    pub described: Described,
    /// The Ljós module its types are, and its import becomes.
    pub lowers_to: Ljos,
    /// Whether its import may take the line of the module it lowers to.
    /// One that does not is emptied: `Ljos.Ui` has the shapes and what
    /// `QtQuick.Window` names, on `QtQuick`'s line.
    pub takes_the_line: bool,
    /// Whether a name the file reads may be one of its types, and the import
    /// is among those that decide what a name is.
    pub names_types: bool,
}

/// Every Qt module the pass lowers.
/// Whether `uri` is a Ljós module the pass writes for one of Qt's.
pub(crate) fn written(uri: &str) -> bool {
    ROWS.iter().any(|row| row.lowers_to.uri() == uri)
}

pub(crate) const ROWS: &[Row] = &[
    Row {
        uri: "QtQuick",
        qt: Qt::Quick,
        under_name: UnderName::Keeps,
        styled: false,
        described: Described::Always,
        lowers_to: Ljos::Ui,
        takes_the_line: true,
        names_types: true,
    },
    Row {
        uri: "QtQml",
        qt: Qt::Qml,
        under_name: UnderName::Keeps,
        styled: false,
        described: Described::Always,
        lowers_to: Ljos::Core,
        takes_the_line: true,
        names_types: true,
    },
    // Its models are `Ljos.Models`'.
    Row {
        uri: "QtQml.Models",
        qt: Qt::Models,
        under_name: UnderName::Keeps,
        styled: false,
        described: Described::No,
        lowers_to: Ljos::Models,
        takes_the_line: true,
        names_types: false,
    },
    // Its line layouts are lowered to Ljós's arrangement (`arrange`); its
    // other types keep the file.
    Row {
        uri: "QtQuick.Layouts",
        qt: Qt::Layouts,
        under_name: UnderName::IsAnother,
        styled: false,
        described: Described::WhereImported,
        lowers_to: Ljos::Layout,
        takes_the_line: true,
        names_types: true,
    },
    // `Ljos.Ui` has its types as they are.
    Row {
        uri: "QtQuick.Shapes",
        qt: Qt::Shapes,
        under_name: UnderName::Keeps,
        styled: false,
        described: Described::WhereImported,
        lowers_to: Ljos::Ui,
        takes_the_line: false,
        names_types: true,
    },
    // Its types are QtQuick's, and `Ljos.Ui` has them.
    Row {
        uri: "QtQuick.Window",
        qt: Qt::Window,
        under_name: UnderName::IsAnother,
        styled: false,
        described: Described::No,
        lowers_to: Ljos::Ui,
        takes_the_line: false,
        names_types: false,
    },
    // Ljós's controls are Basic's; `QtQuick.Controls` is the style the
    // program starts with, which may be another.
    Row {
        uri: "QtQuick.Controls",
        qt: Qt::Controls,
        under_name: UnderName::Keeps,
        styled: true,
        described: Described::WithLjos,
        lowers_to: Ljos::Controls,
        takes_the_line: true,
        names_types: true,
    },
    Row {
        uri: "QtQuick.Controls.Basic",
        qt: Qt::Controls,
        under_name: UnderName::Keeps,
        styled: false,
        described: Described::WithLjos,
        lowers_to: Ljos::Controls,
        takes_the_line: true,
        names_types: true,
    },
    // Its file and folder dialogs are `Ljos.Controls.Basic`'s, so its
    // import is one of the controls'.
    Row {
        uri: QT_DIALOGS,
        qt: Qt::Dialogs,
        under_name: UnderName::Keeps,
        styled: false,
        described: Described::WithLjos,
        lowers_to: Ljos::Controls,
        takes_the_line: true,
        names_types: true,
    },
];

/// The row of the module `uri`, when the pass lowers it.
pub(crate) fn row(uri: &str) -> Option<&'static Row> {
    ROWS.iter().find(|row| row.uri == uri)
}

/// The Qt modules a type name is looked up in, in the order the pass looks.
/// Of those in one group, the type is that of the last imported that has
/// it. `QtQuick`'s, with `QtQml`'s, are last, and known whether the file
/// imports `QtQuick` or not.
pub(crate) const LOOKED_UP: &[&[Qt]] = &[
    &[Qt::Layouts],
    &[Qt::Controls, Qt::Dialogs],
    &[Qt::Shapes],
    &[Qt::Quick],
];

/// The descriptions of the modules the pass reads, Qt's and Ljós's, and
/// what the file does with each Ljós module.
pub(crate) struct Modules {
    qt: [Option<&'static Module>; Qt::COUNT],
    ljos: [Option<&'static Module>; Ljos::ALL.len()],
    /// Which of the file's imports (the pass's `imports`) each Ljós
    /// module's line is: an import of the project after it is found before
    /// Ljós's type.
    lines: [Option<usize>; Ljos::ALL.len()],
    /// Whether the file names a type of each, which it imports then.
    named: [Cell<bool>; Ljos::ALL.len()],
    quick: &'static Module,
    qml: &'static Module,
    ui: &'static Module,
}

impl Modules {
    /// What every file needs described before its imports are read, or the
    /// module that is not.
    pub(crate) fn new() -> Result<Modules, &'static str> {
        let read = |uri: &'static str| qt_types::module(uri).ok_or(uri);
        // Ljós's first, as the pass has always said which it lacks.
        let (ui, core, layout) = (read(MODULE)?, read(CORE)?, read(LAYOUT)?);
        let (quick, qml) = (read("QtQuick")?, read("QtQml")?);
        let mut modules = Modules {
            qt: [None; Qt::COUNT],
            ljos: [None; Ljos::ALL.len()],
            lines: [None; Ljos::ALL.len()],
            named: Default::default(),
            quick,
            qml,
            ui,
        };
        modules.qt[Qt::Quick as usize] = Some(quick);
        modules.qt[Qt::Qml as usize] = Some(qml);
        modules.ljos[Ljos::Ui as usize] = Some(ui);
        modules.ljos[Ljos::Core as usize] = Some(core);
        modules.ljos[Ljos::Layout as usize] = Some(layout);
        Ok(modules)
    }

    pub(crate) fn quick(&self) -> &'static Module {
        self.quick
    }

    pub(crate) fn qml(&self) -> &'static Module {
        self.qml
    }

    pub(crate) fn ui(&self) -> &'static Module {
        self.ui
    }

    /// Qt's description of `qt`, where the pass has read it.
    pub(crate) fn qt(&self, qt: Qt) -> Option<&'static Module> {
        self.qt[qt as usize]
    }

    /// Ljós's description of `ljos`, where the pass has read it.
    pub(crate) fn ljos(&self, ljos: Ljos) -> Option<&'static Module> {
        self.ljos[ljos as usize]
    }

    /// Reads what `row` says of an import of its module. The first import
    /// of a module is the one whose description is kept. `Err` when the
    /// file is kept for want of one.
    pub(crate) fn read(&mut self, row: &Row) -> Result<(), Cause> {
        let qt = &mut self.qt[row.qt as usize];
        match row.described {
            Described::Always | Described::No => {}
            Described::WhereImported => {
                if qt.is_none() {
                    *qt = qt_types::module(row.uri);
                }
            }
            Described::WithLjos => {
                let (Some(described), Some(ljos)) = (
                    qt_types::module(row.uri),
                    qt_types::module(row.lowers_to.uri()),
                ) else {
                    return Err(Cause::NoControlsDescription {
                        uri: row.uri.to_owned(),
                    });
                };
                qt.get_or_insert(described);
                self.ljos[row.lowers_to as usize] = Some(ljos);
            }
        }
        Ok(())
    }

    /// Says that the import `at` of the file's imports lowers to `ljos`,
    /// and so may take its line.
    pub(crate) fn imported(&mut self, ljos: Ljos, at: usize) {
        let line = &mut self.lines[ljos as usize];
        if line.is_none() || !ljos.first_takes_the_line() {
            *line = Some(at);
        }
    }

    /// Gives each Ljós module no Qt import lowers to the line of the one it
    /// shares, once the imports are read.
    pub(crate) fn share_lines(&mut self) {
        for ljos in Ljos::ALL {
            if let Some(shared) = ljos.shares() {
                let line = self.lines[ljos as usize].or(self.lines[shared as usize]);
                self.lines[ljos as usize] = line;
            }
        }
    }

    /// Which of the file's imports `ljos`'s line is.
    pub(crate) fn line(&self, ljos: Ljos) -> Option<usize> {
        self.lines[ljos as usize]
    }

    /// Says that the file names a type of `ljos`'s.
    pub(crate) fn names(&self, ljos: Ljos) {
        self.named[ljos as usize].set(true);
    }

    /// Whether the file names a type of `ljos`'s.
    pub(crate) fn is_named(&self, ljos: Ljos) -> bool {
        self.named[ljos as usize].get()
    }

    /// The Ljós module the lowered file imports that has the type `name`,
    /// in the order the lowered file's imports would find it.
    pub(crate) fn ljos_with(&self, name: &str) -> Option<Ljos> {
        [Ljos::Ui, Ljos::Layout, Ljos::Controls]
            .into_iter()
            .find(|ljos| {
                self.ljos(*ljos)
                    .is_some_and(|m| m.type_named(name).is_some())
            })
    }

    /// The type `name` of a Qt module the pass has read, of the first of
    /// `qts` that has it.
    pub(crate) fn qt_type(&self, qts: &[Qt], name: &str) -> Option<&'static qt_types::Type> {
        qts.iter()
            .find_map(|qt| self.qt(*qt).and_then(|m| m.type_named(name)))
    }
}

/// Where an import of Qt's line goes in the lowered file.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Line {
    /// It takes the line of this Ljós module.
    Takes(Ljos),
    /// It is emptied: another's line has what it imports.
    Emptied,
}

/// What becomes of each Qt import's line, in the order written: for each
/// Ljós module, the one import that lowers to it and takes its line (the
/// last, or for the controls the first), and every other emptied.
pub(crate) fn lines(imports: &[&Row]) -> Vec<Line> {
    let taker = |ljos: Ljos| {
        let mut taking = imports
            .iter()
            .enumerate()
            .filter(|(_, row)| row.takes_the_line && row.lowers_to == ljos)
            .map(|(at, _)| at);
        if ljos.first_takes_the_line() {
            taking.next()
        } else {
            taking.next_back()
        }
    };
    let takers: Vec<(Ljos, Option<usize>)> = Ljos::ALL.iter().map(|l| (*l, taker(*l))).collect();
    (0..imports.len())
        .map(
            |at| match takers.iter().find(|(_, taker)| *taker == Some(at)) {
                Some((ljos, _)) => Line::Takes(*ljos),
                None => Line::Emptied,
            },
        )
        .collect()
}

/// The text of the line that has `pragma Scripts: JavaScript` and the Ljós
/// modules with no line of their own: `QtQuick`'s, else `QtQml`'s, else
/// that of the first import that takes a line (`carrier`'s). `own`
/// says which Ljós modules a Qt import's line is, `named` which the file
/// names a type of.
pub(crate) fn shared_line(
    carrier: Ljos,
    own: impl Fn(Ljos) -> bool,
    named: impl Fn(Ljos) -> bool,
) -> String {
    let mut line = crate::JAVASCRIPT.to_owned();
    for ljos in Ljos::ALL {
        if ljos == carrier || (!own(ljos) && named(ljos) && ljos.shares().is_some()) {
            line.push_str("; import ");
            line.push_str(ljos.uri());
        }
    }
    line
}

#[cfg(test)]
mod tests;
