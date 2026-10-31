//! What crosses: a Rust value as JavaScript sees it.

use wasm_bindgen::JsValue;
use wasm_bindgen::convert::{FromWasmAbi, IntoWasmAbi};
use wasm_bindgen::describe::WasmDescribe;

use super::store::{Atom, ConvertError, ObjectId, face_of, id_of};

/// A value a property, an argument or a return can hold.
///
/// `Wire` is how it goes through wasm-bindgen: a number as a number, one call
/// that allocates nothing, and what has no direct form as a `JsValue`. What
/// an application's type did with the native trait it does with this one: the
/// native `Value` does not exist here, so such an impl is the one source line
/// that needs a `cfg`.
pub trait QmlValue: Clone + 'static {
    type Wire: IntoWasmAbi + FromWasmAbi + WasmDescribe;
    fn from_js(value: &JsValue) -> Result<Self, ConvertError>;
    fn to_js(&self) -> JsValue;
    fn into_wire(self) -> Self::Wire;
    fn from_wire(wire: Self::Wire) -> Result<Self, ConvertError>;
    /// Whether a write of `other` over this is no change.
    fn same(&self, other: &Self) -> bool;
}

macro_rules! number {
    ($($t:ty),*) => {$(
        impl QmlValue for $t {
            type Wire = $t;
            fn from_js(value: &JsValue) -> Result<Self, ConvertError> {
                value.as_f64().map(|n| n as $t).ok_or_else(|| ConvertError::new(stringify!($t)))
            }
            fn to_js(&self) -> JsValue {
                JsValue::from_f64(f64::from(*self))
            }
            fn into_wire(self) -> $t {
                self
            }
            fn from_wire(wire: $t) -> Result<Self, ConvertError> {
                Ok(wire)
            }
            #[allow(clippy::float_cmp)]
            fn same(&self, other: &Self) -> bool {
                self == other
            }
        }
    )*};
}
number!(i32, u32, f32, f64);

// Up to 2^53, which is what JavaScript can hold of one anyway.
impl QmlValue for i64 {
    type Wire = f64;
    fn from_js(value: &JsValue) -> Result<Self, ConvertError> {
        value
            .as_f64()
            .map(|n| n as i64)
            .ok_or_else(|| ConvertError::new("i64"))
    }
    fn to_js(&self) -> JsValue {
        JsValue::from_f64(*self as f64)
    }
    fn into_wire(self) -> f64 {
        self as f64
    }
    fn from_wire(wire: f64) -> Result<Self, ConvertError> {
        Ok(wire as i64)
    }
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl QmlValue for bool {
    type Wire = bool;
    fn from_js(value: &JsValue) -> Result<Self, ConvertError> {
        value.as_bool().ok_or_else(|| ConvertError::new("bool"))
    }
    fn to_js(&self) -> JsValue {
        JsValue::from_bool(*self)
    }
    fn into_wire(self) -> bool {
        self
    }
    fn from_wire(wire: bool) -> Result<Self, ConvertError> {
        Ok(wire)
    }
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl QmlValue for String {
    type Wire = String;
    fn from_js(value: &JsValue) -> Result<Self, ConvertError> {
        value.as_string().ok_or_else(|| ConvertError::new("string"))
    }
    fn to_js(&self) -> JsValue {
        JsValue::from_str(self)
    }
    fn into_wire(self) -> String {
        self
    }
    fn from_wire(wire: String) -> Result<Self, ConvertError> {
        Ok(wire)
    }
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl QmlValue for Atom {
    type Wire = String;
    fn from_js(value: &JsValue) -> Result<Self, ConvertError> {
        String::from_js(value).map(|s| Atom::from(s.as_str()))
    }
    fn to_js(&self) -> JsValue {
        JsValue::from_str(self)
    }
    fn into_wire(self) -> String {
        self.to_string()
    }
    fn from_wire(wire: String) -> Result<Self, ConvertError> {
        Ok(Atom::from(wire.as_str()))
    }
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl QmlValue for () {
    type Wire = JsValue;
    fn from_js(_: &JsValue) -> Result<Self, ConvertError> {
        Ok(())
    }
    fn to_js(&self) -> JsValue {
        JsValue::UNDEFINED
    }
    fn into_wire(self) -> JsValue {
        JsValue::UNDEFINED
    }
    fn from_wire(_: JsValue) -> Result<Self, ConvertError> {
        Ok(())
    }
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl QmlValue for ObjectId {
    type Wire = JsValue;
    fn from_js(value: &JsValue) -> Result<Self, ConvertError> {
        id_of(value).ok_or_else(|| ConvertError::new("object"))
    }
    fn to_js(&self) -> JsValue {
        face_of(*self).unwrap_or(JsValue::NULL)
    }
    fn into_wire(self) -> JsValue {
        self.to_js()
    }
    fn from_wire(wire: JsValue) -> Result<Self, ConvertError> {
        Self::from_js(&wire)
    }
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl<T: QmlValue> QmlValue for Option<T> {
    type Wire = JsValue;
    fn from_js(value: &JsValue) -> Result<Self, ConvertError> {
        if value.is_null() || value.is_undefined() {
            Ok(None)
        } else {
            T::from_js(value).map(Some)
        }
    }
    fn to_js(&self) -> JsValue {
        self.as_ref().map_or(JsValue::NULL, T::to_js)
    }
    fn into_wire(self) -> JsValue {
        self.to_js()
    }
    fn from_wire(wire: JsValue) -> Result<Self, ConvertError> {
        Self::from_js(&wire)
    }
    fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Some(a), Some(b)) => a.same(b),
            (None, None) => true,
            _ => false,
        }
    }
}

impl<T: QmlValue> QmlValue for Vec<T> {
    type Wire = JsValue;
    fn from_js(value: &JsValue) -> Result<Self, ConvertError> {
        if !js_sys::Array::is_array(value) {
            return Err(ConvertError::new("array"));
        }
        js_sys::Array::from(value)
            .iter()
            .map(|v| T::from_js(&v))
            .collect()
    }
    fn to_js(&self) -> JsValue {
        self.iter().map(T::to_js).collect::<js_sys::Array>().into()
    }
    fn into_wire(self) -> JsValue {
        self.to_js()
    }
    fn from_wire(wire: JsValue) -> Result<Self, ConvertError> {
        Self::from_js(&wire)
    }
    fn same(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().zip(other).all(|(a, b)| a.same(b))
    }
}

/// Any value, as it is in JavaScript.
#[derive(Clone, Debug, Default)]
pub struct Value(pub JsValue);

impl QmlValue for Value {
    type Wire = JsValue;
    fn from_js(value: &JsValue) -> Result<Self, ConvertError> {
        Ok(Self(value.clone()))
    }
    fn to_js(&self) -> JsValue {
        self.0.clone()
    }
    fn into_wire(self) -> JsValue {
        self.0
    }
    fn from_wire(wire: JsValue) -> Result<Self, ConvertError> {
        Ok(Self(wire))
    }
    fn same(&self, other: &Self) -> bool {
        js_sys::Object::is(&self.0, &other.0)
    }
}
