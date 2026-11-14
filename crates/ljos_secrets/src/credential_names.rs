//! How the Credential Manager names a secret, and which credential is it.
//! Apart from the Windows calls, so that it is tested on every system.

use crate::SecretError;
use crate::names::ItemNames;

/// `text` as a NUL-terminated UTF-16 string.
pub(crate) fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The target and the user name of the secret of `key` in `service`, as
/// `names` gives them: the target NUL-terminated, the user name as text.
pub(crate) fn names(
    names: &dyn ItemNames,
    service: &str,
    key: &str,
) -> Result<(Vec<u16>, Option<String>), SecretError> {
    let (target, user) = crate::names::credential(names, service, key)?;
    Ok((wide(&target), user))
}

/// Whether a credential found at `target` (NUL-terminated), whose names
/// as written are `target_name` and `user_name` (without their NULs), is
/// the secret whose user name is `user`. Windows finds a target whatever
/// its case, and a dot lets two default names share one, so both are
/// compared exactly; with no `user`, the target alone names the secret.
pub(crate) fn is_for(
    target_name: &[u16],
    user_name: &[u16],
    target: &[u16],
    user: Option<&str>,
) -> bool {
    same_target(target_name, target) && user.is_none_or(|user| same_user(user_name, user))
}

/// Whether `target_name` is `target` (NUL-terminated) exactly, case
/// included.
fn same_target(target_name: &[u16], target: &[u16]) -> bool {
    target.strip_suffix(&[0]) == Some(target_name)
}

/// Whether `user_name` is `key` exactly, case included.
fn same_user(user_name: &[u16], key: &str) -> bool {
    user_name.iter().copied().eq(key.encode_utf16())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::names::DefaultNames;

    /// The default target, NUL-terminated.
    fn target(service: &str, key: &str) -> Vec<u16> {
        names(&DefaultNames, service, key).expect("default names").0
    }

    fn names_of(target_name: &str, user_name: &str) -> (Vec<u16>, Vec<u16>) {
        (
            target_name.encode_utf16().collect(),
            user_name.encode_utf16().collect(),
        )
    }

    /// Another client's names: `<service>:<key>`, with no user name.
    #[derive(Debug)]
    struct Client;

    impl ItemNames for Client {
        fn credential_target(&self, service: &str, key: &str) -> String {
            format!("{service}:{key}")
        }

        fn credential_user_name(&self, _: &str, _: &str) -> Option<String> {
            None
        }
    }

    #[test]
    fn a_caller_names_the_target_and_may_leave_out_the_user_name() {
        assert_eq!(
            names(&Client, "org.example.app", "studio"),
            Ok((wide("org.example.app:studio"), None))
        );
    }

    #[test]
    fn with_no_user_name_the_credential_at_the_exact_target_is_the_secret() {
        let (target, user) = names(&Client, "org.example.app", "studio").unwrap();
        // Written by the other client, with no user name or with one of its own.
        for user_name in ["", "someone"] {
            let (target_name, user_name) = names_of("org.example.app:studio", user_name);
            assert!(is_for(&target_name, &user_name, &target, user.as_deref()));
        }
        // Windows finds this one too; its case differs, so it is another's.
        let (target_name, user_name) = names_of("org.example.app:Studio", "");
        assert!(!is_for(&target_name, &user_name, &target, user.as_deref()));
    }

    #[test]
    fn a_secret_is_named_key_dot_service() {
        assert_eq!(
            target("org.example.mail", "alice"),
            wide("alice.org.example.mail")
        );
        assert_eq!(target("org.example.mail", "alice").last(), Some(&0));
    }

    #[test]
    fn the_credential_written_for_a_secret_is_that_secret() {
        let (target_name, user_name) = names_of("alice.org.example.mail", "alice");
        let target = target("org.example.mail", "alice");
        assert!(is_for(&target_name, &user_name, &target, Some("alice")));
    }

    #[test]
    fn a_credential_that_shares_the_target_through_a_dot_is_another_secret() {
        // Key `a` of `b.c`, found at the target of key `a.b` of `c`.
        let (target_name, user_name) = names_of("a.b.c", "a");
        let target = target("c", "a.b");
        assert_eq!(target, wide("a.b.c"));
        // The targets are one string, so only the user name tells them apart.
        assert!(same_target(&target_name, &target));
        assert!(!same_user(&user_name, "a.b"));
        assert!(!is_for(&target_name, &user_name, &target, Some("a.b")));
    }

    #[test]
    fn a_credential_that_shares_the_target_through_a_dot_and_case_is_another_secret() {
        // Key `a` of `B.c`, which Windows finds at the target of key `a.b`
        // of `c` because it ignores case: the target names differ, so the
        // target-name half refuses it whatever the user name.
        let (target_name, user_name) = names_of("a.B.c", "a");
        let target = target("c", "a.b");
        assert_eq!(target, wide("a.b.c"));
        assert!(!same_target(&target_name, &target));
        assert!(!is_for(&target_name, &user_name, &target, Some("a.b")));
    }

    #[test]
    fn a_credential_of_a_service_differing_only_in_case_is_another_secret() {
        let (target_name, user_name) = names_of("a.Mail", "a");
        assert!(!is_for(
            &target_name,
            &user_name,
            &target("mail", "a"),
            Some("a")
        ));
    }

    #[test]
    fn a_credential_of_a_key_differing_only_in_case_is_another_secret() {
        let (target_name, user_name) = names_of("Alice.mail", "Alice");
        assert!(!is_for(
            &target_name,
            &user_name,
            &target("mail", "alice"),
            Some("alice")
        ));
    }
}
