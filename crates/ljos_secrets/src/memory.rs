//! A store in the process, for tests and for hosts that keep no secrets.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::{Secret, SecretBackend, SecretError};

/// How a [`MemorySecrets`] answers: as an open store, or as a locked or
/// absent one, which refuses every call as a system's store would.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MemoryState {
    #[default]
    Open,
    /// Every call is [`SecretError::Locked`].
    Locked,
    /// Every call is [`SecretError::Unavailable`].
    Absent,
}

/// Secrets kept in the process, gone when it ends. Clones share the
/// secrets, so a test can keep one and hand another to a [`SecretStore`].
///
/// [`SecretStore`]: crate::SecretStore
#[derive(Debug, Clone, Default)]
pub struct MemorySecrets {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Default)]
struct Inner {
    state: MemoryState,
    secrets: HashMap<(String, String), Secret>,
}

impl std::fmt::Debug for Inner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inner")
            .field("state", &self.state)
            .field("secrets", &self.secrets.len())
            .finish()
    }
}

impl MemorySecrets {
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes the store answer as `state` from the next call on. The secrets
    /// it keeps stay.
    pub fn set_state(&self, state: MemoryState) {
        self.lock().state = state;
    }

    /// How many secrets the store keeps, whatever its state.
    pub fn len(&self) -> usize {
        self.lock().secrets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // A test that panicked while holding it left the map whole.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn open(&self) -> Result<MutexGuard<'_, Inner>, SecretError> {
        let inner = self.lock();
        match inner.state {
            MemoryState::Open => Ok(inner),
            MemoryState::Locked => Err(SecretError::Locked),
            MemoryState::Absent => Err(SecretError::Unavailable(
                "the store in memory is set absent".to_owned(),
            )),
        }
    }
}

impl SecretBackend for MemorySecrets {
    fn write(&self, service: &str, key: &str, secret: &[u8]) -> Result<(), SecretError> {
        self.open()?.secrets.insert(
            (service.to_owned(), key.to_owned()),
            Secret::new(secret.to_vec()),
        );
        Ok(())
    }

    fn read(&self, service: &str, key: &str) -> Result<Option<Secret>, SecretError> {
        Ok(self
            .open()?
            .secrets
            .get(&(service.to_owned(), key.to_owned()))
            .map(|secret| Secret::new(secret.as_bytes().to_vec())))
    }

    fn remove(&self, service: &str, key: &str) -> Result<bool, SecretError> {
        Ok(self
            .open()?
            .secrets
            .remove(&(service.to_owned(), key.to_owned()))
            .is_some())
    }
}
