//! The `.ts` reader and writer on a hand-written Icelandic catalogue
//! (ARCHITECTURE.md §15), and against Qt's `lconvert` run as a black box
//! when it is installed.

use std::{
    fs,
    process::{Command, Stdio},
};

use ljos_i18n::{
    I18n, Ljl, Messages,
    ts::{Extracted, TsFile, TsLocation, TsState},
};

const FIXTURE: &str = include_str!("fixtures/app_is.ts");

fn fixture() -> TsFile {
    TsFile::read(FIXTURE).unwrap()
}

#[test]
fn a_file_is_written_back_byte_for_byte() {
    assert_eq!(fixture().write(), FIXTURE);
}

#[test]
fn every_part_of_a_message_is_read() {
    let file = fixture();
    assert_eq!(file.version, "2.1");
    assert_eq!(file.language.as_deref(), Some("is_IS"));
    assert_eq!(file.source_language.as_deref(), Some("en"));
    let names: Vec<_> = file.contexts.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["", "Main", "util"]);

    let main = &file.contexts[1];
    let open = &main.messages[0];
    assert_eq!(open.locations.len(), 2);
    assert_eq!(open.locations[1].filename.as_deref(), Some("Toolbar.qml"));
    assert_eq!(open.locations[1].line.as_deref(), Some("12"));
    assert_eq!(open.state, TsState::Finished);

    let adjective = &main.messages[1];
    assert_eq!(adjective.comment.as_deref(), Some("adjective"));
    assert_eq!(
        adjective.extra_comment.as_deref(),
        Some("Shown beside a door that is not closed")
    );
    assert_eq!(
        adjective.translator_comment.as_deref(),
        Some("Karlkyn, eintala")
    );
    assert_eq!(adjective.extras, [("Scope".to_owned(), "door".to_owned())]);

    let files = &main.messages[2];
    assert!(files.numerus);
    assert_eq!(files.translation, ["%n skrá", "%n skrár"]);

    let by_source = |source: &str| {
        main.messages
            .iter()
            .find(|m| m.source.starts_with(source))
            .unwrap()
    };
    assert_eq!(
        by_source("Save").source,
        "Save & close <b>\"now\"</b> it's late"
    );
    assert_eq!(by_source("Two").source, "Two\nlines\tand a tab");
    assert_eq!(by_source("bell").source, "bell\u{7}return\r");
    assert_eq!(by_source("Draft").old_source.as_deref(), Some("Drafts"));
    assert_eq!(by_source("Draft").state, TsState::Unfinished);
    assert_eq!(by_source("Gone").state, TsState::Vanished);
    assert_eq!(by_source("Stale").state, TsState::Obsolete);
    assert_eq!(by_source("Stale").old_comment.as_deref(), Some("old"));
    assert_eq!(
        by_source("Stale").user_data.as_deref(),
        Some("kept for the record")
    );

    let ids = &file.contexts[0];
    assert_eq!(ids.messages[0].id.as_deref(), Some("app-title"));
}

#[test]
fn a_file_written_other_ways_reads_the_same() {
    let loose = "<?xml version='1.0'?>\n<!-- by hand -->\n<TS version=\"2.0\" language=\"is\">\
        <context><name>Main</name>\
        <message><location filename=\"a.qml\" line=\"+3\"></location>\
        <source><![CDATA[a < b]]> &#38; &#x263A;</source>\
        <translation type=\"unfinished\"/></message>\
        <message><source>No translation element</source></message>\
        <unknown><deep>ignored</deep></unknown>\
        </context></TS>";
    let file = TsFile::read(loose).unwrap();
    let messages = &file.contexts[0].messages;
    assert_eq!(messages[0].source, "a < b & \u{263a}");
    assert_eq!(messages[0].locations[0].line.as_deref(), Some("+3"));
    assert_eq!(messages[0].translation, [""]);
    assert_eq!(messages[1].state, TsState::Unfinished);
    // Once written in the usual layout, it is stable.
    let written = file.write();
    assert_eq!(TsFile::read(&written).unwrap().write(), written);
}

#[test]
fn what_is_not_a_translation_file_says_so() {
    assert!(TsFile::read("<html></html>").is_err());
    assert!(TsFile::read("").is_err());
    let cut = TsFile::read("<TS>\n<context>\n<name>Main</name>\n<message><source>x").unwrap_err();
    assert!(cut.to_string().contains("line "), "{cut}");
    let bad = TsFile::read("<TS><context><name>&nope;</name></context></TS>").unwrap_err();
    assert!(bad.to_string().contains("nope"), "{bad}");
}

fn icelandic() -> I18n {
    let mut i18n = I18n::new();
    i18n.install(Box::new(fixture().catalog()));
    i18n.set_ui_language("is");
    i18n
}

#[test]
fn the_catalogue_translates_what_is_translated() {
    let i = icelandic();
    assert_eq!(i.translate("Main", "Open", "", None), "Opna");
    assert_eq!(i.translate("Main", "Open", "adjective", None), "Opinn");
    assert_eq!(i.translate("Main", "%n file(s)", "", Some(1)), "1 skrá");
    assert_eq!(i.translate("Main", "%n file(s)", "", Some(5)), "5 skrár");
    assert_eq!(i.translate("Main", "%n file(s)", "", Some(31)), "31 skrá");
    assert_eq!(i.translate("util", "From a script", "", None), "Úr skriftu");
    assert_eq!(i.translate_id("app-title", None), "Ljós");
    assert_eq!(i.translate_id("unread-count", Some(2)), "2 ólesin");
    // An unfinished translation with a text is used.
    assert_eq!(i.translate("Main", "Draft", "", None), "Drög");
}

#[test]
fn what_is_not_translated_stays_in_the_source_language() {
    let i = icelandic();
    assert_eq!(i.translate("Main", "Not yet", "", None), "Not yet");
    assert_eq!(i.translate("Main", "Gone", "", None), "Gone");
    assert_eq!(i.translate("Main", "Stale", "", None), "Stale");
    // One of its two forms is missing, so neither is used.
    assert_eq!(
        i.translate("Main", "%n folder(s)", "", Some(1)),
        "1 folder(s)"
    );
    // A text id is not found as a text.
    assert_eq!(i.translate("", "Light", "", None), "Light");
}

#[test]
fn the_runtime_format_holds_the_same_translations() {
    let catalog = fixture().catalog();
    let ljl = Ljl::open(ljos_i18n::ljl::write(&catalog).unwrap()).unwrap();
    assert_eq!(ljl.language(), "is_IS");
    assert_eq!(ljl.len(), catalog.len());
    let mut i18n = I18n::new();
    i18n.install(Box::new(ljl));
    i18n.set_ui_language("is_IS");
    let reference = icelandic();
    for (context, source, comment, n) in [
        ("Main", "Open", "", None),
        ("Main", "Open", "adjective", None),
        ("Main", "%n file(s)", "", Some(21)),
        ("Main", "%n file(s)", "", Some(12)),
        ("Main", "Not yet", "", None),
        ("util", "From a script", "", None),
    ] {
        assert_eq!(
            i18n.translate(context, source, comment, n),
            reference.translate(context, source, comment, n)
        );
    }
    assert_eq!(i18n.translate_id("unread-count", Some(1)), "1 ólesið");
}

fn found(context: &str, source: &str, line: u32) -> Extracted {
    Extracted {
        context: context.to_owned(),
        source: source.to_owned(),
        location: TsLocation {
            filename: Some(format!("{context}.qml")),
            line: Some(line.to_string()),
        },
        ..Extracted::default()
    }
}

#[test]
fn merging_keeps_what_translators_did() {
    let mut file = fixture();
    let report = file.merge(&[
        found("Main", "Open", 3),
        found("Main", "Open", 40),
        found("Main", "Brand new", 5),
        Extracted {
            numerus: true,
            ..found("Main", "%n file(s)", 6)
        },
        Extracted {
            numerus: true,
            ..found("Dialog", "%n item(s)", 2)
        },
        Extracted {
            id: Some("app-title".to_owned()),
            ..found("", "Light", 1)
        },
    ]);
    assert_eq!(report.found, 5);
    assert_eq!(report.new, 2);

    let names: Vec<_> = file.contexts.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["", "Dialog", "Main", "util"]);
    let main = &file.contexts[2];
    let get = |source: &str, comment: Option<&str>| {
        main.messages
            .iter()
            .find(|m| m.source == source && m.comment.as_deref() == comment)
    };

    let open = get("Open", None).unwrap();
    assert_eq!(open.translation, ["Opna"]);
    assert_eq!(open.state, TsState::Finished);
    let lines: Vec<_> = open
        .locations
        .iter()
        .filter_map(|l| l.line.as_deref())
        .collect();
    assert_eq!(lines, ["3", "40"]);

    let new = get("Brand new", None).unwrap();
    assert_eq!(new.state, TsState::Unfinished);
    assert_eq!(new.translation, [""]);
    assert_eq!(
        get("%n file(s)", None).unwrap().translation,
        ["%n skrá", "%n skrár"]
    );

    // No longer in the sources: finished ones vanish, drafts become
    // obsolete, untranslated ones go, and what was obsolete stays so.
    let adjective = get("Open", Some("adjective")).unwrap();
    assert_eq!(adjective.state, TsState::Vanished);
    assert!(adjective.locations.is_empty());
    assert_eq!(
        adjective.translator_comment.as_deref(),
        Some("Karlkyn, eintala")
    );
    assert_eq!(get("Draft", None).unwrap().state, TsState::Obsolete);
    assert_eq!(get("Not yet", None), None);
    assert_eq!(get("Gone", None).unwrap().state, TsState::Vanished);
    assert_eq!(get("Stale", None).unwrap().state, TsState::Obsolete);

    // A new plural message gets one empty form per form of the language.
    let dialog = &file.contexts[1];
    assert_eq!(dialog.messages[0].translation, ["", ""]);
    assert!(dialog.messages[0].numerus);

    // The vanished script message keeps its context alive.
    assert_eq!(file.contexts[3].messages[0].state, TsState::Vanished);
    assert_eq!(file.contexts[0].messages.len(), 2);
    assert_eq!(file.contexts[0].messages[0].translation, ["Ljós"]);
    assert_eq!(file.contexts[0].messages[1].state, TsState::Vanished);
}

#[test]
fn merging_the_same_sources_twice_changes_nothing() {
    let sources = [found("Main", "Open", 3), found("Main", "Brand new", 5)];
    let mut file = fixture();
    file.merge(&sources);
    let once = file.write();
    let report = file.merge(&sources);
    assert_eq!(file.write(), once);
    assert_eq!(report.new, 0);
    assert_eq!(report.dropped, 0);
}

#[test]
fn a_message_that_comes_back_takes_its_translation_up_again() {
    let mut file = fixture();
    let report = file.merge(&[found("Main", "Gone", 9), found("Main", "Stale", 10)]);
    assert_eq!(report.new, 2);
    let main = file.contexts.iter().find(|c| c.name == "Main").unwrap();
    let by_source = |source: &str| {
        let found: Vec<_> = main
            .messages
            .iter()
            .filter(|m| m.source == source)
            .collect();
        assert_eq!(found.len(), 1, "{source}");
        found[0]
    };
    assert_eq!(by_source("Gone").state, TsState::Finished);
    assert_eq!(by_source("Gone").translation, ["Farið"]);
    assert_eq!(by_source("Stale").state, TsState::Unfinished);
    assert_eq!(by_source("Stale").translation, ["Úrelt"]);
    assert_eq!(
        by_source("Stale").user_data.as_deref(),
        Some("kept for the record")
    );
}

#[test]
fn a_context_comment_is_kept() {
    let text = "<TS version=\"2.1\"><context><name>Main</name><comment>The main window</comment>\
        <message><source>Open</source><translation>Opna</translation></message></context></TS>";
    let mut file = TsFile::read(text).unwrap();
    assert_eq!(file.contexts[0].comment.as_deref(), Some("The main window"));
    file.merge(&[found("Main", "Open", 1)]);
    assert_eq!(file.contexts[0].comment.as_deref(), Some("The main window"));
    assert!(file.write().contains("<comment>The main window</comment>"));
}

/// Qt's own converter, if this machine has it: reading our output and
/// writing it again must give our output.
#[test]
fn qt_writes_the_file_the_same_way() {
    let candidates = ["lconvert6", "lconvert-qt6", "/usr/lib/qt6/bin/lconvert"];
    let tool = std::env::var("LCONVERT").ok().or_else(|| {
        candidates
            .iter()
            .find(|name| {
                Command::new(name)
                    .arg("-help")
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .is_ok()
            })
            .map(|name| (*name).to_owned())
    });
    let Some(tool) = tool else {
        eprintln!("lconvert (Qt 6) is not installed: the .ts layout was not compared with Qt's");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let ours = dir.path().join("ours.ts");
    let theirs = dir.path().join("theirs.ts");
    let mut file = fixture();
    file.merge(&[
        found("Main", "Open", 3),
        found("Main", "Brand <new> & \"shiny\"", 5),
    ]);
    let written = file.write();
    fs::write(&ours, &written).unwrap();
    let out = Command::new(&tool)
        .arg("-i")
        .arg(&ours)
        .arg("-o")
        .arg(&theirs)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(fs::read_to_string(&theirs).unwrap(), written);
}
