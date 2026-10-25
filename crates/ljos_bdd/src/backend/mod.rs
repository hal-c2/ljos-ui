//! What a step can ask of a running QML application, independent of who runs it.

use std::fmt;

use serde_json::Value;

pub mod desktop;
pub mod fake_platform;
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

/// One node of the accessibility tree, as an assistive technology sees it;
/// the Ljós backend reads it straight off the scene's tree.
pub use ljos_a11y::AccessibleNode as A11yNode;

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

/// The fingers down in one frame of touch input: `(id, x, y)`.
pub type TouchFrame = Vec<(u32, f64, f64)>;

/// What a pointer does in one step of a press, drag and release.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerStep {
    Press,
    /// With the button held if it was pressed and not yet released.
    Move,
    Release,
}

/// A mouse button other than the left one, which `touch` and `pointer`
/// use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OtherButton {
    Right,
    Middle,
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

/// A step of a drag that comes from another application.
#[derive(Clone, Debug, PartialEq)]
pub enum OutsideDrag {
    /// It comes over the window at this point, offering these MIME types
    /// with these contents.
    Enter {
        formats: Vec<(String, String)>,
        at: (f64, f64),
    },
    Move((f64, f64)),
    /// It leaves the window, or is given up.
    Leave,
    /// It is let go where it was last seen.
    Drop,
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

/// The surface the steps drive: a running application a scenario loads,
/// reads, writes and acts on. `!Send` is fine: cucumber 0.23 does not need
/// `Send`.
///
/// Every backend has these verbs. What only some have (more kinds of
/// input, the system around the application, the runtime's own tools) is
/// a capability trait, reached through the `as_` method of its name. A
/// step asks for it by the method of `dyn Backend` of the same name
/// (`b.input()?`), which says which backend lacks it.
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

    /// Click the control with this accessible role and name, as a hand
    /// would: in its middle, with the pointer moved there first.
    fn click(&mut self, role: &str, name: &str) -> Result<()>;

    /// Press and release a key chord (`"Ctrl+S"`, `"Tab"`), as a keyboard does.
    fn key(&mut self, chord: &str) -> Result<()>;

    /// One half of a click, or a move between the two: what a held or
    /// dragged pointer is made of.
    fn pointer(&mut self, step: PointerStep, x: f64, y: f64) -> Result<()>;

    fn a11y_tree(&mut self) -> Result<A11yNode>;

    fn log(&mut self) -> Result<Vec<LogEntry>>;

    /// PNG bytes of the window.
    fn grab(&mut self) -> Result<Vec<u8>>;

    fn as_input(&mut self) -> Option<&mut dyn Input> {
        None
    }

    fn as_assistive(&mut self) -> Option<&mut dyn Assistive> {
        None
    }

    fn as_files(&mut self) -> Option<&mut dyn Files> {
        None
    }

    fn as_environment(&mut self) -> Option<&mut dyn Environment> {
        None
    }

    fn as_platform(&mut self) -> Option<&mut dyn Platform> {
        None
    }

    fn as_application(&mut self) -> Option<&mut dyn Application> {
        None
    }

    fn as_reload(&mut self) -> Option<&mut dyn Reload> {
        None
    }

    fn as_probes(&mut self) -> Option<&mut dyn Probes> {
        None
    }

    fn as_tooling(&mut self) -> Option<&mut dyn Tooling> {
        None
    }
}

/// A capability the backend lacks: not yet written, or, for Ljós's own
/// extensions, something the oracle has nothing like.
fn lacks(backend: &'static str, what: &'static str, own: bool) -> BackendError {
    if own {
        BackendError::Unsupported { backend, what }
    } else {
        BackendError::NotImplemented(what)
    }
}

impl dyn Backend + '_ {
    /// What the backend can do as [`Input`].
    ///
    /// # Errors
    /// It cannot.
    pub fn input(&mut self) -> Result<&mut dyn Input> {
        let backend = self.name();
        self.as_input().ok_or_else(|| {
            lacks(
                backend,
                "input beyond a click, a key and the pointer",
                false,
            )
        })
    }

    /// What the backend can do as [`Assistive`].
    ///
    /// # Errors
    /// It cannot.
    pub fn assistive(&mut self) -> Result<&mut dyn Assistive> {
        let backend = self.name();
        self.as_assistive()
            .ok_or_else(|| lacks(backend, "an assistive technology's requests", false))
    }

    /// What the backend can do as [`Files`].
    ///
    /// # Errors
    /// It cannot.
    pub fn files(&mut self) -> Result<&mut dyn Files> {
        let backend = self.name();
        self.as_files()
            .ok_or_else(|| lacks(backend, "files beside the QML", false))
    }

    /// What the backend can do as [`Environment`].
    ///
    /// # Errors
    /// It cannot.
    pub fn environment(&mut self) -> Result<&mut dyn Environment> {
        let backend = self.name();
        self.as_environment()
            .ok_or_else(|| lacks(backend, "the environment the application starts in", false))
    }

    /// What the backend can do as [`Platform`].
    ///
    /// # Errors
    /// It cannot.
    pub fn platform(&mut self) -> Result<&mut dyn Platform> {
        let backend = self.name();
        self.as_platform()
            .ok_or_else(|| lacks(backend, "the system's services", false))
    }

    /// What the backend can do as [`Application`].
    ///
    /// # Errors
    /// It cannot.
    pub fn application(&mut self) -> Result<&mut dyn Application> {
        let backend = self.name();
        self.as_application()
            .ok_or_else(|| lacks(backend, "the application around the QML", false))
    }

    /// What the backend can do as [`Reload`].
    ///
    /// # Errors
    /// It cannot.
    pub fn reloading(&mut self) -> Result<&mut dyn Reload> {
        let backend = self.name();
        self.as_reload()
            .ok_or_else(|| lacks(backend, "hot reload", true))
    }

    /// What the backend can do as [`Probes`].
    ///
    /// # Errors
    /// It cannot.
    pub fn probes(&mut self) -> Result<&mut dyn Probes> {
        let backend = self.name();
        self.as_probes()
            .ok_or_else(|| lacks(backend, "seeing inside the runtime", true))
    }

    /// What the backend can do as [`Tooling`].
    ///
    /// # Errors
    /// It cannot.
    pub fn tooling(&mut self) -> Result<&mut dyn Tooling> {
        let backend = self.name();
        self.as_tooling()
            .ok_or_else(|| lacks(backend, "Ljós's own tooling", true))
    }
}

/// Input besides a click by name, a key and the pointer's steps.
pub trait Input: Backend {
    /// Hold a chord down until the keyboard has repeated it `repeats` times,
    /// then let go.
    fn key_hold(&mut self, chord: &str, repeats: u32) -> Result<()>;

    /// The window becomes, or stops being, the one the keyboard types into.
    fn window_active(&mut self, active: bool) -> Result<()>;

    fn touch(&mut self, x: f64, y: f64) -> Result<()>;

    /// Type `text` on the keyboard, one key press per character.
    fn type_text(&mut self, _text: &str) -> Result<()> {
        Err(BackendError::NotImplemented("typing text"))
    }

    /// A click of `button` at one place: down and up, with nothing between.
    fn click_button(&mut self, _button: OtherButton, _x: f64, _y: f64) -> Result<()> {
        Err(BackendError::NotImplemented("a click of another button"))
    }

    /// A click of the left button with the modifiers of `"Ctrl+Shift"`
    /// held.
    fn click_holding(&mut self, _modifiers: &str, _x: f64, _y: f64) -> Result<()> {
        Err(BackendError::NotImplemented("a click with modifiers held"))
    }

    /// Two clicks at one place in quick succession.
    fn double_click(&mut self, _x: f64, _y: f64) -> Result<()> {
        Err(BackendError::NotImplemented("a double click"))
    }

    /// Press at `from`, move to `to` with the button held, release there.
    fn drag(&mut self, _from: (f64, f64), _to: (f64, f64)) -> Result<()> {
        Err(BackendError::NotImplemented("a drag"))
    }

    /// Fingers on the screen, frame by frame: in each, every finger down
    /// then, as `(id, x, y)`. A finger new in a frame comes down, one gone
    /// from it is lifted, and after the last frame every finger is. Frames
    /// are some milliseconds apart, as a touch screen reports them.
    fn touch_frames(&mut self, _frames: &[TouchFrame]) -> Result<()> {
        Err(BackendError::NotImplemented("fingers on a touch screen"))
    }

    /// The wheel turns, or a touchpad scrolls, with the pointer at `at`.
    fn wheel(&mut self, _at: (f64, f64), _by: WheelBy) -> Result<()> {
        Err(BackendError::NotImplemented("the wheel"))
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
}

/// What an assistive technology asks of the application, besides reading
/// its tree.
pub trait Assistive: Backend {
    /// What a screen reader's "move the focus here" does to the node with
    /// this role and name. A node that refuses is not an error.
    fn a11y_focus(&mut self, role: &str, name: &str) -> Result<()>;

    /// An assistive technology asks the node with this role and name to
    /// do something. A node that refuses is not an error.
    fn a11y_act(&mut self, _role: &str, _name: &str, _action: A11yAction) -> Result<()> {
        Err(BackendError::NotImplemented(
            "an assistive technology's requests",
        ))
    }

    /// An assistive technology asks the node with this role and name to
    /// select or change its text. A node that refuses is not an error.
    fn a11y_text(&mut self, _role: &str, _name: &str, _action: A11yTextAction) -> Result<()> {
        Err(BackendError::NotImplemented(
            "an assistive technology's requests",
        ))
    }
}

/// Files that are not QML, beside the QML files or in the user's home.
pub trait Files: Backend {
    /// Put a file that is not QML (an image) beside the QML files, where a
    /// relative URL in them finds it.
    fn resource(&mut self, name: &str, bytes: &[u8]) -> Result<()>;

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
}

/// What the application starts with (a style, a locale, a clock, fonts)
/// and what the system says while it runs (settings, the language).
pub trait Environment: Backend {
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
    /// The program starts with the Qt Quick Controls style `name`, as
    /// `QT_QUICK_CONTROLS_STYLE` chooses it: a built-in style or a module
    /// of the scenario's files.
    fn controls_style(&mut self, _name: &str) -> Result<()> {
        Err(BackendError::NotImplemented("choosing a style"))
    }

    /// The program starts with `-style name` on its command line, which
    /// comes before the variables and the file.
    fn command_line_style(&mut self, _name: &str) -> Result<()> {
        Err(BackendError::NotImplemented("a style on the command line"))
    }

    /// The program starts with this text in the file
    /// `QT_QUICK_CONTROLS_CONF` names.
    fn controls_configuration(&mut self, _text: &str) -> Result<()> {
        Err(BackendError::NotImplemented(
            "a controls configuration file",
        ))
    }

    /// The program starts with the variable `name` (one of
    /// `QT_QUICK_CONTROLS_`) set to `value`.
    fn controls_variable(&mut self, _name: &str, _value: &str) -> Result<()> {
        Err(BackendError::NotImplemented("a controls variable"))
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

    /// The wall clock a script's `Date` reads is `epoch_ms` (milliseconds
    /// since the UNIX epoch) when the application starts, and moves with
    /// application time.
    fn wall_clock(&mut self, _epoch_ms: f64) -> Result<()> {
        Err(BackendError::NotImplemented("a pinned wall clock"))
    }

    /// Every binding tier 1 runs is run in tier 2 as well and the two
    /// compared, a difference being a warning; or not (`on` false).
    fn tier1_check(&mut self, _on: bool) -> Result<()> {
        Err(BackendError::NotImplemented("the tier 1 cross-check"))
    }

    /// The limits on a run of JavaScript read a clock that moves ten
    /// milliseconds each time the interrupt polls it (about every 10k
    /// opcodes), and by `native` over
    /// each piece of native work a script asks for (compiling a component,
    /// making an object).
    fn js_clock(&mut self, _native: std::time::Duration) -> Result<()> {
        Err(BackendError::NotImplemented(
            "a clock for JavaScript's time limits",
        ))
    }

    /// The application takes its standard keys (`StandardKey`, the keys
    /// of an entry) from `platform`'s key bindings, whatever the machine.
    fn key_bindings(&mut self, _platform: ljos_quick::keys::Platform) -> Result<()> {
        Err(BackendError::NotImplemented(
            "another platform's key bindings",
        ))
    }

    /// Text is set in the fonts installed on the machine, as a desktop
    /// application's is, instead of only the fonts the toolkit carries.
    /// Given before anything is loaded.
    fn system_fonts(&mut self) -> Result<()> {
        Err(BackendError::NotImplemented("the system's fonts"))
    }
}

/// The system's services: the clipboard, dialogs, drags from other
/// applications, permissions, sensors, a position, opening URLs, and what
/// the window system draws behind a window.
pub trait Platform: Backend {
    /// What the window system is asked to draw behind the window at
    /// `object`; `None` for nothing.
    fn window_backdrop(&mut self, _object: &str) -> Result<Option<BackdropAsked>> {
        Err(BackendError::NotImplemented("window backdrops"))
    }

    /// The window system brings no files dragged in from other
    /// applications, as winit has none on Wayland.
    fn no_external_drops(&mut self) -> Result<()> {
        Err(BackendError::NotImplemented(
            "a window system without drops from outside",
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

    /// The kind (`"open file"`) and title of each dialog the system shows.
    fn system_dialogs(&mut self) -> Result<Vec<(String, String)>> {
        Err(BackendError::NotImplemented("the system's dialogs"))
    }

    /// The user answers the dialog the system shows.
    fn answer_system_dialog(&mut self, _answer: SystemDialogAnswer) -> Result<()> {
        Err(BackendError::NotImplemented("the system's dialogs"))
    }

    /// The system cannot take away a dialog it shows, from now on.
    fn system_dialogs_stay(&mut self) -> Result<()> {
        Err(BackendError::NotImplemented("the system's dialogs"))
    }

    /// A step of a drag from another application, as the window system
    /// reports it.
    fn drag_from_outside(&mut self, _step: &OutsideDrag) -> Result<()> {
        Err(BackendError::NotImplemented(
            "drags from other applications",
        ))
    }

    /// What the application answered the last step: whether a drop target
    /// under the drag takes it (after `Drop`, whether one took the drop).
    fn outside_drag_taken(&mut self) -> Result<bool> {
        Err(BackendError::NotImplemented(
            "drags from other applications",
        ))
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

    /// The system's position service is at `latitude`, `longitude` and
    /// `altitude` (NaN when it is not known). Said before anything is
    /// loaded; without it there is no service, as on a desktop.
    fn system_position(&mut self, _latitude: f64, _longitude: f64, _altitude: f64) -> Result<()> {
        Err(BackendError::NotImplemented("a position the system is at"))
    }

    /// The system's position moves: every started position source is
    /// given the new fix.
    fn system_position_moves(
        &mut self,
        _latitude: f64,
        _longitude: f64,
        _altitude: f64,
    ) -> Result<()> {
        Err(BackendError::NotImplemented("a position the system is at"))
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
}

/// What the application written around the QML does: windows of its
/// own, models written in Rust, files it watches, faults in native code.
pub trait Application: Backend {
    /// The application makes a root of the loaded file `name` with no
    /// parent, as it makes each window it opens beside the first.
    fn open_window(&mut self, _name: &str) -> Result<()> {
        Err(BackendError::NotImplemented("a window of its own"))
    }

    /// The application makes a root of the loaded file `name` and puts it
    /// in the window of the root `old`, which goes.
    fn put_in_window(&mut self, _name: &str, _old: &str) -> Result<()> {
        Err(BackendError::NotImplemented("another root in a window"))
    }

    /// The application, in development, watches the loaded files on disk
    /// under each of `roots` (directories named by the files' paths,
    /// earlier first) and makes its root of the first of `entries` that
    /// compiles and can be made, at the start and at every reload.
    fn watch_roots(&mut self, _roots: &[String], _entries: &[String]) -> Result<()> {
        Err(BackendError::NotImplemented("a watch over files on disk"))
    }

    /// The file `name` of the watched files is written with `text` (None:
    /// it is removed), and the application takes what the watcher brings.
    fn save_on_disk(&mut self, _name: &str, _text: Option<&str>) -> Result<()> {
        Err(BackendError::NotImplemented("a watch over files on disk"))
    }

    /// As [`Application::open_window`], for an application that made the
    /// window's `Fixtures` `Document` itself, titled `title`, and gives it
    /// to the window.
    fn open_window_given(&mut self, _name: &str, _title: &str) -> Result<()> {
        Err(BackendError::NotImplemented("a window given its singleton"))
    }

    /// The title of the `Document` the application gave a window.
    fn given_title(&mut self) -> Result<String> {
        Err(BackendError::NotImplemented("a window given its singleton"))
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

    /// Native code running for the object panics with `message`, as a
    /// bug in a type written in Rust does.
    fn panic_in(&mut self, _object: &str, _message: &str) -> Result<()> {
        Err(self.no_faults())
    }

    /// The last fault that no UI boundary took.
    fn application_fault(&mut self) -> Result<Option<ApplicationFault>> {
        Err(self.no_faults())
    }

    /// Faults and their boundaries are Ljós's own.
    fn no_faults(&self) -> BackendError {
        BackendError::Unsupported {
            backend: self.name(),
            what: "faults and UI boundaries",
        }
    }
}

/// Files changed under the running application, as a development build
/// reloads them.
pub trait Reload: Backend {
    /// The scenario changes files while the application runs: what a
    /// reload needs is noted from the first load on, as a development
    /// build notes it. Called before anything is loaded.
    fn watch_files(&mut self) {}
    /// A file of the running application is saved with new text, or
    /// removed (`None`). It is applied when the files have been quiet.
    fn change_file(&mut self, _name: &str, _source: Option<&str>) -> Result<()> {
        Err(self.no_reload())
    }

    /// The editor closed the file it saved last (`change_file`): the save
    /// is whole, and the quiet time is a short one.
    fn finish_save(&mut self, _name: &str) -> Result<()> {
        Err(self.no_reload())
    }

    /// A file that is not QML ([`Files::resource`]) is saved again
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

    /// Hot reload is a Ljós extension; the oracle has nothing like it.
    fn no_reload(&self) -> BackendError {
        BackendError::Unsupported {
            backend: self.name(),
            what: "hot reload",
        }
    }
}

/// What the runtime can say of itself that a QML program cannot see:
/// what it has scheduled, what it keeps, how much work it does.
pub trait Probes: Backend {
    /// What the application would wake for if no one touched it: the
    /// next timer, the next frame of something moving, an answer it waits
    /// for. Empty when it is idle. The oracle's event loop cannot be read.
    fn scheduled(&mut self) -> Result<Vec<String>> {
        Err(BackendError::Unsupported {
            backend: self.name(),
            what: "seeing what the application has scheduled",
        })
    }

    /// Shows the first frame as a desktop window does, which waits a short
    /// while for the large pictures it started to decode. The oracle
    /// decodes them on the spot and has no such frame.
    fn first_frame(&mut self) -> Result<()> {
        Err(BackendError::Unsupported {
            backend: self.name(),
            what: "showing a first frame that waits for its pictures",
        })
    }

    /// The size in pixels of the picture kept for the image `object`, None
    /// while none is. The oracle does not say what it keeps.
    fn kept_picture(&mut self, _object: &str) -> Result<Option<(u32, u32)>> {
        Err(BackendError::Unsupported {
            backend: self.name(),
            what: "seeing the size a picture is kept at",
        })
    }

    /// Starts counting the passes the window runs (a run of its scene and
    /// a paint), less those the counting itself causes. Only a shell runs
    /// passes; the oracle's event loop cannot be read.
    fn count_passes(&mut self) -> Result<()> {
        Err(BackendError::Unsupported {
            backend: self.name(),
            what: "counting the passes a window runs",
        })
    }

    /// The passes run since [`Probes::count_passes`].
    fn passes_counted(&mut self) -> Result<u64> {
        Err(BackendError::Unsupported {
            backend: self.name(),
            what: "counting the passes a window runs",
        })
    }

    /// Starts counting how many times Text items are laid out to say what
    /// they measure. The oracle does not say when it lays text out.
    fn count_text_measures(&mut self) -> Result<()> {
        Err(BackendError::Unsupported {
            backend: self.name(),
            what: "counting the times text is measured",
        })
    }

    /// The times text was measured since [`Probes::count_text_measures`].
    fn text_measures_counted(&mut self) -> Result<u64> {
        Err(BackendError::Unsupported {
            backend: self.name(),
            what: "counting the times text is measured",
        })
    }

    /// Starts counting how often each of the application's threads is
    /// woken. Only a shell's process can be read: the oracle's threads
    /// are its own.
    fn count_wakes(&mut self) -> Result<()> {
        Err(BackendError::Unsupported {
            backend: self.name(),
            what: "counting the wakes of the application's threads",
        })
    }

    /// Each thread but the window's own that woke since
    /// [`Probes::count_wakes`], by name, with how many times it did.
    fn wakes_counted(&mut self) -> Result<Vec<(String, u64)>> {
        Err(BackendError::Unsupported {
            backend: self.name(),
            what: "counting the wakes of the application's threads",
        })
    }

    /// Starts `ljos observe` on the running application. Only a shell is
    /// observed: the observer is a window of its own.
    fn observe(&mut self) -> Result<()> {
        Err(BackendError::Unsupported {
            backend: self.name(),
            what: "an observer window",
        })
    }

    /// Starts counting the work the application and its observer do
    /// (passes, frames, jobs, messages), less what the counting causes.
    fn count_work(&mut self) -> Result<()> {
        Err(BackendError::Unsupported {
            backend: self.name(),
            what: "counting the work an observed application does",
        })
    }

    /// The work done since [`Probes::count_work`], one line for each
    /// kind that was; empty when there was none.
    fn work_counted(&mut self) -> Result<Vec<String>> {
        Err(BackendError::Unsupported {
            backend: self.name(),
            what: "counting the work an observed application does",
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
}

/// Ljós's tools: the QML test runner, the inspector and the `top` overlay.
pub trait Tooling: Backend {
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

    /// Backends for a scenario. Only `@qt-parity` scenarios reach Qt; a
    /// `@desktop` one runs Ljós on the desktop sandbox instead of headless.
    #[must_use]
    pub fn backends_for(self, qt_parity: bool, desktop: bool) -> Vec<Box<dyn Backend>> {
        let mut out: Vec<Box<dyn Backend>> = Vec::new();
        if desktop {
            if matches!(self, Self::Ljos | Self::Both) {
                out.push(Box::new(desktop::DesktopBackend::new()));
            }
            return out;
        }
        if matches!(self, Self::Qt | Self::Both) && qt_parity {
            out.push(Box::new(qt::QtBackend::new()));
        }
        if matches!(self, Self::Ljos | Self::Both) {
            out.push(Box::new(ljos::LjosBackend::new()));
        }
        out
    }
}

/// Gives a program the runner starts a home of its own under `dir`: what
/// it saves (Settings, LocalStorage) stays with the scenario and never
/// reaches the developer's directories, and a relative XDG variable falls
/// back inside it. The user's fonts and their configuration come along,
/// so text measures as it does outside. Ljós is told the real home, which
/// it then refuses to write in.
pub(crate) fn own_home(
    command: &mut std::process::Command,
    dir: &std::path::Path,
) -> std::io::Result<()> {
    use std::path::PathBuf;
    let real = std::env::var_os("HOME").map(PathBuf::from);
    let user = |key: &str, fallback: &str| {
        std::env::var_os(key)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| real.as_ref().map(|h| h.join(fallback)))
    };
    let home = dir.join("home");
    let (config, data) = (home.join(".config"), home.join(".local/share"));
    for d in [
        &config,
        &data,
        &home.join(".local/state"),
        &home.join(".cache"),
    ] {
        std::fs::create_dir_all(d)?;
    }
    for (from, to) in [
        (
            user("XDG_CONFIG_HOME", ".config"),
            config.join("fontconfig"),
        ),
        (user("XDG_DATA_HOME", ".local/share"), data.join("fonts")),
    ] {
        let Some(from) = from.map(|f| f.join(to.file_name().unwrap_or_default())) else {
            continue;
        };
        if from.is_dir() && !to.exists() {
            std::os::unix::fs::symlink(&from, &to)?;
        }
    }
    command
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &config)
        .env("XDG_DATA_HOME", &data)
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env("XDG_CACHE_HOME", home.join(".cache"));
    if let Some(real) = real {
        command.env(ljos_platform::GUARD_HOME_VAR, real);
    }
    Ok(())
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
            m.backends_for(p, false).iter().map(|b| b.name()).collect()
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
