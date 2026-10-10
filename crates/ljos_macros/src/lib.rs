//! Proc-macros for `ljos_engine`: `#[derive(QmlObject)]` and `#[qml_impl]`.
//!
//! Generated code names everything through `::ljos_engine`, so the macros
//! are only usable together with that crate (which re-exports them).
//!
//! # `#[derive(QmlObject)]`
//!
//! Container attributes, `#[qml(...)]`:
//!
//! - `name = "Rectangle"`: the QML type name (default: the struct name);
//! - `module = "QtQuick"`: the exporting module, for the type dump;
//! - `extends = Item`: the base type. It must match the `#[qml(base)]`
//!   field, which is how the base's part of the object is stored;
//! - `default = "data"`: the default property;
//! - `attached = KeysAttached`: the type `Name.` attaches;
//! - `uncreatable`: not creatable from QML. Otherwise the type must
//!   implement `Default`, which is how the engine creates it.
//!
//! - `enums = path`: a `fn() -> Vec<(&'static str, Vec<(&'static str, i64)>)>`
//!   listing the type's enums (`Component.Ready`).
//!
//! Field attributes: `base`, `notify` (adds `nameChanged`), `readonly`,
//! `required`, `constant`, `group` (a grouped property; the field's type
//! derives `QmlObject` too), `list` (a QML `list<T>` property, on a
//! `QmlList` field), `append = path` (with `list`: a
//! `fn(&Cx, ObjectId) -> Result<(), EngineError>` every append goes
//! through), `skip`, `rename = "name"`, `enumeration = "Name"` (on an `i32`
//! field: the property holds a key of that enum of the type, one of those
//! `enums` lists or an inherited one; QML may then give the key by name, and
//! what is neither a key's name nor a number is refused). Field names are
//! exposed in camelCase unless renamed.
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
    enums: Option<syn::Path>,
}

#[derive(Default)]
struct FieldAttrs {
    base: bool,
    notify: bool,
    readonly: bool,
    required: bool,
    constant: bool,
    group: bool,
    list: bool,
    append: Option<syn::Path>,
    skip: bool,
    rename: Option<String>,
    enumeration: Option<String>,
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
            } else if m.path.is_ident("enums") {
                c.enums = Some(m.value()?.parse::<syn::Path>()?);
            } else {
                return Err(m.error(
                    "expected one of: name, module, extends, default, attached, uncreatable, enums",
                ));
            }
            Ok(())
        })?;
    }
    Ok(c)
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
            } else if p.is_ident("list") {
                f.list = true;
            } else if p.is_ident("append") {
                f.append = Some(m.value()?.parse::<syn::Path>()?);
            } else if p.is_ident("skip") {
                f.skip = true;
            } else if p.is_ident("rename") {
                f.rename = Some(m.value()?.parse::<LitStr>()?.value());
            } else if p.is_ident("enumeration") {
                f.enumeration = Some(m.value()?.parse::<LitStr>()?.value());
            } else {
                return Err(m.error(
                    "expected one of: base, notify, readonly, required, constant, group, list, append, skip, rename, enumeration",
                ));
            }
            Ok(())
        })?;
    }
    Ok(f)
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
    let e = quote!(::ljos_engine);
    let p = quote!(::ljos_engine::__private);
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
    let base_count = match &base {
        Some((_, b)) => quote!(<#b as #e::QmlType>::PROPERTY_COUNT),
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
        let flags = quote!(#e::PropertyFlags::empty() #(.union(#flags))*);
        let index = quote!(#base_count #(+ #count)*);

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
            let get = format_ident!("__qml_group_{}", ident);
            let get_mut = format_ident!("__qml_group_mut_{}", ident);
            accessors.push(quote! {
                fn #get(o: &dyn #e::QmlObject) -> &dyn #e::QmlObject {
                    &#p::downcast::<#ty>(o).#ident
                }
                fn #get_mut(o: &mut dyn #e::QmlObject) -> &mut dyn #e::QmlObject {
                    &mut #p::downcast_mut::<#ty>(o).#ident
                }
            });
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
                quote!(#e::TypeTag::Enum(#e::Atom::intern(#enumeration)))
            }
            None => quote!(<#fty as #e::QmlValue>::type_tag()),
        };
        builder.push(quote! {
            .field(#name, #tag, #flags, #notify, #get, #set)
        });
        if let Some(append) = &a.append {
            builder.push(quote!(.list_append(#append)));
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
    let attached_call = c
        .attached
        .as_ref()
        .map(|a| quote!(.attached(|| #e::MetaRef::Static(<#a as #e::QmlType>::static_meta()))));
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
                        #default_call
                        #enums_call
                        #attached_call
                        #create_call
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
    let e = quote!(::ljos_engine);
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
    let e = quote!(::ljos_engine);
    let p = quote!(::ljos_engine::__private);
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
                let conv = idents.iter().zip(&tys).enumerate().map(|(i, (id, t))| {
                    quote! {
                        let #id: #t = <#t as #e::QmlValue>::from_value(#p::arg(args, #i))?;
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
