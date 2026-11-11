//! The Credential Manager on Windows: generic credentials of the user's
//! logon session.
//!
//! A secret is the generic credential whose target is `<key>.<service>`
//! and whose user name is the key, as Rust's `keyring` names them, so the
//! same secret is found by either. Windows keeps it encrypted with the
//! user's logon credentials, and lets it roam with the user's profile.
//!
//! Two names can come to one target: through a dot (service `b.c` with
//! key `a`, and service `c` with key `a.b`, are both `a.b.c`), and through
//! case, which Windows ignores when it finds a target (`Mail` and `mail`).
//! A credential keeps its target as it was written and its key as its user
//! name, so one whose target or user name differs from this secret's, even
//! only in case, is another secret: it is not read, replaced or removed
//! for this one.

use std::ffi::c_void;
use std::ptr;

use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_NO_SUCH_LOGON_SESSION, ERROR_NOT_FOUND, FILETIME, GetLastError,
};
use windows_sys::Win32::Security::Credentials::{
    CRED_MAX_CREDENTIAL_BLOB_SIZE, CRED_PERSIST_ENTERPRISE, CRED_TYPE_GENERIC, CREDENTIALW,
    CredDeleteW, CredFree, CredReadW, CredWriteW,
};

use crate::credential_names::{target, wide};
use crate::{Secret, SecretBackend, SecretError};

/// The Credential Manager backend.
#[derive(Debug, Clone, Copy, Default)]
pub struct CredentialManager;

fn last_error() -> SecretError {
    // SAFETY: reads the calling thread's last error, nothing else.
    let code = unsafe { GetLastError() };
    match code {
        ERROR_NO_SUCH_LOGON_SESSION => SecretError::Unavailable(
            "the Credential Manager has no logon session to keep it in".to_owned(),
        ),
        ERROR_ACCESS_DENIED => SecretError::Denied,
        _ => SecretError::Failed(format!("the Credential Manager answered error {code}")),
    }
}

/// The credential Windows finds at `target`, if there is one: its names as
/// written, and its blob when `with_blob`.
fn existing(target: &[u16], with_blob: bool) -> Result<Option<Found>, SecretError> {
    let mut credential: *mut CREDENTIALW = ptr::null_mut();
    // SAFETY: a NUL-terminated target, and a place for the result.
    if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) } == 0 {
        // SAFETY: as in `last_error`.
        if unsafe { GetLastError() } == ERROR_NOT_FOUND {
            return Ok(None);
        }
        return Err(last_error());
    }
    // SAFETY: CredReadW succeeded, so `credential` points to a credential
    // it allocated, whose blob holds CredentialBlobSize bytes and whose
    // names, when not null, are NUL-terminated, until CredFree.
    let found = unsafe {
        let read = &*credential;
        let target_name = until_nul(read.TargetName);
        let user_name = until_nul(read.UserName);
        let blob = if with_blob && !read.CredentialBlob.is_null() {
            std::slice::from_raw_parts(read.CredentialBlob, read.CredentialBlobSize as usize)
                .to_vec()
        } else {
            Vec::new()
        };
        CredFree(credential.cast::<c_void>());
        Found {
            target_name,
            user_name,
            secret: Secret::new(blob),
        }
    };
    Ok(Some(found))
}

/// The UTF-16 string at `text` without its NUL; empty when null.
///
/// # Safety
///
/// `text` is null or points to a NUL-terminated UTF-16 string.
unsafe fn until_nul(text: *const u16) -> Vec<u16> {
    if text.is_null() {
        return Vec::new();
    }
    // SAFETY: the string ends at a NUL, as the caller promises.
    unsafe {
        let len = (0..).take_while(|&i| *text.add(i) != 0).count();
        std::slice::from_raw_parts(text, len).to_vec()
    }
}

struct Found {
    /// UTF-16, without the NUL.
    target_name: Vec<u16>,
    /// UTF-16, without the NUL.
    user_name: Vec<u16>,
    secret: Secret,
}

impl Found {
    fn is_for(&self, target: &[u16], key: &str) -> bool {
        crate::credential_names::is_for(&self.target_name, &self.user_name, target, key)
    }
}

impl SecretBackend for CredentialManager {
    fn write(&self, service: &str, key: &str, secret: &[u8]) -> Result<(), SecretError> {
        let max = CRED_MAX_CREDENTIAL_BLOB_SIZE as usize;
        if secret.len() > max {
            return Err(SecretError::TooLarge {
                len: secret.len(),
                max,
            });
        }
        let mut target = target(service, key);
        if let Some(found) = existing(&target, false)?
            && !found.is_for(&target, key)
        {
            return Err(SecretError::Failed(format!(
                "the Credential Manager keeps another secret where key {key:?} of \
                 {service:?} would go: its name differs only in case, or in where a dot \
                 falls; it is left as it is"
            )));
        }
        let mut user = wide(key);
        let credential = CREDENTIALW {
            Flags: 0,
            Type: CRED_TYPE_GENERIC,
            TargetName: target.as_mut_ptr(),
            Comment: ptr::null_mut(),
            LastWritten: FILETIME {
                dwLowDateTime: 0,
                dwHighDateTime: 0,
            },
            // At most CRED_MAX_CREDENTIAL_BLOB_SIZE, checked above.
            CredentialBlobSize: secret.len() as u32,
            // CredWriteW only reads the blob.
            CredentialBlob: secret.as_ptr().cast_mut(),
            Persist: CRED_PERSIST_ENTERPRISE,
            AttributeCount: 0,
            Attributes: ptr::null_mut(),
            TargetAlias: ptr::null_mut(),
            UserName: user.as_mut_ptr(),
        };
        // SAFETY: every pointer in `credential` is valid for the call.
        if unsafe { CredWriteW(&credential, 0) } == 0 {
            return Err(last_error());
        }
        Ok(())
    }

    fn read(&self, service: &str, key: &str) -> Result<Option<Secret>, SecretError> {
        let target = target(service, key);
        Ok(existing(&target, true)?
            .filter(|found| found.is_for(&target, key))
            .map(|found| found.secret))
    }

    fn remove(&self, service: &str, key: &str) -> Result<bool, SecretError> {
        let target = target(service, key);
        match existing(&target, false)? {
            Some(found) if found.is_for(&target, key) => {}
            _ => return Ok(false),
        }
        // SAFETY: a NUL-terminated target.
        if unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) } == 0 {
            // SAFETY: as in `last_error`.
            if unsafe { GetLastError() } == ERROR_NOT_FOUND {
                return Ok(false);
            }
            return Err(last_error());
        }
        Ok(true)
    }
}
