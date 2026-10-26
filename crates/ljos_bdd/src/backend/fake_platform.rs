//! The fake platform: the system a scenario's application runs on when it
//! runs in the runner's own process. It has a directory of its own for
//! every file the application reads or writes, dialogs the scenario
//! answers as the user, a system that asks for permissions when the
//! scenario says so, sensors and a position that read what the scenario
//! says, and an opener that only notes the URLs. Nothing reaches the real
//! home directory, dialogs, devices or browser.
//!
//! It is a fixture of its own, apart from any backend: a backend gives it
//! to an engine with [`FakePlatform::install`] and answers the platform
//! steps from it.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use ljos_engine::Engine;
use ljos_platform::{
    FakeDialogs, FakePermissions, FakeSensors, HttpFetcher, Measurement, PathEnv, Platform,
    RecordedDragOut, RecordedUrls, SensorKind,
};
use ljos_positioning::{FixedPosition, GeoCoordinate, PositionFix};

use super::{BackendError, Result, SystemDialogAnswer};

/// The user's home, as the application sees it.
pub const HOME: &str = "/home/user";

/// The system of one scenario. Each scenario has a new one.
#[derive(Default)]
pub struct FakePlatform {
    /// The root of the scenario's file system; `/` of the application is
    /// here. Made when first needed, removed with the scenario.
    root: Option<tempfile::TempDir>,
    dialogs: FakeDialogs,
    /// A system that asks the user, when the scenario says so; otherwise
    /// nothing guards anything, as on desktop Linux.
    permissions: Option<FakePermissions>,
    /// What the application asked the system to open; nothing is opened.
    opened: RecordedUrls,
    /// The device's sensors, which read what the scenario says; none
    /// unless it says so, as on a desktop.
    sensors: FakeSensors,
    /// Where the system is; no position service unless the scenario says.
    position: Option<FixedPosition>,
    /// A window system that carries drags out of the window, when the
    /// scenario says so; it carries none to anywhere.
    drag_out: Option<RecordedDragOut>,
}

impl std::fmt::Debug for FakePlatform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FakePlatform")
            .field("root", &self.root.as_ref().map(tempfile::TempDir::path))
            .field("asks_permissions", &self.permissions.is_some())
            .field("has_position", &self.position.is_some())
            .finish_non_exhaustive()
    }
}

impl FakePlatform {
    fn root(&mut self) -> Result<PathBuf> {
        if self.root.is_none() {
            let dir = tempfile::Builder::new()
                .prefix("ljos-bdd-")
                .tempdir()
                .map_err(|e| failed(format!("no directory for the scenario's files: {e}")))?;
            self.root = Some(dir);
        }
        Ok(self
            .root
            .as_ref()
            .map(|d| d.path().to_path_buf())
            .unwrap_or_default())
    }

    /// Where the application's `/name` is.
    fn path(&mut self, name: &str) -> Result<PathBuf> {
        Ok(self.root()?.join(name.trim_start_matches('/')))
    }

    /// Gives the engine the scenario's file system, dialogs and network. Answers
    /// wait until time passes or the user acts, as on a slow system.
    pub fn install(&mut self, engine: &Engine) -> Result<()> {
        let platform = Platform::of(engine);
        platform.set_env(PathEnv::sandbox(HOME, self.root()?));
        platform.set_dialogs(Rc::new(self.dialogs.clone()));
        platform.set_open_url(Rc::new(self.opened.clone()));
        platform.set_sensors(Rc::new(self.sensors.clone()));
        if let Some(position) = &self.position {
            ljos_positioning::set_service(engine, Rc::new(position.clone()));
        }
        // A real network, as Qt has: a scenario reaches its own web server
        // on the loopback interface, and nothing else it names answers.
        platform.set_fetcher(Arc::new(HttpFetcher::new()));
        if let Some(asking) = &self.permissions {
            platform.set_permissions(Rc::new(asking.clone()));
        }
        self.install_drag_out(engine);
        platform.set_held(true);
        Ok(())
    }

    /// The device has the sensor `name`, which reads nothing yet.
    pub fn add_sensor(&self, name: &str) -> Result<()> {
        self.sensors.add(sensor_named(name)?);
        Ok(())
    }

    /// The sensor `name` reads `values` at `timestamp`.
    pub fn sensor_reads(&self, name: &str, timestamp: u64, values: &[f64]) -> Result<()> {
        let kind = sensor_named(name)?;
        // A sensor that is not running is given nothing, as a device's
        // driver gives nothing to who does not listen.
        self.sensors.feed(kind, Measurement::new(timestamp, values));
        Ok(())
    }

    /// The system is at the position given, and has a position service.
    pub fn system_position(&mut self, latitude: f64, longitude: f64, altitude: f64) {
        let fix = PositionFix::at(GeoCoordinate::new(latitude, longitude, altitude));
        self.position = Some(FixedPosition::new(fix));
    }

    /// The system moves to the position given.
    pub fn position_moves(&self, latitude: f64, longitude: f64, altitude: f64) -> Result<()> {
        let Some(position) = &self.position else {
            return Err(failed("say where the system is before it moves"));
        };
        position.move_to(PositionFix::at(GeoCoordinate::new(
            latitude, longitude, altitude,
        )));
        Ok(())
    }

    /// The URLs the application asked the system to open, in order.
    pub fn opened_urls(&self) -> Vec<String> {
        self.opened.urls()
    }

    /// The window system carries drags out of the window, for an engine
    /// made from now on; [`FakePlatform::install_drag_out`] gives it to
    /// one made already.
    pub fn carry_drags_out(&mut self) -> RecordedDragOut {
        self.drag_out
            .get_or_insert_with(RecordedDragOut::default)
            .clone()
    }

    pub fn install_drag_out(&self, engine: &Engine) {
        if let Some(out) = &self.drag_out {
            Platform::of(engine).set_drag_out(Some(Rc::new(out.clone())));
        }
    }

    /// The drags out of the window, when the window system carries them.
    pub fn drag_out(&self) -> Option<&RecordedDragOut> {
        self.drag_out.as_ref()
    }

    /// The system's dialogs cannot be closed by the application.
    pub fn dialogs_stay(&self) {
        self.dialogs.cannot_close();
    }

    /// The system asks the user before it grants a permission.
    pub fn permissions_ask(&mut self) {
        self.permissions
            .get_or_insert_with(FakePermissions::default);
    }

    /// The permissions the system has asked the user for, in order.
    pub fn permission_requests(&self) -> Vec<String> {
        self.permissions
            .as_ref()
            .map(|p| p.asked().into_iter().map(str::to_owned).collect())
            .unwrap_or_default()
    }

    /// The user answers the system's request for `name`.
    pub fn answer_permission(&self, name: &str, granted: bool) -> Result<()> {
        if self
            .permissions
            .as_ref()
            .is_some_and(|p| p.answer(name, granted))
        {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the system asks the user for no {name:?} permission"
            )))
        }
    }

    /// Writes `text` to the application's `name`, each line trimmed.
    pub fn write(&mut self, name: &str, text: &str) -> Result<()> {
        let path = self.path(name)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(failed)?;
        }
        // A doc string keeps the scenario's indentation.
        let text: String = text.lines().map(|l| format!("{}\n", l.trim())).collect();
        std::fs::write(&path, text).map_err(failed)
    }

    /// The text of the application's `name`, if it is there.
    pub fn read(&mut self, name: &str) -> Result<Option<String>> {
        let path = self.path(name)?;
        read_if_there(&path)
    }

    /// The system's dialogs shown so far: their kind and their title.
    pub fn shown(&self) -> Vec<(String, String)> {
        self.dialogs
            .shown()
            .into_iter()
            .map(|(kind, title)| (kind.describe().to_owned(), title))
            .collect()
    }

    /// The user answers the system's dialog that is open.
    pub fn answer(&mut self, answer: SystemDialogAnswer) -> Result<()> {
        let answered = match &answer {
            SystemDialogAnswer::Choose(paths) => self
                .dialogs
                .choose(paths.iter().map(PathBuf::from).collect()),
            SystemDialogAnswer::Button(text) => self.dialogs.answer(text),
            SystemDialogAnswer::Dismiss => self.dialogs.dismiss(),
        };
        if answered {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the system shows no dialog to answer with {answer:?}"
            )))
        }
    }
}

/// Everything the settings writer was given is on disk.
pub fn settle_writes(_engine: &Engine) {
    ljos_platform::flush_writes();
}

fn failed(e: impl std::fmt::Display) -> BackendError {
    BackendError::Failed(e.to_string())
}

fn sensor_named(name: &str) -> Result<SensorKind> {
    SensorKind::named(name).ok_or_else(|| {
        let known: Vec<_> = SensorKind::ALL.iter().map(|k| k.describe()).collect();
        failed(format!("no sensor is called {name:?}; there are {known:?}"))
    })
}

fn read_if_there(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(failed(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_are_written_and_read_in_a_directory_of_their_own() {
        let mut platform = FakePlatform::default();
        assert_eq!(platform.read("/notes.txt").unwrap(), None);
        platform
            .write("/home/user/notes.txt", "  first\n    second")
            .unwrap();
        assert_eq!(
            platform.read("/home/user/notes.txt").unwrap().as_deref(),
            Some("first\nsecond\n")
        );
        let root = platform.root().unwrap();
        assert!(root.join("home/user/notes.txt").is_file());
        assert_ne!(root, Path::new("/"));
    }

    #[test]
    fn nothing_asks_for_permission_until_the_scenario_says_so() {
        let mut platform = FakePlatform::default();
        assert!(platform.permission_requests().is_empty());
        assert!(platform.answer_permission("camera", true).is_err());
        platform.permissions_ask();
        assert!(platform.permission_requests().is_empty());
        assert!(platform.answer_permission("camera", true).is_err());
    }

    #[test]
    fn no_dialog_means_nothing_to_answer() {
        let mut platform = FakePlatform::default();
        assert!(platform.shown().is_empty());
        assert!(platform.answer(SystemDialogAnswer::Dismiss).is_err());
    }

    #[test]
    fn a_position_moves_only_once_there_is_one() {
        let mut platform = FakePlatform::default();
        assert!(platform.position_moves(1.0, 2.0, 0.0).is_err());
        platform.system_position(64.1, -21.9, 10.0);
        platform.position_moves(64.2, -21.8, 12.0).unwrap();
    }

    #[test]
    fn sensors_are_known_by_name() {
        let platform = FakePlatform::default();
        assert!(platform.add_sensor("no such sensor").is_err());
        let name = SensorKind::ALL[0].describe();
        platform.add_sensor(name).unwrap();
        platform.sensor_reads(name, 1, &[0.5]).unwrap();
    }

    #[test]
    fn nothing_is_opened_until_the_application_asks() {
        assert!(FakePlatform::default().opened_urls().is_empty());
    }
}
