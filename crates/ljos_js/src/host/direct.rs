//! A method declared in a script that a script calls without the host in
//! between ([`Scope::direct_method`]).

use std::rc::Rc;

use rquickjs::{Array, Function, Persistent, Value, function::Args};

use super::Scope;
use crate::{FnId, JsArg, JsValue};

/// One value a [`DirectCall`] passes before the caller's arguments.
#[derive(Debug, Clone)]
pub enum DirectParam {
    /// The same value on every call.
    Value(JsValue),
    /// The property `name` of the object, read as the call starts.
    Read(JsValue, String),
}

/// What [`Scope::direct_method`] makes a function of: `func`, called with
/// `this` and `params`, then the caller's arguments.
#[derive(Debug, Clone)]
pub struct DirectCall {
    pub func: FnId,
    pub this: JsValue,
    pub params: Vec<DirectParam>,
    /// For a call that threw, as [`Scope::retell_failed_call`] takes them.
    pub file: String,
    pub export: String,
    pub names: Vec<String>,
    pub takes: Vec<String>,
}

/// The JS that makes a direct method of one shape: the function, `this`,
/// the retelling, then one value per parameter (two for a read whose name
/// is no identifier). Called with no more arguments than it `takes`, it
/// passes them as they are; with more, as a list.
fn shape_source(params: &[DirectParam], takes: usize) -> String {
    let mut given = String::from("f,t,r");
    // What is read is read into a local first, once: a retelling is told
    // the values the function had.
    let mut reads = Vec::new();
    let mut list = Vec::new();
    for (i, p) in params.iter().enumerate() {
        match p {
            DirectParam::Value(_) => {
                given.push_str(&format!(",q{i}"));
                list.push(format!("q{i}"));
                continue;
            }
            DirectParam::Read(_, name) if is_identifier(name) => {
                given.push_str(&format!(",q{i}"));
                reads.push(format!("x{i}=q{i}.{name}"));
            }
            DirectParam::Read(..) => {
                given.push_str(&format!(",q{i},n{i}"));
                reads.push(format!("x{i}=q{i}[n{i}]"));
            }
        }
        list.push(format!("x{i}"));
    }
    let reads = if reads.is_empty() {
        String::new()
    } else {
        format!("var {};", reads.join(","))
    };
    let list = list.join(",");
    let args = (0..takes)
        .map(|i| format!("a{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let all = [list.as_str(), args.as_str()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(",");
    let call = if all.is_empty() {
        "f.call(t)".to_owned()
    } else {
        format!("f.call(t,{all})")
    };
    format!(
        "(function({given}){{return function({args}){{{reads}\
         if(arguments.length>{takes}){{var p=[{list}];\
         for(var i=0;i<arguments.length;i++)p.push(arguments[i]);\
         try{{return f.apply(t,p)}}catch(e){{throw r(e,p)}}}}\
         try{{return {call}}}catch(e){{\
         throw r(e,[{list}].concat(Array.prototype.slice.call(arguments)))}}}}}})"
    )
}

fn shape_key(params: &[DirectParam], takes: usize) -> String {
    let mut key = format!("{takes}\u{0}");
    for p in params {
        match p {
            DirectParam::Value(_) => key.push('v'),
            DirectParam::Read(_, name) if is_identifier(name) => {
                key.push('.');
                key.push_str(name);
            }
            DirectParam::Read(..) => key.push('['),
        }
        key.push('\u{0}');
    }
    key
}

/// Written into the source as it is: ASCII only, and no reserved word
/// cares after a dot.
fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

impl<'js> Scope<'js> {
    /// A function that calls `call.func` as the host would have called
    /// it for a method, in script alone: the parameters read as the call
    /// starts, what it throws the caller's to catch, worded as
    /// [`Scope::retell_failed_call`] words it, and what it gives back
    /// given back as it is. The functions of one shape of parameters share
    /// what makes them, compiled once.
    pub fn direct_method(&self, call: DirectCall, name: &str) -> rquickjs::Result<Function<'js>> {
        let make = self.direct_shape(&call.params, call.takes.len())?;
        let func = {
            let fns = self.state.fns.borrow();
            fns.get(call.func).map(|e| e.func.clone())
        };
        let Some(func) = func else {
            return Err(rquickjs::Exception::throw_message(
                &self.ctx,
                "stale function handle",
            ));
        };
        let func = func.restore(&self.ctx)?;
        let this = self.to_value(&call.this)?;
        let mut a = Args::new(self.ctx.clone(), 3 + 2 * call.params.len());
        a.push_arg(func)?;
        a.push_arg(this)?;
        a.push_arg(self.retelling(&call)?)?;
        for p in &call.params {
            match p {
                DirectParam::Value(v) => a.push_arg(self.to_value(v)?)?,
                DirectParam::Read(o, name) => {
                    a.push_arg(self.to_value(o)?)?;
                    if !is_identifier(name) {
                        a.push_arg(name.as_str())?;
                    }
                }
            }
        }
        let made: Function<'js> = make.call_arg(a)?;
        made.with_name(name)
    }

    fn direct_shape(
        &self,
        params: &[DirectParam],
        takes: usize,
    ) -> rquickjs::Result<Function<'js>> {
        let key = shape_key(params, takes);
        let cached = self.state.direct.borrow().get(&key).cloned();
        if let Some(make) = cached {
            return make.restore(&self.ctx);
        }
        let make: Function<'js> = self.ctx.eval(shape_source(params, takes))?;
        self.state
            .direct
            .borrow_mut()
            .insert(key, Persistent::save(&self.ctx, make.clone()));
        Ok(make)
    }

    /// What a direct method that threw throws: the same error, worded
    /// anew where it can be.
    fn retelling(&self, call: &DirectCall) -> rquickjs::Result<Function<'js>> {
        let state = Rc::downgrade(&self.state);
        let file = call.file.clone();
        let export = call.export.clone();
        let names = call.names.clone();
        let takes = call.takes.clone();
        Function::new(
            self.ctx.clone(),
            move |ctx: rquickjs::Ctx<'js>, thrown: Value<'js>, given: Array<'js>| {
                let Some(state) = state.upgrade() else {
                    return thrown;
                };
                let scope = Scope {
                    ctx: ctx.clone(),
                    state,
                };
                let args: Vec<JsArg> = given
                    .iter::<Value<'js>>()
                    .filter_map(|v| v.ok().and_then(|v| scope.from_value(&v).ok()))
                    .collect();
                let names: Vec<&str> = names.iter().map(String::as_str).collect();
                let takes: Vec<&str> = takes.iter().map(String::as_str).collect();
                let _ = scope.retell(&thrown, &file, &export, &names, &takes, &args);
                // What the retelling threw is dropped; the call's error
                // stands.
                let _ = ctx.catch();
                thrown
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_by_name_is_written_into_the_shape_only_when_it_is_an_identifier() {
        let params = [
            DirectParam::Value(JsValue::Undefined),
            DirectParam::Read(JsValue::Undefined, "display".into()),
            DirectParam::Read(JsValue::Undefined, "font.bold".into()),
        ];
        let src = shape_source(&params, 1);
        assert!(src.contains("(function(f,t,r,q0,q1,q2,n2)"), "{src}");
        assert!(src.contains("var x1=q1.display,x2=q2[n2];"), "{src}");
        assert!(src.contains("var p=[q0,x1,x2]"), "{src}");
        assert!(src.contains("f.call(t,q0,x1,x2,a0)"), "{src}");
        assert_eq!(shape_key(&params, 1), "1\u{0}v\u{0}.display\u{0}[\u{0}");
    }
}
