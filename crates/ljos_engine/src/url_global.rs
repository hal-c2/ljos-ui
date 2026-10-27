//! The `URL` and `URLSearchParams` globals a script sees (WHATWG URL
//! Standard, https://url.spec.whatwg.org/#api).
//!
//! Parsing and serialising are the `url` crate's, which implements that
//! standard; the two classes are a thin JS face over three host
//! functions. Qt's own `URL` global is built on `QUrl` and differs from the
//! standard in places (the default port is kept, a relative reference is
//! not normalised); Ljós follows the standard and
//! `features/language/js-url.feature` says where the two part.

use std::rc::Rc;

use ljos_js::{HostError, HostFn, JsValue};
use url::Url;

use crate::error::EngineError;
use crate::js::JsState;

fn text(v: Option<&JsValue>) -> String {
    match v {
        Some(JsValue::String(s)) => s.clone(),
        _ => String::new(),
    }
}

/// The eleven fields JS keeps for one parsed URL, in the order the class
/// reads them: href, protocol, username, password, host, hostname, port,
/// pathname, search, hash, origin.
fn fields(u: &Url) -> JsValue {
    let non_empty = |s: Option<&str>, lead: char| match s {
        Some(s) if !s.is_empty() => format!("{lead}{s}"),
        _ => String::new(),
    };
    let host = u.host_str().unwrap_or_default();
    let with_port = match u.port() {
        Some(p) => format!("{host}:{p}"),
        None => host.to_owned(),
    };
    let origin = match u.origin() {
        url::Origin::Tuple(..) => u.origin().ascii_serialization(),
        url::Origin::Opaque(_) => "null".to_owned(),
    };
    let s = JsValue::String;
    JsValue::Array(vec![
        s(u.as_str().to_owned()),
        s(format!("{}:", u.scheme())),
        s(u.username().to_owned()),
        s(u.password().unwrap_or_default().to_owned()),
        s(with_port),
        s(host.to_owned()),
        s(u.port().map(|p| p.to_string()).unwrap_or_default()),
        s(u.path().to_owned()),
        s(non_empty(u.query(), '?')),
        s(non_empty(u.fragment(), '#')),
        s(origin),
    ])
}

fn parse(args: &[JsValue]) -> Result<JsValue, HostError> {
    let input = text(args.first());
    let parsed = match args.get(1) {
        Some(JsValue::String(base)) => Url::parse(base)
            .map_err(|_| HostError::type_error(format!("Invalid base URL: {base}")))
            .and_then(|b| {
                b.join(&input)
                    .map_err(|_| HostError::type_error(format!("Invalid URL: {input}")))
            }),
        _ => Url::parse(&input).map_err(|_| HostError::type_error(format!("Invalid URL: {input}"))),
    }?;
    Ok(fields(&parsed))
}

/// `(href, field, value)`: the URL with one part replaced, as the setter
/// of that name of the standard does; a value it refuses leaves the URL as
/// it was.
fn set(args: &[JsValue]) -> Result<JsValue, HostError> {
    let mut u =
        Url::parse(&text(args.first())).map_err(|_| HostError::type_error("Invalid URL"))?;
    let value = text(args.get(2));
    match text(args.get(1)).as_str() {
        "href" => {
            u = Url::parse(&value)
                .map_err(|_| HostError::type_error(format!("Invalid URL: {value}")))?;
        }
        "protocol" => {
            let _ = u.set_scheme(value.trim_end_matches(':'));
        }
        "username" => {
            let _ = u.set_username(&value);
        }
        "password" => {
            let _ = u.set_password((!value.is_empty()).then_some(value.as_str()));
        }
        "host" => {
            let (host, port) = match value.rsplit_once(':') {
                Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => (h, p.parse().ok()),
                _ => (value.as_str(), None),
            };
            if u.set_host(Some(host)).is_ok() && port.is_some() {
                let _ = u.set_port(port);
            }
        }
        "hostname" => {
            let _ = u.set_host(Some(&value));
        }
        "port" => {
            let _ = u.set_port(value.parse().ok());
        }
        "pathname" => u.set_path(&value),
        "search" => u.set_query((!value.is_empty()).then(|| value.trim_start_matches('?'))),
        "hash" => u.set_fragment((!value.is_empty()).then(|| value.trim_start_matches('#'))),
        _ => {}
    }
    Ok(fields(&u))
}

pub(crate) fn install(js: &JsState) -> Result<(), EngineError> {
    let host = |f: fn(&[JsValue]) -> Result<JsValue, HostError>| -> HostFn {
        Rc::new(move |_scope, args| f(args))
    };
    let functions: [(&str, HostFn); 4] = [
        ("__ljos_url_parse", host(parse)),
        ("__ljos_url_set", host(set)),
        (
            // text -> [name, value, name, value, ...]
            "__ljos_url_form_decode",
            host(|a| {
                let t = text(a.first());
                Ok(JsValue::Array(
                    url::form_urlencoded::parse(t.trim_start_matches('?').as_bytes())
                        .flat_map(|(k, v)| [JsValue::String(k.into()), JsValue::String(v.into())])
                        .collect(),
                ))
            }),
        ),
        (
            // [name, value, ...] -> text
            "__ljos_url_form_encode",
            host(|a| {
                let Some(JsValue::Array(items)) = a.first() else {
                    return Ok(JsValue::String(String::new()));
                };
                let mut out = url::form_urlencoded::Serializer::new(String::new());
                for pair in items.chunks(2) {
                    out.append_pair(&text(pair.first()), &text(pair.get(1)));
                }
                Ok(JsValue::String(out.finish()))
            }),
        ),
    ];
    for (name, f) in functions {
        js.host
            .set_global_function(name, f)
            .map_err(EngineError::js)?;
    }
    js.host
        .eval_script("url", PRELUDE)
        .map_err(EngineError::js)?;
    Ok(())
}

pub(crate) const PRELUDE: &str = r##"
(function () {
  "use strict";
  const g = globalThis;
  const parse = g.__ljos_url_parse, set = g.__ljos_url_set;
  const decode = g.__ljos_url_form_decode, encode = g.__ljos_url_form_encode;
  const NAMES = ["href", "protocol", "username", "password", "host", "hostname",
                 "port", "pathname", "search", "hash", "origin"];
  for (const n of ["__ljos_url_parse", "__ljos_url_set", "__ljos_url_form_decode",
                   "__ljos_url_form_encode"]) delete g[n];
  const state = new WeakMap();

  class URLSearchParams {
    #list = [];
    #owner = null;
    constructor(init) {
      if (init === undefined || init === null) return;
      if (typeof init === "object" && typeof init[Symbol.iterator] === "function") {
        for (const pair of init) this.#list.push(String(pair[0]), String(pair[1]));
      } else if (typeof init === "object") {
        for (const k of Object.keys(init)) this.#list.push(k, String(init[k]));
      } else {
        this.#list = decode(String(init));
      }
    }
    // Used by URL: follows the URL's query, and writes back to it.
    static _bind(params, owner, query) {
      params.#owner = owner;
      params.#list = decode(query);
    }
    static _reset(params, query) { params.#list = decode(query); }
    #changed() {
      if (this.#owner) this.#owner(encode(this.#list));
    }
    append(name, value) { this.#list.push(String(name), String(value)); this.#changed(); }
    delete(name) {
      name = String(name);
      const kept = [];
      for (let i = 0; i < this.#list.length; i += 2)
        if (this.#list[i] !== name) kept.push(this.#list[i], this.#list[i + 1]);
      this.#list = kept;
      this.#changed();
    }
    get(name) {
      name = String(name);
      for (let i = 0; i < this.#list.length; i += 2)
        if (this.#list[i] === name) return this.#list[i + 1];
      return null;
    }
    getAll(name) {
      name = String(name);
      const out = [];
      for (let i = 0; i < this.#list.length; i += 2)
        if (this.#list[i] === name) out.push(this.#list[i + 1]);
      return out;
    }
    has(name) { return this.get(name) !== null; }
    set(name, value) {
      name = String(name); value = String(value);
      const out = [];
      let done = false;
      for (let i = 0; i < this.#list.length; i += 2) {
        if (this.#list[i] !== name) { out.push(this.#list[i], this.#list[i + 1]); continue; }
        if (!done) { out.push(name, value); done = true; }
      }
      if (!done) out.push(name, value);
      this.#list = out;
      this.#changed();
    }
    sort() {
      const pairs = [];
      for (let i = 0; i < this.#list.length; i += 2) pairs.push([this.#list[i], this.#list[i + 1]]);
      pairs.sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0));
      this.#list = pairs.flat();
      this.#changed();
    }
    forEach(callback, thisArg) {
      if (typeof callback !== "function")
        throw new TypeError("URLSearchParams.forEach: callback is not a function");
      for (let i = 0; i < this.#list.length; i += 2)
        callback.call(thisArg, this.#list[i + 1], this.#list[i], this);
    }
    *entries() {
      for (let i = 0; i < this.#list.length; i += 2) yield [this.#list[i], this.#list[i + 1]];
    }
    *keys() { for (const [k] of this.entries()) yield k; }
    *values() { for (const [, v] of this.entries()) yield v; }
    [Symbol.iterator]() { return this.entries(); }
    get size() { return this.#list.length / 2; }
    toString() { return encode(this.#list); }
  }

  class URL {
    constructor(url, base) {
      if (arguments.length === 0) throw new TypeError("URL constructor: 1 argument required");
      state.set(this, { f: parse(String(url), base === undefined ? undefined : String(base)),
                        params: null });
    }
    static canParse(url, base) {
      try { new URL(url, base); return true; } catch (e) { return false; }
    }
    #fields() { return state.get(this).f; }
    get searchParams() {
      const s = state.get(this);
      if (!s.params) {
        s.params = new URLSearchParams();
        URLSearchParams._bind(s.params, (q) => {
          s.f = set(s.f[0], "search", q);
        }, s.f[8]);
      }
      return s.params;
    }
    toString() { return this.#fields()[0]; }
    toJSON() { return this.#fields()[0]; }
  }
  NAMES.forEach((name, i) => {
    const writable = name !== "origin";
    Object.defineProperty(URL.prototype, name, {
      get() { return state.get(this).f[i]; },
      set: writable ? function (v) {
        const s = state.get(this);
        s.f = set(s.f[0], name, String(v));
        if (s.params) URLSearchParams._reset(s.params, s.f[8]);
      } : undefined,
      enumerable: true,
      configurable: true,
    });
  });
  Object.defineProperty(g, "URL", { value: URL, writable: true, configurable: true });
  Object.defineProperty(g, "URLSearchParams",
                        { value: URLSearchParams, writable: true, configurable: true });
})();
"##;
