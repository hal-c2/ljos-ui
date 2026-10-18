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
//! seam. The seam is `Platform::request`: a core request converts
//! with [`Request::to_platform`](crate::rest::Request::to_platform)
//! and travels whole, whatever its method, and the reply — any
//! HTTP status, or the transport's refusal — comes back to the
//! core to judge. What goes wrong is logged the way fetch errors
//! have always been logged here, and nothing pretends a failed
//! write or login happened.

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
    /// ColorView.qml:23-25): a POST of the row to the server.
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

/// What a reply that carries nothing back for the core still has
/// to say: a refusal, from the transport or from the server, is
/// logged the way fetch errors always have been here, and an
/// agreed write is silent — the QML re-reads the rows when it
/// asks for the page again.
fn log_refusal(e: &Engine, got: &Result<Response, FetchError>) {
    let refused = match got {
        Ok(response) if response.ok() => None,
        Ok(response) => Some(RestError::HttpStatus(response.status)),
        Err(err) => Some(RestError::Network(err.to_string())),
    };
    if let Some(err) = refused {
        e.log(LogLevel::Warning, CATEGORY, format!("{err}"));
    }
}

/// Builds a write request (`add`, `update`, `remove`) and sends
/// it over the seam, so the methods above stay one line each. A
/// request that cannot even be built — no server picked yet —
/// is logged and nothing is sent.
fn send(e: &Engine, me: ObjectId, build: impl FnOnce(&Paginated, &BaseUrl) -> Request) {
    let built = base_of(e, me).and_then(|base| {
        e.with(me, |r: &PaginatedResource| build(&r.core.borrow(), &base))
            .map_err(|err| RestError::Network(err.to_string()))
    });
    let request = match built {
        Ok(request) => request.to_platform(),
        Err(err) => {
            e.log(LogLevel::Warning, CATEGORY, format!("not sent: {err}"));
            return;
        }
    };
    Platform::of(e).request(request, move |e, got| {
        if e.is_alive(me) {
            log_refusal(e, &got);
        }
    });
}

/// Asks for one page over the seam: a GET today as it always was,
/// now one request among the rest, with the reply's status judged
/// before its bytes are read as a page.
fn fetch_page(e: &Engine, me: ObjectId, asked: i32) {
    let asked = asked.max(1);
    let built = base_of(e, me).and_then(|base| {
        e.with(me, |r: &PaginatedResource| {
            r.core.borrow().request_page(&base, asked)
        })
        .map_err(|err| RestError::Network(err.to_string()))
    });
    let request = match built {
        Ok(request) => request.to_platform(),
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
    Platform::of(e).request(request, move |e, got| {
        let current = e
            .with(me, |r: &PaginatedResource| r.generation == generation)
            .unwrap_or(false);
        if !current || !e.is_alive(me) {
            return;
        }
        match &got {
            Ok(response) if response.ok() => page_fetched(e, me, asked, &response.body),
            _ => log_refusal(e, &got),
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
    /// (`login({email, password, id})`, UserMenu.qml:99-101): a
    /// POST with the credentials as its JSON body. The session
    /// moves only when the reply lands in
    /// [`Session::apply_login_reply`], which is where a non-2xx
    /// answer is turned away — it can never set `loggedIn` — and
    /// the refusal reaches the log in the core's words.
    #[qml(invokable)]
    fn login(cx: &Cx, credentials: Value) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let credentials = value_to_json(e, &credentials);
        let login_path: String = cx.get(BasicLogin::LOGIN_PATH)?;
        let built = base_of(e, me).and_then(|base| {
            cx.with::<BasicLogin, _>(|l| {
                l.session
                    .borrow()
                    .request_login(&base, &login_path, credentials)
            })
            .map_err(|err| RestError::Network(err.to_string()))
        });
        let (request, pending) = match built {
            Ok(built) => built,
            Err(err) => {
                e.log(
                    LogLevel::Warning,
                    CATEGORY,
                    format!("login not sent: {err}"),
                );
                return Ok(());
            }
        };
        Platform::of(e).request(request.to_platform(), move |e, got| {
            if !e.is_alive(me) {
                return;
            }
            let applied = e.with_mut(me, |l: &mut BasicLogin| {
                l.session
                    .borrow_mut()
                    .apply_login_reply(pending, got.as_ref())
            });
            match applied {
                Ok(Ok(())) => session_moved(&Cx::new(e, me)),
                Ok(Err(err)) => e.log(LogLevel::Warning, CATEGORY, format!("{err}")),
                Err(_) => {}
            }
        });
        Ok(())
    }

    /// Logs out (UserMenu.qml:96). The request goes to the server
    /// over the same seam as login's and its refusal is logged
    /// the same way, but the local session ends at once, before
    /// any answer: whoever was logged in no longer is, even if
    /// the request is never built, never travels, or is refused.
    #[qml(invokable)]
    fn logout(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let logout_path: String = cx.get(BasicLogin::LOGOUT_PATH)?;
        let built = base_of(e, me).and_then(|base| {
            cx.with::<BasicLogin, _>(|l| l.session.borrow().request_logout(&base, &logout_path))
                .map_err(|err| RestError::Network(err.to_string()))
        });
        match built {
            Ok(request) => {
                Platform::of(e).request(request.to_platform(), move |e, got| {
                    if e.is_alive(me) {
                        log_refusal(e, &got);
                    }
                });
            }
            Err(err) => e.log(
                LogLevel::Warning,
                CATEGORY,
                format!("logout not sent: {err}"),
            ),
        }
        let _ = cx.with_mut::<BasicLogin, _>(|l| l.session.borrow_mut().apply_logout());
        session_moved(cx);
        Ok(())
    }
}

/// The properties follow the session, wherever it moved. `user` is
/// whoever is logged in whenever `loggedIn` reads true: set before it
/// on the way in and after it on the way out, so a binding that looks
/// the user up on `loggedIn` (ColorView.qml's avatar) never runs with
/// nobody.
fn session_moved(cx: &Cx) {
    let (logged_in, user) = cx
        .with::<BasicLogin, _>(|l| {
            let s = l.session.borrow();
            (s.logged_in(), s.user().to_owned())
        })
        .unwrap_or_default();
    if logged_in {
        let _ = cx.set(BasicLogin::USER, user);
        let _ = cx.set(BasicLogin::LOGGED_IN, true);
    } else {
        let _ = cx.set(BasicLogin::LOGGED_IN, false);
        let _ = cx.set(BasicLogin::USER, user);
    }
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
