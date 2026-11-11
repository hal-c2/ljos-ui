use super::*;

fn rows(uris: &[&str]) -> Vec<&'static Row> {
    uris.iter()
        .map(|uri| row(uri).unwrap_or_else(|| panic!("no row of {uri}")))
        .collect()
}

#[test]
fn each_qt_module_has_one_row() {
    for (i, row) in ROWS.iter().enumerate() {
        assert!(
            ROWS[..i].iter().all(|other| other.uri != row.uri),
            "{} has two rows",
            row.uri
        );
    }
    // The modules of one kind lower to one Ljós module.
    for row in ROWS {
        assert_eq!(row.qt.lowers_to(), row.lowers_to, "{}", row.uri);
    }
}

/// A module whose import decides what a name is, is looked up, and one
/// whose does not is not.
#[test]
fn what_is_looked_up_names_types() {
    for row in ROWS {
        let looked_up = LOOKED_UP.iter().any(|group| group.contains(&row.qt))
            // QtQml's types are looked up among QtQuick's.
            || row.qt == Qt::Qml;
        assert_eq!(looked_up, row.names_types, "{}", row.uri);
    }
    // A module whose types are looked up is described.
    for row in ROWS.iter().filter(|row| row.names_types) {
        assert_ne!(row.described, Described::No, "{}", row.uri);
    }
}

#[test]
fn each_ljos_module_takes_one_line() {
    use Line::{Emptied, Takes};
    let lines = |uris: &[&str]| lines(&rows(uris));
    assert_eq!(
        lines(&["QtQuick", "QtQuick.Shapes", "QtQuick.Window"]),
        [Takes(Ljos::Ui), Emptied, Emptied]
    );
    // The last of an import that lowers to a module takes its line, and
    // every other is emptied: none is left as Qt's.
    assert_eq!(
        lines(&[
            "QtQuick",
            "QtQml",
            "QtQuick",
            "QtQml.Models",
            "QtQml.Models"
        ]),
        [
            Emptied,
            Takes(Ljos::Core),
            Takes(Ljos::Ui),
            Emptied,
            Takes(Ljos::Models)
        ]
    );
    // Of the controls and the dialogs, the first.
    assert_eq!(
        lines(&[
            "QtQuick.Dialogs",
            "QtQuick.Controls",
            "QtQuick.Controls.Basic"
        ]),
        [Takes(Ljos::Controls), Emptied, Emptied]
    );
    assert_eq!(
        lines(&["QtQuick.Layouts", "QtQuick.Layouts"]),
        [Emptied, Takes(Ljos::Layout)]
    );
}

#[test]
fn the_shared_line_names_what_has_no_line_of_its_own() {
    let none = |_: Ljos| false;
    let all = |_: Ljos| true;
    assert_eq!(
        shared_line(Ljos::Ui, |l| l == Ljos::Ui, none),
        "pragma Scripts: JavaScript; import Ljos.Ui"
    );
    assert_eq!(
        shared_line(Ljos::Ui, |l| l == Ljos::Ui, all),
        "pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Core; \
         import Ljos.Models; import Ljos.Layout"
    );
    // A file of QtQml alone names Ljos.Ui first, as every other file does.
    assert_eq!(
        shared_line(Ljos::Core, |l| l == Ljos::Core, |l| l == Ljos::Ui),
        "pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Core"
    );
    // A module with a line of its own is not named again, and Ljós's
    // controls are only ever on theirs.
    assert_eq!(
        shared_line(
            Ljos::Ui,
            |l| matches!(l, Ljos::Ui | Ljos::Core | Ljos::Controls),
            all
        ),
        "pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Models; import Ljos.Layout"
    );
}

#[test]
fn a_module_with_no_line_shares_one() {
    let Ok(mut modules) = Modules::new() else {
        return;
    };
    modules.imported(Ljos::Core, 2);
    modules.imported(Ljos::Controls, 3);
    modules.imported(Ljos::Controls, 5);
    modules.share_lines();
    assert_eq!(modules.line(Ljos::Core), Some(2));
    // QtQml's line is Ljos.Ui's, and so Ljos.Models' and Ljos.Layout's.
    assert_eq!(modules.line(Ljos::Ui), Some(2));
    assert_eq!(modules.line(Ljos::Models), Some(2));
    assert_eq!(modules.line(Ljos::Layout), Some(2));
    // The first of the controls' imports.
    assert_eq!(modules.line(Ljos::Controls), Some(3));

    let Ok(mut modules) = Modules::new() else {
        return;
    };
    modules.imported(Ljos::Ui, 0);
    modules.imported(Ljos::Ui, 4);
    modules.share_lines();
    assert_eq!(modules.line(Ljos::Ui), Some(4));
    assert_eq!(modules.line(Ljos::Core), Some(4));
    assert_eq!(modules.line(Ljos::Controls), None);
}

/// [`Modules::qt_type`] is asked of several modules in one order: it is the
/// same type in whichever of them has it, so the order says nothing.
#[test]
fn no_two_qt_modules_have_a_name_for_two_types() {
    let uris = ROWS.iter().filter(|row| row.names_types).map(|row| row.uri);
    let described: Vec<(&str, &'static Module)> = uris
        .filter_map(|uri| Some((uri, qt_types::module(uri)?)))
        .collect();
    for (i, (a, of_a)) in described.iter().enumerate() {
        for (b, of_b) in &described[i + 1..] {
            let names = [a, b].into_iter().flat_map(|uri| {
                qt_types::native_types(uri)
                    .unwrap_or_default()
                    .into_iter()
                    .chain(qt_types::written_types(uri).unwrap_or_default())
            });
            for name in names {
                if let (Some(x), Some(y)) = (of_a.type_named(name), of_b.type_named(name)) {
                    assert!(
                        std::ptr::eq(x, y),
                        "{name} is one type of {a} and another of {b}"
                    );
                }
            }
        }
    }
}
