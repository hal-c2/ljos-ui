//! Secrets the system keeps safe: a password, a token or a key, written,
//! read and removed by the service it belongs to and a key within that
//! service.
//!
//! [`SecretStore::system`] is the store of the system the program runs on:
//!
//! - Linux: the Secret Service on the session bus (GNOME Keyring, KWallet,
//!   KeePassXC), in its default collection ([`SecretService`]);
//! - macOS: the user's keychain ([`Keychain`]);
//! - Windows: the Credential Manager ([`CredentialManager`]);
//! - anywhere else, [`NoSecureStorage`]: every call is
//!   [`SecretError::Unsupported`].
//!
//! A secret the system cannot keep is an error, never a file: a write to a
//! keyring that is locked, and that the user does not unlock when asked, is
//! [`SecretError::Locked`]; one with no keyring running is
//! [`SecretError::Unavailable`]. Nothing here writes a secret anywhere but
//! the system's store.
//!
//! Every call can wait: for the bus, or for the user to unlock the keyring
//! in a dialog of the system's. Call the store from a thread of your own,
//! never the Engine's, and hand what it answers back to the UI as a message.
//!
//! [`MemorySecrets`] keeps secrets in the process, for tests and for hosts
//! that want none kept; it can be told to act locked or absent.

#[cfg(target_os = "windows")]
mod credential_manager;
#[cfg(target_os = "macos")]
mod keychain;
mod memory;
#[cfg(target_os = "linux")]
mod secret_service;

use std::fmt;

#[cfg(target_os = "windows")]
pub use credential_manager::CredentialManager;
#[cfg(target_os = "macos")]
pub use keychain::Keychain;
pub use memory::{MemorySecrets, MemoryState};
#[cfg(target_os = "linux")]
pub use secret_service::SecretService;

/// Why a secret was not written, read or removed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SecretError {
    /// The system has no secure storage this crate can reach.
    #[error("this system has no secure storage for secrets")]
    Unsupported,
    /// The system's store is not there: no Secret Service on the bus, no
    /// keychain, no logon session.
    #[error("no secure storage is available: {0}")]
    Unavailable(String),
    /// The store is locked, and was not unlocked when the user was asked.
    #[error("the secure storage is locked")]
    Locked,
    /// The store, or the user, refused this program the secret.
    #[error("the secure storage refused access")]
    Denied,
    /// The service or the key cannot name a secret.
    #[error("a secret's {name} {reason}")]
    InvalidName {
        /// `"service"` or `"key"`.
        name: &'static str,
        reason: &'static str,
    },
    /// The secret is larger than the store keeps.
    #[error("the secret is {len} bytes, and the secure storage keeps at most {max}")]
    TooLarge { len: usize, max: usize },
    /// The store failed for a reason of its own, which the text gives.
    #[error("the secure storage failed: {0}")]
    Failed(String),
}

/// What a system's store does. [`SecretStore`] checks the names before it
/// asks, so a backend sees a service and a key that are neither empty nor
/// hold a NUL character.
pub trait SecretBackend: Send + Sync + fmt::Debug {
    /// Keeps `secret` under `service` and `key`, in place of any secret
    /// kept there.
    fn write(&self, service: &str, key: &str, secret: &[u8]) -> Result<(), SecretError>;
    /// The secret kept under `service` and `key`, if there is one.
    fn read(&self, service: &str, key: &str) -> Result<Option<Secret>, SecretError>;
    /// Removes the secret kept under `service` and `key`; whether there was
    /// one.
    fn remove(&self, service: &str, key: &str) -> Result<bool, SecretError>;
}

/// Secrets by service and key, kept by a [`SecretBackend`].
///
/// Shared between threads as it is (`Send + Sync`); each call waits for
/// the store (see the crate's documentation).
#[derive(Debug)]
pub struct SecretStore {
    backend: Box<dyn SecretBackend>,
}

impl SecretStore {
    /// The store of the system the program runs on.
    pub fn system() -> Self {
        #[cfg(target_os = "linux")]
        let backend = SecretService::session();
        #[cfg(target_os = "macos")]
        let backend = Keychain;
        #[cfg(target_os = "windows")]
        let backend = CredentialManager;
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        let backend = NoSecureStorage;
        Self::new(backend)
    }

    /// A store kept by `backend`.
    pub fn new(backend: impl SecretBackend + 'static) -> Self {
        Self {
            backend: Box::new(backend),
        }
    }

    /// Keeps `secret` under `service` and `key`, in place of any secret
    /// kept there. An error means nothing was kept.
    pub fn write(&self, service: &str, key: &str, secret: &[u8]) -> Result<(), SecretError> {
        check_names(service, key)?;
        self.backend.write(service, key, secret)
    }

    /// The secret kept under `service` and `key`; `None` when there is none.
    pub fn read(&self, service: &str, key: &str) -> Result<Option<Secret>, SecretError> {
        check_names(service, key)?;
        self.backend.read(service, key)
    }

    /// Removes the secret kept under `service` and `key`. `false` when
    /// there was none, which is not an error.
    pub fn remove(&self, service: &str, key: &str) -> Result<bool, SecretError> {
        check_names(service, key)?;
        self.backend.remove(service, key)
    }
}

// The stores take names as C strings (Windows) or D-Bus strings, which end
// at a NUL: two names that differ after one would name the same secret.
fn check_names(service: &str, key: &str) -> Result<(), SecretError> {
    for (name, value) in [("service", service), ("key", key)] {
        if value.is_empty() {
            return Err(SecretError::InvalidName {
                name,
                reason: "is empty",
            });
        }
        if value.contains('\0') {
            return Err(SecretError::InvalidName {
                name,
                reason: "holds a NUL character",
            });
        }
    }
    Ok(())
}

/// A secret read from a store. Its bytes are overwritten when it is
/// dropped, and its `Debug` shows none of them, so that a log line or a
/// panic message does not carry it.
///
/// The copies the store's own calls make on the way (the bus's buffers, the
/// system's) are theirs to clear.
pub struct Secret(Vec<u8>);

impl Secret {
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self(bytes.into())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// The secret as text, when it is UTF-8.
    pub fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.0).ok()
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        for byte in self.0.iter_mut() {
            // Volatile, so the stores are not left out as dead writes to
            // memory about to be freed.
            // SAFETY: `byte` is a valid, aligned, exclusive reference.
            unsafe { std::ptr::write_volatile(byte, 0) };
        }
        std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(..)")
    }
}

/// The backend of a system with no secure storage: every call is
/// [`SecretError::Unsupported`].
#[derive(Debug, Clone, Copy, Default)]
pub struct NoSecureStorage;

impl SecretBackend for NoSecureStorage {
    fn write(&self, _: &str, _: &str, _: &[u8]) -> Result<(), SecretError> {
        Err(SecretError::Unsupported)
    }

    fn read(&self, _: &str, _: &str) -> Result<Option<Secret>, SecretError> {
        Err(SecretError::Unsupported)
    }

    fn remove(&self, _: &str, _: &str) -> Result<bool, SecretError> {
        Err(SecretError::Unsupported)
    }
}

#[cfg(test)]
mod tests;
