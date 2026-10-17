//! What a script makes while the program runs: `Qt.createComponent`,
//! `Qt.createQmlObject`, `Qt.resolvedUrl`, and the initial properties of
//! `createObject`.
//!
//! A relative address is read from the file whose script asks, which a
//! global function cannot know. A module that names one of these functions
//! is therefore given a `Qt` of its own by the compiler, made here over the
//! global one once the module is loaded ([`AT_EXPORT`]).
//!
//! `Qt.createQmlObject` takes QML as text. A shipped program has no
//! compiler (it runs compiled files only), so the text goes to whoever the
//! host installed with [`Engine::set_qml_text_compiler`]: a development
//! host. Without one the call is an error the script can catch.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use ljos_ir::CompiledFile;
use ljos_js::{HostError, HostFn, JsFault, JsRef, JsValue};

use super::{Instance, Template};
use crate::WriteKind;
use crate::atom::Atom;
use crate::engine::{Engine, Free};
use crate::error::EngineError;
use crate::ids::{ContextId, ObjectId, PropIndex};
use crate::js::{JsState, obj_to_u64, u64_to_obj};
use crate::loader::{LoadedFile, join_url};
use crate::meta::MetaRef;
use crate::object::QmlType;
use crate::qtqml::Component;
use crate::types::{TypeTag, Url};
use crate::value::Value;

/// The function of a compiled module the runtime calls with the module's
/// address, once, before any other. The compiler writes it.
pub(crate) const AT_EXPORT: &str = "__ljos_at";

/// The name Qt gives text that was handed over without one.
const UNNAMED: &str = "inline";

/// QML a script holds as text, for [`QmlTextCompiler`].
#[derive(Debug, Clone, Copy)]
pub struct QmlText<'a> {
    /// The key the text is compiled under: the types it names are looked
    /// for as a file of this key would find them.
    pub key: &'a str,
    pub text: &'a str,
}

/// What was wrong with a text, and where in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QmlTextError {
    pub line: u32,
    pub column: u32,
    pub message: String,
}

/// Compiles the text of `Qt.createQmlObject`. A development host installs
/// one ([`Engine::set_qml_text_compiler`]); a shipped program has none.
pub type QmlTextCompiler = Rc<dyn Fn(QmlText<'_>) -> Result<CompiledFile, Vec<QmlTextError>>>;

#[derive(Default)]
struct Dynamic {
    compiler: RefCell<Option<QmlTextCompiler>>,
    /// Texts made so far: each is a module of its own.
    texts: Cell<u64>,
    /// While a text is made: the properties it names that no type has.
    refused: RefCell<Option<Vec<String>>>,
}

/// The component instance whose script is running: its context, and the
/// object the bare names of its file are properties of.
#[derive(Clone, Copy)]
pub(crate) struct Calling {
    context: ContextId,
    root: ObjectId,
}

/// The live object a script handed over, if it is one.
fn object(e: &Engine, a: Option<&JsValue>) -> Option<ObjectId> {
    match a {
        Some(JsValue::Object(o)) => Some(u64_to_obj(*o)).filter(|o| e.is_alive(*o)),
        _ => None,
    }
}

/// Whether Qt takes `v` as the initial value of a property of type `ty`.
/// It converts less than an assignment in a script does: a text that is no
/// number is not 0 (Qt 6.11.2, by running qml6).
fn initial_fits(ty: &TypeTag, v: &Value) -> bool {
    match (ty, v) {
        (TypeTag::Var | TypeTag::Data, _) => true,
        (_, Value::Undefined) => false,
        (TypeTag::Int | TypeTag::Real, Value::String(s)) => s.trim().parse::<f64>().is_ok(),
        (TypeTag::Int | TypeTag::Real, v) => {
            matches!(
                v,
                Value::Bool(_) | Value::Int(_) | Value::Real(_) | Value::Enum(_)
            )
        }
        (TypeTag::Url, v) => matches!(v, Value::String(_) | Value::Url(_)),
        (TypeTag::Object(_), v) => matches!(v, Value::Object(_) | Value::Null),
        _ => true,
    }
}

/// Puts back who was calling before.
pub(crate) struct CallingGuard {
    js: Option<Rc<JsState>>,
    prev: Option<Calling>,
}

impl Drop for CallingGuard {
    fn drop(&mut self) {
        if let Some(js) = &self.js {
            js.calling.set(self.prev);
        }
    }
}

/// Why a text did not become an object.
enum TextFault {
    /// The call itself was wrong.
    Call(String),
    /// The text was: (address, error).
    Text(Vec<(String, QmlTextError)>),
}

impl Engine {
    /// Gives `Qt.createQmlObject` its compiler, or takes it away. The
    /// compiler is given the text and the key to compile it under, and is
    /// to compile it against the files the program was compiled from.
    pub fn set_qml_text_compiler(&self, compiler: Option<QmlTextCompiler>) {
        *self.extension::<Dynamic>().compiler.borrow_mut() = compiler;
    }

    /// A script of `inst` runs until the guard is dropped.
    pub(crate) fn calling(&self, inst: &Instance) -> CallingGuard {
        let Ok(js) = self.js() else {
            return CallingGuard {
                js: None,
                prev: None,
            };
        };
        // A delegate or a `Component { }` is written in a file: the names
        // of the file are those of the instance it was made inside.
        let mut file = inst;
        while let Some(outer) = file.outer.as_deref() {
            file = outer;
        }
        let prev = js
            .calling
            .replace(file.objects.first().map(|&root| Calling {
                context: inst.context,
                root,
            }));
        CallingGuard { js: Some(js), prev }
    }

    /// The initial properties of `createObject` and `incubateObject`. What
    /// cannot be set is a warning in Qt's words and the others are still
    /// set; a value made with `Qt.binding` is a binding, except on a member
    /// of a value unless `follow`: that is given the value it gives now.
    /// `url` is the component's.
    pub(crate) fn initial_props(
        &self,
        url: &Atom,
        root: ObjectId,
        props: &[(String, Value)],
        follow: bool,
    ) {
        for (name, v) in props {
            let Some((obj, p, member)) = self.initial_target(root, name) else {
                let ty = self
                    .meta_of(root)
                    .map(|m| m.name.to_string())
                    .unwrap_or_default();
                self.warn(format!(
                    "{url}: Setting initial properties failed: {ty} does not have a property called {name}"
                ));
                continue;
            };
            let set = match member {
                Some(member) => self.initial_member(obj, p, &member, v, follow),
                None => {
                    let fits = self
                        .meta_of(obj)
                        .and_then(|m| m.property(p).map(|i| initial_fits(&i.ty, v)))
                        .unwrap_or(true);
                    match self.initial_binding(obj, p, v) {
                        Some(bound) => bound.is_ok(),
                        None => {
                            fits && self
                                .write_internal(obj, p, v.clone(), WriteKind::Imperative)
                                .is_ok()
                        }
                    }
                }
            };
            if !set {
                self.warn(format!("{url}: Could not set initial property {name}"));
            }
        }
    }

    /// Where the name of an initial property leads, as it would in the
    /// declaration: a property of the new object, of a group or an object on
    /// the way (`border.width`), of what a type attaches to it
    /// (`Accessible.name`), or a member of a value (`font.pixelSize`), whose
    /// name is then given too.
    fn initial_target(
        &self,
        root: ObjectId,
        name: &str,
    ) -> Option<(ObjectId, PropIndex, Option<String>)> {
        if let Ok(p) = self.property_index(root, name) {
            return Some((root, p, None));
        }
        let mut path: Vec<String> = name.split('.').map(str::to_owned).collect();
        if path.len() < 2 {
            return None;
        }
        let mut obj = root;
        if path[0].starts_with(|c: char| c.is_ascii_uppercase()) {
            // The component's imports are not at hand here; the type is
            // looked for in every module, as a script's `Qt` does.
            obj = self.attached_by_name(root, "", &path[0])?;
            path.remove(0);
        }
        let (leaf, through) = path.split_last()?;
        if !through.is_empty()
            && let Some((t, _)) = self.value_target(obj, through)
        {
            return Some((t.obj, t.prop, Some(leaf.clone())));
        }
        let t = self.member_target(obj, through, leaf).ok()?;
        Some((t.obj, t.prop, None))
    }

    /// Sets the member `member` of the value `obj`'s `prop` holds, or binds
    /// it when `v` is what `Qt.binding` gave and `follow`. Another script
    /// object is not a value of a member.
    fn initial_member(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        member: &str,
        v: &Value,
        follow: bool,
    ) -> bool {
        let v = match self.binding_function(obj, v) {
            Some(f) if follow => {
                let to = self.redirect(obj, prop);
                return self
                    .bind_member(
                        (to.0, to.1, Atom::from(member)),
                        Box::new(f),
                        false,
                        Free::Never,
                    )
                    .is_ok();
            }
            Some(mut f) => match f(self) {
                Some(v) => v,
                None => return false,
            },
            None if matches!(v, Value::Js(_)) => return false,
            None => v.clone(),
        };
        let v = &v;
        let Some(ty) = self
            .meta_of(obj)
            .and_then(|m| m.property(prop).map(|p| p.ty.clone()))
        else {
            return false;
        };
        let whole = self.untrack(|| self.read(obj, prop));
        whole.with_member(&ty, member, v).is_some_and(|whole| {
            self.write_internal(obj, prop, whole, WriteKind::Imperative)
                .is_ok()
        })
    }

    /// An object declaration names a property its type does not have.
    ///
    /// Qt refuses the whole document. A compiled file is made without the
    /// property and says so; a text is refused as Qt refuses it, since the
    /// script that made it is there to be told.
    pub(crate) fn not_assignable(&self, message: String) {
        let state = self.extension::<Dynamic>();
        let kept = match &mut *state.refused.borrow_mut() {
            Some(list) => {
                list.push(message);
                return;
            }
            None => message,
        };
        self.warn(kept);
    }

    /// Whether what is being made may be finished ([`Self::not_assignable`]).
    pub(crate) fn check_assignable(&self) -> Result<(), EngineError> {
        let state = self.extension::<Dynamic>();
        match state.refused.borrow().as_ref().and_then(|l| l.first()) {
            Some(first) => Err(EngineError::Load(first.clone())),
            None => Ok(()),
        }
    }

    /// Binds `prop` when `v` is what `Qt.binding(f)` gave; `None` for any
    /// other value.
    fn initial_binding(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        v: &Value,
    ) -> Option<Result<(), EngineError>> {
        let f = self.qt_binding(v)?;
        let (target, target_prop) = self.redirect(obj, prop);
        let hint = self
            .meta_of(target)
            .and_then(|m| m.property(target_prop).map(|i| i.ty.clone()))
            .unwrap_or(TypeTag::Var);
        let this = JsValue::Object(obj_to_u64(target));
        Some(self.bind_fallible_checked(target, target_prop, move |e| {
            e.run_js("Qt.binding", |s| {
                let out = s.call_ref(&f, &this, &[])?;
                Ok(e.js_to_value(s, out, &hint))
            })
        }))
    }

    /// The function `Qt.binding(f)` was given, when `v` is what it gave.
    fn qt_binding(&self, v: &Value) -> Option<JsRef> {
        let Value::Js(h) = v else {
            return None;
        };
        let r = h.0.clone();
        self.run_js("Qt.binding", |s| {
            Ok(match s.get_field(&r, "__ljosBinding")? {
                JsValue::Handle(f) if s.is_function(&f) => Some(f),
                _ => None,
            })
        })?
    }

    /// What computes a member's value when `v` is what `Qt.binding(f)`
    /// gave: `f`, called on `this` as a binding of a property of it is.
    fn binding_function(
        &self,
        this: ObjectId,
        v: &Value,
    ) -> Option<impl FnMut(&Engine) -> Option<Value> + 'static> {
        let f = self.qt_binding(v)?;
        let this = JsValue::Object(obj_to_u64(this));
        Some(move |e: &Engine| {
            e.run_js("Qt.binding", |s| {
                let out = s.call_ref(&f, &this, &[])?;
                Ok(e.js_to_value(s, out, &TypeTag::Var))
            })
        })
    }

    /// `Qt.createComponent(url)` from a script of the file at `base`: the
    /// component of the file, or one that says why there is none.
    fn script_component(
        &self,
        base: &str,
        url: &str,
        owner: Option<ObjectId>,
    ) -> Result<ObjectId, EngineError> {
        let full = join_url(base, url);
        let error = match self.file_template(&full) {
            Ok(template) => return self.make_component_object(owner, template),
            // The loader's words for a file its source does not have.
            Err(EngineError::Load(m)) if m.ends_with(": No such file") => {
                format!("{full}: No such file or directory\n")
            }
            Err(e) => format!("{e}\n"),
        };
        let c = self.create(
            MetaRef::Static(<Component as QmlType>::static_meta()),
            owner,
        )?;
        self.with_mut::<Component, _>(c, |comp| {
            comp.url = Url(Atom::new(&full));
            comp.error = Atom::new(&error);
        })?;
        self.write_internal(
            c,
            Component::STATUS.index,
            Value::Int(3),
            WriteKind::Initial,
        )?;
        Ok(c)
    }

    /// `Qt.createQmlObject(text, parent, place)` from a script of the file
    /// at `base`.
    fn text_object(
        &self,
        base: &str,
        text: &str,
        parent: ObjectId,
        place: Option<&str>,
    ) -> Result<ObjectId, TextFault> {
        let state = self.extension::<Dynamic>();
        let Some(compiler) = state.compiler.borrow().clone() else {
            return Err(TextFault::Call(
                "this program runs compiled QML and has no compiler to make an object from text"
                    .into(),
            ));
        };
        let url = join_url(base, place.filter(|p| !p.is_empty()).unwrap_or(UNNAMED));
        let key = self.url_key(&url);
        let at = |errors: Vec<QmlTextError>| {
            TextFault::Text(errors.into_iter().map(|e| (url.clone(), e)).collect())
        };
        let plain = |message: String| {
            at(vec![QmlTextError {
                line: 0,
                column: 0,
                message,
            }])
        };
        let ir = compiler(QmlText { key: &key, text }).map_err(at)?;
        if ir.version != ljos_ir::IR_VERSION {
            return Err(plain(format!(
                "compiled for IR version {}, this engine reads {}",
                ir.version,
                ljos_ir::IR_VERSION
            )));
        }
        let file = Rc::new(LoadedFile::new(&key, url.clone(), Arc::new(ir), None));
        state.texts.set(state.texts.get() + 1);
        file.text.set(state.texts.get());
        let Some(comp) = file.find_component(None) else {
            return Err(plain("the text has no object".into()));
        };
        // The text reads the names the calling script reads: the ids of
        // its file through the context, the properties of its file's root
        // through a context of their own in between.
        let calling = self
            .inner
            .js
            .borrow()
            .as_ref()
            .and_then(|js| js.calling.get())
            .filter(|c| self.is_alive(c.root))
            .or_else(|| self.written_in(parent));
        let context = self.create_context(calling.map(|c| c.context));
        self.set_context_object(context, calling.map(|c| c.root));
        let template = Template {
            file,
            comp,
            outer: None,
            context: Some(context),
        };
        let before = state.refused.replace(Some(Vec::new()));
        let made = self.create_from_template(&template, Some(parent), Vec::new());
        let refused = state.refused.replace(before).unwrap_or_default();
        match made {
            Ok(root) => {
                // The context goes when the object does.
                self.add_context(root, context);
                Ok(root)
            }
            Err(e) => {
                self.destroy_context(context);
                if !refused.is_empty() {
                    return Err(at(refused.into_iter().map(|m| located(&url, m)).collect()));
                }
                let message = match e {
                    // The message is whole already.
                    EngineError::RequiredNotSet(m) => m,
                    e => e.to_string(),
                };
                Err(at(vec![located(&url, message)]))
            }
        }
    }

    /// Where `obj` was written, for a call that comes from no script of a
    /// file (a function kept and called later): the context it was made in
    /// and the root of that component instance.
    fn written_in(&self, obj: ObjectId) -> Option<Calling> {
        let context = self.context_of(obj)?;
        let store = self.inner.store.borrow();
        let mut cur = obj;
        // Bounded: ownership has no cycles, but a walk must not hang.
        for _ in 0..4096 {
            let h = store.objects.get(cur)?;
            if h.ext
                .as_ref()
                .is_some_and(|e| e.contexts.contains(&context))
            {
                break;
            }
            match h.owner {
                Some(owner) => cur = owner,
                None => break,
            }
        }
        Some(Calling { context, root: cur })
    }

    /// Puts the makers on `Qt` in a new JS host.
    pub(crate) fn install_dynamic_globals(&self, js: &JsState) -> Result<(), EngineError> {
        let weak = Rc::downgrade(&self.inner);
        let host = |f: fn(&Engine, &[JsValue]) -> Result<JsValue, HostError>| -> HostFn {
            let weak = weak.clone();
            Rc::new(move |_scope, args| match weak.upgrade() {
                Some(inner) => f(&Engine::from_inner(inner), args),
                None => Ok(JsValue::Undefined),
            })
        };
        // The prelude below has checked how many arguments there are.
        let functions: [(&str, HostFn); 3] = [
            (
                "__ljos_createComponent",
                host(|e, a| {
                    let (Some(JsValue::String(base)), Some(JsValue::String(url))) =
                        (a.first(), a.get(1))
                    else {
                        return Ok(JsValue::Null);
                    };
                    if url.is_empty() {
                        return Ok(JsValue::Null);
                    }
                    // (url, parent) and (url, mode, parent) are both asked.
                    let owner = object(e, a.get(2)).or_else(|| object(e, a.get(3)));
                    e.script_component(base, url, owner)
                        .map(|c| JsValue::Object(obj_to_u64(c)))
                        .map_err(|e| HostError::msg(e.to_string()))
                }),
            ),
            (
                "__ljos_resolvedUrl",
                host(|_, a| {
                    Ok(match (a.first(), a.get(1)) {
                        (Some(JsValue::String(base)), Some(JsValue::String(url))) => {
                            JsValue::String(join_url(base, url))
                        }
                        (_, Some(other)) => other.clone(),
                        _ => JsValue::Undefined,
                    })
                }),
            ),
            (
                // The object; a string for a call that was wrong; a list
                // of what was wrong with the text.
                "__ljos_createQmlObject",
                host(|e, a| {
                    let (Some(JsValue::String(base)), Some(JsValue::String(text))) =
                        (a.first(), a.get(1))
                    else {
                        return Ok(JsValue::String("Invalid arguments".into()));
                    };
                    let Some(parent) = object(e, a.get(2)) else {
                        return Ok(JsValue::String("Missing parent object".into()));
                    };
                    let place = match a.get(3) {
                        Some(JsValue::String(p)) => Some(p.as_str()),
                        _ => None,
                    };
                    Ok(match e.text_object(base, text, parent, place) {
                        Ok(o) => JsValue::Object(obj_to_u64(o)),
                        Err(TextFault::Call(m)) => JsValue::String(m),
                        Err(TextFault::Text(errors)) => JsValue::Array(
                            errors
                                .into_iter()
                                .map(|(url, e)| {
                                    JsValue::Map(vec![
                                        ("lineNumber".into(), JsValue::Number(e.line.into())),
                                        ("columnNumber".into(), JsValue::Number(e.column.into())),
                                        ("fileName".into(), JsValue::String(url)),
                                        ("message".into(), JsValue::String(e.message)),
                                    ])
                                })
                                .collect(),
                        ),
                    })
                }),
            ),
        ];
        let err = |e: JsFault| EngineError::Js(e.to_string());
        for (name, f) in functions {
            js.host.set_global_function(name, f).map_err(err)?;
        }
        js.host.eval_script("dynamic", PRELUDE).map_err(err)?;
        Ok(())
    }
}

/// An error of the engine's as one of a text: `url:line:col: message` is
/// taken apart again, so that a script reads the place as numbers.
fn located(url: &str, message: String) -> QmlTextError {
    let parsed = message
        .strip_prefix(url)
        .and_then(|rest| rest.strip_prefix(':'))
        .and_then(|rest| {
            let mut parts = rest.splitn(3, ':');
            let line = parts.next()?.parse().ok()?;
            let column = parts.next()?.parse().ok()?;
            Some((line, column, parts.next()?.trim_start().to_owned()))
        });
    match parsed {
        Some((line, column, message)) => QmlTextError {
            line,
            column,
            message,
        },
        None => QmlTextError {
            line: 0,
            column: 0,
            message,
        },
    }
}

/// The JS face of the makers. The argument checks are here because their
/// messages are what a QML author sees (Qt 6.11.2, by running qml6).
const PRELUDE: &str = r##"
(function () {
  "use strict";
  const g = globalThis;
  const Qt = g.Qt;
  const component = g.__ljos_createComponent, resolved = g.__ljos_resolvedUrl;
  const fromText = g.__ljos_createQmlObject;
  // `base` is the address of the file whose script asks; none for a script
  // that belongs to no file.
  const makers = (base) => ({
    createComponent(url, mode, parent) {
      if (arguments.length < 1 || arguments.length > 3) {
        throw new Error("Qt.createComponent(): Invalid arguments");
      }
      return component(base, typeof url === "string" ? url : String(url), mode, parent);
    },
    createQmlObject(text, parent, place) {
      if (arguments.length < 2 || arguments.length > 3) {
        throw new Error("Qt.createQmlObject(): Invalid arguments");
      }
      text = String(text);
      if (text === "") return null;
      const made = fromText(base, text, parent, place === undefined ? undefined : String(place));
      if (typeof made === "string") throw new Error("Qt.createQmlObject(): " + made);
      if (Array.isArray(made)) {
        const lines = made.map((e) =>
          "\n    " + e.fileName + ":" + e.lineNumber + ":" + e.columnNumber + ": " + e.message);
        const error = new Error("Qt.createQmlObject(): failed to create object: " + lines.join(""));
        error.qmlErrors = made;
        throw error;
      }
      return made;
    },
    resolvedUrl(url) { return resolved(base, url); },
  });
  const define = (to, from) => {
    for (const name of Object.keys(from)) {
      Object.defineProperty(to, name, { value: from[name], writable: true, configurable: true });
    }
    return to;
  };
  define(Qt, makers(""));
  // The `Qt` of one module: everything the global one has, and makers
  // that know where the module's file is.
  Object.defineProperty(g, "__ljos_qt", {
    value: (url) => define(Object.create(Qt), makers(String(url))),
  });
})();
"##;
