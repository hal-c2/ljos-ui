//! Module aliases (ARCHITECTURE.md §12.1): an import answered by the module
//! of a directory of the caller's choosing, in a session that reloads.

use std::path::Path;

use ljos_compiler::{Alias, AliasError, Aliases, ReloadSession};
use ljos_ir::TypeRef;
use qml_front::QtTypeOracle;

const MAIN: &str = "import QtQuick\nimport App.Media\nItem { Player { } }\n";

fn write(root: &Path, name: &str, text: &str) {
    let path = root.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// `app/` with `Main` importing `App.Media`, the module in `App/Media`, a
/// stand-in beside it in `mocks/Media`, and another outside in
/// `elsewhere/Media`.
fn tree() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "app/Main.qml", MAIN);
    for (at, width) in [
        ("app/App/Media", 1),
        ("app/mocks/Media", 2),
        ("elsewhere/Media", 3),
    ] {
        write(
            root,
            &format!("{at}/qmldir"),
            "module App.Media\nPlayer 1.0 Player.qml\n",
        );
        write(
            root,
            &format!("{at}/Player.qml"),
            &format!("import QtQuick\nItem {{ width: {width} }}\n"),
        );
    }
    dir
}

/// The file the `Player` of `Main` is made of.
fn player(s: &ReloadSession<QtTypeOracle>) -> String {
    let main = s.live().get("Main").expect("Main compiled");
    main.components
        .iter()
        .flat_map(|c| &c.objects)
        .find_map(|o| match &o.ty {
            TypeRef::Component { file, .. } if file != "Main" => Some(file.clone()),
            _ => None,
        })
        .expect("a Player")
}

fn aliased(module: &str, dir: &Path) -> Aliases {
    [Alias::new(module, dir).unwrap()].into_iter().collect()
}

#[test]
fn the_module_at_its_own_path_is_the_one_imported_and_not_a_stand_in_beside_it() {
    let dir = tree();
    let app = dir.path().join("app");
    let mut s = ReloadSession::open_roots(&[&app], QtTypeOracle).unwrap();
    s.start();
    assert_eq!(player(&s), "App/Media/Player");

    let mut s = ReloadSession::open_reaching(&app, "Main", QtTypeOracle).unwrap();
    s.start();
    assert_eq!(player(&s), "App/Media/Player");
}

#[test]
fn an_alias_inside_the_project_imports_its_directory_and_can_be_dropped() {
    let dir = tree();
    let app = dir.path().join("app");
    for reaching in [false, true] {
        let aliases = aliased("App.Media", &app.join("mocks/Media"));
        let mut s = if reaching {
            ReloadSession::open_reaching_aliased(&app, "Main", aliases, QtTypeOracle).unwrap()
        } else {
            ReloadSession::open_roots_aliased(&[&app], aliases, QtTypeOracle).unwrap()
        };
        s.start();
        assert_eq!(player(&s), "mocks/Media/Player", "reaching: {reaching}");
        // Inside the project: nothing to mount.
        assert!(s.mounts().is_empty());

        s.set_aliases(Aliases::new()).unwrap();
        s.rescan().unwrap();
        let p = s.build().unwrap().expect("a change");
        s.ack(p.generation);
        assert_eq!(player(&s), "App/Media/Player", "reaching: {reaching}");
    }
}

#[test]
fn an_alias_outside_the_project_is_read_under_a_key_of_its_own() {
    let dir = tree();
    let app = dir.path().join("app");
    let outside = dir.path().join("elsewhere/Media");
    for reaching in [false, true] {
        let aliases = aliased("App.Media", &outside);
        let mut s = if reaching {
            ReloadSession::open_reaching_aliased(&app, "Main", aliases, QtTypeOracle).unwrap()
        } else {
            ReloadSession::open_roots_aliased(&[&app], aliases, QtTypeOracle).unwrap()
        };
        s.start();
        assert_eq!(
            player(&s),
            ".aliases/App.Media/Player",
            "reaching: {reaching}"
        );
        let real = std::fs::canonicalize(&outside).unwrap();
        assert_eq!(
            s.key_of(&real.join("Player.qml")).as_deref(),
            Some(".aliases/App.Media/Player")
        );
        assert_eq!(
            s.path_of(".aliases/App.Media/Player"),
            Some(real.join("Player.qml"))
        );
        assert!(s.places().contains(&real));

        // Dropped, its files are none of the project's; it may be named again.
        s.set_aliases(Aliases::new()).unwrap();
        s.rescan().unwrap();
        let p = s.build().unwrap().expect("a change");
        s.ack(p.generation);
        assert_eq!(player(&s), "App/Media/Player");
        assert_eq!(s.key_of(&real.join("Player.qml")), None);
        s.set_aliases(aliased("App.Media", &outside)).unwrap();
        s.rescan().unwrap();
        let p = s.build().unwrap().expect("a change");
        s.ack(p.generation);
        assert_eq!(player(&s), ".aliases/App.Media/Player");
    }
}

#[test]
fn a_directory_outside_the_project_that_the_session_did_not_start_with_is_refused() {
    let dir = tree();
    let app = dir.path().join("app");
    let mut s = ReloadSession::open_reaching(&app, "Main", QtTypeOracle).unwrap();
    s.start();
    let outside = dir.path().join("elsewhere/Media");
    assert!(matches!(
        s.set_aliases(aliased("App.Media", &outside)),
        Err(AliasError::Outside { .. })
    ));
    assert!(s.aliases().is_empty());
}
