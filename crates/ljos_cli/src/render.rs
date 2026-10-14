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
    sync::Arc,
    time::Duration,
};

use anyhow::Context as _;
use clap::Args;
use ljos::dev::{EngineOracle, EngineType};
use ljos_compiler::{Severity, compile_project_with};
use ljos_engine::{Engine, LogLevel, MapSource};
use ljos_quick::QuickScene;
use ljos_render::{FrameView, RasterConfig, Rgba, VelloCpuReference, kurbo};
use ljos_runtime::{Clock as _, Time, VirtualClock};
use qml_front::TypeOracle;
use serde_json::{Value, json};

/// Frames a scene may need to come to rest before something is wrong with
/// it, and timer firings before the settle time is given up on.
const MAX_SETTLE_FRAMES: usize = 64;
const MAX_TIMER_STEPS: usize = 100_000;

#[derive(Args)]
pub struct Render {
    /// The project directory: every `.qml` file below it is part of it.
    dir: PathBuf,
    /// The entry file, relative to the directory.
    #[arg(long, default_value = "Main.qml")]
    entry: String,
    /// The window's size in logical pixels.
    #[arg(long, value_parser = parse_size, default_value = "800x600", value_name = "WxH")]
    size: (u32, u32),
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
        ljos_quick::register(&engine);
        ljos_controls::register(&engine);
        register_example(&engine, self.example.as_deref());
        let source = match &self.example {
            Some(_) => compile(&dir, &key, notes, EngineOracle::from_engine(&engine))?,
            None => compile(&dir, &key, notes, EngineOracle::table_only())?,
        };
        let clock = Arc::new(VirtualClock::new());
        engine.set_clock(Rc::new(clock.clone()));
        engine.set_wall_clock_origin(self.now);
        let said = Said::default();
        let sink = said.clone();
        engine.set_log_handler(move |entry| {
            if matches!(entry.level, LogLevel::Warning | LogLevel::Critical) {
                sink.add(first_line(&entry.message));
            }
        });
        engine.set_component_source(source);
        let root = engine
            .create_from_url(&key, None, Vec::new())
            .map_err(|e| anyhow::anyhow!("{e}"));
        notes.said.clear();
        let root = root.inspect_err(|_| notes.said = said.0.borrow().clone())?;

        let (width, height) = self.size;
        let mut scene = QuickScene::new(kurbo::Size::new(f64::from(width), f64::from(height)));
        let sink = said.clone();
        scene
            .log()
            .set_handler(move |e| sink.add(first_line(&e.message)));
        scene
            .set_root(&engine, root)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        // A root window that leaves its size unsaid is as large as the
        // picture, as the shell gives it the size of its platform window;
        // without this its content item is empty and so is whatever is
        // sized from it (a popup's `parent.width`).
        let unsaid = |name| engine.read_by_name(root, name).to_number() <= 0.0;
        if unsaid("width") || unsaid("height") {
            let size = [f64::from(width), f64::from(height)];
            ljos_controls::WindowSystem::of(&engine).resized(&engine, root, size);
        }

        let mut run = Run {
            engine: &engine,
            scene: &mut scene,
            clock: &clock,
            wake: None,
        };
        run.settle()?;
        run.advance(self.settle)?;
        let png = run.picture()?;
        notes.said = said.0.borrow().clone();
        if let Some(parent) = self.out.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::write(&self.out, png).with_context(|| format!("writing {}", self.out.display()))
    }
}

/// Registers the backends of the gallery example `example` on `engine`;
/// without an example, nothing. Shared with `ljos run --example`.
pub(crate) fn register_example(engine: &Engine, example: Option<&str>) {
    if let Some(id) = example {
        ljos_gallery_backends::register(engine, id);
    }
}

/// The compiler's knowledge of an example's types: the table of types as
/// usual, and the native types registered for the example on top of it.
/// Shared with `ljos run --example`, which compiles before its shell
/// Engine exists.
pub(crate) fn oracle_for_example(id: &str) -> EngineOracle {
    let engine = Engine::new();
    ljos_quick::register(&engine);
    ljos_controls::register(&engine);
    register_example(&engine, Some(id));
    EngineOracle::from_engine(&engine)
}

/// Compiles every file under `dir` against `oracle`, and keeps those that
/// compiled. A file with a mistake is told of; only one the entry needs
/// stops the run.
fn compile<O: TypeOracle<Type = EngineType>>(
    dir: &Path,
    entry: &str,
    notes: &mut Notes,
    oracle: O,
) -> anyhow::Result<MapSource> {
    let base = format!("file://{}", dir.to_string_lossy().replace('\\', "/"));
    let mut source = MapSource::new().with_base_url(base);
    let mut entry_error = None;
    let mut seen = false;
    for r in compile_project_with(dir, oracle).with_context(|| format!("{}", dir.display()))? {
        notes.files += 1;
        let errors: Vec<String> = match r.result {
            Ok(c) => {
                let errors = c
                    .diagnostics
                    .iter()
                    .filter(|d| d.severity == Severity::Error)
                    .map(ToString::to_string)
                    .collect();
                if c.diagnostics.iter().all(|d| d.severity != Severity::Error) {
                    source.insert(r.key.clone(), c.file);
                }
                errors
            }
            Err(ds) => {
                let lines: Vec<String> = ds.iter().map(ToString::to_string).collect();
                if lines.is_empty() {
                    vec!["does not compile".to_owned()]
                } else {
                    lines
                }
            }
        };
        match errors.first() {
            None => notes.compiled += 1,
            Some(first) => {
                notes
                    .compile_errors
                    .insert(format!("{}.qml", r.key), first.clone());
                if r.key == entry {
                    entry_error = Some(format!("{}.qml:{first}", r.key));
                }
            }
        }
        if r.key == entry {
            seen = true;
            notes.entry_compiles = entry_error.is_none();
        }
    }
    if let Some(error) = entry_error {
        anyhow::bail!("{error}");
    }
    if !seen {
        anyhow::bail!("{entry}.qml is not in {}", dir.display());
    }
    Ok(source)
}

/// The scene and the clock it runs on, being driven as a host drives them.
struct Run<'a> {
    engine: &'a Engine,
    scene: &'a mut QuickScene,
    clock: &'a Arc<VirtualClock>,
    /// When the scene last said it next needs a frame.
    wake: Option<Time>,
}

impl Run<'_> {
    /// Frames until the scene is idle: layout, anchors, deferred deletes.
    fn settle(&mut self) -> anyhow::Result<()> {
        for _ in 0..MAX_SETTLE_FRAMES {
            let now = self.clock.now();
            let out = self.scene.frame(self.engine, now);
            if !self.scene.needs_frame() && out.next_wake.is_none_or(|t| t > now) {
                self.wake = out.next_wake;
                return Ok(());
            }
        }
        anyhow::bail!("the scene did not settle in {MAX_SETTLE_FRAMES} frames")
    }

    /// Lets `ms` pass, firing each timer at its own deadline and settling in
    /// between, as a host woken at each deadline would.
    fn advance(&mut self, ms: u64) -> anyhow::Result<()> {
        let images = ljos_quick::ImageStore::of(self.engine);
        images.finish(self.engine);
        let end = self.clock.now() + Duration::from_millis(ms);
        for _ in 0..MAX_TIMER_STEPS {
            let next = [self.engine.next_timer_deadline(), self.wake]
                .into_iter()
                .flatten()
                .min();
            match next {
                Some(t) if t <= end => {
                    if t > self.clock.now() {
                        self.clock.set(t);
                    }
                    self.engine.advance_timers(self.clock.now());
                    self.settle()?;
                }
                _ => {
                    self.clock.set(end);
                    self.engine.advance_timers(end);
                    images.finish(self.engine);
                    return self.settle();
                }
            }
        }
        anyhow::bail!("timers fired more than {MAX_TIMER_STEPS} times in {ms} ms")
    }

    /// A PNG of the scene on white, which is what the reference window is.
    fn picture(&mut self) -> anyhow::Result<Vec<u8>> {
        self.settle()?;
        let out = self.scene.frame(self.engine, self.clock.now());
        let mut r = VelloCpuReference::new(RasterConfig {
            background: Rgba::from_straight(1.0, 1.0, 1.0, 1.0),
            ..RasterConfig::default()
        });
        let image = r
            .render(&FrameView::new(&out.snapshot, 1.0))
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        image.to_png().map_err(|e| anyhow::anyhow!("{e}"))
    }
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
            size: (40, 30),
            settle: 100,
            out: out.clone(),
            example: None,
            now: None,
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
            size: (40, 30),
            settle: 100,
            out: out.clone(),
            example: None,
            now: Some(1_791_741_429_000.0),
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
            size: (40, 30),
            settle: 100,
            out: out.clone(),
            example: Some("no-such-example".to_owned()),
            now: None,
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
