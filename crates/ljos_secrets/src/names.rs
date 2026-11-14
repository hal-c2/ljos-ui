//! What a secret's item is called in each system's store.
//!
//! A store finds a secret by the names its item carries: the Secret
//! Service by attributes, the Credential Manager by a target and a user
//! name, the Keychain by a service and an account. [`DefaultNames`] gives
//! the names Rust's `keyring` and `secret-tool` users find theirs by. A
//! program that shares its secrets with another client, one that named
//! them otherwise before, gives the backend an [`ItemNames`] of its own and
//! reads and writes that client's items.
//!
//! The trait has every system's names, whichever system the program runs
//! on, so that one value names a program's secrets everywhere.

use std::collections::BTreeMap;
use std::fmt;

use crate::SecretError;

/// The names of the item that keeps the secret of `key` in `service`.
/// Each has a default: [`DefaultNames`] overrides none.
///
/// Two secrets must not get the same names, or the store keeps one item
/// for both; the default names keep them apart (see each method).
pub trait ItemNames: Send + Sync + fmt::Debug {
    /// The Secret Service attributes the item is found by: `service` and
    /// `username`.
    ///
    /// The service finds every item that has these attributes, whatever
    /// others it has, so the set is never empty: an empty one would find
    /// every item in the keyring.
    fn secret_service_attributes(&self, service: &str, key: &str) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("service".to_owned(), service.to_owned()),
            ("username".to_owned(), key.to_owned()),
        ])
    }

    /// The Secret Service label, the name a keyring's window shows:
    /// `<service>: <key>`. It does not find the item.
    fn secret_service_label(&self, service: &str, key: &str) -> String {
        format!("{service}: {key}")
    }

    /// The Credential Manager target: `<key>.<service>`.
    ///
    /// Windows finds a target whatever its case. The backend reads,
    /// replaces and removes only a credential whose target is this one
    /// exactly, so two targets that differ only in case name two secrets,
    /// and the second one written is refused.
    fn credential_target(&self, service: &str, key: &str) -> String {
        format!("{key}.{service}")
    }

    /// The Credential Manager user name: the key. A dot lets two secrets
    /// share the default target (key `a` of `b.c` and key `a.b` of `c`),
    /// and the user name tells them apart.
    ///
    /// `None` writes a credential with no user name, and takes the
    /// credential at the target as the secret whatever its user name: the
    /// target alone must then name one secret.
    fn credential_user_name(&self, service: &str, key: &str) -> Option<String> {
        let _ = service;
        Some(key.to_owned())
    }

    /// The Keychain item's service: the service.
    fn keychain_service(&self, service: &str, key: &str) -> String {
        let _ = key;
        service.to_owned()
    }

    /// The Keychain item's account: the key.
    fn keychain_account(&self, service: &str, key: &str) -> String {
        let _ = service;
        key.to_owned()
    }
}

/// The names Rust's `keyring` gives an item, which other programs find
/// theirs by: every default of [`ItemNames`].
#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultNames;

impl ItemNames for DefaultNames {}

// The stores take these names as C strings (Windows) or D-Bus strings,
// which end at a NUL, as `check_names` says of the service and the key.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows", test))]
fn check(name: &'static str, value: &str) -> Result<(), SecretError> {
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
    Ok(())
}

/// The Secret Service attributes, checked: at least one, each named, and
/// no NUL in a name or a value.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn secret_service_attributes(
    names: &dyn ItemNames,
    service: &str,
    key: &str,
) -> Result<BTreeMap<String, String>, SecretError> {
    let attributes = names.secret_service_attributes(service, key);
    if attributes.is_empty() {
        return Err(SecretError::InvalidName {
            name: "Secret Service attributes",
            reason: "are none, which would find every item",
        });
    }
    for (name, value) in &attributes {
        check("Secret Service attribute", name)?;
        if value.contains('\0') {
            return Err(SecretError::InvalidName {
                name: "Secret Service attribute",
                reason: "holds a NUL character",
            });
        }
    }
    Ok(attributes)
}

/// The Secret Service label, checked: it may be empty.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn secret_service_label(
    names: &dyn ItemNames,
    service: &str,
    key: &str,
) -> Result<String, SecretError> {
    let label = names.secret_service_label(service, key);
    if label.contains('\0') {
        return Err(SecretError::InvalidName {
            name: "Secret Service label",
            reason: "holds a NUL character",
        });
    }
    Ok(label)
}

/// The Credential Manager target and user name, checked.
#[cfg(any(target_os = "windows", test))]
pub(crate) fn credential(
    names: &dyn ItemNames,
    service: &str,
    key: &str,
) -> Result<(String, Option<String>), SecretError> {
    let target = names.credential_target(service, key);
    check("Credential Manager target", &target)?;
    let user = names.credential_user_name(service, key);
    if let Some(user) = &user {
        check("Credential Manager user name", user)?;
    }
    Ok((target, user))
}

/// The Keychain service and account, checked.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn keychain(
    names: &dyn ItemNames,
    service: &str,
    key: &str,
) -> Result<(String, String), SecretError> {
    let item_service = names.keychain_service(service, key);
    check("Keychain service", &item_service)?;
    let account = names.keychain_account(service, key);
    check("Keychain account", &account)?;
    Ok((item_service, account))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Names another client gave its items: one application, its secrets
    /// apart by key alone.
    #[derive(Debug)]
    struct Client;

    impl ItemNames for Client {
        fn secret_service_attributes(&self, service: &str, key: &str) -> BTreeMap<String, String> {
            BTreeMap::from([
                ("application".to_owned(), service.to_owned()),
                ("account".to_owned(), key.to_owned()),
            ])
        }

        fn secret_service_label(&self, service: &str, key: &str) -> String {
            format!("{service} account {key}")
        }

        fn credential_target(&self, service: &str, key: &str) -> String {
            format!("{service}:{key}")
        }

        fn credential_user_name(&self, _: &str, _: &str) -> Option<String> {
            None
        }
    }

    #[test]
    fn the_default_names_are_keyrings() {
        let names = DefaultNames;
        assert_eq!(
            secret_service_attributes(&names, "org.example.mail", "alice"),
            Ok(BTreeMap::from([
                ("service".to_owned(), "org.example.mail".to_owned()),
                ("username".to_owned(), "alice".to_owned()),
            ]))
        );
        assert_eq!(
            secret_service_label(&names, "org.example.mail", "alice"),
            Ok("org.example.mail: alice".to_owned())
        );
        assert_eq!(
            credential(&names, "org.example.mail", "alice"),
            Ok((
                "alice.org.example.mail".to_owned(),
                Some("alice".to_owned())
            ))
        );
        assert_eq!(
            keychain(&names, "org.example.mail", "alice"),
            Ok(("org.example.mail".to_owned(), "alice".to_owned()))
        );
    }

    #[test]
    fn a_caller_names_its_items_and_keeps_the_defaults_it_does_not_name() {
        let names = Client;
        assert_eq!(
            secret_service_attributes(&names, "org.example.mail", "alice"),
            Ok(BTreeMap::from([
                ("application".to_owned(), "org.example.mail".to_owned()),
                ("account".to_owned(), "alice".to_owned()),
            ]))
        );
        assert_eq!(
            secret_service_label(&names, "org.example.mail", "alice"),
            Ok("org.example.mail account alice".to_owned())
        );
        assert_eq!(
            credential(&names, "org.example.mail", "alice"),
            Ok(("org.example.mail:alice".to_owned(), None))
        );
        assert_eq!(
            keychain(&names, "org.example.mail", "alice"),
            Ok(("org.example.mail".to_owned(), "alice".to_owned()))
        );
    }

    #[derive(Debug)]
    struct Broken;

    impl ItemNames for Broken {
        fn secret_service_attributes(&self, _: &str, _: &str) -> BTreeMap<String, String> {
            BTreeMap::new()
        }

        fn secret_service_label(&self, _: &str, _: &str) -> String {
            "a\0b".to_owned()
        }

        fn credential_target(&self, _: &str, _: &str) -> String {
            String::new()
        }

        fn keychain_account(&self, _: &str, _: &str) -> String {
            "a\0b".to_owned()
        }
    }

    #[test]
    fn names_that_cannot_name_an_item_are_refused() {
        let names = Broken;
        assert!(matches!(
            secret_service_attributes(&names, "s", "k"),
            Err(SecretError::InvalidName {
                name: "Secret Service attributes",
                ..
            })
        ));
        assert!(matches!(
            secret_service_label(&names, "s", "k"),
            Err(SecretError::InvalidName {
                name: "Secret Service label",
                ..
            })
        ));
        assert_eq!(
            credential(&names, "s", "k"),
            Err(SecretError::InvalidName {
                name: "Credential Manager target",
                reason: "is empty",
            })
        );
        assert_eq!(
            keychain(&names, "s", "k"),
            Err(SecretError::InvalidName {
                name: "Keychain account",
                reason: "holds a NUL character",
            })
        );
    }

    #[test]
    fn an_attribute_with_no_name_is_refused() {
        #[derive(Debug)]
        struct Unnamed;
        impl ItemNames for Unnamed {
            fn secret_service_attributes(&self, _: &str, key: &str) -> BTreeMap<String, String> {
                BTreeMap::from([(String::new(), key.to_owned())])
            }
        }
        assert_eq!(
            secret_service_attributes(&Unnamed, "s", "k"),
            Err(SecretError::InvalidName {
                name: "Secret Service attribute",
                reason: "is empty",
            })
        );
    }
}
