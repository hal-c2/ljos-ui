//! The host of Ljos.Test (`*.test.ts`) in `ljos test`: it erases a test
//! file's types with the compiler, and starts each application the tests
//! ask for in this process, on a virtual clock, behind the inspector an
//! agent would talk to (docs/adr/0005-ljos-test.md). The tests act on it
//! only through that inspector's tools.

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use ljos_compiler::Project;
use ljos_dev::mcp::{Inspector, InspectorConfig};
use ljos_drive::Stage;
use ljos_engine::Engine;
use ljos_mcp_core::{
    Cap, CapSet,
    glue::local::{LocalClient, LocalError, LocalWake},
};
use ljos_runtime::Clock as _;
use ljos_test::script::{LoadError, MODULE, Module, PICTURES, SUFFIX, TestHost};
use qml_front::QtTypeOracle;
use serde_json::{Value as Json, json};

use super::Files;

/// The window an application is shown in when neither the test nor the
/// root says how large.
const DEFAULT_SIZE: (f64, f64) = (640.0, 480.0);

/// The `*.test.ts` files below `root`, by key, in name order.
pub(super) fn discover(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_owned()];
    while let Some(dir) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() && !ljos_compiler::skipped_dir(&name) => {
                    stack.push(entry.path());
                }
                Ok(kind) if kind.is_file() && name.ends_with(SUFFIX) => {
                    found.push(super::key(root, &entry.path()));
                }
                _ => {}
            }
        }
    }
    found.sort();
    found
}

/// An application a test started.
struct App {
    // The client goes before the inspector it talks to.
    agent: LocalClient,
    inspector: Inspector,
    stage: Stage,
    started: ljos_runtime::Time,
}

impl App {
    /// One turn of the host's loop the inspector's wake asked for: the
    /// inspector answers, then the frame it asked for is made.
    fn pump(inspector: &mut Inspector, stage: &mut Stage) {
        inspector.set_virtual_clock((*stage.clock).clone());
        inspector.pump(&stage.engine, &mut stage.scene, &*stage.clock);
        // A scene that does not come to rest is the test's to see: the
        // inspector says so in what it answers.
        let _ = stage.drive().settle();
        inspector.observe(&stage.engine);
        inspector.record_scene_frames(&mut stage.scene, 0);
    }
}

pub(super) struct Host {
    root: PathBuf,
    source: Files,
    project: Rc<RefCell<Project<QtTypeOracle>>>,
    /// Why a file of the project does not compile, by key.
    broken: Rc<dyn Fn(&str) -> Option<String>>,
    apps: Vec<Option<App>>,
}

impl Host {
    pub(super) fn new(
        root: PathBuf,
        source: Files,
        project: Rc<RefCell<Project<QtTypeOracle>>>,
        broken: Rc<dyn Fn(&str) -> Option<String>>,
    ) -> Self {
        Host {
            root,
            source,
            project,
            broken,
            apps: Vec::new(),
        }
    }

    fn start(&self, key: &str, size: (Option<f64>, Option<f64>)) -> Result<App, String> {
        let engine = Engine::new();
        crate::render::register_offscreen(&engine);
        engine.set_component_source(self.source.clone());
        engine.set_qml_text_compiler(Some(super::text_compiler(self.project.clone())));
        crate::render::stage_resources(&engine, &self.root);
        // Before the root, for its timers to start on the test's clock.
        let mut stage = Stage::new(&engine, DEFAULT_SIZE.0, DEFAULT_SIZE.1);
        let started = stage.clock.now();
        let root = engine
            .create_from_url(key, None, Vec::new())
            .map_err(|e| e.to_string())?;
        stage
            .scene
            .set_root(&engine, root)
            .map_err(|e| e.to_string())?;
        // Laid out first, as a Text knows its implicit size only then.
        stage.scene.lay_out(&engine);
        let side = |name, given: Option<f64>, or: f64| {
            given.unwrap_or_else(|| {
                let n = engine.read_by_name(root, name).to_number();
                if n >= 1.0 { n.ceil() } else { or }
            })
        };
        let width = side("width", size.0, DEFAULT_SIZE.0);
        let height = side("height", size.1, DEFAULT_SIZE.1);
        stage
            .scene
            .set_viewport(ljos_render::kurbo::Rect::new(0.0, 0.0, width, height));
        // As the shell gives a window that leaves its size unsaid the size
        // of its platform window.
        let unsaid = |name| engine.read_by_name(root, name).to_number() <= 0.0;
        if size.0.is_some() || size.1.is_some() || unsaid("width") || unsaid("height") {
            ljos_controls::WindowSystem::of(&engine).resized(&engine, root, [width, height]);
        }
        stage.scene.set_window_active(&engine, true);
        stage.drive().settle().map_err(|e| e.to_string())?;

        let wake = LocalWake::new();
        let config = InspectorConfig {
            virtual_clock: true,
            ..InspectorConfig::default()
        };
        let (mut inspector, surface) = Inspector::new(config, wake.hook());
        let caps = CapSet::of([Cap::Inspect, Cap::Input, Cap::Mutate]);
        let agent = LocalClient::connect(surface, caps, &wake).map_err(|e| e.to_string())?;
        inspector.watch_log(&engine, |_| {});
        inspector.observe(&engine);
        Ok(App {
            agent,
            inspector,
            stage,
            started,
        })
    }
}

impl TestHost for Host {
    fn load(&mut self, key: &str) -> Result<Module, LoadError> {
        let fault = |message: String| LoadError {
            key: key.to_owned(),
            line: 1,
            column: 1,
            message,
        };
        let path = super::dir_of(&self.root, key);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| fault(format!("{} cannot be read: {e}", path.display())))?;
        ljos_compiler::standalone_module(key, &text, &[MODULE])
            .map(|module| Module {
                source: module.source,
                imports: module.imports.into_iter().map(|i| i.key).collect(),
            })
            .map_err(|faults| {
                let first = faults.into_iter().next();
                LoadError {
                    key: key.to_owned(),
                    line: first.as_ref().map_or(1, |f| f.line),
                    column: first.as_ref().map_or(1, |f| f.column),
                    message: first.map_or_else(|| "does not compile".into(), |f| f.message),
                }
            })
    }

    fn launch(
        &mut self,
        from: &str,
        app: &str,
        width: Option<f64>,
        height: Option<f64>,
    ) -> Result<u32, String> {
        let path = qml_front::project::join(qml_front::project::directory(from), app);
        let key = path.strip_suffix(".qml").unwrap_or(&path).to_owned();
        if let Some(why) = (self.broken)(&key) {
            return Err(format!("{app} does not compile: {why}"));
        }
        if self.source.0.files.get(&key).is_none() {
            return Err(format!("there is no {app} beside {from}"));
        }
        let started = self
            .start(&key, (width, height))
            .map_err(|e| format!("{app} does not start: {e}"))?;
        self.apps.push(Some(started));
        u32::try_from(self.apps.len() - 1).map_err(|e| e.to_string())
    }

    fn call(&mut self, app: u32, tool: &str, args: Json) -> Result<Json, String> {
        let Some(Some(App {
            agent,
            inspector,
            stage,
            ..
        })) = self.apps.get_mut(app as usize)
        else {
            return Err("the application has ended".into());
        };
        let reply = agent.call(tool, args, &mut || App::pump(inspector, stage));
        match reply {
            Ok(reply) if reply.is_error => Err(reply
                .error_message()
                .map_or_else(|| reply.text.clone(), str::to_owned)),
            Ok(reply) => Ok(json!({ "structured": reply.structured, "text": reply.text })),
            Err(LocalError::Refused(e)) => Err(e.message),
            Err(e) => Err(e.to_string()),
        }
    }

    fn now(&mut self, app: u32) -> Result<f64, String> {
        let Some(Some(app)) = self.apps.get(app as usize) else {
            return Err("the application has ended".into());
        };
        let since = app.stage.clock.now().saturating_since(app.started);
        #[allow(clippy::cast_precision_loss)]
        Ok(since.as_micros() as f64 / 1000.0)
    }

    fn picture(&mut self, app: u32, target: Option<&str>) -> Result<Vec<u8>, String> {
        let Some(Some(App {
            agent,
            inspector,
            stage,
            ..
        })) = self.apps.get_mut(app as usize)
        else {
            return Err("the application has ended".into());
        };
        let args = target.map_or_else(|| json!({}), |t| json!({ "target": t }));
        let reply = agent.call("app_screenshot", args, &mut || App::pump(inspector, stage));
        match reply {
            Ok(reply) if reply.is_error => Err(reply
                .error_message()
                .map_or_else(|| reply.text.clone(), str::to_owned)),
            Ok(mut reply) => match reply.images.pop() {
                Some(image) => Ok(image.data),
                None => Err("the inspector gave no picture".into()),
            },
            Err(LocalError::Refused(e)) => Err(e.message),
            Err(e) => Err(e.to_string()),
        }
    }

    fn picture_path(&self, from: &str, name: &str) -> PathBuf {
        let file = super::dir_of(&self.root, from);
        file.parent()
            .unwrap_or(&self.root)
            .join(PICTURES)
            .join(name)
    }

    fn close_all(&mut self) {
        self.apps.clear();
    }
}
