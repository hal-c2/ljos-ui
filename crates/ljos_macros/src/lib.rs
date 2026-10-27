//! Proc-macros for `ljos_engine`: `#[derive(QmlObject)]` and `#[qml_impl]`.
//!
//! Generated code names everything through the engine crate: as
//! `::ljos_engine` in a crate that depends on it, and through the `ljos`
//! facade in one that depends on the facade alone, so an application
//! needs only `use ljos::prelude::*;` (see `engine_path`).
//!
//! # `#[derive(QmlObject)]`
//!
//! Container attributes, `#[qml(...)]`:
//!
//! - `name = "Rectangle"`: the QML type name (default: the struct name);
//! - `module = "QtQuick"`: the exporting module, for the type dump;
//! - `extends = Item`: the base type. It must match the `#[qml(base)]`
//!   field, which is how the base's part of the object is stored. A type
//!   QML can write by name (a `module` and not `uncreatable`) without a
//!   base field builds on `QtObject`, as every QML object does: the engine
//!   stores that part beside the struct (`object::with_implied_base`), so
//!   the type has `objectName` without a field for it;
//! - `default = "data"`: the default property;
//! - `attached = KeysAttached`: the type `Name.` attaches;
//! - `uncreatable`: not creatable from QML. Otherwise the type must
//!   implement `Default`, which is how the engine creates it.
//! - `plain_object`: scripts see a plain object, not a QObject: it prints
//!   as `[object Object]`, as what Qt makes in script (an incubator) does.
//!
//! - `enums = path`: a `fn() -> Vec<(&'static str, Vec<(&'static str, i64)>)>`
//!   listing the type's enums (`Component.Ready`).
//! - `reset_inherited(baseline_offset = expr)`: how assigning `undefined`
//!   takes back a property the base declares and cannot take back itself
//!   (`expr` is a `ljos_engine::Reset`, as for a field's `reset = expr`).
//!
//! Field attributes: `base`, `notify` (adds `nameChanged`), `readonly`,
//! `required`, `constant`, `group` (a grouped property; the field's type
//! derives `QmlObject` too; with a plain `reset` it is assigned and taken
//! back whole, `MetaBuilder::whole_group`), `lazy` (with `group`, on an `Option<Box<T>>`
//! field: the group is made on its first write, and until then reads give
//! `T::default()`, so an object that never sets the group pays for a
//! pointer and not for the group; `lazy = path` names a `fn() -> T` to
//! give instead, when two groups of a type start out differently), `list` (a QML `list<T>` property, on a
//! `QmlList` field), `append = path` (with `list`: a
//! `fn(&Cx, ObjectId) -> Result<(), EngineError>` every append goes
//! through), `insert = path` (with `append`: a
//! `fn(&Cx, ObjectId, ObjectId) -> Result<(), EngineError>` that puts an
//! item before another), `remove = path` (with `append`: a
//! `fn(&Cx, ObjectId) -> Result<(), EngineError>` that takes an item out
//! when a script shortens the list), `skip`, `rename = "name"`, `enumeration = "Name"` (on an `i32`
//! field: the property holds a key of that enum of the type, one of those
//! `enums` lists or an inherited one; QML may then give the key by name, and
//! what is neither a key's name nor a number is refused; `"Qt.Name"` is an
//! enum of the `Qt` namespace, which no type lists), `flags = "Name"` (as
//! `enumeration`, for a property that holds several keys at once: QML may
//! give them in one string, `"A|B"`), `reset` (assigning `undefined` takes
//! the property back to the value a new object has, and it counts as never
//! set) and `reset = expr` (it takes the property back as that
//! `ljos_engine::Reset` says: `reset = Reset::Follow(margins)`) and
//! `object = "Name"` (on a field that refers to one object: the QML type of
//! that object, which the Rust type of the field cannot say; no other object
//! is taken there, and whoever reads QML before the engine does knows from
//! it what may be written in the property, so that what is written in one of
//! `object = "Component"` is a template made later) and `holds = "Name"` (on
//! a field that takes any object: the QML type a description of the type
//! says it holds, where the engine takes other objects too, as a control's
//! part that a style gives as a `Component` and the control makes) and
//! `write = path` (the field's setter, as in Qt: every write goes through
//! it before it is stored. It is a `fn(&self, T) -> Option<T>`, `T` the
//! field's type, or `fn(cx: &Cx, T) -> Option<T>` when it has more to do
//! than look at the struct; the signature picks the form. It gets the
//! value already converted, and returns `Some` with the value to store,
//! the one given or a corrected one, or `None` to refuse the write. It runs
//! for every write, one of the value already held included; nothing hears
//! of a write that leaves the value as it was, nor of one it refuses. The
//! `Cx` form runs with nothing borrowed: it reads the object through `cx`
//! and may write its other properties, which go through their own setters
//! and are heard before the write that ran it returns. A write of its own
//! property from inside it is refused with
//! `EngineError::WriteInOwnSetter`; the value it returns is what is stored.
//! What it reads is not tracked, so a binding of the property does not come
//! to depend on it. QML assignments, bindings, animation steps and
//! `Engine::write_by_name`/`Engine::set` all go through it; `undefined`
//! taking the property back does not. `Self::name` names a method of the
//! struct) and `given = path` (a `fn(cx: &Cx, PropIndex)` told each time
//! an author gives the property a value, a binding or an override, once it
//! is in place; what bindings, animations and layouts write as they run is
//! not told. A type that follows what few of its objects set follows just
//! those) and `change = path` (the same kind of `fn`, told each time the
//! property changes however it does, a binding's change before the binding
//! has run again: what follows the property on many objects keeps a mark
//! for each instead of an effect). Field names are exposed in camelCase unless renamed.
//!
//! For each property field the derive adds a typed handle,
//! `pub const FIELD_NAME: Prop<FieldType>`, to the struct's inherent impl.
//!
//! # `#[qml_impl]`
//!
//! On an inherent impl block:
//!
//! - `#[qml(signal)] fn pressed(x: f64, y: f64);` declares a signal (the
//!   declaration has no body and is removed) and generates
//!   `emit_pressed(cx, x, y)`. A parameter written `_` has no name: a
//!   handler that does not declare its parameters is given the named ones
//!   only, as Qt gives a handler only those its signal names;
//! - `#[qml(invokable)]` exposes a method to QML and JS;
//! - `#[qml(on_complete)]` marks the completion hook.
//!
//! Exposed methods take `&self`, `&mut self`, or a `cx: &Cx` first
//! parameter, never both a receiver and a `Cx` (see `ljos_engine::Cx`).
//! All take `name = "..."` to override the camelCase name.
//!
//! # `#[qml_singleton]`
//!
//! On a struct that derives `QmlObject`: the type is a singleton, one
//! object per engine, made when a script first reads its name. It is
//! registered with `Engine::register_singleton` (made with `Default`)
//! or `Engine::register_singleton_with` (made by a closure, for one that
//! is handed what it depends on). `scope = "engine"` says the same;
//! `scope = "window"` makes it one object for each root the host makes,
//! read by everything under that root and destroyed with it
//! (`Engine::singleton_in`, ARCHITECTURE.md §6.4).

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{
    Attribute, Data, DeriveInput, Error, Ident, ItemImpl, LitStr, Result, parse_macro_input,
};

mod derive;
mod qml_impl;

use derive::derive;
use qml_impl::qml_impl_inner;

#[proc_macro_derive(QmlObject, attributes(qml))]
pub fn derive_qml_object(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    derive(&input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

#[proc_macro_attribute]
pub fn qml_singleton(attr: TokenStream, item: TokenStream) -> TokenStream {
    let item2: TokenStream2 = item.clone().into();
    let input = parse_macro_input!(item as DeriveInput);
    singleton(attr.into(), &input)
        .map(|imp| quote!(#item2 #imp))
        .unwrap_or_else(|e| {
            // The struct stays, so one mistake is one error.
            let e = e.into_compile_error();
            quote!(#item2 #e)
        })
        .into()
}

fn singleton(attr: TokenStream2, input: &DeriveInput) -> Result<TokenStream2> {
    struct Scope(Option<LitStr>);
    impl Parse for Scope {
        fn parse(input: ParseStream) -> Result<Self> {
            if input.is_empty() {
                return Ok(Scope(None));
            }
            let key: Ident = input.parse()?;
            if key != "scope" {
                return Err(Error::new(
                    key.span(),
                    "#[qml_singleton] takes only `scope = \"engine\"` or `scope = \"window\"`",
                ));
            }
            input.parse::<syn::Token![=]>()?;
            let value: LitStr = input.parse()?;
            if !input.is_empty() {
                return Err(input.error("unexpected tokens after the scope"));
            }
            Ok(Scope(Some(value)))
        }
    }
    if !matches!(input.data, Data::Struct(_)) {
        return Err(Error::new(
            input.ident.span(),
            "#[qml_singleton] goes on a struct that derives QmlObject",
        ));
    }
    let Scope(scope) = syn::parse2(attr)?;
    let which = match scope.as_ref().map(LitStr::value).as_deref() {
        None | Some("engine") => quote!(Engine),
        Some("window") => quote!(Window),
        Some(other) => {
            return Err(Error::new(
                scope.span(),
                format!("unknown singleton scope \"{other}\": expected \"engine\" or \"window\""),
            ));
        }
    };
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let e = engine_path();
    Ok(quote! {
        impl #impl_generics #e::QmlSingleton for #name #ty_generics #where_clause {
            const SCOPE: #e::SingletonScope = #e::SingletonScope::#which;
        }
    })
}

#[proc_macro_attribute]
pub fn qml_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return Error::new(Span::call_site(), "#[qml_impl] takes no arguments")
            .into_compile_error()
            .into();
    }
    let item = parse_macro_input!(item as ItemImpl);
    qml_impl_inner(item)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

/// The path generated code names the engine crate by, as the crate
/// being compiled reaches it: `::ljos_engine` when it depends on the
/// engine directly (or is the engine, which names itself so), else the
/// engine through the `ljos` facade, under whatever name the crate gave
/// that dependency. An application written against the facade alone
/// needs no line of its own for the generated paths to resolve.
///
/// Dependencies and dev-dependencies are read alike, so a facade crate
/// that takes `ljos_engine` as a dev-dependency only gets the direct
/// path in its library too and must depend on the engine there as well.
fn engine_path() -> TokenStream2 {
    use proc_macro_crate::{FoundCrate, crate_name};
    match (crate_name("ljos_engine"), crate_name("ljos")) {
        (Ok(FoundCrate::Name(n)), _) => {
            let n = Ident::new(&n, Span::call_site());
            quote!(::#n)
        }
        // The facade's own library has the engine as a dependency of
        // its own; only its tests and examples see `ljos` by name.
        (Err(_), Ok(FoundCrate::Name(n))) => {
            let n = Ident::new(&n, Span::call_site());
            quote!(::#n::ljos_engine)
        }
        _ => quote!(::ljos_engine),
    }
}

/// `snake_case` to `camelCase`. Leading underscores are kept.
fn camel(s: &str) -> String {
    let lead = s.len() - s.trim_start_matches('_').len();
    let mut out = String::from(&s[..lead]);
    let mut upper = false;
    for c in s[lead..].chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn is_qml(a: &Attribute) -> bool {
    a.path().is_ident("qml")
}

#[cfg(test)]
mod tests {
    use super::camel;

    #[test]
    fn camel_case() {
        assert_eq!(camel("implicit_width"), "implicitWidth");
        assert_eq!(camel("x"), "x");
        assert_eq!(camel("_private_thing"), "_privateThing");
        assert_eq!(camel("double__click"), "doubleClick");
        assert_eq!(camel("already"), "already");
    }
}
