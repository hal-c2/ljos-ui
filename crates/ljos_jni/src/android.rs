use jni::objects::{JClass, JObject, JThrowable};
use jni::refs::LoaderContext;
use jni::strings::JNIStr;
use jni::{Env, JavaVM};

use crate::JavaError;

/// Runs `call` on this thread, attached to the VM, with the application's
/// `Context`. An exception `call` leaves pending is taken and is the
/// error; nothing is left pending for the next call.
///
/// # Errors
/// [`JavaError::NoJava`] before the activity glue recorded the VM; what
/// the call threw or JNI refused.
pub fn with_java<T>(
    call: impl FnOnce(&mut Env<'_>, &JObject<'_>) -> jni::errors::Result<T>,
) -> Result<T, JavaError> {
    let android = ndk_context::android_context();
    let (vm, context) = (android.vm(), android.context());
    if vm.is_null() || context.is_null() {
        return Err(JavaError::NoJava);
    }
    // SAFETY: the VM is the process's, recorded by the activity glue; it
    // lives as long as the process.
    let vm = unsafe { JavaVM::from_raw(vm.cast()) };
    let context = context as jni::sys::jobject;
    vm.attach_current_thread(|env| -> Result<T, JavaError> {
        // SAFETY: the Context is a global reference the glue keeps for the
        // process; a cast does not delete it.
        let context = unsafe { env.as_cast_raw::<JObject>(&context)? };
        match call(env, &context) {
            Ok(value) => match take_exception(env) {
                Some(thrown) => Err(JavaError::Thrown(thrown)),
                None => Ok(value),
            },
            Err(e) => Err(match take_exception(env) {
                Some(thrown) => JavaError::Thrown(thrown),
                None => JavaError::Jni(e.to_string()),
            }),
        }
    })
}

/// The class `name` (a binary name, `dev.ljos.LjosServices`) of the
/// application, found through `context`'s class loader.
///
/// # Errors
/// The application ships no such class.
pub fn app_class<'local>(
    env: &mut Env<'local>,
    context: &JObject<'_>,
    name: impl AsRef<JNIStr>,
) -> jni::errors::Result<JClass<'local>> {
    LoaderContext::FromObject(context).load_class(env, name, false)
}

/// The class [`SERVICES`](crate::SERVICES) of the application.
///
/// # Errors
/// The application was not built from Ljós's template.
pub fn services<'local>(
    env: &mut Env<'local>,
    context: &JObject<'_>,
) -> jni::errors::Result<JClass<'local>> {
    app_class(env, context, jni::jni_str!("dev.ljos.LjosServices"))
}

/// Takes the exception pending on `env`, if any, and says what it was.
pub fn take_exception(env: &mut Env<'_>) -> Option<String> {
    if !env.exception_check() {
        return None;
    }
    let thrown: JThrowable<'_> = match env.exception_occurred() {
        Some(thrown) => thrown,
        None => return None,
    };
    env.exception_clear();
    let text = env
        .call_method(
            &thrown,
            jni::jni_str!("toString"),
            jni::jni_sig!("()Ljava/lang/String;"),
            &[],
        )
        .and_then(|v| v.l())
        .and_then(|o| env.cast_local::<jni::objects::JString>(o))
        .and_then(|s| s.try_to_string(env));
    env.exception_clear();
    Some(text.unwrap_or_else(|_| "an exception".to_owned()))
}

impl From<jni::errors::Error> for JavaError {
    fn from(e: jni::errors::Error) -> Self {
        Self::Jni(e.to_string())
    }
}
