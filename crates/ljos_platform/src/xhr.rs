//! The `XMLHttpRequest` every script sees (https://xhr.spec.whatwg.org/,
//! as Qt has it: `features/platform/xml-http-request.feature`).
//!
//! The class is JS, on one native helper an engine makes when scripts
//! start. A request goes through [`Platform::request`], so it is sent on a
//! thread of its own through the host's [`Fetcher`](crate::Fetcher), and
//! its reply is kept here until the script's object takes it. One that is
//! not asynchronous is sent on the Engine thread and waited for, as Qt
//! waits for it.
//!
//! Qt's reply has the server's own reason phrase; this one has the
//! standard phrase of its status. A local file is not read, as Qt reads
//! none unless `QML_XHR_ALLOW_FILE_READ` is set. `responseXML` is the
//! read-only DOM Qt has, read out of a reply of an XML type when a script
//! first asks for it (`crate::xml_dom`).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use ljos_engine::{Cx, Engine, LogLevel, ObjectId, QmlObject, Value, qml_impl};

use crate::fetch::{FetchError, Method, Request, Response, reason_phrase};
use crate::platform::{CATEGORY, Platform};

/// What the class calls; one per engine.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "XMLHttpRequestHelper", uncreatable)]
pub struct XhrHelper {}

/// Replies that came and were not taken yet, by request number.
#[derive(Default)]
struct Replies {
    next: Cell<i32>,
    kept: RefCell<HashMap<i32, Result<Response, FetchError>>>,
}

#[qml_impl]
impl XhrHelper {
    /// The reply of request `id` has come.
    #[qml(signal)]
    fn replied(id: i32);

    /// Sends a request and gives its number. `headers` is `name: value`
    /// lines; `body` is text, or bytes one to a character when `bytes`.
    /// With `wait` the reply has come when this returns.
    #[qml(invokable)]
    fn send(
        cx: &Cx,
        method: Value,
        url: Value,
        headers: Value,
        body: Value,
        bytes: Value,
        wait: Value,
    ) -> i32 {
        let request = request(&method, &url, &headers, &body, bytes.to_boolean());
        let replies = cx.engine.extension::<Replies>();
        let id = replies.next.get() + 1;
        replies.next.set(id);
        if wait.to_boolean() {
            let reply = Platform::of(cx.engine).fetcher().request(&request);
            replies.kept.borrow_mut().insert(id, reply);
            return id;
        }
        let me = cx.this;
        Platform::of(cx.engine).request(request, move |e, reply| {
            e.extension::<Replies>().kept.borrow_mut().insert(id, reply);
            tell(e, me, id);
        });
        id
    }

    /// The reply of request `id`, which is let go: `{ failed, status,
    /// statusText, headers, text }`, and `bytes`, one to a character, when
    /// asked for.
    #[qml(invokable)]
    fn take(cx: &Cx, id: Value, bytes: Value) -> Value {
        let id = id.to_number() as i32;
        let reply = cx
            .engine
            .extension::<Replies>()
            .kept
            .borrow_mut()
            .remove(&id);
        match reply {
            Some(Ok(response)) => reply_value(&response, bytes.to_boolean()),
            _ => Value::map([("failed", Value::Bool(true))]),
        }
    }

    /// Says, as Qt does, that a local file is not read.
    #[qml(invokable)]
    fn refuse_local(cx: &Cx, method: Value) {
        cx.engine.log(
            LogLevel::Warning,
            CATEGORY,
            format!(
                "XMLHttpRequest: Using {} on a local file is disabled by default.\n\
                 Set QML_XHR_ALLOW_FILE_READ to 1 to enable this feature.",
                method.to_js_string()
            ),
        );
    }

    /// The document in a reply's text, as JSON, or null when the reply is
    /// of no XML type or is not well-formed.
    #[qml(invokable)]
    fn document(_cx: &Cx, text: Value, content_type: Value) -> Value {
        if !crate::xml_dom::is_xml(&content_type.to_js_string()) {
            return Value::Null;
        }
        match crate::xml_dom::document(&text.to_js_string()) {
            Some(json) => Value::from(json.as_str()),
            None => Value::Null,
        }
    }

    /// Lets go of the reply of a request the script stopped.
    #[qml(invokable)]
    fn forget(cx: &Cx, id: Value) {
        let id = id.to_number() as i32;
        cx.engine
            .extension::<Replies>()
            .kept
            .borrow_mut()
            .remove(&id);
    }
}

fn tell(e: &Engine, me: ObjectId, id: i32) {
    if let Err(err) = e.emit_by_name(me, "replied", &[Value::Int(id)]) {
        e.log(
            LogLevel::Warning,
            CATEGORY,
            format!("XMLHttpRequest: a reply was lost: {err}"),
        );
    }
}

fn request(method: &Value, url: &Value, headers: &Value, body: &Value, bytes: bool) -> Request {
    let method = match method.to_js_string().to_ascii_uppercase().as_str() {
        "POST" => Method::Post,
        "PUT" => Method::Put,
        "PATCH" => Method::Patch,
        "DELETE" => Method::Delete,
        "HEAD" => Method::Head,
        "OPTIONS" => Method::Options,
        "PROPFIND" => Method::Propfind,
        _ => Method::Get,
    };
    let mut request = Request::new(method, url.to_js_string().to_string());
    for line in headers.to_js_string().lines() {
        if let Some((name, value)) = line.split_once(": ") {
            request = request.header(name, value);
        }
    }
    let body = match body {
        Value::Undefined | Value::Null => Vec::new(),
        // Characters below 256, one to a byte: what the script made of
        // an ArrayBuffer.
        _ if bytes => body.to_js_string().chars().map(|c| c as u8).collect(),
        _ => body.to_js_string().as_bytes().to_vec(),
    };
    request.body(body)
}

fn reply_value(response: &Response, bytes: bool) -> Value {
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
        ("text", Value::from(response.text().as_str())),
    ];
    if bytes {
        let latin1: String = response.body.iter().map(|&b| char::from(b)).collect();
        fields.push(("bytes", Value::from(latin1.as_str())));
    }
    Value::map(fields)
}

pub(crate) fn register(engine: &Engine) {
    engine.on_js_started(|e| {
        let installed = e
            .create_native_incomplete(XhrHelper {}, None)
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
                format!("XMLHttpRequest is not available: {err}"),
            );
        }
    });
}

const INSTALL: &str = r#"(function (helper) {
  "use strict";
  const UNSENT = 0, OPENED = 1, HEADERS_RECEIVED = 2, LOADING = 3, DONE = 4;
  const METHODS = ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "PROPFIND"];
  // What a script may not set; the network sets these itself.
  const FORBIDDEN = ["accept-charset", "accept-encoding", "connection", "content-length",
    "cookie", "cookie2", "content-transfer-encoding", "date", "expect", "host",
    "keep-alive", "referer", "te", "trailer", "transfer-encoding", "upgrade",
    "user-agent", "via"];
  const waiting = new Map();
  helper.replied.connect(function (id) {
    const done = waiting.get(id);
    waiting.delete(id);
    if (done) done();
  });
  const fail = (code, message) => {
    const e = new Error(message);
    e.code = code;
    return e;
  };
  const invalid = () => fail(11, "Invalid state");
  const state = new WeakMap();
  const remote = (url) => /^https?:/i.test(url);

  // responseXML's nodes. What they hold is kept apart, so a script reads
  // it through the getters only, and a write to one is let go, as Qt's.
  const ELEMENT = 1, ATTRIBUTE = 2, TEXT = 3, CDATA = 4, DOCUMENT = 9;
  const NO_NODES = Object.freeze([]);
  const nodes = new WeakMap();
  const inner = (node) => nodes.get(node);
  const getters = (cls, all) => {
    for (const [name, get] of Object.entries(all))
      Object.defineProperty(cls.prototype, name, { get, set() {}, configurable: true });
  };
  const sibling = (node, step) => {
    const n = inner(node);
    const all = n.parent ? inner(n.parent).children : NO_NODES;
    return all[n.index + step] || null;
  };
  class Node {}
  class Document extends Node {}
  class Element extends Node {}
  class Attr extends Node {}
  class CharacterData extends Node {}
  class Text extends CharacterData {}
  class CDATASection extends Text {}
  getters(Node, {
    nodeName() { return inner(this).name; },
    nodeValue() { return inner(this).value; },
    nodeType() { return inner(this).type; },
    parentNode() { return inner(this).parent; },
    childNodes() { return inner(this).children; },
    firstChild() { return inner(this).children[0] || null; },
    lastChild() { const c = inner(this).children; return c[c.length - 1] || null; },
    previousSibling() { return sibling(this, -1); },
    nextSibling() { return sibling(this, 1); },
    attributes() { return inner(this).attributes; },
  });
  getters(Document, {
    xmlVersion() { return inner(this).version; },
    xmlEncoding() { return inner(this).encoding; },
    xmlStandalone() { return inner(this).standalone; },
    documentElement() { return inner(this).root; },
  });
  getters(Element, { tagName() { return inner(this).name; } });
  getters(Attr, {
    name() { return inner(this).name; },
    value() { return inner(this).value; },
    ownerElement() { return inner(this).owner; },
  });
  getters(CharacterData, {
    data() { return inner(this).value; },
    length() { return inner(this).value.length; },
  });
  getters(Text, {
    isElementContentWhitespace() { return /^\s*$/.test(inner(this).value); },
    wholeText() { return inner(this).value; },
  });
  const node = (cls, fields) => {
    const n = Object.create(cls.prototype);
    nodes.set(n, Object.assign({ value: null, parent: null, index: 0,
      children: NO_NODES, attributes: null }, fields));
    return n;
  };
  // A tree as helper.document gives it.
  const build = (tree, parent, index) => {
    if (tree[0] === TEXT || tree[0] === CDATA) {
      const cdata = tree[0] === CDATA;
      return node(cdata ? CDATASection : Text, { type: tree[0],
        name: cdata ? '#cdata-section' : '#text', value: tree[1], parent, index });
    }
    const element = node(Element, { type: ELEMENT, name: tree[1], parent, index });
    const n = inner(element);
    n.attributes = Object.freeze(tree[2].map(([name, value]) =>
      node(Attr, { type: ATTRIBUTE, name, value, owner: element })));
    n.children = Object.freeze(tree[3].map((child, i) => build(child, element, i)));
    return element;
  };
  const documentOf = (json) => {
    const tree = JSON.parse(json);
    return node(Document, { type: DOCUMENT, name: '#document', version: tree.version,
      encoding: tree.encoding, standalone: tree.standalone, root: build(tree.root, null, 0) });
  };

  class XMLHttpRequest {
    constructor() {
      state.set(this, { ready: UNSENT, type: "", reset: true });
      this.onreadystatechange = null;
      this.onload = null;
      this.onerror = null;
      this.onloadend = null;
    }
    get readyState() { return state.get(this).ready; }
    get responseType() { return state.get(this).type; }
    set responseType(v) { state.get(this).type = String(v); }
    get status() {
      const s = state.get(this);
      if (s.ready < HEADERS_RECEIVED) throw invalid();
      return s.reply ? s.reply.status : 0;
    }
    get statusText() {
      const s = state.get(this);
      if (s.ready < HEADERS_RECEIVED) throw invalid();
      return s.reply ? s.reply.statusText : "";
    }
    get responseText() {
      const s = state.get(this);
      return s.ready === DONE && s.reply ? s.reply.text : "";
    }
    get responseURL() {
      const s = state.get(this);
      return s.ready === DONE && s.reply ? s.url : "";
    }
    get responseXML() {
      const s = state.get(this);
      if (s.ready !== DONE || !s.reply) return null;
      if (s.doc === undefined) {
        const json = helper.document(s.reply.text, this.getResponseHeader("Content-Type"));
        s.doc = json === null ? null : documentOf(json);
      }
      return s.doc;
    }
    get response() {
      const s = state.get(this);
      if (s.type === "" || s.type === "text") return this.responseText;
      if (s.type === "document") return this.responseXML;
      if (s.ready !== DONE || !s.reply) return null;
      if (s.type === "json") {
        try { return JSON.parse(s.reply.text); } catch (e) { return null; }
      }
      if (s.type === "arraybuffer") {
        const raw = s.reply.bytes || "";
        const buffer = new ArrayBuffer(raw.length);
        const view = new Uint8Array(buffer);
        for (let i = 0; i < raw.length; i++) view[i] = raw.charCodeAt(i);
        return buffer;
      }
      return null;
    }
    open(method, url, async) {
      method = String(method).toUpperCase();
      if (!METHODS.includes(method)) throw fail(12, "Unsupported HTTP method type");
      const s = state.get(this);
      if (s.id !== undefined) helper.forget(s.id);
      state.set(this, {
        ready: OPENED, type: s.type, method: method, url: String(url),
        wait: arguments.length >= 3 && !async, headers: [], reply: null,
      });
      this.#changed();
    }
    setRequestHeader(name, value) {
      const s = state.get(this);
      if (s.ready !== OPENED || s.sent) throw invalid();
      name = String(name);
      const lower = name.toLowerCase();
      if (FORBIDDEN.includes(lower) || lower.startsWith("proxy-") || lower.startsWith("sec-"))
        return;
      const same = s.headers.find((h) => h[0].toLowerCase() === lower);
      if (same) same[1] += "," + String(value);
      else s.headers.push([name, String(value)]);
    }
    send(body) {
      const s = state.get(this);
      if (s.ready !== OPENED || s.sent) throw invalid();
      if (!remote(s.url)) {
        helper.refuseLocal(s.method);
        s.sent = true;
        return;
      }
      let bytes = false;
      if (s.method === "GET" || s.method === "HEAD" || body === undefined || body === null) {
        body = undefined;
      } else if (body instanceof ArrayBuffer || ArrayBuffer.isView(body)) {
        const view = body instanceof ArrayBuffer ? new Uint8Array(body)
          : new Uint8Array(body.buffer, body.byteOffset, body.byteLength);
        let raw = "";
        for (let i = 0; i < view.length; i++) raw += String.fromCharCode(view[i]);
        body = raw;
        bytes = true;
      } else {
        body = String(body);
      }
      const headers = s.headers.map(([n, v]) => {
        // Text goes as UTF-8, and Qt says so.
        if (body !== undefined && !bytes && n.toLowerCase() === "content-type" && !/charset=/i.test(v))
          v += ";charset=UTF-8";
        return n + ": " + v;
      }).join("\n");
      s.sent = true;
      const id = helper.send(s.method, s.url, headers, body, bytes, s.wait);
      s.id = id;
      if (s.wait) {
        this.#arrive(id);
        return;
      }
      waiting.set(id, () => this.#arrive(id));
    }
    abort() {
      const s = state.get(this);
      if (s.id !== undefined) {
        waiting.delete(s.id);
        helper.forget(s.id);
        s.id = undefined;
      }
      // Only a request on its way hears that it ended.
      const told = s.sent && s.ready !== DONE;
      s.sent = false;
      s.reply = null;
      if (told) {
        s.ready = DONE;
        this.#changed();
      }
      s.ready = UNSENT;
    }
    getResponseHeader(name) {
      if (arguments.length !== 1) throw fail(12, "Incorrect argument count");
      const s = state.get(this);
      if (s.ready < HEADERS_RECEIVED) throw invalid();
      if (!s.reply) return "";
      const lower = String(name).toLowerCase();
      const found = s.reply.list.filter((h) => h[0] === lower).map((h) => h[1]);
      return found.length ? found.join(", ") : "";
    }
    getAllResponseHeaders() {
      if (arguments.length !== 0) throw fail(12, "Incorrect argument count");
      const s = state.get(this);
      if (s.ready < HEADERS_RECEIVED) throw invalid();
      return s.reply ? s.reply.headers : "";
    }
    #arrive(id) {
      const s = state.get(this);
      if (s.id !== id) return;
      const wantBytes = s.type === "arraybuffer";
      const reply = helper.take(id, wantBytes);
      s.id = undefined;
      if (reply.failed) {
        s.ready = DONE;
        this.#changed();
        this.#call("onerror");
        this.#call("onloadend");
        return;
      }
      reply.list = reply.headers === "" ? [] : reply.headers.split("\r\n").map((line) => {
        const at = line.indexOf(": ");
        return [line.slice(0, at), line.slice(at + 2)];
      });
      s.reply = reply;
      if (s.wait) {
        s.ready = DONE;
        return;
      }
      for (const ready of [HEADERS_RECEIVED, LOADING, DONE]) {
        s.ready = ready;
        this.#changed();
        if (state.get(this) !== s) return;
      }
      this.#call("onload");
      this.#call("onloadend");
    }
    #changed() { this.#call("onreadystatechange"); }
    #call(name) {
      const f = this[name];
      if (typeof f === "function") f.call(this);
    }
  }
  for (const [name, value] of Object.entries({ UNSENT, OPENED, HEADERS_RECEIVED, LOADING, DONE })) {
    Object.defineProperty(XMLHttpRequest, name, { value, enumerable: true });
    Object.defineProperty(XMLHttpRequest.prototype, name, { value, enumerable: true });
  }
  Object.defineProperty(globalThis, "XMLHttpRequest",
    { value: XMLHttpRequest, writable: true, configurable: true });
})"#;
