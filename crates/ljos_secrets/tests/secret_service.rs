//! The Secret Service backend against a Secret Service of the test's own,
//! on a bus of the test's own: nothing reaches the desktop's keyring.

#![cfg(target_os = "linux")]

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard, mpsc};
use std::time::Duration;

use ljos_secrets::{SecretError, SecretService, SecretStore};
use zbus::blocking::connection;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

const NAME: &str = "org.freedesktop.secrets";
const SERVICE_PATH: &str = "/org/freedesktop/secrets";
const COLLECTION_PATH: &str = "/org/freedesktop/secrets/collection/login";
const SESSION_PATH: &str = "/org/freedesktop/secrets/session/s1";

/// How the user answers when asked to unlock the keyring.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Answer {
    #[default]
    Unlock,
    Dismiss,
    /// The service quits (or crashes) while the dialog is open, and
    /// nothing answers.
    Vanish,
}

#[derive(Debug, Default)]
struct Keyring {
    locked: bool,
    no_default_collection: bool,
    answer: Answer,
    prompts: usize,
    algorithms: Vec<String>,
    sessions_closed: usize,
    /// By item number; a deleted item is `None`.
    items: Vec<Option<Stored>>,
    /// Items that stay locked while the keyring is open: copies in
    /// another collection, which is locked.
    locked_items: Vec<usize>,
}

impl Keyring {
    fn kept(&self) -> Vec<&Stored> {
        self.items.iter().flatten().collect()
    }
}

#[derive(Debug, Clone)]
struct Stored {
    label: String,
    attributes: HashMap<String, String>,
    secret: Vec<u8>,
    content_type: String,
    session: String,
}

type Shared = Arc<Mutex<Keyring>>;

fn lock(keyring: &Shared) -> MutexGuard<'_, Keyring> {
    keyring.lock().unwrap_or_else(|e| e.into_inner())
}

fn path(p: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(p).expect("an object path")
}

fn item_path(id: usize) -> String {
    format!("{COLLECTION_PATH}/i{id}")
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.Secret.Error")]
enum Refusal {
    #[zbus(error)]
    ZBus(zbus::Error),
    IsLocked(String),
    NoSuchObject(String),
}

struct Service(Shared);

#[zbus::interface(name = "org.freedesktop.Secret.Service")]
impl Service {
    fn open_session(&self, algorithm: String, _input: OwnedValue) -> (OwnedValue, OwnedObjectPath) {
        lock(&self.0).algorithms.push(algorithm);
        let output = OwnedValue::try_from(Value::from("")).expect("a string value");
        (output, path(SESSION_PATH))
    }

    fn read_alias(&self, name: String) -> OwnedObjectPath {
        if name == "default" && !lock(&self.0).no_default_collection {
            path(COLLECTION_PATH)
        } else {
            path("/")
        }
    }

    async fn unlock(
        &self,
        objects: Vec<OwnedObjectPath>,
        #[zbus(object_server)] server: &zbus::ObjectServer,
    ) -> Result<(Vec<OwnedObjectPath>, OwnedObjectPath), Refusal> {
        let prompt = {
            let mut keyring = lock(&self.0);
            let locked_item = objects.iter().any(|object| {
                keyring
                    .locked_items
                    .iter()
                    .any(|&id| object.as_str() == item_path(id))
            });
            if !keyring.locked && !locked_item {
                return Ok((objects, path("/")));
            }
            keyring.prompts += 1;
            format!("/org/freedesktop/secrets/prompt/p{}", keyring.prompts)
        };
        server
            .at(prompt.as_str(), Prompt(Arc::clone(&self.0)))
            .await?;
        Ok((Vec::new(), path(&prompt)))
    }

    fn search_items(
        &self,
        attributes: HashMap<String, String>,
    ) -> (Vec<OwnedObjectPath>, Vec<OwnedObjectPath>) {
        let keyring = lock(&self.0);
        let found = keyring
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                item.as_ref().is_some_and(|item| {
                    attributes
                        .iter()
                        .all(|(name, value)| item.attributes.get(name) == Some(value))
                })
            })
            .map(|(id, _)| id);
        if keyring.locked {
            (Vec::new(), found.map(|id| path(&item_path(id))).collect())
        } else {
            let (locked, unlocked): (Vec<usize>, Vec<usize>) =
                found.partition(|id| keyring.locked_items.contains(id));
            let paths = |ids: Vec<usize>| ids.into_iter().map(|id| path(&item_path(id))).collect();
            (paths(unlocked), paths(locked))
        }
    }
}

struct Collection(Shared);

#[zbus::interface(name = "org.freedesktop.Secret.Collection")]
impl Collection {
    async fn create_item(
        &self,
        properties: HashMap<String, OwnedValue>,
        secret: (OwnedObjectPath, Vec<u8>, Vec<u8>, String),
        replace: bool,
        #[zbus(object_server)] server: &zbus::ObjectServer,
    ) -> Result<(OwnedObjectPath, OwnedObjectPath), Refusal> {
        let property = |name: &str| {
            properties
                .get(name)
                .and_then(|value| value.try_clone().ok())
        };
        let label = property("org.freedesktop.Secret.Item.Label")
            .and_then(|value| String::try_from(value).ok())
            .unwrap_or_default();
        let attributes = property("org.freedesktop.Secret.Item.Attributes")
            .and_then(|value| HashMap::<String, String>::try_from(value).ok())
            .unwrap_or_default();
        let (session, _parameters, value, content_type) = secret;
        let (id, new) = {
            let mut keyring = lock(&self.0);
            if keyring.locked {
                return Err(Refusal::IsLocked("the collection is locked".to_owned()));
            }
            let stored = Stored {
                label,
                attributes,
                secret: value,
                content_type,
                session: session.to_string(),
            };
            let same = replace
                .then(|| {
                    keyring.items.iter().position(|item| {
                        item.as_ref()
                            .is_some_and(|item| item.attributes == stored.attributes)
                    })
                })
                .flatten();
            match same {
                Some(id) => {
                    keyring.items[id] = Some(stored);
                    (id, false)
                }
                None => {
                    keyring.items.push(Some(stored));
                    (keyring.items.len() - 1, true)
                }
            }
        };
        if new {
            let item = Item {
                id,
                keyring: Arc::clone(&self.0),
            };
            server.at(item_path(id).as_str(), item).await?;
        }
        Ok((path(&item_path(id)), path("/")))
    }
}

struct Item {
    id: usize,
    keyring: Shared,
}

#[zbus::interface(name = "org.freedesktop.Secret.Item")]
impl Item {
    // One out argument, a struct: the spec's `(oayays)`.
    #[allow(clippy::type_complexity)]
    fn get_secret(
        &self,
        session: OwnedObjectPath,
    ) -> Result<((OwnedObjectPath, Vec<u8>, Vec<u8>, String),), Refusal> {
        let keyring = lock(&self.keyring);
        if keyring.locked {
            return Err(Refusal::IsLocked("the item is locked".to_owned()));
        }
        let item = keyring.items[self.id]
            .as_ref()
            .ok_or_else(|| Refusal::NoSuchObject("deleted".to_owned()))?;
        Ok(((
            session,
            Vec::new(),
            item.secret.clone(),
            item.content_type.clone(),
        ),))
    }

    fn delete(&self) -> Result<OwnedObjectPath, Refusal> {
        let mut keyring = lock(&self.keyring);
        if keyring.locked {
            return Err(Refusal::IsLocked("the item is locked".to_owned()));
        }
        keyring.items[self.id]
            .take()
            .ok_or_else(|| Refusal::NoSuchObject("deleted".to_owned()))?;
        Ok(path("/"))
    }
}

struct Prompt(Shared);

#[zbus::interface(name = "org.freedesktop.Secret.Prompt")]
impl Prompt {
    async fn prompt(
        &self,
        _window_id: String,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> zbus::fdo::Result<()> {
        let answer = lock(&self.0).answer;
        let dismissed = match answer {
            Answer::Unlock => {
                let mut keyring = lock(&self.0);
                keyring.locked = false;
                keyring.locked_items.clear();
                false
            }
            Answer::Dismiss => true,
            Answer::Vanish => {
                connection.release_name(NAME).await?;
                return Ok(());
            }
        };
        let result = if dismissed {
            Value::from("")
        } else {
            Value::from(vec![ObjectPath::try_from(COLLECTION_PATH).expect("a path")])
        };
        Self::completed(&emitter, dismissed, result).await?;
        Ok(())
    }

    fn dismiss(&self) {}

    #[zbus(signal)]
    async fn completed(
        emitter: &SignalEmitter<'_>,
        dismissed: bool,
        result: Value<'_>,
    ) -> zbus::Result<()>;
}

struct Session(Shared);

#[zbus::interface(name = "org.freedesktop.Secret.Session")]
impl Session {
    fn close(&self) {
        lock(&self.0).sessions_closed += 1;
    }
}

/// A bus of the test's own. Its configuration names no directories of
/// services, so nothing on it is started on demand: a keyring daemon
/// started there would open the user's keyrings.
struct Bus {
    daemon: Child,
    address: String,
    _dir: tempfile::TempDir,
}

impl Bus {
    fn start() -> Option<Self> {
        let dir = tempfile::tempdir().ok()?;
        let config = dir.path().join("bus.conf");
        std::fs::write(
            &config,
            format!(
                r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:path={}</listen>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#,
                dir.path().join("bus").display()
            ),
        )
        .ok()?;
        let mut daemon = Command::new("dbus-daemon")
            .arg(format!("--config-file={}", config.display()))
            .args(["--nofork", "--print-address"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let mut address = String::new();
        BufReader::new(daemon.stdout.take()?)
            .read_line(&mut address)
            .ok()?;
        Some(Self {
            daemon,
            address: address.trim().to_owned(),
            _dir: dir,
        })
    }

    /// The Secret Service, keeping `keyring`, on this bus.
    fn serve(&self, keyring: &Shared) -> zbus::blocking::Connection {
        connection::Builder::address(self.address.as_str())
            .and_then(|b| b.name(NAME))
            .and_then(|b| b.serve_at(SERVICE_PATH, Service(Arc::clone(keyring))))
            .and_then(|b| b.serve_at(COLLECTION_PATH, Collection(Arc::clone(keyring))))
            .and_then(|b| b.serve_at(SESSION_PATH, Session(Arc::clone(keyring))))
            .and_then(connection::Builder::build)
            .expect("the test's Secret Service")
    }

    fn store(&self) -> SecretStore {
        SecretStore::new(SecretService::at_address(self.address.clone()))
    }
}

impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

/// Runs `test` against a Secret Service keeping `keyring`, and fails it
/// when it has not ended in time: a call with no answer fails the test
/// instead of hanging the run.
fn with_service(keyring: Keyring, test: impl FnOnce(SecretStore, Shared) + Send + 'static) {
    let Some(bus) = Bus::start() else {
        eprintln!("skipped: no dbus-daemon to start");
        return;
    };
    let keyring = Arc::new(Mutex::new(keyring));
    let _service = bus.serve(&keyring);
    let store = bus.store();
    within(move || test(store, keyring));
    drop(bus);
}

fn within(test: impl FnOnce() + Send + 'static) {
    const LIMIT: Duration = Duration::from_secs(60);
    let (done, ended) = mpsc::channel::<()>();
    let thread = std::thread::spawn(move || {
        let _done = done;
        test();
    });
    if let Err(mpsc::RecvTimeoutError::Timeout) = ended.recv_timeout(LIMIT) {
        panic!("the test hung for {LIMIT:?}");
    }
    if let Err(panic) = thread.join() {
        std::panic::resume_unwind(panic);
    }
}

fn read(store: &SecretStore, service: &str, key: &str) -> Option<Vec<u8>> {
    store
        .read(service, key)
        .expect("the store reads")
        .map(|secret| secret.as_bytes().to_vec())
}

#[test]
fn a_secret_written_is_kept_in_the_default_collection_and_read_back() {
    with_service(Keyring::default(), |store, keyring| {
        store
            .write("org.example.mail", "alice", b"hunter2")
            .expect("the store writes");
        {
            let keyring = lock(&keyring);
            let kept = keyring.kept();
            assert_eq!(kept.len(), 1);
            assert_eq!(kept[0].secret, b"hunter2");
            assert_eq!(
                kept[0].attributes,
                HashMap::from([
                    ("service".to_owned(), "org.example.mail".to_owned()),
                    ("username".to_owned(), "alice".to_owned()),
                ])
            );
            assert_eq!(kept[0].label, "org.example.mail: alice");
            assert_eq!(kept[0].content_type, "text/plain");
            assert_eq!(kept[0].session, SESSION_PATH);
        }
        assert_eq!(
            read(&store, "org.example.mail", "alice"),
            Some(b"hunter2".to_vec())
        );
        let keyring = lock(&keyring);
        assert!(keyring.algorithms.iter().all(|a| a == "plain"));
        assert_eq!(keyring.sessions_closed, keyring.algorithms.len());
    });
}

#[test]
fn a_secret_never_written_is_none() {
    with_service(Keyring::default(), |store, _| {
        assert_eq!(read(&store, "org.example.mail", "alice"), None);
    });
}

#[test]
fn a_second_write_replaces_the_first() {
    with_service(Keyring::default(), |store, keyring| {
        store.write("org.example.mail", "alice", b"old").unwrap();
        store.write("org.example.mail", "alice", b"new").unwrap();
        assert_eq!(lock(&keyring).kept().len(), 1);
        assert_eq!(
            read(&store, "org.example.mail", "alice"),
            Some(b"new".to_vec())
        );
    });
}

#[test]
fn secrets_are_apart_by_service_and_by_key() {
    with_service(Keyring::default(), |store, _| {
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
    });
}

#[test]
fn a_removed_secret_is_gone_and_removing_it_again_says_so() {
    with_service(Keyring::default(), |store, keyring| {
        store
            .write("org.example.mail", "alice", b"hunter2")
            .unwrap();
        store.write("org.example.mail", "bob", b"b").unwrap();
        assert_eq!(store.remove("org.example.mail", "alice"), Ok(true));
        assert_eq!(read(&store, "org.example.mail", "alice"), None);
        assert_eq!(store.remove("org.example.mail", "alice"), Ok(false));
        assert_eq!(read(&store, "org.example.mail", "bob"), Some(b"b".to_vec()));
        assert_eq!(lock(&keyring).kept().len(), 1);
    });
}

#[test]
fn a_secret_that_is_not_text_is_kept_as_bytes() {
    with_service(Keyring::default(), |store, keyring| {
        let bytes = [0u8, 159, 146, 150, 255];
        store.write("org.example.mail", "alice", &bytes).unwrap();
        assert_eq!(
            lock(&keyring).kept()[0].content_type,
            "application/octet-stream"
        );
        assert_eq!(
            read(&store, "org.example.mail", "alice"),
            Some(bytes.to_vec())
        );
    });
}

#[test]
fn with_no_secret_service_on_the_bus_every_call_says_it_is_unavailable() {
    let Some(bus) = Bus::start() else {
        eprintln!("skipped: no dbus-daemon to start");
        return;
    };
    let store = bus.store();
    within(move || {
        let unavailable = |e: &SecretError| matches!(e, SecretError::Unavailable(_));
        let written = store.write("org.example.mail", "alice", b"hunter2");
        assert!(written.as_ref().is_err_and(unavailable), "{written:?}");
        let read = store.read("org.example.mail", "alice");
        assert!(read.as_ref().is_err_and(unavailable), "{read:?}");
        let removed = store.remove("org.example.mail", "alice");
        assert!(removed.as_ref().is_err_and(unavailable), "{removed:?}");
    });
}

#[test]
fn with_no_bus_at_all_a_write_says_it_is_unavailable() {
    let dir = tempfile::tempdir().expect("a directory");
    let address = format!("unix:path={}", dir.path().join("no-bus").display());
    let store = SecretStore::new(SecretService::at_address(address));
    let written = store.write("org.example.mail", "alice", b"hunter2");
    assert!(
        matches!(written, Err(SecretError::Unavailable(_))),
        "{written:?}"
    );
}

#[test]
fn with_no_default_collection_a_write_fails_and_keeps_nothing() {
    let keyring = Keyring {
        no_default_collection: true,
        ..Keyring::default()
    };
    with_service(keyring, |store, keyring| {
        let written = store.write("org.example.mail", "alice", b"hunter2");
        assert!(
            matches!(written, Err(SecretError::Unavailable(_))),
            "{written:?}"
        );
        assert!(lock(&keyring).kept().is_empty());
    });
}

#[test]
fn a_locked_keyring_is_unlocked_when_the_user_agrees_and_the_write_is_kept() {
    let keyring = Keyring {
        locked: true,
        answer: Answer::Unlock,
        ..Keyring::default()
    };
    with_service(keyring, |store, keyring| {
        store
            .write("org.example.mail", "alice", b"hunter2")
            .expect("the store writes once unlocked");
        let keyring = lock(&keyring);
        assert_eq!(keyring.prompts, 1);
        assert!(!keyring.locked);
        assert_eq!(keyring.kept().len(), 1);
    });
}

#[test]
fn a_write_to_a_keyring_the_user_leaves_locked_fails_and_keeps_nothing() {
    let keyring = Keyring {
        locked: true,
        answer: Answer::Dismiss,
        ..Keyring::default()
    };
    with_service(keyring, |store, keyring| {
        assert_eq!(
            store.write("org.example.mail", "alice", b"hunter2"),
            Err(SecretError::Locked)
        );
        let keyring = lock(&keyring);
        assert_eq!(keyring.prompts, 1);
        assert!(keyring.kept().is_empty());
    });
}

#[test]
fn a_secret_in_a_locked_keyring_is_read_once_the_user_unlocks_it() {
    with_service(Keyring::default(), |store, keyring| {
        store
            .write("org.example.mail", "alice", b"hunter2")
            .unwrap();
        lock(&keyring).locked = true;
        assert_eq!(
            read(&store, "org.example.mail", "alice"),
            Some(b"hunter2".to_vec())
        );
        assert_eq!(lock(&keyring).prompts, 1);
    });
}

#[test]
fn a_keyring_the_user_leaves_locked_reads_and_removes_nothing() {
    with_service(Keyring::default(), |store, keyring| {
        store
            .write("org.example.mail", "alice", b"hunter2")
            .unwrap();
        {
            let mut keyring = lock(&keyring);
            keyring.locked = true;
            keyring.answer = Answer::Dismiss;
        }
        assert!(matches!(
            store.read("org.example.mail", "alice"),
            Err(SecretError::Locked)
        ));
        assert_eq!(
            store.remove("org.example.mail", "alice"),
            Err(SecretError::Locked)
        );
        assert_eq!(lock(&keyring).kept().len(), 1);
    });
}

#[test]
fn a_read_takes_an_unlocked_copy_without_asking_to_unlock_a_locked_one() {
    with_service(Keyring::default(), |store, keyring| {
        store
            .write("org.example.mail", "alice", b"hunter2")
            .unwrap();
        {
            let mut keyring = lock(&keyring);
            let copy = Stored {
                secret: b"elsewhere".to_vec(),
                ..keyring.kept()[0].clone()
            };
            keyring.items.push(Some(copy));
            let id = keyring.items.len() - 1;
            keyring.locked_items.push(id);
            keyring.answer = Answer::Dismiss;
        }
        assert_eq!(
            read(&store, "org.example.mail", "alice"),
            Some(b"hunter2".to_vec())
        );
        assert_eq!(lock(&keyring).prompts, 0);
    });
}

#[test]
fn a_secret_service_that_goes_while_the_user_is_asked_is_unavailable() {
    let keyring = Keyring {
        locked: true,
        answer: Answer::Vanish,
        ..Keyring::default()
    };
    with_service(keyring, |store, keyring| {
        let written = store.write("org.example.mail", "alice", b"hunter2");
        assert!(
            matches!(written, Err(SecretError::Unavailable(_))),
            "{written:?}"
        );
        let keyring = lock(&keyring);
        assert_eq!(keyring.prompts, 1);
        assert!(keyring.kept().is_empty());
    });
}

/// The desktop's own keyring. Opt in with `--ignored`: it asks the user's
/// Secret Service, and may show its unlock dialog.
#[test]
#[ignore = "uses the desktop's Secret Service; run with --ignored"]
fn the_desktops_secret_service_keeps_a_secret() {
    let store = SecretStore::new(SecretService::session());
    let service = format!("org.example.ljos-secrets-test.{}", std::process::id());
    store.write(&service, "probe", b"probe").expect("written");
    assert_eq!(read(&store, &service, "probe"), Some(b"probe".to_vec()));
    assert_eq!(store.remove(&service, "probe"), Ok(true));
}
