//! Compares [`PluralForms`] with what Qt's own tools do (ARCHITECTURE.md §15).
//!
//! The tools run as black boxes: `lupdate` says how many numerus forms a
//! language has by writing a `.ts` file for it, and `lrelease` plus `qml6`
//! say which form a number selects by translating with a catalogue whose
//! forms are numbered. Nothing they write is kept. Without Qt 6 on the
//! machine the tests pass without checking anything, and say so.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    path::Path,
    process::{Command, Stdio},
    thread,
};

use ljos_i18n::PluralForms;

/// Ordinary numbers, the hundreds and thousands CLDR rules repeat over, and
/// the whole millions some of them single out.
fn numbers() -> Vec<u32> {
    let mut out: Vec<u32> = (0..=220).collect();
    out.extend([
        300, 311, 400, 1000, 1001, 1002, 1003, 1005, 1011, 1021, 2000, 5000, 10_000, 100_000,
        1_000_000, 1_000_001, 1_000_002, 1_000_011, 2_000_000, 1_234_567,
    ]);
    out
}

struct Qt {
    lupdate: String,
    lrelease: String,
    qml: String,
}

/// The first of `names` that runs. Distributions that also ship Qt 5 put
/// its tools first on the path, so the Qt 6 names come first.
fn tool(var: &str, names: &[&str], probe: &str) -> Option<String> {
    let runs = |name: &str| {
        Command::new(name)
            .arg(probe)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    };
    if let Ok(name) = std::env::var(var) {
        return runs(&name).then_some(name);
    }
    names.iter().find(|n| runs(n)).map(|n| (*n).to_owned())
}

fn qt() -> Option<Qt> {
    let found = (|| {
        Some(Qt {
            lupdate: tool(
                "LUPDATE",
                &["lupdate6", "lupdate-qt6", "/usr/lib/qt6/bin/lupdate"],
                "-version",
            )?,
            lrelease: tool(
                "LRELEASE",
                &["lrelease6", "lrelease-qt6", "/usr/lib/qt6/bin/lrelease"],
                "-version",
            )?,
            qml: tool("QML6", &["qml6"], "--version")?,
        })
    })();
    if found.is_none() {
        eprintln!("Qt 6 (lupdate, lrelease, qml6) is not installed: nothing was compared");
    }
    found
}

const SOURCE: &str = "import QtQuick\nItem { property string t: qsTr(\"%n\", \"\", 1) }\n";

/// What Qt does for one language.
struct Observed {
    /// How many numerus forms `lupdate` writes.
    count: usize,
    /// Which form each of [`numbers`] selects (`None` when Qt shows the
    /// source text). Empty when `qml6` did not take the code for a language
    /// of its own and so never loaded the catalogue.
    index: Vec<Option<usize>>,
}

fn observe(qt: &Qt, dir: &Path, languages: &[String]) -> BTreeMap<String, Observed> {
    fs::create_dir_all(dir.join("i18n")).unwrap();
    fs::write(dir.join("Scenario.qml"), SOURCE).unwrap();
    // One language per tool run; the runs are independent, so they share the cores.
    let workers = thread::available_parallelism().map_or(4, usize::from);
    let counts: BTreeMap<String, usize> = thread::scope(|scope| {
        let handles: Vec<_> = languages
            .chunks(languages.len().div_ceil(workers).max(1))
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .map(|l| (l.clone(), release(qt, dir, l)))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    });

    let driver = format!(
        "import QtQuick\nItem {{\n  Component.onCompleted: {{\n    var languages = {languages:?}\n    var numbers = {numbers:?}\n    \
         var c = Qt.createComponent(\"Scenario.qml\")\n    var o = c.createObject(this)\n    \
         for (var l = 0; l < languages.length; ++l) {{\n      Qt.uiLanguage = languages[l]\n      var row = []\n      \
         for (var i = 0; i < numbers.length; ++i) row.push(o.say(numbers[i]))\n      \
         console.log(\"@@\" + languages[l] + \" \" + o.language() + \" \" + row.join(\",\"))\n    }}\n    Qt.quit()\n  }}\n}}\n",
        numbers = numbers()
    );
    fs::write(
        dir.join("Scenario.qml"),
        "import QtQuick\nItem {\n  function say(n) { return qsTr(\"%n\", \"\", n) }\n  function language() { return qsTr(\"language\") }\n}\n",
    )
    .unwrap();
    fs::write(dir.join("main.qml"), driver).unwrap();
    let out = Command::new(&qt.qml)
        .arg(dir.join("main.qml"))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .env("QT_QPA_PLATFORM", "offscreen")
        .env("QT_MESSAGE_PATTERN", "%{message}")
        .env("LANG", "en_US.UTF-8")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    let mut observed = BTreeMap::new();
    for line in stderr.lines() {
        let Some(rest) = line.strip_prefix("@@") else {
            continue;
        };
        let mut parts = rest.splitn(3, ' ');
        let (language, loaded, row) = (
            parts.next().unwrap(),
            parts.next().unwrap(),
            parts.next().unwrap(),
        );
        let index = if loaded == language {
            row.split(',')
                .map(|cell| cell.strip_prefix('f').and_then(|i| i.parse().ok()))
                .collect()
        } else {
            Vec::new()
        };
        observed.insert(
            language.to_owned(),
            Observed {
                count: counts[language],
                index,
            },
        );
    }
    assert_eq!(observed.len(), languages.len(), "qml6 said:\n{stderr}");
    observed
}

/// Asks `lupdate` for the `.ts` of one language, counts its numerus forms,
/// and releases the same file with each form saying which one it is.
fn release(qt: &Qt, dir: &Path, language: &str) -> usize {
    let ts = dir.join(format!("{language}.ts"));
    let out = Command::new(&qt.lupdate)
        .arg(dir.join("Scenario.qml"))
        .arg("-ts")
        .arg(&ts)
        .args(["-target-language", language])
        .output()
        .unwrap();
    assert!(out.status.success(), "lupdate failed for {language}");
    let written = fs::read_to_string(&ts).unwrap();
    let count = written.matches("<numerusform>").count();
    assert!(count > 0, "lupdate wrote no numerus form for {language}");
    let mut forms = String::new();
    for i in 0..count {
        write!(forms, "<numerusform>f{i}</numerusform>").unwrap();
    }
    let filled = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE TS>\n\
         <TS version=\"2.1\" language=\"{language}\">\n<context><name>Scenario</name>\n\
         <message numerus=\"yes\"><source>%n</source><translation>{forms}</translation></message>\n\
         <message><source>language</source><translation>{language}</translation></message>\n\
         </context></TS>\n"
    );
    fs::write(&ts, filled).unwrap();
    let out = Command::new(&qt.lrelease)
        .arg(&ts)
        .arg("-qm")
        .arg(dir.join(format!("i18n/qml_{language}.qm")))
        .output()
        .unwrap();
    assert!(out.status.success(), "lrelease failed for {language}");
    count
}

/// One line per language that differs, empty when all agree.
fn differences(observed: &BTreeMap<String, Observed>) -> Vec<String> {
    let numbers = numbers();
    let mut out = Vec::new();
    for (language, qt) in observed {
        // A three-letter code `lupdate` gives one form and `qml6` loads no
        // catalogue for is not a language to Qt (`arb`, `ekk`), so no `.ts`
        // file exists to agree with; Ljós reads it as the CLDR alias it is.
        if language.len() == 3 && qt.count == 1 && qt.index.is_empty() {
            continue;
        }
        let ours = PluralForms::for_language(language);
        let same_count = ours.count() == qt.count;
        let wrong = numbers
            .iter()
            .zip(&qt.index)
            .filter(|(n, i)| **i != Some(ours.index(**n)))
            .count();
        if same_count && wrong == 0 {
            continue;
        }
        // Enough of Qt's answers to read its rule off: 0 to 30, then the
        // numbers where a rule about hundreds or millions would show.
        let row = |pick: &dyn Fn(u32) -> bool| -> String {
            numbers
                .iter()
                .zip(&qt.index)
                .filter(|(n, _)| pick(**n))
                .map(|(_, i)| i.map_or_else(|| "-".to_owned(), |i| i.to_string()))
                .collect()
        };
        out.push(format!(
            "{language}: Qt writes {} forms, Ljós {}; {wrong} numbers differ. Qt for 0..=30: {}, 100..=130: {}, \
             1000 1001 1002 1011 1000000 1000001: {}",
            qt.count,
            ours.count(),
            row(&|n| n <= 30),
            row(&|n| (100..=130).contains(&n)),
            row(&|n| [1000, 1001, 1002, 1011, 1_000_000, 1_000_001].contains(&n)),
        ));
    }
    out
}

/// Every code that could name a language with plural forms on either side:
/// all two-letter codes, the three-letter codes Ljós has more than one form
/// for, and the three-letter codes `lupdate` has a rule of its own for.
fn candidates() -> Vec<String> {
    let letters = || 'a'..='z';
    let mut out: Vec<String> = letters()
        .flat_map(|a| letters().map(move |b| format!("{a}{b}")))
        .collect();
    out.extend(
        letters()
            .flat_map(|a| letters().flat_map(move |b| letters().map(move |c| format!("{a}{b}{c}"))))
            .filter(|code| PluralForms::for_language(code).count() > 1),
    );
    out.extend(["fil", "fur", "nso", "pt_BR", "pt_PT", "zh_TW"].map(str::to_owned));
    out.sort();
    out.dedup();
    out
}

#[test]
fn every_language_has_the_numerus_forms_qt_gives_it() {
    let Some(qt) = qt() else { return };
    let dir = tempfile::tempdir().unwrap();
    let languages = candidates();
    let observed = observe(&qt, dir.path(), &languages);
    let checked = observed.values().filter(|o| !o.index.is_empty()).count();
    assert!(checked > 100, "qml6 loaded only {checked} catalogues");
    let diff = differences(&observed);
    assert!(
        diff.is_empty(),
        "plural forms differ from Qt in {} of {} languages:\n{}",
        diff.len(),
        languages.len(),
        diff.join("\n")
    );
}
