//! The types around a view: the `WebEngine` singleton, profiles,
//! settings, scripts and what a load or a new window says.

use std::cell::RefCell;
use std::sync::Arc;

use ljos_engine::{
    Atom, Cx, Engine, EngineError, ObjectId, QmlObject, Rect, Url, Value, qml_impl, qml_singleton,
};

/// The user agent a profile says it is, unless it is given another:
/// Chromium's own, with Ljós's name where Qt puts its own.
pub(crate) const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Ljos Chrome/140.0.0.0 Safari/537.36";

fn script_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "InjectionPoint",
            vec![
                ("Deferred", 0),
                ("DocumentReady", 1),
                ("DocumentCreation", 2),
            ],
        ),
        (
            "ScriptWorldId",
            vec![("MainWorld", 0), ("ApplicationWorld", 1), ("UserWorld", 2)],
        ),
    ]
}

/// When a script is put in a page.
pub const DEFERRED: i32 = 0;
/// Once the document is parsed (`DOMContentLoaded`).
pub const DOCUMENT_READY: i32 = 1;
/// Before the page's own scripts run.
pub const DOCUMENT_CREATION: i32 = 2;

/// A script that runs in every page a view loads (`WebEngineScript`).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtWebEngine", name = "WebEngineScript", enums = script_enums)]
pub struct WebEngineScript {
    #[qml(notify)]
    pub name: String,
    #[qml(notify)]
    pub source_url: Url,
    #[qml(notify)]
    pub source_code: String,
    #[qml(notify, enumeration = "InjectionPoint")]
    pub injection_point: i32,
    #[qml(notify, enumeration = "ScriptWorldId")]
    pub world_id: i32,
    #[qml(notify)]
    pub runs_on_sub_frames: bool,
}

/// What a backend is given of a script.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Script {
    pub name: String,
    pub source: String,
    pub injection_point: i32,
    pub world: i32,
    pub sub_frames: bool,
}

impl Script {
    /// The script an object or a value written in QML describes: a
    /// `WebEngineScript`, or a plain object with its property names.
    pub(crate) fn of(e: &Engine, v: &Value) -> Option<Script> {
        if let Value::Object(o) = v
            && let Ok(s) = e.with::<WebEngineScript, _>(*o, |s| Script {
                name: s.name.clone(),
                source: s.source_code.clone(),
                injection_point: s.injection_point,
                world: s.world_id,
                sub_frames: s.runs_on_sub_frames,
            })
        {
            return Some(s);
        }
        let fields: Vec<(String, Value)> = match v {
            Value::Map(m) => m.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(),
            Value::Js(_) => e.js_object_entries(v),
            _ => return None,
        };
        let mut s = Script::default();
        for (k, v) in fields {
            match k.as_str() {
                "name" => s.name = v.to_js_string().to_string(),
                "sourceCode" => s.source = v.to_js_string().to_string(),
                "injectionPoint" => s.injection_point = v.to_number() as i32,
                "worldId" => s.world = v.to_number() as i32,
                "runsOnSubFrames" => s.sub_frames = v.to_boolean(),
                _ => {}
            }
        }
        Some(s)
    }

    /// The script as a script reads it back: a plain object with the
    /// names of a `WebEngineScript`'s properties.
    pub(crate) fn to_value(&self) -> Value {
        Value::Map(Arc::new(vec![
            (
                Atom::from("name"),
                Value::String(Atom::from(self.name.as_str())),
            ),
            (
                Atom::from("sourceCode"),
                Value::String(Atom::from(self.source.as_str())),
            ),
            (
                Atom::from("injectionPoint"),
                Value::Int(self.injection_point),
            ),
            (Atom::from("worldId"), Value::Int(self.world)),
            (Atom::from("runsOnSubFrames"), Value::Bool(self.sub_frames)),
        ]))
    }
}

/// The scripts of a view (`WebEngineScriptCollection`): its
/// `userScripts`.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtWebEngine",
    name = "WebEngineScriptCollection",
    uncreatable
)]
pub struct WebEngineScriptCollection {
    /// Read and written as a list of scripts; kept as [`Script`]s.
    #[qml(notify, write = Self::take_collection)]
    pub collection: Value,
    #[qml(skip)]
    pub(crate) scripts: RefCell<Vec<Script>>,
    /// The view whose scripts these are, told of every change.
    #[qml(skip)]
    pub(crate) view: Option<ObjectId>,
}

impl WebEngineScriptCollection {
    fn take_collection(cx: &Cx, v: Value) -> Option<Value> {
        let items: Vec<Value> = match &v {
            Value::List(l) => l.iter().cloned().collect(),
            Value::Js(_) => {
                let n = cx
                    .engine
                    .js_member(&v, "length")
                    .map_or(0, |n| n.to_number() as usize);
                (0..n)
                    .filter_map(|i| cx.engine.js_member(&v, &i.to_string()))
                    .collect()
            }
            Value::Undefined | Value::Null => Vec::new(),
            other => vec![other.clone()],
        };
        let scripts: Vec<Script> = items
            .iter()
            .filter_map(|i| Script::of(cx.engine, i))
            .collect();
        let value = list_value(&scripts);
        let view = cx
            .with(|c: &Self| {
                *c.scripts.borrow_mut() = scripts;
                c.view
            })
            .ok()
            .flatten();
        if let Some(view) = view {
            crate::view::scripts_changed(cx.engine, view);
        }
        Some(value)
    }

    fn change(e: &Engine, me: ObjectId, f: impl FnOnce(&mut Vec<Script>)) {
        let Ok(scripts) = e.with::<Self, _>(me, |c| {
            f(&mut c.scripts.borrow_mut());
            c.scripts.borrow().clone()
        }) else {
            return;
        };
        let _ = e.set_internal(me, Self::COLLECTION, list_value(&scripts));
        if let Some(view) = e.with::<Self, _>(me, |c| c.view).ok().flatten() {
            crate::view::scripts_changed(e, view);
        }
    }

    /// The scripts of `me`, for a backend.
    pub(crate) fn scripts_of(e: &Engine, me: ObjectId) -> Vec<Script> {
        e.with::<Self, _>(me, |c| c.scripts.borrow().clone())
            .unwrap_or_default()
    }
}

fn list_value(scripts: &[Script]) -> Value {
    Value::List(Arc::new(scripts.iter().map(Script::to_value).collect()))
}

#[qml_impl]
impl WebEngineScriptCollection {
    #[qml(invokable)]
    fn insert(cx: &Cx, script: Value) {
        if let Some(s) = Script::of(cx.engine, &script) {
            Self::change(cx.engine, cx.this, |all| all.push(s));
        } else if let Value::List(l) = script {
            let more: Vec<Script> = l.iter().filter_map(|i| Script::of(cx.engine, i)).collect();
            Self::change(cx.engine, cx.this, |all| all.extend(more));
        }
    }

    #[qml(invokable)]
    fn remove(cx: &Cx, script: Value) -> bool {
        let Some(s) = Script::of(cx.engine, &script) else {
            return false;
        };
        let mut found = false;
        Self::change(cx.engine, cx.this, |all| {
            if let Some(i) = all.iter().position(|x| *x == s) {
                all.remove(i);
                found = true;
            }
        });
        found
    }

    #[qml(invokable)]
    fn contains(cx: &Cx, script: Value) -> bool {
        Script::of(cx.engine, &script).is_some_and(|s| {
            cx.engine
                .with::<Self, _>(cx.this, |c| c.scripts.borrow().contains(&s))
                .unwrap_or(false)
        })
    }

    #[qml(invokable)]
    fn find(cx: &Cx, name: Value) -> Value {
        let name = name.to_js_string().to_string();
        let found: Vec<Script> = cx
            .engine
            .with::<Self, _>(cx.this, |c| {
                c.scripts
                    .borrow()
                    .iter()
                    .filter(|s| s.name == name)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        list_value(&found)
    }

    #[qml(invokable)]
    fn clear(cx: &Cx) {
        Self::change(cx.engine, cx.this, Vec::clear);
    }
}

/// What a view lets its pages do (`WebEngineSettings`), its `settings`.
#[derive(Clone, Debug, PartialEq, QmlObject)]
#[qml(module = "QtWebEngine", name = "WebEngineSettings", uncreatable)]
pub struct WebEngineSettings {
    #[qml(notify)]
    pub javascript_enabled: bool,
    #[qml(notify)]
    pub javascript_can_open_windows: bool,
    #[qml(notify)]
    pub javascript_can_access_clipboard: bool,
    #[qml(notify)]
    pub javascript_can_paste: bool,
    #[qml(notify)]
    pub auto_load_images: bool,
    #[qml(notify)]
    pub local_storage_enabled: bool,
    #[qml(notify)]
    pub local_content_can_access_remote_urls: bool,
    #[qml(notify)]
    pub local_content_can_access_file_urls: bool,
    #[qml(notify)]
    pub error_page_enabled: bool,
    #[qml(notify)]
    pub focus_on_navigation_enabled: bool,
    #[qml(notify)]
    pub plugins_enabled: bool,
    #[qml(notify)]
    pub webgl_enabled: bool,
    #[qml(notify)]
    pub accelerated_2d_canvas_enabled: bool,
    #[qml(notify)]
    pub scroll_animator_enabled: bool,
    #[qml(notify)]
    pub show_scroll_bars: bool,
    #[qml(notify)]
    pub playback_requires_user_gesture: bool,
    #[qml(notify)]
    pub spatial_navigation_enabled: bool,
    #[qml(notify)]
    pub hyperlink_auditing_enabled: bool,
    #[qml(notify)]
    pub fullscreen_support_enabled: bool,
    #[qml(notify)]
    pub screen_capture_enabled: bool,
    #[qml(notify)]
    pub allow_running_insecure_content: bool,
    #[qml(notify)]
    pub dns_prefetch_enabled: bool,
    #[qml(notify)]
    pub pdf_viewer_enabled: bool,
    #[qml(notify)]
    pub print_element_backgrounds: bool,
    #[qml(notify)]
    pub default_text_encoding: String,
}

impl Default for WebEngineSettings {
    /// What Qt's are on a new view (doc.qt.io, qml-qtwebengine-webenginesettings).
    fn default() -> Self {
        WebEngineSettings {
            javascript_enabled: true,
            javascript_can_open_windows: true,
            javascript_can_access_clipboard: false,
            javascript_can_paste: false,
            auto_load_images: true,
            local_storage_enabled: true,
            local_content_can_access_remote_urls: false,
            local_content_can_access_file_urls: true,
            error_page_enabled: true,
            focus_on_navigation_enabled: false,
            plugins_enabled: false,
            webgl_enabled: true,
            accelerated_2d_canvas_enabled: false,
            scroll_animator_enabled: false,
            show_scroll_bars: true,
            playback_requires_user_gesture: true,
            spatial_navigation_enabled: false,
            hyperlink_auditing_enabled: false,
            fullscreen_support_enabled: false,
            screen_capture_enabled: false,
            allow_running_insecure_content: false,
            dns_prefetch_enabled: false,
            pdf_viewer_enabled: true,
            print_element_backgrounds: true,
            default_text_encoding: String::new(),
        }
    }
}

fn profile_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "HttpCacheType",
            vec![("MemoryHttpCache", 0), ("DiskHttpCache", 1), ("NoCache", 2)],
        ),
        (
            "PersistentCookiesPolicy",
            vec![
                ("NoPersistentCookies", 0),
                ("AllowPersistentCookies", 1),
                ("ForcePersistentCookies", 2),
            ],
        ),
        (
            "PersistentPermissionsPolicy",
            vec![
                ("AskEveryTime", 0),
                ("StoreInMemory", 1),
                ("StoreOnDisk", 2),
            ],
        ),
    ]
}

/// `WebEngineProfile.PersistentPermissionsPolicy`: how long a profile
/// keeps what a view answered a page.
pub mod permissions_policy {
    /// Nothing is kept: the page asks the view every time.
    pub const ASK_EVERY_TIME: i32 = 0;
    /// Kept while the application runs.
    pub const STORE_IN_MEMORY: i32 = 1;
    /// Kept on disk, for the next run too.
    pub const STORE_ON_DISK: i32 = 2;
}

/// Where a view's pages keep what they keep (`WebEngineProfile`).
#[derive(Debug, QmlObject)]
#[qml(module = "QtWebEngine", name = "WebEngineProfile", enums = profile_enums)]
pub struct WebEngineProfile {
    #[qml(notify)]
    pub storage_name: String,
    #[qml(notify)]
    pub off_the_record: bool,
    #[qml(notify)]
    pub persistent_storage_path: String,
    #[qml(notify)]
    pub cache_path: String,
    #[qml(notify)]
    pub http_user_agent: String,
    #[qml(notify)]
    pub http_accept_language: String,
    #[qml(notify, enumeration = "HttpCacheType")]
    pub http_cache_type: i32,
    #[qml(notify, enumeration = "PersistentCookiesPolicy")]
    pub persistent_cookies_policy: i32,
    #[qml(notify)]
    pub http_cache_maximum_size: i32,
    #[qml(notify, enumeration = "PersistentPermissionsPolicy")]
    pub persistent_permissions_policy: i32,
    #[qml(notify)]
    pub download_path: String,
    #[qml(notify)]
    pub spell_check_enabled: bool,
}

impl Default for WebEngineProfile {
    /// A profile written in QML: off the record until it is given a name
    /// (Qt's are, doc.qt.io qml-qtwebengine-webengineprofile).
    fn default() -> Self {
        WebEngineProfile {
            storage_name: String::new(),
            off_the_record: true,
            persistent_storage_path: String::new(),
            cache_path: String::new(),
            http_user_agent: USER_AGENT.to_owned(),
            http_accept_language: String::new(),
            http_cache_type: 0,
            persistent_cookies_policy: 0,
            http_cache_maximum_size: 0,
            persistent_permissions_policy: permissions_policy::STORE_ON_DISK,
            download_path: String::new(),
            spell_check_enabled: false,
        }
    }
}

/// What a backend is given of a profile.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProfileSpec {
    /// Empty: kept in memory only.
    pub storage_name: String,
    pub off_the_record: bool,
    /// Where the profile keeps what it keeps; empty for the engine's own
    /// place, named after `storage_name`.
    pub storage_path: String,
    /// Where its HTTP cache goes; empty for beside the storage.
    pub cache_path: String,
    /// A [`permissions_policy`], as the profile can keep it: one off the
    /// record keeps nothing on disk.
    pub permissions_policy: i32,
    pub user_agent: String,
    pub accept_language: String,
}

impl Default for ProfileSpec {
    /// What the engine's default profile is: off the record.
    fn default() -> Self {
        ProfileSpec {
            storage_name: String::new(),
            off_the_record: true,
            storage_path: String::new(),
            cache_path: String::new(),
            permissions_policy: permissions_policy::STORE_IN_MEMORY,
            user_agent: String::new(),
            accept_language: String::new(),
        }
    }
}

impl WebEngineProfile {
    pub(crate) fn spec(e: &Engine, me: ObjectId) -> ProfileSpec {
        e.with::<Self, _>(me, |p| {
            let off_the_record = p.off_the_record || p.storage_name.is_empty();
            ProfileSpec {
                storage_name: p.storage_name.clone(),
                off_the_record,
                storage_path: p.persistent_storage_path.clone(),
                cache_path: p.cache_path.clone(),
                permissions_policy: if off_the_record {
                    p.persistent_permissions_policy
                        .min(permissions_policy::STORE_IN_MEMORY)
                } else {
                    p.persistent_permissions_policy
                },
                user_agent: p.http_user_agent.clone(),
                accept_language: p.http_accept_language.clone(),
            }
        })
        .unwrap_or_default()
    }
}

#[qml_impl]
impl WebEngineProfile {
    #[qml(invokable, name = "clearHttpCache")]
    fn clear_http_cache(cx: &Cx) {
        crate::backend::clear_http_cache(cx.engine, Self::spec(cx.engine, cx.this));
    }

    #[qml(signal, name = "clearHttpCacheCompleted")]
    fn clear_http_cache_completed();
}

/// The `WebEngine` singleton: the default profile and a maker of scripts.
#[derive(Debug, QmlObject)]
#[qml(module = "QtWebEngine", name = "WebEngine", uncreatable)]
#[qml_singleton]
pub struct WebEngine {
    #[qml(readonly, constant, object = "WebEngineProfile")]
    pub default_profile: Option<ObjectId>,
}

impl WebEngine {
    pub(crate) fn make(e: &Engine) -> WebEngine {
        let profile = e.create_native(WebEngineProfile::default(), None).ok();
        WebEngine {
            default_profile: profile,
        }
    }

    /// The engine's default profile, made when first asked for.
    pub(crate) fn default_profile(e: &Engine) -> Option<ObjectId> {
        let me = e.singleton::<WebEngine>().ok()?;
        e.with::<WebEngine, _>(me, |w| w.default_profile)
            .ok()
            .flatten()
    }
}

#[qml_impl]
impl WebEngine {
    /// A new script to give a view.
    #[qml(invokable)]
    fn script(cx: &Cx) -> Result<Value, EngineError> {
        let s = cx
            .engine
            .create_native(WebEngineScript::default(), Some(cx.this))?;
        Ok(Value::Object(s))
    }
}

fn loading_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
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
    ]
}

/// How a load went, as `loadingChanged` says (`WebEngineLoadingInfo`).
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtWebEngine",
    name = "WebEngineLoadingInfo",
    uncreatable,
    enums = loading_enums
)]
pub struct WebEngineLoadingInfo {
    #[qml(readonly, constant)]
    pub url: Url,
    #[qml(readonly, constant)]
    pub is_error_page: bool,
    #[qml(readonly, constant, enumeration = "LoadStatus")]
    pub status: i32,
    #[qml(readonly, constant)]
    pub error_string: String,
    #[qml(readonly, constant, enumeration = "ErrorDomain")]
    pub error_domain: i32,
    #[qml(readonly, constant)]
    pub error_code: i32,
    #[qml(readonly, constant)]
    pub http_status_code: i32,
}

fn window_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "DestinationType",
        vec![
            ("InNewWindow", 0),
            ("InNewTab", 1),
            ("InNewDialog", 2),
            ("InNewBackgroundTab", 3),
        ],
    )]
}

/// A page asking for a new window (`WebEngineNewWindowRequest`): it opens
/// in the view given to [`openIn`](Self::open_in), or not at all.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtWebEngine",
    name = "WebEngineNewWindowRequest",
    uncreatable,
    enums = window_enums
)]
pub struct WebEngineNewWindowRequest {
    #[qml(readonly, constant, enumeration = "DestinationType")]
    pub destination: i32,
    #[qml(readonly, constant)]
    pub requested_url: Url,
    #[qml(readonly, constant)]
    pub requested_geometry: Rect,
    #[qml(readonly, constant)]
    pub user_initiated: bool,
    #[qml(skip)]
    pub(crate) handled: bool,
}

#[qml_impl]
impl WebEngineNewWindowRequest {
    /// Opens the new window's page in `view`.
    #[qml(invokable, name = "openIn")]
    fn open_in(cx: &Cx, view: Value) -> Result<(), EngineError> {
        let Value::Object(view) = view else {
            return Ok(());
        };
        let url = cx.get(Self::REQUESTED_URL)?;
        cx.with_mut(|r: &mut Self| r.handled = true)?;
        crate::view::open_request(cx.engine, view, url.0.as_str());
        Ok(())
    }
}

fn navigation_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "NavigationType",
            vec![
                ("LinkClickedNavigation", 0),
                ("TypedNavigation", 1),
                ("FormSubmittedNavigation", 2),
                ("BackForwardNavigation", 3),
                ("ReloadNavigation", 4),
                ("OtherNavigation", 5),
                ("RedirectNavigation", 6),
            ],
        ),
        (
            "NavigationRequestAction",
            vec![("AcceptRequest", 0), ("IgnoreRequest", 255)],
        ),
    ]
}

/// A page about to go somewhere (`WebEngineNavigationRequest`): it goes
/// unless the handler of `navigationRequested` rejects it there and then.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtWebEngine",
    name = "WebEngineNavigationRequest",
    uncreatable,
    enums = navigation_enums
)]
pub struct WebEngineNavigationRequest {
    #[qml(readonly, constant)]
    pub url: Url,
    #[qml(readonly, constant)]
    pub is_main_frame: bool,
    #[qml(readonly, constant)]
    pub has_form_data: bool,
    #[qml(readonly, constant, enumeration = "NavigationType")]
    pub navigation_type: i32,
    #[qml(notify, readonly, enumeration = "NavigationRequestAction")]
    pub action: i32,
}

/// What [`WebEngineNavigationRequest::action`] is once rejected.
pub(crate) const IGNORE_REQUEST: i32 = 255;

#[qml_impl]
impl WebEngineNavigationRequest {
    #[qml(invokable)]
    fn accept(cx: &Cx) -> Result<(), EngineError> {
        cx.engine.set_internal(cx.this, Self::ACTION, 0)
    }

    #[qml(invokable)]
    fn reject(cx: &Cx) -> Result<(), EngineError> {
        cx.engine
            .set_internal(cx.this, Self::ACTION, IGNORE_REQUEST)
    }
}

fn permission_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "PermissionType",
            vec![
                ("Unsupported", 0),
                ("MediaAudioCapture", 1),
                ("MediaVideoCapture", 2),
                ("MediaAudioVideoCapture", 3),
                ("DesktopVideoCapture", 4),
                ("DesktopAudioVideoCapture", 5),
                ("MouseLock", 6),
                ("Notifications", 7),
                ("Geolocation", 8),
                ("ClipboardReadWrite", 9),
                ("LocalFontsAccess", 10),
            ],
        ),
        (
            "State",
            vec![("Invalid", 0), ("Ask", 1), ("Granted", 2), ("Denied", 3)],
        ),
    ]
}

/// `WebEnginePermission.State`.
pub(crate) mod permission_state {
    pub const INVALID: i32 = 0;
    pub const ASK: i32 = 1;
    pub const GRANTED: i32 = 2;
    pub const DENIED: i32 = 3;
}

/// What a page asks to be allowed (`WebEnginePermission`): answered with
/// [`grant`](Self::grant) or [`deny`](Self::deny), then or later. The page
/// waits until it is.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtWebEngine",
    name = "WebEnginePermission",
    uncreatable,
    enums = permission_enums
)]
pub struct WebEnginePermission {
    #[qml(readonly, constant)]
    pub origin: Url,
    #[qml(readonly, constant, enumeration = "PermissionType")]
    pub permission_type: i32,
    #[qml(notify, readonly, enumeration = "State")]
    pub state: i32,
    #[qml(notify, readonly)]
    pub is_valid: bool,
    /// The view the page is in, which hands the answer on.
    #[qml(skip)]
    pub(crate) view: Option<ObjectId>,
    /// What the browser knows the question by.
    #[qml(skip)]
    pub(crate) id: u64,
}

#[qml_impl]
impl WebEnginePermission {
    #[qml(invokable)]
    fn grant(cx: &Cx) {
        crate::view::answer_permission(cx.engine, cx.this, true);
    }

    #[qml(invokable)]
    fn deny(cx: &Cx) {
        crate::view::answer_permission(cx.engine, cx.this, false);
    }

    /// Forgets the answer kept for the page's origin: it is asked again.
    #[qml(invokable)]
    fn reset(cx: &Cx) {
        crate::view::reset_permission(cx.engine, cx.this);
    }
}

/// A page asking to be shown a screen or a window
/// (`WebEngineDesktopMediaRequest`): [`selectScreen`](Self::select_screen)
/// or [`selectWindow`](Self::select_window) chooses one of the models'
/// rows, [`cancel`](Self::cancel) refuses.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtWebEngine",
    name = "WebEngineDesktopMediaRequest",
    uncreatable
)]
pub struct WebEngineDesktopMediaRequest {
    #[qml(readonly, constant, object = "ListModel")]
    pub screens_model: Option<ObjectId>,
    #[qml(readonly, constant, object = "ListModel")]
    pub windows_model: Option<ObjectId>,
    #[qml(skip)]
    pub(crate) view: Option<ObjectId>,
    #[qml(skip)]
    pub(crate) id: u64,
    /// Whether the page also asked for the screen's sound.
    #[qml(skip)]
    pub(crate) audio: bool,
    #[qml(skip)]
    pub(crate) answered: bool,
}

#[qml_impl]
impl WebEngineDesktopMediaRequest {
    #[qml(invokable, name = "selectScreen")]
    fn select_screen(cx: &Cx, index: Value) {
        let _ = index;
        crate::view::choose_desktop_media(cx.engine, cx.this, Some(crate::view::Shown::Screen));
    }

    #[qml(invokable, name = "selectWindow")]
    fn select_window(cx: &Cx, index: Value) {
        let _ = index;
        crate::view::choose_desktop_media(cx.engine, cx.this, Some(crate::view::Shown::Window));
    }

    #[qml(invokable)]
    fn cancel(cx: &Cx) {
        crate::view::choose_desktop_media(cx.engine, cx.this, None);
    }
}
