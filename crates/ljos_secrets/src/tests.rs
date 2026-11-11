//! What a store does, on the store in memory; the system's backends have
//! tests of their own.

use crate::{MemorySecrets, MemoryState, NoSecureStorage, Secret, SecretError, SecretStore};

fn store() -> (SecretStore, MemorySecrets) {
    let memory = MemorySecrets::new();
    (SecretStore::new(memory.clone()), memory)
}

fn read(store: &SecretStore, service: &str, key: &str) -> Option<Vec<u8>> {
    store
        .read(service, key)
        .expect("the store reads")
        .map(|secret| secret.as_bytes().to_vec())
}

#[test]
fn a_secret_written_is_read_back() {
    let (store, _) = store();
    store
        .write("org.example.mail", "alice", b"hunter2")
        .expect("the store writes");
    assert_eq!(
        read(&store, "org.example.mail", "alice"),
        Some(b"hunter2".to_vec())
    );
}

#[test]
fn a_secret_never_written_is_none() {
    let (store, _) = store();
    assert_eq!(read(&store, "org.example.mail", "alice"), None);
}

#[test]
fn a_second_write_replaces_the_first() {
    let (store, memory) = store();
    store.write("org.example.mail", "alice", b"old").unwrap();
    store.write("org.example.mail", "alice", b"new").unwrap();
    assert_eq!(
        read(&store, "org.example.mail", "alice"),
        Some(b"new".to_vec())
    );
    assert_eq!(memory.len(), 1);
}

#[test]
fn secrets_are_apart_by_service_and_by_key() {
    let (store, _) = store();
    store.write("org.example.mail", "alice", b"a").unwrap();
    store.write("org.example.mail", "bob", b"b").unwrap();
    store.write("org.example.chat", "alice", b"c").unwrap();
    assert_eq!(
        read(&store, "org.example.mail", "alice"),
        Some(b"a".to_vec())
    );
    assert_eq!(read(&store, "org.example.mail", "bob"), Some(b"b".to_vec()));
    assert_eq!(
        read(&store, "org.example.chat", "alice"),
        Some(b"c".to_vec())
    );
    assert_eq!(read(&store, "org.example.chat", "bob"), None);
}

#[test]
fn a_removed_secret_is_gone_and_removing_it_again_says_so() {
    let (store, _) = store();
    store
        .write("org.example.mail", "alice", b"hunter2")
        .unwrap();
    assert_eq!(store.remove("org.example.mail", "alice"), Ok(true));
    assert_eq!(read(&store, "org.example.mail", "alice"), None);
    assert_eq!(store.remove("org.example.mail", "alice"), Ok(false));
}

#[test]
fn removing_one_secret_leaves_the_others() {
    let (store, _) = store();
    store.write("org.example.mail", "alice", b"a").unwrap();
    store.write("org.example.mail", "bob", b"b").unwrap();
    store.remove("org.example.mail", "alice").unwrap();
    assert_eq!(read(&store, "org.example.mail", "bob"), Some(b"b".to_vec()));
}

#[test]
fn an_empty_secret_is_a_secret() {
    let (store, _) = store();
    store.write("org.example.mail", "alice", b"").unwrap();
    assert_eq!(read(&store, "org.example.mail", "alice"), Some(Vec::new()));
}

#[test]
fn a_write_to_a_locked_store_fails_and_keeps_nothing() {
    let (store, memory) = store();
    memory.set_state(MemoryState::Locked);
    assert_eq!(
        store.write("org.example.mail", "alice", b"hunter2"),
        Err(SecretError::Locked)
    );
    memory.set_state(MemoryState::Open);
    assert_eq!(read(&store, "org.example.mail", "alice"), None);
    assert!(memory.is_empty());
}

#[test]
fn a_write_with_no_store_fails_and_keeps_nothing() {
    let (store, memory) = store();
    memory.set_state(MemoryState::Absent);
    assert!(matches!(
        store.write("org.example.mail", "alice", b"hunter2"),
        Err(SecretError::Unavailable(_))
    ));
    memory.set_state(MemoryState::Open);
    assert!(memory.is_empty());
}

#[test]
fn a_locked_store_reads_and_removes_nothing() {
    let (store, memory) = store();
    store
        .write("org.example.mail", "alice", b"hunter2")
        .unwrap();
    memory.set_state(MemoryState::Locked);
    assert!(matches!(
        store.read("org.example.mail", "alice"),
        Err(SecretError::Locked)
    ));
    assert_eq!(
        store.remove("org.example.mail", "alice"),
        Err(SecretError::Locked)
    );
    memory.set_state(MemoryState::Open);
    assert_eq!(
        read(&store, "org.example.mail", "alice"),
        Some(b"hunter2".to_vec())
    );
}

#[test]
fn an_empty_service_or_key_names_no_secret() {
    let (store, memory) = store();
    let empty = |name| SecretError::InvalidName {
        name,
        reason: "is empty",
    };
    assert_eq!(store.write("", "alice", b"x"), Err(empty("service")));
    assert_eq!(store.write("org.example.mail", "", b"x"), Err(empty("key")));
    assert!(matches!(store.read("", "alice"), Err(e) if e == empty("service")));
    assert_eq!(store.remove("org.example.mail", ""), Err(empty("key")));
    assert!(memory.is_empty());
}

#[test]
fn a_name_with_a_nul_names_no_secret() {
    let (store, memory) = store();
    assert_eq!(
        store.write("org.example.mail", "alice\0bob", b"x"),
        Err(SecretError::InvalidName {
            name: "key",
            reason: "holds a NUL character",
        })
    );
    assert!(memory.is_empty());
}

#[test]
fn a_system_with_no_secure_storage_refuses_every_call() {
    let store = SecretStore::new(NoSecureStorage);
    assert_eq!(
        store.write("org.example.mail", "alice", b"hunter2"),
        Err(SecretError::Unsupported)
    );
    assert!(matches!(
        store.read("org.example.mail", "alice"),
        Err(SecretError::Unsupported)
    ));
    assert_eq!(
        store.remove("org.example.mail", "alice"),
        Err(SecretError::Unsupported)
    );
}

#[test]
fn a_secret_shows_none_of_itself_when_debugged() {
    let secret = Secret::new("hunter2");
    assert_eq!(format!("{secret:?}"), "Secret(..)");
    assert_eq!(secret.as_str(), Some("hunter2"));
    let (store, _) = store();
    store
        .write("org.example.mail", "alice", b"hunter2")
        .unwrap();
    let read = store.read("org.example.mail", "alice");
    assert!(!format!("{read:?}").contains("hunter2"));
}

#[test]
fn a_store_is_shared_between_threads() {
    fn shared<T: Send + Sync>() {}
    shared::<SecretStore>();
    shared::<Secret>();
}
