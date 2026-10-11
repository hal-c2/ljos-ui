//! The IR survives postcard unchanged, and keys and signatures are stable.

use ljos_ir::*;

fn span(start: u32, end: u32) -> SourceSpan {
    SourceSpan {
        start,
        end,
        line: 1,
        column: start + 1,
    }
}

fn native(name: &str) -> TypeRef {
    TypeRef::Native {
        module: "QtQuick".into(),
        name: name.into(),
    }
}

/// A file using every kind of value the IR has.
fn sample() -> CompiledFile {
    let scope = ObjRef {
        component: CompIdx(0),
        object: ObjIdx(0),
    };
    let child = ObjRef {
        component: CompIdx(0),
        object: ObjIdx(1),
    };
    let root = ObjectDef {
        ty: native("Rectangle"),
        key: KeySeg::Typed {
            ty: "Rectangle".into(),
            nth: 0,
        },
        id: Some("root".into()),
        span: span(0, 200),
        properties: vec![
            PropertyDecl {
                name: "count".into(),
                ty: "int".into(),
                is_list: false,
                is_default: false,
                is_readonly: false,
                is_required: true,
                alias: None,
                span: span(10, 30),
            },
            PropertyDecl {
                name: "label".into(),
                ty: "alias".into(),
                is_list: false,
                is_default: false,
                is_readonly: false,
                is_required: false,
                alias: Some(AliasTarget {
                    id: "text".into(),
                    object: Some(child),
                    path: vec!["text".into()],
                }),
                span: span(31, 60),
            },
        ],
        signals: vec![SignalDecl {
            name: "picked".into(),
            params: vec![SignalParam {
                name: "row".into(),
                ty: Some("int".into()),
            }],
        }],
        functions: vec![FunctionDecl {
            name: "reset".into(),
            expr: ExprId(1),
        }],
        enums: vec![EnumDecl {
            name: "Mode".into(),
            keys: vec![("Light".into(), 0), ("Dark".into(), 7)],
        }],
        members: Members {
            assignments: vec![
                Assignment {
                    name: "color".into(),
                    value: Literal::Color("#ff0000".into()),
                    span: span(61, 70),
                },
                Assignment {
                    name: "model".into(),
                    value: Literal::List(vec![
                        Literal::Undefined,
                        Literal::Null,
                        Literal::Bool(true),
                        Literal::Number(-1.5),
                        Literal::String("x".into()),
                        Literal::Object(vec![("a".into(), Literal::Number(1.0))]),
                    ]),
                    span: span(71, 80),
                },
                Assignment {
                    name: "title".into(),
                    value: Literal::Translatable(Translatable {
                        kind: TranslationKind::Text,
                        context: "Main".into(),
                        source: "%n file(s)".into(),
                        comment: Some("files".into()),
                        plural: Some(3),
                        is_plural: true,
                    }),
                    span: span(81, 90),
                },
            ],
            bindings: vec![PropertyBinding {
                name: "width".into(),
                expr: ExprId(0),
                name_span: span(92, 97),
            }],
            handlers: vec![],
            objects: vec![],
        },
        grouped: vec![Group {
            path: vec!["anchors".into()],
            members: Members::default(),
        }],
        attached: vec![Attached {
            ty: native("Keys"),
            written: "Keys".into(),
            members: Members {
                handlers: vec![Handler {
                    signal: "pressed".into(),
                    expr: ExprId(1),
                }],
                ..Members::default()
            },
        }],
        children: vec![
            ObjectValue::Object(ObjIdx(1)),
            ObjectValue::Component(CompIdx(1)),
        ],
        ..ObjectDef::default()
    };
    let text = ObjectDef {
        ty: native("Text"),
        key: KeySeg::Id("text".into()),
        parent: Some(ObjIdx(0)),
        id: Some("text".into()),
        members: Members {
            assignments: vec![Assignment {
                name: "horizontalAlignment".into(),
                value: Literal::Enum(EnumRef {
                    ty: native("Text"),
                    scope: None,
                    key: "AlignHCenter".into(),
                    value: Some(4),
                }),
                span: span(100, 120),
            }],
            ..Members::default()
        },
        ..ObjectDef::default()
    };
    let signature = ComponentSignature::of(&root, &["Cell".into()]);
    let delegate = ObjectDef {
        ty: TypeRef::Component {
            file: "Cell".into(),
            inline: None,
        },
        on: Some(vec!["x".into()]),
        ..ObjectDef::default()
    };
    let resolved = [
        Resolved::Local,
        Resolved::Id(child),
        Resolved::OuterId(child),
        Resolved::ScopeProp,
        Resolved::RootProp,
        Resolved::OuterProp(scope),
        Resolved::Attached {
            ty: native("ListView"),
            member: "isCurrentItem".into(),
        },
        Resolved::Enum(EnumRef {
            ty: native("Text"),
            scope: Some("HAlignment".into()),
            key: "AlignLeft".into(),
            value: None,
        }),
        Resolved::Singleton(TypeRef::Component {
            file: "Theme".into(),
            inline: None,
        }),
        Resolved::Type(TypeRef::Unresolved("Foo".into())),
        Resolved::ImportNs(ImportSource::Path("logic.js".into())),
        Resolved::Global { is_qml: true },
        Resolved::ContextProp {
            component: Some(CompIdx(1)),
            fallback: Some(Box::new(Resolved::ScopeProp)),
        },
        Resolved::Unknown,
    ];
    let binding = Binding {
        kind: BindingKind::Property,
        scope,
        js_fn: FnIndex(0),
        params: vec![
            Param {
                name: "$scope".into(),
                arg: Arg::Object(scope),
            },
            Param {
                name: "Text".into(),
                arg: Arg::Type(native("Text")),
            },
            Param {
                name: "Theme".into(),
                arg: Arg::Singleton(native("Theme")),
            },
            Param {
                name: "$w_count".into(),
                arg: Arg::Holder {
                    component: Some(CompIdx(1)),
                    name: "count".into(),
                    fallback: Some(scope),
                },
            },
            Param {
                name: "$a_Layout".into(),
                arg: Arg::Attached {
                    ty: native("Layout"),
                    object: scope,
                },
            },
            Param {
                name: "Logic".into(),
                arg: Arg::Namespace {
                    alias: "Logic".into(),
                    source: ImportSource::Path("logic.js".into()),
                },
            },
            Param {
                name: "modelData".into(),
                arg: Arg::Context {
                    component: Some(CompIdx(1)),
                    name: "modelData".into(),
                    fallback: Some(Box::new(Arg::Dynamic("modelData".into()))),
                },
            },
        ],
        args: vec![],
        refs: resolved
            .into_iter()
            .enumerate()
            .map(|(n, resolved)| Ref {
                name: format!("n{n}"),
                resolved,
            })
            .collect(),
        span: span(130, 150),
        hash: 0xdead_beef,
        tier1: None,
    };
    let handler = Binding {
        kind: BindingKind::Handler,
        js_fn: FnIndex(1),
        params: vec![],
        args: vec!["event".into()],
        refs: vec![],
        tier1: Some(Tier1 {
            registers: 3,
            consts: vec![
                Const::Undefined,
                Const::Null,
                Const::Bool(true),
                Const::number(-0.0),
                Const::number(f64::NAN),
                Const::String("px".into()),
            ],
            names: vec!["border".into(), "width".into()],
            args: vec![InlineArg {
                component: CompIdx(0),
                arg: Arg::Dynamic("index".into()),
            }],
            code: vec![
                Op::Param { dst: 0, param: 0 },
                Op::Get {
                    dst: 1,
                    obj: 0,
                    path: 0,
                    len: 2,
                },
                Op::Const { dst: 2, k: 5 },
                Op::Binary {
                    op: BinaryOp::Add,
                    dst: 1,
                    a: 1,
                    b: 2,
                },
                Op::JumpIfNot { cond: 1, to: 7 },
                Op::Guard {
                    obj: 0,
                    name: 1,
                    expr: ExprId(0),
                },
                Op::Call {
                    dst: 1,
                    f: Builtin::MathRound,
                    base: 1,
                    count: 1,
                },
                Op::Set {
                    obj: 0,
                    name: 1,
                    val: 1,
                    group: Some((2, 0)),
                },
                Op::Return { src: 1 },
            ],
        }),
        ..binding.clone()
    };
    CompiledFile {
        version: IR_VERSION,
        file: "app/Main".into(),
        source_hash: *blake3::hash(b"Rectangle {}").as_bytes(),
        imports: vec![Import {
            source: ImportSource::Module("QtQuick".into()),
            version: Some("2.15".into()),
            alias: None,
        }],
        pragmas: vec![Pragma {
            name: "ComponentBehavior".into(),
            value: Some("Bound".into()),
        }],
        components: vec![
            ComponentDef {
                name: "Main".into(),
                id: "app/Main".into(),
                kind: ComponentKind::File,
                objects: vec![root, text],
                signature,
            },
            ComponentDef {
                name: "Cell".into(),
                id: "app/Main/delegate".into(),
                kind: ComponentKind::Template {
                    outer: CompIdx(0),
                    is_delegate: true,
                },
                objects: vec![delegate],
                signature,
            },
        ],
        exprs: vec![binding, handler],
        translations: vec![],
        js: JsModule {
            source: "export function f0($scope) { return ($scope.width); }".into(),
            functions: vec![
                JsFunction {
                    export: "f0".into(),
                    expr: ExprId(0),
                },
                JsFunction {
                    export: "f1".into(),
                    expr: ExprId(1),
                },
            ],
        },
    }
}

#[test]
fn postcard_round_trip() {
    let file = sample();
    let bytes = file.to_bytes().unwrap();
    let back = CompiledFile::from_bytes(&bytes).unwrap();
    assert_eq!(back, file);
    // Twice gives the same bytes: a cache can compare them.
    assert_eq!(back.to_bytes().unwrap(), bytes);
}

#[test]
fn every_literal_round_trips() {
    let literals = [
        Literal::Undefined,
        Literal::Null,
        Literal::Bool(false),
        Literal::Number(f64::INFINITY),
        Literal::Number(0.1),
        Literal::String("ünïcode ✓".into()),
        Literal::Color("steelblue".into()),
        Literal::List(vec![Literal::List(vec![])]),
        Literal::Object(vec![]),
    ];
    for literal in literals {
        let bytes = postcard::to_allocvec(&literal).unwrap();
        let back: Literal = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(back, literal);
    }
}

#[test]
fn another_version_is_refused() {
    let mut file = sample();
    file.version = IR_VERSION + 1;
    let bytes = file.to_bytes().unwrap();
    assert!(matches!(
        CompiledFile::from_bytes(&bytes),
        Err(Error::Version { .. })
    ));
}

#[test]
fn object_keys_are_paths_from_the_root() {
    let file = sample();
    let root = ObjRef {
        component: CompIdx(0),
        object: ObjIdx(0),
    };
    let text = ObjRef {
        component: CompIdx(0),
        object: ObjIdx(1),
    };
    assert_eq!(
        file.object_key(root),
        ObjectKey {
            component: "app/Main".into(),
            segments: vec![],
        }
    );
    assert_eq!(
        file.object_key(text),
        ObjectKey {
            component: "app/Main".into(),
            segments: vec![KeySeg::Id("text".into())],
        }
    );
}

#[test]
fn signature_ignores_order_and_bindings_but_not_types() {
    let file = sample();
    let root = file.component(CompIdx(0)).root().clone();
    let inline = ["Cell".to_string()];
    let base = ComponentSignature::of(&root, &inline);

    let mut reordered = root.clone();
    reordered.properties.reverse();
    reordered.members = Members::default();
    assert_eq!(ComponentSignature::of(&reordered, &inline), base);

    let mut retyped = root.clone();
    retyped.properties[0].ty = "real".into();
    assert_ne!(ComponentSignature::of(&retyped, &inline), base);

    let mut signal = root.clone();
    signal.signals[0].params.clear();
    assert_ne!(ComponentSignature::of(&signal, &inline), base);

    assert_ne!(ComponentSignature::of(&root, &[]), base);
}
