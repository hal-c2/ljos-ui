//! The compiler's scripts: an expression or a function body becomes a
//! function of the file's module, with its free names resolved.

use std::collections::{HashMap, HashSet};

use ljos_ir::*;
use qml_front::{
    TypeOracle,
    names::{Global, Target},
    types::Found,
};
use qml_syntax::Span;

use super::{
    ATTACHED, CHAIN_LINK, CHAIN_THIS, Compiler, HELD, OPTIONAL, OPTIONAL_CALL, OPTIONAL_CALL_WAITS,
    PLACE, Plan, SINGLETON, VALUE_AT, VALUE_HELD, VALUE_IN, block, held, import_source,
    is_key_name, or_throw, read_or_throw, the_type, type_name, value_in,
};
use crate::{
    Code, Diagnostic,
    js::{self, Form},
};

impl<'t, 'a, 'p, O: TypeOracle> Compiler<'t, 'a, 'p, O> {
    /// Compiles a script to a function of the module. `args` are the
    /// signal's parameters; None when the signal is not known.
    pub(super) fn expr(
        &mut self,
        kind: BindingKind,
        form: Form,
        span: Span,
        args: Option<Vec<String>>,
        target: &str,
    ) -> ExprId {
        let id = ExprId(self.exprs.len() as u32);
        let js_fn = FnIndex(self.functions.len() as u32);
        let export = format!("f{}", js_fn.0);
        let text = qml_syntax::text(self.source, span);
        let scope_index = self.tree.scope_at(span.start);
        let scope = self.place[scope_index];
        let source_span = self.span(span);
        let hash = u64::from_le_bytes(
            blake3::hash(text.as_bytes()).as_bytes()[..8]
                .try_into()
                .unwrap_or_default(),
        );
        let known = args.clone().unwrap_or_default();
        let header = format!(
            "// {}.qml:{}:{} {target}\n",
            self.key, source_span.line, source_span.column
        );
        // A handler that is a function declares its own parameters: a name
        // of the signal's it does not declare is the document's, as Qt has it.
        let analysis = js::analyze(text, form, &known).and_then(|analysis| {
            if kind == BindingKind::Handler && analysis.is_function_value && !known.is_empty() {
                js::analyze(text, form, &[])
            } else {
                Ok(analysis)
            }
        });
        let (params, refs, body_args, code) = match analysis {
            Ok(analysis) => self.function(
                &export,
                text,
                span,
                form,
                kind,
                scope_index,
                &args,
                analysis,
            ),
            Err(message) => {
                self.diagnostics.push(Diagnostic::error(
                    Code::ScriptSyntax,
                    format!("the script does not parse as JavaScript: {message}"),
                    source_span,
                ));
                let quoted = format!("{message:?}");
                let code = format!(
                    "export function {export}() {{\n  throw new SyntaxError({quoted});\n}}\n"
                );
                (Vec::new(), Vec::new(), Vec::new(), code)
            }
        };
        self.functions
            .push((JsFunction { export, expr: id }, format!("{header}{code}")));
        self.exprs.push(Binding {
            kind,
            scope,
            js_fn,
            params,
            args: body_args,
            refs,
            span: source_span,
            hash,
            tier1: None,
        });
        id
    }

    /// Resolves the script's free names and writes its function.
    #[allow(clippy::too_many_arguments)]
    fn function(
        &mut self,
        export: &str,
        text: &str,
        span: Span,
        form: Form,
        kind: BindingKind,
        scope: usize,
        args: &Option<Vec<String>>,
        analysis: js::Analysis,
    ) -> (Vec<Param>, Vec<Ref>, Vec<String>, String) {
        let mut params: Vec<Param> = Vec::new();
        let mut refs: Vec<Ref> = Vec::new();
        let mut edits = analysis.edits.clone();
        if form == Form::Statement && kind == BindingKind::Property {
            edits.extend(js::completion_returns(text));
        }
        let mut plans: HashMap<(String, Vec<String>, bool), Plan> = HashMap::new();
        let written: HashSet<&str> = analysis
            .free
            .iter()
            .filter(|free| free.written)
            .map(|free| free.name.as_str())
            .collect();
        // Names whose members the script writes: a value type read through
        // one has to be written back through it.
        let value_bases: HashSet<u32> = analysis
            .member_writes
            .iter()
            .filter_map(|write| match write {
                js::ValueSource::Free(start) => Some(*start),
                _ => None,
            })
            .collect();
        for free in &analysis.free {
            let offset = span.start + free.span.start;
            let plan = plans
                .entry((free.name.clone(), free.chain.clone(), free.instance_type))
                .or_insert_with(|| {
                    let plan = self.resolve(&free.name, offset, &free.chain, scope);
                    let plan = if free.instance_type {
                        the_type(plan)
                    } else {
                        plan
                    };
                    if written.contains(free.name.as_str()) {
                        held(plan)
                    } else {
                        plan
                    }
                })
                .clone();
            if let Some(param) = &plan.param
                && !params.iter().any(|p| p.name == param.name)
            {
                params.push(param.clone());
            }
            let r = Ref {
                name: free.name.clone(),
                resolved: plan.resolved.clone(),
            };
            if free.name == "qsTr" && matches!(r.resolved, Resolved::Global { .. }) {
                self.translates = true;
            }
            // A global of a name `Ljos.Core` gives, in a file that imports
            // it, is that one: no global of QML's or JavaScript's has it.
            if matches!(r.resolved, Resolved::Global { .. })
                && qml_front::names::CORE_GLOBALS.contains(&free.name.as_str())
                && self.types.imports(qml_front::names::CORE)
            {
                self.cores.insert(free.name.clone());
            }
            if !refs.contains(&r) {
                refs.push(r);
            }
            let value_base = value_bases.contains(&free.span.start);
            if let Some(through) = &plan.rewrite {
                let text = if free.shorthand {
                    format!("{0}: {through}.{0}", free.name)
                } else if value_base {
                    format!("{VALUE_AT}({through}, {:?})", free.name)
                } else {
                    format!("{through}.{}", free.name)
                };
                edits.push(js::Edit {
                    span: free.span,
                    text,
                });
            } else if value_base
                && plan.replace.is_none()
                && plan.param.as_ref().is_some_and(|param| {
                    param.name == free.name
                        && matches!(param.arg, Arg::Context { .. } | Arg::Dynamic(_))
                })
            {
                // The name reached the function by value: it is a variable
                // of the function's like any other.
                let unknown = plan
                    .param
                    .as_ref()
                    .is_some_and(|param| matches!(param.arg, Arg::Dynamic(_)));
                let text = if unknown {
                    let name = &free.name;
                    format!("{VALUE_IN}({}, ($v) => {name} = $v)", read_or_throw(name))
                } else {
                    value_in(&free.name)
                };
                edits.push(js::Edit {
                    span: free.span,
                    text,
                });
            } else if plan.replace.is_none()
                && plan.param.as_ref().is_some_and(|param| {
                    param.name == free.name && matches!(param.arg, Arg::Dynamic(_))
                })
            {
                // Nothing in sight has the name. Where the running program
                // has none either, reading it throws.
                edits.push(js::Edit {
                    span: free.span,
                    text: or_throw(free),
                });
            }
            if let Some((text, members)) = &plan.replace {
                let end = match members.checked_sub(1) {
                    Some(last) => free.chain_ends.get(last).copied(),
                    None => Some(free.span.end),
                };
                if let Some(end) = end {
                    // `{ Theme }` is `{ Theme: Theme }`.
                    let text = if free.shorthand {
                        format!("{}: {text}", free.name)
                    } else {
                        text.clone()
                    };
                    edits.push(js::Edit {
                        span: Span::new(free.span.start, end),
                        text,
                    });
                }
            }
        }
        // `const p = a.b` of a `p` whose members are written: the variable
        // keeps where its value was read from beside it, unless an edit
        // already made runs across where the place is cut.
        let mut placed: HashSet<u32> = HashSet::new();
        for place in &analysis.places {
            let crosses = |at: u32| {
                edits
                    .iter()
                    .any(|edit| edit.span.start < at && edit.span.end > at)
            };
            let held = format!("{}{HELD}", place.name);
            let rest = format!(", {} = {held}.v", place.name);
            let made = match &place.source {
                js::ValueSource::Member { object, name, end } if !crosses(object.end) => vec![
                    js::Edit {
                        span: Span::new(place.binding.start, object.start),
                        text: format!("{held} = {PLACE}("),
                    },
                    js::Edit {
                        span: Span::new(object.end, *end),
                        text: format!(", {name:?}){rest}"),
                    },
                ],
                js::ValueSource::Element { object, key, end }
                    if !crosses(object.end) && !crosses(key.start) && !crosses(key.end) =>
                {
                    vec![
                        js::Edit {
                            span: Span::new(place.binding.start, object.start),
                            text: format!("{held} = {PLACE}("),
                        },
                        js::Edit {
                            span: Span::new(object.end, key.start),
                            text: ", ".into(),
                        },
                        js::Edit {
                            span: Span::new(key.end, *end),
                            text: format!("){rest}"),
                        },
                    ]
                }
                js::ValueSource::Free(start) => {
                    // A name read through the object that has it; one the
                    // engine passes in by value has no place to go back to.
                    let found = analysis.free.iter().find(|free| free.span.start == *start);
                    let through = found.and_then(|free| {
                        let plan = plans.get(&(
                            free.name.clone(),
                            free.chain.clone(),
                            free.instance_type,
                        ))?;
                        plan.replace.is_none().then_some(())?;
                        Some((free, plan.rewrite.clone()?))
                    });
                    match through {
                        // Covers the edit made for the name itself.
                        Some((free, through)) => vec![js::Edit {
                            span: Span::new(place.binding.start, free.span.end),
                            text: format!("{held} = {PLACE}({through}, {:?}){rest}", free.name),
                        }],
                        None => Vec::new(),
                    }
                }
                _ => Vec::new(),
            };
            if !made.is_empty() {
                edits.extend(made);
                placed.insert(place.symbol);
            }
        }
        // Before the edits for attached objects: where both start at the same
        // place, this one is the outer call.
        for write in &analysis.member_writes {
            match write {
                js::ValueSource::Member { object, name, end } => {
                    edits.push(js::Edit {
                        span: Span::new(object.start, object.start),
                        text: format!("{VALUE_AT}("),
                    });
                    edits.push(js::Edit {
                        span: Span::new(object.end, *end),
                        text: format!(", {name:?})"),
                    });
                }
                js::ValueSource::Element { object, key, end } => {
                    edits.push(js::Edit {
                        span: Span::new(object.start, object.start),
                        text: format!("{VALUE_AT}("),
                    });
                    edits.push(js::Edit {
                        span: Span::new(object.end, key.start),
                        text: ", ".into(),
                    });
                    edits.push(js::Edit {
                        span: Span::new(key.end, *end),
                        text: ")".into(),
                    });
                }
                js::ValueSource::Local { span, name, symbol } => edits.push(js::Edit {
                    span: *span,
                    text: if placed.contains(symbol) {
                        format!("{VALUE_HELD}({name}, {name}{HELD}, ($v) => {name} = $v)")
                    } else {
                        value_in(name)
                    },
                }),
                js::ValueSource::Free(_) => {}
            }
        }
        // Which members are read as attached objects, settled before any is
        // rewritten: a chain's rest ends only where one of these wraps it,
        // never at a member named like a type that stays a plain read
        // (`parent?.Keys.forwardTo.Foo.x`).
        let attached: Vec<(&js::TypeMember, (String, String), Span)> = analysis
            .type_members
            .iter()
            .filter_map(|member| {
                // `Text.AlignLeft`, `Controls.Button`: a name that is a type
                // or a module has no object a type could have attached to.
                let named = member.base.is_some_and(|start| {
                    analysis
                        .free
                        .iter()
                        .filter(|free| free.span.start == start)
                        .filter_map(|free| {
                            plans.get(&(free.name.clone(), free.chain.clone(), free.instance_type))
                        })
                        .any(|plan| {
                            matches!(
                                plan.resolved,
                                Resolved::ImportNs(_)
                                    | Resolved::Type(_)
                                    | Resolved::Enum(_)
                                    | Resolved::Attached { .. }
                                    | Resolved::Global { .. }
                            )
                        })
                });
                if named {
                    return None;
                }
                // `item.L.Layout`: the type is named through a namespace,
                // and what it attached to is `item`.
                let through = member
                    .through
                    .as_ref()
                    .and_then(|(ns, object)| Some((self.attaching(&[ns, &member.name])?, *object)));
                let (type_name, object) =
                    through.or_else(|| Some((self.attaching(&[&member.name])?, member.object)))?;
                Some((member, type_name, object))
            })
            .collect();
        // A read made off a chain's spine after a `?.` waits for the chain
        // to meet something: see `chain_edits`.
        let mut chained: Vec<Vec<&Attached>> = vec![Vec::new(); analysis.optional_chains.len()];
        for read in &attached {
            match read.0.chain {
                Some(chain) => chained[chain].push(read),
                None => edits.extend(attach(read)),
            }
        }
        for (chain, reads) in analysis.optional_chains.iter().zip(&chained) {
            let Some(last) = reads.iter().map(|(member, _, _)| member.end).max() else {
                continue;
            };
            let links: Vec<&js::Link> = chain
                .links
                .iter()
                .filter(|link| link.token.start < last)
                .collect();
            let Some(first) = links.first() else {
                edits.extend(reads.iter().flat_map(|read| attach(read)));
                continue;
            };
            // A callee that is itself an attached read is the attached
            // object's member: it is called as it is read.
            let called_attached = |end: u32| attached.iter().any(|(m, _, _)| m.end == end);
            // What waits from here on, in the function the chain is in,
            // ends up in a function of the chain's own: from the first
            // `?.`, or before it where that is an optional call whose
            // callee is read by a function too (`o[await k]?.()`).
            let from = match kept_this(first, &called_attached) {
                Some(js::This::Value { object, .. }) => object.end,
                Some(js::This::Super) => chain.span.start,
                None => first.token.start,
            };
            let suspends: Vec<&js::Suspend> = analysis
                .suspends
                .iter()
                .filter(|s| s.at >= from && s.at < chain.span.end)
                .filter(|s| s.function.is_none_or(|start| start < chain.span.start))
                .collect();
            if let Some(at) = suspends.iter().find(|s| s.yields) {
                let at = span.start + at.at;
                self.diagnostics.push(Diagnostic::error(
                    Code::Semantic,
                    "a `?.` chain that reads an attached object cannot `yield` after its \
                     first `?.`, nor where it reads the function a `?.()` calls: yield \
                     before the chain"
                        .to_owned(),
                    self.span(Span::new(at, at + 5)),
                ));
                edits.extend(reads.iter().flat_map(|read| attach(read)));
                continue;
            }
            edits.extend(chain_edits(
                chain,
                &links,
                reads,
                !suspends.is_empty(),
                &called_attached,
            ));
        }
        // A handler that is a function takes the signal's arguments as its
        // own parameters: the wrapper's are not in its scope at run time.
        let injected = !(kind == BindingKind::Handler && analysis.is_function_value);
        for local in analysis.locals.iter().filter(|_| injected) {
            refs.push(Ref {
                name: local.clone(),
                resolved: Resolved::Local,
            });
        }
        for call in &analysis.calls {
            if let Some((translatable, _)) = self.translatable(&call.callee, &call.args) {
                self.translations.push(translatable);
            }
        }
        let mut signature: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
        // A method is the exported function itself where it can be: a
        // function applied would be made anew, with a list of the
        // arguments, on every call.
        let flat = (form == Form::Function)
            .then_some(analysis.flat.as_ref())
            .flatten()
            .filter(|flat| !flat.names.iter().any(|name| signature.contains(name)));
        if let Some(flat) = flat {
            edits.push(js::Edit {
                span: flat.head,
                text: String::new(),
            });
        }
        let body = js::apply(text, &edits);
        let close = if body.contains("//") { "\n  " } else { "" };
        let wrapped = args.is_none() || analysis.uses_arguments;
        let known = args.clone().unwrap_or_default();
        let (body_args, code) = match form {
            Form::Function if flat.is_some() => {
                let between = if signature.is_empty() || analysis.function_params.is_empty() {
                    ""
                } else {
                    ", "
                };
                let code = format!(
                    "export function {export}({}{between}{body}\n",
                    signature.join(", ")
                );
                (analysis.function_params.clone(), code)
            }
            Form::Function => {
                signature.push("...$args".into());
                let code = format!(
                    "export function {export}({}) {{\n  return ({body}{close}).apply(this, $args);\n}}\n",
                    signature.join(", ")
                );
                (analysis.function_params.clone(), code)
            }
            Form::Expression if kind == BindingKind::Handler && analysis.is_function_value => {
                signature.push("...$args".into());
                let code = format!(
                    "export function {export}({}) {{\n  return ({body}{close}).apply(this, $args);\n}}\n",
                    signature.join(", ")
                );
                (known, code)
            }
            _ if wrapped => {
                signature.push("...$args".into());
                let inner = match form {
                    Form::Expression => format!(
                        "function ({}) {{ return ({body}{close}); }}",
                        known.join(", ")
                    ),
                    _ => format!("function ({}) {}", known.join(", "), block(&body)),
                };
                let code = format!(
                    "export function {export}({}) {{\n  return ({inner}).apply(this, $args);\n}}\n",
                    signature.join(", ")
                );
                (known, code)
            }
            Form::Expression => {
                signature.extend(known.iter().cloned());
                let code = format!(
                    "export function {export}({}) {{\n  return ({body}{close});\n}}\n",
                    signature.join(", ")
                );
                (known, code)
            }
            Form::Statement => {
                signature.extend(known.iter().cloned());
                let code = format!(
                    "export function {export}({}) {}\n",
                    signature.join(", "),
                    block(&body)
                );
                (known, code)
            }
        };
        (params, refs, body_args, code)
    }

    /// What a free name at `offset` is, and how the function gets it.
    fn resolve(&mut self, name: &str, offset: u32, chain: &[String], scope: usize) -> Plan {
        let context = self.tree.objects[self.tree.object_at(offset)].context;
        let Some(resolution) = self.lookup(name, offset) else {
            if self.project.dynamic_names().contains(name) {
                return Plan {
                    resolved: Resolved::ContextProp {
                        component: None,
                        fallback: None,
                    },
                    param: Some(Param {
                        name: name.to_string(),
                        arg: Arg::Context {
                            component: None,
                            name: name.to_string(),
                            fallback: None,
                        },
                    }),
                    rewrite: None,
                    replace: None,
                };
            }
            if self.warned.insert(name.to_string()) {
                let span = self
                    .lines
                    .span(self.source, Span::new(offset, offset + name.len() as u32));
                self.diagnostics.push(Diagnostic::warning(
                    Code::UnresolvedName,
                    format!(
                        "`{name}` is not a name in sight: it is looked up when the program runs"
                    ),
                    span,
                ));
            }
            return Plan {
                resolved: Resolved::Unknown,
                param: Some(Param {
                    name: name.to_string(),
                    arg: Arg::Dynamic(name.to_string()),
                }),
                rewrite: None,
                replace: None,
            };
        };
        if let Some(Target::Type(found)) = &resolution.target {
            self.check_type_member(name, found, chain, offset);
        }
        let base = resolution
            .target
            .as_ref()
            .map(|target| self.target(name, target, chain, scope, context));
        let role = resolution.roles.first().copied();
        if role.is_none()
            && resolution.aimed.is_none()
            && let Some(base) = base
        {
            return base;
        }
        // The running program may have the name first: a role of the model,
        // or what the target of a PropertyChanges has.
        let component = role.map(|c| CompIdx(c as u32));
        let fallback_arg = base.as_ref().and_then(|base| {
            let param = base.param.as_ref()?;
            Some(Box::new(match (&base.rewrite, &param.arg) {
                (Some(_), Arg::Object(object)) => Arg::Property {
                    object: *object,
                    name: name.to_string(),
                },
                (_, arg) => arg.clone(),
            }))
        });
        Plan {
            resolved: Resolved::ContextProp {
                component,
                fallback: base.map(|base| Box::new(base.resolved)),
            },
            param: Some(Param {
                name: name.to_string(),
                arg: Arg::Context {
                    component,
                    name: name.to_string(),
                    fallback: fallback_arg,
                },
            }),
            rewrite: None,
            replace: None,
        }
    }

    fn target(
        &self,
        name: &str,
        target: &Target<O::Type>,
        chain: &[String],
        scope: usize,
        context: usize,
    ) -> Plan {
        let as_is = |arg: Arg| {
            Some(Param {
                name: name.to_string(),
                arg,
            })
        };
        match target {
            Target::Id(object) => {
                let at = self.place[*object];
                let resolved = if self.tree.objects[*object].context == context {
                    Resolved::Id(at)
                } else {
                    Resolved::OuterId(at)
                };
                Plan {
                    resolved,
                    param: as_is(Arg::Object(at)),
                    rewrite: None,
                    replace: None,
                }
            }
            Target::Property { object, .. } => {
                let at = self.place[*object];
                let root = self.tree.contexts[context].root;
                let (resolved, through) = if *object == scope {
                    (Resolved::ScopeProp, "$scope".to_string())
                } else if *object == root {
                    (Resolved::RootProp, "$root".to_string())
                } else {
                    (
                        Resolved::OuterProp(at),
                        format!("$c{}o{}", at.component.0, at.object.0),
                    )
                };
                Plan {
                    resolved,
                    param: Some(Param {
                        name: through.clone(),
                        arg: Arg::Object(at),
                    }),
                    rewrite: Some(through),
                    replace: None,
                }
            }
            Target::Context { context, .. } => {
                let component = Some(CompIdx(*context as u32));
                Plan {
                    resolved: Resolved::ContextProp {
                        component,
                        fallback: None,
                    },
                    param: as_is(Arg::Context {
                        component,
                        name: name.to_string(),
                        fallback: None,
                    }),
                    rewrite: None,
                    replace: None,
                }
            }
            Target::Type(found) => self.type_plan(name, found, None, chain, scope),
            Target::Namespace(source) => {
                let source = import_source(source);
                // `S.Theme.accent`: the script reads a type of the module,
                // which is what the reference is, through the namespace.
                let through = chain
                    .first()
                    .filter(|ty| is_key_name(ty))
                    .and_then(|ty| Some((self.types.find(&[name, ty.as_str()])?, &chain[1..])))
                    .map(|(found, rest)| self.type_plan(name, &found, Some(name), rest, scope));
                // `L.Layout.row`: what the type attached is all the script
                // needs, where the namespace alone is nothing to it.
                if let Some(plan) = &through
                    && plan.replace.is_some()
                {
                    return plan.clone();
                }
                Plan {
                    resolved: through
                        .map_or_else(|| Resolved::ImportNs(source.clone()), |plan| plan.resolved),
                    param: as_is(Arg::Namespace {
                        alias: name.to_string(),
                        source,
                    }),
                    rewrite: None,
                    replace: None,
                }
            }
            Target::Global(global) => {
                if name == "Qt"
                    && let Some(key) = chain.first().filter(|key| is_key_name(key))
                    && let Some(value) = self.qt_enum(key)
                {
                    return Plan {
                        resolved: Resolved::Enum(value),
                        param: None,
                        rewrite: None,
                        replace: None,
                    };
                }
                Plan {
                    resolved: Resolved::Global {
                        is_qml: *global != Global::Js,
                    },
                    param: None,
                    rewrite: None,
                    replace: None,
                }
            }
        }
    }

    /// The module and name of the type `path` names in this file (`Layout`,
    /// or `L.Layout` through a namespace), when it is one that attaches an
    /// object to others.
    fn attaching(&self, path: &[&str]) -> Option<(String, String)> {
        let found = self.types.find(path)?;
        let base = self.types.base(&found.kind)?;
        self.types.oracle.attached(base)?;
        match self.type_ref(&found, None) {
            TypeRef::Native { module, name } => Some((module, name)),
            _ => None,
        }
    }

    /// A type named in a script: what of it the script reads. `scope` is
    /// the object the script is written in.
    fn type_plan(
        &self,
        name: &str,
        found: &Found<O::Type>,
        qualifier: Option<&str>,
        chain: &[String],
        scope: usize,
    ) -> Plan {
        let ty = self.type_ref(found, qualifier);
        let kind = &found.kind;
        // Through a namespace the name the script has is the namespace's,
        // and every type of it is read by that one name: each gets a
        // parameter of its own, written where `Alias.Type` was.
        let through = |prefix: &str| match qualifier {
            Some(qualifier) => format!("${prefix}_{qualifier}_{}", type_name(&ty)),
            None => format!("${prefix}_{name}"),
        };
        let qualified = qualifier.map(|_| through("t"));
        let param = |arg: Arg| {
            Some(Param {
                name: qualified.clone().unwrap_or_else(|| name.to_string()),
                arg,
            })
        };
        let replace = || qualified.clone().map(|through| (through, 1));
        let enumeration = |scope: Option<&str>, key: &str| Plan {
            resolved: Resolved::Enum(EnumRef {
                ty: ty.clone(),
                scope: scope.map(str::to_string),
                key: key.to_string(),
                value: self.enum_value(kind, scope, key),
            }),
            param: param(Arg::Type(ty.clone())),
            rewrite: None,
            replace: replace(),
        };
        if let Some(first) = chain.first() {
            match self.types.declared_enum(kind, first) {
                Some(false) => return enumeration(None, first),
                Some(true) => {
                    if let Some(key) = chain.get(1)
                        && self.types.is_key(kind, Some(first), key)
                    {
                        return enumeration(Some(first), key);
                    }
                }
                None => {}
            }
        }
        if self.types.is_singleton(kind) {
            // The object is made when the script first comes to the name,
            // not when the function is called: a branch not taken leaves it
            // unmade. So the function is given what stands for it, and asks
            // for the object where the name was.
            let through = through("s");
            return Plan {
                resolved: Resolved::Singleton(ty.clone()),
                param: Some(Param {
                    name: through.clone(),
                    arg: Arg::Singleton(ty),
                }),
                rewrite: None,
                replace: Some((
                    format!("{SINGLETON}({through})"),
                    usize::from(qualifier.is_some()),
                )),
            };
        }
        let attaches = self
            .types
            .base(kind)
            .and_then(|base| self.types.oracle.attached(base))
            .is_some();
        // The name alone, as a value (`target: Component`), is what the type
        // attached to the scope object too, as in Qt; its member is "".
        let member = match chain.first() {
            Some(member) if member.starts_with(|c: char| c.is_ascii_lowercase()) => {
                Some(member.as_str())
            }
            None => Some(""),
            Some(_) => None,
        };
        match member {
            Some(member) if attaches => {
                // Its own parameter: the type's name may also be read for an
                // enum in the same script (`Accessible.Button`), and that is
                // the type, not what it attached.
                let through = through("a");
                Plan {
                    resolved: Resolved::Attached {
                        ty: ty.clone(),
                        member: member.to_string(),
                    },
                    param: Some(Param {
                        name: through.clone(),
                        arg: Arg::Attached {
                            ty,
                            object: self.place[scope],
                        },
                    }),
                    rewrite: None,
                    replace: Some((through, usize::from(qualifier.is_some()))),
                }
            }
            _ => Plan {
                resolved: Resolved::Type(ty.clone()),
                param: param(Arg::Type(ty)),
                rewrite: None,
                replace: replace(),
            },
        }
    }
}

/// A member read as an attached object, the module and type that attach
/// it, and what it is read off.
type Attached<'m> = (&'m js::TypeMember, (String, String), Span);

/// `other.Layout` as `__ljos_att(other, "QtQuick.Layouts", "Layout")`.
fn attach((member, (module, name), object): &Attached) -> [js::Edit; 2] {
    [
        js::Edit {
            span: Span::new(object.start, object.start),
            text: format!("{ATTACHED}("),
        },
        js::Edit {
            span: Span::new(object.end, member.end),
            text: format!(", {module:?}, {name:?})"),
        },
    ]
}

/// `parent?.Window.window`: the attached read is a call, and a call is no
/// member a `?.` could cut short. So each `?.` of the chain up to its last
/// attached read becomes a call that is given what the `?.` follows and
/// the rest of the chain as a function of it (`OPTIONAL`), and that
/// gives `undefined` without calling it when what it is given is nothing.
/// The rest of the chain is read as written: `x?.y.Window` throws where
/// `x.y` is nothing, as `x?.y.z` does.
///
/// `links` are the chain's `?.`s up to its last attached read. The
/// function after the `n`th starts with the calls that what follows it
/// up to the next is in, outermost first: the next `?.`'s, then the
/// attached reads', the last outermost. An optional call keeps its
/// `this` (`OPTIONAL_CALL`). Where the chain waits after its first `?.`,
/// the functions are `async` and the chain is waited for.
fn chain_edits(
    chain: &js::OptionalChain,
    links: &[&js::Link],
    reads: &[&Attached],
    waits: bool,
    called_attached: &dyn Fn(u32) -> bool,
) -> Vec<js::Edit> {
    let asynchronous = if waits { "async " } else { "" };
    let call = if waits {
        OPTIONAL_CALL_WAITS
    } else {
        OPTIONAL_CALL
    };
    let this = |link: &js::Link| kept_this(link, called_attached);
    // After how many of the `?.`s a read is made.
    let segment = |end: u32| links.iter().filter(|link| link.token.start < end).count();
    let openers = |n: usize| {
        let mut text = match links.get(n).map(|link| (link, this(link))) {
            Some((_, Some(js::This::Super))) => format!("{call}(this, {asynchronous}() => "),
            Some((_, Some(js::This::Value { .. }))) => format!("{call}("),
            Some(_) => format!("{OPTIONAL}("),
            None => String::new(),
        };
        let here = reads
            .iter()
            .filter(|(member, _, _)| segment(member.end) == n);
        text.push_str(&format!("{ATTACHED}(").repeat(here.count()));
        text
    };
    // The callee's object of an optional call, read by a function of it.
    let reader = |n: usize| format!(", {asynchronous}({CHAIN_THIS}{n}) => {CHAIN_THIS}{n}");
    let mut edits = vec![js::Edit {
        span: Span::new(chain.span.start, chain.span.start),
        text: format!("{}{}", if waits { "(await " } else { "" }, openers(0)),
    }];
    for paren in &chain.parens {
        edits.push(js::Edit {
            span: Span::new(paren.start, paren.start + 1),
            text: String::new(),
        });
        edits.push(js::Edit {
            span: Span::new(paren.end - 1, paren.end),
            text: String::new(),
        });
    }
    for (index, link) in links.iter().enumerate() {
        let n = index + 1;
        let mut text = format!(
            ", {asynchronous}({CHAIN_LINK}{n}) => {}{CHAIN_LINK}{n}",
            openers(n)
        );
        // `a?.f?.()`: what `f` is read off is what the `?.` before it met.
        if let Some(next) = links.get(n)
            && let Some(js::This::Value { object, .. }) = this(next)
            && object.end <= link.token.start
        {
            text.push_str(&reader(n + 1));
        }
        // `parent?.Window`: the attached read is made of what the `?.` met.
        let merged = reads.iter().find(|(member, _, object)| {
            object.end <= link.token.start && link.token.end <= member.end
        });
        match merged {
            Some((member, (module, name), object)) => edits.push(js::Edit {
                span: Span::new(object.end, member.end),
                text: format!("{text}, {module:?}, {name:?})"),
            }),
            None => {
                if link.kind == js::LinkKind::Member {
                    text.push('.');
                }
                edits.push(js::Edit {
                    span: link.token,
                    text,
                });
            }
        }
        // Unless the `?.` before gave it (see above).
        if let Some(js::This::Value { object, .. }) = this(link)
            && !index
                .checked_sub(1)
                .is_some_and(|before| object.end <= links[before].token.start)
        {
            edits.push(js::Edit {
                span: Span::new(object.end, object.end),
                text: reader(n),
            });
        }
    }
    for (member, (module, name), object) in reads {
        let merged = links
            .iter()
            .any(|link| object.end <= link.token.start && link.token.end <= member.end);
        if !merged {
            edits.push(js::Edit {
                span: Span::new(object.end, member.end),
                text: format!(", {module:?}, {name:?})"),
            });
        }
    }
    edits.push(js::Edit {
        span: Span::new(chain.span.end, chain.span.end),
        text: ")".repeat(links.len() + usize::from(waits)),
    });
    edits
}

/// The `this` an optional call keeps, where it is kept: its callee is
/// then read by a function of it.
fn kept_this(link: &js::Link, called_attached: &dyn Fn(u32) -> bool) -> Option<js::This> {
    match &link.kind {
        js::LinkKind::Call(Some(js::This::Value { name_end, .. }))
            if name_end.is_some_and(called_attached) =>
        {
            None
        }
        js::LinkKind::Call(Some(this)) => Some(this.clone()),
        _ => None,
    }
}
