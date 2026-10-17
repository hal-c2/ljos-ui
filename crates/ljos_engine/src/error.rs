//! Engine errors.

use ljos_reactive::ReactiveError;

use crate::ids::ObjectId;
use crate::types::ConvertError;

/// What can go wrong when Rust or JS talks to the engine. The messages
/// follow Qt's wording where Qt has one, so warnings read the same.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum EngineError {
    /// The object was destroyed (or the id never existed).
    #[error("object {0:?} has been destroyed")]
    DeadObject(ObjectId),
    #[error("{ty} has no property \"{name}\"")]
    UnknownProperty { ty: String, name: String },
    #[error("{ty} has no signal \"{name}\"")]
    UnknownSignal { ty: String, name: String },
    #[error("{ty} has no method \"{name}\"")]
    UnknownMethod { ty: String, name: String },
    /// A typed handle (`Rectangle::RADIUS`) used on an object that is not a
    /// `Rectangle`.
    #[error("{ty} is not a {expected}")]
    WrongType { ty: String, expected: String },
    #[error("Invalid property assignment: \"{0}\" is a read-only property")]
    ReadOnly(String),
    /// `border` in `border.width` is not itself assignable.
    #[error("Cannot assign to grouped property \"{0}\"; assign its members")]
    GroupProperty(String),
    /// A setter that gets a `Cx` wrote the property it is the setter of:
    /// the value it returns is what is stored.
    #[error(
        "{ty}.{name} cannot be written from inside its own setter; the setter returns the value to store"
    )]
    WriteInOwnSetter { ty: String, name: String },
    #[error(transparent)]
    Convert(#[from] ConvertError),
    #[error("{name}: expected {expected} arguments, got {got}")]
    ArgumentCount {
        name: String,
        expected: usize,
        got: usize,
    },
    /// A method thunk (or a native method returning `Err`) failed.
    #[error("{0}")]
    Invoke(String),
    /// A method declared in QML; only the JS host can run it.
    #[error("{0} has no native implementation")]
    NotNative(String),
    #[error("{0} is not creatable")]
    NotCreatable(String),
    #[error("{0} has no attached type")]
    NoAttachedType(String),
    #[error("type \"{name}\" is not in module \"{uri}\"")]
    UnknownType { uri: String, name: String },
    /// `bind` and override writes on a property that has no reactive node
    /// cannot happen; this reports graph misuse that slipped through.
    #[error(transparent)]
    Reactive(#[from] ReactiveError),
    /// A component could not be loaded or made.
    #[error("{0}")]
    Load(String),
    /// `required property` left unset by whoever created the object.
    #[error("Required property {0} was not initialized")]
    RequiredNotSet(String),
    /// A JS exception or fault.
    #[error("{0}")]
    Js(String),
}

/// Why [`Engine::try_call_js`](crate::Engine::try_call_js) or
/// [`Engine::try_call_value`](crate::Engine::try_call_value) gave no value:
/// what Qt's `QJSValue::call` answers with an error value for.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum JsError {
    /// The function threw. `message` is what the thrown value prints as
    /// (`Error: why it failed`); `value` is the value thrown, an `Error`
    /// or any object as a [`Value::Js`](crate::Value::Js) whose members
    /// [`Engine::js_member`](crate::Engine::js_member) reads, or a
    /// primitive; `file` and `line` place the throw, in QML terms when
    /// the code came from QML; `stack` is the QML trace, given when the
    /// throw passed through more than one function.
    ///
    /// `value` holds a JS object by reference, which keeps it from being
    /// collected while the error is held; once the engine is gone it holds
    /// nothing, and dropping it is still safe.
    #[error("{message}")]
    Thrown {
        message: String,
        value: crate::Value,
        file: Option<String>,
        line: Option<u32>,
        stack: Option<String>,
    },
    /// What was called is not a JS function (`QJSValue::isCallable` false).
    #[error("not a function")]
    NotCallable,
    /// The call did not finish: interrupted over its time budget, out of
    /// heap, or the JS host could not start.
    #[error("{0}")]
    Failed(String),
}

impl JsError {
    /// What went wrong, for a person: the `message` of what was thrown
    /// when it has one that is a string (`no` for `new Error('no')`, a
    /// subclass or a plain object alike), else what it prints as. A
    /// failure that was not a throw gives its description.
    pub fn message(&self, engine: &crate::Engine) -> String {
        match self {
            JsError::Thrown { message, value, .. } => {
                let own = match value {
                    crate::Value::Js(_) => engine.js_member(value, "message"),
                    crate::Value::Object(o) => engine
                        .property_index(*o, "message")
                        .ok()
                        .map(|_| engine.read_by_name(*o, "message")),
                    _ => None,
                };
                match own {
                    Some(crate::Value::String(s)) => s.to_string(),
                    _ => message.clone(),
                }
            }
            other => other.to_string(),
        }
    }
}

/// Lets a native method return `Result<T, String>` (or `&str`): the
/// message becomes an [`EngineError::Invoke`].
impl EngineError {
    /// [`EngineError::DeadObject`], for `ok_or_else`: built eagerly, the
    /// error is dropped on every lookup that finds its object, and the hot
    /// lookups run hundreds of thousands of times a second.
    #[cold]
    pub fn dead(obj: ObjectId) -> Self {
        Self::DeadObject(obj)
    }
}

impl From<String> for EngineError {
    fn from(s: String) -> Self {
        EngineError::Invoke(s)
    }
}

impl From<&str> for EngineError {
    fn from(s: &str) -> Self {
        EngineError::Invoke(s.to_owned())
    }
}
