//! The `@android` surface: a scenario run on a phone or an emulator, in
//! the application the runner installs there once
//! (`examples/android_scenarios`), driven over adb.
//!
//! Each scenario writes its files into that application's own files
//! directory (`run-as`), with its entry as `Main.qml`, and starts it anew
//! with its inspector. The application lays the files over the window
//! built in, as a user's files are laid over a shipped application's, so
//! a scenario takes no build. The computer reaches the inspector as
//! `ljos android connect` does (a port forwarded over adb, the token read
//! with `run-as`) and asks it through `ljos attach`, as the `@desktop`
//! backend asks its own; what the user and the system do to the
//! application (the home screen, a turn, Back, the system ending its
//! screen or its process) is adb's `input`, `settings` and `am`.
//!
//! `mise run features:android` installs the application and names the
//! device in `LJOS_BDD_ANDROID`, as `adb -s` takes it.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::attached::{Reached, Running, SEEN};
use super::{
    Backend, BackendError, Device, LogEntry, LogLevel, Platform, PointerStep, QmlFile, Result,
    SystemDialogAnswer,
};

/// Names the device the scenarios run on, as `adb -s` takes it; set when
/// one is there.
pub const ENV: &str = "LJOS_BDD_ANDROID";
/// The id of the application the scenarios run in, when it is not the
/// one `examples/android_scenarios` builds.
pub const ID_ENV: &str = "LJOS_BDD_ANDROID_ID";
/// What `ljos android run examples/android_scenarios` installs.
pub const DEFAULT_ID: &str = "dev.ljos.ljos_example_android_scenarios";
/// The activity the Gradle template (`android/`) declares.
const ACTIVITY: &str = "dev.ljos.LjosActivity";
/// The extra of the intent that starts the application with its inspector
/// (`LjosActivity`).
const MCP_EXTRA: &str = "dev.ljos.MCP";
/// What the runner asks of the inspector, as on the desktop.
const CAPS: &str = "inspect,input,mutate,eval";
/// How long the application may take to start and serve its inspector:
/// an emulator without a GPU of its own is slow to.
const STARTUP: Duration = Duration::from_secs(60);
/// Where the scenario's files go, below the application's data directory.
const FILES: &str = "files/qml";
/// What the application writes when the scenario's files do not compile.
const FAULT: &str = "files/scenario-fault.txt";
/// What lets the scenario's application read local files, when it is
/// there (`examples/android_scenarios`).
const XHR_LOCAL_READ: &str = "files/xhr-local-read";

/// The permissions `examples/android_scenarios` declares, which each
/// scenario starts from never asked for.
const DECLARED: [&str; 2] = [
    "android.permission.CAMERA",
    "android.permission.RECORD_AUDIO",
];
/// What the template's `LjosPermissions` remembers it asked for.
const ASKED: &str = "shared_prefs/dev.ljos.permissions.xml";
/// How long the system's question of a permission may take to come.
const ASKING: Duration = Duration::from_secs(5);

/// What `find` sees in what `read` gives, read again while it sees
/// nothing until `until`. That nothing was seen is said by a read made at
/// `until` or after it: one before it says nothing of a question that
/// comes later. A read that fails then is tried again for `grace` more,
/// and is an error after that: a screen not read proves nothing.
fn seen_until<S, T>(
    until: Instant,
    grace: Duration,
    mut read: impl FnMut() -> Result<S>,
    find: impl Fn(&S) -> Option<T>,
) -> Result<Option<T>> {
    loop {
        match read() {
            Ok(seen) => {
                if let Some(found) = find(&seen) {
                    return Ok(Some(found));
                }
                if Instant::now() > until {
                    return Ok(None);
                }
            }
            Err(e) if Instant::now() > until + grace => return Err(e),
            Err(_) => {}
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// The question on screen: its words, and the activity record that shows
/// it (`ActivityRecord{1f2e u0 …}`, from `dumpsys activity activities`).
/// Each request starts an activity of its own, so the same words asked
/// again, for another object or a stage after it, are another question.
#[derive(Debug, PartialEq)]
struct Shown {
    message: Option<String>,
    front: Option<String>,
}

impl Shown {
    fn of(nodes: &[ScreenNode], activities: &str) -> Self {
        Shown {
            message: nodes
                .iter()
                .find(|n| n.id.ends_with(":id/permission_message"))
                .map(|n| n.text.clone()),
            front: front_record(activities).map(str::to_owned),
        }
    }

    /// Whether the question `asked` showed has been answered: no question
    /// is on screen now, or another one is.
    fn answers(&self, asked: &Shown) -> bool {
        self.message.is_none() || self.message != asked.message || self.front != asked.front
    }
}

/// The record of the activity in front: what tells one started activity
/// from another of the same class.
fn front_record(activities: &str) -> Option<&str> {
    activities
        .lines()
        .find(|l| l.contains("topResumedActivity") || l.contains("ResumedActivity:"))?
        .split_once("ActivityRecord{")?
        .1
        .split(' ')
        .next()
}

/// The permission (as the steps name it) the system's question on screen
/// asks for, by its English words.
fn permission_asked(nodes: &[ScreenNode]) -> Option<&'static str> {
    let message = nodes
        .iter()
        .find(|n| n.id.ends_with(":id/permission_message"))?
        .text
        .to_lowercase();
    [
        ("take pictures", "camera"),
        ("record audio", "microphone"),
        ("location", "location"),
        ("calendar", "calendar"),
        ("contacts", "contacts"),
        ("nearby devices", "bluetooth"),
    ]
    .into_iter()
    .find(|(words, _)| message.contains(words))
    .map(|(_, name)| name)
}

/// Where the system's question has the button that grants, or denies: the
/// first of its buttons that does, as each version of it names them.
fn permission_button(nodes: &[ScreenNode], granted: bool) -> Option<(i64, i64)> {
    let ids: &[&str] = if granted {
        &[
            ":id/permission_allow_foreground_only_button",
            ":id/permission_allow_button",
        ]
    } else {
        &[
            ":id/permission_deny_button",
            ":id/permission_deny_and_dont_ask_again_button",
        ]
    };
    ids.iter().find_map(|id| {
        nodes
            .iter()
            .find(|n| n.id.ends_with(id) && n.enabled)
            .map(|n| n.at)
    })
}

/// The phone's shared Downloads, which its document picker offers.
const DOWNLOADS: &str = "/sdcard/Download";

/// Where `path` is in the Downloads, when it names something below them:
/// what a scenario puts there is removed again.
fn download_path(path: &str) -> Result<String> {
    let below = !path.contains(char::is_control)
        && path.split('/').all(|part| !matches!(part, "" | "." | ".."));
    if below {
        Ok(format!("{DOWNLOADS}/{path}"))
    } else {
        Err(BackendError::Failed(format!(
            "{path:?}: a download is named below the phone's Downloads"
        )))
    }
}

/// How long the document picker may take to show what a step wants, and
/// the application to hear its answer: a picked document is copied first.
const PICKER: Duration = Duration::from_secs(20);
/// Where the screen's nodes are written to be read.
const SCREEN: &str = "/data/local/tmp/ljos-screen.xml";
/// The id of a row's name in the picker's lists, and of the name's field
/// when it saves.
const TITLE: &str = "android:id/title";
/// The id of the picker's button that saves or takes a folder, and of the
/// button that then allows the folder.
const CONFIRM: &str = "android:id/button1";

/// The kind of dialog the document picker is, as the steps name it, when
/// it is in front in what `dumpsys activity activities` says.
fn picker_shown(activities: &str) -> Option<&'static str> {
    let front = activities.lines().any(|line| {
        (line.contains("topResumedActivity") || line.contains("ResumedActivity:"))
            && line.contains("documentsui/")
    });
    if !front {
        return None;
    }
    activities
        .lines()
        .filter(|line| line.contains("documentsui"))
        .find_map(|line| {
            let action = line.split_once("act=android.intent.action.")?.1;
            match action.split([' ', '}']).next()? {
                "OPEN_DOCUMENT" => Some("open file"),
                "CREATE_DOCUMENT" => Some("save file"),
                "OPEN_DOCUMENT_TREE" => Some("folder"),
                _ => None,
            }
        })
}

/// A node of the phone's screen, as `uiautomator dump` writes it.
#[derive(Debug)]
struct ScreenNode {
    text: String,
    id: String,
    class: String,
    desc: String,
    enabled: bool,
    /// Its middle, where it is tapped.
    at: (i64, i64),
}

fn screen_nodes(xml: &str) -> Vec<ScreenNode> {
    xml.split("<node ")
        .skip(1)
        .filter_map(|rest| {
            // An attribute's own `>` is written `&gt;`.
            let tag = format!(" {}", rest.split('>').next()?);
            let value = |name: &str| -> Option<String> {
                let (_, after) = tag.split_once(&format!(" {name}=\""))?;
                let raw = after.split('"').next()?;
                Some(
                    raw.replace("&quot;", "\"")
                        .replace("&apos;", "'")
                        .replace("&lt;", "<")
                        .replace("&gt;", ">")
                        .replace("&amp;", "&"),
                )
            };
            let bounds: Vec<i64> = value("bounds")?
                .split(['[', ']', ','])
                .filter_map(|n| n.parse().ok())
                .collect();
            let [left, top, right, bottom] = bounds[..] else {
                return None;
            };
            Some(ScreenNode {
                text: value("text").unwrap_or_default(),
                id: value("resource-id").unwrap_or_default(),
                class: value("class").unwrap_or_default(),
                desc: value("content-desc").unwrap_or_default(),
                enabled: value("enabled").as_deref() == Some("true"),
                at: ((left + right) / 2, (top + bottom) / 2),
            })
        })
        .collect()
}

/// The device the scenarios run on, when there is one.
#[must_use]
pub fn device() -> Option<String> {
    std::env::var(ENV).ok().filter(|d| !d.is_empty())
}

/// adb, talking to the one device and the one application.
#[derive(Clone, Debug)]
struct Phone {
    serial: String,
    id: String,
}

impl Phone {
    fn from_env() -> Result<Self> {
        let serial = device().ok_or_else(|| {
            BackendError::Failed(format!(
                "no phone ({ENV} is not set; `mise run features:android` names one)"
            ))
        })?;
        let id = std::env::var(ID_ENV)
            .ok()
            .filter(|i| !i.is_empty())
            .unwrap_or_else(|| DEFAULT_ID.to_owned());
        Ok(Self { serial, id })
    }

    /// adb in the SDK the developer names, else the one on the path.
    fn adb_program() -> PathBuf {
        std::env::var_os("ANDROID_HOME")
            .or_else(|| std::env::var_os("ANDROID_SDK_ROOT"))
            .map(|sdk| PathBuf::from(sdk).join("platform-tools/adb"))
            .filter(|adb| adb.is_file())
            .unwrap_or_else(|| PathBuf::from("adb"))
    }

    fn adb(&self) -> Command {
        let mut command = Command::new(Self::adb_program());
        command.args(["-s", &self.serial]);
        command
    }

    /// What `command`, run on the phone's shell, printed; its failure with
    /// what it said.
    fn shell(&self, command: &str) -> Result<String> {
        self.output(self.adb().args(["shell", command]), command, None)
    }

    /// As [`shell`](Self::shell), as the application (`run-as`), in its
    /// data directory, with `stdin` given to it.
    fn as_app(&self, script: &str, stdin: Option<&str>) -> Result<String> {
        let command = format!("run-as {} sh -c {}", self.id, quoted(script));
        let mut adb = self.adb();
        // exec-in hands stdin over as it is; shell may not.
        adb.args([if stdin.is_some() { "exec-in" } else { "shell" }, &command]);
        self.output(&mut adb, script, stdin)
    }

    fn output(&self, command: &mut Command, what: &str, stdin: Option<&str>) -> Result<String> {
        let mut child = command
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| BackendError::Failed(format!("adb: {e}")))?;
        if let Some(text) = stdin {
            let mut pipe = child.stdin.take().expect("piped");
            pipe.write_all(text.as_bytes())?;
        }
        let out = child.wait_with_output()?;
        let said = String::from_utf8_lossy(&out.stdout).replace('\r', "");
        if out.status.success() {
            Ok(said)
        } else {
            Err(BackendError::Failed(format!(
                "adb -s {} ({what}) {}: {}{}",
                self.serial,
                out.status,
                said.trim(),
                String::from_utf8_lossy(&out.stderr).trim()
            )))
        }
    }

    /// Whether the phone's shell finds something at `path`.
    fn has(&self, path: &str) -> Result<bool> {
        let said = self.shell(&format!("if [ -e {} ]; then echo yes; fi", quoted(path)))?;
        Ok(said.trim() == "yes")
    }

    fn picker(&self) -> Result<Option<&'static str>> {
        Ok(picker_shown(&self.shell("dumpsys activity activities")?))
    }

    /// Waits for the document picker: its kind, or none within [`PICKER`].
    fn wait_picker(&self) -> Result<Option<&'static str>> {
        let until = Instant::now() + PICKER;
        loop {
            let shown = self.picker()?;
            if shown.is_some() || Instant::now() > until {
                return Ok(shown);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn screen(&self) -> Result<Vec<ScreenNode>> {
        // uiautomator can fail and still exit with 0: an earlier screen's
        // file mustn't be read for this one's.
        self.shell(&format!(
            "rm -f {SCREEN} && uiautomator dump {SCREEN} >/dev/null && cat {SCREEN}"
        ))
        .map(|xml| screen_nodes(&xml))
    }

    /// Waits until the screen is as `find` wants it: where that says to tap.
    fn wait_screen(
        &self,
        what: &str,
        find: impl Fn(&[ScreenNode]) -> Option<(i64, i64)>,
    ) -> Result<(i64, i64)> {
        let until = Instant::now() + PICKER;
        loop {
            // A screen that is moving has no dump yet.
            let seen = self.screen();
            if let Some(at) = seen.as_deref().ok().and_then(&find) {
                return Ok(at);
            }
            if Instant::now() > until {
                return Err(BackendError::Failed(match seen {
                    Ok(nodes) => format!(
                        "the phone showed no {what}; its screen has {:?}",
                        nodes
                            .iter()
                            .map(|n| if n.text.is_empty() { &n.desc } else { &n.text })
                            .filter(|shown| !shown.is_empty())
                            .collect::<Vec<_>>()
                    ),
                    Err(e) => format!("looking for {what}, the screen was not read: {e}"),
                }));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn tap(&self, (x, y): (i64, i64)) -> Result<()> {
        self.shell(&format!("input tap {x} {y}")).map(drop)
    }

    /// Takes the picker to the Downloads, where the steps' files are; it
    /// opens where it was left.
    fn picker_to_downloads(&self) -> Result<()> {
        // The drawer's button has no id of its own, and English words.
        self.tap(self.wait_screen("button that shows the roots", |nodes| {
            nodes.iter().find(|n| n.desc == "Show roots").map(|n| n.at)
        })?)?;
        self.tap(self.wait_screen("Downloads among its roots", |nodes| {
            nodes
                .iter()
                .find(|n| n.id == TITLE && n.text == "Downloads")
                .map(|n| n.at)
        })?)
    }

    /// Picks `name`, which is in the Downloads, in the picker of `kind`.
    fn pick(&self, kind: &str, name: &str) -> Result<()> {
        self.picker_to_downloads()?;
        if kind == "save file" {
            // The name is the one the application suggested: none is typed.
            self.wait_screen(&format!("field with the name {name:?}"), |nodes| {
                nodes
                    .iter()
                    .find(|n| n.class.ends_with("EditText") && n.text == name)
                    .map(|n| n.at)
            })?;
            return self.tap(self.wait_screen("button that saves", |nodes| {
                nodes
                    .iter()
                    .find(|n| n.id == CONFIRM && n.enabled)
                    .map(|n| n.at)
            })?);
        }
        let row = |n: &ScreenNode| n.id == TITLE && n.text == name;
        self.tap(self.wait_screen(&format!("row {name:?}"), |nodes| {
            nodes.iter().find(|n| row(n)).map(|n| n.at)
        })?)?;
        if kind != "folder" {
            return Ok(());
        }
        // In the folder once its own row is gone: the button before that
        // would take the folder around it.
        self.tap(self.wait_screen("button that takes the folder", |nodes| {
            if nodes.iter().any(row) {
                return None;
            }
            nodes
                .iter()
                .find(|n| n.id == CONFIRM && n.enabled)
                .map(|n| n.at)
        })?)?;
        // The system then asks whether the application may have it.
        self.tap(
            self.wait_screen("question that allows the folder", |nodes| {
                if !nodes.iter().any(|n| n.id == "android:id/message") {
                    return None;
                }
                nodes.iter().find(|n| n.id == CONFIRM).map(|n| n.at)
            })?,
        )
    }

    /// Leaves the picker as the user does, by Back, which first leaves
    /// whatever is open in it.
    fn leave_picker(&self) -> Result<()> {
        let until = Instant::now() + PICKER;
        while self.picker()?.is_some() {
            if Instant::now() > until {
                return Err(BackendError::Failed(
                    "Back didn't leave the document picker".into(),
                ));
            }
            self.shell("input keyevent KEYCODE_BACK")?;
            // Another Back, once it is gone, would leave the application.
            if self.wait_front(true, Duration::from_secs(2))? {
                break;
            }
        }
        Ok(())
    }

    /// The application's process, when it runs.
    fn pid(&self) -> Result<Option<u32>> {
        // pidof says nothing, and fails, for a name nothing runs as.
        let said = self.shell(&format!("pidof {} || true", self.id))?;
        Ok(said.split_whitespace().next().and_then(|p| p.parse().ok()))
    }

    /// Starts the activity with the inspector; `anew` ends a running
    /// process first. The application reads the inspector's extra once,
    /// as its process starts.
    fn start(&self, anew: bool) -> Result<()> {
        let stop = if anew { "-S " } else { "" };
        self.shell(&format!(
            "am start -W {stop}--es {MCP_EXTRA} {CAPS} -n {}/{ACTIVITY}",
            self.id
        ))
        .map(drop)
    }

    /// Whether the application is the one the user sees.
    fn in_front(&self) -> Result<bool> {
        let said = self.shell("dumpsys activity activities")?;
        Ok(said
            .lines()
            .filter(|l| l.contains("topResumedActivity") || l.contains("ResumedActivity:"))
            .any(|l| l.contains(&format!("{}/", self.id))))
    }

    /// Waits until the application is in front, or behind, as `front`
    /// says, or until `within` has passed: whether it was.
    fn wait_front(&self, front: bool, within: Duration) -> Result<bool> {
        let until = Instant::now() + within;
        loop {
            if self.in_front()? == front {
                return Ok(true);
            }
            if Instant::now() > until {
                return Ok(false);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// The directory the system keeps for the application alone.
    fn data_dir(&self) -> Result<String> {
        Ok(self.as_app("pwd", None)?.trim().to_owned())
    }
}

/// `text` as one word of a POSIX shell.
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// The application's process on the phone, as `ljos attach` reaches it.
#[derive(Debug)]
struct OnPhone {
    phone: Phone,
    pid: u32,
}

impl Reached for OnPhone {
    fn state(&mut self) -> String {
        match self.phone.pid() {
            Ok(Some(pid)) if pid == self.pid => format!("runs on the phone as {pid}"),
            Ok(Some(pid)) => format!("ended on the phone; {pid} runs in its place"),
            Ok(None) => "ended on the phone".to_owned(),
            Err(e) => format!("not known ({e})"),
        }
    }

    fn end(&mut self) {
        // The next scenario starts it anew; the backend stops it on drop.
    }
}

/// A scenario's own directory in `parent`. `ljos android connect` puts
/// the inspector's token in it, and refuses one that others can enter.
fn scenario_dir(parent: &Path) -> std::io::Result<tempfile::TempDir> {
    let mut dir = tempfile::Builder::new();
    dir.prefix("android-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        dir.permissions(std::fs::Permissions::from_mode(0o700));
    }
    dir.tempdir_in(parent)
}

#[derive(Debug, Default)]
pub struct AndroidBackend {
    phone: Option<Phone>,
    running: Option<Running<OnPhone>>,
    /// Where the token file `ljos android connect` writes is kept.
    dir: Option<tempfile::TempDir>,
    /// Whether the scenario's application may read local files.
    xhr_local_read: bool,
    /// The files this scenario made in the phone's Downloads, and
    /// `download_dirs` the folders, outer before inner.
    downloads: Vec<String>,
    download_dirs: Vec<String>,
    /// The object the scenario last called `open` of: the dialog the
    /// picker answers.
    opened: Option<String>,
    /// The phone's clock as the scenario's application was started, in
    /// seconds: what its main and crash logs say after it is the
    /// scenario's.
    started_at: Option<String>,
}

impl Drop for AndroidBackend {
    fn drop(&mut self) {
        self.running = None;
        if let Some(phone) = &self.phone {
            let _ = phone.shell(&format!("am force-stop {}", phone.id));
            // A scenario that ended on its way through `end_screen` would
            // leave every application's screen ending as it is hidden.
            let _ = phone.shell("settings put global always_finish_activities 0");
            // Only what the scenario made: the Downloads are the user's.
            for file in &self.downloads {
                let _ = phone.shell(&format!("rm -f {}", quoted(file)));
            }
            for dir in self.download_dirs.iter().rev() {
                let _ = phone.shell(&format!("rmdir {}", quoted(dir)));
            }
        }
    }
}

impl AndroidBackend {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn phone(&mut self) -> Result<&Phone> {
        if self.phone.is_none() {
            self.phone = Some(Phone::from_env()?);
        }
        Ok(self.phone.as_ref().expect("just set"))
    }

    fn running(&mut self) -> Result<&mut Running<OnPhone>> {
        self.running
            .as_mut()
            .ok_or_else(|| BackendError::Failed("no application is running".into()))
    }

    fn dir(&mut self) -> Result<PathBuf> {
        if self.dir.is_none() {
            let features = crate::runner::target_dir().join("features");
            std::fs::create_dir_all(&features)?;
            self.dir = Some(scenario_dir(&features)?);
        }
        Ok(self.dir.as_ref().expect("just made").path().to_owned())
    }

    /// What the application said of the scenario's files, when they did
    /// not compile.
    fn fault(&mut self) -> Result<Option<String>> {
        let said = self
            .phone()?
            .as_app(&format!("cat {FAULT} 2>/dev/null || true"), None)?;
        Ok(Some(said).filter(|s| !s.trim().is_empty()))
    }

    /// Reaches the inspector of the running application as `ljos android
    /// connect` does, and asks it through `ljos attach`.
    fn connect(&mut self) -> Result<()> {
        self.running = None;
        let phone = self.phone()?.clone();
        let dir = self.dir()?;
        let token = dir.join("token.json");
        let ljos = super::testrun::ljos_bin()?;
        let deadline = Instant::now() + STARTUP;
        // The process serves its inspector a moment after it starts.
        let pid = loop {
            if let Some(fault) = self.fault()? {
                return Err(BackendError::Failed(format!(
                    "the scenario's files did not compile on the phone:\n{fault}"
                )));
            }
            let out = Command::new(&ljos)
                .current_dir(&dir)
                .args(["android", "connect", "--application-id", &phone.id])
                .args(["--device", &phone.serial])
                .arg("--out")
                .arg(&token)
                .stdin(Stdio::null())
                .output()?;
            if out.status.success()
                && let Some(pid) = phone.pid()?
            {
                break pid;
            }
            if Instant::now() > deadline {
                return Err(BackendError::Failed(format!(
                    "the application on the phone served no inspector in {} s; \
                     `ljos android connect` said: {}",
                    STARTUP.as_secs(),
                    String::from_utf8_lossy(&out.stderr).trim()
                )));
            }
            std::thread::sleep(Duration::from_millis(250));
        };
        let mut running = Running::attach(&ljos, &token, OnPhone { phone, pid }, (0, 0, 0, 0))?;
        running.ask("app_eval", json!({"source": "true"}))?;
        // The window built in shows only while the scenario's is not there.
        if running
            .ask("app_eval", json!({"source": "true", "target": "#waiting"}))
            .is_ok()
        {
            return Err(BackendError::Failed(
                "the application on the phone shows its own window, not the scenario's: \
                 its Main.qml was not laid over it"
                    .into(),
            ));
        }
        self.running = Some(running);
        Ok(())
    }

    /// The application, started anew, with the scenario's files.
    fn start(&mut self) -> Result<()> {
        self.phone()?.start(true)?;
        self.connect()
    }

    /// Waits until what was asked of the application has run.
    fn settle(&mut self) -> Result<()> {
        self.running()?.settle()
    }

    fn eval(&mut self, object: &str, source: &str) -> Result<Value> {
        self.running()?.eval(object, source)
    }

    /// Waits until the dialog has heard the picker: the picker is gone
    /// first, and a picked document is still copied then.
    fn heard(&mut self) -> Result<()> {
        let until = Instant::now() + PICKER;
        if !self.phone()?.wait_front(true, PICKER)? {
            return Err(BackendError::Failed(
                "the application didn't come back from the document picker".into(),
            ));
        }
        if let Some(dialog) = self.opened.take() {
            loop {
                // Counted before it is asked: an answer between the two
                // is a turn since.
                let turns = self.running()?.turns()?;
                if self.eval(&dialog, "visible")? != Value::Bool(true) {
                    break;
                }
                if Instant::now() > until {
                    return Err(BackendError::Failed(format!(
                        "the dialog {dialog:?} didn't hear the document picker's answer"
                    )));
                }
                self.running()?.turned_after(turns, until)?;
            }
        }
        self.settle()
    }
}

fn not_yet(what: &'static str) -> BackendError {
    BackendError::NotImplemented(what)
}

impl Backend for AndroidBackend {
    fn name(&self) -> &'static str {
        "android"
    }

    fn load(&mut self, files: &[QmlFile], entry: &str) -> Result<()> {
        self.running = None;
        let phone = self.phone()?.clone();
        phone.shell(&format!("am force-stop {}", phone.id))?;
        // Each scenario's application has never been asked for anything:
        // what an answer granted is taken back, and what it decided for
        // good is forgotten. An application that declares less has less
        // to take back.
        let forget: Vec<String> = DECLARED
            .iter()
            .map(|p| {
                format!(
                    "{{ pm revoke {id} {p}; pm clear-permission-flags {id} {p} user-set user-fixed; }} \
                     >/dev/null 2>&1",
                    id = phone.id
                )
            })
            .collect();
        phone.shell(&format!("{}; true", forget.join("; ")))?;
        phone.as_app(
            &format!("rm -rf {FILES} {FAULT} {XHR_LOCAL_READ} {ASKED} && mkdir -p {FILES}"),
            None,
        )?;
        if self.xhr_local_read {
            phone.as_app(&format!("touch {XHR_LOCAL_READ}"), None)?;
        }
        for f in files {
            let name = if f.name == entry {
                "Main.qml"
            } else {
                f.name.as_str()
            };
            if Path::new(name)
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
            {
                return Err(BackendError::Failed(format!(
                    "{name}: a scenario's file is named below its directory"
                )));
            }
            let path = format!("{FILES}/{name}");
            phone.as_app(
                &format!("mkdir -p \"$(dirname {0})\" && cat > {0}", quoted(&path)),
                Some(&f.source),
            )?;
        }
        // What the system logged before is another scenario's. The events
        // log is cleared, as the lifecycle steps read it whole; the main
        // and crash logs are the whole phone's, another application's crash
        // or another run's on the same phone, and are read from now on.
        phone.shell(FORGET_EVENTS)?;
        self.started_at = Some(phone.shell("date +%s.%N")?.trim().to_owned());
        self.start()
    }

    fn set(&mut self, object: &str, property: &str, value: &Value) -> Result<()> {
        self.running()?
            .ask(
                "app_set_property",
                json!({"target": format!("#{object}"), "property": property, "value": value}),
            )
            .map(drop)
    }

    fn bind(&mut self, object: &str, property: &str, expression: &str) -> Result<()> {
        self.running()?
            .ask(
                "app_set_property",
                json!({"target": format!("#{object}"), "property": property, "binding": expression}),
            )
            .map(drop)
    }

    fn get(&mut self, object: &str, property: &str) -> Result<Value> {
        self.eval(object, property)
    }

    fn call(&mut self, object: &str, method: &str, args: &[Value]) -> Result<Value> {
        if method == "open" {
            self.opened = Some(object.to_owned());
        }
        let args: Vec<String> = args.iter().map(Value::to_string).collect();
        self.eval(object, &format!("{method}({})", args.join(", ")))
    }

    fn exists(&mut self, object: &str) -> Result<bool> {
        Ok(self.eval(object, "true").is_ok())
    }

    fn advance(&mut self, ms: u64) -> Result<()> {
        // The phone's clock is real; the time is waited out, as far as
        // the application can tell from its own wakes.
        let deadline = Instant::now() + Duration::from_millis(ms);
        let quiet = |rest: &Value| match rest["wake_in_ms"].as_u64() {
            Some(n) => Instant::now() + Duration::from_millis(n) >= deadline,
            None => rest.is_object(),
        };
        self.running()?.rest_until(deadline, quiet)?;
        Ok(())
    }

    fn click(&mut self, _: &str, _: &str) -> Result<()> {
        Err(not_yet("a click by role and name on the phone"))
    }

    fn key(&mut self, _: &str) -> Result<()> {
        Err(not_yet("keys on the phone"))
    }

    fn pointer(&mut self, _: PointerStep, _: f64, _: f64) -> Result<()> {
        Err(not_yet("the pointer on the phone"))
    }

    fn a11y_tree(&mut self) -> Result<super::A11yNode> {
        Err(not_yet("the accessibility tree on the phone"))
    }

    fn log(&mut self) -> Result<Vec<LogEntry>> {
        let told = self.running()?.ask("app_logs", json!({"level": "info"}))?;
        let entries = told["entries"].as_array().cloned().unwrap_or_default();
        Ok(entries
            .iter()
            .filter_map(|e| {
                let level = match e["level"].as_str()? {
                    "debug" => LogLevel::Debug,
                    "info" => LogLevel::Info,
                    "warning" => LogLevel::Warning,
                    _ => LogLevel::Critical,
                };
                Some(LogEntry {
                    level,
                    message: e["message"].as_str()?.to_owned(),
                })
            })
            .collect())
    }

    fn grab(&mut self) -> Result<Vec<u8>> {
        Err(not_yet("a picture of the phone's screen"))
    }

    fn as_device(&mut self) -> Option<&mut dyn Device> {
        Some(self)
    }

    fn as_platform(&mut self) -> Option<&mut dyn Platform> {
        Some(self)
    }
}

impl Platform for AndroidBackend {
    fn xhr_local_read(&mut self) -> Result<()> {
        if self.running.is_some() {
            // It reads the word once, as it starts.
            return Err(not_yet(
                "letting a running application on the phone read local files",
            ));
        }
        self.xhr_local_read = true;
        Ok(())
    }

    fn system_dialogs(&mut self) -> Result<Vec<(String, String)>> {
        let phone = self.phone()?.clone();
        // A picker that was asked for takes a moment to come.
        let shown = if self.opened.is_some() {
            phone.wait_picker()?
        } else {
            phone.picker()?
        };
        // The picker shows no title.
        Ok(shown
            .map(|kind| (kind.to_owned(), String::new()))
            .into_iter()
            .collect())
    }

    fn answer_system_dialog(&mut self, answer: SystemDialogAnswer) -> Result<()> {
        let phone = self.phone()?.clone();
        let chosen = match &answer {
            SystemDialogAnswer::Button(_) => {
                return Err(not_yet("a button of the phone's own dialogs"));
            }
            SystemDialogAnswer::Dismiss => None,
            SystemDialogAnswer::Choose(paths) => match paths.as_slice() {
                [name] if !name.contains('/') => Some(name.as_str()),
                _ => {
                    return Err(not_yet(
                        "several documents, or a path, in the phone's picker",
                    ));
                }
            },
        };
        let kind = phone
            .wait_picker()?
            .ok_or_else(|| BackendError::Failed("the phone shows no document picker".into()))?;
        match chosen {
            None => phone.leave_picker()?,
            Some(name) => {
                if kind == "save file" {
                    let to = download_path(name)?;
                    if phone.has(&to)? {
                        return Err(BackendError::Failed(format!(
                            "the phone already holds {to}: the picker would save under another \
                             name, and finding that file would prove nothing"
                        )));
                    }
                    self.downloads.push(to);
                }
                phone.pick(kind, name)?;
            }
        }
        self.heard()
    }

    fn permissions_ask(&mut self) -> Result<()> {
        // A phone does.
        Ok(())
    }

    fn permission_requests(&mut self) -> Result<Vec<String>> {
        let phone = self.phone()?.clone();
        // The question is shown from the application's main thread, after
        // the request: a moment later.
        let asked = seen_until(
            Instant::now() + ASKING,
            ASKING,
            || phone.screen(),
            |nodes| permission_asked(nodes),
        )?;
        Ok(asked.map(|name| vec![name.to_owned()]).unwrap_or_default())
    }

    fn answer_permission(&mut self, name: &str, granted: bool) -> Result<()> {
        let phone = self.phone()?.clone();
        let how = if granted { "grants" } else { "denies" };
        let at = phone.wait_screen(
            &format!("question of the {name:?} permission with a button that {how} it"),
            |nodes| {
                (permission_asked(nodes) == Some(name))
                    .then(|| permission_button(nodes, granted))
                    .flatten()
            },
        )?;
        let shown = |phone: &Phone| -> Result<Shown> {
            let nodes = phone.screen()?;
            Ok(Shown::of(
                &nodes,
                &phone.shell("dumpsys activity activities")?,
            ))
        };
        let asked = shown(&phone)?;
        phone.tap(at)?;
        // The answer is taken once its question is gone. The application
        // need not be in front then: a request it queued behind this one
        // is asked next, with no frame of the application between them,
        // and may ask for the same permission.
        let answered = seen_until(
            Instant::now() + PICKER,
            ASKING,
            || shown(&phone),
            |now| now.answers(&asked).then_some(()),
        )?;
        if answered.is_none() {
            return Err(BackendError::Failed(format!(
                "the question of the {name:?} permission was still asked {} s after it was answered",
                PICKER.as_secs()
            )));
        }
        self.settle()
    }
}

/// What clears the system's events log as a scenario loads.
const FORGET_EVENTS: &str = "logcat -b events -c";

/// The logcat that reads what the application's user (`uid`) logged in the
/// main and crash logs since `since`, the phone's clock in seconds: to the
/// millisecond, as logcat takes it (`-t`, which implies `-d`). None for
/// what is not a user or a time, so that nothing else reaches the shell.
fn abort_query(uid: &str, since: &str) -> Option<String> {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(uid) {
        return None;
    }
    let (seconds, fraction) = since.split_once('.').unwrap_or((since, ""));
    if !digits(seconds) || !(fraction.is_empty() || digits(fraction)) {
        return None;
    }
    let millis: String = fraction.chars().chain("000".chars()).take(3).collect();
    Some(format!(
        "logcat -d -b main -b crash --uid={uid} -t {seconds}.{millis}"
    ))
}

/// What the system logs of a process that aborted: CheckJNI's refusal,
/// bionic's checks (`FORTIFY: pthread_mutex_lock called on a destroyed
/// mutex`), the signal it ended on, and the crash dump's message.
const ABORTED: [&str; 4] = [
    "JNI DETECTED ERROR",
    "FORTIFY:",
    "Fatal signal",
    "Abort message",
];

impl Device for AndroidBackend {
    fn leave_for_home(&mut self) -> Result<()> {
        let phone = self.phone()?.clone();
        phone.shell("input keyevent KEYCODE_HOME")?;
        if !phone.wait_front(false, SEEN)? {
            return Err(BackendError::Failed(format!(
                "the application was still in front {} s after Home",
                SEEN.as_secs()
            )));
        }
        Ok(())
    }

    fn open_again(&mut self) -> Result<()> {
        let phone = self.phone()?.clone();
        let before = self.running.as_ref().map(|r| r.app.pid);
        phone.start(false)?;
        if !phone.wait_front(true, STARTUP)? {
            return Err(BackendError::Failed(format!(
                "the application was not in front {} s after it was opened",
                STARTUP.as_secs()
            )));
        }
        // A new process is a new inspector; the one before went with the
        // process it was in.
        if before.is_none() || phone.pid()? != before {
            self.connect()?;
        }
        self.settle()
    }

    fn turn(&mut self, landscape: bool) -> Result<()> {
        let phone = self.phone()?.clone();
        let want = if landscape { "1" } else { "0" };
        let auto = phone.shell("settings get system accelerometer_rotation")?;
        let now = phone.shell("settings get system user_rotation")?;
        if auto.trim() == "0" && now.trim() == want {
            return Ok(());
        }
        let turns = self.running()?.turns()?;
        phone.shell("settings put system accelerometer_rotation 0")?;
        phone.shell(&format!("settings put system user_rotation {want}"))?;
        // The window draws at its new size once the system has turned it.
        let running = self.running()?;
        if !running.turned_after(turns, Instant::now() + SEEN)? {
            return Err(BackendError::Failed(format!(
                "the application drew nothing in {} s after the phone turned",
                SEEN.as_secs()
            )));
        }
        self.settle()
    }

    fn back(&mut self) -> Result<()> {
        self.phone()?.shell("input keyevent KEYCODE_BACK")?;
        // Back with nothing to go back to takes the application behind;
        // its inspector answers either way.
        self.settle()
    }

    fn end_screen(&mut self) -> Result<()> {
        // The system ends a hidden application's screen when it is short
        // of memory; told to end every screen it hides, it does so at
        // once. The application is hidden again the way the user hid it.
        let phone = self.phone()?.clone();
        phone.shell("settings put global always_finish_activities 1")?;
        let ended = (|| {
            phone.start(false)?;
            phone.wait_front(true, STARTUP)?;
            phone.shell("input keyevent KEYCODE_HOME")?;
            let until = Instant::now() + SEEN;
            loop {
                let said = phone.shell(&format!(
                    "dumpsys activity activities | grep -c {} || true",
                    phone.id
                ))?;
                if said.trim() == "0" {
                    return Ok(());
                }
                if Instant::now() > until {
                    return Err(BackendError::Failed(format!(
                        "the system kept the application's screen {} s after it was hidden",
                        SEEN.as_secs()
                    )));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        })();
        phone.shell("settings put global always_finish_activities 0")?;
        ended
    }

    fn running(&mut self) -> Result<bool> {
        Ok(self.phone()?.pid()?.is_some())
    }

    fn in_front(&mut self) -> Result<bool> {
        self.phone()?.in_front()
    }

    fn not_responding(&mut self) -> Result<bool> {
        let phone = self.phone()?.clone();
        // The system logs each "not responding" as `am_anr`; the log was
        // cleared as the scenario's files were written.
        let said = phone.shell("logcat -d -b events -s am_anr")?;
        Ok(said.lines().any(|l| l.contains(&phone.id)))
    }

    fn aborts(&mut self) -> Result<Vec<String>> {
        let phone = self.phone()?.clone();
        // By the application's user, not its process: the process that
        // aborted is gone. CheckJNI says what it refused in the main log,
        // bionic what it caught (`FORTIFY:`) and the signal it ended on in
        // the crash log, and the crash dump its abort message there too.
        // logcat keeps to one user's lines itself (`--uid`, Android 9).
        // Since the scenario loaded: what is before is not the scenario's.
        let uid = phone.as_app("id -u", None)?.trim().to_owned();
        let since = self
            .started_at
            .clone()
            .ok_or_else(|| BackendError::Failed("no scenario's application was started".into()))?;
        let query = abort_query(&uid, &since).ok_or_else(|| {
            BackendError::Failed(format!(
                "the application's user {uid:?} or the phone's clock {since:?} is not a number"
            ))
        })?;
        let said = phone.shell(&query)?;
        Ok(said
            .lines()
            .filter(|l| ABORTED.iter().any(|said| l.contains(said)))
            .map(str::to_owned)
            .collect())
    }

    fn connect_inspector(&mut self) -> Result<()> {
        self.connect()
    }

    fn end_process(&mut self) -> Result<()> {
        let phone = self.phone()?.clone();
        // `am kill` ends only a process the user does not see, as the
        // system does to save memory.
        phone.shell(&format!("am kill {}", phone.id))?;
        let until = Instant::now() + SEEN;
        while phone.pid()?.is_some() {
            if Instant::now() > until {
                return Err(BackendError::Failed(format!(
                    "the application's process still ran {} s after the system ended it",
                    SEEN.as_secs()
                )));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        self.running = None;
        Ok(())
    }

    fn own_directory(&mut self, path: &str) -> Result<bool> {
        let phone = self.phone()?.clone();
        let data = phone.data_dir()?;
        // The same directory by either of the names the system gives it.
        let own = [data.clone(), format!("/data/data/{}", phone.id)]
            .iter()
            .any(|d| path == d || path.starts_with(&format!("{d}/")));
        if !own {
            return Ok(false);
        }
        let said = phone.as_app(
            &format!("test -d {} && echo yes || true", quoted(path)),
            None,
        )?;
        Ok(said.trim() == "yes")
    }

    fn files_hold(&mut self, text: &str) -> Result<bool> {
        let said = self.phone()?.as_app(
            &format!("grep -rlF -- {} . 2>/dev/null || true", quoted(text)),
            None,
        )?;
        Ok(!said.trim().is_empty())
    }

    fn put_download(&mut self, path: &str, text: &str) -> Result<()> {
        let to = download_path(path)?;
        let phone = self.phone()?.clone();
        if !self.downloads.contains(&to) && phone.has(&to)? {
            return Err(BackendError::Failed(format!(
                "the phone already holds {to}, and the runner replaces no file of the phone's: \
                 remove it to run this scenario"
            )));
        }
        for (at, _) in path.match_indices('/') {
            let dir = download_path(&path[..at])?;
            if !phone.has(&dir)? {
                phone.shell(&format!("mkdir {}", quoted(&dir)))?;
                self.download_dirs.push(dir);
            }
        }
        let from = self.dir()?.join("download");
        std::fs::write(&from, text)?;
        let mut push = phone.adb();
        push.arg("push").arg(&from).arg(&to);
        if !self.downloads.contains(&to) {
            self.downloads.push(to.clone());
        }
        phone.output(&mut push, "push", None).map(drop)
    }

    fn download_exists(&mut self, path: &str) -> Result<bool> {
        let at = download_path(path)?;
        self.phone()?.has(&at)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BackendError, FORGET_EVENTS, Shown, abort_query, download_path, front_record,
        permission_asked, permission_button, picker_shown, quoted, scenario_dir, screen_nodes,
        seen_until,
    };

    // The main and crash logs are the whole phone's: another application's
    // crash, or another run's scenario on the same phone, is not ours to
    // clear. The abort is read from the scenario's start instead.
    #[test]
    fn the_aborts_are_read_from_the_scenarios_start_not_cleared() {
        assert_eq!(FORGET_EVENTS, "logcat -b events -c");
        assert_eq!(
            abort_query("10123", "1795595685.807349043").as_deref(),
            Some("logcat -d -b main -b crash --uid=10123 -t 1795595685.807")
        );
        assert_eq!(
            abort_query("10123", "1795595685").as_deref(),
            Some("logcat -d -b main -b crash --uid=10123 -t 1795595685.000")
        );
        assert_eq!(abort_query("10123", "%s.%N"), None);
        assert_eq!(abort_query("10123", ""), None);
        assert_eq!(abort_query("10 23", "1795595685"), None);
    }

    #[test]
    fn a_screen_never_read_is_not_one_that_asks_nothing() {
        use std::time::{Duration, Instant};
        let none = Duration::ZERO;
        let asked = |nodes: &Vec<super::ScreenNode>| permission_asked(nodes);
        let unread = || -> super::Result<Vec<super::ScreenNode>> {
            Err(BackendError::Failed(
                "uiautomator: could not get idle state".into(),
            ))
        };
        assert!(
            seen_until(Instant::now(), none, unread, asked).is_err(),
            "an unread screen is an error"
        );
        let read = seen_until(Instant::now(), none, || Ok(screen_nodes("")), asked);
        assert!(matches!(read, Ok(None)), "a read screen with no question");
        let mut reads = 0;
        let late = seen_until(
            Instant::now() + Duration::from_secs(5),
            none,
            || {
                reads += 1;
                if reads < 3 {
                    Err(BackendError::Failed("not yet".into()))
                } else {
                    Ok(screen_nodes(""))
                }
            },
            |_| Some("camera"),
        );
        assert!(matches!(late, Ok(Some("camera"))), "read once it could be");
        assert_eq!(reads, 3);
        // An empty screen before the deadline says nothing of a question
        // that comes after it, when no screen could be read.
        let mut reads = 0;
        let early = seen_until(
            Instant::now() + Duration::from_millis(300),
            none,
            || {
                reads += 1;
                if reads == 1 {
                    Ok(screen_nodes(""))
                } else {
                    Err(BackendError::Failed("not now".into()))
                }
            },
            asked,
        );
        assert!(reads > 1);
        assert!(
            early.is_err(),
            "only a read before the deadline saw nothing"
        );
        // A read that fails at the deadline is tried again for the grace.
        let until = Instant::now() + Duration::from_millis(300);
        let mut failed = false;
        let again = seen_until(
            until,
            Duration::from_secs(5),
            || {
                if Instant::now() > until && !failed {
                    failed = true;
                    Err(BackendError::Failed("not now".into()))
                } else {
                    Ok(screen_nodes(""))
                }
            },
            asked,
        );
        assert!(failed);
        assert!(matches!(again, Ok(None)), "read again after the deadline");
    }

    #[test]
    fn a_question_asked_again_is_not_the_one_answered() {
        let shown = |message: Option<&str>, front: &str| Shown {
            message: message.map(str::to_owned),
            front: Some(front.to_owned()),
        };
        let camera = Some("Allow Ljós to take pictures and record video?");
        let asked = shown(camera, "1f2e");
        assert!(!shown(camera, "1f2e").answers(&asked), "still asked");
        assert!(shown(None, "9a0b").answers(&asked), "gone");
        assert!(
            shown(camera, "9a0b").answers(&asked),
            "asked again, for another"
        );
        let all_the_time = Some("Allow Ljós to access this device's location all the time?");
        assert!(
            shown(all_the_time, "1f2e").answers(&asked),
            "the next stage"
        );
        let activities = "  ResumedActivity: ActivityRecord{1f2e u0 \
             com.google.android.permissioncontroller/.GrantPermissionsActivity t12}\n";
        assert_eq!(front_record(activities), Some("1f2e"));
        assert_eq!(front_record("nothing resumed\n"), None);
    }

    #[test]
    fn the_picker_in_front_says_its_kind() {
        let picker = "com.google.android.documentsui/com.android.documentsui.picker.PickActivity";
        let app = "dev.ljos.app/dev.ljos.LjosActivity";
        let screen = |action: &str, front: &str| {
            format!(
                "  Intent {{ act=android.intent.action.{action} cmp={picker} (has extras) }}\n  \
                 topResumedActivity=ActivityRecord{{1f2e u0 {front} t12}}\n"
            )
        };
        for (action, kind) in [
            ("OPEN_DOCUMENT", "open file"),
            ("CREATE_DOCUMENT", "save file"),
            ("OPEN_DOCUMENT_TREE", "folder"),
        ] {
            assert_eq!(picker_shown(&screen(action, picker)), Some(kind));
            assert_eq!(
                picker_shown(&screen(action, app)),
                None,
                "the picker is behind"
            );
        }
        assert_eq!(
            picker_shown(&screen("MAIN", picker)),
            None,
            "the Files application"
        );
    }

    #[test]
    fn a_screen_s_nodes_are_found_by_what_they_show() {
        let nodes = screen_nodes(concat!(
            r#"<?xml version='1.0'?><hierarchy rotation="0">"#,
            r#"<node text="" resource-id="" class="android.widget.FrameLayout" "#,
            r#"content-desc="Show roots" enabled="true" bounds="[0,0][100,50]">"#,
            r#"<node text="Q&amp;A &lt;1&gt;.txt" resource-id="android:id/title" "#,
            r#"class="android.widget.TextView" content-desc="" enabled="false" "#,
            r#"bounds="[10,200][30,260]" /></node></hierarchy>"#,
        ));
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].desc, "Show roots");
        assert!(nodes[0].enabled);
        assert_eq!(nodes[0].at, (50, 25));
        assert_eq!(nodes[1].text, "Q&A <1>.txt");
        assert_eq!(nodes[1].id, "android:id/title");
        assert_eq!(nodes[1].class, "android.widget.TextView");
        assert!(!nodes[1].enabled);
        assert_eq!(nodes[1].at, (20, 230));
    }

    #[test]
    fn the_systems_question_is_known_by_its_words_and_buttons() {
        let question = |message: &str, buttons: &[&str]| {
            let mut xml = format!(
                r#"<node text="{message}" resource-id="com.android.permissioncontroller:id/permission_message" class="android.widget.TextView" content-desc="" enabled="true" bounds="[0,0][10,10]" />"#
            );
            for (i, id) in buttons.iter().enumerate() {
                let top = 100 * (i + 1);
                xml.push_str(&format!(
                    r#"<node text="" resource-id="com.android.permissioncontroller:id/{id}" class="android.widget.Button" content-desc="" enabled="true" bounds="[0,{top}][100,{}]" />"#,
                    top + 50
                ));
            }
            screen_nodes(&xml)
        };
        let camera = question(
            "Allow ljos_example_android_scenarios to take pictures and record video?",
            &[
                "permission_allow_foreground_only_button",
                "permission_allow_one_time_button",
                "permission_deny_button",
            ],
        );
        assert_eq!(permission_asked(&camera), Some("camera"));
        assert_eq!(permission_button(&camera, true), Some((50, 125)));
        assert_eq!(permission_button(&camera, false), Some((50, 325)));
        let microphone = question(
            "Allow ljos_example_android_scenarios to record audio?",
            &[
                "permission_allow_button",
                "permission_deny_and_dont_ask_again_button",
            ],
        );
        assert_eq!(permission_asked(&microphone), Some("microphone"));
        assert_eq!(permission_button(&microphone, true), Some((50, 125)));
        assert_eq!(permission_button(&microphone, false), Some((50, 225)));
        assert_eq!(permission_asked(&screen_nodes("")), None);
    }

    #[test]
    fn a_download_is_named_below_the_downloads() {
        assert_eq!(
            download_path("letters/one.txt").expect("a file in a folder"),
            "/sdcard/Download/letters/one.txt"
        );
        for outside in [
            "",
            "/etc/hosts",
            "../DCIM",
            "a/../../b",
            "a//b",
            "a/",
            ".",
            "a\nb",
        ] {
            assert!(download_path(outside).is_err(), "{outside:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_scenario_s_directory_is_the_user_s_alone() {
        use std::os::unix::fs::PermissionsExt as _;
        let parent = tempfile::tempdir().unwrap();
        let dir = scenario_dir(parent.path()).unwrap();
        let mode = std::fs::metadata(dir.path()).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700, "mode {mode:o}");
    }

    #[test]
    fn a_quoted_word_keeps_its_quotes() {
        assert_eq!(quoted("plain"), "'plain'");
        assert_eq!(quoted("it's"), r"'it'\''s'");
    }
}
