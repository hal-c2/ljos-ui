//! `#[derive(QmlObject)]`: a Rust struct as a QML type.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::{Attribute, Data, DeriveInput, Error, Fields, Ident, LitStr, Result, Type};

use crate::{camel, engine_path, is_qml};

#[derive(Default)]
struct ContainerAttrs {
    name: Option<String>,
    module: Option<String>,
    extends: Option<Type>,
    default: Option<String>,
    attached: Option<Type>,
    uncreatable: bool,
    plain_object: bool,
    /// `block`: a functional block; `category` and `summary` are what a
    /// palette shows of it.
    block: bool,
    category: Option<String>,
    summary: Option<String>,
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
    /// `lazy = path`: what an unset lazy group is.
    lazy_default: Option<syn::Path>,
    list: bool,
    append: Option<syn::Path>,
    insert: Option<syn::Path>,
    remove: Option<syn::Path>,
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
    /// `given = path`: told when the property is given a value, a binding
    /// or an override.
    given: Option<syn::Path>,
    /// `change = path`: told when the property changes, however it does.
    change: Option<syn::Path>,
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
            } else if m.path.is_ident("block") {
                c.block = true;
            } else if m.path.is_ident("category") {
                c.category = Some(m.value()?.parse::<LitStr>()?.value());
            } else if m.path.is_ident("summary") {
                c.summary = Some(m.value()?.parse::<LitStr>()?.value());
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
                    "expected one of: name, module, extends, default, attached, uncreatable, plain_object, block, category, summary, enums, reset_inherited",
                ));
            }
            Ok(())
        })?;
        if !c.block && (c.category.is_some() || c.summary.is_some()) {
            return Err(Error::new(
                a.span(),
                "`category` and `summary` describe a block: add `block`",
            ));
        }
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
                if m.input.peek(syn::Token![=]) {
                    f.lazy_default = Some(m.value()?.parse::<syn::Path>()?);
                }
            } else if p.is_ident("list") {
                f.list = true;
            } else if p.is_ident("append") {
                f.append = Some(m.value()?.parse::<syn::Path>()?);
            } else if p.is_ident("insert") {
                f.insert = Some(m.value()?.parse::<syn::Path>()?);
            } else if p.is_ident("remove") {
                f.remove = Some(m.value()?.parse::<syn::Path>()?);
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
            } else if p.is_ident("given") {
                f.given = Some(m.value()?.parse::<syn::Path>()?);
            } else if p.is_ident("change") {
                f.change = Some(m.value()?.parse::<syn::Path>()?);
            } else if p.is_ident("reset") {
                f.reset = Some(if m.input.peek(syn::Token![=]) {
                    Some(m.value()?.parse::<syn::Expr>()?)
                } else {
                    None
                });
            } else {
                return Err(m.error(
                    "expected one of: base, notify, readonly, required, constant, group, lazy, list, append, insert, remove, skip, rename, enumeration, flags, reset, object, holds, write, given, change",
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

pub(crate) fn derive(input: &DeriveInput) -> Result<TokenStream2> {
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
        if a.remove.is_some() && a.append.is_none() {
            return Err(Error::new(
                ident.span(),
                "`remove` needs `append`: a list without an append hook is taken from as it is",
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
            if matches!(a.reset, Some(Some(_))) {
                return Err(Error::new(
                    ident.span(),
                    "a group is taken back whole by a plain `reset`; put `reset = expr` on the group type's fields",
                ));
            }
            if a.write.is_some() {
                return Err(Error::new(
                    ident.span(),
                    "a group is not written whole; put `write` on the group type's fields",
                ));
            }
            if a.given.is_some() {
                return Err(Error::new(
                    ident.span(),
                    "a group is not given whole; put `given` on the group type's fields",
                ));
            }
            if a.change.is_some() {
                return Err(Error::new(
                    ident.span(),
                    "a group does not change whole; put `change` on the group type's fields",
                ));
            }
            let get = format_ident!("__qml_group_{}", ident);
            let get_mut = format_ident!("__qml_group_mut_{}", ident);
            let fty = if a.lazy {
                let inner = boxed_inner(fty).ok_or_else(|| {
                    Error::new(fty.span(), "a `lazy` group is an `Option<Box<T>>` field")
                })?;
                let unset = match &a.lazy_default {
                    Some(path) => self_to(path, ty),
                    None => quote!(<#inner as ::std::default::Default>::default),
                };
                accessors.push(quote! {
                    fn #get(o: &dyn #e::QmlObject) -> &dyn #e::QmlObject {
                        static UNSET: ::std::sync::OnceLock<#inner> = ::std::sync::OnceLock::new();
                        match &#p::downcast::<#ty>(o).#ident {
                            ::std::option::Option::Some(g) => &**g,
                            ::std::option::Option::None => UNSET.get_or_init(#unset),
                        }
                    }
                    fn #get_mut(o: &mut dyn #e::QmlObject) -> &mut dyn #e::QmlObject {
                        &mut **#p::downcast_mut::<#ty>(o).#ident.get_or_insert_with(
                            || #p::Box::new(#unset()),
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
            if a.reset.is_some() {
                builder.push(quote!(.whole_group(#name)));
            }
            let cname = format_ident!(
                "{}",
                ident.to_string().trim_start_matches('_').to_uppercase()
            );
            let doc = format!(
                "The `{name}` group; each member follows it, at its index in the group plus one."
            );
            consts.push(quote! {
                #[doc = #doc]
                pub const #cname: #e::Prop<#fty> =
                    #e::Prop::new(#index, <#ty as #e::QmlType>::static_meta);
            });
            count.push(quote!((1 + <#fty as #e::QmlType>::PROPERTY_COUNT)));
            continue;
        }

        let get = format_ident!("__qml_get_{}", ident);
        let set = format_ident!("__qml_set_{}", ident);
        let notify = a.notify;
        // An enum's number reads as Qt's does: unsigned unless a key is
        // negative. Reads are many, so the enum is asked once, and only
        // when the number is negative.
        let read = if let Some(enumeration) = a.enumeration.as_ref().filter(|_| !a.flags) {
            quote! {{
                static SIGNED: ::std::sync::OnceLock<bool> = ::std::sync::OnceLock::new();
                let n: i32 = #p::downcast::<#ty>(o).#ident;
                #e::Value::of_enum_field(
                    n,
                    n >= 0
                        || *SIGNED.get_or_init(|| {
                            <#ty as #e::QmlType>::static_meta().enum_signed(#enumeration)
                        }),
                )
            }}
        } else {
            quote!(#e::QmlValue::to_value(&#p::downcast::<#ty>(o).#ident))
        };
        accessors.push(quote! {
            fn #get(o: &dyn #e::QmlObject) -> #e::Value {
                #read
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
        if let Some(remove) = &a.remove {
            builder.push(quote!(.list_remove(#remove)));
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
        if let Some(path) = &a.given {
            let path = self_to(path, ty);
            builder.push(quote!(.given(#path)));
        }
        if let Some(path) = &a.change {
            let path = self_to(path, ty);
            builder.push(quote!(.change(#path)));
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
            fn part(
                &self,
                ty: ::core::any::TypeId,
            ) -> #p::Option<#p::Option<&dyn #e::QmlObject>> {
                if ty == ::core::any::TypeId::of::<Self>() {
                    return #p::Option::Some(#p::Option::Some(self));
                }
                // The base's own, called as its type's: no call through
                // the vtable for each part.
                #p::Option::Some(
                    #e::QmlObject::part(&self.#f, ty)
                        .unwrap_or_else(|| #e::find_native(&self.#f, ty)),
                )
            }
            fn answers_parts(&self) -> bool {
                true
            }
            fn part_mut(
                &mut self,
                ty: ::core::any::TypeId,
            ) -> #p::Option<&mut dyn #e::QmlObject> {
                if ty == ::core::any::TypeId::of::<Self>() {
                    return #p::Option::Some(self);
                }
                if #e::QmlObject::answers_parts(&self.#f) {
                    #e::QmlObject::part_mut(&mut self.#f, ty)
                } else {
                    #e::find_native_mut(&mut self.#f, ty)
                }
            }
        },
        None => quote! {
            fn part(
                &self,
                ty: ::core::any::TypeId,
            ) -> #p::Option<#p::Option<&dyn #e::QmlObject>> {
                #p::Option::Some(
                    (ty == ::core::any::TypeId::of::<Self>()).then_some(self as &dyn #e::QmlObject),
                )
            }
            fn answers_parts(&self) -> bool {
                true
            }
            fn part_mut(
                &mut self,
                ty: ::core::any::TypeId,
            ) -> #p::Option<&mut dyn #e::QmlObject> {
                (ty == ::core::any::TypeId::of::<Self>()).then_some(self as &mut dyn #e::QmlObject)
            }
        },
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
    let block_call = c.block.then(|| {
        let some = |v: &Option<String>| match v {
            Some(v) => quote!(#p::Option::Some(#v)),
            None => quote!(#p::Option::None),
        };
        let (category, summary) = (some(&c.category), some(&c.summary));
        quote!(.block(#category, #summary))
    });
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
                        #block_call
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
