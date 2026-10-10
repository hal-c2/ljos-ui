//! A compiled file as text a person can read and review in a diff.

use std::fmt::Write;

use ljos_ir::*;

/// The components, objects and scripts of a file, then its module.
pub fn dump(file: &CompiledFile) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "file {}", file.file);
    for import in &file.imports {
        let source = match &import.source {
            ImportSource::Module(uri) => uri.clone(),
            ImportSource::Path(path) => format!("{path:?}"),
        };
        let version = import
            .version
            .as_deref()
            .map(|v| format!(" {v}"))
            .unwrap_or_default();
        let alias = import
            .alias
            .as_deref()
            .map(|a| format!(" as {a}"))
            .unwrap_or_default();
        let _ = writeln!(out, "import {source}{version}{alias}");
    }
    for pragma in &file.pragmas {
        let value = pragma
            .value
            .as_deref()
            .map(|v| format!(": {v}"))
            .unwrap_or_default();
        let _ = writeln!(out, "pragma {}{value}", pragma.name);
    }
    for (index, component) in file.components.iter().enumerate() {
        let kind = match &component.kind {
            ComponentKind::File => "file".to_string(),
            ComponentKind::Inline => "inline".to_string(),
            ComponentKind::Template { outer, is_delegate } => format!(
                "template of c{}{}",
                outer.0,
                if *is_delegate { ", delegate" } else { "" }
            ),
        };
        let _ = writeln!(
            out,
            "\ncomponent c{index} {} ({kind}) id={} signature={}",
            component.name,
            component.id,
            hex(&component.signature.0[..4])
        );
        for (object, def) in component.objects.iter().enumerate() {
            let at = ObjRef {
                component: CompIdx(index as u32),
                object: ObjIdx(object as u32),
            };
            object_def(&mut out, file, at, def);
        }
    }
    if !file.translations.is_empty() {
        let _ = writeln!(out, "\ntranslations");
        for t in &file.translations {
            let _ = writeln!(out, "  {}", translatable(t));
        }
    }
    let _ = writeln!(out, "\nexprs");
    for (index, expr) in file.exprs.iter().enumerate() {
        let kind = match expr.kind {
            BindingKind::Property => "property",
            BindingKind::Handler => "handler",
            BindingKind::Function => "function",
        };
        let params: Vec<String> = expr
            .params
            .iter()
            .map(|p| format!("{}={}", p.name, arg(&p.arg)))
            .collect();
        let refs: Vec<String> = expr
            .refs
            .iter()
            .map(|r| format!("{}:{}", r.name, resolved(&r.resolved)))
            .collect();
        let _ = writeln!(
            out,
            "  e{index} {kind} f{} scope={} at {}:{} params[{}] args[{}] refs[{}]",
            expr.js_fn.0,
            obj(expr.scope),
            expr.span.line,
            expr.span.column,
            params.join(", "),
            expr.args.join(", "),
            refs.join(", ")
        );
    }
    let _ = writeln!(out, "\nmodule\n{}", file.js.source);
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn obj(at: ObjRef) -> String {
    format!("c{}o{}", at.component.0, at.object.0)
}

fn type_ref(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Native { module, name } => format!("{module}/{name}"),
        TypeRef::Component { file, inline: None } => format!("<{file}>"),
        TypeRef::Component {
            file,
            inline: Some(inline),
        } => format!("<{file}#{inline}>"),
        TypeRef::Unresolved(name) => format!("?{name}"),
    }
}

fn key_seg(segment: &KeySeg) -> String {
    match segment {
        KeySeg::Id(id) => id.clone(),
        KeySeg::Named(name) => format!("'{name}'"),
        KeySeg::ModelRow(row) => format!("[{row}]"),
        KeySeg::Typed { ty, nth } => format!("{ty}[{nth}]"),
    }
}

fn object_def(out: &mut String, file: &CompiledFile, at: ObjRef, def: &ObjectDef) {
    let key = file.object_key(at);
    let path: Vec<String> = key.segments.iter().map(key_seg).collect();
    let parent = def
        .parent
        .map(|p| format!(" parent=o{}", p.0))
        .unwrap_or_default();
    let on = def
        .on
        .as_ref()
        .map(|on| format!(" on {}", on.join(".")))
        .unwrap_or_default();
    let _ = writeln!(
        out,
        "  o{} {}{on} key=/{}{parent}",
        at.object.0,
        type_ref(&def.ty),
        path.join("/")
    );
    for p in &def.properties {
        let mut flags = String::new();
        for (on, flag) in [
            (p.is_default, "default "),
            (p.is_required, "required "),
            (p.is_readonly, "readonly "),
        ] {
            if on {
                flags.push_str(flag);
            }
        }
        let list = if p.is_list { "list<" } else { "" };
        let close = if p.is_list { ">" } else { "" };
        let alias = p
            .alias
            .as_ref()
            .map(|a| {
                let object = a.object.map(obj).unwrap_or_else(|| "?".into());
                let path = if a.path.is_empty() {
                    String::new()
                } else {
                    format!(".{}", a.path.join("."))
                };
                format!(" -> {}({object}){path}", a.id)
            })
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "    {flags}property {list}{}{close} {}{alias}",
            p.ty, p.name
        );
    }
    for name in &def.required {
        let _ = writeln!(out, "    required {name}");
    }
    for s in &def.signals {
        let params: Vec<String> = s
            .params
            .iter()
            .map(|p| match &p.ty {
                Some(ty) => format!("{}: {ty}", p.name),
                None => p.name.clone(),
            })
            .collect();
        let _ = writeln!(out, "    signal {}({})", s.name, params.join(", "));
    }
    for e in &def.enums {
        let keys: Vec<String> = e.keys.iter().map(|(k, v)| format!("{k}={v}")).collect();
        let _ = writeln!(out, "    enum {} {{ {} }}", e.name, keys.join(", "));
    }
    for f in &def.functions {
        let _ = writeln!(out, "    function {} = e{}", f.name, f.expr.0);
    }
    members(out, "    ", &def.members);
    for g in &def.grouped {
        let _ = writeln!(out, "    group {}", g.path.join("."));
        members(out, "      ", &g.members);
    }
    for a in &def.attached {
        let _ = writeln!(out, "    attached {} = {}", a.written, type_ref(&a.ty));
        members(out, "      ", &a.members);
    }
    for child in &def.children {
        let _ = writeln!(out, "    child {}", object_value(child));
    }
}

fn object_value(value: &ObjectValue) -> String {
    match value {
        ObjectValue::Object(o) => format!("o{}", o.0),
        ObjectValue::Component(c) => format!("c{}", c.0),
    }
}

fn members(out: &mut String, indent: &str, members: &Members) {
    for a in &members.assignments {
        let _ = writeln!(out, "{indent}{} = {}", a.name, literal(&a.value));
    }
    for b in &members.bindings {
        let _ = writeln!(out, "{indent}{} := e{}", b.name, b.expr.0);
    }
    for h in &members.handlers {
        let _ = writeln!(out, "{indent}on {} => e{}", h.signal, h.expr.0);
    }
    for o in &members.objects {
        let values: Vec<String> = o.values.iter().map(object_value).collect();
        let (open, close) = if o.is_list { ("[", "]") } else { ("", "") };
        let _ = writeln!(
            out,
            "{indent}{}: {open}{}{close}",
            o.name,
            values.join(", ")
        );
    }
}

fn literal(literal: &Literal) -> String {
    match literal {
        Literal::Undefined => "undefined".into(),
        Literal::Null => "null".into(),
        Literal::Bool(b) => b.to_string(),
        Literal::Number(n) => n.to_string(),
        Literal::String(s) => format!("{s:?}"),
        Literal::Color(c) => format!("color({c:?})"),
        Literal::Enum(e) => enum_ref(e),
        Literal::Translatable(t) => translatable(t),
        Literal::List(items) => {
            let items: Vec<String> = items.iter().map(self::literal).collect();
            format!("[{}]", items.join(", "))
        }
        Literal::Object(fields) => {
            let fields: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{k}: {}", self::literal(v)))
                .collect();
            format!("{{{}}}", fields.join(", "))
        }
    }
}

fn enum_ref(e: &EnumRef) -> String {
    let scope = e
        .scope
        .as_deref()
        .map(|s| format!("{s}."))
        .unwrap_or_default();
    let value = e.value.map(|v| format!("={v}")).unwrap_or_default();
    format!("enum({}.{scope}{}{value})", type_ref(&e.ty), e.key)
}

fn translatable(t: &Translatable) -> String {
    let kind = match t.kind {
        TranslationKind::Text => "tr",
        TranslationKind::Id => "trid",
        TranslationKind::NoOp => "noop",
    };
    let comment = t
        .comment
        .as_deref()
        .map(|c| format!(" // {c:?}"))
        .unwrap_or_default();
    let plural = match (t.is_plural, t.plural) {
        (true, Some(n)) => format!(" n={n}"),
        (true, None) => " n=?".into(),
        _ => String::new(),
    };
    format!("{kind}({:?}, {:?}{plural}{comment})", t.context, t.source)
}

fn arg(arg: &Arg) -> String {
    match arg {
        Arg::Object(at) => obj(*at),
        Arg::Type(ty) => format!("type {}", type_ref(ty)),
        Arg::Singleton(ty) => format!("singleton {}", type_ref(ty)),
        Arg::Attached { ty, object } => {
            format!("attached {} of {}", type_ref(ty), obj(*object))
        }
        Arg::Namespace { alias, source } => format!("ns {alias}={source:?}"),
        Arg::Context {
            component,
            name,
            fallback,
        } => {
            let component = component.map(|c| format!("c{}", c.0)).unwrap_or("?".into());
            let fallback = fallback
                .as_ref()
                .map(|f| format!(" else {}", self::arg(f)))
                .unwrap_or_default();
            format!("context {component}.{name}{fallback}")
        }
        Arg::Holder {
            component,
            name,
            fallback,
        } => {
            let component = component.map(|c| format!("c{}", c.0)).unwrap_or("?".into());
            let fallback = fallback
                .map(|f| format!(" else {}", obj(f)))
                .unwrap_or_default();
            format!("holder of {component}.{name}{fallback}")
        }
        Arg::Property { object, name } => format!("{}.{name}", obj(*object)),
        Arg::Dynamic(name) => format!("dynamic {name}"),
    }
}

fn resolved(resolved: &Resolved) -> String {
    match resolved {
        Resolved::Local => "local".into(),
        Resolved::Id(at) => format!("id {}", obj(*at)),
        Resolved::OuterId(at) => format!("outer id {}", obj(*at)),
        Resolved::ScopeProp => "scope".into(),
        Resolved::RootProp => "root".into(),
        Resolved::OuterProp(at) => format!("prop of {}", obj(*at)),
        Resolved::Attached { ty, member } => format!("attached {}.{member}", type_ref(ty)),
        Resolved::Enum(e) => enum_ref(e),
        Resolved::Singleton(ty) => format!("singleton {}", type_ref(ty)),
        Resolved::Type(ty) => format!("type {}", type_ref(ty)),
        Resolved::ImportNs(source) => format!("ns {source:?}"),
        Resolved::Global { is_qml: true } => "qml global".into(),
        Resolved::Global { is_qml: false } => "js global".into(),
        Resolved::ContextProp {
            component,
            fallback,
        } => {
            let component = component.map(|c| format!("c{}", c.0)).unwrap_or("?".into());
            let fallback = fallback
                .as_ref()
                .map(|f| format!(" else {}", self::resolved(f)))
                .unwrap_or_default();
            format!("context {component}{fallback}")
        }
        Resolved::Unknown => "UNKNOWN".into(),
    }
}
