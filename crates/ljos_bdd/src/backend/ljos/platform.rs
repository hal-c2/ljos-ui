//! The system a scenario's application runs on: a directory of its own
//! for every file it reads or writes, and dialogs the scenario answers as
//! the user. Nothing reaches the real home directory, clipboard or
//! dialogs.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use ljos_engine::Engine;
use ljos_platform::{
    FakeDialogs, FakePermissions, FakeSensors, HttpFetcher, Measurement, PathEnv, Platform,
    RecordedUrls, SensorKind,
};
use ljos_positioning::{FixedPosition, GeoCoordinate, PositionFix};

use super::{BackendError, Result, failed};
use crate::backend::SystemDialogAnswer;

/// The user's home, as the application sees it.
pub(super) const HOME: &str = "/home/user";

#[derive(Default)]
pub(super) struct Rig {
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
}

impl Rig {
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
    pub(super) fn install(&mut self, engine: &Engine) -> Result<()> {
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
        platform.set_held(true);
        Ok(())
    }

    pub(super) fn add_sensor(&self, name: &str) -> Result<()> {
        self.sensors.add(sensor_named(name)?);
        Ok(())
    }

    pub(super) fn sensor_reads(&self, name: &str, timestamp: u64, values: &[f64]) -> Result<()> {
        let kind = sensor_named(name)?;
        // A sensor that is not running is given nothing, as a device's
        // driver gives nothing to who does not listen.
        self.sensors.feed(kind, Measurement::new(timestamp, values));
        Ok(())
    }

    pub(super) fn system_position(&mut self, latitude: f64, longitude: f64, altitude: f64) {
        let fix = PositionFix::at(GeoCoordinate::new(latitude, longitude, altitude));
        self.position = Some(FixedPosition::new(fix));
    }

    pub(super) fn position_moves(
        &self,
        latitude: f64,
        longitude: f64,
        altitude: f64,
    ) -> Result<()> {
        let Some(position) = &self.position else {
            return Err(failed("say where the system is before it moves"));
        };
        position.move_to(PositionFix::at(GeoCoordinate::new(
            latitude, longitude, altitude,
        )));
        Ok(())
    }

    pub(super) fn opened_urls(&self) -> Vec<String> {
        self.opened.urls()
    }

    pub(super) fn dialogs_stay(&self) {
        self.dialogs.cannot_close();
    }

    pub(super) fn permissions_ask(&mut self) {
        self.permissions
            .get_or_insert_with(FakePermissions::default);
    }

    pub(super) fn permission_requests(&self) -> Vec<String> {
        self.permissions
            .as_ref()
            .map(|p| p.asked().into_iter().map(str::to_owned).collect())
            .unwrap_or_default()
    }

    pub(super) fn answer_permission(&self, name: &str, granted: bool) -> Result<()> {
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

    pub(super) fn write(&mut self, name: &str, text: &str) -> Result<()> {
        let path = self.path(name)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(failed)?;
        }
        // A doc string keeps the scenario's indentation.
        let text: String = text.lines().map(|l| format!("{}\n", l.trim())).collect();
        std::fs::write(&path, text).map_err(failed)
    }

    pub(super) fn read(&mut self, name: &str) -> Result<Option<String>> {
        let path = self.path(name)?;
        read_if_there(&path)
    }

    pub(super) fn shown(&self) -> Vec<(String, String)> {
        self.dialogs
            .shown()
            .into_iter()
            .map(|(kind, title)| (kind.describe().to_owned(), title))
            .collect()
    }

    pub(super) fn answer(&mut self, answer: SystemDialogAnswer) -> Result<()> {
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
pub(super) fn settle_writes(_engine: &Engine) {
    ljos_platform::flush_writes();
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
