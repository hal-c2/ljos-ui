//! What a script has of the web's: `fetch` and the classes around it, with
//! the types lib.dom gives them, in the subset docs/DIALECT.md §4.19 lists;
//! and ECMAScript's `DisposableStack` and `AsyncDisposableStack` (§4.22),
//! with lib.esnext.disposable's.
//!
//! DECISION: a stack's `use` and `adopt` are generic, giving what they are
//! given; the checker types them at the call (`Checker::stack_hold`), and
//! the signatures here are only what the rest of a call is held to.
//!
//! DECISION: `Headers.entries()` is a readonly array of `[name, value]`
//! pairs, each a readonly array of two strings: the subset has no tuples
//! or iterators, and `for (const [name, value] of headers.entries())`
//! reads the same. `tsc` sees lib.dom's iterator.
//! DECISION: `Headers.get` gives `string | null`, as lib.dom says: a
//! script compares it with `null`, as it does `typeof v === "object"`.
//! DECISION: an init's `headers` is a `Headers`, an array of pairs or an
//! object; the object's values are not checked to be strings, as the
//! subset has no index signature, and a value that is not one becomes its
//! text, as WHATWG has it.

use super::ty::{Arena, Field, Global, Kind, Method, Param, Sig, Ty, Web};

/// The globals of §4.19, by name.
pub(super) fn global(name: &str) -> Option<Global> {
    if name == "fetch" {
        return Some(Global::Fetch);
    }
    Web::named(name).map(Global::Class)
}

fn param(name: &str, ty: Ty) -> Param {
    Param {
        name: name.into(),
        ty,
        optional: false,
        written: None,
    }
}

fn optional(name: &str, ty: Ty) -> Param {
    Param {
        optional: true,
        ..param(name, ty)
    }
}

fn sig(params: Vec<Param>, ret: Ty) -> Sig {
    Sig {
        params: params.into_boxed_slice(),
        rest: None,
        ret,
        guard: None,
    }
}

fn web(arena: &mut Arena, web: Web) -> Ty {
    arena.intern(Kind::Web(web))
}

fn promise(arena: &mut Arena, value: Ty) -> Ty {
    arena.intern(Kind::Promise(value))
}

fn field(name: &str, ty: Ty) -> Field {
    Field {
        name: name.into(),
        ty,
        optional: true,
        readonly: false,
    }
}

/// What a request's headers may be given as.
fn headers_init(arena: &mut Arena) -> Ty {
    let headers = web(arena, Web::Headers);
    let pair = arena.array(Ty::STRING, true);
    let pairs = arena.array(pair, true);
    arena.union([headers, pairs, Ty::OBJECT])
}

/// What a body may be.
fn body(arena: &mut Arena) -> Ty {
    let buffer = web(arena, Web::ArrayBuffer);
    let bytes = web(arena, Web::Uint8Array);
    arena.union([Ty::STRING, buffer, bytes])
}

/// lib.dom's `RequestInit`, in the subset: no `mode`, `credentials`,
/// `cache` or the rest, which `tsc` rejects too.
fn request_init(arena: &mut Arena) -> Ty {
    let headers = headers_init(arena);
    let body = body(arena);
    let signal = web(arena, Web::AbortSignal);
    arena.intern(Kind::Record(Box::new([
        field("method", Ty::STRING),
        field("headers", headers),
        field("body", body),
        field("signal", signal),
    ])))
}

/// What calling `fetch` takes and gives.
pub(super) fn fetch(arena: &mut Arena) -> Sig {
    let request = web(arena, Web::Request);
    let input = arena.or(Ty::STRING, request);
    let init = request_init(arena);
    let reply = web(arena, Web::Response);
    let ret = promise(arena, reply);
    sig(vec![param("input", input), optional("init", init)], ret)
}

/// What `new Class(…)` takes, for the classes a script makes; None for
/// those only `fetch` and the platform make.
pub(super) fn construct(arena: &mut Arena, class: Web) -> Option<Sig> {
    let made = web(arena, class);
    Some(match class {
        Web::Request => {
            let init = request_init(arena);
            sig(
                vec![param("input", Ty::STRING), optional("init", init)],
                made,
            )
        }
        Web::Headers => {
            let init = headers_init(arena);
            sig(vec![optional("init", init)], made)
        }
        Web::AbortController | Web::DisposableStack | Web::AsyncDisposableStack => {
            sig(vec![], made)
        }
        Web::Uint8Array => {
            let numbers = arena.array(Ty::NUMBER, true);
            let buffer = web(arena, Web::ArrayBuffer);
            let from = arena.union([Ty::NUMBER, numbers, buffer]);
            sig(vec![param("from", from)], made)
        }
        Web::Response | Web::AbortSignal | Web::DOMException | Web::ArrayBuffer => return None,
    })
}

/// What a class has of its own: `AbortSignal.timeout`, `Response.error`.
pub(super) fn static_member(arena: &mut Arena, class: Web, name: &str) -> Option<Ty> {
    let found = match (class, name) {
        (Web::AbortSignal, "timeout") => "timeout",
        (Web::AbortSignal, "any") => "any",
        (Web::AbortSignal, "abort") => "abort",
        (Web::Response, "error") => "error",
        _ => return None,
    };
    Some(arena.intern(Kind::Method(
        Ty::VOID,
        Method::Static(Global::Class(class), found),
    )))
}

/// What a class's own function takes and gives.
pub(super) fn static_sig(arena: &mut Arena, class: Web, name: &str) -> Option<Sig> {
    let signal = web(arena, Web::AbortSignal);
    Some(match (class, name) {
        (Web::AbortSignal, "timeout") => sig(vec![param("milliseconds", Ty::NUMBER)], signal),
        (Web::AbortSignal, "any") => {
            let signals = arena.array(signal, true);
            sig(vec![param("signals", signals)], signal)
        }
        (Web::AbortSignal, "abort") => sig(vec![optional("reason", Ty::UNKNOWN)], signal),
        (Web::Response, "error") => {
            let reply = web(arena, Web::Response);
            sig(vec![], reply)
        }
        _ => return None,
    })
}

/// What `value.name` is on an object of a class of the web's.
pub(super) fn member(arena: &mut Arena, receiver: Ty, class: Web, name: &str) -> Option<Ty> {
    let property = match (class, name) {
        (Web::Response, "ok" | "redirected" | "bodyUsed") => Some(Ty::BOOLEAN),
        (Web::Response, "status") => Some(Ty::NUMBER),
        (Web::Response, "statusText" | "url") => Some(Ty::STRING),
        (Web::Response | Web::Request, "headers") => Some(web(arena, Web::Headers)),
        (Web::Request, "url" | "method") => Some(Ty::STRING),
        (Web::Request, "signal") | (Web::AbortController, "signal") => {
            Some(web(arena, Web::AbortSignal))
        }
        (Web::AbortSignal, "aborted") => Some(Ty::BOOLEAN),
        (Web::AbortSignal, "reason") => Some(Ty::UNKNOWN),
        (Web::AbortSignal, "onabort") => {
            let handler = arena.function(sig(vec![], Ty::VOID));
            Some(arena.or(handler, Ty::UNDEFINED))
        }
        (Web::DOMException, "name" | "message") => Some(Ty::STRING),
        (Web::DOMException, "code") => Some(Ty::NUMBER),
        (Web::ArrayBuffer | Web::Uint8Array, "byteLength") => Some(Ty::NUMBER),
        (Web::Uint8Array, "length") => Some(Ty::NUMBER),
        (Web::DisposableStack | Web::AsyncDisposableStack, "disposed") => Some(Ty::BOOLEAN),
        _ => None,
    };
    if property.is_some() {
        return property;
    }
    let found: &'static str = match (class, name) {
        (Web::Response, "json") => "json",
        (Web::Response, "text") => "text",
        (Web::Response, "arrayBuffer") => "arrayBuffer",
        (Web::Headers, "get") => "get",
        (Web::Headers, "has") => "has",
        (Web::Headers, "set") => "set",
        (Web::Headers, "append") => "append",
        (Web::Headers, "delete") => "delete",
        (Web::Headers, "forEach") => "forEach",
        (Web::Headers, "entries") => "entries",
        (Web::AbortController, "abort") => "abort",
        (Web::AbortSignal, "throwIfAborted") => "throwIfAborted",
        (Web::DisposableStack | Web::AsyncDisposableStack, "use") => "use",
        (Web::DisposableStack | Web::AsyncDisposableStack, "adopt") => "adopt",
        (Web::DisposableStack | Web::AsyncDisposableStack, "defer") => "defer",
        (Web::DisposableStack | Web::AsyncDisposableStack, "move") => "move",
        (Web::DisposableStack, "dispose") => "dispose",
        (Web::AsyncDisposableStack, "disposeAsync") => "disposeAsync",
        _ => return None,
    };
    Some(arena.intern(Kind::Method(receiver, Method::Web(class, found))))
}

/// What a method of an object of a class of the web's takes and gives.
pub(super) fn method_sig(arena: &mut Arena, class: Web, name: &str) -> Option<Sig> {
    let name_param = || param("name", Ty::STRING);
    Some(match (class, name) {
        (Web::Response, "json") => {
            let ret = promise(arena, Ty::UNKNOWN);
            sig(vec![], ret)
        }
        (Web::Response, "text") => {
            let ret = promise(arena, Ty::STRING);
            sig(vec![], ret)
        }
        (Web::Response, "arrayBuffer") => {
            let buffer = web(arena, Web::ArrayBuffer);
            let ret = promise(arena, buffer);
            sig(vec![], ret)
        }
        (Web::Headers, "get") => {
            let ret = arena.or(Ty::STRING, Ty::NULL);
            sig(vec![name_param()], ret)
        }
        (Web::Headers, "has") => sig(vec![name_param()], Ty::BOOLEAN),
        (Web::Headers, "set" | "append") => {
            sig(vec![name_param(), param("value", Ty::STRING)], Ty::VOID)
        }
        (Web::Headers, "delete") => sig(vec![name_param()], Ty::VOID),
        (Web::Headers, "forEach") => {
            let each = arena.function(sig(
                vec![param("value", Ty::STRING), optional("name", Ty::STRING)],
                Ty::VOID,
            ));
            sig(vec![param("callback", each)], Ty::VOID)
        }
        (Web::Headers, "entries") => {
            let pair = arena.array(Ty::STRING, true);
            let pairs = arena.array(pair, true);
            sig(vec![], pairs)
        }
        (Web::AbortController, "abort") => sig(vec![optional("reason", Ty::UNKNOWN)], Ty::VOID),
        (Web::AbortSignal, "throwIfAborted") => sig(vec![], Ty::VOID),
        (Web::DisposableStack | Web::AsyncDisposableStack, "use") => {
            sig(vec![param("value", Ty::UNKNOWN)], Ty::UNKNOWN)
        }
        (Web::DisposableStack | Web::AsyncDisposableStack, "adopt") => {
            let on_dispose = on_dispose(arena, class, Ty::UNKNOWN);
            sig(
                vec![param("value", Ty::UNKNOWN), param("onDispose", on_dispose)],
                Ty::UNKNOWN,
            )
        }
        (Web::DisposableStack | Web::AsyncDisposableStack, "defer") => {
            let on_dispose = on_dispose(arena, class, Ty::NEVER);
            sig(vec![param("onDispose", on_dispose)], Ty::VOID)
        }
        (Web::DisposableStack | Web::AsyncDisposableStack, "move") => {
            let made = web(arena, class);
            sig(vec![], made)
        }
        (Web::DisposableStack, "dispose") => sig(vec![], Ty::VOID),
        (Web::AsyncDisposableStack, "disposeAsync") => {
            let done = promise(arena, Ty::VOID);
            sig(vec![], done)
        }
        _ => return None,
    })
}

/// What a stack calls when it is disposed of: given `value` (none for
/// `NEVER`, as `defer`'s is), and for an asynchronous stack it may give a
/// promise it awaits.
pub(super) fn on_dispose(arena: &mut Arena, class: Web, value: Ty) -> Ty {
    let ret = if class == Web::AsyncDisposableStack {
        let done = promise(arena, Ty::VOID);
        arena.or(Ty::VOID, done)
    } else {
        Ty::VOID
    };
    let params = if value == Ty::NEVER {
        vec![]
    } else {
        vec![param("value", value)]
    };
    arena.function(sig(params, ret))
}

/// What a promise of `value` has: `then`, `catch` and `finally`, whose
/// types hang on their callbacks' (the checker's `promise_call`).
pub(super) fn promise_member(arena: &mut Arena, receiver: Ty, name: &str) -> Option<Ty> {
    let found = ["then", "catch", "finally"]
        .into_iter()
        .find(|known| *known == name)?;
    Some(arena.intern(Kind::Method(receiver, Method::Promise(found))))
}

/// What awaiting a value of `ty` gives: a promise's value, or the value.
pub(super) fn awaited(arena: &mut Arena, ty: Ty) -> Ty {
    let parts: Vec<Ty> = arena
        .members(ty)
        .into_iter()
        .map(|part| match *arena.kind(part) {
            Kind::Promise(value) => awaited(arena, value),
            _ => part,
        })
        .collect();
    arena.union(parts)
}
