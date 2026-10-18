//! Classic scripts compiled ahead of their first [`JsHost::eval_script`].
//!
//! The scripts every host runs first are the same for the whole process,
//! and QuickJS spends most of their evaluation parsing them. They are
//! compiled once, on a thread of their own while the program does its other
//! starting work, and every host then reads the bytecode instead: reading is
//! several times cheaper than parsing. A script that was not compiled ahead,
//! or whose text is not the text compiled, is parsed as before.
//!
//! [`JsHost::eval_script`]: crate::JsHost::eval_script

use std::{
    ffi::CString,
    mem::MaybeUninit,
    sync::{Arc, Mutex, OnceLock},
    thread::JoinHandle,
};

use rquickjs::{Context, Ctx, Runtime, Value, qjs};

/// The file name QuickJS gives a script `Ctx::eval` runs: the compiled one
/// keeps it, so that a stack reads the same either way.
const FILE_NAME: &std::ffi::CStr = c"eval_script";

/// One script and, where it compiled, its bytecode.
struct Compiled {
    name: String,
    source: String,
    bytecode: Option<Vec<u8>>,
}

enum State {
    Compiling(JoinHandle<Vec<Compiled>>),
    Ready(Arc<[Compiled]>),
}

static AHEAD: OnceLock<Mutex<State>> = OnceLock::new();

/// Starts compiling the scripts `scripts` gives, on a thread that ends when
/// they are compiled. Only the first call in a process does anything.
///
/// `scripts` runs on that thread too, so that making their texts costs the
/// caller nothing.
pub fn compile_ahead<F>(scripts: F)
where
    F: FnOnce() -> Vec<(String, String)> + Send + 'static,
{
    AHEAD.get_or_init(|| {
        let job = move || {
            scripts()
                .into_iter()
                .map(|(name, source)| {
                    let bytecode = compile(&source);
                    Compiled {
                        name,
                        source,
                        bytecode,
                    }
                })
                .collect::<Vec<_>>()
        };
        Mutex::new(
            match std::thread::Builder::new()
                .name("ljos-scripts".into())
                .spawn(job)
            {
                Ok(handle) => State::Compiling(handle),
                // Without a thread, every script is parsed where it runs.
                Err(_) => State::Ready(Arc::from([])),
            },
        )
    });
}

/// A script compiled ahead: the bytecode of all of them and which is this one.
pub(crate) struct Script(Arc<[Compiled]>, usize);

/// The script `name` compiled ahead, if its text is `source`. Waits for the
/// compiling thread when it has not finished: the work is under way, and
/// starting it again here would only add to it.
pub(crate) fn script(name: &str, source: &str) -> Option<Script> {
    let lock = AHEAD.get()?;
    let mut state = lock
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if matches!(&*state, State::Compiling(_)) {
        let State::Compiling(handle) = std::mem::replace(&mut *state, State::Ready(Arc::from([])))
        else {
            unreachable!("matched above")
        };
        // A thread that panicked compiled nothing.
        *state = State::Ready(handle.join().map_or_else(|_| Arc::from([]), Arc::from));
    }
    let State::Ready(all) = &*state else {
        return None;
    };
    let at = all
        .iter()
        .position(|c| c.name == name && c.source == source && c.bytecode.is_some())?;
    Some(Script(all.clone(), at))
}

impl Script {
    /// Runs the script as `Ctx::eval` would run its text: global code,
    /// strict.
    pub(crate) fn eval<'js>(&self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
        let Some(bytes) = self.0[self.1].bytecode.as_deref() else {
            return Err(rquickjs::Error::Unknown);
        };
        let ptr = ctx.as_raw().as_ptr();
        // SAFETY: the bytes were written by `JS_WriteObject` from a script
        // this same QuickJS compiled in this process, so they are well-formed
        // bytecode; `JS_EvalFunction` takes the function object read from
        // them, and the value it gives is handed to rquickjs to own.
        unsafe {
            let fun = qjs::JS_ReadObject(
                ptr,
                bytes.as_ptr(),
                bytes.len() as _,
                qjs::JS_READ_OBJ_BYTECODE as i32,
            );
            if qjs::JS_VALUE_GET_NORM_TAG(fun) == qjs::JS_TAG_EXCEPTION {
                return Err(rquickjs::Error::Exception);
            }
            let v = qjs::JS_EvalFunction(ptr, fun);
            if qjs::JS_VALUE_GET_NORM_TAG(v) == qjs::JS_TAG_EXCEPTION {
                return Err(rquickjs::Error::Exception);
            }
            Ok(Value::from_raw(ctx.clone(), v))
        }
    }
}

/// The bytecode of `source` compiled as `Ctx::eval` compiles it, or `None`
/// for a script that does not compile: that one is parsed where it runs,
/// which reports the error as it always has.
fn compile(source: &str) -> Option<Vec<u8>> {
    let rt = Runtime::new().ok()?;
    let ctx = Context::full(&rt).ok()?;
    let text = CString::new(source).ok()?;
    ctx.with(|ctx| {
        let ptr = ctx.as_raw().as_ptr();
        let flags =
            qjs::JS_EVAL_TYPE_GLOBAL | qjs::JS_EVAL_FLAG_STRICT | qjs::JS_EVAL_FLAG_COMPILE_ONLY;
        // SAFETY: `text` is a NUL-terminated copy of `source` that outlives
        // the call; the compiled function and the buffer written from it are
        // freed here, on the runtime that made them.
        unsafe {
            let fun = qjs::JS_Eval(
                ptr,
                text.as_ptr(),
                source.len() as _,
                FILE_NAME.as_ptr(),
                flags as i32,
            );
            if qjs::JS_VALUE_GET_NORM_TAG(fun) == qjs::JS_TAG_EXCEPTION {
                // Drops the pending exception with the runtime.
                let _ = ctx.catch();
                return None;
            }
            let mut len = MaybeUninit::uninit();
            let buf = qjs::JS_WriteObject(
                ptr,
                len.as_mut_ptr(),
                fun,
                qjs::JS_WRITE_OBJ_BYTECODE as i32,
            );
            qjs::JS_FreeValue(ptr, fun);
            if buf.is_null() {
                let _ = ctx.catch();
                return None;
            }
            let bytes = std::slice::from_raw_parts(buf, len.assume_init() as usize).to_vec();
            qjs::js_free(ptr, buf.cast());
            Some(bytes)
        }
    })
}
