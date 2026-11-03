//! `WebEngineView`: a live web page as an item of the scene.
//!
//! The page is a picture in the item's display list, like a video
//! frame's: the backend draws it off screen, hands each new picture
//! through its [`Sink`](crate::backend::Sink), and the view keeps the
//! newest and repaints. So the page is clipped, transformed, faded and
//! stacked as any item is; no window of its own sits over the scene.
//!
//! A view makes its browser when it is first given a URL, not before: a
//! view that never loads costs nothing. While a load is under way the
//! view holds a promise of the platform, so a host that waits for its
//! answers (a test) waits for the page, and its first picture.

use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use ljos_engine::{
    Atom, Color, Cx, Engine, EngineError, LogLevel, NodeId, ObjectId, QmlObject, QmlType, Url,
    Value, qml_impl,
};
use ljos_platform::{Platform, Reply};
use ljos_quick::{Item, ItemPaint, TextEngine};
use ljos_render::{DisplayListBuilder, DrawOp, ImageHandle, Rgba, kurbo};

use crate::backend::{self, Browser, Event, LoadStatus};
use crate::types::{
    WebEngine, WebEngineLoadingInfo, WebEngineNewWindowRequest, WebEngineProfile,
    WebEngineScriptCollection, WebEngineSettings,
};

fn view_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "LoadStatus",
            vec![
                ("LoadStartedStatus", 0),
                ("LoadStoppedStatus", 1),
                ("LoadSucceededStatus", 2),
                ("LoadFailedStatus", 3),
            ],
        ),
        (
            "ErrorDomain",
            vec![
                ("NoErrorDomain", 0),
                ("InternalErrorDomain", 1),
                ("ConnectionErrorDomain", 2),
                ("CertificateErrorDomain", 3),
                ("HttpErrorDomain", 4),
                ("FtpErrorDomain", 5),
                ("DnsErrorDomain", 6),
                ("HttpStatusCodeDomain", 7),
            ],
        ),
        (
            "JavaScriptConsoleMessageLevel",
            vec![
                ("InfoMessageLevel", 0),
                ("WarningMessageLevel", 1),
                ("ErrorMessageLevel", 2),
            ],
        ),
        (
            "RenderProcessTerminationStatus",
            vec![
                ("NormalTerminationStatus", 0),
                ("AbnormalTerminationStatus", 1),
                ("CrashedTerminationStatus", 2),
                ("KilledTerminationStatus", 3),
            ],
        ),
        (
            "NewViewDestination",
            vec![
                ("NewViewInWindow", 0),
                ("NewViewInTab", 1),
                ("NewViewInDialog", 2),
                ("NewViewInBackgroundTab", 3),
            ],
        ),
        (
            "WebAction",
            vec![
                ("NoWebAction", -1),
                ("Back", 0),
                ("Forward", 1),
                ("Stop", 2),
                ("Reload", 3),
            ],
        ),
    ]
}

/// The zoom a page can have, as Chromium allows it.
const ZOOM_MIN: f64 = 0.25;
const ZOOM_MAX: f64 = 5.0;

/// `WebEngineView` (QtWebEngine).
#[derive(QmlObject)]
#[qml(
    module = "QtWebEngine",
    name = "WebEngineView",
    extends = Item,
    default = "data",
    enums = view_enums
)]
pub struct WebEngineView {
    #[qml(base)]
    pub item: Item,
    #[qml(notify, write = Self::take_url)]
    pub url: Url,
    #[qml(notify, readonly)]
    pub title: String,
    #[qml(notify, readonly)]
    pub icon: Url,
    #[qml(notify, readonly)]
    pub loading: bool,
    #[qml(notify, readonly)]
    pub load_progress: i32,
    #[qml(notify, readonly)]
    pub can_go_back: bool,
    #[qml(notify, readonly)]
    pub can_go_forward: bool,
    #[qml(notify, write = Self::take_zoom)]
    pub zoom_factor: f64,
    #[qml(notify, object = "WebEngineProfile")]
    pub profile: Option<ObjectId>,
    #[qml(group)]
    pub settings: WebEngineSettings,
    #[qml(readonly, constant, object = "WebEngineScriptCollection")]
    pub user_scripts: Option<ObjectId>,
    #[qml(notify)]
    pub audio_muted: bool,
    #[qml(notify, readonly)]
    pub recently_audible: bool,
    #[qml(notify)]
    pub background_color: Color,
    #[qml(notify, readonly)]
    pub is_full_screen: bool,
    #[qml(notify)]
    pub active_focus_on_press: bool,
    #[qml(notify, object = "WebEngineView")]
    pub dev_tools_view: Option<ObjectId>,
    #[qml(notify, object = "WebEngineView")]
    pub inspected_view: Option<ObjectId>,
    #[qml(skip)]
    run: Run,
}

/// What a view holds besides its properties.
#[derive(Default)]
struct Run {
    browser: Option<Rc<dyn Browser>>,
    completed: bool,
    frame: Option<ImageHandle>,
    serial: Option<NodeId>,
    /// The load under way, kept until its first picture after it ended.
    load: Option<Reply>,
    awaiting_frame: bool,
    /// Scripts run and not answered yet: their callbacks and promises.
    scripts: HashMap<u64, (Value, Reply)>,
    next_script: u64,
    /// Why there is no browser, said once.
    refused: bool,
    /// The browser says where it is: the URL written is news, not
    /// somewhere to go.
    reporting: bool,
}

impl fmt::Debug for WebEngineView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebEngineView")
            .field("url", &self.url)
            .field("title", &self.title)
            .field("browser", &self.run.browser.is_some())
            .finish_non_exhaustive()
    }
}

impl Default for WebEngineView {
    fn default() -> Self {
        WebEngineView {
            item: Item::default(),
            url: Url::default(),
            title: String::new(),
            icon: Url::default(),
            loading: false,
            load_progress: 0,
            can_go_back: false,
            can_go_forward: false,
            zoom_factor: 1.0,
            profile: None,
            settings: WebEngineSettings::default(),
            user_scripts: None,
            audio_muted: false,
            recently_audible: false,
            background_color: Color::from_rgba8(255, 255, 255, 255),
            is_full_screen: false,
            active_focus_on_press: true,
            dev_tools_view: None,
            inspected_view: None,
            run: Run::default(),
        }
    }
}

impl Drop for WebEngineView {
    fn drop(&mut self) {
        if let Some(b) = self.run.browser.take() {
            b.close();
        }
        keep_frame(&mut self.run, None);
    }
}

impl WebEngineView {
    fn take_url(cx: &Cx, url: Url) -> Option<Url> {
        let resolved = if url.0.as_str().is_empty() {
            url
        } else {
            Url(Atom::from(
                cx.engine.resolve_url(cx.this, url.0.as_str()).as_str(),
            ))
        };
        let (completed, reporting) = cx
            .with(|v: &Self| (v.run.completed, v.run.reporting))
            .unwrap_or((false, false));
        if completed && !reporting && !resolved.0.as_str().is_empty() {
            let e = cx.engine;
            let me = cx.this;
            let target = resolved.0.as_str().to_owned();
            // After the write, so the browser reads the new URL.
            e.call_at(e.now(), move |e| navigate(e, me, &target));
        }
        Some(resolved)
    }

    fn take_zoom(cx: &Cx, factor: f64) -> Option<f64> {
        if !(ZOOM_MIN..=ZOOM_MAX).contains(&factor) {
            return None;
        }
        if let Some(b) = browser(cx.engine, cx.this) {
            b.set_zoom(factor);
        }
        Some(factor)
    }
}

#[qml_impl]
impl WebEngineView {
    #[qml(signal, name = "loadingChanged")]
    fn loading_changed(load_request: Option<ObjectId>);
    #[qml(signal, name = "javaScriptConsoleMessage")]
    fn java_script_console_message(level: i32, message: Atom, line_number: i32, source_id: Atom);
    #[qml(signal, name = "newWindowRequested")]
    fn new_window_requested(request: Option<ObjectId>);
    #[qml(signal, name = "windowCloseRequested")]
    fn window_close_requested();
    #[qml(signal, name = "renderProcessTerminated")]
    fn render_process_terminated(termination_status: i32, exit_code: i32);

    #[qml(invokable, name = "goBack")]
    fn go_back(cx: &Cx) {
        if let Some(b) = browser(cx.engine, cx.this) {
            b.go_back();
        }
    }

    #[qml(invokable, name = "goForward")]
    fn go_forward(cx: &Cx) {
        if let Some(b) = browser(cx.engine, cx.this) {
            b.go_forward();
        }
    }

    #[qml(invokable)]
    fn reload(cx: &Cx) {
        if let Some(b) = browser(cx.engine, cx.this) {
            b.reload();
        }
    }

    #[qml(invokable)]
    fn stop(cx: &Cx) {
        if let Some(b) = browser(cx.engine, cx.this) {
            b.stop();
        }
    }

    /// Shows `html` as a page. Not built: `baseUrl` (ARCHITECTURE.md §7.9);
    /// the page is a `data:` URL, so nothing resolves against it.
    #[qml(invokable, name = "loadHtml")]
    fn load_html(cx: &Cx, html: Value, base_url: Value) {
        let _ = base_url;
        let html = html.to_js_string().to_string();
        let url = format!("data:text/html;charset=utf-8,{}", percent_encode(&html));
        navigate(cx.engine, cx.this, &url);
    }

    /// Runs `script` in the page: `(script)`, `(script, callback)`,
    /// `(script, worldId)` or `(script, worldId, callback)`. The callback
    /// is given what the script's last expression is, or `undefined`.
    #[qml(invokable, name = "runJavaScript")]
    fn run_java_script(cx: &Cx, script: Value, a: Value, b: Value) {
        let e = cx.engine;
        let (world, callback) = if e.is_callable(&a) {
            (0, a)
        } else {
            let world = if matches!(a, Value::Undefined | Value::Null) {
                0
            } else {
                a.to_number() as i32
            };
            (world, b)
        };
        let source = script.to_js_string().to_string();
        let Some(browser) = browser(e, cx.this) else {
            return;
        };
        let id = if e.is_callable(&callback) {
            let reply = Platform::of(e).expect_for("runJavaScript", Some(cx.this));
            e.with_mut::<Self, _>(cx.this, |v| {
                v.run.next_script += 1;
                let id = v.run.next_script;
                v.run.scripts.insert(id, (callback, reply));
                id
            })
            .unwrap_or(0)
        } else {
            0
        };
        browser.run_script(id, &source, world);
    }

    /// Zooms by Chromium's steps.
    #[qml(invokable, name = "triggerWebAction")]
    fn trigger_web_action(cx: &Cx, action: i32) {
        let Some(b) = browser(cx.engine, cx.this) else {
            return;
        };
        match action {
            0 => b.go_back(),
            1 => b.go_forward(),
            2 => b.stop(),
            3 => b.reload(),
            _ => {}
        }
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        view_completed(cx.engine, cx.this)
    }
}

fn view_completed(e: &Engine, me: ObjectId) -> Result<(), EngineError> {
    let scripts = e.create_native(
        WebEngineScriptCollection {
            view: Some(me),
            ..WebEngineScriptCollection::default()
        },
        Some(me),
    )?;
    e.set_internal(me, WebEngineView::USER_SCRIPTS, Some(scripts))?;
    if e.get(me, WebEngineView::PROFILE)?.is_none() {
        e.set_internal(me, WebEngineView::PROFILE, WebEngine::default_profile(e))?;
    }
    let scope = e.object_scope(me).unwrap_or_else(|| e.graph().root());
    let serial = e.graph().property(scope, Value::Int(0)).ok();
    // Read before the view is complete: a binding on `url` not yet
    // evaluated writes it now, and a write to a complete view navigates,
    // so the first page was asked for twice and the first load reported
    // as stopped by the second.
    let url = e.get(me, WebEngineView::URL)?;
    e.with_mut::<WebEngineView, _>(me, |v| {
        v.run.serial = serial;
        v.run.completed = true;
    })?;
    // Size, scale and whether it is seen go to the browser as they change.
    e.effect_in(me, move |e| {
        let w = e.get(me, Item::WIDTH).unwrap_or(0.0);
        let h = e.get(me, Item::HEIGHT).unwrap_or(0.0);
        let scale = ljos_controls::device_pixel_ratio(e, me);
        let seen = shown_tracked(e, me);
        if let Some(b) = e.untrack(|| browser(e, me)) {
            b.resize(w, h, scale);
            b.set_hidden(!seen);
        }
    })?;
    e.watch(me, Item::ACTIVE_FOCUS.index, move |e, v| {
        if let Some(b) = browser(e, me) {
            b.set_focus(v.to_boolean());
        }
    })?;
    e.watch(me, WebEngineView::AUDIO_MUTED.index, move |e, v| {
        if let Some(b) = browser(e, me) {
            b.set_muted(v.to_boolean());
        }
    })?;
    if !url.0.as_str().is_empty() {
        let target = url.0.as_str().to_owned();
        e.call_at(e.now(), move |e| navigate(e, me, &target));
    }
    Ok(())
}

fn browser(e: &Engine, me: ObjectId) -> Option<Rc<dyn Browser>> {
    e.with::<WebEngineView, _>(me, |v| v.run.browser.clone())
        .ok()
        .flatten()
}

/// The file a `file:` URL names, as the browser must be told it: an
/// application whose files live under a root of their own (a test's
/// sandbox) names them from that root, the browser from the machine's.
fn to_browser_url(e: &Engine, url: &str) -> String {
    let env = Platform::of(e).env();
    let Some(rest) = url.strip_prefix("file://") else {
        return url.to_owned();
    };
    let cut = rest.find(['?', '#']).unwrap_or(rest.len());
    let (path, tail) = rest.split_at(cut);
    match env.real(std::path::Path::new(path)) {
        Ok(real) if real.as_os_str() != path => format!("file://{}{tail}", real.display()),
        _ => url.to_owned(),
    }
}

/// The URL the browser names, as the application names it.
fn from_browser_url(e: &Engine, url: &str) -> String {
    let env = Platform::of(e).env();
    if let Some(rest) = url.strip_prefix("file://")
        && let Ok(root) = env.real(std::path::Path::new("/"))
        && root.as_os_str() != "/"
        && let Some(inside) = rest.strip_prefix(&*root.to_string_lossy())
        && inside.starts_with('/')
    {
        return format!("file://{inside}");
    }
    url.to_owned()
}

/// Goes to `url`, making the browser if there is none yet.
fn navigate(e: &Engine, me: ObjectId, url: &str) {
    if !e.is_alive(me) {
        return;
    }
    let target = to_browser_url(e, url);
    if let Some(b) = browser(e, me) {
        b.navigate(&target);
        return;
    }
    let start = start_of(e, me, &target);
    match backend::start(e, me, start) {
        Ok(b) => {
            let _ = e.with_mut::<WebEngineView, _>(me, |v| v.run.browser = Some(b));
        }
        Err(why) => refuse(e, me, url, &why),
    }
}

/// A view that cannot show pages fails every load, and says why once.
fn refuse(e: &Engine, me: ObjectId, url: &str, why: &str) {
    let first = e
        .with_mut::<WebEngineView, _>(me, |v| !std::mem::replace(&mut v.run.refused, true))
        .unwrap_or(false);
    if first {
        e.log(
            LogLevel::Warning,
            crate::CATEGORY,
            format!("WebEngineView cannot show {url}: {why}"),
        );
    }
    apply(
        e,
        me,
        Event::LoadStarted {
            url: url.to_owned(),
        },
    );
    apply(
        e,
        me,
        Event::LoadEnded {
            url: url.to_owned(),
            status: LoadStatus::Failed,
            code: -2,
            error: "net::ERR_FAILED".to_owned(),
            domain: 1,
        },
    );
}

fn start_of(e: &Engine, me: ObjectId, url: &str) -> backend::Start {
    let w = e.untrack(|| e.get(me, Item::WIDTH)).unwrap_or(0.0);
    let h = e.untrack(|| e.get(me, Item::HEIGHT)).unwrap_or(0.0);
    let profile = e
        .get(me, WebEngineView::PROFILE)
        .ok()
        .flatten()
        .map(|p| WebEngineProfile::spec(e, p))
        .unwrap_or_default();
    let scripts = e
        .get(me, WebEngineView::USER_SCRIPTS)
        .ok()
        .flatten()
        .map(|c| WebEngineScriptCollection::scripts_of(e, c))
        .unwrap_or_default();
    let (zoom, muted, background, javascript) = e
        .with::<WebEngineView, _>(me, |v| {
            let c = v.background_color.to_rgba8();
            (
                v.zoom_factor,
                v.audio_muted,
                c,
                v.settings.javascript_enabled,
            )
        })
        .unwrap_or((1.0, false, [255; 4], true));
    backend::Start {
        url: url.to_owned(),
        width: w,
        height: h,
        scale: e.untrack(|| ljos_controls::device_pixel_ratio(e, me)),
        zoom,
        background,
        profile,
        scripts,
        javascript,
        muted,
        hidden: !e.untrack(|| shown_tracked(e, me)),
    }
}

/// The view's scripts changed: the browser runs the new ones from its
/// next page on.
pub(crate) fn scripts_changed(e: &Engine, me: ObjectId) {
    let Some(b) = browser(e, me) else {
        return;
    };
    let scripts = e
        .get(me, WebEngineView::USER_SCRIPTS)
        .ok()
        .flatten()
        .map(|c| WebEngineScriptCollection::scripts_of(e, c))
        .unwrap_or_default();
    b.set_scripts(&scripts);
}

/// A new window a page asked for opens in `view`.
pub(crate) fn open_request(e: &Engine, view: ObjectId, url: &str) {
    if e.with::<WebEngineView, _>(view, |_| ()).is_err() {
        return;
    }
    let _ = e.set(view, WebEngineView::URL, Url(Atom::from(url)));
}

/// What `event` says, applied to `me` on the Engine thread.
pub(crate) fn apply(e: &Engine, me: ObjectId, event: Event) {
    if !e.is_alive(me) {
        return;
    }
    let event = match event {
        Event::LoadStarted { url } => Event::LoadStarted {
            url: from_browser_url(e, &url),
        },
        Event::LoadEnded {
            url,
            status,
            code,
            error,
            domain,
        } => Event::LoadEnded {
            url: from_browser_url(e, &url),
            status,
            code,
            error,
            domain,
        },
        Event::Url(url) => Event::Url(from_browser_url(e, &url)),
        Event::NewWindow {
            url,
            destination,
            geometry,
            user_initiated,
        } => Event::NewWindow {
            url: from_browser_url(e, &url),
            destination,
            geometry,
            user_initiated,
        },
        other => other,
    };
    match event {
        Event::LoadStarted { url } => {
            let reply = Platform::of(e).expect_for(format!("page {url}"), Some(me));
            let _ = e.with_mut::<WebEngineView, _>(me, |v| {
                v.run.load = Some(reply);
                v.run.awaiting_frame = false;
            });
            let _ = e.set_internal(me, WebEngineView::LOADING, true);
            let _ = e.set_internal(me, WebEngineView::LOAD_PROGRESS, 0);
            emit_loading(e, me, &url, LoadStatus::Started, 0, String::new(), 0);
        }
        Event::LoadEnded {
            url,
            status,
            code,
            error,
            domain,
        } => {
            if status == LoadStatus::Succeeded {
                let _ = e.set_internal(me, WebEngineView::LOAD_PROGRESS, 100);
            }
            let _ = e.set_internal(me, WebEngineView::LOADING, false);
            // The load is over for a test once its picture is there; a
            // view that cannot be seen draws none.
            let seen = e.untrack(|| shown_tracked(e, me));
            let _ = e.with_mut::<WebEngineView, _>(me, |v| {
                if seen && v.run.browser.is_some() {
                    v.run.awaiting_frame = true;
                } else {
                    v.run.load = None;
                }
            });
            emit_loading(e, me, &url, status, code, error, domain);
        }
        Event::Url(url) => {
            let _ = e.with_mut::<WebEngineView, _>(me, |v| v.run.reporting = true);
            let _ = e.set_internal(me, WebEngineView::URL, Url(Atom::from(url.as_str())));
            let _ = e.with_mut::<WebEngineView, _>(me, |v| v.run.reporting = false);
        }
        Event::Title(title) => {
            let _ = e.set_internal(me, WebEngineView::TITLE, title);
        }
        Event::Progress(p) => {
            let _ = e.set_internal(
                me,
                WebEngineView::LOAD_PROGRESS,
                (p * 100.0).round().clamp(0.0, 100.0) as i32,
            );
        }
        Event::Navigation { back, forward } => {
            let _ = e.set_internal(me, WebEngineView::CAN_GO_BACK, back);
            let _ = e.set_internal(me, WebEngineView::CAN_GO_FORWARD, forward);
        }
        Event::Frame(image) => {
            let Ok(serial) = e.with_mut::<WebEngineView, _>(me, |v| {
                keep_frame(&mut v.run, Some(image));
                if v.run.awaiting_frame {
                    v.run.awaiting_frame = false;
                    v.run.load = None;
                }
                v.run.serial
            }) else {
                return;
            };
            bump(e, serial);
        }
        Event::Console {
            level,
            message,
            line,
            source,
        } => {
            let _ = e.emit_by_name(
                me,
                "javaScriptConsoleMessage",
                &[
                    Value::Int(level),
                    Value::String(Atom::from(message.as_str())),
                    Value::Int(line),
                    Value::String(Atom::from(source.as_str())),
                ],
            );
        }
        Event::ScriptResult { id, value } => {
            let Some((callback, reply)) = e
                .with_mut::<WebEngineView, _>(me, |v| v.run.scripts.remove(&id))
                .ok()
                .flatten()
            else {
                return;
            };
            let value = value.as_ref().map_or(Value::Undefined, from_json);
            e.call_js(&callback, &[value]);
            drop(reply);
        }
        Event::NewWindow {
            url,
            destination,
            geometry,
            user_initiated,
        } => new_window(e, me, &url, destination, geometry, user_initiated),
        Event::CloseRequested => {
            let _ = e.emit_by_name(me, "windowCloseRequested", &[]);
        }
        Event::Crashed { status, code } => {
            let _ = e.emit_by_name(
                me,
                "renderProcessTerminated",
                &[Value::Int(status), Value::Int(code)],
            );
        }
    }
}

fn emit_loading(
    e: &Engine,
    me: ObjectId,
    url: &str,
    status: LoadStatus,
    code: i32,
    error: String,
    domain: i32,
) {
    let info = WebEngineLoadingInfo {
        url: Url(Atom::from(url)),
        is_error_page: false,
        status: status as i32,
        error_string: error,
        error_domain: domain,
        error_code: code,
        http_status_code: if domain == 7 { code } else { 0 },
    };
    let Ok(info) = e.create_native(info, Some(me)) else {
        return;
    };
    let _ = e.emit_by_name(me, "loadingChanged", &[Value::Object(info)]);
    e.destroy_at_turn_end(info);
}

fn new_window(
    e: &Engine,
    me: ObjectId,
    url: &str,
    destination: i32,
    (x, y, w, h): (f64, f64, f64, f64),
    user_initiated: bool,
) {
    let request = WebEngineNewWindowRequest {
        destination,
        requested_url: Url(Atom::from(url)),
        requested_geometry: ljos_engine::Rect {
            x,
            y,
            width: w,
            height: h,
        },
        user_initiated,
        handled: false,
    };
    let Ok(request) = e.create_native(request, Some(me)) else {
        return;
    };
    let _ = e.emit_by_name(me, "newWindowRequested", &[Value::Object(request)]);
    e.destroy_at_turn_end(request);
}

/// A value a page's script gave, as a QML script is handed it.
fn from_json(v: &serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => Value::Real(n.as_f64().unwrap_or(f64::NAN)),
        serde_json::Value::String(s) => Value::String(Atom::from(s.as_str())),
        serde_json::Value::Array(items) => {
            Value::List(Arc::new(items.iter().map(from_json).collect()))
        }
        serde_json::Value::Object(fields) => Value::Map(Arc::new(
            fields
                .iter()
                .map(|(k, v)| (Atom::from(k.as_str()), from_json(v)))
                .collect(),
        )),
    }
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn bump(e: &Engine, serial: Option<NodeId>) {
    if let Some(n) = serial {
        let next = e.graph().get_untracked(n).to_number() as i32 + 1;
        e.graph().set(n, Value::Int(next));
    }
}

/// Whether `me` can be seen: of some size, and it and its ancestors
/// visible and not transparent.
fn shown_tracked(e: &Engine, me: ObjectId) -> bool {
    let w = e.get(me, Item::WIDTH).unwrap_or(0.0);
    let h = e.get(me, Item::HEIGHT).unwrap_or(0.0);
    if w <= 0.0 || h <= 0.0 {
        return false;
    }
    let mut at = Some(me);
    while let Some(o) = at {
        if ljos_quick::is_item(e, o)
            && (!e.get(o, Item::VISIBLE).unwrap_or(true)
                || e.get(o, Item::OPACITY).unwrap_or(1.0) <= 0.0)
        {
            return false;
        }
        at = e.visual_parent(o);
    }
    true
}

/// What a view paints: its background, then the page over all of it.
#[derive(Debug)]
struct PagePaint {
    frame: Option<ImageHandle>,
    background: Rgba,
}

impl ItemPaint for PagePaint {
    fn paint(&self, size: kurbo::Size, _text: &mut TextEngine, out: &mut DisplayListBuilder) {
        let item = kurbo::Rect::new(0.0, 0.0, size.width, size.height);
        if item.width() <= 0.0 || item.height() <= 0.0 {
            return;
        }
        if self.background.a > 0.0 {
            out.fill_rect(item, self.background);
        }
        let Some(frame) = &self.frame else {
            return;
        };
        // The page was drawn for the size the view had then; until one of
        // the new size comes it is shown at its own size, not stretched.
        let px = frame.rect();
        let scale = if item.width() > 0.0 {
            (px.width() / item.width()).max(f64::MIN_POSITIVE)
        } else {
            1.0
        };
        let shown = kurbo::Rect::new(0.0, 0.0, px.width() / scale, px.height() / scale);
        let dst = shown.intersect(item);
        if dst.width() <= 0.0 || dst.height() <= 0.0 {
            return;
        }
        let src = kurbo::Rect::new(0.0, 0.0, dst.width() * scale, dst.height() * scale);
        out.push(DrawOp::Image {
            image: frame.clone(),
            src,
            dst,
            tile: None,
            mirror: [false, false],
            smooth: true,
            opacity: 1.0,
            tint: None,
        });
    }
}

fn read_paint(e: &Engine, me: ObjectId) -> Rc<dyn ItemPaint> {
    let serial = e
        .with::<WebEngineView, _>(me, |v| v.run.serial)
        .ok()
        .flatten();
    if let Some(n) = serial {
        let _ = e.graph().get(n);
    }
    let bg = e
        .get(me, WebEngineView::BACKGROUND_COLOR)
        .unwrap_or_default();
    let [r, g, b, a] = bg.to_rgba8();
    let frame = e
        .with::<WebEngineView, _>(me, |v| v.run.frame.clone())
        .ok()
        .flatten();
    Rc::new(PagePaint {
        frame,
        background: Rgba::from_straight_u8(r, g, b, a),
    })
}

pub(crate) fn register_paint(engine: &Engine) {
    ljos_quick::register_paint_item(
        engine,
        ljos_engine::MetaRef::Static(<WebEngineView as QmlType>::static_meta()),
        read_paint,
    );
    crate::input::register(engine);
}

/// What the views hold of their pages' pictures, for app memory.
pub(crate) fn frames_held() -> (usize, usize) {
    (
        SHOWN_BYTES.load(Ordering::Relaxed),
        SHOWN_FRAMES.load(Ordering::Relaxed),
    )
}

static SHOWN_BYTES: AtomicUsize = AtomicUsize::new(0);
static SHOWN_FRAMES: AtomicUsize = AtomicUsize::new(0);

/// Puts `frame` in `run`, keeping [`frames_held`] true.
fn keep_frame(run: &mut Run, frame: Option<ImageHandle>) {
    let held = |f: &Option<ImageHandle>| f.as_ref().map_or((0, 0), |i| (i.byte_len(), 1));
    let (was_b, was_n) = held(&run.frame);
    run.frame = frame;
    let (b, n) = held(&run.frame);
    SHOWN_BYTES.fetch_add(b, Ordering::Relaxed);
    SHOWN_BYTES.fetch_sub(was_b, Ordering::Relaxed);
    SHOWN_FRAMES.fetch_add(n, Ordering::Relaxed);
    SHOWN_FRAMES.fetch_sub(was_n, Ordering::Relaxed);
}

/// The browser of `me`, for the input hooks.
pub(crate) fn browser_of(e: &Engine, me: ObjectId) -> Option<Rc<dyn Browser>> {
    browser(e, me)
}
