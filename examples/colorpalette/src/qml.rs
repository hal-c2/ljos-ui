//! The QML face of the core in [`crate::rest`]: `RestService`,
//! `PaginatedResource` and `BasicLogin` in the `ColorPalette`
//! module — the module the example's own QML files belong to
//! (their `qmldir` says so), which is why `Main.qml` names the
//! three types without importing anything.
//!
//! Every member here is one a QML file of the example uses; the
//! survey that found them is in this crate's report. Members are
//! thin: a property mirrors core state, a method builds a core
//! [`Request`](crate::rest::Request) and hands it to the transport
//! seam. The seam is `Platform::fetch` where the request is a GET;
//! where it is not, the core says so ([`RestError::UnsupportedMethod`])
//! and the type logs it and changes nothing — no raw socket, no
//! second HTTP client, and no pretending the write happened.

use std::cell::RefCell;

use ljos::prelude::*;
use serde_json::Value as Json;

use crate::rest::{BaseUrl, Paginated, Request, RestError, Session};

const CATEGORY: &str = "colorpalette";

/// `RestService`: the server every resource declared inside it
/// talks to (Main.qml:39-57).
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "ColorPalette",
    name = "RestService",
    extends = QtObject,
    default = "resources"
)]
pub struct RestService {
    #[qml(base)]
    pub object: QtObject,
    /// Written by the server picker before it refreshes its
    /// resources (ServerSelection.qml, `restPalette.url = url`).
    #[qml(notify)]
    pub url: String,
    /// Read by the connect button's label (ServerSelection.qml:
    /// "Connect (SSL)"). Says whether the platform's fetcher can
    /// speak TLS; no fetcher in the tree can yet (the default is
    /// `NoNetwork`), and the platform offers no way to ask, so
    /// this honestly says no until it can be derived.
    #[qml(notify, readonly)]
    pub ssl_supported: bool,
    /// The resources and logins declared inside the service.
    #[qml(list)]
    pub resources: QmlList,
}

/// The service `obj` was declared in, and the address it holds:
/// how a resource finds where to ask. A service with no URL yet —
/// before the user picks a server — is the core's `NoBaseUrl`.
fn base_of(engine: &Engine, obj: ObjectId) -> Result<BaseUrl, RestError> {
    let mut at = engine.owner(obj);
    while let Some(o) = at {
        if let Ok(url) = engine.with(o, |s: &RestService| s.url.clone()) {
            return BaseUrl::new(url);
        }
        at = engine.owner(o);
    }
    Err(RestError::NoBaseUrl)
}

/// `PaginatedResource`: one paged collection of the service it is
/// declared in (`users` and `unknown` in Main.qml:42-50).
#[derive(Debug, QmlObject)]
#[qml(
    module = "ColorPalette",
    name = "PaginatedResource",
    extends = QtObject
)]
pub struct PaginatedResource {
    #[qml(base)]
    pub object: QtObject,
    /// The collection's path under the service's URL.
    #[qml(notify)]
    pub path: String,
    /// The current page's rows: a list of row objects, which the
    /// QML both binds as a view model (ColorView.qml:244,
    /// UserMenu.qml:28) and walks in script
    /// (`users.data[i].email`, ColorView.qml:88-90).
    #[qml(notify, readonly)]
    pub data: Value,
    /// The page shown, from 1; writing it asks for that page
    /// (the page buttons in ColorView.qml:374, UserMenu.qml).
    #[qml(notify)]
    pub page: i32,
    /// How many pages there are, once a reply said so.
    #[qml(notify, readonly)]
    pub pages: i32,
    /// The core's state: the same page and count, and the rows
    /// still as JSON.
    #[qml(skip)]
    core: RefCell<Paginated>,
    /// Bumped by every fetch, so an answer to an older one is
    /// dropped (the FontLoader pattern).
    #[qml(skip)]
    generation: u64,
}

impl Default for PaginatedResource {
    fn default() -> Self {
        PaginatedResource {
            object: QtObject::default(),
            path: String::new(),
            // An empty list, not `undefined`: the QML reads
            // `data.length` and binds `data` as a model before
            // any reply exists.
            data: Value::list(Vec::new()),
            page: 1,
            pages: 0,
            core: RefCell::new(Paginated::new("")),
            generation: 0,
        }
    }
}

#[qml_impl]
impl PaginatedResource {
    /// A page arrived and `data`, `page`, `pages` now show it.
    /// ServerSelection closes its picker on the first one.
    #[qml(signal)]
    fn data_updated();

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        sync_path(e, me);
        e.watch(me, PaginatedResource::PATH.index, move |e, _| {
            sync_path(e, me);
        })?;
        e.watch(me, PaginatedResource::PAGE.index, move |e, v| {
            // Writing the page that is already shown is a
            // refresh, so it fetches too — but not when the
            // write is the core reporting the page a reply
            // actually answered for, which `page_fetched`
            // already knows.
            let shown = e
                .with(me, |r: &PaginatedResource| r.core.borrow().page())
                .unwrap_or(0);
            if v.to_number() as i32 != shown || shown == 0 {
                fetch_page(e, me, v.to_number() as i32);
            }
        })?;
        Ok(())
    }

    /// Asks for the current page again (ColorView.qml:231,
    /// ServerSelection's connect test).
    #[qml(invokable)]
    fn refresh_current_page(cx: &Cx) -> Result<(), EngineError> {
        let page = cx.get(PaginatedResource::PAGE)?;
        fetch_page(cx.engine, cx.this, page);
        Ok(())
    }

    /// Adds a row (`add({name, color, pantone_value})`,
    /// ColorView.qml:23-25). A POST: over today's seam the core's
    /// refusal is logged and nothing changes.
    #[qml(invokable)]
    fn add(cx: &Cx, row: Value) -> Result<(), EngineError> {
        let row = value_to_json(cx.engine, &row);
        send(cx.engine, cx.this, |core, base| core.request_add(base, row));
        Ok(())
    }

    /// Replaces a row (`update({…}, cid)`, ColorView.qml:29-32).
    #[qml(invokable)]
    fn update(cx: &Cx, row: Value, id: i32) -> Result<(), EngineError> {
        let row = value_to_json(cx.engine, &row);
        send(cx.engine, cx.this, |core, base| {
            core.request_update(base, row, id)
        });
        Ok(())
    }

    /// Removes a row (`remove(cid)`, ColorView.qml:39).
    #[qml(invokable)]
    fn remove(cx: &Cx, id: i32) -> Result<(), EngineError> {
        send(cx.engine, cx.this, |core, base| {
            core.request_remove(base, id)
        });
        Ok(())
    }
}

fn sync_path(e: &Engine, me: ObjectId) {
    let _ = e.with_mut(me, |r: &mut PaginatedResource| {
        r.core.borrow_mut().set_path(r.path.clone());
    });
}

/// Builds a write request (`add`, `update`, `remove`) and offers
/// it to the seam. None of them is a GET, so today this only ever
/// logs the core's refusal; it exists so the methods above stay
/// one line each and the day the seam carries a method and a
/// body, the sending lands here.
fn send(e: &Engine, me: ObjectId, build: impl FnOnce(&Paginated, &BaseUrl) -> Request) {
    let offered = base_of(e, me).and_then(|base| {
        e.with(me, |r: &PaginatedResource| build(&r.core.borrow(), &base))
            .map_err(|err| RestError::Network(err.to_string()))
    });
    if let Err(err) = offered.and_then(|r| r.get_url().map(|_| ())) {
        e.log(LogLevel::Warning, CATEGORY, format!("not sent: {err}"));
    }
}

/// The one place a request leaves the core today: a GET for one
/// page, over `Platform::fetch`.
fn fetch_page(e: &Engine, me: ObjectId, asked: i32) {
    let asked = asked.max(1);
    let built = base_of(e, me).and_then(|base| {
        e.with(me, |r: &PaginatedResource| {
            r.core.borrow().request_page(&base, asked)
        })
        .map_err(|err| RestError::Network(err.to_string()))
    });
    let url = match built.and_then(|r| r.get_url().map(|u: &str| u.to_owned())) {
        Ok(url) => url,
        Err(err) => {
            e.log(LogLevel::Warning, CATEGORY, format!("{err}"));
            return;
        }
    };
    let Ok(generation) = e.with_mut(me, |r: &mut PaginatedResource| {
        r.generation += 1;
        r.generation
    }) else {
        return;
    };
    Platform::of(e).fetch(&url, move |e, got| {
        let current = e
            .with(me, |r: &PaginatedResource| r.generation == generation)
            .unwrap_or(false);
        if !current || !e.is_alive(me) {
            return;
        }
        match got {
            Ok(bytes) => page_fetched(e, me, asked, &bytes),
            Err(err) => e.log(
                LogLevel::Warning,
                CATEGORY,
                format!("{}", RestError::Network(err.to_string())),
            ),
        }
    });
}

/// A page arrived: the core moves, the properties follow, and
/// `dataUpdated` says so — in that order, so a handler reading
/// `data` sees the new rows.
fn page_fetched(e: &Engine, me: ObjectId, asked: i32, bytes: &[u8]) {
    let applied = e.with_mut(me, |r: &mut PaginatedResource| {
        r.core.borrow_mut().apply_reply(asked, bytes)
    });
    let page = match applied {
        Ok(Ok(page)) => page,
        Ok(Err(err)) => {
            e.log(LogLevel::Warning, CATEGORY, format!("{err}"));
            return;
        }
        Err(_) => return,
    };
    let rows: Vec<Value> = page
        .rows
        .iter()
        .map(|row| json_to_value(e, me, row))
        .collect();
    let cx = Cx::new(e, me);
    let _ = cx.set(PaginatedResource::DATA, Value::list(rows));
    let _ = cx.set(PaginatedResource::PAGE, page.page);
    let _ = cx.set(PaginatedResource::PAGES, page.pages);
    let _ = PaginatedResource::emit_data_updated(&cx);
}

/// `BasicLogin`: logging in and out against paths of the service
/// it is declared in (Main.qml:52-56).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "ColorPalette", name = "BasicLogin", extends = QtObject)]
pub struct BasicLogin {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub login_path: String,
    #[qml(notify)]
    pub logout_path: String,
    /// Read all over ColorView to show and enable editing.
    #[qml(notify, readonly)]
    pub logged_in: bool,
    /// The logged-in user's email, compared against rows'
    /// `email` (UserMenu.qml:44, ColorView.qml:89).
    #[qml(notify, readonly)]
    pub user: String,
    #[qml(skip)]
    session: RefCell<Session>,
}

#[qml_impl]
impl BasicLogin {
    /// Logs in with the credentials as given
    /// (`login({email, password, id})`, UserMenu.qml:99-101).
    /// Login is a POST with a JSON body, which the platform's
    /// GET-bytes seam cannot express: today the core's refusal
    /// is logged and the session stays logged out. When a
    /// request-shaped seam exists, its reply lands in
    /// [`Session::apply_login_reply`] through `session_moved`
    /// and nothing else here changes.
    #[qml(invokable)]
    fn login(cx: &Cx, credentials: Value) -> Result<(), EngineError> {
        let credentials = value_to_json(cx.engine, &credentials);
        let login_path = cx.get(BasicLogin::LOGIN_PATH)?;
        let built = base_of(cx.engine, cx.this)
            .map(|base| Session::default().request_login(&base, &login_path, credentials));
        match built.and_then(|(request, _pending)| request.get_url().map(|u: &str| u.to_owned())) {
            // Unreachable while login is a POST; kept so the day
            // the seam grows, this arms instead of being written.
            Ok(_) => {}
            Err(err) => cx.engine.log(
                LogLevel::Warning,
                CATEGORY,
                format!("login not sent: {err}"),
            ),
        }
        Ok(())
    }

    /// Logs out (UserMenu.qml:96). The local session ends at
    /// once — that much needs no transport — and the request
    /// the core builds for the server meets the same seam as
    /// login's does, and is logged the same way.
    #[qml(invokable)]
    fn logout(cx: &Cx) -> Result<(), EngineError> {
        let logout_path = cx.get(BasicLogin::LOGOUT_PATH)?;
        if let Err(err) = base_of(cx.engine, cx.this).and_then(|base| {
            Session::default()
                .request_logout(&base, &logout_path)
                .get_url()
                .map(|_| ())
        }) {
            cx.engine.log(
                LogLevel::Warning,
                CATEGORY,
                format!("logout not sent: {err}"),
            );
        }
        let _ = cx.with_mut::<BasicLogin, _>(|l| l.session.borrow_mut().apply_logout());
        session_moved(cx);
        Ok(())
    }
}

/// The properties follow the session, wherever it moved.
fn session_moved(cx: &Cx) {
    let (logged_in, user) = cx
        .with::<BasicLogin, _>(|l| {
            let s = l.session.borrow();
            (s.logged_in(), s.user().to_owned())
        })
        .unwrap_or_default();
    let _ = cx.set(BasicLogin::LOGGED_IN, logged_in);
    let _ = cx.set(BasicLogin::USER, user);
}

/// A JSON row as QML reads it: primitives as themselves, an array
/// as a list, an object as a property map owned by the resource,
/// so `modelData.email` and `data[i].avatar` are plain reads.
fn json_to_value(e: &Engine, owner: ObjectId, json: &Json) -> Value {
    match json {
        Json::Null => Value::Null,
        Json::Bool(b) => Value::Bool(*b),
        Json::Number(n) => match n.as_i64().and_then(|i| i32::try_from(i).ok()) {
            Some(i) => Value::Int(i),
            None => Value::Real(n.as_f64().unwrap_or(f64::NAN)),
        },
        Json::String(s) => Value::String(Atom::new(s)),
        Json::Array(items) => {
            Value::list(items.iter().map(|i| json_to_value(e, owner, i)).collect())
        }
        Json::Object(fields) => match PropertyMap::new(e, Some(owner)) {
            Ok(map) => {
                for (key, value) in fields {
                    let value = json_to_value(e, owner, value);
                    map.insert(e, key, value);
                }
                Value::Object(map.object())
            }
            Err(_) => Value::Undefined,
        },
    }
}

/// What QML handed a method (`add({…})`, `login({…})`), back as
/// JSON for the core. A script object's own fields are read
/// through the engine; anything else converts as itself.
fn value_to_json(e: &Engine, value: &Value) -> Json {
    match value {
        Value::Undefined | Value::Null => Json::Null,
        Value::Bool(b) => Json::Bool(*b),
        Value::Int(i) => Json::from(*i),
        Value::Real(r) => serde_json::Number::from_f64(*r).map_or(Json::Null, Json::Number),
        Value::String(s) | Value::Url(s) => Json::String(s.to_string()),
        Value::List(items) => Json::Array(items.iter().map(|i| value_to_json(e, i)).collect()),
        Value::Object(id) => PropertyMap::from_object(e, *id).map_or(Json::Null, |map| {
            Json::Object(
                map.keys()
                    .iter()
                    .map(|k| (k.to_string(), value_to_json(e, &map.value(e, k))))
                    .collect(),
            )
        }),
        Value::Js(_) => Json::Object(
            e.js_object_entries(value)
                .into_iter()
                .map(|(k, v)| (k, value_to_json(e, &v)))
                .collect(),
        ),
        other => Json::String(other.to_js_string().to_string()),
    }
}

/// Registers the three types on `engine`, in the module the
/// example's QML files are themselves in.
pub fn register(engine: &Engine) {
    engine.register_module(ljos::module!(
        "ColorPalette" 1.0 => [RestService, PaginatedResource, BasicLogin]
    ));
}
