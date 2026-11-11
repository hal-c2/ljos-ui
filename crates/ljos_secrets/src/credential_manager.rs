//! The Credential Manager on Windows: generic credentials of the user's
//! logon session.
//!
//! A secret is the generic credential whose target is `<key>.<service>`
//! and whose user name is the key, as Rust's `keyring` names them, so the
//! same secret is found by either. Windows keeps it encrypted with the
//! user's logon credentials, and lets it roam with the user's profile.
//!
//! A dot in a name makes the target ambiguous: service `b.c` with key `a`,
//! and service `c` with key `a.b`, are both `a.b.c`. The key the
//! credential was written for is its user name, so a credential whose user
//! name is another key is another secret: it is not read, replaced or
//! removed for this one.

use std::ffi::c_void;
use std::ptr;

use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_NO_SUCH_LOGON_SESSION, ERROR_NOT_FOUND, FILETIME, GetLastError,
};
use windows_sys::Win32::Security::Credentials::{
    CRED_MAX_CREDENTIAL_BLOB_SIZE, CRED_PERSIST_ENTERPRISE, CRED_TYPE_GENERIC, CREDENTIALW,
    CredDeleteW, CredFree, CredReadW, CredWriteW,
};

use crate::{Secret, SecretBackend, SecretError};

/// The Credential Manager backend.
#[derive(Debug, Clone, Copy, Default)]
pub struct CredentialManager;

/// `text` as a NUL-terminated UTF-16 string.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn target(service: &str, key: &str) -> Vec<u16> {
    wide(&format!("{key}.{service}"))
}

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

/// The credential at `target`, if there is one: its user name, and its
/// blob when `with_blob`.
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
    // user name, when not null, is NUL-terminated, until CredFree.
    let found = unsafe {
        let read = &*credential;
        let user_name = if read.UserName.is_null() {
            Vec::new()
        } else {
            let len = (0..).take_while(|&i| *read.UserName.add(i) != 0).count();
            std::slice::from_raw_parts(read.UserName, len).to_vec()
        };
        let blob = if with_blob && !read.CredentialBlob.is_null() {
            std::slice::from_raw_parts(read.CredentialBlob, read.CredentialBlobSize as usize)
                .to_vec()
        } else {
            Vec::new()
        };
        CredFree(credential.cast::<c_void>());
        Found {
            user_name,
            secret: Secret::new(blob),
        }
    };
    Ok(Some(found))
}

struct Found {
    /// UTF-16, without the NUL.
    user_name: Vec<u16>,
    secret: Secret,
}

impl Found {
    fn is_for(&self, key: &str) -> bool {
        self.user_name.iter().copied().eq(key.encode_utf16())
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
            && !found.is_for(key)
        {
            return Err(SecretError::Failed(format!(
                "the credential for key {key:?} of {service:?} has the name of another \
                 key's, as a dot in the service or the key makes it; it is left as it is"
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
        Ok(existing(&target(service, key), true)?
            .filter(|found| found.is_for(key))
            .map(|found| found.secret))
    }

    fn remove(&self, service: &str, key: &str) -> Result<bool, SecretError> {
        let target = target(service, key);
        match existing(&target, false)? {
            Some(found) if found.is_for(key) => {}
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
