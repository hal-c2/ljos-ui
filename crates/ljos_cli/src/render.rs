//! `ljos render`: loads a QML project without a window, lets a stated time
//! pass on a virtual clock, and writes a picture of what it shows.
//!
//! It is the renderer the gallery (`mise run gallery`) gives `qml-compare`.
//! The last line of standard output is a JSON object that says what
//! happened (how many files compiled, what stopped the example, what the
//! program logged), whether or not a picture was written; that is the
//! protocol `qml-compare run` reads. The exit status is 0 only when the
//! picture was written.

use std::{
    cell::RefCell,
    collections::BTreeMap,
    path::{Path, PathBuf},
    rc::Rc,
};

use anyhow::Context as _;
use clap::Args;
use ljos::dev::{EngineOracle, EngineType};
use ljos_compiler::{Severity, compile_project_with};
use ljos_engine::{Engine, LogLevel, MapSource};
use qml_front::TypeOracle;
use serde_json::{Value, json};

/// The size of a root that says none, in logical pixels.
const DEFAULT_SIZE: (u32, u32) = (800, 600);

#[derive(Args)]
pub struct Render {
    /// The project directory: every `.qml` file below it is part of it.
    dir: PathBuf,
    /// The entry file, relative to the directory.
    #[arg(long, default_value = "Main.qml")]
    entry: String,
    /// The window's size in logical pixels. Without it, the root's own
    /// size, as `qml6` opens a window, and 800x600 for a side it leaves
    /// at 0.
    #[arg(long, value_parser = parse_size, value_name = "WxH")]
    size: Option<(u32, u32)>,
    /// Milliseconds of application time to let pass before the picture.
    #[arg(long, default_value_t = 1500, value_name = "MS")]
    settle: u64,
    /// Where the PNG goes.
    #[arg(long)]
    out: PathBuf,
    /// A gallery example id (of `corpus/examples.json`) whose native
    /// backends to register on the Engine before the project is compiled
    /// and loaded, as the example's own program registers its types when
    /// run under Qt. Without it, nothing is registered: a plain render
    /// of a user project is unaffected. `mise run gallery` passes the id
    /// `qml-compare` gives it (`QML_COMPARE_ID`).
    #[arg(long, value_name = "ID")]
    example: Option<String>,
    /// Pins the wall clock a script's `Date` sees to this RFC 3339 time
    /// (`2026-10-11T17:57:09Z`); it then moves with the settle time. The
    /// gallery passes the time the Qt picture was captured at, so a clock
    /// example is the same picture on every run.
    #[arg(long, value_parser = parse_now, value_name = "RFC3339")]
    now: Option<f64>,
    /// Sets text in the fonts installed on the machine, the way a desktop
    /// application does, with the platform's substitute for a family it
    /// lacks. Without it, text is set in the bundled font only and the
    /// picture is the same on every machine. `mise run gallery` passes it:
    /// the qml6 pictures it compares with were drawn with the machine's
    /// fonts.
    #[arg(long)]
    system_fonts: bool,
}

fn parse_now(text: &str) -> Result<f64, String> {
    ljos_runtime::parse_rfc3339_ms(text)
        .ok_or_else(|| format!("{text:?} is not an RFC 3339 time such as 2026-10-11T17:57:09Z"))
}

fn parse_size(text: &str) -> Result<(u32, u32), String> {
    let wrong = || format!("{text:?} is not a size such as 800x600");
    let (w, h) = text.split_once(['x', 'X']).ok_or_else(wrong)?;
    let side = |s: &str| {
        s.trim()
            .parse::<u32>()
            .ok()
            .filter(|v| (1..=16_384).contains(v))
            .ok_or_else(wrong)
    };
    Ok((side(w)?, side(h)?))
}

/// What one run found out, in the words `qml-compare` reads.
#[derive(Default)]
struct Notes {
    files: usize,
    compiled: usize,
    entry_compiles: bool,
    error: Option<String>,
    compile_errors: BTreeMap<String, String>,
    said: Vec<(String, u32)>,
}

impl Notes {
    fn json(&self) -> Value {
        json!({
            "files": self.files,
            "compiled": self.compiled,
            "entryCompiles": self.entry_compiles,
            "error": self.error,
            "compileErrors": self.compile_errors,
            "said": self.said.iter().map(|(text, times)| json!({"text": text, "times": times})).collect::<Vec<_>>(),
        })
    }
}

/// The first line of a message: the rest is a stack or a list.
fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or("").trim()
}

/// Messages the program logged, each once with how many times.
#[derive(Clone, Default)]
struct Said(Rc<RefCell<Vec<(String, u32)>>>);

impl Said {
    fn add(&self, text: &str) {
        let mut all = self.0.borrow_mut();
        match all.iter_mut().find(|(t, _)| t == text) {
            Some((_, times)) => *times += 1,
            None => all.push((text.to_owned(), 1)),
        }
    }
}

impl Render {
    /// Whether the picture was written.
    pub fn run(self) -> bool {
        let mut notes = Notes::default();
        let outcome = self.render(&mut notes);
        if let Err(e) = &outcome {
            // A fault of the example is data for the report, not of the tool.
            let text = format!("{e:#}");
            eprintln!("ljos: {text}");
            notes.error.get_or_insert(text);
        }
        println!("{}", notes.json());
        outcome.is_ok()
    }

    fn render(&self, notes: &mut Notes) -> anyhow::Result<()> {
        let dir = self
            .dir
            .canonicalize()
            .with_context(|| format!("{}", self.dir.display()))?;
        let key = self
            .entry
            .strip_suffix(".qml")
            .unwrap_or(&self.entry)
            .to_owned();

        // The Engine first: with `--example`, the backends are registered
        // before the project is compiled, so that the compiler knows the
        // example's own types (a native type is in no table of types) and
        // not only the runtime does.
        let engine = Engine::new();
        // Before anything measures text: the engine is the thread's, and the
        // bundled one is made on first use.
        ljos::render::set_fonts(if self.system_fonts {
            ljos::render::Fonts::System
        } else {
            ljos::render::Fonts::Bundled
        });
        ljos_quick::register(&engine);
        ljos_controls::register(&engine);
        ljos_media::register(&engine);
        // The render's clock is virtual: sound on a device would play at
        // the wrong pace.
        ljos_media::use_null_audio(&engine);
        // A picture leaves no file behind: databases stay in memory.
        ljos_localstorage::register(&engine);
        ljos_localstorage::set_storage(&engine, ljos_localstorage::Storage::Memory);
        register_example(&engine, self.example.as_deref());
        // As if the program were started on the entry file, as Qt's
        // launcher is.
        ljos_controls::set_application_arguments(
            &engine,
            [
                crate::run::program(),
                self.dir.join(&self.entry).display().to_string(),
            ],
        );
        let source = match &self.example {
            Some(_) => compile(&dir, &key, notes, EngineOracle::from_engine(&engine))?,
            None => compile(&dir, &key, notes, EngineOracle::table_only())?,
        };
        let said = Said::default();
        let sink = said.clone();
        // The reference picture of a window is of its items: the window's
        // own colour is not in it, and where no item paints the picture is
        // transparent (corpus/harness/reference.qml).
        let (width, height) = self.size.unwrap_or(DEFAULT_SIZE);
        let picture = ljos::render::Picture::new(width, height);
        let picture = if self.size.is_none() {
            picture.root_sized()
        } else {
            picture
        }
        .settle(self.settle)
        .on_scene_message(move |text| sink.add(text))
        .without_window_colour();
        picture.install(&engine);
        engine.set_wall_clock_origin(self.now);
        let sink = said.clone();
        engine.set_log_handler(move |entry| {
            if matches!(entry.level, LogLevel::Warning | LogLevel::Critical) {
                sink.add(first_line(&entry.message));
            }
        });
        engine.set_component_source(source);
        stage_resources(&engine, &dir);
        let root = engine
            .create_from_url(&key, None, example_properties(self.example.as_deref()))
            .map_err(|e| anyhow::anyhow!("{e}"));
        notes.said.clear();
        let root = root.inspect_err(|_| notes.said = said.0.borrow().clone())?;
        let png = picture
            .png(&engine, root)
            .map_err(|e| anyhow::anyhow!("{e}"));
        notes.said = said.0.borrow().clone();
        let png = png?;
        if let Some(parent) = self.out.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::write(&self.out, png).with_context(|| format!("writing {}", self.out.display()))
    }
}

/// Makes the pictures the project's build keeps inside its program
/// (`qrc:/qt/qml/<Module>/icons/a.svg`, as `qt_add_qml_module` lays out a
/// module's `RESOURCES`) loadable by the address the program names them by.
pub(crate) fn stage_resources(engine: &Engine, dir: &Path) {
    if let Ok(files) = ljos_compiler::resources(dir) {
        stage(engine, &files);
    }
}

/// Stages the pictures of `files` (address, path), each read when it is
/// first shown.
pub(crate) fn stage(engine: &Engine, files: &[(String, std::path::PathBuf)]) {
    const PICTURES: [&str; 7] = ["png", "jpg", "jpeg", "gif", "bmp", "svg", "webp"];
    let images = ljos_quick::ImageStore::of(engine);
    for (address, path) in files {
        let picture = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| PICTURES.iter().any(|p| e.eq_ignore_ascii_case(p)));
        if picture {
            images.register_resource_file(address.clone(), path.clone());
        }
    }
}

/// Registers the backends of the gallery example `example` on `engine`;
/// without an example, nothing. Shared with `ljos run --example`.
pub(crate) fn register_example(engine: &Engine, example: Option<&str>) {
    if let Some(id) = example {
        ljos_gallery_backends::register(engine, id);
    }
}

/// What the launcher of the gallery example `example` gives its root
/// object; without an example, nothing. Shared with `ljos run --example`.
pub(crate) fn example_properties(example: Option<&str>) -> Vec<(String, ljos_engine::Value)> {
    example
        .map(ljos_gallery_backends::initial_properties)
        .unwrap_or_default()
}

/// The compiler's knowledge of an example's types: the table of types as
/// usual, and the native types registered for the example on top of it.
/// Shared with `ljos run --example`, which compiles before its shell
/// Engine exists.
pub(crate) fn oracle_for_example(id: &str) -> EngineOracle {
    let engine = Engine::new();
    ljos_quick::register(&engine);
    ljos_controls::register(&engine);
    ljos_media::register(&engine);
    ljos_localstorage::register(&engine);
    register_example(&engine, Some(id));
    EngineOracle::from_engine(&engine)
}

/// A project compiled whole: every file that compiled, and for each one
/// that did not, why, which whatever makes an object of it is told. The
/// gallery's report counts the project's files, so a render compiles them
/// all; `ljos run` compiles only what the entry reaches (`demand.rs`).
struct Loaded {
    source: MapSource,
    files: usize,
    compiled: usize,
    /// The first error of each file that did not compile, by file name.
    compile_errors: BTreeMap<String, String>,
    /// The entry is in the project.
    entry_seen: bool,
    /// The entry's own first error, with its name.
    entry_error: Option<String>,
}

impl Loaded {
    /// The source, unless the entry cannot be made: it is not in the
    /// project, or it did not compile.
    fn into_source(self, dir: &Path, entry: &str) -> anyhow::Result<MapSource> {
        if let Some(error) = self.entry_error {
            anyhow::bail!("{error}");
        }
        if !self.entry_seen {
            anyhow::bail!("{entry}.qml is not in {}", dir.display());
        }
        Ok(self.source)
    }
}

/// Compiles every file under `dir` against `oracle`, as Qt loads a
/// program: a file with a mistake stops nothing until an object of it is
/// made, so only the entry's own mistake stops the program.
fn load_project<O: TypeOracle<Type = EngineType>>(
    dir: &Path,
    entry: &str,
    oracle: O,
) -> anyhow::Result<Loaded> {
    let base = format!("file://{}", dir.to_string_lossy().replace('\\', "/"));
    let mut out = Loaded {
        source: MapSource::new().with_base_url(base),
        files: 0,
        compiled: 0,
        compile_errors: BTreeMap::new(),
        entry_seen: false,
        entry_error: None,
    };
    for r in compile_project_with(dir, oracle).with_context(|| format!("{}", dir.display()))? {
        out.files += 1;
        out.entry_seen |= r.key == entry;
        match r.usable() {
            Ok(_) => {}
            Err(why) => {
                // The compiler's own words for the report; the loader's
                // for whatever makes an object of the file.
                let first = match &r.result {
                    Ok(c) => c.diagnostics.iter().find(|d| d.severity == Severity::Error),
                    Err(ds) => ds.first(),
                }
                .map_or_else(|| "does not compile".to_owned(), ToString::to_string);
                out.source.insert_failure(r.key.clone(), why);
                out.compile_errors
                    .insert(format!("{}.qml", r.key), first.clone());
                if r.key == entry {
                    out.entry_error = Some(format!("{}.qml:{first}", r.key));
                }
                continue;
            }
        }
        let Ok(c) = r.result else { continue };
        out.compiled += 1;
        out.source.insert(r.key, c.file);
    }
    Ok(out)
}

/// [`load_project`], told in `notes`.
fn compile<O: TypeOracle<Type = EngineType>>(
    dir: &Path,
    entry: &str,
    notes: &mut Notes,
    oracle: O,
) -> anyhow::Result<MapSource> {
    let loaded = load_project(dir, entry, oracle)?;
    notes.files = loaded.files;
    notes.compiled = loaded.compiled;
    notes.compile_errors = loaded.compile_errors.clone();
    notes.entry_compiles = loaded.entry_seen && loaded.entry_error.is_none();
    loaded.into_source(dir, entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, text) in files {
            let path = dir.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        dir
    }

    fn render(dir: &Path, entry: &str) -> (anyhow::Result<()>, Notes, PathBuf) {
        let out = dir.join("out.png");
        let r = Render {
            dir: dir.to_owned(),
            entry: entry.to_owned(),
            size: Some((40, 30)),
            settle: 100,
            out: out.clone(),
            example: None,
            now: None,
            system_fonts: false,
        };
        let mut notes = Notes::default();
        let result = r.render(&mut notes);
        (result, notes, out)
    }

    #[test]
    fn sizes_are_width_by_height() {
        assert_eq!(parse_size("800x600"), Ok((800, 600)));
        assert!(parse_size("0x600").is_err());
        assert!(parse_size("800").is_err());
        assert!(parse_size("1.5x2").is_err());
    }

    #[test]
    fn a_project_is_drawn_on_white_at_the_size_asked_for() {
        let dir = project(&[(
            "Main.qml",
            "import QtQuick\nRectangle { width: 20; height: 10; color: \"red\" }\n",
        )]);
        let (result, notes, out) = render(dir.path(), "Main.qml");
        result.unwrap();
        assert_eq!((notes.files, notes.compiled), (1, 1));
        assert!(notes.entry_compiles);
        let bytes = std::fs::read(out).unwrap();
        let image = ljos_render::RgbaImage::from_png(&bytes).unwrap();
        assert_eq!((image.width, image.height), (40, 30));
        let at = |x: usize, y: usize| &image.data[(y * 40 + x) * 4..(y * 40 + x) * 4 + 4];
        assert_eq!(at(5, 5), [255, 0, 0, 255]);
        assert_eq!(at(30, 20), [255, 255, 255, 255]);
    }

    #[test]
    fn a_root_window_that_leaves_its_size_unsaid_is_as_large_as_the_picture() {
        // A popup sized and centred from its parent is where a window
        // that has a size puts it; the window's content item is empty
        // unless the picture's size is told to the window.
        let dir = project(&[(
            "Main.qml",
            "import QtQuick\nimport QtQuick.Controls.Basic\nApplicationWindow {\n  visible: true\n  Rectangle { x: 0; y: 0; width: parent.width; height: parent.height; color: \"red\" }\n}\n",
        )]);
        let (result, _, out) = render(dir.path(), "Main.qml");
        result.unwrap();
        let image = ljos_render::RgbaImage::from_png(&std::fs::read(out).unwrap()).unwrap();
        let at = |x: usize, y: usize| &image.data[(y * 40 + x) * 4..(y * 40 + x) * 4 + 4];
        assert_eq!(at(35, 25), [255, 0, 0, 255]);
    }

    #[test]
    fn a_window_is_pictured_without_its_colour() {
        let dir = project(&[(
            "Main.qml",
            "import QtQuick\nWindow {\n  visible: true\n  color: \"blue\"\n  Rectangle { width: 20; height: 10; color: \"red\" }\n}\n",
        )]);
        let (result, _, out) = render(dir.path(), "Main.qml");
        result.unwrap();
        let image = ljos_render::RgbaImage::from_png(&std::fs::read(out).unwrap()).unwrap();
        let at = |x: usize, y: usize| &image.data[(y * 40 + x) * 4..(y * 40 + x) * 4 + 4];
        assert_eq!(at(5, 5), [255, 0, 0, 255]);
        assert_eq!(at(30, 20)[3], 0);
    }

    #[test]
    fn time_passes_on_a_virtual_clock() {
        let dir = project(&[(
            "Main.qml",
            "import QtQuick\nRectangle {\n  width: 40; height: 30; color: \"red\"\n  Timer { interval: 50; running: true; onTriggered: parent.color = \"blue\" }\n}\n",
        )]);
        let (result, _, out) = render(dir.path(), "Main.qml");
        result.unwrap();
        let image = ljos_render::RgbaImage::from_png(&std::fs::read(out).unwrap()).unwrap();
        assert_eq!(&image.data[..4], [0, 0, 255, 255]);
    }

    #[test]
    fn a_mistake_in_the_entry_is_the_error_and_other_files_are_counted() {
        let dir = project(&[
            ("Main.qml", "import QtQuick\nItem {\n  width: 20 +\n}\n"),
            ("Other.qml", "import QtQuick\nItem {}\n"),
        ]);
        let (result, notes, out) = render(dir.path(), "Main.qml");
        let error = result.unwrap_err().to_string();
        assert!(error.starts_with("Main.qml:"), "{error}");
        assert_eq!((notes.files, notes.compiled), (2, 1));
        assert!(!notes.entry_compiles);
        assert!(notes.compile_errors.contains_key("Main.qml"));
        assert!(!out.exists());
    }

    #[test]
    fn a_missing_entry_is_said_so() {
        let dir = project(&[("Other.qml", "import QtQuick\nItem {}\n")]);
        let (result, _, _) = render(dir.path(), "Main.qml");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Main.qml is not in")
        );
    }

    #[derive(clap::Parser)]
    struct Line {
        #[command(flatten)]
        render: Render,
    }

    fn line(args: &[&str]) -> Render {
        use clap::Parser as _;
        Line::try_parse_from([&["ljos-render"], args].concat())
            .map(|l| l.render)
            .unwrap()
    }

    #[test]
    fn no_example_is_the_default_and_an_id_is_named() {
        assert_eq!(line(&["proj", "--out", "p.png"]).example, None);
        assert_eq!(
            line(&["proj", "--out", "p.png", "--example", "rssnews"]).example,
            Some("rssnews".to_owned())
        );
    }

    #[test]
    fn the_bundled_font_is_the_default() {
        assert!(!line(&["proj", "--out", "p.png"]).system_fonts);
        assert!(line(&["proj", "--out", "p.png", "--system-fonts"]).system_fonts);
    }

    #[test]
    fn now_is_an_rfc3339_time_or_nothing() {
        assert_eq!(line(&["proj", "--out", "p.png"]).now, None);
        assert_eq!(
            line(&["proj", "--out", "p.png", "--now", "2026-10-11T17:57:09Z"]).now,
            Some(1_791_741_429_000.0)
        );
        use clap::Parser as _;
        assert!(Line::try_parse_from(["r", "p", "--out", "o", "--now", "noon"]).is_err());
    }

    #[test]
    fn a_pinned_clock_is_what_the_script_reads() {
        let dir = project(&[(
            "Main.qml",
            "import QtQuick\nRectangle { width: 20; height: 10\n  color: new Date().getUTCFullYear() === 2026 ? \"green\" : \"red\" }\n",
        )]);
        let out = dir.path().join("out.png");
        let r = Render {
            dir: dir.path().to_owned(),
            entry: "Main.qml".to_owned(),
            size: Some((40, 30)),
            settle: 100,
            out: out.clone(),
            example: None,
            now: Some(1_791_741_429_000.0),
            system_fonts: false,
        };
        r.render(&mut Notes::default()).unwrap();
        let image = ljos_render::RgbaImage::from_png(&std::fs::read(out).unwrap()).unwrap();
        // "green" is #008000; "red" would have a full red channel.
        assert_eq!(&image.data[..4], [0, 128, 0, 255]);
    }

    #[test]
    fn an_example_without_a_backend_renders_as_before() {
        let dir = project(&[(
            "Main.qml",
            "import QtQuick\nRectangle { width: 20; height: 10; color: \"red\" }\n",
        )]);
        let out = dir.path().join("out.png");
        let r = Render {
            dir: dir.path().to_owned(),
            entry: "Main.qml".to_owned(),
            size: Some((40, 30)),
            settle: 100,
            out: out.clone(),
            example: Some("no-such-example".to_owned()),
            now: None,
            system_fonts: false,
        };
        let mut notes = Notes::default();
        r.render(&mut notes).unwrap();
        assert!(notes.entry_compiles);
        assert!(out.exists());
        // And nothing was registered for it.
        assert!(!ljos_gallery_backends::supported().contains(&"no-such-example"));
    }

    /// A backend type of the kind a gallery example registers: in no
    /// table of types, known only from its description.
    #[derive(Debug, Default, ljos_engine::QmlObject)]
    #[qml(module = "Backends", name = "Gauge")]
    pub struct Gauge {
        #[qml(notify)]
        pub level: i32,
    }

    #[test]
    fn a_registered_native_type_compiles_only_with_its_oracle() {
        let dir = project(&[(
            "Main.qml",
            "import QtQuick\nimport Backends\nItem {\n  Gauge { id: g; level: 3 }\n  property int seen: g.level\n}\n",
        )]);
        // The table of types does not know it: the entry does not compile.
        let mut notes = Notes::default();
        let plain = compile(dir.path(), "Main", &mut notes, EngineOracle::table_only());
        assert!(plain.is_err(), "compiled without the backend");
        assert!(!notes.entry_compiles);

        // Registered on an Engine, its description is enough.
        let engine = Engine::new();
        ljos_quick::register(&engine);
        engine.register_module(ljos_engine::module!("Backends" 1.0 => [Gauge]));
        let mut notes = Notes::default();
        compile(
            dir.path(),
            "Main",
            &mut notes,
            EngineOracle::from_engine(&engine),
        )
        .unwrap();
        assert!(notes.entry_compiles);
        assert!(
            notes.compile_errors.is_empty(),
            "{:?}",
            notes.compile_errors
        );
    }

    #[test]
    fn a_module_resource_is_drawn_from_the_address_its_build_gives_it() {
        let dir = project(&[
            (
                "CMakeLists.txt",
                "qt_add_qml_module(app URI Shapes QML_FILES Main.qml RESOURCES icons/red.svg)\n",
            ),
            (
                "icons/red.svg",
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"40\" height=\"30\"><rect width=\"40\" height=\"30\" fill=\"#ff0000\"/></svg>",
            ),
            (
                "Main.qml",
                "import QtQuick\nImage { width: 40; height: 30; source: \"qrc:/qt/qml/Shapes/icons/red.svg\" }\n",
            ),
        ]);
        let (result, notes, out) = render(dir.path(), "Main.qml");
        result.unwrap();
        assert!(notes.said.is_empty(), "{:?}", notes.said);
        let image = ljos_render::RgbaImage::from_png(&std::fs::read(out).unwrap()).unwrap();
        assert_eq!(&image.data[(15 * 40 + 20) * 4..][..4], [255, 0, 0, 255]);
    }

    #[test]
    fn a_file_that_did_not_compile_is_said_to_be_that_not_missing() {
        let dir = project(&[
            (
                "Main.qml",
                "import QtQuick\nItem { Loader { source: \"Broken.qml\" } }\n",
            ),
            ("Broken.qml", "import QtQuick\nNoSuchType {}\n"),
        ]);
        let (result, notes, _) = render(dir.path(), "Main.qml");
        result.unwrap();
        let said: Vec<_> = notes.said.iter().map(|(t, _)| t.as_str()).collect();
        assert!(
            said.iter()
                .any(|t| t.contains("Broken.qml:2:1: ") && t.contains("is not a type")),
            "{said:?}"
        );
        assert!(said.iter().all(|t| !t.contains("No such file")), "{said:?}");
    }

    #[test]
    fn what_the_program_warns_of_is_counted() {
        let dir = project(&[(
            "Main.qml",
            "import QtQuick\nItem {\n  Component.onCompleted: { console.warn(\"twice\"); console.warn(\"twice\") }\n}\n",
        )]);
        let (result, notes, _) = render(dir.path(), "Main.qml");
        result.unwrap();
        assert_eq!(notes.said, vec![("twice".to_owned(), 2)]);
    }
}
