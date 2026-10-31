//! The web backend: what `ljos_web` needs of a type, beside the engine's
//! tables (docs/LJOS_WEB.md A22 of ljos-web). Never a second reading of the
//! attributes: the native pass has validated them by the time this runs.

use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::{
    Data, DeriveInput, Error, Fields, Ident, ImplItem, ItemImpl, LitByteStr, Result, ReturnType,
    Type,
};

use crate::camel;
use crate::derive::{boxed_inner, container_attrs, field_attrs, self_to};
use crate::qml_impl::{BodylessFn, Kind, Shape, call, result_ok, shape, take_member};

/// The path to `ljos_web` as the crate being compiled reaches it. Only with
/// the feature on and through the facade: a crate of the engine's own has no
/// face.
fn web_path() -> Option<TokenStream2> {
    if !cfg!(feature = "web") {
        return None;
    }
    match (crate_name("ljos_engine"), crate_name("ljos")) {
        (Err(_), Ok(FoundCrate::Name(n))) => {
            let n = Ident::new(&n, Span::call_site());
            Some(quote!(::#n::ljos_web))
        }
        _ => None,
    }
}

/// `native` alone, or with the feature on each behind its target.
pub(crate) fn beside(
    native: TokenStream2,
    web: impl FnOnce(&TokenStream2) -> Result<TokenStream2>,
) -> Result<TokenStream2> {
    match web_path() {
        None => Ok(native),
        Some(w) => joined(native, web(&w)?),
    }
}

fn joined(native: TokenStream2, web: TokenStream2) -> Result<TokenStream2> {
    let native = gated(native, quote!(#[cfg(not(target_arch = "wasm32"))]))?;
    let web = gated(web, quote!(#[cfg(target_arch = "wasm32")]))?;
    Ok(quote!(#native #web))
}

/// The attribute on each of the items, as one can't go on all of them.
fn gated(items: TokenStream2, cfg: TokenStream2) -> Result<TokenStream2> {
    let file: syn::File = syn::parse2(items)?;
    let items = file.items;
    Ok(quote!(#(#cfg #items)*))
}

fn json(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_opt(s: Option<&str>) -> String {
    s.map_or_else(|| "null".to_owned(), json)
}

/// A line of the wasm's `ljos_web_types` section, which the tool reads
/// instead of parsing Rust. A macro cannot see the rest of the crate, so each
/// type writes its own and the linker puts them together.
fn metadata(line: &str) -> TokenStream2 {
    let mut bytes = line.as_bytes().to_vec();
    bytes.push(b'\n');
    let len = bytes.len();
    let bytes = LitByteStr::new(&bytes, Span::call_site());
    quote! {
        const _: () = {
            #[used]
            #[unsafe(link_section = "ljos_web_types")]
            static LJOS_WEB_TYPE: [u8; #len] = *#bytes;
        };
    }
}

fn last_segment(t: &Type) -> Option<String> {
    match t {
        Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    }
}

/// The kernel's kind, by how the type is spelled (A22): a macro sees no
/// alias, and one that hides an `i32` is a `var`, which still works.
fn kind_of(t: &Type) -> &'static str {
    match last_segment(t).as_deref() {
        Some("i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32") => "int",
        Some("f32" | "f64") => "real",
        Some("bool") => "bool",
        Some("String" | "Atom") => "string",
        _ => "var",
    }
}

fn wrapper_of(ty: &Ident) -> Ident {
    format_ident!("__LjosWeb_{}", ty)
}

pub(crate) fn derive(input: &DeriveInput, w: &TokenStream2) -> Result<TokenStream2> {
    let Data::Struct(data) = &input.data else {
        return Err(Error::new(input.ident.span(), "needs a struct"));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(Error::new(input.ident.span(), "needs named fields"));
    };
    let c = container_attrs(&input.attrs)?;
    let ty = &input.ident;
    let wrapper = wrapper_of(ty);
    let holder = format_ident!("__ljos_web_{}", ty);
    let qml_name = c.name.clone().unwrap_or_else(|| ty.to_string());
    // The wasm-bindgen class goes by the Rust name, the one `#[qml_impl]` can
    // see too; the face gives it its QML name.
    let rust_name = ty.to_string();
    let prv = quote!(#w::__private);
    let bindgen = quote!(#prv::wasm_bindgen);
    let js = quote!(#prv::JsValue);
    let path = quote!(wasm_bindgen = #w::wasm_bindgen);

    let mut base_name = None;
    let mut base_check = None;
    let mut consts = Vec::new();
    let mut methods = Vec::new();
    let mut members = Vec::new();
    let mut reads = Vec::new();
    let mut fields_get = Vec::new();
    let mut fields_set = Vec::new();
    let mut fields_all = Vec::new();
    let mut group_get = Vec::new();
    let mut group_set = Vec::new();
    let mut props = Vec::new();
    let mut refused = Vec::new();
    let mut index = 0u32;

    for field in &fields.named {
        let a = field_attrs(&field.attrs)?;
        if a.skip {
            continue;
        }
        let ident = field.ident.clone().expect("named field");
        let fty = &field.ty;
        if a.base {
            base_name = last_segment(fty);
            base_check = Some(quote! {
                const _: fn() = || {
                    fn is_a_base<T: #w::WebBase>() {}
                    is_a_base::<#fty>();
                };
            });
            continue;
        }
        let name = a
            .rename
            .clone()
            .unwrap_or_else(|| camel(&ident.to_string()));
        let constant = format_ident!(
            "{}",
            ident.to_string().trim_start_matches('_').to_uppercase()
        );
        let at = index;
        index += 1;
        if a.list || a.append.is_some() || a.insert.is_some() || a.remove.is_some() {
            let said = format!(
                "`{ty}.{ident}` is a #[qml(list)], which has no web counterpart yet: a Vec<T> property crosses as an array (docs/LJOS_WEB.md A22)"
            );
            refused.push(quote!(
                const _: () = ::core::compile_error!(#said);
            ));
            continue;
        }
        if a.group {
            let inner = if a.lazy { boxed_inner(fty) } else { None };
            let shown = inner.as_ref().unwrap_or(fty);
            consts
                .push(quote!(pub const #constant: #w::Prop<#shown> = #w::Prop::slot(#name, #at);));
            let lent = if a.lazy {
                quote!(this.#ident.as_deref())
            } else {
                quote!(::core::option::Option::Some(&this.#ident))
            };
            group_get.push(quote! {
                #name => cx.with::<#ty, _>(|this| {
                    #lent.and_then(|group| #prv::WebFields::web_get(group, member))
                })?,
            });
            let lent_mut = if a.lazy {
                quote!(&mut **this.#ident.get_or_insert_with(::core::default::Default::default))
            } else {
                quote!(&mut this.#ident)
            };
            group_set.push(quote! {
                #name => cx.with_mut::<#ty, _>(|this| {
                    #prv::WebFields::web_set(#lent_mut, member, &value)
                })??,
            });
            let whole = if a.lazy {
                quote!(self.#ident.as_deref().map_or(#js::NULL, #prv::WebFields::web_all))
            } else {
                quote!(#prv::WebFields::web_all(&self.#ident))
            };
            reads.push(quote!(#at => #whole,));
            props.push(format!(
                "{{\"name\":{},\"group\":true,\"of\":{},\"lazy\":{}}}",
                json(&name),
                json_opt(last_segment(shown).as_deref()),
                a.lazy,
            ));
            continue;
        }
        let get = format_ident!("__web_get_{}", ident);
        let set = format_ident!("__web_set_{}", ident);
        let getter = format_ident!("__get_{}", ident);
        let setter = format_ident!("__set_{}", ident);
        consts.push(quote! {
            pub const #constant: #w::Prop<#fty> = #w::Prop::own(#name, #at, Self::#get, Self::#set);
        });
        let filter = a.write.as_ref().map(|path| {
            let path = self_to(path, ty);
            quote! {
                let ::core::option::Option::Some(value) =
                    #prv::WriteFn::<#ty, #fty, _>::call(&#path, cx, value)?
                else {
                    return ::core::result::Result::Ok(());
                };
            }
        });
        let told = (!a.constant).then(|| {
            quote! { #prv::tell(cx.this, #at, #w::QmlValue::to_js(&told)); }
        });
        let change = a.change.as_ref().map(|path| {
            let path = self_to(path, ty);
            quote! { #path(cx, #w::PropIndex(#at)); }
        });
        members.push(quote! {
            #[doc(hidden)]
            fn #get(cx: &#w::Cx<'_>) -> ::core::result::Result<#fty, #w::EngineError> {
                cx.with::<Self, _>(|this| ::core::clone::Clone::clone(&this.#ident))
            }
            #[doc(hidden)]
            fn #set(cx: &#w::Cx<'_>, value: #fty) -> ::core::result::Result<(), #w::EngineError> {
                let _call = #prv::Call::enter();
                #filter
                let told = ::core::clone::Clone::clone(&value);
                let changed = cx.with_mut::<Self, _>(|this| {
                    let changed = !#w::QmlValue::same(&this.#ident, &value);
                    if changed {
                        this.#ident = value;
                    }
                    changed
                })?;
                if changed {
                    #told
                    #change
                }
                ::core::result::Result::Ok(())
            }
        });
        methods.push(quote! {
            #[wasm_bindgen(getter = #name)]
            pub fn #getter(&self) -> ::core::result::Result<<#fty as #w::QmlValue>::Wire, #js> {
                let value = <#ty>::#get(&#w::Cx::new(&#w::ENGINE, self.id))?;
                ::core::result::Result::Ok(#w::QmlValue::into_wire(value))
            }
        });
        if !a.readonly {
            let given = a.given.as_ref().map(|path| {
                let path = self_to(path, ty);
                quote! { #path(&cx, #w::PropIndex(#at)); }
            });
            methods.push(quote! {
                #[wasm_bindgen(setter = #name)]
                pub fn #setter(
                    &self,
                    value: <#fty as #w::QmlValue>::Wire,
                ) -> ::core::result::Result<(), #js> {
                    let _call = #prv::Call::enter();
                    let cx = #w::Cx::new(&#w::ENGINE, self.id);
                    let value = <#fty as #w::QmlValue>::from_wire(value)
                        .map_err(#w::EngineError::from)?;
                    <#ty>::#set(&cx, value)?;
                    #given
                    ::core::result::Result::Ok(())
                }
            });
        }
        reads.push(quote!(#at => #w::QmlValue::to_js(&self.#ident),));
        fields_get.push(
            quote!(#name => ::core::option::Option::Some(#w::QmlValue::to_js(&self.#ident)),),
        );
        fields_set.push(quote! {
            #name => {
                let value = <#fty as #w::QmlValue>::from_js(value)?;
                let changed = !#w::QmlValue::same(&self.#ident, &value);
                if changed {
                    self.#ident = value;
                }
                ::core::result::Result::Ok(changed)
            }
        });
        fields_all.push(quote! {
            let _ = #prv::js_sys::Reflect::set(
                &all,
                &#js::from_str(#name),
                &#w::QmlValue::to_js(&self.#ident),
            );
        });
        props.push(format!(
            "{{\"name\":{},\"kind\":{},\"readonly\":{},\"constant\":{},\"notify\":{},\"required\":{},\"write\":{},\"enum\":{},\"flags\":{},\"object\":{}}}",
            json(&name),
            json(kind_of(fty)),
            a.readonly,
            a.constant,
            a.notify,
            a.required,
            a.write.is_some(),
            json_opt(a.enumeration.as_deref()),
            a.flags,
            json_opt(a.object.as_deref()),
        ));
    }

    // QML can't write an `uncreatable` type, but the runtime still makes one,
    // as attached object or singleton, and only if it has a `Default`.
    let default = if c.uncreatable {
        quote!({
            #[allow(unused_imports)]
            use #prv::NoDefault as _;
            #prv::DefaultOf::<#ty>::new().get()
        })
    } else {
        quote!(::core::option::Option::Some(<#ty as ::core::default::Default>::default))
    };
    let enums = c
        .enums
        .as_ref()
        .map_or_else(|| quote!(#js::NULL), |f| quote!(#prv::enums_to_js(#f())));
    let base = c
        .extends
        .as_ref()
        .and_then(last_segment)
        .or(base_name)
        .unwrap_or_else(|| "QtObject".to_owned());
    let line = metadata(&format!(
        "{{\"rust\":{},\"name\":{},\"module\":{},\"base\":{},\"uncreatable\":{},\"default\":{},\"attached\":{},\"enums\":{},\"props\":[{}]}}",
        json(&ty.to_string()),
        json(&qml_name),
        json_opt(c.module.as_deref()),
        json(&base),
        c.uncreatable,
        json_opt(c.default.as_deref()),
        json_opt(c.attached.as_ref().and_then(last_segment).as_deref()),
        c.enums.is_some(),
        props.join(","),
    ));

    Ok(quote! {
        #(#refused)*
        #base_check
        #line

        // wasm-bindgen's struct derive drops the `wasm_bindgen = path` option
        // and names a bare `wasm_bindgen`, so an app that doesn't depend on it
        // needs that name in scope; a module keeps it out of the app's own.
        #[allow(non_snake_case)]
        #[doc(hidden)]
        mod #holder {
            use #w::wasm_bindgen;

            #[allow(non_camel_case_types, dead_code)]
            #[doc(hidden)]
            #[#bindgen(js_name = #rust_name, #path)]
            pub struct #wrapper {
                pub(super) id: #w::ObjectId,
            }
        }
        #[doc(hidden)]
        pub use #holder::#wrapper;

        impl ::core::ops::Drop for #wrapper {
            fn drop(&mut self) {
                #prv::free(self.id);
            }
        }

        #[allow(clippy::new_without_default, missing_docs, non_snake_case)]
        #[#bindgen(js_class = #rust_name, #path)]
        impl #wrapper {
            #[wasm_bindgen(constructor)]
            pub fn __new() -> ::core::result::Result<#wrapper, #js> {
                let id = #prv::make::<#ty>(#default)?;
                ::core::result::Result::Ok(Self { id })
            }
            #[wasm_bindgen(js_name = __ljos_from_id)]
            pub fn __from_id(id: u32) -> #wrapper {
                Self { id: #prv::by_id(id) }
            }
            #[wasm_bindgen(getter = __ljos_id)]
            pub fn __id(&self) -> u32 {
                #prv::self_id(self.id)
            }
            #[wasm_bindgen(js_name = __ljos_bind)]
            pub fn __bind(&self, face: #js) {
                #prv::bind(self.id, face);
            }
            #[wasm_bindgen(js_name = __ljos_listen)]
            pub fn __listen(&self, listener: #prv::js_sys::Function) {
                #prv::listen(self.id, listener);
            }
            #[wasm_bindgen(js_name = __ljos_hooks)]
            pub fn __hooks(&self) -> u32 {
                #prv::hooks_of(self.id)
            }
            #[wasm_bindgen(js_name = __ljos_enums)]
            pub fn __enums() -> #js {
                #enums
            }
            #[wasm_bindgen(js_name = __ljos_group_get)]
            pub fn __group_get(&self, group: &str, member: &str) -> ::core::result::Result<#js, #js> {
                let cx = #w::Cx::new(&#w::ENGINE, self.id);
                let got = match group {
                    #(#group_get)*
                    _ => ::core::option::Option::None,
                };
                ::core::result::Result::Ok(got.unwrap_or(#js::UNDEFINED))
            }
            #[wasm_bindgen(js_name = __ljos_group_set)]
            pub fn __group_set(
                &self,
                group: &str,
                member: &str,
                value: #js,
            ) -> ::core::result::Result<bool, #js> {
                let _call = #prv::Call::enter();
                let cx = #w::Cx::new(&#w::ENGINE, self.id);
                ::core::result::Result::Ok(match group {
                    #(#group_set)*
                    _ => false,
                })
            }
            #(#methods)*
        }

        #[allow(dead_code, non_snake_case)]
        impl #ty {
            #(#consts)*
            #(#members)*
        }

        impl #w::QmlType for #ty {
            const NAME: &'static str = #qml_name;
            fn web_read(&self, index: u32) -> #js {
                match index {
                    #(#reads)*
                    _ => #js::UNDEFINED,
                }
            }
        }

        impl #prv::WebFields for #ty {
            fn web_get(&self, name: &str) -> ::core::option::Option<#js> {
                match name {
                    #(#fields_get)*
                    _ => ::core::option::Option::None,
                }
            }
            fn web_set(
                &mut self,
                name: &str,
                value: &#js,
            ) -> ::core::result::Result<bool, #w::EngineError> {
                let _ = &value;
                match name {
                    #(#fields_set)*
                    _ => ::core::result::Result::Ok(false),
                }
            }
            fn web_all(&self) -> #js {
                let all = #prv::js_sys::Object::new();
                #(#fields_all)*
                all.into()
            }
        }
    })
}

pub(crate) fn singleton(name: &Ident, which: &TokenStream2, w: &TokenStream2) -> TokenStream2 {
    let scope = which.to_string().to_lowercase();
    let line = metadata(&format!(
        "{{\"singleton\":{},\"scope\":{}}}",
        json(&name.to_string()),
        json(&scope)
    ));
    quote! {
        #line
        impl #w::QmlSingleton for #name {
            const SCOPE: #w::SingletonScope = #w::SingletonScope::#which;
        }
    }
}

fn parameters(s: &Shape) -> String {
    let each: Vec<String> = s
        .args
        .iter()
        .map(|(n, t)| format!("{{\"name\":{},\"type\":{}}}", json(n), json(kind_of(t))))
        .collect();
    each.join(",")
}

pub(crate) fn qml_impl(mut item: ItemImpl, w: &TokenStream2) -> Result<TokenStream2> {
    let self_ty = item.self_ty.clone();
    let Type::Path(self_path) = &*self_ty else {
        return Err(Error::new(
            Span::call_site(),
            "#[qml_impl] needs a named type",
        ));
    };
    let mut wrapper = self_path.path.clone();
    let last = wrapper.segments.last_mut().expect("a path has a segment");
    let rust_name = last.ident.to_string();
    last.ident = wrapper_of(&last.ident);

    let prv = quote!(#w::__private);
    let bindgen = quote!(#prv::wasm_bindgen);
    let js = quote!(#prv::JsValue);
    let mut on_type = Vec::new();
    let mut on_wrapper = Vec::new();
    let mut signals = Vec::new();
    let mut methods = Vec::new();
    let mut complete = false;

    for it in std::mem::take(&mut item.items) {
        let (member, sig, has_body) = match it {
            ImplItem::Fn(mut f) => (take_member(&mut f.attrs)?, f.sig, true),
            ImplItem::Verbatim(ts) => match syn::parse2::<BodylessFn>(ts) {
                Ok(mut b) => (take_member(&mut b.attrs)?, b.sig, false),
                Err(_) => continue,
            },
            _ => continue,
        };
        let Some(member) = member else { continue };
        let fn_ident = sig.ident.clone();
        let qml_name = member
            .name
            .clone()
            .unwrap_or_else(|| camel(&fn_ident.to_string()));
        let s = shape(&sig)?;
        let idents: Vec<Ident> = (0..s.args.len()).map(|i| format_ident!("a{}", i)).collect();
        let tys: Vec<&Type> = s.args.iter().map(|(_, t)| t).collect();
        if matches!(member.kind, Kind::Signal) {
            let emit = format_ident!("emit_{}", fn_ident);
            signals.push(format!(
                "{{\"name\":{},\"parameters\":[{}]}}",
                json(&qml_name),
                parameters(&s)
            ));
            on_type.push(quote! {
                #[allow(clippy::too_many_arguments)]
                pub fn #emit(
                    cx: &#w::Cx<'_>,
                    #(#idents: #tys),*
                ) -> ::core::result::Result<(), #w::EngineError> {
                    #prv::emit(cx, #qml_name, &[#(#w::QmlValue::to_js(&#idents)),*])
                }
            });
            continue;
        }
        if !has_body {
            continue;
        }
        let thunk = format_ident!("__web_thunk_{}", fn_ident);
        let shim = format_ident!("__call_{}", fn_ident);
        let invoked = call(&fn_ident, &s, &idents, w);
        let ret_ty: Option<&Type> = match &sig.output {
            ReturnType::Default => None,
            ReturnType::Type(_, t) => Some(&**t),
        };
        let ok_ty = ret_ty.and_then(result_ok);
        let unwrap = ok_ty
            .map(|_| quote!(let r = r.map_err(::core::convert::Into::<#w::EngineError>::into)?;));
        let value_ty = match (ok_ty, ret_ty) {
            (Some(t), _) | (None, Some(t)) => quote!(#t),
            (None, None) => quote!(()),
        };
        on_type.push(quote! {
            #[doc(hidden)]
            #[allow(clippy::too_many_arguments, clippy::let_unit_value)]
            pub fn #thunk(
                cx: &#w::Cx<'_>,
                #(#idents: #tys),*
            ) -> ::core::result::Result<#value_ty, #w::EngineError> {
                let r = #invoked;
                #unwrap
                ::core::result::Result::Ok(r)
            }
        });
        if matches!(member.kind, Kind::Complete) {
            complete = true;
            on_wrapper.push(quote! {
                #[wasm_bindgen(js_name = __ljos_complete)]
                pub fn #shim(&self) -> ::core::result::Result<(), #js> {
                    let _call = #prv::Call::enter();
                    <#self_ty>::#thunk(&#w::Cx::new(&#w::ENGINE, self.id))?;
                    ::core::result::Result::Ok(())
                }
            });
            continue;
        }
        let returns = ok_ty
            .or(ret_ty)
            .filter(|t| !matches!(t, Type::Tuple(unit) if unit.elems.is_empty()));
        methods.push(format!(
            "{{\"name\":{},\"parameters\":[{}],\"returns\":{}}}",
            json(&qml_name),
            parameters(&s),
            returns.map_or("null".to_string(), |t| json(kind_of(t)))
        ));
        let converted = idents.iter().zip(&tys).map(|(id, t)| {
            quote! {
                let #id = <#t as #w::QmlValue>::from_wire(#id).map_err(#w::EngineError::from)?;
            }
        });
        on_wrapper.push(quote! {
            #[wasm_bindgen(js_name = #qml_name)]
            #[allow(clippy::too_many_arguments)]
            pub fn #shim(
                &self,
                #(#idents: <#tys as #w::QmlValue>::Wire),*
            ) -> ::core::result::Result<<#value_ty as #w::QmlValue>::Wire, #js> {
                let _call = #prv::Call::enter();
                #(#converted)*
                let r = <#self_ty>::#thunk(&#w::Cx::new(&#w::ENGINE, self.id) #(, #idents)*)?;
                ::core::result::Result::Ok(#w::QmlValue::into_wire(r))
            }
        });
    }
    let line = metadata(&format!(
        "{{\"impl\":{},\"signals\":[{}],\"methods\":[{}],\"complete\":{}}}",
        json(&rust_name),
        signals.join(","),
        methods.join(","),
        complete,
    ));
    let class_attr = {
        let path = quote!(wasm_bindgen = #w::wasm_bindgen);
        quote!(#[#bindgen(#path, js_class = #rust_name)])
    };
    Ok(quote! {
        #line

        #[allow(dead_code)]
        impl #self_ty {
            #(#on_type)*
        }

        #[allow(missing_docs, non_snake_case)]
        #class_attr
        impl #wrapper {
            #(#on_wrapper)*
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counter() -> DeriveInput {
        syn::parse_quote! {
            #[qml(module = "Example", name = "Counter")]
            struct Counter {
                #[qml(notify)]
                count: i32,
                #[qml(readonly)]
                done: bool,
                #[qml(skip)]
                secret: String,
            }
        }
    }

    fn compact(tokens: &TokenStream2) -> String {
        tokens.to_string().replace(' ', "")
    }

    fn w() -> TokenStream2 {
        quote!(::ljos::ljos_web)
    }

    #[test]
    fn a_property_is_a_getter_and_a_setter_on_a_wrapper_named_as_the_type() {
        let out = compact(&derive(&counter(), &w()).unwrap());
        assert!(out.contains("js_name=\"Counter\""), "{out}");
        assert!(out.contains("pubstruct__LjosWeb_Counter"), "{out}");
        assert!(out.contains("getter=\"count\""), "{out}");
        assert!(out.contains("setter=\"count\""), "{out}");
    }

    #[test]
    fn a_readonly_property_has_no_setter() {
        let out = compact(&derive(&counter(), &w()).unwrap());
        assert!(out.contains("getter=\"done\""), "{out}");
        assert!(!out.contains("setter=\"done\""), "{out}");
    }

    #[test]
    fn a_skipped_field_is_not_there() {
        let out = compact(&derive(&counter(), &w()).unwrap());
        assert!(!out.contains("secret"), "{out}");
    }

    #[test]
    fn a_constant_is_not_told() {
        let input: DeriveInput = syn::parse_quote! {
            struct K { #[qml(constant)] a: i32, b: i32 }
        };
        let out = derive(&input, &w()).unwrap().to_string();
        assert_eq!(out.matches(":: tell").count(), 1, "{out}");
    }

    #[test]
    fn the_section_says_what_the_tool_needs() {
        let out = derive(&counter(), &w()).unwrap().to_string();
        assert!(out.contains("link_section = \"ljos_web_types\""), "{out}");
        for said in [
            r#"\"name\":\"Counter\""#,
            r#"\"module\":\"Example\""#,
            r#"\"base\":\"QtObject\""#,
            r#"\"kind\":\"int\""#,
            r#"\"readonly\":true"#,
        ] {
            assert!(out.contains(said), "{said} in {out}");
        }
    }

    #[test]
    fn a_list_is_refused_naming_the_field() {
        let input: DeriveInput = syn::parse_quote! {
            struct L { #[qml(list)] rows: Vec<u32> }
        };
        let out = derive(&input, &w()).unwrap().to_string();
        assert!(out.contains("compile_error"), "{out}");
        assert!(out.contains("`L.rows`"), "{out}");
    }

    #[test]
    fn a_base_must_be_one_the_web_has() {
        let input: DeriveInput = syn::parse_quote! {
            #[qml(extends = Item)]
            struct G { #[qml(base)] item: Item, #[qml(notify)] value: f64 }
        };
        let out = compact(&derive(&input, &w()).unwrap());
        assert!(out.contains("is_a_base::<Item>"), "{out}");
    }

    #[test]
    fn a_group_is_of_the_one_struct() {
        let input: DeriveInput = syn::parse_quote! {
            struct H { #[qml(group)] border: Border }
        };
        let out = compact(&derive(&input, &w()).unwrap());
        assert!(out.contains("WebFields::web_get(group,member)"), "{out}");
        assert!(out.contains("\"border\"=>"), "{out}");
    }

    #[test]
    fn an_invokable_and_a_signal_cross() {
        let item: ItemImpl = syn::parse_quote! {
            impl Counter {
                #[qml(signal)]
                fn bumped(value: i32);
                #[qml(invokable)]
                fn increment(cx: &Cx) {}
                fn private(&self) {}
            }
        };
        let out = compact(&qml_impl(item, &w()).unwrap());
        assert!(out.contains("pubfnemit_bumped"), "{out}");
        assert!(out.contains("js_name=\"increment\""), "{out}");
        assert!(out.contains("impl__LjosWeb_Counter"), "{out}");
        assert!(!out.contains("__call_private"), "{out}");
        assert!(
            out.contains("\\\"signals\\\":[{\\\"name\\\":\\\"bumped\\\""),
            "{out}"
        );
    }

    #[test]
    fn each_side_is_behind_its_target() {
        let out = joined(quote!(impl A {}), quote!(impl B {}))
            .unwrap()
            .to_string();
        assert!(out.contains("not (target_arch = \"wasm32\")"), "{out}");
        let (native, web) = out.split_once("impl A").unwrap();
        assert!(!native.contains("impl B"), "{out}");
        assert!(web.contains("target_arch = \"wasm32\""), "{out}");
    }

    /// With the feature off the desktop's output is the native one, whatever
    /// the backend would have made.
    #[cfg(not(feature = "web"))]
    #[test]
    fn with_the_feature_off_nothing_is_added() {
        let native = quote!(impl A {});
        let out = beside(native.clone(), |_| panic!("the backend ran")).unwrap();
        assert_eq!(out.to_string(), native.to_string());
    }
}
