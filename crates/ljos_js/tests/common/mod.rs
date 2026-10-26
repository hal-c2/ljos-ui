//! A tiny object store standing in for the Engine.
#![allow(dead_code)]

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use ljos_js::{FnId, HostError, HostObjects, HostTypeId, JsArg, JsHost, JsOut, JsValue, Scope};

#[derive(Default)]
pub struct Obj {
    pub ty: u32,
    pub props: HashMap<u32, JsValue>,
}

/// Property ids used by the tests.
pub const WIDTH: u32 = 0;
pub const HEIGHT: u32 = 1;
/// A property whose getter calls back into JS through `reenter`.
pub const DERIVED: u32 = 2;
pub const RO: u32 = 3;
pub const FAIL: u32 = 4;
pub const CHILD: u32 = 5;
pub const M_ADD: u32 = 0;
pub const M_ECHO: u32 = 1;

#[derive(Default)]
pub struct Store {
    pub objects: RefCell<HashMap<u64, Obj>>,
    pub ty: RefCell<Option<HostTypeId>>,
    pub log: RefCell<Vec<String>>,
    pub reenter: RefCell<Option<FnId>>,
    pub released: RefCell<Vec<u64>>,
    pub gets: std::cell::Cell<u64>,
}

impl Store {
    pub fn insert(&self, id: u64, width: f64, height: f64) {
        let mut o = Obj::default();
        o.props.insert(WIDTH, JsValue::Number(width));
        o.props.insert(HEIGHT, JsValue::Number(height));
        self.objects.borrow_mut().insert(id, o);
    }
}

impl HostObjects for Store {
    fn type_of(&self, obj: u64) -> Option<HostTypeId> {
        self.objects.borrow().get(&obj)?;
        *self.ty.borrow()
    }

    fn get<'js>(&self, scope: &Scope<'js>, obj: u64, prop: u32) -> Result<JsOut, HostError> {
        self.gets.set(self.gets.get() + 1);
        match prop {
            DERIVED => {
                // Calls back into JS from inside a getter (re-entrancy).
                let f = self.reenter.borrow().expect("reenter fn");
                Ok(scope.call(f, &[JsValue::Object(obj)])?)
            }
            FAIL => Err(HostError::msg("boom from getter")),
            CHILD => Ok(JsValue::Object(obj + 1)),
            _ => Ok(self
                .objects
                .borrow()
                .get(&obj)
                .and_then(|o| o.props.get(&prop).cloned())
                .unwrap_or(JsValue::Undefined)),
        }
    }

    fn set<'js>(
        &self,
        _scope: &Scope<'js>,
        obj: u64,
        prop: u32,
        value: JsArg,
    ) -> Result<(), HostError> {
        self.log
            .borrow_mut()
            .push(format!("set {obj}.{prop} = {value:?}"));
        if let Some(o) = self.objects.borrow_mut().get_mut(&obj) {
            o.props.insert(prop, value);
        }
        Ok(())
    }

    fn call<'js>(
        &self,
        _scope: &Scope<'js>,
        obj: u64,
        method: u32,
        args: Vec<JsArg>,
    ) -> Result<JsOut, HostError> {
        self.log
            .borrow_mut()
            .push(format!("call {obj}.{method}({args:?})"));
        if args.first() == Some(&JsValue::String("panic".into())) {
            panic!("host callback panicked");
        }
        match method {
            M_ADD => {
                let sum: f64 = args
                    .iter()
                    .filter_map(|a| {
                        if let JsValue::Number(n) = a {
                            Some(*n)
                        } else {
                            None
                        }
                    })
                    .sum();
                Ok(JsValue::Number(sum))
            }
            _ => Ok(JsValue::Array(args.to_vec())),
        }
    }

    fn released(&self, obj: u64) {
        self.released.borrow_mut().push(obj);
    }
}

pub fn type_def() -> ljos_js::TypeDef {
    use ljos_js::{MethodDef, PropDef, TypeDef};
    let p = |name: &str, id, writable| PropDef {
        name: name.into(),
        id,
        writable,
    };
    TypeDef {
        name: "Item".into(),
        props: vec![
            p("width", WIDTH, true),
            p("height", HEIGHT, true),
            p("derived", DERIVED, false),
            p("ro", RO, false),
            p("fail", FAIL, false),
            p("child", CHILD, false),
        ],
        methods: vec![
            MethodDef {
                name: "add".into(),
                id: M_ADD,
                copied: 0,
            },
            MethodDef {
                name: "echo".into(),
                id: M_ECHO,
                copied: 0,
            },
        ],
        ..Default::default()
    }
}

pub fn host_with_store() -> (JsHost, Rc<Store>) {
    let host = JsHost::new(ljos_js::JsConfig::default()).unwrap();
    let store = Rc::new(Store::default());
    host.set_host_objects(store.clone());
    let ty = host.register_type(&type_def()).unwrap();
    *store.ty.borrow_mut() = Some(ty);
    (host, store)
}
