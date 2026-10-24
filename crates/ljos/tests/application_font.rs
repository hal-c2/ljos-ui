//! A plain text item made before the application font changes follows it.

fn compile(main: &str) -> ljos::MapSource {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Main.qml"), main).unwrap();
    ljos::dev::compile_dir_with(dir.path(), &ljos::engine()).unwrap()
}

#[test]
fn a_plain_text_made_earlier_takes_the_application_font() {
    let engine = ljos::engine();
    let source = compile(
        "import QtQuick\nimport QtQuick.Controls\nItem {\n\
         Text { id: text }\n\
         TextInput { id: input }\n\
         TextEdit { id: edit }\n\
         Label { id: label }\n\
         Text { id: named; font.family: \"Monospace\" }\n\
         property string text: text.font.family\n\
         property string input: input.font.family\n\
         property string edit: edit.font.family\n\
         property string label: label.font.family\n\
         property string named: named.font.family\n}\n",
    );
    let root = ljos::create_root(&engine, source, "Main").unwrap();
    let font = ljos::item::Font {
        family: "Literata".into(),
        ..(*ljos::application_font(&engine)).clone()
    };
    ljos::set_application_font(&engine, Some(&font));
    let family = |name: &str| {
        engine
            .to_js_string(&engine.read_by_name(root, name))
            .to_string()
    };
    assert_eq!(family("label"), "Literata");
    assert_eq!(family("text"), "Literata");
    assert_eq!(family("input"), "Literata");
    assert_eq!(family("edit"), "Literata");
    assert_eq!(family("named"), "Monospace");
    ljos::set_application_font(&engine, None);
    assert_eq!(
        family("text"),
        ljos::item::Font::process_default().family.to_string()
    );
}
