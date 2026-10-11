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
use ljos_compiler::{Severity, compile_project};
use ljos_engine::{Engine, LogLevel, MapSource};
use ljos_quick::QuickScene;
use ljos_render::{FrameView, RasterConfig, Rgba, VelloCpuReference, kurbo};
use ljos_runtime::{Clock as _, Time, VirtualClock};
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
        let source = compile(&dir, &key, notes)?;

        let engine = Engine::new();
        ljos_quick::register(&engine);
        ljos_controls::register(&engine);
        let clock = Arc::new(VirtualClock::new());
        engine.set_clock(Rc::new(clock.clone()));
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

/// Compiles every file under `dir`, and keeps those that compiled. A file
/// with a mistake is told of; only one the entry needs stops the run.
fn compile(dir: &Path, entry: &str, notes: &mut Notes) -> anyhow::Result<MapSource> {
    let base = format!("file://{}", dir.to_string_lossy().replace('\\', "/"));
    let mut source = MapSource::new().with_base_url(base);
    let mut entry_error = None;
    let mut seen = false;
    for r in compile_project(dir).with_context(|| format!("{}", dir.display()))? {
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
