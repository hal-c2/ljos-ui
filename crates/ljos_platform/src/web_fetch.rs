//! `fetch` and the objects around it, as WHATWG's Fetch and DOM standards
//! have them (https://fetch.spec.whatwg.org/,
//! https://dom.spec.whatwg.org/#aborting-ongoing-activities), in the subset
//! docs/DIALECT.md §4.19 lists: `fetch`, `Request`, `Response`, `Headers`,
//! `AbortController`, `AbortSignal` and the `DOMException` an abort gives.
//! Every file's scripts see them (`features/platform/fetch.feature`).
//!
//! The classes are JS, on one native helper an engine makes when scripts
//! start, as `XMLHttpRequest`'s are. A request goes through
//! [`Platform::request_cancellable`] on a thread of its own, and its reply
//! comes back through the platform's inbox, whose first job wakes the
//! engine: while a request is out, nothing is scheduled. An aborted signal
//! settles the promise at once and cancels the request, whose thread stops
//! at its next read. `AbortSignal.timeout` is an engine deadline
//! ([`Engine::call_at`]), so it is a timer only while it has not fired.
//!
//! There is no CORS here, as there is no origin: the script is the
//! application. The forbidden request headers are still left out, as a
//! browser leaves them, so a script means the same on the web.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::time::Duration;

use ljos_engine::{Cx, Engine, LogLevel, ObjectId, QmlObject, Value, qml_impl};

use crate::fetch::{FetchError, Method, Request, Response, reason_phrase};
use crate::platform::{CATEGORY, Platform, RequestId};

/// What the classes call; one per engine.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "FetchHelper", uncreatable)]
pub struct FetchHelper {}

/// Requests on their way and replies not taken yet, by request number, and
/// the deadlines of `AbortSignal.timeout`, by theirs.
#[derive(Default)]
struct Fetches {
    next: Cell<i32>,
    out: RefCell<HashMap<i32, RequestId>>,
    kept: RefCell<HashMap<i32, Result<Response, FetchError>>>,
    deadlines: RefCell<HashMap<i32, ljos_runtime::TimerId>>,
}

impl Fetches {
    fn number(&self) -> i32 {
        let id = self.next.get() + 1;
        self.next.set(id);
        id
    }
}

#[qml_impl]
impl FetchHelper {
    /// The reply of request `id` has come.
    #[qml(signal)]
    fn replied(id: i32);

    /// The deadline `id` has come.
    #[qml(signal)]
    fn elapsed(id: i32);

    /// Sends a request and gives its number. `headers` is `name: value`
    /// lines; `body` is undefined, text (sent as UTF-8) or bytes.
    #[qml(invokable)]
    fn send(cx: &Cx, method: Value, url: Value, headers: Value, body: Value) -> i32 {
        let method = match method.to_js_string().as_str() {
            "POST" => Method::Post,
            "PUT" => Method::Put,
            "PATCH" => Method::Patch,
            "DELETE" => Method::Delete,
            "HEAD" => Method::Head,
            "OPTIONS" => Method::Options,
            "PROPFIND" => Method::Propfind,
            // The script has already refused any other.
            _ => Method::Get,
        };
        let mut request = Request::new(method, url.to_js_string().to_string());
        for line in headers.to_js_string().lines() {
            if let Some((name, value)) = line.split_once(": ") {
                request = request.header(name, value);
            }
        }
        let body = match &body {
            Value::Undefined | Value::Null => Vec::new(),
            Value::String(_) => body.to_js_string().as_bytes().to_vec(),
            _ => cx.engine.js_bytes(&body).unwrap_or_default(),
        };
        let request = request.body(body);
        let fetches = cx.engine.extension::<Fetches>();
        let id = fetches.number();
        let me = cx.this;
        let out = Platform::of(cx.engine).request_cancellable(request, move |e, reply| {
            let fetches = e.extension::<Fetches>();
            fetches.out.borrow_mut().remove(&id);
            fetches.kept.borrow_mut().insert(id, reply);
            tell(e, me, "replied", id);
        });
        fetches.out.borrow_mut().insert(id, out);
        id
    }

    /// The reply of request `id`, which is let go: `{ failed, message }`, or
    /// `{ status, statusText, headers, url, bytes }` with `headers` as
    /// `name: value` lines and the body as an ArrayBuffer.
    #[qml(invokable)]
    fn take(cx: &Cx, id: Value) -> Value {
        let id = id.to_number() as i32;
        let reply = cx
            .engine
            .extension::<Fetches>()
            .kept
            .borrow_mut()
            .remove(&id);
        match reply {
            Some(Ok(response)) => reply_value(cx.engine, response),
            Some(Err(error)) => Value::map([
                ("failed", Value::Bool(true)),
                ("message", Value::from(error.to_string().as_str())),
            ]),
            None => Value::map([
                ("failed", Value::Bool(true)),
                ("message", Value::from("the reply was lost")),
            ]),
        }
    }

    /// Stops request `id`: nobody wants its reply any more.
    #[qml(invokable)]
    fn cancel(cx: &Cx, id: Value) {
        let id = id.to_number() as i32;
        let fetches = cx.engine.extension::<Fetches>();
        fetches.kept.borrow_mut().remove(&id);
        let out = fetches.out.borrow_mut().remove(&id);
        if let Some(out) = out {
            Platform::of(cx.engine).cancel_request(out);
        }
    }

    /// The text of a body, as UTF-8, without a byte order mark.
    #[qml(invokable)]
    fn text_of(cx: &Cx, buffer: Value) -> String {
        let bytes = cx.engine.js_bytes(&buffer).unwrap_or_default();
        let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
        String::from_utf8_lossy(bytes).into_owned()
    }

    /// A deadline `ms` milliseconds from now, by its number; `elapsed`
    /// says when it comes.
    #[qml(invokable)]
    fn after(cx: &Cx, ms: Value) -> i32 {
        let ms = ms.to_number();
        let ms = if ms.is_finite() && ms > 0.0 {
            ms as u64
        } else {
            0
        };
        let fetches = cx.engine.extension::<Fetches>();
        let id = fetches.number();
        let me = cx.this;
        let at = cx.engine.now() + Duration::from_millis(ms);
        let timer = cx.engine.call_at(at, move |e| {
            e.extension::<Fetches>().deadlines.borrow_mut().remove(&id);
            tell(e, me, "elapsed", id);
        });
        fetches.deadlines.borrow_mut().insert(id, timer);
        id
    }
}

fn tell(e: &Engine, me: ObjectId, signal: &str, id: i32) {
    if let Err(err) = e.emit_by_name(me, signal, &[Value::Int(id)]) {
        e.log(
            LogLevel::Warning,
            CATEGORY,
            format!("fetch: an answer was lost: {err}"),
        );
    }
}

fn reply_value(engine: &Engine, response: Response) -> Value {
    let headers: String = response
        .headers
        .iter()
        .map(|(name, value)| format!("{}: {value}", name.to_ascii_lowercase()))
        .collect::<Vec<_>>()
        .join("\r\n");
    let mut fields = vec![
        ("failed", Value::Bool(false)),
        ("status", Value::Int(i32::from(response.status))),
        ("statusText", Value::from(reason_phrase(response.status))),
        ("headers", Value::from(headers.as_str())),
        (
            "url",
            Value::from(response.url.as_deref().unwrap_or_default()),
        ),
    ];
    // Handed over, not copied: a large reply stays out of the JS heap's
    // limit until a script reads it.
    fields.extend(
        engine
            .js_array_buffer(response.body)
            .map(|buffer| ("bytes", buffer)),
    );
    Value::map(fields)
}

pub(crate) fn register(engine: &Engine) {
    engine.on_js_started(|e| {
        let installed = e
            .create_native_incomplete(FetchHelper {}, None)
            .map_err(|err| err.to_string())
            .and_then(|helper| {
                let install = e.eval_js(INSTALL).map_err(|err| err.to_string())?;
                e.call_js(&install, &[Value::Object(helper)]);
                Ok(())
            });
        if let Err(err) = installed {
            e.log(
                LogLevel::Warning,
                CATEGORY,
                format!("fetch is not available: {err}"),
            );
        }
    });
}

const INSTALL: &str = r#"(function (helper) {
  "use strict";
  const define = (name, value) =>
    Object.defineProperty(globalThis, name, { value, writable: true, configurable: true });
  const replies = new Map();
  const deadlines = new Map();
  helper.replied.connect((id) => {
    const done = replies.get(id);
    replies.delete(id);
    if (done) done();
  });
  helper.elapsed.connect((id) => {
    const done = deadlines.get(id);
    deadlines.delete(id);
    if (done) done();
  });
  // An exception in a listener is reported, as a browser reports it, and
  // the other listeners still run.
  const report = (e) => console.error(e);

  // WebIDL's DOMException, where the engine has none.
  const CODES = {
    IndexSizeError: 1, HierarchyRequestError: 3, WrongDocumentError: 4,
    InvalidCharacterError: 5, NoModificationAllowedError: 7, NotFoundError: 8,
    NotSupportedError: 9, InvalidStateError: 11, SyntaxError: 12,
    InvalidModificationError: 13, NamespaceError: 14, InvalidAccessError: 15,
    TypeMismatchError: 17, SecurityError: 18, NetworkError: 19, AbortError: 20,
    URLMismatchError: 21, QuotaExceededError: 22, TimeoutError: 23,
    InvalidNodeTypeError: 24, DataCloneError: 25,
  };
  if (typeof globalThis.DOMException !== "function") {
    class DOMException extends Error {
      #name;
      constructor(message = "", name = "Error") {
        super(String(message));
        this.#name = String(name);
      }
      get name() { return this.#name; }
      get code() { return CODES[this.#name] || 0; }
    }
    const CONSTANTS = {
      INDEX_SIZE_ERR: 1, HIERARCHY_REQUEST_ERR: 3, WRONG_DOCUMENT_ERR: 4,
      INVALID_CHARACTER_ERR: 5, NO_MODIFICATION_ALLOWED_ERR: 7, NOT_FOUND_ERR: 8,
      NOT_SUPPORTED_ERR: 9, INVALID_STATE_ERR: 11, SYNTAX_ERR: 12,
      INVALID_MODIFICATION_ERR: 13, NAMESPACE_ERR: 14, INVALID_ACCESS_ERR: 15,
      TYPE_MISMATCH_ERR: 17, SECURITY_ERR: 18, NETWORK_ERR: 19, ABORT_ERR: 20,
      URL_MISMATCH_ERR: 21, QUOTA_EXCEEDED_ERR: 22, TIMEOUT_ERR: 23,
      INVALID_NODE_TYPE_ERR: 24, DATA_CLONE_ERR: 25,
    };
    for (const [name, value] of Object.entries(CONSTANTS)) {
      Object.defineProperty(DOMException, name, { value, enumerable: true });
      Object.defineProperty(DOMException.prototype, name, { value, enumerable: true });
    }
    define("DOMException", DOMException);
  }
  const DOMException = globalThis.DOMException;

  // AbortSignal and AbortController.
  const signals = new WeakMap();
  const signalOf = (signal) => {
    const s = signals.get(signal);
    if (!s) throw new TypeError("not an AbortSignal");
    return s;
  };
  let making = false;
  const newSignal = () => {
    making = true;
    try { return new AbortSignal(); } finally { making = false; }
  };
  // The steps an abort runs: what is waiting on the signal (a fetch) is
  // stopped first, then the listeners hear it.
  const fire = (signal) => {
    const s = signals.get(signal);
    for (const step of [...s.steps]) {
      try { step(); } catch (e) { report(e); }
    }
    s.steps.clear();
    const event = { type: "abort", target: signal, currentTarget: signal, defaultPrevented: false };
    for (const entry of [...s.listeners]) {
      if (entry.removed) continue;
      if (entry.once) removeEntry(s, entry);
      const listener = entry.handler ? s.onabort : entry.listener;
      try {
        if (typeof listener === "function") listener.call(signal, event);
        else if (listener && typeof listener.handleEvent === "function") listener.handleEvent(event);
      } catch (e) { report(e); }
    }
  };
  const removeEntry = (s, entry) => {
    entry.removed = true;
    s.listeners.splice(s.listeners.indexOf(entry), 1);
  };
  const abort = (signal, reason) => {
    const s = signals.get(signal);
    if (s.aborted) return;
    s.aborted = true;
    s.reason = reason !== undefined ? reason
      : new DOMException("signal is aborted without reason", "AbortError");
    // Those that follow it are aborted with it, before any listener runs.
    const followers = [];
    for (const follower of s.followers) {
      const f = signals.get(follower);
      if (f.aborted) continue;
      f.aborted = true;
      f.reason = s.reason;
      followers.push(follower);
    }
    s.followers.clear();
    fire(signal);
    for (const follower of followers) fire(follower);
  };
  class AbortSignal {
    constructor() {
      if (!making) throw new TypeError("Illegal constructor");
      signals.set(this, {
        aborted: false, reason: undefined, listeners: [], onabort: null,
        steps: new Set(), followers: new Set(),
      });
    }
    get aborted() { return signalOf(this).aborted; }
    get reason() { return signalOf(this).reason; }
    get onabort() { return signalOf(this).onabort; }
    set onabort(handler) {
      const s = signalOf(this);
      s.onabort = typeof handler === "function" ? handler : null;
      // An event handler holds its place among the listeners from when it
      // was first set, as HTML says.
      if (s.onabort && !s.listeners.some((entry) => entry.handler))
        s.listeners.push({ handler: true });
    }
    throwIfAborted() {
      const s = signalOf(this);
      if (s.aborted) throw s.reason;
    }
    addEventListener(type, listener, options) {
      const s = signalOf(this);
      if (String(type) !== "abort" || listener === null || listener === undefined) return;
      const once = typeof options === "object" && options !== null && !!options.once;
      if (s.listeners.some((entry) => entry.listener === listener)) return;
      s.listeners.push({ listener, once });
    }
    removeEventListener(type, listener) {
      const s = signalOf(this);
      if (String(type) !== "abort") return;
      const entry = s.listeners.find((e) => e.listener === listener);
      if (entry) removeEntry(s, entry);
    }
    static abort(reason) {
      const signal = newSignal();
      abort(signal, reason);
      return signal;
    }
    static timeout(ms) {
      const delay = Number(ms);
      if (!Number.isFinite(delay) || delay < 0 || delay > Number.MAX_SAFE_INTEGER)
        throw new TypeError("AbortSignal.timeout: the delay must be a whole number of milliseconds");
      const signal = newSignal();
      deadlines.set(helper.after(Math.trunc(delay)), () =>
        abort(signal, new DOMException("signal timed out", "TimeoutError")));
      return signal;
    }
    static any(given) {
      const signal = newSignal();
      const all = [...given];
      for (const source of all) signalOf(source);
      const first = all.find((source) => source.aborted);
      if (first) {
        signals.get(signal).aborted = true;
        signals.get(signal).reason = first.reason;
        return signal;
      }
      for (const source of all) signals.get(source).followers.add(signal);
      return signal;
    }
  }
  class AbortController {
    #signal = newSignal();
    get signal() { return this.#signal; }
    abort(reason) { abort(this.#signal, reason); }
  }

  // Headers.
  const TOKEN = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;
  const trim = (value) => String(value).replace(/^[\t\n\r ]+|[\t\n\r ]+$/g, "");
  const FORBIDDEN = new Set(["accept-charset", "accept-encoding",
    "access-control-request-headers", "access-control-request-method", "connection",
    "content-length", "cookie", "cookie2", "date", "dnt", "expect", "host", "keep-alive",
    "origin", "referer", "set-cookie", "te", "trailer", "transfer-encoding", "upgrade", "via"]);
  const OVERRIDES = new Set(["x-http-method", "x-http-method-override", "x-method-override"]);
  const BANNED_METHODS = new Set(["CONNECT", "TRACE", "TRACK"]);
  // What only the network sets: the fetch leaves these out of a request,
  // as a browser does.
  const forbidden = (name, value) => {
    if (FORBIDDEN.has(name) || name.startsWith("proxy-") || name.startsWith("sec-")) return true;
    return OVERRIDES.has(name) && value.split(",").some((m) => BANNED_METHODS.has(trim(m).toUpperCase()));
  };
  const lists = new WeakMap();
  const listOf = (headers) => {
    const h = lists.get(headers);
    if (!h) throw new TypeError("not a Headers");
    return h;
  };
  const checked = (h, name, value) => {
    name = String(name);
    if (!TOKEN.test(name)) throw new TypeError(`"${name}" is not a valid header name`);
    if (value !== undefined) {
      value = trim(value);
      if (/[\0\r\n]/.test(value)) throw new TypeError(`"${value}" is not a valid header value`);
    }
    if (h.guard === "immutable") throw new TypeError("these headers cannot be changed");
    return [name.toLowerCase(), value];
  };
  const append = (h, name, value) => {
    const [lower, v] = checked(h, name, value);
    if (h.guard === "request" && forbidden(lower, v)) return;
    h.list.push([lower, v]);
  };
  const fill = (h, init) => {
    if (init === undefined || init === null) return;
    if (init instanceof Headers) {
      for (const [name, value] of listOf(init).list) append(h, name, value);
    } else if (typeof init === "object" && typeof init[Symbol.iterator] === "function") {
      for (const pair of init) {
        const both = [...pair];
        if (both.length !== 2) throw new TypeError("a header is a pair of a name and a value");
        append(h, both[0], both[1]);
      }
    } else if (typeof init === "object") {
      for (const [name, value] of Object.entries(init)) append(h, name, value);
    } else {
      throw new TypeError("headers are a Headers, pairs or an object");
    }
  };
  // Sorted by name, each name once with its values joined, as WHATWG's
  // "sort and combine" gives them.
  const combined = (h) => {
    const names = [...new Set(h.list.map(([name]) => name))].sort();
    const out = [];
    for (const name of names) {
      const values = h.list.filter(([n]) => n === name).map(([, v]) => v);
      if (name === "set-cookie") for (const v of values) out.push([name, v]);
      else out.push([name, values.join(", ")]);
    }
    return out;
  };
  const guarded = (init, guard) => {
    const headers = new Headers();
    const h = listOf(headers);
    h.guard = guard === "immutable" ? "none" : guard;
    fill(h, init);
    h.guard = guard;
    return headers;
  };
  class Headers {
    constructor(init) {
      lists.set(this, { list: [], guard: "none" });
      fill(listOf(this), init);
    }
    append(name, value) { append(listOf(this), name, value); }
    delete(name) {
      const h = listOf(this);
      const [lower] = checked(h, name);
      if (h.guard === "request" && forbidden(lower, "")) return;
      h.list = h.list.filter(([n]) => n !== lower);
    }
    get(name) {
      const h = listOf(this);
      const lower = String(name).toLowerCase();
      if (!TOKEN.test(lower)) throw new TypeError(`"${name}" is not a valid header name`);
      const values = h.list.filter(([n]) => n === lower).map(([, v]) => v);
      return values.length ? values.join(", ") : null;
    }
    has(name) {
      const lower = String(name).toLowerCase();
      if (!TOKEN.test(lower)) throw new TypeError(`"${name}" is not a valid header name`);
      return listOf(this).list.some(([n]) => n === lower);
    }
    set(name, value) {
      const h = listOf(this);
      const [lower, v] = checked(h, name, value);
      if (h.guard === "request" && forbidden(lower, v)) return;
      const at = h.list.findIndex(([n]) => n === lower);
      if (at < 0) { h.list.push([lower, v]); return; }
      h.list[at] = [lower, v];
      h.list = h.list.filter(([n], i) => n !== lower || i <= at);
    }
    getSetCookie() {
      return listOf(this).list.filter(([n]) => n === "set-cookie").map(([, v]) => v);
    }
    forEach(callback, thisArg) {
      for (const [name, value] of combined(listOf(this))) callback.call(thisArg, value, name, this);
    }
    entries() { return combined(listOf(this))[Symbol.iterator](); }
    keys() { return combined(listOf(this)).map(([name]) => name)[Symbol.iterator](); }
    values() { return combined(listOf(this)).map(([, value]) => value)[Symbol.iterator](); }
    [Symbol.iterator]() { return this.entries(); }
  }

  // Bodies: what init's body is, as bytes or text, and the type it says.
  const bytesOf = (body) => {
    if (body instanceof ArrayBuffer) return new Uint8Array(body.slice(0));
    return new Uint8Array(body.buffer.slice(body.byteOffset, body.byteOffset + body.byteLength));
  };
  const extract = (body) => {
    if (body === undefined || body === null) return { body: null, type: null };
    if (body instanceof ArrayBuffer || ArrayBuffer.isView(body))
      return { body: bytesOf(body), type: null };
    return { body: String(body), type: "text/plain;charset=UTF-8" };
  };

  // Request.
  const METHODS = ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "PROPFIND"];
  const NORMAL = new Set(["DELETE", "GET", "HEAD", "OPTIONS", "POST", "PUT"]);
  const requests = new WeakMap();
  const requestOf = (request) => {
    const r = requests.get(request);
    if (!r) throw new TypeError("not a Request");
    return r;
  };
  class Request {
    constructor(input, init) {
      init = init === undefined || init === null ? {} : init;
      if (typeof init !== "object") throw new TypeError("a request's init is an object");
      const from = input instanceof Request ? requestOf(input) : null;
      const url = from ? from.url : String(input);
      let method = from ? from.method : "GET";
      if (init.method !== undefined) {
        method = String(init.method);
        if (!TOKEN.test(method)) throw new TypeError(`"${method}" is not a valid HTTP method`);
        const upper = method.toUpperCase();
        if (BANNED_METHODS.has(upper)) throw new TypeError(`"${method}" HTTP method is unsupported`);
        if (NORMAL.has(upper)) method = upper;
      }
      if (!METHODS.includes(method))
        throw new TypeError(`"${method}" HTTP method is unsupported`);
      let body = from ? from.body : null;
      let type = null;
      if (init.body !== undefined) ({ body, type } = extract(init.body));
      if (body !== null && (method === "GET" || method === "HEAD"))
        throw new TypeError("a request with a GET or HEAD method cannot have a body");
      const headers = guarded(init.headers !== undefined ? init.headers
        : from ? from.headers : undefined, "request");
      if (type !== null && !headers.has("content-type")) headers.append("content-type", type);
      let signal = from ? from.signal : null;
      if (init.signal !== undefined) {
        if (init.signal !== null) signalOf(init.signal);
        signal = init.signal;
      }
      requests.set(this, { url, method, headers, body, signal: AbortSignal.any(signal ? [signal] : []), given: signal });
    }
    get url() { return requestOf(this).url; }
    get method() { return requestOf(this).method; }
    get headers() { return requestOf(this).headers; }
    get signal() { return requestOf(this).signal; }
  }

  // Response.
  const responses = new WeakMap();
  const responseOf = (response) => {
    const r = responses.get(response);
    if (!r) throw new TypeError("not a Response");
    return r;
  };
  const consume = (response, read) => {
    const r = responseOf(response);
    if (r.used) return Promise.reject(new TypeError("the body of this response has already been read"));
    if (r.body !== null) r.used = true;
    try { return Promise.resolve(read(r.body)); } catch (e) { return Promise.reject(e); }
  };
  const asText = (body) => body === null ? ""
    : typeof body === "string" ? body : helper.textOf(body);
  const asBytes = (body) => body === null ? new ArrayBuffer(0)
    : typeof body === "string" ? utf8(body) : body;
  // A string as UTF-8, where the engine has no TextEncoder; a lone
  // surrogate is U+FFFD, as TextEncoder makes it.
  const utf8 = (text) => {
    const out = [];
    for (const ch of text) {
      let c = ch.codePointAt(0);
      if (c >= 0xd800 && c <= 0xdfff) c = 0xfffd;
      if (c < 0x80) out.push(c);
      else if (c < 0x800) out.push(0xc0 | (c >> 6), 0x80 | (c & 63));
      else if (c < 0x10000) out.push(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
      else out.push(0xf0 | (c >> 18), 0x80 | ((c >> 12) & 63), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
    }
    return new Uint8Array(out).buffer;
  };
  const made = (fields) => {
    const response = Object.create(Response.prototype);
    responses.set(response, fields);
    return response;
  };
  class Response {
    constructor(body, init) {
      init = init === undefined || init === null ? {} : init;
      const status = init.status === undefined ? 200 : Number(init.status);
      if (!Number.isInteger(status) || status < 200 || status > 599)
        throw new RangeError(`${init.status} is not a status a response can be made with`);
      const statusText = init.statusText === undefined ? "" : String(init.statusText);
      const headers = guarded(init.headers, "response");
      const { body: content, type } = extract(body);
      if (content !== null && (status === 101 || status === 204 || status === 205 || status === 304))
        throw new TypeError(`a response of status ${status} cannot have a body`);
      if (type !== null && !headers.has("content-type")) headers.append("content-type", type);
      responses.set(this, { type: "default", status, statusText, headers, url: "",
        body: content === null ? null : typeof content === "string" ? content : content.buffer,
        used: false, redirected: false });
    }
    get type() { return responseOf(this).type; }
    get url() { return responseOf(this).url; }
    get redirected() { return responseOf(this).redirected; }
    get status() { return responseOf(this).status; }
    get ok() { const s = responseOf(this).status; return s >= 200 && s <= 299; }
    get statusText() { return responseOf(this).statusText; }
    get headers() { return responseOf(this).headers; }
    get bodyUsed() { return responseOf(this).used; }
    text() { return consume(this, asText); }
    json() { return consume(this, (body) => JSON.parse(asText(body))); }
    arrayBuffer() { return consume(this, asBytes); }
    static error() {
      return made({ type: "error", status: 0, statusText: "", url: "", body: null,
        headers: guarded(undefined, "immutable"), used: false, redirected: false });
    }
  }

  // fetch.
  const fetch = (input, init) => new Promise((resolve, reject) => {
    const request = new Request(input, init);
    const r = requestOf(request);
    const signal = r.given;
    if (signal && signal.aborted) {
      reject(signal.reason);
      return;
    }
    const lines = [...r.headers.entries()].map(([name, value]) => name + ": " + value).join("\n");
    const body = r.body === null || r.method === "GET" || r.method === "HEAD" ? undefined : r.body;
    const id = helper.send(r.method, r.url, lines, body);
    let stop = null;
    if (signal) {
      stop = () => {
        replies.delete(id);
        helper.cancel(id);
        reject(signal.reason);
      };
      signalOf(signal).steps.add(stop);
    }
    replies.set(id, () => {
      if (stop) signalOf(signal).steps.delete(stop);
      const reply = helper.take(id);
      if (reply.failed) {
        reject(new TypeError("fetch failed: " + reply.message));
        return;
      }
      const headers = guarded(reply.headers === "" ? [] : reply.headers.split("\r\n").map((line) => {
        const at = line.indexOf(": ");
        return [line.slice(0, at), line.slice(at + 2)];
      }), "immutable");
      resolve(made({ type: "basic", status: reply.status, statusText: reply.statusText,
        headers, url: reply.url || r.url, redirected: reply.url !== "" && reply.url !== r.url,
        body: reply.bytes || null, used: false }));
    });
  });

  define("AbortSignal", AbortSignal);
  define("AbortController", AbortController);
  define("Headers", Headers);
  define("Request", Request);
  define("Response", Response);
  define("fetch", fetch);
})
"#;
