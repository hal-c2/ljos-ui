//! The user's keychain on macOS, through Security.framework's item calls.
//!
//! A secret is a generic password whose service is the service and whose
//! account is the key: the item Keychain Access shows under that name, and
//! the one `security find-generic-password -s <service> -a <key>` finds.
//! [`Keychain::with_names`] gives the service and the account of another
//! client's items instead.

use std::ptr::{self, NonNull};
use std::sync::Arc;

use objc2_core_foundation::{CFBoolean, CFData, CFDictionary, CFRetained, CFString, CFType};

use crate::names::{self, DefaultNames, ItemNames};
use crate::{Secret, SecretBackend, SecretError};

type OSStatus = i32;

// Security.framework's result codes (SecBase.h).
const ERR_SEC_SUCCESS: OSStatus = 0;
const ERR_SEC_USER_CANCELED: OSStatus = -128;
const ERR_SEC_NOT_AVAILABLE: OSStatus = -25291;
const ERR_SEC_AUTH_FAILED: OSStatus = -25293;
const ERR_SEC_NO_SUCH_KEYCHAIN: OSStatus = -25294;
const ERR_SEC_ITEM_NOT_FOUND: OSStatus = -25300;
const ERR_SEC_NO_DEFAULT_KEYCHAIN: OSStatus = -25307;
const ERR_SEC_INTERACTION_NOT_ALLOWED: OSStatus = -25308;

#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    static kSecClass: &'static CFString;
    static kSecClassGenericPassword: &'static CFString;
    static kSecAttrService: &'static CFString;
    static kSecAttrAccount: &'static CFString;
    static kSecValueData: &'static CFString;
    static kSecReturnData: &'static CFString;
    static kSecMatchLimit: &'static CFString;
    static kSecMatchLimitOne: &'static CFString;

    fn SecItemAdd(attributes: &CFDictionary, result: *mut *const CFType) -> OSStatus;
    fn SecItemCopyMatching(query: &CFDictionary, result: *mut *const CFType) -> OSStatus;
    fn SecItemUpdate(query: &CFDictionary, attributes: &CFDictionary) -> OSStatus;
    fn SecItemDelete(query: &CFDictionary) -> OSStatus;
}

/// The Keychain backend.
#[derive(Debug, Clone)]
pub struct Keychain {
    names: Arc<dyn ItemNames>,
}

impl Default for Keychain {
    fn default() -> Self {
        Self::new()
    }
}

impl Keychain {
    /// The user's keychain, items named by service and key.
    pub fn new() -> Self {
        Self {
            names: Arc::new(DefaultNames),
        }
    }

    /// The same keychain, items named by `names`'s
    /// [service](ItemNames::keychain_service) and
    /// [account](ItemNames::keychain_account).
    pub fn with_names(self, names: impl ItemNames + 'static) -> Self {
        Self {
            names: Arc::new(names),
        }
    }

    /// The query that finds the item of `service` and `key`, with `more`.
    fn query(
        &self,
        service: &str,
        key: &str,
        more: &[(&CFString, &CFType)],
    ) -> Result<CFRetained<CFDictionary<CFString, CFType>>, SecretError> {
        let (service, account) = names::keychain(&*self.names, service, key)?;
        Ok(query(&service, &account, more))
    }
}

fn as_type<T: AsRef<CFType> + ?Sized>(value: &T) -> &CFType {
    value.as_ref()
}

fn dictionary(entries: &[(&CFString, &CFType)]) -> CFRetained<CFDictionary<CFString, CFType>> {
    let keys: Vec<&CFString> = entries.iter().map(|(k, _)| *k).collect();
    let values: Vec<&CFType> = entries.iter().map(|(_, v)| *v).collect();
    CFDictionary::from_slices(&keys, &values)
}

/// The query that finds the item of the Keychain's `service` and
/// `account`, with `more`.
fn query(
    service: &str,
    account: &str,
    more: &[(&CFString, &CFType)],
) -> CFRetained<CFDictionary<CFString, CFType>> {
    let service = CFString::from_str(service);
    let account = CFString::from_str(account);
    // SAFETY: Security.framework's constants are set before any code of
    // the program runs and never change.
    let mut entries = unsafe {
        vec![
            (kSecClass, as_type(kSecClassGenericPassword)),
            (kSecAttrService, as_type(&*service)),
            (kSecAttrAccount, as_type(&*account)),
        ]
    };
    entries.extend_from_slice(more);
    dictionary(&entries)
}

fn failure(status: OSStatus) -> SecretError {
    match status {
        ERR_SEC_INTERACTION_NOT_ALLOWED => SecretError::Locked,
        ERR_SEC_USER_CANCELED | ERR_SEC_AUTH_FAILED => SecretError::Denied,
        ERR_SEC_NOT_AVAILABLE | ERR_SEC_NO_SUCH_KEYCHAIN | ERR_SEC_NO_DEFAULT_KEYCHAIN => {
            SecretError::Unavailable(format!("the keychain answered {status}"))
        }
        _ => SecretError::Failed(format!("the keychain answered {status}")),
    }
}

impl SecretBackend for Keychain {
    fn write(&self, service: &str, key: &str, secret: &[u8]) -> Result<(), SecretError> {
        let data = CFData::from_bytes(secret);
        // SAFETY: as in `query`.
        let value = unsafe { [(kSecValueData, as_type(&*data))] };
        // Updating first keeps one item, with the access list the user may
        // have given it, instead of adding a duplicate.
        let found = self.query(service, key, &[])?;
        let changes = dictionary(&value);
        // SAFETY: both are valid dictionaries for the call's duration.
        let status = unsafe { SecItemUpdate(found.as_opaque(), changes.as_opaque()) };
        match status {
            ERR_SEC_SUCCESS => Ok(()),
            ERR_SEC_ITEM_NOT_FOUND => {
                let item = self.query(service, key, &value)?;
                // SAFETY: a valid dictionary; no result is asked for.
                let status = unsafe { SecItemAdd(item.as_opaque(), ptr::null_mut()) };
                if status == ERR_SEC_SUCCESS {
                    Ok(())
                } else {
                    Err(failure(status))
                }
            }
            _ => Err(failure(status)),
        }
    }

    fn read(&self, service: &str, key: &str) -> Result<Option<Secret>, SecretError> {
        // SAFETY: as in `query`.
        let more = unsafe {
            [
                (kSecReturnData, as_type(CFBoolean::new(true))),
                (kSecMatchLimit, as_type(kSecMatchLimitOne)),
            ]
        };
        let found = self.query(service, key, &more)?;
        let mut result: *const CFType = ptr::null();
        // SAFETY: a valid dictionary, and a place for the result.
        let status = unsafe { SecItemCopyMatching(found.as_opaque(), &mut result) };
        match status {
            ERR_SEC_SUCCESS => {}
            ERR_SEC_ITEM_NOT_FOUND => return Ok(None),
            _ => return Err(failure(status)),
        }
        let Some(result) = NonNull::new(result.cast_mut()) else {
            return Err(SecretError::Failed(
                "the keychain found the item and returned no data".to_owned(),
            ));
        };
        // SAFETY: a copy function returns its result retained, and the
        // caller owns it.
        let result = unsafe { CFRetained::from_raw(result) };
        let data = result.downcast::<CFData>().map_err(|_| {
            SecretError::Failed("the keychain returned something other than data".to_owned())
        })?;
        Ok(Some(Secret::new(data.to_vec())))
    }

    fn remove(&self, service: &str, key: &str) -> Result<bool, SecretError> {
        let found = self.query(service, key, &[])?;
        // SAFETY: a valid dictionary.
        match unsafe { SecItemDelete(found.as_opaque()) } {
            ERR_SEC_SUCCESS => Ok(true),
            ERR_SEC_ITEM_NOT_FOUND => Ok(false),
            status => Err(failure(status)),
        }
    }
}
