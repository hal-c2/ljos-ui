//! The Android Keystore, through the application's Java side
//! (`dev.ljos.LjosServices`): each secret is a file of the application's
//! own, encrypted with AES-GCM under a key that never leaves the Keystore.
//! The file alone, copied off the device or out of a backup (the files
//! are in the directory the system leaves out of backups), opens to no one.

use ljos_jni::jni::objects::{JByteArray, JValue};
use ljos_jni::jni::{jni_sig, jni_str};
use ljos_jni::{JavaError, services, with_java};

use crate::{Secret, SecretBackend, SecretError};

/// The Android Keystore, as [`SecretStore::system`](crate::SecretStore::system)
/// has it on Android.
///
/// The secrets are the application's alone: no other application reads
/// them, so there are no other clients' names to give it.
#[derive(Debug, Default, Clone, Copy)]
pub struct AndroidKeystore;

impl AndroidKeystore {
    /// The Keystore of the application the program is.
    pub const fn new() -> Self {
        Self
    }
}

fn failed(e: JavaError) -> SecretError {
    match e {
        JavaError::NoJava => SecretError::Unavailable(e.to_string()),
        e => SecretError::Failed(e.to_string()),
    }
}

impl SecretBackend for AndroidKeystore {
    fn write(&self, service: &str, key: &str, secret: &[u8]) -> Result<(), SecretError> {
        with_java(|env, context| {
            let class = services(env, context)?;
            let service = env.new_string(service)?;
            let key = env.new_string(key)?;
            let secret = env.byte_array_from_slice(secret)?;
            env.call_static_method(
                &class,
                jni_str!("secretWrite"),
                jni_sig!("(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;[B)V"),
                &[
                    JValue::Object(context),
                    JValue::from(&service),
                    JValue::from(&key),
                    JValue::from(&secret),
                ],
            )?;
            Ok(())
        })
        .map_err(failed)
    }

    fn read(&self, service: &str, key: &str) -> Result<Option<Secret>, SecretError> {
        with_java(|env, context| {
            let class = services(env, context)?;
            let service = env.new_string(service)?;
            let key = env.new_string(key)?;
            let kept = env
                .call_static_method(
                    &class,
                    jni_str!("secretRead"),
                    jni_sig!("(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)[B"),
                    &[
                        JValue::Object(context),
                        JValue::from(&service),
                        JValue::from(&key),
                    ],
                )?
                .l()?;
            if kept.is_null() {
                return Ok(None);
            }
            let kept: JByteArray<'_> = env.cast_local::<JByteArray>(kept)?;
            Ok(Some(env.convert_byte_array(&kept)?))
        })
        .map(|kept| kept.map(Secret::new))
        .map_err(failed)
    }

    fn remove(&self, service: &str, key: &str) -> Result<bool, SecretError> {
        with_java(|env, context| {
            let class = services(env, context)?;
            let service = env.new_string(service)?;
            let key = env.new_string(key)?;
            env.call_static_method(
                &class,
                jni_str!("secretRemove"),
                jni_sig!("(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)Z"),
                &[
                    JValue::Object(context),
                    JValue::from(&service),
                    JValue::from(&key),
                ],
            )?
            .z()
        })
        .map_err(failed)
    }
}
