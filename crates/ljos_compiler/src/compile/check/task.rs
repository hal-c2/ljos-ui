//! `Task` (docs/DIALECT.md §4.19): typed from its `run`. The parameters
//! before a last `AbortSignal` are what `args` and `start` give, and what
//! the run's promise gives is `T`: `value` is `T | undefined`, `done`
//! hands a `T`, and `state` is the union a `switch` or a comparison of its
//! `status` takes apart.

use qml_front::TypeOracle;
use qml_front::scope::is_group;
use qml_syntax::{
    GetSpan, Span,
    ast::{QmlBindingValue, QmlMember, QmlObject},
    js::{Argument, ArrayExpressionElement, CallExpression, Expression},
};

use super::ty::{Field, Kind, Param, Sig, Ty, Web};
use super::{Checker, Obj, Own, param};
use crate::Code;

/// What a Task's `run` says of it.
#[derive(Clone)]
pub(super) struct TaskSig {
    /// The parameters before the signal: what `args` holds.
    params: Vec<Param>,
}

/// The type's name, as `Ljos.Core` has it.
const TASK: &str = "Task";
const MODULE: &str = "Ljos.Core";

impl<'a, O: TypeOracle> Checker<'_, '_, 'a, '_, O> {
    /// Gives each Task of the file the members its `run` types. Before
    /// `functions`, which checks their bodies: a function that reads a
    /// Task's `state` or calls its `start` sees them typed.
    ///
    /// DECISION: so a `run` whose return type is not written, and which
    /// gives what a function of the file returns, gives `unknown`: write
    /// the run's return type, as the examples do.
    pub(super) fn tasks(&mut self, object: &QmlObject<'a>, owner: Option<usize>) {
        let index = if is_group(object) {
            owner
        } else {
            self.c.tree.index_of(object)
        };
        if let Some(index) = index
            && !is_group(object)
            && self.is_task(index)
        {
            self.declare_task(object, index);
        }
        for member in &object.members {
            match member {
                QmlMember::Object(child) => self.tasks(child, index),
                QmlMember::Binding(binding) => self.tasks_value(&binding.value, index),
                QmlMember::Property(declaration) => {
                    if let Some(value) = &declaration.value {
                        self.tasks_value(value, index);
                    }
                }
                QmlMember::InlineComponent(inline) => self.tasks(&inline.object, None),
                QmlMember::Signal(_) | QmlMember::Function(_) | QmlMember::Enum(_) => {}
            }
        }
    }

    fn tasks_value(&mut self, value: &QmlBindingValue<'a>, owner: Option<usize>) {
        match value {
            QmlBindingValue::Object(object) => self.tasks(object, owner),
            QmlBindingValue::Objects(objects) => {
                for object in objects {
                    self.tasks(object, owner);
                }
            }
            QmlBindingValue::Expression(_) | QmlBindingValue::Statement(_) => {}
        }
    }

    /// Whether the object `index` of the file is a `Task`.
    fn is_task(&self, index: usize) -> bool {
        let types = self.c.types;
        let Some(task) = types.oracle.module_type(MODULE, TASK) else {
            return false;
        };
        self.c.tree.objects[index]
            .kind
            .as_ref()
            .and_then(|kind| types.base(kind))
            == Some(task)
    }

    /// Types the Task `index` from the `run` written on it.
    ///
    /// DECISION: a Task whose `run` is not written on it (a component
    /// built on Task, given its run elsewhere) is typed as the table has
    /// it: what it gives is `unknown`.
    fn declare_task(&mut self, object: &QmlObject<'a>, index: usize) {
        let Some(run) = object.members.iter().find_map(|member| match member {
            QmlMember::Binding(binding) if binding.name.as_simple() == Some("run") => {
                match &binding.value {
                    QmlBindingValue::Expression(expression) => Some(expression),
                    _ => None,
                }
            }
            _ => None,
        }) else {
            return;
        };
        // Learnt quietly: the run is checked where it is written, as the
        // binding it is.
        self.quiet += 1;
        let outer = self.this.replace(index);
        let ty = self.expr(run, None);
        self.this = outer;
        self.quiet -= 1;
        let Kind::Function(sig) = self.arena.kind(ty).clone() else {
            return;
        };
        let mut params = sig.params.to_vec();
        // The signal is the last parameter, optional or not: the run is
        // given it after the arguments.
        if params.last().is_some_and(|last| {
            let ty = self.arena.without_undefined(last.ty);
            matches!(self.arena.kind(ty), Kind::Web(Web::AbortSignal))
        }) {
            params.pop();
        }
        let value = match self.arena.kind(sig.ret) {
            Kind::Promise(value) => *value,
            _ => sig.ret,
        };
        self.give_task(index, &params, value);
        self.tasks_typed.insert(index, TaskSig { params });
    }

    fn give_task(&mut self, index: usize, params: &[Param], value: Ty) {
        let maybe = self.arena.or(value, Ty::UNDEFINED);
        let status = |checker: &mut Self, name: &str| Field {
            name: "status".into(),
            ty: checker.arena.str(name),
            optional: false,
            readonly: true,
        };
        let field = |name: &str, ty: Ty| Field {
            name: name.into(),
            ty,
            optional: false,
            readonly: true,
        };
        let initial = Kind::Record(Box::new([status(self, "initial")]));
        let pending = Kind::Record(Box::new([status(self, "pending"), field("value", maybe)]));
        let done = Kind::Record(Box::new([status(self, "done"), field("value", value)]));
        let failed = Kind::Record(Box::new([
            status(self, "failed"),
            field("error", Ty::UNKNOWN),
            field("value", maybe),
        ]));
        let parts: Vec<Ty> = [initial, pending, done, failed]
            .into_iter()
            .map(|kind| self.arena.intern(kind))
            .collect();
        let state = self.arena.union(parts);
        let statuses: Vec<Ty> = ["initial", "pending", "done", "failed"]
            .into_iter()
            .map(|name| self.arena.str(name))
            .collect();
        let statuses = self.arena.union(statuses);
        let var = || Some("var".to_string());
        for (name, ty, written) in [
            ("state", state, var()),
            ("status", statuses, Some("string".to_string())),
            ("value", maybe, var()),
            ("error", Ty::UNKNOWN, var()),
        ] {
            self.own.insert((index, name), Own { ty, written });
        }
        // `start()` runs with `args`; `start(…)` with what it is given.
        let optional: Vec<Param> = params
            .iter()
            .map(|p| Param {
                optional: true,
                ..p.clone()
            })
            .collect();
        let start = self.arena.function(Sig {
            params: optional.into_boxed_slice(),
            rest: None,
            ret: Ty::VOID,
            guard: None,
        });
        let abort = self.signal_ty(Vec::new());
        let done = self.signal_ty(vec![param("value", value)]);
        let failed = self.signal_ty(vec![param("error", Ty::UNKNOWN)]);
        for (name, ty) in [
            ("start", start),
            ("abort", abort),
            ("done", done),
            ("failed", failed),
        ] {
            self.own.insert((index, name), Own { ty, written: None });
        }
    }

    /// A call of `object.name`: when it is a typed Task's `start`, it is
    /// given none of the run's arguments, so it runs with `args`, or all
    /// of them, optional ones too. Its type has each parameter optional, for `start()`; what
    /// is given is held to them there.
    ///
    /// DECISION: told apart by what is called, not by `start`'s type: a
    /// function type is interned, so a function of the same shape is the
    /// same type.
    pub(super) fn task_call(&mut self, object: Ty, name: &str, call: &CallExpression<'a>) {
        if name != "start" {
            return;
        }
        let Kind::Object(o) = *self.arena.kind(object) else {
            return;
        };
        let Obj::Instance(index) = self.objs[o as usize] else {
            return;
        };
        let Some(task) = self.tasks_typed.get(&index) else {
            return;
        };
        // Every parameter, optional ones too, as for `args`: the signal
        // comes after what is given, so a parameter left out takes it.
        let needs = task.params.len();
        let given = call.arguments.len();
        let spread = call
            .arguments
            .iter()
            .any(|argument| matches!(argument, Argument::SpreadElement(_)));
        if spread || given == 0 || given >= needs {
            return;
        }
        // Where TypeScript says it: at what is given.
        let span = match (call.arguments.first(), call.arguments.last()) {
            (Some(first), Some(last)) => Span::new(first.span().start, last.span().end),
            _ => call.span,
        };
        self.report(
            Code::TsAssignType,
            format!(
                "`start` is given none of `run`'s {needs} arguments before its signal, or all of them, and is given {given}"
            ),
            span,
        );
    }

    /// `args` written on a Task whose run is typed: each element goes
    /// where the parameter at its place is, and there are as many as the
    /// run takes before its signal. True when it was checked here.
    ///
    /// DECISION: an `args` that is not an array written in place (one a
    /// function gives, say) is checked as what it is, against nothing:
    /// the subset has no tuple type to hold it to.
    pub(super) fn task_args(&mut self, owner: usize, value: &QmlBindingValue<'a>) -> bool {
        let Some(task) = self.tasks_typed.get(&owner).cloned() else {
            return false;
        };
        let QmlBindingValue::Expression(Expression::ArrayExpression(array)) = value else {
            return false;
        };
        let elements: Vec<&Expression<'a>> = array
            .elements
            .iter()
            .filter_map(|element| match element {
                ArrayExpressionElement::SpreadElement(_) | ArrayExpressionElement::Elision(_) => {
                    None
                }
                other => Some(other.to_expression()),
            })
            .collect();
        if elements.len() != array.elements.len() {
            return false;
        }
        if elements.len() != task.params.len() {
            self.report(
                Code::TsAssignType,
                format!(
                    "`run` takes {} before its signal, and `args` gives it {}",
                    arguments(task.params.len()),
                    elements.len()
                ),
                array.span,
            );
        }
        for (at, element) in elements.into_iter().enumerate() {
            let Some(param) = task.params.get(at) else {
                self.expr(element, None);
                continue;
            };
            let given = self.expr(element, Some(param.ty));
            let word = self.word(param.ty);
            self.check_assign(&param.name, param.ty, &word, given, element.span());
        }
        true
    }
}

fn arguments(count: usize) -> String {
    match count {
        1 => "1 argument".to_string(),
        _ => format!("{count} arguments"),
    }
}
