//! What a step can ask of a running QML application, independent of who runs it.

use std::fmt;

use serde_json::Value;

pub mod inspector;
pub mod ljos;
pub mod qt;
mod rfb;
pub mod testrun;
pub mod top;

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    /// The backend exists but the capability does not yet. Reported as pending.
    #[error("not implemented yet: {0}")]
    NotImplemented(&'static str),
    /// The oracle cannot do this (e.g. Qt cannot read default accessible names).
    #[error("unsupported on the {backend} backend: {what}")]
    Unsupported {
        backend: &'static str,
        what: &'static str,
    },
    #[error("{0}")]
    Failed(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, BackendError>;

/// A QML source file staged by a `Given` step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QmlFile {
    pub name: String,
    pub source: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Critical,
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Critical => "error",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogEntry {
    pub level: LogLevel,
    pub message: String,
}

/// One node of the accessibility tree, as an assistive technology sees it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct A11yNode {
    pub role: String,
    pub name: String,
    pub description: String,
    /// The author's `objectName`, when set.
    pub object_name: String,
    /// Whether this is the one node the keyboard is at: what a screen
    /// reader reads out. The top node when nothing below it has it.
    pub focused: bool,
    /// What the node holds besides its name: the text of an editor.
    pub value: String,
    /// The part of `value` that is selected.
    pub selected: String,
    /// What the node says of itself, in the words of the steps:
    /// "checkable", "checked", "partially checked", "pressed",
    /// "expandable", "expanded", "selectable", "selected", "read-only",
    /// "disabled", and for a live region "live" (and "assertive" when
    /// it interrupts).
    pub states: Vec<String>,
    /// Its place among the rows it is one of, counted from 1, and how
    /// many there are.
    pub set: Option<(usize, usize)>,
    /// The value of a slider-like node, then its least and its most, as
    /// numbers are written.
    pub range: Option<(String, String, String)>,
    /// The role and name of each node that labels this one.
    pub labelled_by: Vec<(String, String)>,
    /// What a screen reader calls the node when its role alone does not
    /// say ("dial" for a slider).
    pub called: String,
    pub children: Vec<A11yNode>,
}

impl A11yNode {
    /// Depth-first search for `role` + `name`.
    #[must_use]
    pub fn find(&self, role: &str, name: &str) -> Option<&A11yNode> {
        if self.is(role, name) {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(role, name))
    }

    #[must_use]
    pub fn is(&self, role: &str, name: &str) -> bool {
        self.role == role && self.name == name
    }

    /// The node the keyboard is at.
    #[must_use]
    pub fn focus(&self) -> Option<&A11yNode> {
        if self.focused {
            return Some(self);
        }
        self.children.iter().find_map(A11yNode::focus)
    }

    /// The node that has the first `role` + `name` as a child.
    #[must_use]
    pub fn parent_of(&self, role: &str, name: &str) -> Option<&A11yNode> {
        if self.children.iter().any(|c| c.is(role, name)) {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.parent_of(role, name))
    }
}

/// Why a changed file was not applied.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReloadFailure {
    /// The file's name with `.qml`; empty when the fault is in no one file.
    pub file: String,
    /// 1-based; 0 when unknown.
    pub line: u32,
    pub message: String,
}

/// A fault no UI boundary took: what a host shows over the application.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApplicationFault {
    /// `"Panic"`, `"JsInterrupted"`, `"Startup"`, ...
    pub kind: String,
    pub message: String,
}

/// A backdrop a window asks the window system for, in a scenario's words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackdropAsked {
    /// `"blurred"` or `"glass"`.
    pub kind: &'static str,
    /// `"light"`, `"dark"`, or `"the system's"` when the window leaves it
    /// to the window system.
    pub look: &'static str,
}

/// What a pointer does in one step of a press, drag and release.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerStep {
    Press,
    /// With the button held if it was pressed and not yet released.
    Move,
    Release,
}

/// How far a wheel event scrolls, with Qt's signs: positive is away from
/// the user (content goes up or left to its beginning).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WheelBy {
    /// Notches of a mouse wheel, sideways and lengthways.
    Notches(f64, f64),
    /// Pixels, from a device that scrolls by the pixel (a touchpad).
    Pixels(f64, f64),
}

/// What an assistive technology asks of a node besides a press and the
/// focus.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum A11yAction {
    Increase,
    Decrease,
    SetValue(f64),
    /// `(0, 1)` is down and `(-1, 0)` left; by a page, or else by a step.
    Scroll {
        dx: i8,
        dy: i8,
        page: bool,
    },
    /// The node's default action: what a screen reader's "activate" does.
    Press,
    /// Come into view.
    ShowOnScreen,
    Expand,
    Collapse,
}

/// A setting of the system an application adapts to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SystemSetting {
    DarkScheme(bool),
    HighContrast(bool),
    ReducedMotion(bool),
    /// How much larger than written the user wants text.
    FontScale(f64),
    /// Whether an assistive technology reads the application.
    ScreenReader(bool),
}

/// What the user does with a dialog the system shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SystemDialogAnswer {
    /// Chooses these files or this folder.
    Choose(Vec<String>),
    /// Presses the button with this text (`"OK"`).
    Button(String),
    /// Closes it without choosing.
    Dismiss,
}

/// What an assistive technology asks of a node that holds text.
/// Characters are counted from the start of the node's value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum A11yTextAction {
    /// From the first character to the second, which may be before it.
    Select(usize, usize),
    ReplaceSelection(String),
    SetValue(String),
}

/// The surface the steps drive. `!Send` is fine: cucumber 0.23 does not need `Send`.
pub trait Backend: fmt::Debug {
    fn name(&self) -> &'static str;
    /// Stage `files` and instantiate `entry` (one of them). May be called repeatedly.
    fn load(&mut self, files: &[QmlFile], entry: &str) -> Result<()>;
    fn set(&mut self, object: &str, property: &str, value: &Value) -> Result<()>;
    /// Replace the property by a binding to `expression`, as `Qt.binding` does.
    fn bind(&mut self, object: &str, property: &str, expression: &str) -> Result<()>;
    fn get(&mut self, object: &str, property: &str) -> Result<Value>;
    fn call(&mut self, object: &str, method: &str, args: &[Value]) -> Result<Value>;
    fn exists(&mut self, object: &str) -> Result<bool>;
    /// Let `ms` of application time pass (virtual on Ljós, real on Qt).
    fn advance(&mut self, ms: u64) -> Result<()>;
    fn click(&mut self, role: &str, name: &str) -> Result<()>;
    /// Press and release a key chord (`"Ctrl+S"`, `"Tab"`), as a keyboard does.
    fn key(&mut self, chord: &str) -> Result<()>;
    /// Hold a chord down until the keyboard has repeated it `repeats` times,
    /// then let go.
    fn key_hold(&mut self, chord: &str, repeats: u32) -> Result<()>;
    /// The window becomes, or stops being, the one the keyboard types into.
    fn window_active(&mut self, active: bool) -> Result<()>;
    fn touch(&mut self, x: f64, y: f64) -> Result<()>;
    /// One half of a click, or a move between the two: what a held or
    /// dragged pointer is made of.
    fn pointer(&mut self, step: PointerStep, x: f64, y: f64) -> Result<()>;
    fn a11y_tree(&mut self) -> Result<A11yNode>;
    /// What a screen reader's "move the focus here" does to the node with
    /// this role and name. A node that refuses is not an error.
    fn a11y_focus(&mut self, role: &str, name: &str) -> Result<()>;
    fn log(&mut self) -> Result<Vec<LogEntry>>;
    /// PNG bytes of the window.
    fn grab(&mut self) -> Result<Vec<u8>>;
    /// Make the translations of a Qt Linguist `.ts` file available for the
    /// language its `language` attribute names. Nothing is shown in that
    /// language until it is the UI language.
    fn translation(&mut self, name: &str, ts: &str) -> Result<()>;
    /// What assigning `Qt.uiLanguage` does.
    fn set_ui_language(&mut self, language: &str) -> Result<()>;
    /// The host makes another font the application font while the
    /// application runs: the default font with these attributes (`family`,
    /// `pointSize`, `pixelSize`, `weight`, `bold`, `italic`), or the
    /// default again for `None`.
    fn set_application_font(&mut self, _font: Option<&[(String, String)]>) -> Result<()> {
        Err(BackendError::NotImplemented(
            "changing the application font",
        ))
    }
    /// Put a file that is not QML (an image) beside the QML files, where a
    /// relative URL in them finds it.
    fn resource(&mut self, name: &str, bytes: &[u8]) -> Result<()>;
    /// The application makes a root of the loaded file `name` with no
    /// parent, as it makes each window it opens beside the first.
    fn open_window(&mut self, _name: &str) -> Result<()> {
        Err(BackendError::NotImplemented("a window of its own"))
    }
    /// Type `text` on the keyboard, one key press per character.
    fn type_text(&mut self, _text: &str) -> Result<()> {
        Err(BackendError::NotImplemented("typing text"))
    }
    /// Two clicks at one place in quick succession.
    fn double_click(&mut self, _x: f64, _y: f64) -> Result<()> {
        Err(BackendError::NotImplemented("a double click"))
    }
    /// Press at `from`, move to `to` with the button held, release there.
    fn drag(&mut self, _from: (f64, f64), _to: (f64, f64)) -> Result<()> {
        Err(BackendError::NotImplemented("a drag"))
    }
    /// The wheel turns, or a touchpad scrolls, with the pointer at `at`.
    fn wheel(&mut self, _at: (f64, f64), _by: WheelBy) -> Result<()> {
        Err(BackendError::NotImplemented("the wheel"))
    }
    /// What the window system is asked to draw behind the window at
    /// `object`; `None` for nothing.
    fn window_backdrop(&mut self, _object: &str) -> Result<Option<BackdropAsked>> {
        Err(BackendError::NotImplemented("window backdrops"))
    }
    /// An assistive technology asks the node with this role and name to
    /// do something. A node that refuses is not an error.
    fn a11y_act(&mut self, _role: &str, _name: &str, _action: A11yAction) -> Result<()> {
        Err(BackendError::NotImplemented(
            "an assistive technology's requests",
        ))
    }
    /// The program starts with the Qt Quick Controls style `name`, as
    /// `QT_QUICK_CONTROLS_STYLE` chooses it: a built-in style or a module
    /// of the scenario's files.
    fn controls_style(&mut self, _name: &str) -> Result<()> {
        Err(BackendError::NotImplemented("choosing a style"))
    }
    /// The program starts with these locale variables (`LANG`, `LC_ALL`,
    /// `LC_NUMERIC`, …) in its environment and no others.
    fn locale_environment(&mut self, _vars: &[(String, String)]) -> Result<()> {
        Err(BackendError::NotImplemented("a locale environment"))
    }
    /// The system says what the user prefers: before anything is loaded
    /// it is what the application starts with, after that it is a setting
    /// the user changed.
    fn system_setting(&mut self, _setting: SystemSetting) -> Result<()> {
        Err(BackendError::NotImplemented("system settings"))
    }
    /// The application sets a setting of the system itself, over what the
    /// system says.
    fn app_setting(&mut self, _setting: SystemSetting) -> Result<()> {
        Err(BackendError::NotImplemented(
            "an application's own settings",
        ))
    }
    /// The desktop's settings portal says a key's value: before anything
    /// is loaded, what the desktop had at start.
    fn desktop_setting(
        &mut self,
        _namespace: &str,
        _key: &str,
        _value: ljos_a11y::SettingValue,
    ) -> Result<()> {
        Err(BackendError::NotImplemented("a desktop's settings"))
    }
    /// The system says what it knows of its network: before anything is
    /// loaded, what it had at start.
    fn network(&mut self, _status: ljos_platform::NetworkStatus) -> Result<()> {
        Err(BackendError::NotImplemented("the system's network"))
    }
    /// An assistive technology asks the node with this role and name to
    /// select or change its text. A node that refuses is not an error.
    fn a11y_text(&mut self, _role: &str, _name: &str, _action: A11yTextAction) -> Result<()> {
        Err(BackendError::NotImplemented(
            "an assistive technology's requests",
        ))
    }
    /// Put `text` on the clipboard, as copying it in another application
    /// does.
    fn set_clipboard(&mut self, _text: &str) -> Result<()> {
        Err(BackendError::NotImplemented("the clipboard"))
    }
    /// Put formatted text on the clipboard, as copying it in a rich text
    /// editor of another application does: `html`, and its characters as
    /// plain text beside it.
    fn set_clipboard_html(&mut self, _html: &str) -> Result<()> {
        Err(BackendError::NotImplemented("HTML on the clipboard"))
    }
    /// The text on the clipboard.
    fn clipboard(&mut self) -> Result<String> {
        Err(BackendError::NotImplemented("the clipboard"))
    }
    /// A text file beside the QML files, where a relative URL finds it.
    fn text_file(&mut self, name: &str, text: &str) -> Result<()> {
        self.resource(name, text.as_bytes())
    }
    /// A text file in the user's home directory, at `path` below it.
    fn user_file(&mut self, _path: &str, _text: &str) -> Result<()> {
        Err(BackendError::NotImplemented("the user's home directory"))
    }
    /// What the text file beside the QML files holds, once everything the
    /// application wrote has reached it; `None` when there is no file.
    fn read_text_file(&mut self, _name: &str) -> Result<Option<String>> {
        Err(BackendError::NotImplemented("reading back files"))
    }
    /// The kind (`"open file"`) and title of each dialog the system shows.
    fn system_dialogs(&mut self) -> Result<Vec<(String, String)>> {
        Err(BackendError::NotImplemented("the system's dialogs"))
    }
    /// The user answers the dialog the system shows.
    fn answer_system_dialog(&mut self, _answer: SystemDialogAnswer) -> Result<()> {
        Err(BackendError::NotImplemented("the system's dialogs"))
    }
    /// The wall clock a script's `Date` reads is `epoch_ms` (milliseconds
    /// since the UNIX epoch) when the application starts, and moves with
    /// application time.
    fn wall_clock(&mut self, _epoch_ms: f64) -> Result<()> {
        Err(BackendError::NotImplemented("a pinned wall clock"))
    }
    /// Text is set in the fonts installed on the machine, as a desktop
    /// application's is, instead of only the fonts the toolkit carries.
    /// Given before anything is loaded.
    fn system_fonts(&mut self) -> Result<()> {
        Err(BackendError::NotImplemented("the system's fonts"))
    }
    /// The application gives the QML a tree model named `name` that reads
    /// a folder's rows only when a view asks for them, and has them
    /// `delay_ms` later. `rows` are (folder, name) pairs, the folder ""
    /// being the top. Given before anything is loaded.
    fn lazy_tree(&mut self, _name: &str, _delay_ms: u64, _rows: &[(String, String)]) -> Result<()> {
        Err(BackendError::NotImplemented("tree models written in Rust"))
    }
    /// The application gives the QML a list model named `name` of
    /// `total` rows ("row 0" on) that it reads `page` at a time, each page
    /// `delay_ms` after a view asks for more. None is read at the start.
    /// Given before anything is loaded.
    fn paged_model(
        &mut self,
        _name: &str,
        _total: usize,
        _page: usize,
        _delay_ms: u64,
    ) -> Result<()> {
        Err(BackendError::NotImplemented("list models written in Rust"))
    }
    /// The system cannot take away a dialog it shows, from now on.
    fn system_dialogs_stay(&mut self) -> Result<()> {
        Err(BackendError::NotImplemented("the system's dialogs"))
    }
    /// Another application copies `text` while this one runs, and nobody
    /// tells it. Where the clipboard tells every application of a change,
    /// that is putting it on the clipboard.
    fn copy_elsewhere(&mut self, text: &str) -> Result<()> {
        self.set_clipboard(text)
    }
    /// The system asks the user before it grants a permission. Said before
    /// anything is loaded.
    fn permissions_ask(&mut self) -> Result<()> {
        Err(BackendError::NotImplemented(
            "a system that asks for permissions",
        ))
    }
    /// The device has a sensor (`"accelerometer"`, as
    /// `ljos_platform::SensorKind::describe` names it). Said before
    /// anything is loaded.
    fn add_sensor(&mut self, _kind: &str) -> Result<()> {
        Err(BackendError::NotImplemented(
            "a sensor a scenario reads for",
        ))
    }
    /// The device's sensor measures `values`, timestamped with the
    /// scenario's clock.
    fn sensor_reads(&mut self, _kind: &str, _values: &[f64]) -> Result<()> {
        Err(BackendError::NotImplemented(
            "a sensor a scenario reads for",
        ))
    }
    /// The URLs the application asked the system to open, in order.
    fn opened_urls(&mut self) -> Result<Vec<String>> {
        Err(BackendError::NotImplemented("the system's opener of URLs"))
    }
    /// The permissions (`"camera"`) the system is asking the user for.
    fn permission_requests(&mut self) -> Result<Vec<String>> {
        Err(BackendError::NotImplemented(
            "a system that asks for permissions",
        ))
    }
    /// The user grants or denies the permission the system asks for.
    fn answer_permission(&mut self, _name: &str, _granted: bool) -> Result<()> {
        Err(BackendError::NotImplemented(
            "a system that asks for permissions",
        ))
    }
    /// An input method shows `text` as what is being composed at the
    /// cursor; an empty text ends the composition with nothing typed.
    fn compose(&mut self, _text: &str) -> Result<()> {
        Err(BackendError::NotImplemented("an input method"))
    }
    /// An input method ends the composition and types `text`.
    fn commit(&mut self, _text: &str) -> Result<()> {
        Err(BackendError::NotImplemented("an input method"))
    }
    /// The scenario changes files while the application runs: what a
    /// reload needs is noted from the first load on, as a development
    /// build notes it. Called before anything is loaded.
    fn watch_files(&mut self) {}
    /// A file of the running application is saved with new text, or
    /// removed (`None`). It is applied when the files have been quiet.
    fn change_file(&mut self, _name: &str, _source: Option<&str>) -> Result<()> {
        Err(self.no_reload())
    }
    /// A file that is not QML ([`Backend::resource`]) is saved again
    /// while the application runs. It is shown when the files have been
    /// quiet.
    fn change_resource(&mut self, _name: &str, _bytes: &[u8]) -> Result<()> {
        Err(self.no_reload())
    }
    /// A file of the running application is emptied by a save that has
    /// not finished: its text comes with a later `change_file`.
    fn empty_file(&mut self, _name: &str) -> Result<()> {
        Err(self.no_reload())
    }
    /// The quiet time ends now: what was saved is applied. A change that
    /// cannot be applied is not an error here; `reload_error` says why.
    fn reload(&mut self) -> Result<()> {
        Err(self.no_reload())
    }
    /// The last reload that failed, until one succeeds.
    fn reload_error(&mut self) -> Result<Option<ReloadFailure>> {
        Err(self.no_reload())
    }
    /// How many reloads were applied.
    fn reload_count(&mut self) -> Result<u64> {
        Err(self.no_reload())
    }
    /// What the application would wake for if no one touched it: the
    /// next timer, the next frame of something moving, an answer it waits
    /// for. Empty when it is idle. The oracle's event loop cannot be read.
    fn scheduled(&mut self) -> Result<Vec<String>> {
        Err(BackendError::Unsupported {
            backend: self.name(),
            what: "seeing what the application has scheduled",
        })
    }
    /// The user opens the application again, as a click on its icon in
    /// the Dock does. Qt's QML is not told.
    fn reopen(&mut self) -> Result<()> {
        Err(BackendError::Unsupported {
            backend: self.name(),
            what: "opening the application again",
        })
    }
    /// Whether the application asked to end (`Qt.quit()`). The oracle is
    /// itself the application that would end, so it cannot say.
    fn quit_requested(&mut self) -> Result<bool> {
        Err(BackendError::Unsupported {
            backend: self.name(),
            what: "seeing a request to quit",
        })
    }
    /// The `Qt.CursorShape` the pointer shows. The oracle runs off-screen,
    /// where there is no pointer to show one.
    fn pointer_cursor(&mut self) -> Result<i64> {
        Err(BackendError::Unsupported {
            backend: self.name(),
            what: "seeing the pointer's cursor",
        })
    }
    /// Native code running for the object panics with `message`, as a
    /// bug in a type written in Rust does.
    fn panic_in(&mut self, _object: &str, _message: &str) -> Result<()> {
        Err(self.no_faults())
    }
    /// The last fault that no UI boundary took.
    fn application_fault(&mut self) -> Result<Option<ApplicationFault>> {
        Err(self.no_faults())
    }
    /// Runs the staged `tst_*.qml` files with this backend's test runner
    /// and gives what it printed. Nothing is loaded for this: the runner
    /// is a program of its own.
    fn run_tests(
        &mut self,
        _files: &[QmlFile],
        _run: &testrun::TestRun,
    ) -> Result<testrun::TestReport> {
        Err(BackendError::NotImplemented("running QML tests"))
    }
    /// An agent's inspector is asked something (ARCHITECTURE.md §19.4).
    fn inspector(&mut self, _ask: &inspector::Ask) -> Result<()> {
        Err(self.no_inspector())
    }
    /// What the agent has been offered and was last answered.
    fn inspector_seen(&mut self) -> Result<inspector::Seen> {
        Err(self.no_inspector())
    }
    /// The `top` overlay is shown or hidden (ARCHITECTURE.md §19.10).
    fn top(&mut self, _shown: bool) -> Result<()> {
        Err(self.no_top())
    }
    /// The overlay as it stands.
    fn top_seen(&mut self) -> Result<top::TopSeen> {
        Err(self.no_top())
    }
    /// The overlay is Ljós's own tooling; the oracle has none.
    fn no_top(&self) -> BackendError {
        BackendError::Unsupported {
            backend: self.name(),
            what: "the top overlay",
        }
    }
    /// The inspector is Ljós's own tooling; the oracle has none.
    fn no_inspector(&self) -> BackendError {
        BackendError::Unsupported {
            backend: self.name(),
            what: "the inspector",
        }
    }
    /// Faults and their boundaries are Ljós's own.
    fn no_faults(&self) -> BackendError {
        BackendError::Unsupported {
            backend: self.name(),
            what: "faults and UI boundaries",
        }
    }
    /// Hot reload is a Ljós extension; the oracle has nothing like it.
    fn no_reload(&self) -> BackendError {
        BackendError::Unsupported {
            backend: self.name(),
            what: "hot reload",
        }
    }
}

/// Which backends run a scenario (`LJOS_BDD_BACKEND=qt|ljos|both`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendMode {
    Qt,
    Ljos,
    Both,
}

impl BackendMode {
    /// Default is `ljos`: the ledger is about Ljós, Qt is the oracle.
    #[must_use]
    pub fn from_env() -> Self {
        std::env::var("LJOS_BDD_BACKEND").map_or(Self::Ljos, |v| {
            Self::parse(&v)
                .unwrap_or_else(|| panic!("LJOS_BDD_BACKEND must be qt, ljos or both (got {v:?})"))
        })
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "qt" => Some(Self::Qt),
            "ljos" | "" => Some(Self::Ljos),
            "both" => Some(Self::Both),
            _ => None,
        }
    }

    /// Backends for a scenario. Only `@qt-parity` scenarios reach Qt.
    #[must_use]
    pub fn backends_for(self, qt_parity: bool) -> Vec<Box<dyn Backend>> {
        let mut out: Vec<Box<dyn Backend>> = Vec::new();
        if matches!(self, Self::Qt | Self::Both) && qt_parity {
            out.push(Box::new(qt::QtBackend::new()));
        }
        if matches!(self, Self::Ljos | Self::Both) {
            out.push(Box::new(ljos::LjosBackend::new()));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_parsing() {
        assert_eq!(BackendMode::parse("QT"), Some(BackendMode::Qt));
        assert_eq!(BackendMode::parse("both"), Some(BackendMode::Both));
        assert_eq!(BackendMode::parse("nope"), None);
    }

    #[test]
    fn only_parity_scenarios_reach_qt() {
        let names = |m: BackendMode, p| -> Vec<&'static str> {
            m.backends_for(p).iter().map(|b| b.name()).collect()
        };
        assert_eq!(names(BackendMode::Both, true), ["qt", "ljos"]);
        assert_eq!(names(BackendMode::Both, false), ["ljos"]);
        assert!(names(BackendMode::Qt, false).is_empty());
    }

    #[test]
    fn a11y_find_is_depth_first() {
        let tree = A11yNode {
            role: "window".into(),
            children: vec![A11yNode {
                role: "button".into(),
                name: "Save".into(),
                ..A11yNode::default()
            }],
            ..A11yNode::default()
        };
        assert!(tree.find("button", "Save").is_some());
        assert!(tree.focus().is_none());
        assert!(tree.find("button", "Open").is_none());
        assert_eq!(tree.parent_of("button", "Save").unwrap().role, "window");
        assert!(tree.parent_of("window", "").is_none());
    }
}
