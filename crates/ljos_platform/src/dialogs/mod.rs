//! QtQuick.Dialogs (doc.qt.io/qt-6/qtquickdialogs-index.html): a file,
//! folder, colour, font or message dialog declared in QML.
//!
//! `open()` asks the system's dialogs ([`DialogService`]) first; a dialog
//! the system does not have, or one told `DontUseNativeDialog`, is drawn
//! with the application's own controls ([`fallback`]). Either way the
//! answer comes back later and ends the dialog: the user's choice is
//! written into the dialog's properties, then it is accepted or rejected.
//! Nothing waits on the Engine thread while a dialog is open.

mod fallback;
mod service;

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

use ljos_engine::{
    Atom, Color, Cx, Engine, EngineError, Font, LogLevel, ObjectId, QmlList, QmlObject, QtObject,
    Url, Value, qml_impl,
};

pub use service::{
    CANCEL, DialogAnswer, DialogKind, DialogReply, DialogRequest, DialogService, NO, NameFilter,
    NoSystemDialogs, OK, STANDARD_BUTTONS, YES, button_named, button_role, role, system_buttons,
};

use crate::paths::{file_url, url_path};
use crate::platform::{CATEGORY, Platform};

/// `options` bits that say "draw it yourself", per type.
const FILE_NO_NATIVE: i32 = 8;
const COLOR_NO_NATIVE: i32 = 4;
const FONT_NO_NATIVE: i32 = 2;

fn standard_code() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![("StandardCode", vec![("Rejected", 0), ("Accepted", 1)])]
}

/// What every dialog has (Qt's `QQuickAbstractDialog`, which QML cannot
/// name).
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Dialogs",
    name = "AbstractDialog",
    extends = QtObject,
    enums = standard_code,
    default = "data",
    uncreatable
)]
pub struct AbstractDialog {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify, list)]
    pub data: QmlList,
    #[qml(notify, reset = ljos_engine::Reset::Default)]
    pub parent_window: Option<ObjectId>,
    #[qml(notify)]
    pub title: Atom,
    #[qml(notify, flags = "Qt.WindowFlags")]
    pub flags: i32,
    #[qml(notify, enumeration = "Qt.WindowModality")]
    pub modality: i32,
    #[qml(notify)]
    pub visible: bool,
    #[qml(notify)]
    pub result: i32,
}

impl Default for AbstractDialog {
    fn default() -> Self {
        AbstractDialog {
            object: QtObject::default(),
            data: QmlList::default(),
            parent_window: None,
            title: Atom::EMPTY,
            // Qt.Dialog, and window-modal: what a dialog starts as in Qt.
            flags: 3,
            modality: 1,
            visible: false,
            result: 0,
        }
    }
}

#[qml_impl]
impl AbstractDialog {
    #[qml(signal)]
    fn accepted();
    #[qml(signal)]
    fn rejected();

    #[qml(invokable)]
    fn open(cx: &Cx) -> Result<(), EngineError> {
        cx.set(AbstractDialog::VISIBLE, true)
    }

    /// Closing an open dialog rejects it, as Qt 6.11 does (measured: the
    /// documentation does not say); closing a closed one does nothing.
    #[qml(invokable)]
    fn close(cx: &Cx) -> Result<(), EngineError> {
        if cx
            .engine
            .untrack(|| cx.engine.get(cx.this, AbstractDialog::VISIBLE))?
        {
            done(cx.engine, cx.this, 0)
        } else {
            Ok(())
        }
    }

    #[qml(invokable)]
    fn accept(cx: &Cx) -> Result<(), EngineError> {
        done(cx.engine, cx.this, 1)
    }

    #[qml(invokable)]
    fn reject(cx: &Cx) -> Result<(), EngineError> {
        done(cx.engine, cx.this, 0)
    }

    #[qml(invokable)]
    fn done(cx: &Cx, result: Value) -> Result<(), EngineError> {
        done(cx.engine, cx.this, result.to_number() as i32)
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let me = cx.this;
        cx.engine
            .watch(me, AbstractDialog::VISIBLE.index, move |e, v| {
                if v.to_boolean() {
                    show(e, me);
                } else {
                    hide(e, me);
                }
            })?;
        // `visible: true` as declared opens the dialog once it is complete.
        if cx.get(AbstractDialog::VISIBLE)? {
            show(cx.engine, me);
        }
        Ok(())
    }
}

fn file_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "FileMode",
            vec![("OpenFile", 0), ("OpenFiles", 1), ("SaveFile", 2)],
        ),
        file_options(),
    ]
}

fn file_options() -> (&'static str, Vec<(&'static str, i64)>) {
    (
        "FileDialogOption",
        vec![
            ("ShowDirsOnly", 1),
            ("DontResolveSymlinks", 2),
            ("DontConfirmOverwrite", 4),
            ("DontUseNativeDialog", 8),
            ("ReadOnly", 16),
            ("HideNameFilterDetails", 32),
            ("DontUseCustomDirectoryIcons", 64),
        ],
    )
}

fn current_folder() -> Url {
    Url(Atom::from(
        std::env::current_dir()
            .map(|d| file_url(&d))
            .unwrap_or_default()
            .as_str(),
    ))
}

/// `FileDialog` (doc.qt.io/qt-6/qml-qtquick-dialogs-filedialog.html).
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Dialogs",
    name = "FileDialog",
    extends = AbstractDialog,
    enums = file_enums,
    default = "data"
)]
pub struct FileDialog {
    #[qml(base)]
    pub dialog: AbstractDialog,
    #[qml(notify, enumeration = "FileMode")]
    pub file_mode: i32,
    #[qml(notify)]
    pub selected_file: Url,
    #[qml(notify, readonly)]
    pub selected_files: Vec<Value>,
    #[qml(notify)]
    pub current_file: Url,
    #[qml(notify)]
    pub current_files: Vec<Value>,
    #[qml(notify)]
    pub current_folder: Url,
    #[qml(notify, reset = ljos_engine::Reset::Default, flags = "FileDialogOption")]
    pub options: i32,
    #[qml(notify, reset = ljos_engine::Reset::Default)]
    pub name_filters: Vec<Value>,
    #[qml(notify, reset = ljos_engine::Reset::Default)]
    pub default_suffix: Atom,
    #[qml(notify, reset = ljos_engine::Reset::Default)]
    pub accept_label: Atom,
    #[qml(notify, reset = ljos_engine::Reset::Default)]
    pub reject_label: Atom,
}

impl Default for FileDialog {
    fn default() -> Self {
        FileDialog {
            dialog: AbstractDialog::default(),
            file_mode: 0,
            selected_file: Url::default(),
            selected_files: Vec::new(),
            current_file: Url::default(),
            current_files: Vec::new(),
            current_folder: current_folder(),
            options: 0,
            name_filters: vec![Value::from("All Files (*)")],
            default_suffix: Atom::EMPTY,
            accept_label: Atom::EMPTY,
            reject_label: Atom::EMPTY,
        }
    }
}

#[qml_impl]
impl FileDialog {}

fn folder_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![file_options()]
}

/// `FolderDialog` (doc.qt.io/qt-6/qml-qtquick-dialogs-folderdialog.html).
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Dialogs",
    name = "FolderDialog",
    extends = AbstractDialog,
    enums = folder_enums,
    default = "data"
)]
pub struct FolderDialog {
    #[qml(base)]
    pub dialog: AbstractDialog,
    #[qml(notify)]
    pub current_folder: Url,
    #[qml(notify)]
    pub selected_folder: Url,
    #[qml(notify, reset = ljos_engine::Reset::Default, flags = "FileDialogOption")]
    pub options: i32,
    #[qml(notify, reset = ljos_engine::Reset::Default)]
    pub accept_label: Atom,
    #[qml(notify, reset = ljos_engine::Reset::Default)]
    pub reject_label: Atom,
}

impl Default for FolderDialog {
    fn default() -> Self {
        FolderDialog {
            dialog: AbstractDialog::default(),
            current_folder: current_folder(),
            selected_folder: Url::default(),
            options: 0,
            accept_label: Atom::EMPTY,
            reject_label: Atom::EMPTY,
        }
    }
}

#[qml_impl]
impl FolderDialog {}

fn color_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "ColorDialogOption",
        vec![
            ("ShowAlphaChannel", 1),
            ("NoButtons", 2),
            ("DontUseNativeDialog", 4),
            ("NoEyeDropperButton", 8),
        ],
    )]
}

/// `ColorDialog` (doc.qt.io/qt-6/qml-qtquick-dialogs-colordialog.html).
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Dialogs",
    name = "ColorDialog",
    extends = AbstractDialog,
    enums = color_enums,
    default = "data"
)]
pub struct ColorDialog {
    #[qml(base)]
    pub dialog: AbstractDialog,
    #[qml(notify)]
    pub selected_color: Color,
    #[qml(notify, reset = ljos_engine::Reset::Default, flags = "ColorDialogOption")]
    pub options: i32,
}

impl Default for ColorDialog {
    fn default() -> Self {
        ColorDialog {
            dialog: AbstractDialog::default(),
            selected_color: Color::parse("#ffffff").unwrap_or_default(),
            options: 0,
        }
    }
}

#[qml_impl]
impl ColorDialog {}

fn font_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "FontDialogOption",
        vec![
            ("NoButtons", 1),
            ("DontUseNativeDialog", 2),
            ("ScalableFonts", 4),
            ("NonScalableFonts", 8),
            ("MonospacedFonts", 16),
            ("ProportionalFonts", 32),
        ],
    )]
}

/// `FontDialog` (doc.qt.io/qt-6/qml-qtquick-dialogs-fontdialog.html).
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick.Dialogs",
    name = "FontDialog",
    extends = AbstractDialog,
    enums = font_enums,
    default = "data"
)]
pub struct FontDialog {
    #[qml(base)]
    pub dialog: AbstractDialog,
    #[qml(notify)]
    pub selected_font: Font,
    #[qml(notify)]
    pub current_font: Font,
    #[qml(notify, reset = ljos_engine::Reset::Default, flags = "FontDialogOption")]
    pub options: i32,
}

#[qml_impl]
impl FontDialog {}

fn message_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    let mut buttons = vec![("NoButton", 0)];
    buttons.extend(
        STANDARD_BUTTONS
            .iter()
            .map(|(flag, name, ..)| (*name, i64::from(*flag))),
    );
    buttons.push(("FirstButton", 0x0000_0400));
    buttons.push(("LastButton", 0x0800_0000));
    vec![
        ("StandardButton", buttons),
        (
            "ButtonRole",
            vec![
                ("InvalidRole", i64::from(role::INVALID)),
                ("AcceptRole", i64::from(role::ACCEPT)),
                ("RejectRole", i64::from(role::REJECT)),
                ("DestructiveRole", i64::from(role::DESTRUCTIVE)),
                ("ActionRole", i64::from(role::ACTION)),
                ("HelpRole", i64::from(role::HELP)),
                ("YesRole", i64::from(role::YES)),
                ("NoRole", i64::from(role::NO)),
                ("ResetRole", i64::from(role::RESET)),
                ("ApplyRole", i64::from(role::APPLY)),
            ],
        ),
    ]
}

/// `MessageDialog` (doc.qt.io/qt-6/qml-qtquick-dialogs-messagedialog.html).
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Dialogs",
    name = "MessageDialog",
    extends = AbstractDialog,
    enums = message_enums,
    default = "data"
)]
pub struct MessageDialog {
    #[qml(base)]
    pub dialog: AbstractDialog,
    #[qml(notify)]
    pub text: Atom,
    #[qml(notify)]
    pub informative_text: Atom,
    #[qml(notify)]
    pub detailed_text: Atom,
    #[qml(notify, flags = "StandardButton")]
    pub buttons: i32,
}

impl Default for MessageDialog {
    fn default() -> Self {
        MessageDialog {
            dialog: AbstractDialog::default(),
            text: Atom::EMPTY,
            informative_text: Atom::EMPTY,
            detailed_text: Atom::EMPTY,
            buttons: service::OK,
        }
    }
}

#[qml_impl]
impl MessageDialog {
    #[qml(signal, name = "buttonClicked")]
    fn button_clicked(button: i32, role: i32);
}

/// How an open dialog is shown.
#[derive(Clone, Copy, Debug)]
enum Shown {
    /// By the system, which answers the ticket.
    System(u64),
    /// Drawn: the popup.
    Drawn(ObjectId),
}

/// The dialogs of one engine that are open, and the drawn popups kept for
/// the next time.
#[derive(Debug, Default)]
struct Dialogs {
    shown: RefCell<HashMap<ObjectId, Shown>>,
    tickets: RefCell<HashMap<u64, ObjectId>>,
    next_ticket: std::cell::Cell<u64>,
}

impl Dialogs {
    fn of(engine: &Engine) -> std::rc::Rc<Dialogs> {
        engine.extension::<Dialogs>()
    }
}

fn is<T: ljos_engine::QmlType>(engine: &Engine, obj: ObjectId) -> bool {
    engine
        .meta_of(obj)
        .is_some_and(|m| m.inherits(T::static_meta()))
}

fn int(engine: &Engine, obj: ObjectId, name: &str) -> i32 {
    engine
        .untrack(|| engine.read_by_name(obj, name))
        .to_number() as i32
}

fn text(engine: &Engine, obj: ObjectId, name: &str) -> String {
    engine
        .untrack(|| engine.read_by_name(obj, name))
        .to_js_string()
        .to_string()
}

/// What the system is asked to show for `obj`. Its `kind` is `None` when
/// the dialog must be drawn.
fn request(engine: &Engine, obj: ObjectId) -> DialogRequest {
    let mut req = DialogRequest {
        title: text(engine, obj, "title"),
        ..DialogRequest::default()
    };
    let options = int(engine, obj, "options");
    if is::<FileDialog>(engine, obj) {
        req.kind = (options & FILE_NO_NATIVE == 0).then_some(match int(engine, obj, "fileMode") {
            1 => DialogKind::OpenFiles,
            2 => DialogKind::SaveFile,
            _ => DialogKind::OpenFile,
        });
        req.folder = url_path(&text(engine, obj, "currentFolder"));
        req.file_name = url_path(&text(engine, obj, "currentFile"))
            .or_else(|| url_path(&text(engine, obj, "selectedFile")))
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
        if let Value::List(filters) = engine.untrack(|| engine.read_by_name(obj, "nameFilters")) {
            req.filters = filters
                .iter()
                .map(|f| NameFilter::parse(&f.to_js_string()))
                .collect();
        }
        req.accept_label = text(engine, obj, "acceptLabel");
    } else if is::<FolderDialog>(engine, obj) {
        req.kind = (options & FILE_NO_NATIVE == 0).then_some(DialogKind::Folder);
        req.folder = url_path(&text(engine, obj, "currentFolder"));
        req.accept_label = text(engine, obj, "acceptLabel");
    } else if is::<ColorDialog>(engine, obj) {
        req.kind = (options & COLOR_NO_NATIVE == 0).then_some(DialogKind::Color);
    } else if is::<FontDialog>(engine, obj) {
        req.kind = (options & FONT_NO_NATIVE == 0).then_some(DialogKind::Font);
    } else if is::<MessageDialog>(engine, obj) {
        req.kind = Some(DialogKind::Message);
        req.text = text(engine, obj, "text");
        req.informative_text = text(engine, obj, "informativeText");
        req.detailed_text = text(engine, obj, "detailedText");
        req.buttons = int(engine, obj, "buttons");
    }
    req
}

fn show(engine: &Engine, obj: ObjectId) {
    let dialogs = Dialogs::of(engine);
    if dialogs.shown.borrow().contains_key(&obj) {
        return;
    }
    let req = request(engine, obj);
    let platform = Platform::of(engine);
    let service = platform.dialogs();
    if req.kind.is_some() && service.can_show(&req) {
        let ticket = dialogs.next_ticket.get();
        dialogs.next_ticket.set(ticket + 1);
        dialogs
            .shown
            .borrow_mut()
            .insert(obj, Shown::System(ticket));
        dialogs.tickets.borrow_mut().insert(ticket, obj);
        service.show(
            req,
            DialogAnswer {
                reply: Some(platform.expect()),
                ticket,
            },
        );
        return;
    }
    match fallback::open(engine, obj) {
        Some(popup) => {
            dialogs.shown.borrow_mut().insert(obj, Shown::Drawn(popup));
        }
        None => {
            engine.log(
                LogLevel::Warning,
                CATEGORY,
                format!(
                    "{}: no dialog to show: the system has none of this kind and the \
                     application's own could not be made (is QtQuick.Controls registered?)",
                    text(engine, obj, "title")
                ),
            );
        }
    }
}

fn hide(engine: &Engine, obj: ObjectId) {
    let shown = Dialogs::of(engine).shown.borrow_mut().remove(&obj);
    match shown {
        Some(Shown::System(ticket)) => {
            Dialogs::of(engine).tickets.borrow_mut().remove(&ticket);
            Platform::of(engine).dialogs().close(ticket);
        }
        Some(Shown::Drawn(popup)) => fallback::close(engine, popup),
        None => {}
    }
}

/// Ends the dialog with `result`: hides it, then says how it ended.
fn done(engine: &Engine, obj: ObjectId, result: i32) -> Result<(), EngineError> {
    engine.set(obj, AbstractDialog::RESULT, result)?;
    engine.set(obj, AbstractDialog::VISIBLE, false)?;
    let signal = if result == 1 { "accepted" } else { "rejected" };
    engine.emit_by_name(obj, signal, &[])
}

fn report(engine: &Engine, what: Result<(), EngineError>) {
    if let Err(e) = what {
        engine.log(
            LogLevel::Warning,
            CATEGORY,
            format!("a dialog's answer: {e}"),
        );
    }
}

/// A button of a message was pressed: says which, then ends the dialog
/// as the button's role does. A button of another role (Apply, Help)
/// only closes it.
pub(crate) fn clicked(engine: &Engine, obj: ObjectId, button: i32) {
    let role = button_role(button);
    report(
        engine,
        engine.emit_by_name(
            obj,
            "buttonClicked",
            &[Value::Int(button), Value::Int(role)],
        ),
    );
    if !engine.is_alive(obj) {
        return;
    }
    let ended = match role {
        role::ACCEPT | role::YES => done(engine, obj, 1),
        role::REJECT | role::NO => done(engine, obj, 0),
        _ => engine.set(obj, AbstractDialog::VISIBLE, false),
    };
    report(engine, ended);
}

/// The files or folder chosen go into the dialog's properties.
pub(crate) fn chose(engine: &Engine, obj: ObjectId, files: &[PathBuf]) -> Result<(), EngineError> {
    let urls: Vec<Value> = files
        .iter()
        .map(|p| Value::Url(Atom::from(file_url(p).as_str())))
        .collect();
    let first = Url(Atom::from(
        files
            .first()
            .map(|p| file_url(p))
            .unwrap_or_default()
            .as_str(),
    ));
    if is::<FolderDialog>(engine, obj) {
        engine.set(obj, FolderDialog::SELECTED_FOLDER, first.clone())?;
        engine.set(obj, FolderDialog::CURRENT_FOLDER, first)?;
    } else if is::<FileDialog>(engine, obj) {
        // Read-only to QML; the dialog itself writes it.
        engine.write_internal(
            obj,
            FileDialog::SELECTED_FILES.index,
            Value::List(std::sync::Arc::new(urls.clone())),
            ljos_reactive::WriteKind::Imperative,
        )?;
        engine.set(obj, FileDialog::CURRENT_FILES, urls)?;
        engine.set(obj, FileDialog::SELECTED_FILE, first.clone())?;
        engine.set(obj, FileDialog::CURRENT_FILE, first)?;
    }
    Ok(())
}

/// The system answered the dialog with `ticket`.
pub(crate) fn system_answered(engine: &Engine, ticket: u64, reply: DialogReply) {
    let dialogs = Dialogs::of(engine);
    // Closed from QML meanwhile: the answer is not heard.
    let Some(obj) = dialogs.tickets.borrow_mut().remove(&ticket) else {
        return;
    };
    dialogs.shown.borrow_mut().remove(&obj);
    if !engine.is_alive(obj) {
        return;
    }
    match reply {
        DialogReply::Files(files) if !files.is_empty() => {
            let chosen = chose(engine, obj, &files);
            report(engine, chosen.and_then(|()| done(engine, obj, 1)));
        }
        DialogReply::Button(button) => clicked(engine, obj, button),
        DialogReply::Files(_) | DialogReply::Dismissed => report(engine, done(engine, obj, 0)),
    }
}

/// A drawn dialog answered: `accepted` and, for a message, the button.
pub(crate) fn drawn_answered(engine: &Engine, obj: ObjectId, accepted: bool, button: i32) {
    // A dialog already ended (closed from QML, say) hears no more.
    let open = engine.is_alive(obj)
        && engine
            .untrack(|| engine.get(obj, AbstractDialog::VISIBLE))
            .unwrap_or(false);
    if !open {
        return;
    }
    if is::<MessageDialog>(engine, obj) && button != 0 {
        clicked(engine, obj, button);
        return;
    }
    if accepted {
        report(engine, fallback::take_values(engine, obj));
    }
    report(engine, done(engine, obj, i32::from(accepted)));
}
