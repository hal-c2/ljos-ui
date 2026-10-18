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
//! derives `QmlObject` too), `lazy` (with `group`, on an `Option<Box<T>>`
//! field: the group is made on its first write, and until then reads give
//! `T::default()`, so an object that never sets the group pays for a
//! pointer and not for the group), `list` (a QML `list<T>` property, on a
//! `QmlList` field), `append = path` (with `list`: a
//! `fn(&Cx, ObjectId) -> Result<(), EngineError>` every append goes
//! through), `insert = path` (with `append`: a
//! `fn(&Cx, ObjectId, ObjectId) -> Result<(), EngineError>` that puts an
//! item before another), `skip`, `rename = "name"`, `enumeration = "Name"` (on an `i32`
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
//! //! struct). Field names are exposed in camelCase unless renamed.
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
//!   `emit_pressed(cx, x, y)`;
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
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{
    Attribute, Data, DeriveInput, Error, Fields, FnArg, Ident, ImplItem, ItemImpl, LitStr, Pat,
    Result, ReturnType, Signature, Type, Visibility, parse_macro_input,
};

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

// ---------------------------------------------------------------- derive

#[derive(Default)]
struct ContainerAttrs {
    name: Option<String>,
    module: Option<String>,
    extends: Option<Type>,
    default: Option<String>,
    attached: Option<Type>,
    uncreatable: bool,
    plain_object: bool,
    enums: Option<syn::Path>,
    /// (camelCase name, the `Reset`) for properties of the base.
    reset_inherited: Vec<(String, syn::Expr)>,
}

#[derive(Default)]
struct FieldAttrs {
    base: bool,
    notify: bool,
    readonly: bool,
    required: bool,
    constant: bool,
    group: bool,
    /// With `group`: an `Option<Box<T>>` made on the first write.
    lazy: bool,
    list: bool,
    append: Option<syn::Path>,
    insert: Option<syn::Path>,
    skip: bool,
    rename: Option<String>,
    enumeration: Option<String>,
    /// With `enumeration`: it was written `flags = "Name"`.
    flags: bool,
    /// `reset` or `reset = expr`: what `undefined` does to the property.
    reset: Option<Option<syn::Expr>>,
    /// `object = "Name"`: the QML type of the object the field refers to.
    object: Option<String>,
    /// `holds = "Name"`: the QML type a description says the field holds.
    holds: Option<String>,
    /// `write = path`: what every write goes through before it is stored.
    write: Option<syn::Path>,
}

fn container_attrs(attrs: &[Attribute]) -> Result<ContainerAttrs> {
    let mut c = ContainerAttrs::default();
    for a in attrs.iter().filter(|a| is_qml(a)) {
        a.parse_nested_meta(|m| {
            if m.path.is_ident("name") {
                c.name = Some(m.value()?.parse::<LitStr>()?.value());
            } else if m.path.is_ident("module") {
                c.module = Some(m.value()?.parse::<LitStr>()?.value());
            } else if m.path.is_ident("extends") {
                c.extends = Some(m.value()?.parse::<Type>()?);
            } else if m.path.is_ident("default") {
                c.default = Some(m.value()?.parse::<LitStr>()?.value());
            } else if m.path.is_ident("attached") {
                c.attached = Some(m.value()?.parse::<Type>()?);
            } else if m.path.is_ident("uncreatable") {
                c.uncreatable = true;
            } else if m.path.is_ident("plain_object") {
                c.plain_object = true;
            } else if m.path.is_ident("enums") {
                c.enums = Some(m.value()?.parse::<syn::Path>()?);
            } else if m.path.is_ident("reset_inherited") {
                m.parse_nested_meta(|n| {
                    let field = n.path.require_ident()?.to_string();
                    let reset = n.value()?.parse::<syn::Expr>()?;
                    c.reset_inherited.push((camel(&field), reset));
                    Ok(())
                })?;
            } else {
                return Err(m.error(
                    "expected one of: name, module, extends, default, attached, uncreatable, plain_object, enums, reset_inherited",
                ));
            }
            Ok(())
        })?;
    }
    Ok(c)
}

/// `path` with a leading `Self` spelled as `ty`: the accessors are items
/// nested in a function, where `Self` does not reach.
fn self_to(path: &syn::Path, ty: &Ident) -> TokenStream2 {
    let mut segments = path.segments.iter();
    match segments.next() {
        Some(first) if first.ident == "Self" && path.leading_colon.is_none() => {
            let rest = segments.collect::<Vec<_>>();
            quote!(<#ty> #(:: #rest)*)
        }
        _ => quote!(#path),
    }
}

fn field_attrs(attrs: &[Attribute]) -> Result<FieldAttrs> {
    let mut f = FieldAttrs::default();
    for a in attrs.iter().filter(|a| is_qml(a)) {
        a.parse_nested_meta(|m| {
            let p = &m.path;
            if p.is_ident("base") {
                f.base = true;
            } else if p.is_ident("notify") {
                f.notify = true;
            } else if p.is_ident("readonly") {
                f.readonly = true;
            } else if p.is_ident("required") {
                f.required = true;
            } else if p.is_ident("constant") {
                f.constant = true;
            } else if p.is_ident("group") {
                f.group = true;
            } else if p.is_ident("lazy") {
                f.lazy = true;
            } else if p.is_ident("list") {
                f.list = true;
            } else if p.is_ident("append") {
                f.append = Some(m.value()?.parse::<syn::Path>()?);
            } else if p.is_ident("insert") {
                f.insert = Some(m.value()?.parse::<syn::Path>()?);
            } else if p.is_ident("skip") {
                f.skip = true;
            } else if p.is_ident("rename") {
                f.rename = Some(m.value()?.parse::<LitStr>()?.value());
            } else if p.is_ident("enumeration") || p.is_ident("flags") {
                if f.enumeration.is_some() {
                    return Err(m.error("a property holds one enum: `enumeration` or `flags`"));
                }
                f.flags = p.is_ident("flags");
                f.enumeration = Some(m.value()?.parse::<LitStr>()?.value());
            } else if p.is_ident("object") {
                f.object = Some(m.value()?.parse::<LitStr>()?.value());
            } else if p.is_ident("holds") {
                f.holds = Some(m.value()?.parse::<LitStr>()?.value());
            } else if p.is_ident("write") {
                f.write = Some(m.value()?.parse::<syn::Path>()?);
            } else if p.is_ident("reset") {
                f.reset = Some(if m.input.peek(syn::Token![=]) {
                    Some(m.value()?.parse::<syn::Expr>()?)
                } else {
                    None
                });
            } else {
                return Err(m.error(
                    "expected one of: base, notify, readonly, required, constant, group, lazy, list, append, insert, skip, rename, enumeration, flags, reset, object, holds, write",
                ));
            }
            Ok(())
        })?;
    }
    Ok(f)
}

/// `T` of `Option<Box<T>>`.
fn boxed_inner(t: &Type) -> Option<Type> {
    fn only_arg(t: &Type, outer: &str) -> Option<Type> {
        let Type::Path(tp) = t else { return None };
        let seg = tp.path.segments.last()?;
        if seg.ident != outer {
            return None;
        }
        let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
            return None;
        };
        match args.args.first()? {
            syn::GenericArgument::Type(inner) if args.args.len() == 1 => Some(inner.clone()),
            _ => None,
        }
    }
    only_arg(&only_arg(t, "Option")?, "Box")
}

fn type_key(t: &Type) -> String {
    quote!(#t).to_string()
}

fn derive(input: &DeriveInput) -> Result<TokenStream2> {
    if !input.generics.params.is_empty() || input.generics.where_clause.is_some() {
        return Err(Error::new(
            input.generics.span(),
            "#[derive(QmlObject)] does not support generic types",
        ));
    }
    let Data::Struct(data) = &input.data else {
        return Err(Error::new(
            input.ident.span(),
            "#[derive(QmlObject)] needs a struct with named fields",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(Error::new(
            input.ident.span(),
            "#[derive(QmlObject)] needs a struct with named fields",
        ));
    };
    let c = container_attrs(&input.attrs)?;
    let ty = &input.ident;
    let e = engine_path();
    let p = quote!(#e::__private);
    let qml_name = c.name.clone().unwrap_or_else(|| ty.to_string());

    // The base comes first in every table, so find it before numbering.
    let mut base: Option<(Ident, Type)> = None;
    for field in &fields.named {
        let a = field_attrs(&field.attrs)?;
        if !a.base || a.skip {
            continue;
        }
        let ident = field.ident.clone().expect("named field");
        if base.is_some() {
            return Err(Error::new(ident.span(), "only one #[qml(base)] field"));
        }
        if a.group || a.notify || a.readonly || a.rename.is_some() {
            return Err(Error::new(
                ident.span(),
                "#[qml(base)] takes no other attributes",
            ));
        }
        base = Some((ident, field.ty.clone()));
    }
    // A group's or an attached type's struct is not an object the engine
    // stores, so only a type QML makes by name has the implied base.
    let implied_base = base.is_none()
        && c.extends.is_none()
        && c.module.is_some()
        && !c.uncreatable
        && qml_name != "QtObject";
    let base_count = match &base {
        Some((_, b)) => quote!(<#b as #e::QmlType>::PROPERTY_COUNT),
        None if implied_base => quote!(<#e::QtObject as #e::QmlType>::PROPERTY_COUNT),
        None => quote!(0u32),
    };

    let mut builder = Vec::new();
    let mut accessors = Vec::new();
    let mut consts = Vec::new();
    // Property count of everything declared so far, as a const expression.
    let mut count: Vec<TokenStream2> = Vec::new();

    for field in &fields.named {
        let a = field_attrs(&field.attrs)?;
        let ident = field.ident.clone().expect("named field");
        let fty = &field.ty;
        if a.skip || a.base {
            continue;
        }
        let name = a
            .rename
            .clone()
            .unwrap_or_else(|| camel(&ident.to_string()));
        let mut flags = Vec::new();
        if a.readonly {
            flags.push(quote!(#e::PropertyFlags::READONLY));
        }
        if a.required {
            flags.push(quote!(#e::PropertyFlags::REQUIRED));
        }
        if a.constant {
            flags.push(quote!(#e::PropertyFlags::CONSTANT));
        }
        if a.list {
            flags.push(quote!(#e::PropertyFlags::LIST));
        }
        if a.append.is_some() && !a.list {
            return Err(Error::new(
                ident.span(),
                "`append` needs `list`: only a list property has appends",
            ));
        }
        if a.insert.is_some() && a.append.is_none() {
            return Err(Error::new(
                ident.span(),
                "`insert` needs `append`: a list without an append hook is inserted into as it is",
            ));
        }
        let flags = quote!(#e::PropertyFlags::empty() #(.union(#flags))*);
        let index = quote!(#base_count #(+ #count)*);

        if a.lazy && !a.group {
            return Err(Error::new(ident.span(), "`lazy` is for a `group`"));
        }
        if a.group {
            if a.enumeration.is_some() {
                return Err(Error::new(
                    ident.span(),
                    "a group holds no enum; put `enumeration` on the group type's field",
                ));
            }
            if a.notify {
                return Err(Error::new(
                    ident.span(),
                    "a group has no change signal; put `notify` on the group type's fields",
                ));
            }
            if a.reset.is_some() {
                return Err(Error::new(
                    ident.span(),
                    "a group is not taken back whole; put `reset` on the group type's fields",
                ));
            }
            if a.write.is_some() {
                return Err(Error::new(
                    ident.span(),
                    "a group is not written whole; put `write` on the group type's fields",
                ));
            }
            let get = format_ident!("__qml_group_{}", ident);
            let get_mut = format_ident!("__qml_group_mut_{}", ident);
            let fty = if a.lazy {
                let inner = boxed_inner(fty).ok_or_else(|| {
                    Error::new(fty.span(), "a `lazy` group is an `Option<Box<T>>` field")
                })?;
                accessors.push(quote! {
                    fn #get(o: &dyn #e::QmlObject) -> &dyn #e::QmlObject {
                        static UNSET: ::std::sync::OnceLock<#inner> = ::std::sync::OnceLock::new();
                        match &#p::downcast::<#ty>(o).#ident {
                            ::std::option::Option::Some(g) => &**g,
                            ::std::option::Option::None => {
                                UNSET.get_or_init(<#inner as ::std::default::Default>::default)
                            }
                        }
                    }
                    fn #get_mut(o: &mut dyn #e::QmlObject) -> &mut dyn #e::QmlObject {
                        &mut **#p::downcast_mut::<#ty>(o).#ident.get_or_insert_with(
                            ::std::default::Default::default,
                        )
                    }
                });
                inner
            } else {
                accessors.push(quote! {
                    fn #get(o: &dyn #e::QmlObject) -> &dyn #e::QmlObject {
                        &#p::downcast::<#ty>(o).#ident
                    }
                    fn #get_mut(o: &mut dyn #e::QmlObject) -> &mut dyn #e::QmlObject {
                        &mut #p::downcast_mut::<#ty>(o).#ident
                    }
                });
                fty.clone()
            };
            let fty = &fty;
            builder.push(quote! {
                .group(#name, <#fty as #e::QmlType>::static_meta(), #flags, #get, #get_mut)
            });
            count.push(quote!((1 + <#fty as #e::QmlType>::PROPERTY_COUNT)));
            continue;
        }

        let get = format_ident!("__qml_get_{}", ident);
        let set = format_ident!("__qml_set_{}", ident);
        let notify = a.notify;
        accessors.push(quote! {
            fn #get(o: &dyn #e::QmlObject) -> #e::Value {
                #e::QmlValue::to_value(&#p::downcast::<#ty>(o).#ident)
            }
            fn #set(
                o: &mut dyn #e::QmlObject,
                v: &#e::Value,
            ) -> #p::Result<(), #e::ConvertError> {
                #p::downcast_mut::<#ty>(o).#ident = <#fty as #e::QmlValue>::from_value(v)?;
                #p::Result::Ok(())
            }
        });
        let tag = match &a.enumeration {
            Some(enumeration) => {
                if a.list {
                    return Err(Error::new(
                        ident.span(),
                        "`enumeration` is for a field that holds one number, not a list",
                    ));
                }
                // The field keeps the number: the engine has turned a key's
                // name into it before the setter runs.
                if a.flags {
                    quote!(#e::TypeTag::Flags(#e::Atom::intern(#enumeration)))
                } else {
                    quote!(#e::TypeTag::Enum(#e::Atom::intern(#enumeration)))
                }
            }
            // A field that refers to an object has no Rust type for what
            // the object is, so the attribute says it: the engine then takes
            // no other object there, and a table of types knows what is
            // under the property (and that a `Component` is a template).
            None => match &a.object {
                Some(_) if a.list => {
                    return Err(Error::new(
                        ident.span(),
                        "`object` is for a field that refers to one object, not a list",
                    ));
                }
                Some(object) => {
                    quote!(#e::TypeTag::Object(#p::Option::Some(#e::Atom::intern(#object))))
                }
                None => quote!(<#fty as #e::QmlValue>::type_tag()),
            },
        };
        builder.push(quote! {
            .field(#name, #tag, #flags, #notify, #get, #set)
        });
        if let Some(append) = &a.append {
            builder.push(quote!(.list_append(#append)));
        }
        if let Some(insert) = &a.insert {
            builder.push(quote!(.list_insert(#insert)));
        }
        if let Some(holds) = &a.holds {
            builder.push(quote!(.holds(#holds)));
        }
        if let Some(path) = &a.write {
            if a.list {
                return Err(Error::new(
                    ident.span(),
                    "`write` is for a field that holds a value; a list's appends go through `append`",
                ));
            }
            let write = format_ident!("__qml_write_{}", ident);
            let write_cx = format_ident!("__qml_write_cx_{}", ident);
            let path = self_to(path, ty);
            // Both forms are generated; the method's signature picks one.
            accessors.push(quote! {
                fn #write(
                    o: &dyn #e::QmlObject,
                    v: &#e::Value,
                ) -> #p::Result<#p::Option<#e::Value>, #e::ConvertError> {
                    let wanted = <#fty as #e::QmlValue>::from_value(v)?;
                    let kept: #p::Option<#fty> = #p::WriteFn::<#ty, #fty, _>::on_native(
                        &#path,
                        #p::downcast::<#ty>(o),
                        wanted,
                    );
                    #p::Result::Ok(kept.map(|k| #e::QmlValue::to_value(&k)))
                }
                fn #write_cx(
                    cx: &#e::Cx<'_>,
                    v: &#e::Value,
                ) -> #p::Result<#p::Option<#e::Value>, #e::ConvertError> {
                    let wanted = <#fty as #e::QmlValue>::from_value(v)?;
                    let kept: #p::Option<#fty> =
                        #p::WriteFn::<#ty, #fty, _>::on_cx(&#path, cx, wanted);
                    #p::Result::Ok(kept.map(|k| #e::QmlValue::to_value(&k)))
                }
            });
            builder.push(
                quote!(.write(#p::WriteFn::<#ty, #fty, _>::filter(&#path, #write, #write_cx))),
            );
        }
        match &a.reset {
            Some(Some(reset)) => builder.push(quote!(.reset(#reset))),
            Some(None) => builder.push(quote!(.reset(#e::Reset::Default))),
            None => {}
        }
        if a.list {
            // The engine appends to the field where it is, so that adding
            // a child does not cost more the more children there are.
            let list = format_ident!("__qml_list_{}", ident);
            accessors.push(quote! {
                fn #list(o: &mut dyn #e::QmlObject) -> &mut #e::QmlList {
                    &mut #p::downcast_mut::<#ty>(o).#ident
                }
            });
            builder.push(quote!(.list_field(#list)));
        }
        let cname = format_ident!(
            "{}",
            ident.to_string().trim_start_matches('_').to_uppercase()
        );
        let doc = format!("The `{name}` property.");
        consts.push(quote! {
            #[doc = #doc]
            pub const #cname: #e::Prop<#fty> =
                #e::Prop::new(#index, <#ty as #e::QmlType>::static_meta);
        });
        count.push(quote!(1));
    }

    // `extends` is checked against the base field so the declared base and
    // the stored one cannot drift apart.
    let base_ty = match (&c.extends, &base) {
        (Some(ext), Some((_, fty))) => {
            if type_key(ext) != type_key(fty) {
                return Err(Error::new(
                    ext.span(),
                    format!(
                        "`extends = {}` but the #[qml(base)] field is a `{}`",
                        type_key(ext),
                        type_key(fty)
                    ),
                ));
            }
            Some(fty.clone())
        }
        (Some(ext), None) => {
            return Err(Error::new(
                ext.span(),
                "`extends` needs a #[qml(base)] field holding the base type",
            ));
        }
        (None, Some((_, fty))) => Some(fty.clone()),
        (None, None) if implied_base => Some(syn::parse_quote!(#e::QtObject)),
        (None, None) => None,
    };

    let base_methods = match &base {
        Some((f, _)) => quote! {
            fn base(&self) -> #p::Option<&dyn #e::QmlObject> {
                #p::Option::Some(&self.#f)
            }
            fn base_mut(&mut self) -> #p::Option<&mut dyn #e::QmlObject> {
                #p::Option::Some(&mut self.#f)
            }
        },
        None => quote!(),
    };
    let base_call = base_ty
        .as_ref()
        .map(|b| quote!(.base(<#b as #e::QmlType>::static_meta())));
    let module_call = c.module.as_ref().map(|m| quote!(.module(#m)));
    let default_call = c.default.as_ref().map(|d| quote!(.default_property(#d)));
    let enums_call = c.enums.as_ref().map(|f| {
        quote! {
            .enumerations(#f())
        }
    });
    let inherited_resets = c
        .reset_inherited
        .iter()
        .map(|(name, reset)| quote!(.reset_inherited(#name, #reset)));
    let attached_call = c
        .attached
        .as_ref()
        .map(|a| quote!(.attached(|| #e::MetaRef::Static(<#a as #e::QmlType>::static_meta()))));
    let plain_call = c.plain_object.then(|| quote!(.plain_object()));
    let create_call = if c.uncreatable {
        quote!(.uncreatable())
    } else {
        quote!(.creatable(|| #p::Box::new(<#ty as ::core::default::Default>::default())))
    };

    Ok(quote! {
        impl #e::QmlObject for #ty {
            fn meta(&self) -> &'static #e::MetaObject {
                <#ty as #e::QmlType>::static_meta()
            }
            fn as_any(&self) -> &dyn #p::Any {
                self
            }
            fn as_any_mut(&mut self) -> &mut dyn #p::Any {
                self
            }
            #base_methods
        }

        impl #e::QmlType for #ty {
            const PROPERTY_COUNT: u32 = #base_count #(+ #count)*;

            fn static_meta() -> &'static #e::MetaObject {
                #(#accessors)*
                static META: #p::LazyLock<#e::MetaObject> = #p::LazyLock::new(|| {
                    #[allow(unused_imports)]
                    use #p::{ImplFallback as _, ImplViaTrait as _};
                    let meta = #e::MetaBuilder::new(#qml_name)
                        .native::<#ty>()
                        #module_call
                        #base_call
                        #(#builder)*
                        #(#inherited_resets)*
                        #default_call
                        #enums_call
                        #attached_call
                        #create_call
                        #plain_call
                        .impl_info((&#p::Probe::<#ty>::new()).qml_impl_info())
                        .build();
                    debug_assert_eq!(
                        meta.properties.len() as u32,
                        <#ty as #e::QmlType>::PROPERTY_COUNT,
                        "ljos_engine: property count of {} disagrees with its table",
                        #qml_name,
                    );
                    meta
                });
                &META
            }
        }

        #[allow(dead_code)]
        impl #ty {
            #(#consts)*
        }
    })
}

// ------------------------------------------------------------- qml_impl

enum Kind {
    Signal,
    Invokable,
    Complete,
}

struct Member {
    kind: Kind,
    name: Option<String>,
}

/// Reads and removes the `#[qml(...)]` attributes of an impl item.
fn take_member(attrs: &mut Vec<Attribute>) -> Result<Option<Member>> {
    let mut out: Option<Member> = None;
    let mut err = None;
    attrs.retain(|a| {
        if !is_qml(a) {
            return true;
        }
        let mut kind = None;
        let mut name = None;
        let r = a.parse_nested_meta(|m| {
            if m.path.is_ident("signal") {
                kind = Some(Kind::Signal);
            } else if m.path.is_ident("invokable") {
                kind = Some(Kind::Invokable);
            } else if m.path.is_ident("on_complete") {
                kind = Some(Kind::Complete);
            } else if m.path.is_ident("name") {
                name = Some(m.value()?.parse::<LitStr>()?.value());
            } else {
                return Err(m.error("expected one of: signal, invokable, on_complete, name"));
            }
            Ok(())
        });
        match (r, kind) {
            (Err(e), _) => err = Some(e),
            (Ok(()), None) => {
                err = Some(Error::new(
                    a.span(),
                    "expected #[qml(signal)], #[qml(invokable)] or #[qml(on_complete)]",
                ))
            }
            (Ok(()), Some(kind)) => out = Some(Member { kind, name }),
        }
        false
    });
    match err {
        Some(e) => Err(e),
        None => Ok(out),
    }
}

/// A bodyless `fn name(..);`, which syn leaves as verbatim tokens.
struct BodylessFn {
    attrs: Vec<Attribute>,
    sig: Signature,
}

impl Parse for BodylessFn {
    fn parse(input: ParseStream) -> Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let _vis: Visibility = input.parse()?;
        let sig: Signature = input.parse()?;
        input.parse::<syn::Token![;]>()?;
        Ok(BodylessFn { attrs, sig })
    }
}

enum Receiver {
    None,
    Ref,
    Mut,
}

/// What an exposed method takes, apart from its QML arguments.
struct Shape {
    receiver: Receiver,
    /// `None`, `Some(false)` for `&Cx`, `Some(true)` for `&mut Cx`.
    cx: Option<bool>,
    args: Vec<(String, Type)>,
}

/// Whether `t` is `&Cx`/`&mut Cx` (any lifetime, any path to it).
fn cx_ref(t: &Type) -> Option<bool> {
    let Type::Reference(r) = t else { return None };
    let Type::Path(p) = &*r.elem else { return None };
    (p.path.segments.last()?.ident == "Cx").then_some(r.mutability.is_some())
}

fn shape(sig: &Signature) -> Result<Shape> {
    if !sig.generics.params.is_empty() {
        return Err(Error::new(
            sig.generics.span(),
            "exposed methods cannot be generic",
        ));
    }
    if sig.asyncness.is_some() {
        return Err(Error::new(sig.span(), "exposed methods cannot be async"));
    }
    let mut s = Shape {
        receiver: Receiver::None,
        cx: None,
        args: Vec::new(),
    };
    for (i, arg) in sig.inputs.iter().enumerate() {
        match arg {
            FnArg::Receiver(r) => match &r.kind {
                syn::ReceiverKind::Reference(_, _, m) => {
                    s.receiver = if m.is_some() {
                        Receiver::Mut
                    } else {
                        Receiver::Ref
                    };
                }
                _ => {
                    return Err(Error::new(
                        r.span(),
                        "exposed methods take `&self` or `&mut self`",
                    ));
                }
            },
            FnArg::Typed(pt) => {
                if let Some(m) = cx_ref(&pt.ty) {
                    if !matches!(s.receiver, Receiver::None) {
                        // Holding the struct borrowed while the engine runs
                        // user code through the Cx would re-enter the store.
                        return Err(Error::new(
                            pt.span(),
                            "take `&self`/`&mut self` or a `Cx`, not both; \
                             read fields through `cx.with` instead",
                        ));
                    }
                    if i != 0 {
                        return Err(Error::new(
                            pt.span(),
                            "the `Cx` must be the first parameter",
                        ));
                    }
                    s.cx = Some(m);
                    continue;
                }
                let name = match &*pt.pat {
                    Pat::Ident(pi) => camel(&pi.ident.to_string()),
                    _ => format!("arg{}", s.args.len()),
                };
                s.args.push((name, (*pt.ty).clone()));
            }
        }
    }
    Ok(s)
}

/// The last segment of an impl's type, as errors name it.
fn type_name(t: &Type) -> String {
    match t {
        Type::Path(p) => p
            .path
            .segments
            .last()
            .map_or_else(String::new, |s| s.ident.to_string()),
        _ => quote!(#t).to_string(),
    }
}

/// For `-> Result<T, E>`, `Some(T)`.
fn result_ok(t: &Type) -> Option<&Type> {
    let Type::Path(p) = t else { return None };
    let seg = p.path.segments.last()?;
    if seg.ident != "Result" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(a) = &seg.arguments else {
        return None;
    };
    a.args.iter().find_map(|g| match g {
        syn::GenericArgument::Type(t) => Some(t),
        _ => None,
    })
}

/// The call expression for an exposed method, given argument idents.
fn call(fn_ident: &Ident, s: &Shape, args: &[Ident]) -> TokenStream2 {
    let e = engine_path();
    match (&s.receiver, s.cx) {
        (Receiver::Ref, _) => quote! {
            cx.with::<Self, _>(|this| this.#fn_ident(#(#args),*))?
        },
        (Receiver::Mut, _) => quote! {
            cx.with_mut::<Self, _>(|this| this.#fn_ident(#(#args),*))?
        },
        (Receiver::None, Some(false)) => quote!(Self::#fn_ident(cx #(, #args)*)),
        (Receiver::None, Some(true)) => quote! {{
            let mut cx: #e::Cx<'_> = *cx;
            Self::#fn_ident(&mut cx #(, #args)*)
        }},
        (Receiver::None, None) => quote!(Self::#fn_ident(#(#args),*)),
    }
}

fn qml_impl_inner(mut item: ItemImpl) -> Result<TokenStream2> {
    if item.trait_.is_some() {
        return Err(Error::new(
            item.span(),
            "#[qml_impl] goes on an inherent impl block",
        ));
    }
    if !item.generics.params.is_empty() {
        return Err(Error::new(
            item.generics.span(),
            "#[qml_impl] does not support generic impls",
        ));
    }
    let e = engine_path();
    let p = quote!(#e::__private);
    let self_ty = item.self_ty.clone();

    let mut signals = Vec::new();
    let mut methods = Vec::new();
    let mut generated = Vec::new();
    let mut on_complete = None;
    let mut kept = Vec::new();

    for it in std::mem::take(&mut item.items) {
        // A signal has no body; syn hands it over as verbatim tokens.
        let (member, sig, body_item) = match it {
            ImplItem::Fn(mut f) => {
                let m = take_member(&mut f.attrs)?;
                let sig = f.sig.clone();
                (m, sig, Some(ImplItem::Fn(f)))
            }
            ImplItem::Verbatim(ts) => match syn::parse2::<BodylessFn>(ts.clone()) {
                Ok(mut b) => {
                    let m = take_member(&mut b.attrs)?;
                    if !matches!(
                        m,
                        Some(Member {
                            kind: Kind::Signal,
                            ..
                        })
                    ) {
                        kept.push(ImplItem::Verbatim(ts));
                        continue;
                    }
                    (m, b.sig, None)
                }
                Err(_) => {
                    kept.push(ImplItem::Verbatim(ts));
                    continue;
                }
            },
            other => {
                kept.push(other);
                continue;
            }
        };
        let Some(member) = member else {
            kept.extend(body_item);
            continue;
        };
        let fn_ident = sig.ident.clone();
        let qml_name = member
            .name
            .clone()
            .unwrap_or_else(|| camel(&fn_ident.to_string()));
        match member.kind {
            Kind::Signal => {
                if body_item.is_some() {
                    return Err(Error::new(
                        fn_ident.span(),
                        "a #[qml(signal)] is declared without a body: `fn name(args);`",
                    ));
                }
                let s = shape(&sig)?;
                if s.cx.is_some() || !matches!(s.receiver, Receiver::None) {
                    return Err(Error::new(
                        sig.inputs.span(),
                        "a signal lists only its arguments",
                    ));
                }
                let params = s
                    .args
                    .iter()
                    .map(|(n, t)| quote!((#n, <#t as #e::QmlValue>::type_tag())));
                signals.push(quote! {
                    #e::SignalDecl { name: #qml_name, params: #p::Vec::from([#(#params),*]) }
                });
                let emit = format_ident!("emit_{}", fn_ident);
                let idents: Vec<Ident> =
                    (0..s.args.len()).map(|i| format_ident!("a{}", i)).collect();
                let tys = s.args.iter().map(|(_, t)| t);
                let doc = format!("Emits `{qml_name}` on `cx.this`.");
                generated.push(quote! {
                    #[doc = #doc]
                    #[allow(clippy::too_many_arguments)]
                    pub fn #emit(
                        cx: &#e::Cx<'_>,
                        #(#idents: #tys),*
                    ) -> #p::Result<(), #e::EngineError> {
                        // Stable in derived types: tables are prefix-flattened.
                        static INDEX: #p::OnceLock<#e::SignalIndex> = #p::OnceLock::new();
                        let meta = <Self as #e::QmlType>::static_meta();
                        let index = *INDEX.get_or_init(|| {
                            meta.signal_index(#qml_name).expect("declared by #[qml_impl]")
                        });
                        cx.engine.emit_typed(
                            cx.this,
                            meta,
                            index,
                            &[#(#e::QmlValue::to_value(&#idents)),*],
                        )
                    }
                });
            }
            Kind::Invokable | Kind::Complete => {
                let Some(body_item) = body_item else {
                    return Err(Error::new(fn_ident.span(), "this method needs a body"));
                };
                kept.push(body_item);
                let s = shape(&sig)?;
                let idents: Vec<Ident> =
                    (0..s.args.len()).map(|i| format_ident!("a{}", i)).collect();
                let call = call(&fn_ident, &s, &idents);
                let ret_ty: Option<&Type> = match &sig.output {
                    ReturnType::Default => None,
                    ReturnType::Type(_, t) => Some(t),
                };
                let ok_ty = ret_ty.and_then(result_ok);
                let unwrap = if ok_ty.is_some() {
                    quote!(let r = r.map_err(::core::convert::Into::<#e::EngineError>::into)?;)
                } else {
                    quote!()
                };
                let value_ty: TokenStream2 = match (ok_ty, ret_ty) {
                    (Some(t), _) => quote!(#t),
                    (None, Some(t)) => quote!(#t),
                    (None, None) => quote!(()),
                };
                if matches!(member.kind, Kind::Complete) {
                    if !s.args.is_empty() {
                        return Err(Error::new(
                            sig.inputs.span(),
                            "#[qml(on_complete)] takes no arguments besides `self` or a `Cx`",
                        ));
                    }
                    if on_complete.is_some() {
                        return Err(Error::new(fn_ident.span(), "only one #[qml(on_complete)]"));
                    }
                    let thunk = format_ident!("__qml_complete_{}", fn_ident);
                    generated.push(quote! {
                        #[doc(hidden)]
                        fn #thunk(cx: &#e::Cx<'_>) -> #p::Result<(), #e::EngineError> {
                            #[allow(clippy::let_unit_value, unused_variables)]
                            let r = #call;
                            #unwrap
                            let _ = r;
                            #p::Result::Ok(())
                        }
                    });
                    on_complete = Some(quote!(Self::#thunk));
                    continue;
                }
                let thunk = format_ident!("__qml_thunk_{}", fn_ident);
                let tys: Vec<&Type> = s.args.iter().map(|(_, t)| t).collect();
                let names = s.args.iter().map(|(n, _)| n);
                let method = format!("{}.{qml_name}", type_name(&self_ty));
                let conv = idents
                    .iter()
                    .zip(&s.args)
                    .enumerate()
                    .map(|(i, (id, (n, t)))| {
                        quote! {
                            let #id: #t = <#t as #e::QmlValue>::from_value(#p::arg(args, #i))
                                .map_err(|e| e.in_call(#method, #n))?;
                        }
                    });
                generated.push(quote! {
                    #[doc(hidden)]
                    fn #thunk(
                        cx: &#e::Cx<'_>,
                        args: &[#e::Value],
                    ) -> #p::Result<#e::Value, #e::EngineError> {
                        let _ = args;
                        #(#conv)*
                        #[allow(clippy::let_unit_value)]
                        let r = #call;
                        #unwrap
                        #p::Result::Ok(<#value_ty as #e::QmlValue>::to_value(&r))
                    }
                });
                methods.push(quote! {
                    #e::MethodDecl {
                        name: #qml_name,
                        params: #p::Vec::from([#((#names, <#tys as #e::QmlValue>::type_tag())),*]),
                        ret: <#value_ty as #e::QmlValue>::type_tag(),
                        thunk: Self::#thunk,
                    }
                });
            }
        }
    }
    item.items = kept;
    let on_complete = match on_complete {
        Some(t) => quote!(#p::Option::Some(#t)),
        None => quote!(#p::Option::None),
    };
    Ok(quote! {
        #item

        #[allow(dead_code)]
        impl #self_ty {
            #(#generated)*
        }

        impl #e::QmlImplInfo for #self_ty {
            fn impl_info() -> #e::ImplInfo {
                #e::ImplInfo {
                    signals: #p::Vec::from([#(#signals),*]),
                    methods: #p::Vec::from([#(#methods),*]),
                    on_complete: #on_complete,
                }
            }
        }
    })
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
