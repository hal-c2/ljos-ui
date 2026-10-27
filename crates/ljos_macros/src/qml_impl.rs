//! `#[qml_impl]`: the methods, signals and slots of a QML type from an
//! `impl` block.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{
    Attribute, Error, FnArg, Ident, ImplItem, ItemImpl, LitStr, Pat, Result, ReturnType, Signature,
    Type, Visibility,
};

use crate::{camel, engine_path, is_qml};

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
                    // No name, as a parameter of a C++ signal can be.
                    Pat::Wild(_) => String::new(),
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

pub(crate) fn qml_impl_inner(mut item: ItemImpl) -> Result<TokenStream2> {
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
