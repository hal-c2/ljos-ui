//! How the Credential Manager names a secret, and which credential is it.
//! Apart from the Windows calls, so that it is tested on every system.

/// `text` as a NUL-terminated UTF-16 string.
pub(crate) fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The target of the secret of `key` in `service`, NUL-terminated:
/// `<key>.<service>`, as Rust's `keyring` names it.
pub(crate) fn target(service: &str, key: &str) -> Vec<u16> {
    wide(&format!("{key}.{service}"))
}

/// Whether a credential found at `target` (NUL-terminated), whose names
/// as written are `target_name` and `user_name` (without their NULs), is
/// the secret of `key` there. Windows finds a target whatever its case,
/// and a dot lets two names share one, so both are compared exactly.
pub(crate) fn is_for(target_name: &[u16], user_name: &[u16], target: &[u16], key: &str) -> bool {
    same_target(target_name, target) && same_user(user_name, key)
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

    fn names(target_name: &str, user_name: &str) -> (Vec<u16>, Vec<u16>) {
        (
            target_name.encode_utf16().collect(),
            user_name.encode_utf16().collect(),
        )
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
        let (target_name, user_name) = names("alice.org.example.mail", "alice");
        let target = target("org.example.mail", "alice");
        assert!(is_for(&target_name, &user_name, &target, "alice"));
    }

    #[test]
    fn a_credential_that_shares_the_target_through_a_dot_is_another_secret() {
        // Key `a` of `b.c`, found at the target of key `a.b` of `c`.
        let (target_name, user_name) = names("a.b.c", "a");
        let target = target("c", "a.b");
        assert_eq!(target, wide("a.b.c"));
        // The targets are one string, so only the user name tells them apart.
        assert!(same_target(&target_name, &target));
        assert!(!same_user(&user_name, "a.b"));
        assert!(!is_for(&target_name, &user_name, &target, "a.b"));
    }

    #[test]
    fn a_credential_that_shares_the_target_through_a_dot_and_case_is_another_secret() {
        // Key `a` of `B.c`, which Windows finds at the target of key `a.b`
        // of `c` because it ignores case: the target names differ, so the
        // target-name half refuses it whatever the user name.
        let (target_name, user_name) = names("a.B.c", "a");
        let target = target("c", "a.b");
        assert_eq!(target, wide("a.b.c"));
        assert!(!same_target(&target_name, &target));
        assert!(!is_for(&target_name, &user_name, &target, "a.b"));
    }

    #[test]
    fn a_credential_of_a_service_differing_only_in_case_is_another_secret() {
        let (target_name, user_name) = names("a.Mail", "a");
        assert!(!is_for(&target_name, &user_name, &target("mail", "a"), "a"));
    }

    #[test]
    fn a_credential_of_a_key_differing_only_in_case_is_another_secret() {
        let (target_name, user_name) = names("Alice.mail", "Alice");
        assert!(!is_for(
            &target_name,
            &user_name,
            &target("mail", "alice"),
            "alice"
        ));
    }
}
