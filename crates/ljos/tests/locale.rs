//! What Rust writes with `ljos::locale` is what QML writes with
//! `Qt.locale()`, for the application's locale and a named one, and a
//! change of locale reaches Rust.

use std::cell::RefCell;
use std::rc::Rc;

use ljos::locale::{DataSizeFormat, DateTime, FormatType, Locale, SystemLocale};
use ljos::prelude::*;

const MAIN: &str = r#"
import QtQuick
Item {
    property string name: Qt.locale().name
    property string number: (1234567.891).toLocaleString(Qt.locale(), "f", 2)
    property string size: Qt.locale().formattedDataSize(1234567)
    property string si: Qt.locale().formattedDataSize(1234567, 1, Locale.DataSizeSIFormat)
    readonly property var day: new Date(2024, 1, 29, 13, 5, 9)
    property string shortDate: day.toLocaleDateString(Qt.locale(), Locale.ShortFormat)
    property string longDate: day.toLocaleDateString(Qt.locale(), Locale.LongFormat)
    property string shortTime: day.toLocaleTimeString(Qt.locale(), Locale.ShortFormat)
    property string both: day.toLocaleString(Qt.locale(), Locale.ShortFormat)
    property string pattern: day.toLocaleString(Qt.locale(), "dddd d. MMMM yyyy, hh:mm")
    property string named: (1234.5).toLocaleString(Qt.locale("fr_FR"), "f", 1)
}
"#;

fn load(engine: &Engine) -> ObjectId {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Main.qml"), MAIN).unwrap();
    let source = ljos::dev::compile_dir_with(dir.path(), engine).unwrap();
    ljos::create_root(engine, source, "Main").unwrap()
}

fn text(engine: &Engine, root: ObjectId, name: &str) -> String {
    engine.read_by_name(root, name).to_js_string().to_string()
}

/// The `Date` above, in the zone the scripts run in.
fn day(engine: &Engine) -> DateTime {
    let offset = engine
        .eval_js("-new Date(2024, 1, 29, 13, 5, 9).getTimezoneOffset() * 60")
        .map(|v| v.to_number() as i32)
        .unwrap_or_default();
    DateTime {
        year: 2024,
        month: 2,
        day: 29,
        hour: 13,
        minute: 5,
        second: 9,
        millisecond: 0,
        offset,
    }
}

#[test]
fn rust_writes_what_qml_writes() {
    for name in ["en_US", "de_DE", "is_IS", "ar_EG", "C"] {
        let engine = ljos::engine();
        ljos::locale::set_locale(&engine, name);
        let root = load(&engine);
        let locale = Locale::of(&engine);
        let day = day(&engine);
        assert_eq!(locale.name(), text(&engine, root, "name"));
        assert_eq!(
            locale.format_number(1234567.891, 'f', 2),
            text(&engine, root, "number"),
            "{name}"
        );
        assert_eq!(
            locale.format_data_size(1234567, 2, DataSizeFormat::Iec),
            text(&engine, root, "size")
        );
        assert_eq!(
            locale.format_data_size(1234567, 1, DataSizeFormat::Si),
            text(&engine, root, "si")
        );
        assert_eq!(
            locale.format_date(&day, FormatType::Short),
            text(&engine, root, "shortDate"),
            "{name}"
        );
        assert_eq!(
            locale.format_date(&day, FormatType::Long),
            text(&engine, root, "longDate")
        );
        assert_eq!(
            locale.format_time(&day, FormatType::Short),
            text(&engine, root, "shortTime")
        );
        assert_eq!(
            locale.format_date_time(&day, FormatType::Short),
            text(&engine, root, "both")
        );
        assert_eq!(
            locale.format_pattern(&day, "dddd d. MMMM yyyy, hh:mm"),
            text(&engine, root, "pattern")
        );
        assert_eq!(
            Locale::named("fr_FR").format_number(1234.5, 'f', 1),
            text(&engine, root, "named")
        );
    }
}

#[test]
fn a_few_known_answers() {
    let de = Locale::named("de");
    assert_eq!(de.name(), "de_DE");
    assert_eq!(de.format_integer(1234567), "1.234.567");
    assert_eq!(de.date_format(FormatType::Short), "dd.MM.yy");
    let moon = DateTime::from_unix_ms(-14_159_040_000, 0);
    assert_eq!(
        de.format_pattern(&moon, "yyyy-MM-dd HH:mm"),
        "1969-07-21 02:56"
    );
    let en = Locale::named("en_US");
    assert_eq!(
        en.format_data_size(1023, 2, DataSizeFormat::Iec),
        "1,023 bytes"
    );
    assert_eq!(
        en.format_data_size(1536, 0, DataSizeFormat::Traditional),
        "2 kB"
    );
}

#[test]
fn a_change_of_locale_reaches_rust() {
    let engine = ljos::engine();
    let _root = load(&engine);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&seen);
    let watch = ljos::locale::on_change(&engine, move |_, l| {
        sink.borrow_mut().push(l.name().to_owned());
    })
    .unwrap();
    assert!(
        seen.borrow().is_empty(),
        "not called for the locale it starts in"
    );
    ljos::locale::set_locale(&engine, "de_DE");
    ljos::locale::set_locale(&engine, "de_DE");
    engine.eval_js("Qt.uiLanguage = 'fr'").unwrap();
    ljos::locale::set_locale(&engine, "is");
    assert_eq!(*seen.borrow(), ["de_DE", "is_IS"]);
    watch.stop(&engine);
    ljos::locale::set_locale(&engine, "fr");
    assert_eq!(seen.borrow().len(), 2);
}

#[test]
fn the_system_locale_takes_each_category_from_its_own_variable() {
    let engine = ljos::engine();
    let root = load(&engine);
    let env = [
        ("LANG", "en_US.UTF-8"),
        ("LC_MESSAGES", "de_DE.UTF-8"),
        ("LC_NUMERIC", "fr_FR.UTF-8"),
    ];
    let system = SystemLocale::from_vars(|k| {
        env.iter()
            .find(|(n, _)| *n == k)
            .map(|(_, v)| (*v).to_owned())
    });
    engine.use_system_locale_from(&system);
    assert_eq!(text(&engine, root, "name"), "de_DE");
    assert_eq!(engine.ui_language(), "de");
    assert_eq!(text(&engine, root, "number"), "1\u{202f}234\u{202f}567,89");
    assert_eq!(Locale::of(&engine).format_number(1.5, 'f', 1), "1,5");
    assert_eq!(text(&engine, root, "shortDate"), "2/29/24");
}

#[test]
fn an_application_chooses_its_locale_over_the_systems() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Main.qml"), MAIN).unwrap();
    let (engine, root) = ljos::App::new("Locale")
        .qml_dir(dir.path(), "Main")
        .unwrap()
        .locale("is_IS")
        .ui_language("is")
        .build()
        .unwrap();
    assert_eq!(text(&engine, root, "name"), "is_IS");
    assert_eq!(engine.ui_language(), "is");
    // A build that chooses nothing is the same on every machine.
    let (engine, root) = ljos::App::new("Locale")
        .qml_dir(dir.path(), "Main")
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(text(&engine, root, "name"), "en_US");
}
