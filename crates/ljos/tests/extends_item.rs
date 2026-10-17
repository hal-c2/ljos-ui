//! An application's visual type that extends the toolkit's `Item`: the
//! build checks the members of its own group as it does the type's, and
//! leaves the toolkit's groups (`anchors`) to the runtime as it does on
//! `Item` itself.

use ljos::prelude::*;

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Panel", extends = Item, default = "data")]
pub struct Panel {
    #[qml(base)]
    pub item: Item,
    #[qml(notify)]
    pub title: Atom,
    #[qml(group)]
    pub padding: Padding,
}

/// `Panel`'s `padding` group.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "PanelPadding", uncreatable)]
pub struct Padding {
    #[qml(notify)]
    pub top: f64,
}

fn register(engine: &Engine) {
    engine.register_module(ljos::module!("Example" 1.0 => [Panel]));
}

const MAIN: &str = r#"
import QtQuick
import Example
Item {
    id: root
    width: 200
    height: 100
    property real seen: panel.width + panel.padding.top
    Panel {
        id: panel
        anchors.fill: parent
        anchors.margins: 10
        padding.top: 4
        title: "p"
    }
}
"#;

/// `main` as `Main.qml`, compiled against the type dump file the
/// application's test writes.
fn compile(main: &str) -> Result<Vec<u8>, String> {
    let dir = tempfile::tempdir().unwrap();
    let qml = dir.path().join("qml");
    std::fs::create_dir(&qml).unwrap();
    std::fs::write(qml.join("Main.qml"), main).unwrap();
    let types = dir.path().join("ljostypes.json");
    std::fs::write(&types, ljos::types::dump(register).to_json()).unwrap();
    let pack = dir.path().join("qml.pack");
    ljos::build::compile_dir_with_types(&qml, &types, &pack)?;
    Ok(std::fs::read(pack).unwrap())
}

#[test]
fn a_type_extending_item_takes_anchors_and_its_own_group() {
    let pack = compile(MAIN).unwrap();
    let (engine, root) = ljos::App::new("Extends Item")
        .register(register)
        .source(ljos::MapSource::from_pack(&pack).unwrap())
        .root("Main")
        .build()
        .unwrap();
    // Anchors are laid out by a scene, which drawing the root makes.
    ljos::render::Picture::new(200, 100)
        .png(&engine, root)
        .unwrap();
    // 200 less the margins, and the padding.
    assert_eq!(engine.read_by_name(root, "seen").to_number(), 184.0);
}

#[test]
fn a_member_its_own_group_lacks_fails_the_build() {
    let err = compile(&MAIN.replace("padding.top:", "padding.tpo:")).unwrap_err();
    assert!(err.contains("`padding.tpo`"), "{err}");
}

#[test]
fn the_toolkits_groups_stay_as_on_item() {
    // `anchors` has no `margin`; on `Item` the build leaves that to the
    // runtime, and on a type extending it too.
    let item = "import QtQuick\nItem { Item { anchors.margin: 10 } }\n";
    assert!(compile(item).is_ok());
    assert!(compile(&MAIN.replace("anchors.margins:", "anchors.margin:")).is_ok());
}
