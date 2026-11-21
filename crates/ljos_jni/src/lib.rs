//! An Android application's Java side, for the crates that reach the
//! system's services through it (ARCHITECTURE.md §10.1): the clipboard,
//! the opener of links, the places an application keeps its files, the
//! store of its secrets.
//!
//! The process's VM and the application's `Context` are the ones the
//! activity glue records as the process starts (`ndk-context`). A call may
//! come from any thread: [`with_java`] attaches it to the VM for the call.
//! A class the application ships (`dev.ljos.LjosServices`) is found
//! through the Context's class loader ([`app_class`]), since a thread
//! started by native code sees only the system's classes by default.
//!
//! Anywhere but Android there is no Java side, and every call is
//! [`JavaError::NoJava`]; the crate builds everywhere so that its users
//! need no gates of their own beyond the call.

#[cfg(target_os = "android")]
mod android;

#[cfg(target_os = "android")]
pub use android::{app_class, services, take_exception, with_java};
#[cfg(target_os = "android")]
pub use jni;

/// The class of the system's services that Ljós's Android template ships
/// (`android/app/src/main/java/dev/ljos/LjosServices.java`); every method
/// of it is static and takes the application's `Context` first.
pub const SERVICES: &str = "dev.ljos.LjosServices";

/// Why a call to the Java side did not answer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum JavaError {
    /// The program is not an Android application, or the activity glue
    /// has not recorded the VM yet.
    #[error("there is no Java side to call")]
    NoJava,
    /// The call threw; the text is the exception's.
    #[error("the Java side threw: {0}")]
    Thrown(String),
    /// JNI refused the call: a missing class or method, a wrong signature.
    #[error("the Java call failed: {0}")]
    Jni(String),
}

#[cfg(not(target_os = "android"))]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_say_what_failed() {
        assert_eq!(
            JavaError::NoJava.to_string(),
            "there is no Java side to call"
        );
        assert_eq!(
            JavaError::Thrown("java.lang.SecurityException".into()).to_string(),
            "the Java side threw: java.lang.SecurityException"
        );
    }
}

#[cfg(target_os = "android")]
#[cfg(test)]
mod android_tests {
    use super::*;

    // A test binary run on a device has no activity glue: nothing
    // recorded the VM.
    #[test]
    fn a_call_before_the_glue_recorded_the_vm_is_no_java() {
        assert_eq!(with_java(|_, _| Ok(())), Err(JavaError::NoJava));
        // Asked again, it answers the same.
        assert_eq!(with_java(|_, _| Ok(())), Err(JavaError::NoJava));
    }
}
