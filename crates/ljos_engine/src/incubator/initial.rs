//! The initial properties of an object being made: what native code,
//! `createObject`, `incubateObject` (and `Loader.setSource` and
//! `StackView.push`, which set them the same way) and a view's required
//! properties give it before it completes.
//!
//! [`decide`] says what becomes of one property, from what its name leads
//! to: it is set, bound, or not set, and then nothing is told, a warning
//! is, the object is withheld, or the creation stops.
//! [`Engine::apply_initial`] finds what each name leads to and carries the
//! verdict out. It does so one property at a time: a path is read on the
//! object as the properties before it left it.

use ljos_js::{JsRef, JsValue};

use super::Initial;
use crate::WriteKind;
use crate::atom::Atom;
use crate::engine::{Engine, Free};
use crate::error::EngineError;
use crate::ids::{ObjectId, PropIndex};
use crate::introspect::SourcePlace;
use crate::js::obj_to_u64;
use crate::meta::PropertyFlags;
use crate::types::TypeTag;
use crate::value::{Color, Value};

/// How the initial properties are set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode<'a> {
    /// [`Initial::Native`].
    Native,
    /// [`Initial::Created`].
    Created(Option<&'a SourcePlace>),
    /// [`Initial::Incubated`].
    Incubated(Option<&'a SourcePlace>),
    /// A view's object, given the values named like its required
    /// properties (`section` of a section heading): each is written as a
    /// model's role is, and what cannot be is passed over.
    Required,
}

impl<'a> From<&'a Initial> for Mode<'a> {
    fn from(how: &'a Initial) -> Self {
        match how {
            Initial::Native => Mode::Native,
            Initial::Created(p) => Mode::Created(p.as_deref()),
            Initial::Incubated(p) => Mode::Incubated(p.as_ref()),
        }
    }
}

/// Why the name of an initial property leads nowhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Missing {
    /// A plain name the object has no property of.
    Property,
    /// A path whose start is nothing with members: no property, a property
    /// of a type without (`count.x` of an int, a `var`), an object, or no
    /// attaching type.
    Start,
    /// A path to a group or an attached object that has no such member
    /// (`border.nosuch`, `Layout.nosuch`).
    Member,
    /// A path through properties that hold objects to a name the last
    /// object does not have (`q.zz`, `q.r.zz`).
    ObjectMember,
}

/// What [`decide`] knows of the property an initial name leads to.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Facts {
    /// The member of the value the property holds (`pixelSize` of `font`).
    pub member: Option<String>,
    /// The property is of what a type attaches (`Layout.fillWidth`).
    pub attached: bool,
    /// Its type, when its object says.
    pub ty: Option<TypeTag>,
    pub readonly: bool,
    /// The value is what `Qt.binding(f)` gave, for a property itself (a
    /// member of a value finds that out as it is set).
    pub binding: bool,
}

/// What becomes of one initial property.
#[derive(Debug, PartialEq)]
pub(crate) enum Outcome {
    /// It is set as the [`Write`] says; if it cannot be, the [`Verdict`]
    /// holds.
    Set(Write, Verdict),
    /// It is not set.
    Not(Verdict),
}

/// How an initial property is set.
#[derive(Debug, PartialEq)]
pub(crate) enum Write {
    /// Bound to the function `Qt.binding` was given.
    Bind,
    /// Given the value, converted for its type.
    Value(Value, WriteKind),
    /// Its value is given the member [`Facts::member`]; with `follow`, a
    /// `Qt.binding` binds the member rather than giving it its value now.
    Member { follow: bool },
    /// It takes nothing of the value. With `reset`, a binding it has is
    /// taken away and it is left at its type's default.
    Unconverted { reset: bool },
}

/// What a property that is not set does to the creation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// Nothing.
    Quiet,
    /// A warning; the object is made all the same.
    Tell(Told),
    /// The object is completed, then destroyed, and not given back, after
    /// any warning (Qt 6.12, `createObject`).
    Withhold(Option<Told>),
    /// The creation fails with the error it met (native code).
    Stop,
}

/// The warning a property that is not set is told with, in Qt's words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Told {
    /// `Cannot assign to non-existent property "<leaf>"`, at the call.
    NoObjectMember,
    /// `Cannot assign to non-existent property "<name>"`, at the call.
    NoProperty,
    /// `Cannot resolve property "<name>".`, by the caller's file.
    Unresolved,
    /// `Cannot assign to read-only property "<name>"`, at the call.
    ReadOnly,
    /// `Cannot assign <the value's type> to <the property's>`, at the call.
    CannotAssign,
    /// `Could not set initial property <name>`, by the component's file.
    CouldNotSet,
}

/// What became of the initial properties as a whole.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[must_use]
pub(crate) struct Settled {
    /// The object is not to be given back ([`Verdict::Withhold`]).
    pub withheld: bool,
}

/// What becomes of the initial property `v` that leads to `target`.
///
/// `createObject` passes over in silence a name that leads nowhere and a
/// member a value, group or attached object does not have, as Qt 6.12
/// does; `incubateObject` tells them. Both tell, at the place of the
/// call, a name an object on the path does not have, and `createObject`
/// then withholds the object. `createObject` sets a read-only property;
/// `incubateObject` assigns it as a script would, and is refused. A value
/// made with `Qt.binding` is a binding, except on a member of a value when
/// `createObject` sets it: that is given the value it gives now.
pub(crate) fn decide(mode: Mode<'_>, target: Result<&Facts, Missing>, v: &Value) -> Outcome {
    let script = match mode {
        Mode::Native => {
            return match target {
                Ok(f) if f.binding => Outcome::Set(Write::Bind, Verdict::Stop),
                Ok(_) => Outcome::Set(
                    Write::Value(v.clone(), WriteKind::Imperative),
                    Verdict::Stop,
                ),
                Err(_) => Outcome::Not(Verdict::Stop),
            };
        }
        Mode::Required => {
            return match target {
                Ok(_) => Outcome::Set(Write::Value(v.clone(), WriteKind::Initial), Verdict::Quiet),
                Err(_) => Outcome::Not(Verdict::Quiet),
            };
        }
        Mode::Created(_) => Script::Created,
        Mode::Incubated(_) => Script::Incubated,
    };
    let incubated = script == Script::Incubated;
    let f = match target {
        Ok(f) => f,
        Err(missing) => {
            return Outcome::Not(match (script, missing) {
                (Script::Created, Missing::ObjectMember) => {
                    Verdict::Withhold(Some(Told::NoObjectMember))
                }
                (Script::Incubated, Missing::ObjectMember) => Verdict::Tell(Told::NoObjectMember),
                (Script::Incubated, Missing::Property) => Verdict::Tell(Told::NoProperty),
                (Script::Incubated, _) => Verdict::Tell(Told::Unresolved),
                (Script::Created, Missing::Start) => Verdict::Withhold(None),
                (Script::Created, _) => Verdict::Quiet,
            });
        }
    };
    // A member a value does not have, as a member a group does not.
    if !incubated
        && let (Some(member), Some(ty)) = (&f.member, &f.ty)
        && Value::Undefined.with_member(ty, member, v).is_none()
    {
        return Outcome::Not(Verdict::Quiet);
    }
    if incubated && f.member.is_none() && f.readonly {
        return Outcome::Not(Verdict::Tell(Told::ReadOnly));
    }
    let write = match (&f.member, &f.ty) {
        (Some(_), _) => Write::Member { follow: incubated },
        (None, _) if f.binding => Write::Bind,
        (None, Some(ty)) => match initial_value(ty, v, !incubated) {
            Some(v) => Write::Value(v, WriteKind::Imperative),
            // createObject has taken the binding away before it finds it
            // cannot convert: the type's default is left.
            None => Write::Unconverted { reset: !incubated },
        },
        (None, None) => Write::Value(v.clone(), WriteKind::Imperative),
    };
    let otherwise = match script {
        Script::Incubated if f.member.is_none() && f.ty.is_some() => {
            Verdict::Tell(Told::CannotAssign)
        }
        Script::Incubated => Verdict::Tell(Told::CouldNotSet),
        // What an attached object cannot take keeps nothing back.
        Script::Created if f.attached => Verdict::Quiet,
        Script::Created => Verdict::Withhold(Some(Told::CouldNotSet)),
    };
    Outcome::Set(write, otherwise)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Script {
    Created,
    Incubated,
}

/// What Qt makes of `v` as the initial value of a property of type `ty`,
/// or `None` when it takes nothing. `createObject` converts as a value
/// from outside QML is: a text "0" is no bool, a number is rounded to an
/// int, and what is no colour an invalid one. `incubateObject` converts as
/// a script does, but a text that is no number is not 0 for an int. A list
/// of numbers is the text of those character codes (Qt 6.11.2, by running
/// qml6).
fn initial_value(ty: &TypeTag, v: &Value, created: bool) -> Option<Value> {
    let number = |s: &Atom| s.trim().parse::<f64>().ok().filter(|n| n.is_finite());
    let plain = || Some(v.clone());
    match (ty, v) {
        (TypeTag::Var | TypeTag::Data | TypeTag::Copied, _) => plain(),
        (_, Value::Undefined) => None,
        (TypeTag::Bool, Value::String(s)) if created => Some(Value::Bool(
            !(s.is_empty() || &**s == "0" || s.eq_ignore_ascii_case("false")),
        )),
        (TypeTag::Bool, Value::Real(n)) if created => Some(Value::Bool(n.round() != 0.0)),
        (TypeTag::Bool, Value::Null | Value::List(_) | Value::Map(_) | Value::Js(_)) if created => {
            None
        }
        (TypeTag::Int, Value::String(s)) if created => number(s).map(|n| Value::Real(n.round())),
        (TypeTag::Int, Value::Real(n)) if created => Some(*n)
            .filter(|n| !n.is_nan())
            .map(|n| Value::Real(n.round())),
        (
            TypeTag::Real,
            Value::Null | Value::String(_) | Value::List(_) | Value::Map(_) | Value::Js(_),
        ) if !created => Some(Value::Real(v.to_number())),
        (TypeTag::Int | TypeTag::Real, Value::String(s)) => number(s).map(|_| v.clone()),
        (TypeTag::Int | TypeTag::Real, v) => matches!(
            v,
            Value::Bool(_) | Value::Int(_) | Value::Real(_) | Value::Enum(_)
        )
        .then(|| v.clone()),
        (TypeTag::String, Value::List(items)) => Some(Value::String(char_codes(items).into())),
        (TypeTag::Color, Value::List(_)) if created => None,
        (TypeTag::Color, Value::String(_) | Value::Color(_)) => plain(),
        (TypeTag::Color, _) => Some(Value::Color(Color::INVALID)),
        (TypeTag::Url, v) => matches!(v, Value::String(_) | Value::Url(_)).then(|| v.clone()),
        (TypeTag::Object(_), v) => matches!(v, Value::Object(_) | Value::Null).then(|| v.clone()),
        _ => plain(),
    }
}

/// A list as Qt makes it a text: a character for each item, a text's first
/// and the code a number gives.
fn char_codes(items: &[Value]) -> String {
    let units: Vec<u16> = items
        .iter()
        .map(|item| match item {
            Value::String(s) => s.encode_utf16().next().unwrap_or(0),
            // ToUint16: what is no number is 0, the rest is cut to its whole part and wraps.
            item => {
                let n = item.to_number();
                if n.is_finite() {
                    n.trunc().rem_euclid(65536.0) as u16
                } else {
                    0
                }
            }
        })
        .collect();
    String::from_utf16_lossy(&units)
}

/// Where the name of an initial property leads.
struct Reached {
    obj: ObjectId,
    prop: PropIndex,
    facts: Facts,
    /// The function `Qt.binding` was given, when [`Facts::binding`].
    binding: Option<JsRef>,
}

/// Where what the initial properties could not set is told.
#[derive(Clone, Copy)]
enum At<'a> {
    /// The place of the call, `file:line`.
    Call(&'a SourcePlace),
    File(&'a str),
}

impl std::fmt::Display for At<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            At::Call(p) => write!(f, "{}:{}", p.file, p.line),
            At::File(file) => f.write_str(file),
        }
    }
}

impl Engine {
    /// The initial properties, as `how` sets them. Native code's are
    /// noted for a reload ([`Engine::reload_note_given`]).
    pub(crate) fn set_initial(
        &self,
        url: &Atom,
        root: ObjectId,
        props: &[(String, Value)],
        how: &Initial,
    ) -> Result<Settled, EngineError> {
        self.reload_note_given(root, props);
        self.apply_initial(url, root, props, how.into())
    }

    /// Sets `props` on `root` as `mode` says ([`decide`]). `url` is the
    /// component's.
    pub(crate) fn apply_initial(
        &self,
        url: &Atom,
        root: ObjectId,
        props: &[(String, Value)],
        mode: Mode<'_>,
    ) -> Result<Settled, EngineError> {
        let mut settled = Settled::default();
        for (name, v) in props {
            let reached = self.initial_reached(mode, root, name, v);
            let verdict = match decide(mode, reached.as_ref().map(|r| &r.facts).map_err(|m| *m), v)
            {
                Outcome::Not(Verdict::Stop) => {
                    // The error native code is told: the one looking the
                    // name up gives.
                    self.property_index(root, name)?;
                    continue;
                }
                Outcome::Not(verdict) => verdict,
                Outcome::Set(write, otherwise) => {
                    let Ok(r) = &reached else {
                        continue;
                    };
                    match (self.initial_write(r, write, v), otherwise) {
                        (Ok(()), _) => continue,
                        (Err(Some(e)), Verdict::Stop) => return Err(e),
                        (Err(None), Verdict::Stop) => continue,
                        (Err(_), otherwise) => otherwise,
                    }
                }
            };
            let told = match verdict {
                Verdict::Quiet | Verdict::Stop => None,
                Verdict::Tell(told) => Some(told),
                Verdict::Withhold(told) => {
                    settled.withheld = true;
                    told
                }
            };
            if let Some(told) = told {
                let ty = reached.as_ref().ok().and_then(|r| r.facts.ty.as_ref());
                self.tell_initial(mode, url, name, v, ty, told);
            }
        }
        Ok(settled)
    }

    /// What the name of an initial property leads to, as `mode` looks
    /// for it: native code and a view name properties of the root alone.
    fn initial_reached(
        &self,
        mode: Mode<'_>,
        root: ObjectId,
        name: &str,
        v: &Value,
    ) -> Result<Reached, Missing> {
        let (obj, prop, member, attached) = match mode {
            Mode::Native | Mode::Required => {
                let p = self.find_property(root, name).ok_or(Missing::Property)?;
                (root, p, None, false)
            }
            Mode::Created(_) | Mode::Incubated(_) => self.initial_target(root, name)?,
        };
        let (ty, readonly) = self
            .meta_of(obj)
            .and_then(|m| m.property(prop).map(|i| (i.ty.clone(), i.is_readonly())))
            .unzip();
        // A member of a value finds out for itself; a view's object takes
        // what it is given as it is.
        let binding = match (&member, mode) {
            (None, Mode::Native | Mode::Created(_) | Mode::Incubated(_)) => self.qt_binding(v),
            _ => None,
        };
        Ok(Reached {
            obj,
            prop,
            facts: Facts {
                member,
                attached,
                ty,
                readonly: readonly == Some(true),
                binding: binding.is_some(),
            },
            binding,
        })
    }

    /// Carries out `write`; `Err` when the property is not set, with the
    /// error when there is one.
    fn initial_write(
        &self,
        r: &Reached,
        write: Write,
        v: &Value,
    ) -> Result<(), Option<EngineError>> {
        let set = match write {
            Write::Bind => {
                let Some(f) = r.binding.clone() else {
                    return Err(None);
                };
                return self.bind_qt(r.obj, r.prop, f).map_err(Some);
            }
            Write::Value(v, kind) => {
                return self.write_internal(r.obj, r.prop, v, kind).map_err(Some);
            }
            Write::Member { follow } => match &r.facts.member {
                Some(member) => self.initial_member(r.obj, r.prop, member, v, follow),
                None => false,
            },
            Write::Unconverted { reset } => {
                if reset && self.is_bound(r.obj, r.prop) {
                    let _ = self.reset_to_default(r.obj, r.prop);
                }
                false
            }
        };
        if set { Ok(()) } else { Err(None) }
    }

    /// Tells `told` of the initial property `name`. `incubateObject` says
    /// what it could not set as the error of its call, where `createObject`
    /// speaks for the file; a path that leads nowhere is told by the
    /// caller's file alone.
    fn tell_initial(
        &self,
        mode: Mode<'_>,
        url: &str,
        name: &str,
        v: &Value,
        ty: Option<&TypeTag>,
        told: Told,
    ) {
        let (at, caller) = match mode {
            Mode::Incubated(Some(p)) => (Some(At::Call(p)), At::File(&p.file)),
            Mode::Incubated(None) => (Some(At::File(url)), At::File(url)),
            _ => (None, At::File(url)),
        };
        let at_call = at.unwrap_or(At::File(url));
        let message = match told {
            Told::NoObjectMember => {
                let leaf = name.rsplit('.').next().unwrap_or(name);
                // createObject is still in its call: the place is that of
                // the script running now (Qt 6.11 and 6.12 both tell it),
                // or, in a reload, the one its call had.
                let given = match mode {
                    Mode::Created(Some(p)) => Some(At::Call(p)),
                    _ => at,
                };
                let here = given.is_none().then(|| self.js_origin()).flatten();
                let at = given
                    .or(here.as_ref().map(At::Call))
                    .unwrap_or(At::File(url));
                format!("{at}: Error: Cannot assign to non-existent property \"{leaf}\"")
            }
            Told::NoProperty => {
                format!("{at_call}: Error: Cannot assign to non-existent property \"{name}\"")
            }
            Told::Unresolved => format!("{caller}: Cannot resolve property \"{name}\"."),
            Told::ReadOnly => {
                format!("{at_call}: TypeError: Cannot assign to read-only property \"{name}\"")
            }
            Told::CannotAssign => format!(
                "{at_call}: Error: Cannot assign {} to {}",
                self.assigned_type(v),
                ty.map(|ty| self.qt_type(ty)).unwrap_or_default()
            ),
            Told::CouldNotSet => format!("{url}: Could not set initial property {name}"),
        };
        self.warn(message);
    }

    /// The C++ type Qt's engine names a value by when it cannot assign it.
    fn assigned_type(&self, v: &Value) -> &'static str {
        match v {
            Value::Undefined => "[undefined]",
            Value::Null => "std::nullptr_t",
            Value::Bool(_) => "bool",
            Value::Int(_) | Value::Enum(_) => "int",
            // A script's number is an int while it is a whole one.
            Value::Real(n) if n.fract() == 0.0 && n.abs() <= f64::from(i32::MAX) => "int",
            Value::Real(_) => "double",
            Value::String(_) => "QString",
            Value::Color(_) => "QColor",
            Value::Point(_) => "QPointF",
            Value::IntPoint(_) => "QPoint",
            Value::Size(_) => "QSizeF",
            Value::Rect(_) => "QRectF",
            Value::Font(_) => "QFont",
            Value::Vector2d(_) => "QVector2D",
            Value::Vector3d(_) => "QVector3D",
            Value::Vector4d(_) => "QVector4D",
            Value::Quaternion(_) => "QQuaternion",
            Value::Url(_) => "QUrl",
            Value::Object(_) => "QObject*",
            Value::Js(_) if self.is_callable(v) => "JavaScript function",
            Value::List(_) | Value::Map(_) | Value::Js(_) => "QJSValue",
            Value::Custom(c) => c.ty().name(),
        }
    }

    /// Where the name of an initial property leads, as it would in the
    /// declaration: a property of the new object, of a group or an object on
    /// the way (`border.width`), of what a type attaches to it
    /// (`Accessible.name`), or a member of a value (`font.pixelSize`), whose
    /// name is then given too. Also whether the property is of what a type
    /// attaches.
    fn initial_target(
        &self,
        root: ObjectId,
        name: &str,
    ) -> Result<(ObjectId, PropIndex, Option<String>, bool), Missing> {
        if let Some(p) = self.find_property(root, name) {
            return Ok((root, p, None, false));
        }
        let mut path: Vec<String> = name.split('.').map(str::to_owned).collect();
        if path.len() < 2 {
            return Err(Missing::Property);
        }
        let mut obj = root;
        let attached = path[0].starts_with(|c: char| c.is_ascii_uppercase());
        if attached {
            // The component's imports are not at hand here; the type is
            // looked for in every module, as a script's `Qt` does.
            obj = self
                .attached_by_name(root, "", &path[0])
                .ok_or(Missing::Start)?;
            path.remove(0);
        }
        let (leaf, through) = path.split_last().ok_or(Missing::Start)?;
        if !through.is_empty()
            && let Some((t, _)) = self.value_target(obj, through)
        {
            return Ok((t.obj, t.prop, Some(leaf.clone()), attached));
        }
        match self.member_target(obj, through, leaf) {
            Ok(t) => Ok((t.obj, t.prop, None, attached)),
            Err(_) if attached || self.is_group(obj, &through[0]) => Err(Missing::Member),
            Err(_) if self.reaches_object(obj, through) => Err(Missing::ObjectMember),
            Err(_) => Err(Missing::Start),
        }
    }

    /// Whether each name of `through`, from `obj`, holds an object.
    fn reaches_object(&self, obj: ObjectId, through: &[String]) -> bool {
        through
            .iter()
            .try_fold(obj, |cur, seg| {
                self.untrack(|| self.read_by_name(cur, seg)).as_object()
            })
            .is_some()
    }

    /// Whether `name` of `obj` is a group (`border` of a `Rectangle`).
    fn is_group(&self, obj: ObjectId, name: &str) -> bool {
        self.find_property(obj, name)
            .zip(self.meta_of(obj))
            .and_then(|(p, mm)| {
                mm.property(p)
                    .map(|i| i.flags.contains(PropertyFlags::GROUP))
            })
            .unwrap_or(false)
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
        let v = &self.plain(&v);
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

    /// Binds `prop` to `f`, the function `Qt.binding(f)` was given.
    fn bind_qt(&self, obj: ObjectId, prop: PropIndex, f: JsRef) -> Result<(), EngineError> {
        let (target, target_prop) = self.redirect(obj, prop);
        let hint = self
            .meta_of(target)
            .and_then(|m| m.property(target_prop).map(|i| i.ty.clone()))
            .unwrap_or(TypeTag::Var);
        let this = JsValue::Object(obj_to_u64(target));
        self.bind_fallible_checked(target, target_prop, move |e| {
            e.run_js("Qt.binding", |s| {
                let out = s.call_ref(&f, &this, &[])?;
                Ok(e.js_to_value(s, out, &hint))
            })
        })
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place() -> SourcePlace {
        SourcePlace {
            file: "Main.qml".into(),
            line: 4,
            column: 0,
        }
    }

    fn int() -> Facts {
        Facts {
            ty: Some(TypeTag::Int),
            ..Facts::default()
        }
    }

    fn real(n: f64) -> Value {
        Value::Real(n)
    }

    #[test]
    fn native_code_sets_what_it_gives_and_stops_at_what_it_cannot() {
        assert_eq!(
            decide(Mode::Native, Ok(&int()), &real(2.5)),
            Outcome::Set(
                Write::Value(real(2.5), WriteKind::Imperative),
                Verdict::Stop
            )
        );
        let bound = Facts {
            binding: true,
            ..int()
        };
        assert_eq!(
            decide(Mode::Native, Ok(&bound), &Value::Undefined),
            Outcome::Set(Write::Bind, Verdict::Stop)
        );
        assert_eq!(
            decide(Mode::Native, Err(Missing::Property), &real(1.0)),
            Outcome::Not(Verdict::Stop)
        );
    }

    #[test]
    fn a_view_writes_its_required_properties_as_roles_and_passes_over_the_rest() {
        assert_eq!(
            decide(Mode::Required, Ok(&int()), &Value::from("a")),
            Outcome::Set(
                Write::Value(Value::from("a"), WriteKind::Initial),
                Verdict::Quiet
            )
        );
        assert_eq!(
            decide(Mode::Required, Err(Missing::Property), &real(1.0)),
            Outcome::Not(Verdict::Quiet)
        );
    }

    #[test]
    fn create_object_converts_as_a_value_from_outside_qml_is() {
        assert_eq!(
            decide(Mode::Created(None), Ok(&int()), &real(2.5)),
            Outcome::Set(
                Write::Value(real(3.0), WriteKind::Imperative),
                Verdict::Withhold(Some(Told::CouldNotSet))
            )
        );
        assert_eq!(
            decide(Mode::Created(None), Ok(&int()), &Value::from("x")),
            Outcome::Set(
                Write::Unconverted { reset: true },
                Verdict::Withhold(Some(Told::CouldNotSet))
            )
        );
    }

    #[test]
    fn incubate_object_converts_as_a_script_does_and_tells_what_it_cannot() {
        let p = place();
        let mode = Mode::Incubated(Some(&p));
        assert_eq!(
            decide(mode, Ok(&int()), &real(2.5)),
            Outcome::Set(
                Write::Value(real(2.5), WriteKind::Imperative),
                Verdict::Tell(Told::CannotAssign)
            )
        );
        assert_eq!(
            decide(mode, Ok(&int()), &Value::from("x")),
            Outcome::Set(
                Write::Unconverted { reset: false },
                Verdict::Tell(Told::CannotAssign)
            )
        );
    }

    #[test]
    fn a_name_that_leads_nowhere_is_quiet_or_withholds_for_create_object() {
        let created = Mode::Created(None);
        let v = real(1.0);
        assert_eq!(
            decide(created, Err(Missing::Property), &v),
            Outcome::Not(Verdict::Quiet)
        );
        assert_eq!(
            decide(created, Err(Missing::Member), &v),
            Outcome::Not(Verdict::Quiet)
        );
        assert_eq!(
            decide(created, Err(Missing::Start), &v),
            Outcome::Not(Verdict::Withhold(None))
        );
        assert_eq!(
            decide(created, Err(Missing::ObjectMember), &v),
            Outcome::Not(Verdict::Withhold(Some(Told::NoObjectMember)))
        );
    }

    #[test]
    fn a_name_that_leads_nowhere_is_told_by_incubate_object() {
        let incubated = Mode::Incubated(None);
        let v = real(1.0);
        assert_eq!(
            decide(incubated, Err(Missing::Property), &v),
            Outcome::Not(Verdict::Tell(Told::NoProperty))
        );
        assert_eq!(
            decide(incubated, Err(Missing::Start), &v),
            Outcome::Not(Verdict::Tell(Told::Unresolved))
        );
        assert_eq!(
            decide(incubated, Err(Missing::Member), &v),
            Outcome::Not(Verdict::Tell(Told::Unresolved))
        );
        assert_eq!(
            decide(incubated, Err(Missing::ObjectMember), &v),
            Outcome::Not(Verdict::Tell(Told::NoObjectMember))
        );
    }

    #[test]
    fn a_read_only_property_is_set_by_create_object_and_refused_to_incubate_object() {
        let ro = Facts {
            readonly: true,
            ..int()
        };
        assert_eq!(
            decide(Mode::Incubated(None), Ok(&ro), &real(1.0)),
            Outcome::Not(Verdict::Tell(Told::ReadOnly))
        );
        assert!(matches!(
            decide(Mode::Created(None), Ok(&ro), &real(1.0)),
            Outcome::Set(Write::Value(..), _)
        ));
    }

    #[test]
    fn a_member_of_a_value_follows_its_binding_only_for_incubate_object() {
        let font = Facts {
            member: Some("pixelSize".into()),
            ty: Some(TypeTag::Font),
            ..Facts::default()
        };
        assert_eq!(
            decide(Mode::Incubated(None), Ok(&font), &real(12.0)),
            Outcome::Set(
                Write::Member { follow: true },
                Verdict::Tell(Told::CouldNotSet)
            )
        );
        assert_eq!(
            decide(Mode::Created(None), Ok(&font), &real(12.0)),
            Outcome::Set(
                Write::Member { follow: false },
                Verdict::Withhold(Some(Told::CouldNotSet))
            )
        );
        let nosuch = Facts {
            member: Some("nosuch".into()),
            ..font
        };
        assert_eq!(
            decide(Mode::Created(None), Ok(&nosuch), &real(12.0)),
            Outcome::Not(Verdict::Quiet)
        );
    }

    #[test]
    fn what_an_attached_object_cannot_take_keeps_nothing_back() {
        let attached = Facts {
            attached: true,
            ..int()
        };
        assert_eq!(
            decide(Mode::Created(None), Ok(&attached), &Value::from("x")),
            Outcome::Set(Write::Unconverted { reset: true }, Verdict::Quiet)
        );
    }

    #[test]
    fn a_qt_binding_is_a_binding_for_a_property_itself() {
        let bound = Facts {
            binding: true,
            ..int()
        };
        for mode in [Mode::Created(None), Mode::Incubated(None)] {
            assert!(matches!(
                decide(mode, Ok(&bound), &Value::Undefined),
                Outcome::Set(Write::Bind, _)
            ));
        }
    }
}
