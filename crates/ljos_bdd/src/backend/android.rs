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
use super::inspector::{Ask, Seen};
use super::{
    Backend, BackendError, Device, LogEntry, LogLevel, Platform, PointerStep, QmlFile, Result,
    SystemDialogAnswer, Tooling,
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
/// `Qt.ApplicationHidden` and `Qt.ApplicationActive`.
const HIDDEN: i64 = 1;
const ACTIVE: i64 = 4;
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
/// The count the scenario's application sets its badge to as it starts,
/// when it is there (`examples/android_scenarios`).
const BADGE: &str = "files/badge";
/// The channel of the notification whose number is the application's
/// badge (`showBadge` of the template's `LjosServices`).
const BADGE_CHANNEL: &str = "ljos.badge";

/// The permissions `examples/android_scenarios` declares, which each
/// scenario starts from never asked for.
const DECLARED: [&str; 3] = [
    "android.permission.CAMERA",
    "android.permission.RECORD_AUDIO",
    "android.permission.POST_NOTIFICATIONS",
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
        // When the read began: a screen captured before `until` says
        // nothing of what comes after it, however late the read returns.
        let began = Instant::now();
        match read() {
            Ok(seen) => {
                if let Some(found) = find(&seen) {
                    return Ok(Some(found));
                }
                if began >= until {
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

/// The count the system shows on `package`'s icon, from `dumpsys
/// notification --noredact`: the number of its notification in the badge
/// channel, when the system lets that notification show a badge (the
/// record's own `mShowBadge`, which weighs the channel and the user's
/// settings). 0 when there is none.
fn badge_in(dump: &str, package: &str) -> u32 {
    let pkg = format!(" pkg={package} ");
    let channel = format!("Notification(channel={BADGE_CHANNEL} ");
    dump.split("NotificationRecord(")
        .skip(1)
        .find(|record| {
            let head = record.lines().next().unwrap_or_default();
            format!(" {head}").contains(&pkg) && head.contains(&channel)
        })
        .map_or(0, |record| {
            let field = |name: &str| {
                record
                    .lines()
                    .find_map(|l| l.trim().strip_prefix(name).map(str::trim))
            };
            if field("mShowBadge=") == Some("true") {
                field("number=").and_then(|n| n.parse().ok()).unwrap_or(0)
            } else {
                0
            }
        })
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
        ("send you notifications", "notifications"),
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

/// The name the scenario application (examples/android_scenarios) puts
/// its `ScenarioSecrets` in the root's scope by: a secret goes through the
/// store the way the application's own would. The application makes the
/// object itself, since compiled QML has no compiler to make one from
/// text.
const SECRETS: &str = "scenarioSecrets";

/// How long the system's store of secrets may take to answer: the Keystore
/// makes its key the first time.
const SECRET_WAIT: Duration = Duration::from_secs(20);

/// The script, run in the scene's root, that calls `method` of the
/// scenario application's secrets with `args`.
fn secrets_call(method: &str, args: &[&str]) -> String {
    let args: Vec<String> = args.iter().map(|a| Value::from(*a).to_string()).collect();
    format!("{SECRETS}.{method}({})", args.join(", "))
}

/// What the store answered, read from `said`, the reply to the poll:
/// `None` while a call is under way, and the failure it gave as an error.
fn secrets_answer(said: &Value) -> Option<Result<(bool, String)>> {
    // The inspector cuts a long reply, a long secret among it, and says
    // so: what is left is no secret the store kept.
    if said["truncated"] == Value::Bool(true) {
        return Some(Err(BackendError::Failed(
            "the system's store of secrets answered more than comes back whole".into(),
        )));
    }
    let state = &said["value"];
    if state["busy"] != Value::Bool(false) {
        return None;
    }
    Some(match state["error"].as_str().filter(|e| !e.is_empty()) {
        Some(why) => Err(BackendError::Failed(format!(
            "the system's store of secrets: {why}"
        ))),
        None => Ok((
            state["found"] == Value::Bool(true),
            state["secret"].as_str().unwrap_or_default().to_owned(),
        )),
    })
}

/// The name the scenario application (examples/android_scenarios) puts
/// its `ScenarioClipboard` in the root's scope by: what the runner copies
/// goes through the system's clipboard as another application's copy
/// does.
const CLIPBOARD: &str = "scenarioClipboard";

/// How long the application may take to hear that the clipboard changed:
/// the system tells it through its main thread a moment after the copy.
const CLIPBOARD_HEARD: Duration = Duration::from_secs(2);

/// The script, run in the scene's root, that calls `method` of the
/// scenario application's clipboard with `args`.
fn clipboard_call(method: &str, args: &[Value]) -> String {
    let args: Vec<String> = args.iter().map(Value::to_string).collect();
    format!("{CLIPBOARD}.{method}({})", args.join(", "))
}

/// The width and height in `said`, the scenario clipboard's "width
/// height"; `None` for the empty answer of a clipboard without a picture.
fn picture_size(said: &str) -> Result<Option<(u32, u32)>> {
    if said.is_empty() {
        return Ok(None);
    }
    let size = said
        .split_once(' ')
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)));
    size.map(Some).ok_or_else(|| {
        BackendError::Failed(format!(
            "the clipboard's picture was said to be {said:?}, not a width and a height"
        ))
    })
}

/// The documents and folders whose permission the system keeps for
/// `package` past its restarts, as `dumpsys activity permissions` lists
/// them: each `UriPermission{… <url> [user N]}` line is followed by one
/// naming its `targetPkg`, then one with its `persisted` modes.
fn kept_permissions(dump: &str, package: &str) -> Vec<String> {
    let mut kept = Vec::new();
    let mut lines = dump.lines().map(str::trim);
    while let Some(line) = lines.next() {
        let Some(rest) = line.strip_prefix("UriPermission{") else {
            continue;
        };
        let Some(url) = rest.split_whitespace().nth(1) else {
            continue;
        };
        let (Some(target), Some(modes)) = (lines.next(), lines.next()) else {
            break;
        };
        let ours = target
            .split_whitespace()
            .any(|w| w.strip_prefix("targetPkg=") == Some(package));
        let persisted = modes
            .split_whitespace()
            .find_map(|w| w.strip_prefix("persisted="))
            .is_some_and(|m| m != "0x0");
        if ours && persisted && !kept.iter().any(|k| k == url) {
            kept.push(url.to_owned());
        }
    }
    kept
}

/// `text` with each `%XX` its byte: a provider's URL names the same
/// document escaped or not.
fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| char::from(b).to_digit(16);
        if let (b'%', Some(h), Some(l)) = (
            bytes[i],
            bytes.get(i + 1).copied().and_then(hex),
            bytes.get(i + 2).copied().and_then(hex),
        ) {
            out.push(u8::try_from(h * 16 + l).unwrap_or(b'%'));
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// How long the document picker may take to show what a step wants, and
/// the application to hear its answer: a picked document is copied first.
const PICKER: Duration = Duration::from_secs(20);
/// Where the screen's nodes are written to be read.
const SCREEN: &str = "/data/local/tmp/ljos-screen.xml";
/// The id of a row's name in the picker's lists, and of the name's field
/// when it saves.
const TITLE: &str = "android:id/title";
/// The end of the id of each crumb of the path the picker of a folder
/// shows: the picker is Google's or the open-source one, by phone.
const CRUMB: &str = "documentsui:id/breadcrumb_text";
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

    /// Goes to the Downloads in the picker of a folder. It shows no roots:
    /// it is in the phone's storage, or wherever it was left, and the
    /// Downloads there are the storage's folder "Download", since Android
    /// lets no application take the Downloads root itself as a folder.
    fn folder_picker_to_downloads(&self) -> Result<()> {
        // The first crumb of the path shown is the storage itself.
        self.tap(self.wait_screen("crumb of the phone's storage", |nodes| {
            nodes
                .iter()
                .filter(|n| n.id.ends_with(CRUMB))
                .min_by_key(|n| n.at.0)
                .map(|n| n.at)
        })?)?;
        self.tap(
            self.wait_screen("the storage's folder \"Download\"", |nodes| {
                nodes
                    .iter()
                    .find(|n| n.id == TITLE && n.text == "Download")
                    .map(|n| n.at)
            })?,
        )
    }

    /// Picks `name`, which is in the Downloads, in the picker of `kind`.
    fn pick(&self, kind: &str, name: &str) -> Result<()> {
        if kind == "folder" {
            self.folder_picker_to_downloads()?;
        } else {
            self.picker_to_downloads()?;
        }
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
    /// The count the application sets its badge to as it starts.
    badge: Option<u32>,
    /// The object the scenario last called `open` of: the dialog the
    /// picker answers.
    opened: Option<String>,
    /// The phone's clock as the scenario's application was started, in
    /// seconds: what its main and crash logs say after it is the
    /// scenario's.
    started_at: Option<String>,
    /// What the agent was last answered.
    seen: Seen,
    /// The phone's settings as the scenario found them, in the order it
    /// first changed them: put back, the last first, as it ends.
    saved: Vec<Setting>,
    /// The scenario left the developer option that ends every activity
    /// on: it is turned off as the scenario ends.
    finishing: bool,
    /// How many of the application's trims it had logged as the user
    /// last left it for the home screen.
    trims_before_home: usize,
    /// The port here the developer's last connect forwarded, as they
    /// disconnected or connected again to the application started anew.
    left_port: Option<u16>,
    /// `put_back` stopped the scenario's application: Drop, which can run
    /// as the next scenario runs the same application, does not again.
    stopped: bool,
}

impl Drop for AndroidBackend {
    fn drop(&mut self) {
        self.running = None;
        // The phone is still here only if `put_back` never ran (no `after`
        // hook) or left something undone: it takes the phone once done.
        if let Some(phone) = &self.phone {
            if self.stops_on_drop() {
                let _ = phone.shell(&format!("am force-stop {}", phone.id));
            }
            // A scenario that ended on its way through `end_screen` would
            // leave every application's screen ending as it is hidden.
            if self.finishing {
                let _ = phone.always_finish(false);
            }
            for put in restored(&self.saved) {
                let _ = phone.shell(&put);
            }
            // Only what the scenario made: the Downloads are the user's.
            for file in &self.downloads {
                let _ = phone.shell(&format!("rm -f {}", quoted(file)));
            }
            for dir in self.download_dirs.iter().rev() {
                let _ = phone.shell(&format!("rmdir {}", quoted(dir)));
            }
            let _ = self.disconnect(phone);
        }
    }
}

impl AndroidBackend {
    /// Stops the application and puts back the settings the scenario
    /// changed, then reads them: an error says which read otherwise.
    fn left_as_found(&mut self, phone: &Phone) -> Result<()> {
        let mut failed = Vec::new();
        match phone.shell(&format!("am force-stop {}", phone.id)) {
            Ok(_) => self.stopped = true,
            Err(e) => failed.push(format!("the application was not stopped: {e}")),
        }
        let mut finish = || phone.always_finish(false);
        let left = tear_down(
            std::mem::take(&mut self.saved),
            self.finishing,
            &mut |command| phone.shell(command),
            &mut finish,
            &mut settles(),
        );
        self.saved = left.saved;
        self.finishing = left.finishing;
        failed.extend(left.failed);
        if failed.is_empty() {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "the phone was not left as the scenario found it: {}",
                failed.join("; ")
            )))
        }
    }

    /// Notes `key` as the scenario found it, unless it was noted already:
    /// called before the scenario first changes it.
    fn save(&mut self, phone: &Phone, namespace: &'static str, key: &'static str) -> Result<()> {
        if self
            .saved
            .iter()
            .any(|s| s.namespace == namespace && s.key == key)
        {
            return Ok(());
        }
        let was = phone.shell(&format!("settings get {namespace} {key}"))?;
        self.saved.push(Setting {
            namespace,
            key,
            was: was.trim().to_owned(),
        });
        Ok(())
    }

    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Waits until the application says it is in `state`
    /// (`Qt.application.state`), as the system told it.
    fn wait_state(&mut self, state: i64, what: &str) -> Result<()> {
        let running = self.running()?;
        let until = Instant::now() + SEEN;
        loop {
            let said = running.ask("app_eval", json!({"source": "Qt.application.state"}))?;
            if said["value"].as_i64() == Some(state) {
                return Ok(());
            }
            if Instant::now() > until {
                return Err(BackendError::Failed(format!(
                    "the application did not say it was {what} {} s after the system made it so; \
                     its Qt.application.state is {}",
                    SEEN.as_secs(),
                    said["value"]
                )));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// The bytes each of the application's trims gave back, as it logged
    /// them, oldest first. The shell logs them to the phone's log, not to
    /// the application's own (`app_logs`), which holds what its QML and
    /// scripts say.
    fn trims(&mut self) -> Result<Vec<usize>> {
        Ok(self.logged()?.lines().filter_map(trimmed_bytes).collect())
    }

    /// What the application's user logged in the phone's main and crash
    /// logs since the scenario loaded: what is before is not the
    /// scenario's. By the user, not the process, so that the lines of a
    /// process that is gone are there too; logcat keeps to one user's
    /// lines itself (`--uid`, Android 9).
    fn logged(&mut self) -> Result<String> {
        let phone = self.phone()?.clone();
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
        phone.shell(&query)
    }

    /// Fails unless the application rests with nothing to do and nothing
    /// wakes it: no wake asked for, no frame owed, nothing animating, and
    /// no pass run while it is watched. The watch waits on the
    /// inspector's thread, so it wakes the application for nothing.
    fn rests(&mut self) -> Result<()> {
        let running = self.running()?;
        // What leaving it set going (its surface going, the system's
        // trim) runs first.
        running.rest_until(Instant::now() + SEEN, |rest| not_idle(rest).is_none())?;
        stays_idle(running)
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

    /// The port here the developer's last connect forwarded, as its token
    /// file names it.
    fn connected_port(&mut self) -> Result<u16> {
        let token = self.dir()?.join("token.json");
        let text = std::fs::read_to_string(&token).map_err(|e| {
            BackendError::Failed(format!("no connect wrote {}: {e}", token.display()))
        })?;
        let body = ljos_mcp_core::protocol::TokenFileBody::parse(&text)
            .map_err(|e| BackendError::Failed(format!("{}: {e}", token.display())))?;
        url_port(&body.url).ok_or_else(|| {
            BackendError::Failed(format!("the connect's address {} has no port", body.url))
        })
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
        self.reach(false)
    }

    /// As [`connect`](Self::connect); `anew` when the application was
    /// just started anew, so starting it once more changes nothing the
    /// scenario did.
    fn reach(&mut self, anew: bool) -> Result<()> {
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
            // The inspector's port, chosen on the phone, must be the same
            // here, and here it is another's: another phone's forward, not
            // this scenario's to remove, or another program's. A new
            // process listens at another port.
            if anew && port_taken_here(&String::from_utf8_lossy(&out.stderr)) {
                phone.start(true)?;
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
        self.reach(true)
    }

    /// Whether Drop stops the application: only when `put_back` did not.
    fn stops_on_drop(&self) -> bool {
        !self.stopped
    }

    /// Whether the tidying left nothing for Drop to try again, the
    /// inspector's forward going as `forward` says: a forward left stays
    /// until adb restarts.
    fn nothing_left(&self, forward: &Result<()>) -> bool {
        self.saved.is_empty() && !self.finishing && forward.is_ok()
    }

    /// Undoes the developer's connect as `ljos android disconnect` does:
    /// the forward it made goes, unless another phone or application
    /// holds that port now.
    fn disconnect(&self, phone: &Phone) -> Result<()> {
        let Some(dir) = &self.dir else {
            return Ok(());
        };
        let token = dir.path().join("token.json");
        if !token.exists() {
            return Ok(());
        }
        let out = Command::new(super::testrun::ljos_bin()?)
            .current_dir(dir.path())
            .args(["android", "disconnect", "--device", &phone.serial])
            .arg("--out")
            .arg(&token)
            .stdin(Stdio::null())
            .output()?;
        if out.status.success() {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "`ljos android disconnect` said: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )))
        }
    }

    /// Waits until what was asked of the application has run.
    fn settle(&mut self) -> Result<()> {
        self.running()?.settle()
    }

    fn eval(&mut self, object: &str, source: &str) -> Result<Value> {
        self.running()?.eval(object, source)
    }

    /// The handle of the scene's root, where the scenario application's
    /// own objects are in scope.
    fn root(&mut self) -> Result<String> {
        let roots = self
            .running()?
            .ask("app_objects", json!({"depth": 0, "max": 1}))?;
        roots["objects"][0]["handle"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| {
                BackendError::Failed(
                    "the scene has no root object to reach the scenario application's objects in"
                        .into(),
                )
            })
    }

    /// Calls `method` of the scenario application's secrets and waits
    /// for the store's answer: whether it found (or removed) one, and the
    /// secret it read.
    fn secrets(&mut self, method: &str, args: &[&str]) -> Result<(bool, String)> {
        let root = self.root()?;
        let running = self.running()?;
        running.ask(
            "app_eval",
            json!({"source": secrets_call(method, args), "target": root}),
        )?;
        let state = format!(
            "({{busy: {SECRETS}.busy, found: {SECRETS}.found, secret: {SECRETS}.secret, error: {SECRETS}.error}})"
        );
        let until = Instant::now() + SECRET_WAIT;
        loop {
            let said = running.ask("app_eval", json!({"source": state, "target": root}))?;
            if let Some(answer) = secrets_answer(&said) {
                return answer;
            }
            if Instant::now() > until {
                return Err(BackendError::Failed(format!(
                    "the system's store of secrets didn't answer {method}() in {} s",
                    SECRET_WAIT.as_secs()
                )));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Runs `source` in the scene's root, where the scenario
    /// application's own objects are in scope: the value, whole.
    fn in_root(&mut self, source: &str) -> Result<Value> {
        let root = self.root()?;
        let running = self.running()?;
        let said = running.ask("app_eval", json!({"source": source, "target": root}))?;
        if said["truncated"] == Value::Bool(true) {
            return Err(BackendError::Failed(format!(
                "the inspector cut the answer to {source}"
            )));
        }
        Ok(said["value"].clone())
    }

    /// Calls `method` of the scenario application's clipboard, which
    /// answers what went wrong, or nothing.
    fn clipboard_does(&mut self, method: &str, args: &[Value]) -> Result<()> {
        let said = self.in_root(&clipboard_call(method, args))?;
        match said.as_str() {
            Some("") => {}
            Some(why) => return Err(BackendError::Failed(format!("the clipboard: {why}"))),
            None => {
                return Err(BackendError::Failed(format!(
                    "the clipboard's {method}() answered {said}"
                )));
            }
        }
        // The system tells every application of a copy, this one too,
        // through its main thread a turn after the copy's; a step after
        // reads what it heard. Heard already, the wait is only longer.
        let until = Instant::now() + CLIPBOARD_HEARD;
        let turns = self.running()?.turns()?;
        self.running()?.turned_after(turns, until)?;
        self.settle()
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

    /// All of the scenario's tidying is done here, at its end, and none
    /// is left to Drop: cucumber hands the finished world to its writer
    /// (`world.map(Arc::new)` in cucumber 0.23's runner), so Drop runs
    /// whenever the writer lets go of it, which can be while the next
    /// scenario turns the phone or runs the same application. The
    /// application is stopped first, so nothing it does on its way out
    /// comes after the settings are put back.
    fn put_back(&mut self) -> Result<()> {
        self.running = None;
        let Some(phone) = self.phone.clone() else {
            return Ok(());
        };
        let left = self.left_as_found(&phone);
        // Every scenario has a token file of its own, so no later connect
        // removes this one's forward, and adb keeps it until it restarts:
        // a port forwarded here for any phone refuses the next scenario's
        // inspector there (#822).
        let forward = self.disconnect(&phone);
        let done = self.nothing_left(&forward);
        let left = match (left, forward) {
            (Ok(()), forward) => forward,
            (Err(BackendError::Failed(why)), Err(e)) => {
                Err(BackendError::Failed(format!("{why}; {e}")))
            }
            (left, _) => left,
        };
        // Only what the scenario made: the Downloads are the user's.
        for file in self.downloads.drain(..) {
            let _ = phone.shell(&format!("rm -f {}", quoted(&file)));
        }
        for dir in self.download_dirs.drain(..).rev() {
            let _ = phone.shell(&format!("rmdir {}", quoted(&dir)));
        }
        // What failed is tried once more by Drop, late as it may be: the
        // scenario has failed already, and a phone left changed is worse.
        if done {
            self.phone = None;
        }
        left
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
            &format!("rm -rf {FILES} {FAULT} {XHR_LOCAL_READ} {BADGE} {ASKED} && mkdir -p {FILES}"),
            None,
        )?;
        if self.xhr_local_read {
            phone.as_app(&format!("touch {XHR_LOCAL_READ}"), None)?;
        }
        if let Some(count) = self.badge {
            phone.as_app(&format!("echo {count} > {BADGE}"), None)?;
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

    fn as_tooling(&mut self) -> Option<&mut dyn Tooling> {
        Some(self)
    }
}

impl Tooling for AndroidBackend {
    fn inspector(&mut self, ask: &Ask) -> Result<()> {
        match ask {
            Ask::Call { tool, args } => {
                let answer = self.running()?.call(tool, args.clone())?;
                self.seen.answer = Some(answer);
                Ok(())
            }
            Ask::Rests => self.rests(),
            _ => Err(BackendError::Unsupported {
                backend: "android",
                what: "an agent here only calls tools and sees the application rest",
            }),
        }
    }

    fn inspector_seen(&mut self) -> Result<Seen> {
        Ok(self.seen.clone())
    }
}

impl Platform for AndroidBackend {
    fn set_clipboard(&mut self, text: &str) -> Result<()> {
        self.clipboard_does("copyElsewhere", &[Value::from(text)])
    }

    fn clipboard(&mut self) -> Result<String> {
        match self.in_root(&clipboard_call("text", &[]))? {
            Value::String(text) => Ok(text),
            said => Err(BackendError::Failed(format!(
                "the system's clipboard could not be read: it answered {said}"
            ))),
        }
    }

    fn copy_picture(&mut self, width: u32, height: u32) -> Result<()> {
        self.clipboard_does("copyPicture", &[Value::from(width), Value::from(height)])
    }

    fn clipboard_picture(&mut self) -> Result<Option<(u32, u32)>> {
        match self.in_root(&clipboard_call("picture", &[]))? {
            Value::String(said) => picture_size(&said),
            said => Err(BackendError::Failed(format!(
                "the clipboard's picture() answered {said}"
            ))),
        }
    }

    fn badge_at_start(&mut self, count: u32) -> Result<()> {
        if self.running.is_some() {
            // It reads the count once, as it starts.
            return Err(not_yet(
                "setting the badge of a running application on the phone",
            ));
        }
        self.badge = Some(count);
        Ok(())
    }

    fn badge_shown(&mut self) -> Result<u32> {
        let phone = self.phone()?.clone();
        // The application posts its notification from the main thread, a
        // moment after it sets the count; a count not there by then is
        // not shown.
        let shown = seen_until(
            Instant::now() + ASKING,
            ASKING,
            || phone.shell("dumpsys notification --noredact"),
            |dump: &String| Some(badge_in(dump, &phone.id)).filter(|n| *n != 0),
        )?;
        Ok(shown.unwrap_or(0))
    }

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
            |nodes: &Vec<ScreenNode>| permission_asked(nodes),
        )?;
        Ok(asked.map(|name| vec![name.to_owned()]).unwrap_or_default())
    }

    fn answer_permission(&mut self, name: &str, granted: bool) -> Result<()> {
        let phone = self.phone()?.clone();
        let how = if granted { "grants" } else { "denies" };
        let shown = |phone: &Phone| -> Result<(Vec<ScreenNode>, String)> {
            let nodes = phone.screen()?;
            Ok((nodes, phone.shell("dumpsys activity activities")?))
        };
        // Where to tap and what was asked, from the one screen: read again
        // while it moves, as the answer's is after it.
        let found = seen_until(
            Instant::now() + PICKER,
            ASKING,
            || shown(&phone),
            |(nodes, activities): &(Vec<ScreenNode>, String)| {
                (permission_asked(nodes) == Some(name))
                    .then(|| permission_button(nodes, granted))
                    .flatten()
                    .map(|at| (at, Shown::of(nodes, activities)))
            },
        )?;
        let Some((at, asked)) = found else {
            return Err(BackendError::Failed(format!(
                "the phone showed no question of the {name:?} permission with a button that {how} it within {} s",
                PICKER.as_secs()
            )));
        };
        phone.tap(at)?;
        // The answer is taken once its question is gone. The application
        // need not be in front then: a request it queued behind this one
        // is asked next, with no frame of the application between them,
        // and may ask for the same permission.
        let answered = seen_until(
            Instant::now() + PICKER,
            ASKING,
            || shown(&phone),
            |(nodes, activities): &(Vec<ScreenNode>, String)| {
                Shown::of(nodes, activities).answers(&asked).then_some(())
            },
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

/// The developer option the system ends a hidden activity at once by
/// (`always_finish_activities`), as its Settings screen names it.
const DONT_KEEP: &str = "Don't keep activities";

/// Whether a node of the screen is the switch's row: the Settings
/// application writes the apostrophe as a typographer's.
fn is_dont_keep(node: &ScreenNode) -> bool {
    node.text.replace('\u{2019}', "'") == DONT_KEEP
}

/// How many of `package`'s activities the system still keeps a screen of,
/// in `dumpsys activity activities`: the records in its history (`Hist`)
/// not destroyed. A destroyed one stays listed until its task goes, with
/// no process (`app=null`) and `state=DESTROYED`.
fn screens_kept(activities: &str, package: &str) -> usize {
    let mut kept = 0;
    let mut record: Option<bool> = None;
    let theirs = format!(" {package}/");
    for line in activities.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("* ") {
            kept += usize::from(record.take().unwrap_or(false));
            if trimmed.starts_with("* Hist ") && trimmed.contains(&theirs) {
                record = Some(true);
            }
        } else if let Some(alive) = record.as_mut()
            && (trimmed.starts_with("app=null") || trimmed.starts_with("state=DESTROYED"))
        {
            *alive = false;
        }
    }
    kept + usize::from(record.unwrap_or(false))
}

/// One of the phone's settings as `settings get` read it before the
/// scenario changed it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Setting {
    /// `system`, `secure` or `global`.
    namespace: &'static str,
    key: &'static str,
    was: String,
}

/// The commands that put `saved` back, the last changed first. A key that
/// was unset reads `null`, and is deleted again rather than set to that
/// word. The rotation is put back by the window manager, which writes both
/// of its settings itself: put through `settings`, its own record of the
/// lock it was turned with can be written back after the new value, as a
/// slow emulator showed (#823).
fn restored(saved: &[Setting]) -> Vec<String> {
    saved
        .iter()
        .rev()
        .map(
            |Setting {
                 namespace,
                 key,
                 was,
             }| match (*namespace, *key, was.as_str()) {
                (_, _, "null" | "") => format!("settings delete {namespace} {key}"),
                ("system", "user_rotation", turn @ ("0" | "1" | "2" | "3")) => {
                    format!("cmd window user-rotation lock {turn}")
                }
                ("system", "accelerometer_rotation", "1") => "cmd window user-rotation free".into(),
                ("system", "accelerometer_rotation", "0") => "cmd window user-rotation lock".into(),
                (_, _, was) => format!("settings put {namespace} {key} {}", quoted(was)),
            },
        )
        .collect()
}

/// How long a setting put back has to read as it was, and the display
/// to turn back to the lock put back: a slow emulator's turn takes
/// seconds.
const SETTLES_IN: Duration = Duration::from_secs(5);

/// [`tear_down`]'s pause on the phone: 50 ms at a time, [`SETTLES_IN`] in
/// all for each wait.
fn settles() -> impl FnMut(u32) -> bool {
    let mut until = Instant::now();
    move |tried| {
        if tried == 0 {
            until = Instant::now() + SETTLES_IN;
        }
        if Instant::now() >= until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
        true
    }
}

/// What a scenario's tidying could not do: the settings still to put
/// back, whether every activity still ends as it is left, and why.
#[derive(Debug, PartialEq, Eq)]
struct Left {
    saved: Vec<Setting>,
    finishing: bool,
    failed: Vec<String>,
}

/// Turns `always_finish` off (`finish_off`) when `finishing`, puts back
/// each of `saved`, the last changed first, and reads it back, through
/// `shell`. Each step is tried whatever an earlier one did: a switch the
/// Settings screen did not show still leaves the rotation put back. What
/// failed stays in [`Left`] for another try. Each is read back before the
/// next is put: the window manager turns a lock a moment after `cmd`
/// returns, and that late write would undo a `free` put after it.
/// A lock put back is also waited for until the display has turned to it,
/// since the window manager writes the lock again as the turn ends.
/// `pause(tried)` waits a moment more for the setting to read as it was put,
/// or the display to turn (`tried` 0 as it is first read again), false once
/// it is too late.
fn tear_down(
    saved: Vec<Setting>,
    finishing: bool,
    shell: &mut dyn FnMut(&str) -> Result<String>,
    finish_off: &mut dyn FnMut() -> Result<()>,
    pause: &mut dyn FnMut(u32) -> bool,
) -> Left {
    let mut failed = Vec::new();
    let mut finishing = finishing;
    if finishing {
        match finish_off() {
            Ok(()) => finishing = false,
            Err(e) => failed.push(e.to_string()),
        }
    }
    let mut undone = vec![false; saved.len()];
    for (at, setting) in saved.iter().enumerate().rev() {
        let command = restored(std::slice::from_ref(setting)).remove(0);
        if let Err(e) = shell(&command) {
            failed.push(format!("`{command}` failed: {e}"));
            undone[at] = true;
            continue;
        }
        let mut tried = 0;
        let off = loop {
            let read = |s: &Setting| shell(&format!("settings get {} {}", s.namespace, s.key));
            match not_put_back(std::slice::from_ref(setting), read) {
                Ok(Some(_)) if pause(tried) => tried += 1,
                Ok(off) => break off,
                Err(e) => {
                    break Some(format!(
                        "{} {} could not be read back: {e}",
                        setting.namespace, setting.key
                    ));
                }
            }
        };
        if let Some(why) = off {
            // A rotation that does not read as put back says what the
            // window manager holds and where the display is turned: a
            // late write of its own reads differently from a lost `cmd`.
            let why = if setting.namespace == "system" && ROTATION.contains(&setting.key) {
                format!("{why} ({})", rotation_now(shell))
            } else {
                why
            };
            failed.push(why);
            undone[at] = true;
        } else if let Some(turn) = locked_to(&command) {
            // The settings read as locked before the display has turned
            // back, and the window manager writes the lock again as the
            // turn ends: a `free` put before then is locked once more
            // (lifecycle.feature:65 after the landscape outline).
            let mut tried = 0;
            while display_turn(&shell("dumpsys display").unwrap_or_default())
                .is_some_and(|now| now != turn)
                && pause(tried)
            {
                tried += 1;
            }
        }
    }
    // Turning the switch off again turns the developer settings on, so a
    // retry of it needs every value, put back already or not. What is
    // kept keeps the order it was changed in, as `restored` takes it.
    let saved = saved
        .into_iter()
        .zip(undone)
        .filter(|(_, undone)| finishing || *undone)
        .map(|(setting, _)| setting)
        .collect();
    Left {
        saved,
        finishing,
        failed,
    }
}

/// The settings the window manager writes as the phone is locked or freed.
const ROTATION: [&str; 2] = ["accelerometer_rotation", "user_rotation"];

/// The turn `command` locks the phone to, if it is a lock to one.
fn locked_to(command: &str) -> Option<&str> {
    command
        .strip_prefix("cmd window user-rotation lock ")
        .filter(|turn| matches!(*turn, "0" | "1" | "2" | "3"))
}

/// The turn of the default display in `dumpsys display`, as the window
/// manager last gave it: the first logical display's `rotation`. `None`
/// when the dump says none, and nothing is waited for.
fn display_turn(dump: &str) -> Option<&str> {
    let (_, info) = dump.split_once("mOverrideDisplayInfo=DisplayInfo{")?;
    let line = info.lines().next()?;
    let (_, turn) = line.split_once(", rotation ")?;
    let end = turn
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(turn.len());
    (end > 0).then(|| &turn[..end])
}

/// What the window manager holds of the rotation and where the display
/// is turned, for a rotation that did not read as put back.
fn rotation_now(shell: &mut dyn FnMut(&str) -> Result<String>) -> String {
    let held = match shell("cmd window user-rotation") {
        Ok(out) => format!("the window manager holds `{}`", out.trim()),
        Err(e) => format!("the window manager's rotation could not be read: {e}"),
    };
    let turned = match shell("dumpsys display") {
        Ok(dump) => match display_turn(&dump) {
            Some(turn) => format!("the display is turned to {turn}"),
            None => "the display's turn is not in its dump".to_owned(),
        },
        Err(e) => format!("the display could not be read: {e}"),
    };
    format!("{held}, {turned}")
}

/// What `read` says of each of `saved` that is not what it was, as one
/// line per setting; `None` when all read back as they were.
fn not_put_back(
    saved: &[Setting],
    mut read: impl FnMut(&Setting) -> Result<String>,
) -> Result<Option<String>> {
    // An unset key reads `null`; what `restored` deleted reads so again.
    let unset = |v: &str| matches!(v, "null" | "");
    let mut off = Vec::new();
    for setting in saved {
        let now = read(setting)?;
        let now = now.trim();
        if now != setting.was && !(unset(now) && unset(&setting.was)) {
            off.push(format!(
                "{} {} reads {now}, was {}",
                setting.namespace, setting.key, setting.was
            ));
        }
    }
    Ok((!off.is_empty()).then(|| off.join("; ")))
}

/// The bytes one of the application's trims gave back, from the line the
/// shell logs for it (`short of memory: trimmed at …, N bytes given
/// back`), as logcat writes it after its time, process and tag; `None` for
/// any other line.
fn trimmed_bytes(line: &str) -> Option<usize> {
    let (_, rest) = line.split_once("short of memory: trimmed at ")?;
    let (_, freed) = rest.rsplit_once(", ")?;
    freed.strip_suffix(" bytes given back")?.parse().ok()
}

/// What in the UI thread's last rest (`app_watch`'s `rest`) is still to
/// be done, if anything: a wake it asked for, a frame it owes, an
/// animation under way.
fn not_idle(rest: &Value) -> Option<String> {
    if let Some(ms) = rest["wake_in_ms"].as_u64() {
        return Some(format!("it asks to be woken in {ms} ms"));
    }
    if rest["frame_owed"] == Value::Bool(true) {
        return Some("it owes its window a frame".into());
    }
    if rest["animating"] == Value::Bool(true) {
        return Some("an animation is under way".into());
    }
    None
}

/// How the UI thread's turns are watched: what [`Running`] asks the
/// inspector, apart so a test can turn the thread between the asks.
trait Watched {
    fn turns(&mut self) -> Result<(u64, u64)>;
    fn rest(&mut self) -> Result<(u64, Value)>;
    fn turned_after(&mut self, turns: (u64, u64), deadline: Instant) -> Result<bool>;
}

impl<P: Reached> Watched for Running<P> {
    fn turns(&mut self) -> Result<(u64, u64)> {
        Running::turns(self)
    }

    fn rest(&mut self) -> Result<(u64, Value)> {
        Running::rest(self)
    }

    fn turned_after(&mut self, turns: (u64, u64), deadline: Instant) -> Result<bool> {
        Running::turned_after(self, turns, deadline)
    }
}

/// Fails unless the rest the UI thread is in has nothing left to do and
/// it runs no pass while it is watched for [`IDLE_WATCH`].
fn stays_idle(running: &mut impl Watched) -> Result<()> {
    // Counted before the rest is read: a pass run between the two asks is
    // then seen by one or the other, never by neither.
    let turns = running.turns()?;
    let (rests, rest) = running.rest()?;
    if let Some(why) = not_idle(&rest) {
        return Err(BackendError::Failed(format!(
            "the application is not idle {} s after it was left: {why}",
            SEEN.as_secs()
        )));
    }
    if running.turned_after(turns, Instant::now() + IDLE_WATCH)? {
        let (now, _) = running.rest()?;
        return Err(BackendError::Failed(format!(
            "the application is not idle: it ran a pass within {} ms of resting \
             ({} rests since)",
            IDLE_WATCH.as_millis(),
            now.saturating_sub(rests)
        )));
    }
    Ok(())
}

/// How long an application at rest is watched for anything that wakes it.
const IDLE_WATCH: Duration = Duration::from_secs(1);

impl Phone {
    /// Sets the developer option that ends every activity the user leaves
    /// (`always_finish` on). The system follows the option only as its
    /// Settings application sets it: `settings put` alone changes what
    /// `settings get` says and not what the system does (Android 16), so
    /// the switch is tapped on that screen, as the developer sets it.
    fn always_finish(&self, on: bool) -> Result<()> {
        let want = if on { "1" } else { "0" };
        let setting = "settings get global always_finish_activities";
        // The switch shows the setting: written the other way first, the
        // tap turns it the way wanted whatever the system followed.
        let other = if on { "0" } else { "1" };
        self.shell(&format!(
            "settings put global always_finish_activities {other}"
        ))?;
        self.shell("settings put global development_settings_enabled 1")?;
        self.shell(
            "am start -W -a android.settings.APPLICATION_DEVELOPMENT_SETTINGS \
             --es :settings:fragment_args_key immediately_destroy_activities",
        )?;
        let at = self.wait_screen(&format!("developer option {DONT_KEEP:?}"), |nodes| {
            nodes.iter().find(|n| is_dont_keep(n)).map(|n| n.at)
        })?;
        self.tap(at)?;
        let until = Instant::now() + SEEN;
        while self.shell(setting)?.trim() != want {
            if Instant::now() > until {
                return Err(BackendError::Failed(format!(
                    "the developer option {DONT_KEEP:?} was not turned {} {} s after it was tapped",
                    if on { "on" } else { "off" },
                    SEEN.as_secs()
                )));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        self.shell("input keyevent KEYCODE_HOME").map(drop)
    }
}

impl Device for AndroidBackend {
    fn leave_for_home(&mut self) -> Result<()> {
        let phone = self.phone()?.clone();
        if self.running.is_some() {
            self.trims_before_home = self.trims()?.len();
        }
        phone.shell("input keyevent KEYCODE_HOME")?;
        if !phone.wait_front(false, SEEN)? {
            return Err(BackendError::Failed(format!(
                "the application was still in front {} s after Home",
                SEEN.as_secs()
            )));
        }
        // The system puts another activity in front before it tells the
        // application it was stopped: what follows is for an application
        // that knows it is hidden.
        if self.running.is_some() {
            self.wait_state(HIDDEN, "hidden")?;
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
        // In front once it has the keyboard, as the user sees it.
        self.wait_state(ACTIVE, "active")?;
        self.settle()
    }

    fn turn(&mut self, landscape: bool) -> Result<()> {
        let phone = self.phone()?.clone();
        let want = if landscape { "1" } else { "0" };
        self.save(&phone, "system", "accelerometer_rotation")?;
        self.save(&phone, "system", "user_rotation")?;
        // Turned by the user's own lock, not the sensor: an emulator's
        // sensor says the phone stands, whatever it was told before. The
        // window manager sets the lock, as the quick setting does, so its
        // record and the settings agree when they are put back.
        phone.shell(&format!("cmd window user-rotation lock {want}"))?;
        // Turned once the window the application sees is as wide as the
        // turn makes it: a frame drawn before that is of the old size.
        let until = Instant::now() + SEEN;
        loop {
            let status = self.running()?.ask("app_status", json!({}))?;
            let window = &status["windows"][0];
            let (w, h) = (window["width"].as_f64(), window["height"].as_f64());
            if let (Some(w), Some(h)) = (w, h)
                && (w > h) == landscape
                && w != h
            {
                break;
            }
            if Instant::now() > until {
                return Err(BackendError::Failed(format!(
                    "the application's window was {w:?} by {h:?} {} s after the phone turned to {}",
                    SEEN.as_secs(),
                    if landscape { "landscape" } else { "portrait" }
                )));
            }
            std::thread::sleep(Duration::from_millis(100));
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
        // The option's screen is reached only with developer settings on.
        self.save(&phone, "global", "development_settings_enabled")?;
        self.finishing = true;
        phone.always_finish(true)?;
        let ended = (|| {
            phone.start(false)?;
            if !phone.wait_front(true, STARTUP)? {
                return Err(BackendError::Failed(format!(
                    "the application was not in front {} s after it was opened",
                    STARTUP.as_secs()
                )));
            }
            phone.shell("input keyevent KEYCODE_HOME")?;
            let until = Instant::now() + SEEN;
            loop {
                let said = phone.shell("dumpsys activity activities")?;
                let kept = screens_kept(&said, &phone.id);
                if kept == 0 {
                    return Ok(());
                }
                if Instant::now() > until {
                    return Err(BackendError::Failed(format!(
                        "the system kept {kept} of the application's screens {} s after it was hidden",
                        SEEN.as_secs()
                    )));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        })();
        phone.always_finish(false)?;
        self.finishing = false;
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
        // The process that aborted is gone. CheckJNI says what it refused
        // in the main log, bionic what it caught (`FORTIFY:`) and the
        // signal it ended on in the crash log, and the crash dump its
        // abort message there too.
        Ok(self
            .logged()?
            .lines()
            .filter(|l| ABORTED.iter().any(|said| l.contains(said)))
            .map(str::to_owned)
            .collect())
    }

    fn connect_inspector(&mut self) -> Result<()> {
        self.connect()
    }

    fn disconnect_inspector(&mut self) -> Result<()> {
        self.running = None;
        let phone = self.phone()?.clone();
        self.left_port = Some(self.connected_port()?);
        self.disconnect(&phone)
    }

    fn connect_inspector_anew(&mut self) -> Result<()> {
        let before = self.connected_port()?;
        // The inspector chooses its port on the phone: a process started
        // anew may get the last one's again, and then the connect keeps
        // the forward it made, rightly. Started once more, it gets
        // another; three alike say something keeps the port.
        for _ in 0..3 {
            self.start()?;
            if self.connected_port()? != before {
                self.left_port = Some(before);
                return Ok(());
            }
        }
        Err(BackendError::Failed(format!(
            "the application, started anew three times, served its inspector at port {before} \
             each time, the port of the connect before: no connect to another port replaced it"
        )))
    }

    fn inspector_forwarded(&mut self) -> Result<bool> {
        let phone = self.phone()?.clone();
        let port = self.left_port.ok_or_else(|| {
            BackendError::Failed(
                "the developer neither disconnected from the inspector nor connected to one \
                 started anew"
                    .into(),
            )
        })?;
        let listed = phone.output(
            phone.adb().args(["forward", "--list"]),
            "forward --list",
            None,
        )?;
        Ok(forwards_port(&listed, &phone.serial, port))
    }

    fn short_of_memory(&mut self) -> Result<()> {
        let phone = self.phone()?.clone();
        let before = self.trims()?.len();
        let pid = self.running()?.app.pid;
        // `RUNNING_LOW`, which the system sends a process in front or
        // behind; a background level it refuses for a process it still
        // counts as seen, as it does for a while after Home.
        phone.shell(&format!("am send-trim-memory {pid} RUNNING_LOW"))?;
        let until = Instant::now() + SEEN;
        while self.trims()?.len() <= before {
            if Instant::now() > until {
                return Err(BackendError::Failed(format!(
                    "the application logged no trim {} s after the system said it was short of memory",
                    SEEN.as_secs()
                )));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }

    fn memory_given_back(&mut self) -> Result<usize> {
        // Hiding it is a trim of its own on a phone (`onTrimMemory` with
        // `TRIM_MEMORY_UI_HIDDEN`), which gives back what the trim the
        // scenario asked for then finds already given: all of them count.
        let trims = self.trims()?;
        Ok(trims.iter().skip(self.trims_before_home).sum())
    }

    fn end_process(&mut self) -> Result<()> {
        let phone = self.phone()?.clone();
        // `am kill` ends only a process the user does not see, as the
        // system does to save memory: the application goes behind the home
        // screen first, and is killed again until the system has stopped
        // counting it as seen.
        phone.shell("input keyevent KEYCODE_HOME")?;
        let until = Instant::now() + SEEN;
        while {
            phone.shell(&format!("am kill {}", phone.id))?;
            phone.pid()?.is_some()
        } {
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

    fn app_directory(&mut self, name: &str) -> Result<Vec<String>> {
        let phone = self.phone()?.clone();
        let data = phone.data_dir()?;
        Ok(vec![
            format!("{data}/{name}"),
            format!("/data/data/{}/{name}", phone.id),
        ])
    }

    fn files_hold(&mut self, text: &str) -> Result<bool> {
        let said = self.phone()?.as_app(
            &format!("grep -rlF -- {} . 2>/dev/null || true", quoted(text)),
            None,
        )?;
        Ok(!said.trim().is_empty())
    }

    fn keep_secret(&mut self, service: &str, key: &str, secret: &str) -> Result<()> {
        self.secrets("keep", &[service, key, secret]).map(drop)
    }

    fn remove_secret(&mut self, service: &str, key: &str) -> Result<()> {
        self.secrets("remove", &[service, key]).map(drop)
    }

    fn read_secret(&mut self, service: &str, key: &str) -> Result<Option<String>> {
        let (found, secret) = self.secrets("read", &[service, key])?;
        Ok(found.then_some(secret))
    }

    fn keeps_permission(&mut self, url: &str) -> Result<bool> {
        let phone = self.phone()?.clone();
        let dump = phone.shell("dumpsys activity permissions")?;
        let url = percent_decoded(url);
        Ok(kept_permissions(&dump, &phone.id)
            .iter()
            .any(|kept| percent_decoded(kept) == url))
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

/// Whether `ljos android connect` said the inspector's port here is
/// already forwarded, so its forward could not be made: its own words
/// (`connect` in crates/ljos_cli/src/android.rs, whose tests hold them).
fn port_taken_here(said: &str) -> bool {
    said.contains("could not be forwarded to the inspector")
}

/// The port of an `http://host:port/…` address.
fn url_port(url: &str) -> Option<u16> {
    let authority = url.strip_prefix("http://")?.split('/').next()?;
    authority.rsplit_once(':')?.1.parse().ok()
}

/// Whether `adb forward --list` says `serial` holds a forward from `port`
/// here.
fn forwards_port(listed: &str, serial: &str, port: u16) -> bool {
    let local = format!("tcp:{port}");
    listed.lines().any(|line| {
        let mut fields = line.split_whitespace();
        fields.next() == Some(serial) && fields.next() == Some(local.as_str())
    })
}

#[cfg(test)]
mod tests {
    use super::{
        AndroidBackend, BackendError, FORGET_EVENTS, Setting, Shown, Watched, abort_query,
        badge_in, clipboard_call, display_turn, download_path, forwards_port, front_record,
        is_dont_keep, kept_permissions, locked_to, not_idle, not_put_back, percent_decoded,
        permission_asked, permission_button, picker_shown, picture_size, port_taken_here, quoted,
        restored, scenario_dir, screen_nodes, screens_kept, secrets_answer, secrets_call,
        seen_until, stays_idle, tear_down, trimmed_bytes, url_port,
    };
    use serde_json::{Value, json};
    use std::time::Instant;

    // Trimmed from Android 16's `dumpsys activity activities`, the
    // application's screen kept behind the home screen.
    const KEPT: &str = "\
  * Task{80cd59b #25 type=standard A=10216:dev.ljos.scenarios U=0 visible=false sz=1}
    mLastPausedActivity: ActivityRecord{144717226 u0 dev.ljos.scenarios/dev.ljos.LjosActivity t25}
    * Hist  #0: ActivityRecord{144717226 u0 dev.ljos.scenarios/dev.ljos.LjosActivity t25}
      packageName=dev.ljos.scenarios processName=dev.ljos.scenarios
      app=ProcessRecord{8d93077 7723:dev.ljos.scenarios/u0a216}
      state=STOPPED delayedResume=false finishing=false
  * Task{89217e2 #22 type=standard A=1000:com.android.settings U=0 visible=false sz=1}
    * Hist  #0: ActivityRecord{141659222 u0 com.android.settings/.Settings t22}
      app=ProcessRecord{b70751e 1319:com.android.settings/1000}
      state=STOPPED delayedResume=false finishing=false
      * Task{80cd59b #25 type=standard A=10216:dev.ljos.scenarios U=0 visible=false sz=1}
        * ActivityRecord{144717226 u0 dev.ljos.scenarios/dev.ljos.LjosActivity t25}
";

    #[test]
    fn a_screen_kept_behind_the_home_screen_is_counted() {
        assert_eq!(screens_kept(KEPT, "dev.ljos.scenarios"), 1);
        assert_eq!(screens_kept(KEPT, "com.android.settings"), 1);
        assert_eq!(screens_kept(KEPT, "dev.ljos.other"), 0);
    }

    #[test]
    fn a_destroyed_screen_still_listed_is_not_kept() {
        let destroyed = KEPT
            .replace(
                "app=ProcessRecord{8d93077 7723:dev.ljos.scenarios/u0a216}",
                "app=null",
            )
            .replacen("state=STOPPED", "state=DESTROYED", 1);
        assert_eq!(screens_kept(&destroyed, "dev.ljos.scenarios"), 0);
        assert_eq!(screens_kept(&destroyed, "com.android.settings"), 1);
    }

    #[test]
    fn a_trim_says_the_bytes_it_gave_back() {
        assert_eq!(
            trimmed_bytes("short of memory: trimmed at Critical, 232 bytes given back"),
            Some(232)
        );
        assert_eq!(
            trimmed_bytes("short of memory: trimmed at Moderate, 0 bytes given back"),
            Some(0)
        );
        assert_eq!(
            trimmed_bytes(
                "11-27 09:40:40.455  9684  9703 I ljos    : ljos_shell::android: \
                 short of memory: trimmed at Critical, 264432 bytes given back"
            ),
            Some(264_432)
        );
        assert_eq!(trimmed_bytes("touch slop 8 dp"), None);
        assert_eq!(trimmed_bytes("short of memory: trimmed at Critical"), None);
    }

    /// A UI thread at rest that runs one pass, which asks to be woken in
    /// 2 s, just after the first of the inspector's asks is answered.
    struct PassesAfterTheFirstAsk {
        asks: u32,
        rests: u64,
        rest: Value,
    }

    impl PassesAfterTheFirstAsk {
        fn ask(&mut self) {
            self.asks += 1;
            if self.asks == 2 {
                self.rests += 1;
                self.rest = json!({"wake_in_ms": 2000});
            }
        }
    }

    impl Watched for PassesAfterTheFirstAsk {
        fn turns(&mut self) -> super::Result<(u64, u64)> {
            self.ask();
            Ok((0, self.rests))
        }

        fn rest(&mut self) -> super::Result<(u64, Value)> {
            self.ask();
            Ok((self.rests, self.rest.clone()))
        }

        fn turned_after(&mut self, (_, rests): (u64, u64), _: Instant) -> super::Result<bool> {
            self.ask();
            Ok(self.rests > rests)
        }
    }

    #[test]
    fn a_pass_between_the_asks_is_not_taken_for_rest() {
        let mut thread = PassesAfterTheFirstAsk {
            asks: 0,
            rests: 5,
            rest: json!({}),
        };
        assert!(stays_idle(&mut thread).is_err());
    }

    fn setting(namespace: &'static str, key: &'static str, was: &str) -> Setting {
        Setting {
            namespace,
            key,
            was: was.to_owned(),
        }
    }

    #[test]
    fn a_setting_that_was_unset_is_deleted_again() {
        assert_eq!(
            restored(&[
                setting("system", "accelerometer_rotation", "null"),
                setting("system", "user_rotation", "1"),
            ]),
            [
                "cmd window user-rotation lock 1",
                "settings delete system accelerometer_rotation",
            ]
        );
    }

    #[test]
    fn the_rotation_is_put_back_by_the_window_manager() {
        // Put through `settings`, the window manager's own record of the
        // lock could write the old one back after the read.
        assert_eq!(
            restored(&[
                setting("system", "accelerometer_rotation", "1"),
                setting("system", "user_rotation", "0"),
            ]),
            [
                "cmd window user-rotation lock 0",
                "cmd window user-rotation free",
            ]
        );
        assert_eq!(
            restored(&[
                setting("system", "accelerometer_rotation", "0"),
                setting("system", "user_rotation", "3"),
            ]),
            [
                "cmd window user-rotation lock 3",
                "cmd window user-rotation lock",
            ]
        );
    }

    #[test]
    fn a_setting_read_back_otherwise_than_it_was_is_told() {
        let saved = [
            setting("system", "accelerometer_rotation", "null"),
            setting("system", "user_rotation", "0"),
            setting("global", "development_settings_enabled", "0"),
        ];
        let phone = |s: &Setting| {
            Ok(match s.key {
                "accelerometer_rotation" => "null\n",
                "user_rotation" => "1\n",
                _ => "0\n",
            }
            .to_owned())
        };
        assert_eq!(
            not_put_back(&saved, phone).unwrap(),
            Some("system user_rotation reads 1, was 0".to_owned())
        );
        assert_eq!(not_put_back(&saved, |s| Ok(s.was.clone())).unwrap(), None);
    }

    #[test]
    fn a_failed_step_of_the_tidying_leaves_the_others_done() {
        let saved = vec![
            setting("system", "accelerometer_rotation", "null"),
            setting("system", "user_rotation", "0"),
            setting("global", "development_settings_enabled", "0"),
        ];
        let mut ran = Vec::new();
        let mut shell = |command: &str| {
            ran.push(command.to_owned());
            match command {
                "cmd window user-rotation lock 0" => {
                    Err(BackendError::Failed("adb went away".into()))
                }
                "settings get system accelerometer_rotation" => Ok("null\n".to_owned()),
                _ => Ok("0\n".to_owned()),
            }
        };
        let mut finish_off = || Err(BackendError::Failed("no switch".into()));
        let left = tear_down(saved, true, &mut shell, &mut finish_off, &mut |_| false);
        assert!(left.finishing, "{left:?}");
        // Turning the switch off again turns the developer settings on:
        // the retry needs every value, put back already or not.
        assert_eq!(
            left.saved,
            [
                setting("system", "accelerometer_rotation", "null"),
                setting("system", "user_rotation", "0"),
                setting("global", "development_settings_enabled", "0"),
            ]
        );
        assert_eq!(left.failed.len(), 2, "{left:?}");
        assert!(
            ran.contains(&"settings put global development_settings_enabled '0'".to_owned()),
            "{ran:?}"
        );
        assert!(
            ran.contains(&"settings delete system accelerometer_rotation".to_owned()),
            "{ran:?}"
        );
    }

    #[test]
    fn what_the_tidying_left_undone_keeps_the_order_it_was_changed_in() {
        let saved = vec![
            setting("system", "accelerometer_rotation", "null"),
            setting("system", "user_rotation", "0"),
            setting("global", "development_settings_enabled", "0"),
        ];
        let mut shell = |command: &str| match command {
            "settings delete system accelerometer_rotation" => {
                Err(BackendError::Failed("adb went away".into()))
            }
            "settings get global development_settings_enabled" => Ok("1\n".to_owned()),
            _ => Ok("0\n".to_owned()),
        };
        let left = tear_down(saved, false, &mut shell, &mut || Ok(()), &mut |_| false);
        assert!(!left.finishing, "{left:?}");
        assert_eq!(
            left.saved,
            [
                setting("system", "accelerometer_rotation", "null"),
                setting("global", "development_settings_enabled", "0"),
            ]
        );
    }

    #[test]
    fn a_rotation_the_window_manager_locks_late_is_waited_for_before_it_is_freed() {
        // The phone as the landscape scenario leaves it. The window manager
        // turns its lock a moment after `cmd` returns while the display
        // still turns back, and its late write turned the `free` after it
        // back to locked (lifecycle.feature:65, reproduced on API 36).
        let saved = vec![
            setting("system", "accelerometer_rotation", "1"),
            setting("system", "user_rotation", "0"),
        ];
        let (mut user, mut accel) = ("1", "0");
        let mut late: Option<(u8, &str)> = None;
        let mut ran = Vec::new();
        let mut shell = |command: &str| {
            ran.push(command.to_owned());
            if let Some((calls, turn)) = late.as_mut() {
                *calls -= 1;
                if *calls == 0 {
                    (user, accel) = (turn, "0");
                    late = None;
                }
            }
            match command {
                "cmd window user-rotation lock 0" => late = Some((2, "0")),
                "cmd window user-rotation free" => accel = "1",
                "settings get system user_rotation" => return Ok(format!("{user}\n")),
                "settings get system accelerometer_rotation" => return Ok(format!("{accel}\n")),
                _ => {}
            }
            Ok(String::new())
        };
        let mut waited = 0;
        let mut pause = |_| {
            waited += 1;
            waited < 10
        };
        let left = tear_down(saved, false, &mut shell, &mut || Ok(()), &mut pause);
        assert!(left.failed.is_empty(), "{left:?} after {ran:?}");
        assert!(left.saved.is_empty(), "{left:?}");
        assert!(waited > 0, "the lock was not waited for: {ran:?}");
    }

    /// `dumpsys display`'s default display, turned to `turn`.
    fn display_dump(turn: &str) -> String {
        format!(
            "  mBaseDisplayInfo=DisplayInfo{{\"Built-in Screen\", displayId 0, real 1080 x 2400, rotation 0, state ON}}\n  mOverrideDisplayInfo=DisplayInfo{{\"Built-in Screen\", displayId 0, cutout DisplayCutout{{insets=Rect(0, 0 - 0, 0)}}, real 1080 x 2400, rotation {turn}, state ON}}\n"
        )
    }

    #[test]
    fn a_rotation_is_freed_only_once_the_display_has_turned_back() {
        // As the landscape outline leaves the phone: the settings read as
        // locked at once, the display turns back a moment later, and as it
        // does the window manager writes its lock again, over a `free` put
        // before then (lifecycle.feature:65, main run 46016933337).
        let saved = vec![
            setting("system", "accelerometer_rotation", "1"),
            setting("system", "user_rotation", "0"),
        ];
        let (mut user, mut accel, mut display) = ("1", "0", "1");
        let mut turning: Option<u8> = None;
        let mut ran = Vec::new();
        let mut shell = |command: &str| {
            ran.push(command.to_owned());
            if let Some(left) = turning.as_mut() {
                *left -= 1;
                if *left == 0 {
                    (display, accel) = ("0", "0");
                    turning = None;
                }
            }
            match command {
                "cmd window user-rotation lock 0" => {
                    (user, accel) = ("0", "0");
                    turning = Some(3);
                }
                "cmd window user-rotation free" => accel = "1",
                "settings get system user_rotation" => return Ok(format!("{user}\n")),
                "settings get system accelerometer_rotation" => return Ok(format!("{accel}\n")),
                "dumpsys display" => return Ok(display_dump(display)),
                _ => {}
            }
            Ok(String::new())
        };
        let left = tear_down(saved, false, &mut shell, &mut || Ok(()), &mut |tried| {
            tried < 10
        });
        assert!(left.failed.is_empty(), "{left:?} after {ran:?}");
        assert!(left.saved.is_empty(), "{left:?}");
        let free = ran
            .iter()
            .position(|c| c == "cmd window user-rotation free");
        let turned = ran.iter().rposition(|c| c == "dumpsys display");
        assert!(
            turned < free,
            "freed before the display turned back: {ran:?}"
        );
    }

    #[test]
    fn a_rotation_left_locked_says_what_the_window_manager_holds() {
        let saved = vec![setting("system", "accelerometer_rotation", "1")];
        let mut shell = |command: &str| {
            Ok(match command {
                "settings get system accelerometer_rotation" => "0\n".to_owned(),
                "cmd window user-rotation" => "lock 0\n".to_owned(),
                "dumpsys display" => display_dump("1"),
                _ => String::new(),
            })
        };
        let left = tear_down(saved, false, &mut shell, &mut || Ok(()), &mut |_| false);
        assert_eq!(
            left.failed,
            [
                "system accelerometer_rotation reads 0, was 1 (the window manager holds `lock 0`, the display is turned to 1)"
            ]
        );
    }

    #[test]
    fn the_display_s_turn_is_read_from_its_override() {
        assert_eq!(display_turn(&display_dump("3")), Some("3"));
        assert_eq!(display_turn("Display Devices: size=1"), None);
        assert_eq!(locked_to("cmd window user-rotation lock 2"), Some("2"));
        assert_eq!(locked_to("cmd window user-rotation lock"), None);
        assert_eq!(locked_to("cmd window user-rotation free"), None);
    }

    #[test]
    fn the_developer_settings_are_put_back_as_they_were() {
        assert_eq!(
            restored(&[setting("global", "development_settings_enabled", "0")]),
            ["settings put global development_settings_enabled '0'"]
        );
    }

    #[test]
    fn an_application_at_rest_has_nothing_left_to_do() {
        let rest = |wake: Value, owed: bool, animating: bool| json!({"wake_in_ms": wake, "frame_owed": owed, "animating": animating});
        assert_eq!(not_idle(&rest(Value::Null, false, false)), None);
        assert!(not_idle(&rest(json!(16), false, false)).is_some_and(|w| w.contains("16 ms")));
        assert!(not_idle(&rest(Value::Null, true, false)).is_some_and(|w| w.contains("frame")));
        assert!(not_idle(&rest(Value::Null, false, true)).is_some_and(|w| w.contains("animation")));
    }

    #[test]
    fn the_developer_option_is_found_by_its_words() {
        let shown = screen_nodes(
            r#"<node text="Don’t keep activities" resource-id="android:id/title" class="android.widget.TextView" content-desc="" enabled="true" bounds="[0,10][100,30]" /><node text="Keep activities" resource-id="" class="" content-desc="" enabled="true" bounds="[0,40][100,60]" />"#,
        );
        let found: Vec<_> = shown.iter().filter(|n| is_dont_keep(n)).collect();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].at, (50, 20));
    }

    // Trimmed from Android 16's `dumpsys notification --noredact`.
    const NOTIFICATIONS: &str = "\
  Notification List:
    NotificationRecord(0x03c10bb5: pkg=dev.ljos.other user=UserHandle{0} id=1 tag=null importance=2 key=0|dev.ljos.other|1|null|10124: Notification(channel=ljos.badge shortcut=null flags=0 vis=PRIVATE))
      uid=10124 userId=0
      notification=
            number=9
      mShowBadge=true
    NotificationRecord(0x0656b7a8: pkg=dev.ljos.scenarios user=UserHandle{0} id=1818783603 tag=null importance=2 key=0|dev.ljos.scenarios|1818783603|null|10123: Notification(channel=ljos.badge shortcut=null flags=ONLY_ALERT_ONCE vis=PRIVATE))
      uid=10123 userId=0
      notification=
            fullscreenIntent=null
            number=3
            groupAlertBehavior=0
      mShowBadge=true
      effectiveNotificationChannel=NotificationChannel{mId='ljos.badge', mName=Badge, mShowBadge=true}
";

    #[test]
    fn the_badge_is_the_number_of_the_applications_badge_notification() {
        assert_eq!(badge_in(NOTIFICATIONS, "dev.ljos.scenarios"), 3);
        assert_eq!(badge_in(NOTIFICATIONS, "dev.ljos.other"), 9);
        assert_eq!(badge_in(NOTIFICATIONS, "dev.ljos.none"), 0);
        // A package whose name another's begins with is not that one.
        assert_eq!(badge_in(NOTIFICATIONS, "dev.ljos"), 0);
        assert_eq!(badge_in("", "dev.ljos.scenarios"), 0);
    }

    #[test]
    fn a_notification_that_may_show_no_badge_shows_none() {
        let hidden = NOTIFICATIONS.replace(
            "      mShowBadge=true\n    Notif",
            "      mShowBadge=false\n    Notif",
        );
        assert_eq!(badge_in(&hidden, "dev.ljos.other"), 0);
        let elsewhere = NOTIFICATIONS.replace(
            "channel=ljos.badge shortcut=null flags=ONLY",
            "channel=chat shortcut=null flags=ONLY",
        );
        assert_eq!(badge_in(&elsewhere, "dev.ljos.scenarios"), 0);
    }

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
        // A screen captured before the deadline, returned after it: read
        // once more, begun at the deadline or after it.
        let until = Instant::now() + Duration::from_millis(100);
        let mut reads = 0;
        let slow = seen_until(
            until,
            none,
            || {
                reads += 1;
                std::thread::sleep(Duration::from_millis(300));
                Ok(screen_nodes(""))
            },
            asked,
        );
        assert!(matches!(slow, Ok(None)));
        assert_eq!(reads, 2, "the first read began before the deadline");
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
        let notifications = question(
            "Allow ljos_example_android_scenarios to send you notifications?",
            &["permission_allow_button", "permission_deny_button"],
        );
        assert_eq!(permission_asked(&notifications), Some("notifications"));
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

    #[test]
    fn a_secret_goes_through_the_scenario_applications_store() {
        assert_eq!(
            secrets_call("keep", &["accounts", "token", "it's \"s3cret\""]),
            "scenarioSecrets.keep(\"accounts\", \"token\", \"it's \\\"s3cret\\\"\")"
        );
    }

    #[test]
    fn the_stores_answer_is_read_once_it_is_not_busy() {
        use serde_json::json;
        let said = |value| json!({"value": value, "truncated": false});
        assert!(secrets_answer(&said(json!({"busy": true}))).is_none());
        let read = said(json!({"busy": false, "found": true, "secret": "s3cret", "error": ""}));
        assert_eq!(
            secrets_answer(&read).map(|a| a.ok()),
            Some(Some((true, "s3cret".to_owned())))
        );
        let failed = said(json!({"busy": false, "found": false, "secret": "", "error": "locked"}));
        assert!(matches!(
            secrets_answer(&failed),
            Some(Err(BackendError::Failed(why))) if why.contains("locked")
        ));
    }

    #[test]
    fn a_secret_too_long_to_come_back_whole_is_a_failure() {
        use serde_json::json;
        let cut = json!({
            "value": {"busy": false, "found": true, "secret": "s3c", "error": ""},
            "truncated": true,
        });
        assert!(matches!(
            secrets_answer(&cut),
            Some(Err(BackendError::Failed(why))) if why.contains("whole")
        ));
        let replaced = json!({"value": "<an object too large to show>", "truncated": true});
        assert!(matches!(secrets_answer(&replaced), Some(Err(_))));
    }

    #[test]
    fn a_clipboard_call_quotes_its_text() {
        assert_eq!(
            clipboard_call("copyElsewhere", &[Value::from("it's \"so\"")]),
            r#"scenarioClipboard.copyElsewhere("it's \"so\"")"#
        );
        assert_eq!(
            clipboard_call("copyPicture", &[Value::from(3), Value::from(2)]),
            "scenarioClipboard.copyPicture(3, 2)"
        );
    }

    #[test]
    fn a_picture_is_its_width_and_height_or_none() {
        assert_eq!(picture_size("3 2").ok(), Some(Some((3, 2))));
        assert_eq!(picture_size("").ok(), Some(None));
        assert!(picture_size("3").is_err());
        assert!(picture_size("wide 2").is_err());
    }

    // Trimmed from Android 16's `dumpsys activity permissions`.
    const PERMISSIONS: &str = "\
ACTIVITY MANAGER URI PERMISSIONS (dumpsys activity permissions)
  Granted Uri Permissions:
  * UID 1000 holds:
    UriPermission{9577ec8 content://downloads/all_downloads/1 [user 0]}
      targetUserId=0 sourcePkg=com.android.providers.downloads targetPkg=com.android.keychain
      mode=0x3 owned=0x0 global=0x3 persistable=0x0 persisted=0x0
  * UID 10217 holds:
    UriPermission{6712f86 content://com.android.externalstorage.documents/tree/primary%3ADownload%2Fletters [user 0]}
      targetUserId=0 sourcePkg=com.android.externalstorage targetPkg=dev.ljos.app
      mode=0x3 owned=0x0 global=0x0 persistable=0x3 persisted=0x3 persistedCreate=1795712187756
    UriPermission{10577a0 content://com.android.providers.downloads.documents/document/16 [user 0]}
      targetUserId=0 sourcePkg=com.android.providers.downloads targetPkg=dev.ljos.app
      mode=0x1 owned=0x0 global=0x0 persistable=0x3 persisted=0x0
";

    #[test]
    fn only_a_permission_the_system_keeps_for_the_application_is_kept() {
        assert_eq!(
            kept_permissions(PERMISSIONS, "dev.ljos.app"),
            ["content://com.android.externalstorage.documents/tree/primary%3ADownload%2Fletters"]
        );
        assert!(kept_permissions(PERMISSIONS, "com.android.keychain").is_empty());
    }

    #[test]
    fn a_url_escaped_or_not_names_the_same_document() {
        assert_eq!(
            percent_decoded("tree/primary%3ADownload%2Fletters"),
            "tree/primary:Download/letters"
        );
        assert_eq!(percent_decoded("100%"), "100%");
        assert_eq!(percent_decoded("%zz"), "%zz");
    }

    #[test]
    fn a_late_drop_leaves_the_next_scenarios_application_running() {
        // Cucumber keeps a finished world until the writer lets go of it,
        // which can be while the next scenario runs the same application:
        // a forward left to Drop must not take that application down.
        let mut backend = AndroidBackend::default();
        assert!(backend.stops_on_drop(), "no `put_back` stopped it");
        backend.stopped = true;
        assert!(!backend.stops_on_drop());
    }

    #[test]
    fn a_forward_the_tidying_could_not_remove_is_left_to_drop() {
        let backend = AndroidBackend::default();
        assert!(backend.nothing_left(&Ok(())));
        assert!(!backend.nothing_left(&Err(BackendError::Failed("adb went away".into()))));
    }

    #[test]
    fn a_port_another_phone_holds_here_asks_for_a_new_process() {
        // What `ljos android connect` said in #822's run.
        let said = "ljos: port 34041 of this computer is emulator-5554's forward to its \
                    tcp:34041, so it could not be forwarded to the inspector: the inspector \
                    answers only at its own port, so that port must be free here: adb forward \
                    --no-rebind tcp:34041 tcp:34041: adb: error: cannot rebind existing socket";
        assert!(port_taken_here(said));
        // Not yet serving: the same process serves it a moment later.
        assert!(!port_taken_here(
            "ljos: dev.ljos.hello serves no inspector: start it with `ljos android run --mcp=inspect`"
        ));
    }

    #[test]
    fn a_forward_is_told_by_its_phone_and_its_port_here() {
        let listed = "emulator-5554 tcp:4410 tcp:4410\nemulator-5580 tcp:39001 tcp:39001\n";
        assert!(forwards_port(listed, "emulator-5580", 39001));
        assert!(!forwards_port(listed, "emulator-5580", 4410));
        assert!(!forwards_port(listed, "emulator-558", 39001));
        assert_eq!(url_port("http://127.0.0.1:39001/mcp"), Some(39001));
        assert_eq!(url_port("http://127.0.0.1/mcp"), None);
    }
}
