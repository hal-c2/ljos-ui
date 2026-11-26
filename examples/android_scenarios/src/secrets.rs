//! `ScenarioSecrets` (module `LjosScenarios`): the system's store of
//! secrets as the application reaches it, for the runner to keep, read
//! and remove a secret the way an application does
//! (features/mobile/platform-services.feature).
//!
//! A call into the store can wait (on a phone, for the Keystore through
//! the Java side), so each one runs on a thread of its own and its answer
//! comes back to the Engine thread as a promise kept: `busy` is true until
//! then.

use std::sync::{Arc, OnceLock};

use ljos::prelude::*;
use ljos_secrets::SecretStore;

/// The name the application puts its one `ScenarioSecrets` in the root's
/// scope by, for the runner (`ljos_bdd`'s `@android` backend) to call.
pub(crate) const NAME: &str = "scenarioSecrets";

static STORE: OnceLock<Arc<SecretStore>> = OnceLock::new();

/// Keeps the scenarios' secrets in `store` in place of the system's, for
/// a test without the system's store. Only the first store given, before
/// any secret is kept, is taken; whether this one was.
pub fn keep_secrets_in(store: SecretStore) -> bool {
    STORE.set(Arc::new(store)).is_ok()
}

fn store() -> Arc<SecretStore> {
    Arc::clone(STORE.get_or_init(|| Arc::new(SecretStore::system())))
}

/// What a call into the store answered.
enum Answer {
    Kept,
    Read(Option<String>),
    Removed(bool),
}

#[derive(Debug, Default, QmlObject)]
#[qml(module = "LjosScenarios", name = "ScenarioSecrets", extends = QtObject)]
pub struct ScenarioSecrets {
    #[qml(base)]
    pub object: QtObject,
    /// A call is under way; what it answered is below once it is false.
    #[qml(notify, readonly)]
    pub busy: bool,
    /// The last read found a secret, or the last remove removed one.
    #[qml(notify, readonly)]
    pub found: bool,
    /// The secret the last read found, as text.
    #[qml(notify, readonly)]
    pub secret: String,
    /// Why the last call failed; empty when it did not.
    #[qml(notify, readonly)]
    pub error: String,
}

#[qml_impl]
impl ScenarioSecrets {
    /// Keeps `secret` as `key` of `service`.
    #[qml(invokable)]
    fn keep(cx: &Cx, service: String, key: String, secret: String) -> Result<(), EngineError> {
        call(cx, move |s| {
            s.write(&service, &key, secret.as_bytes())
                .map(|()| Answer::Kept)
        })
    }

    /// Reads the secret kept as `key` of `service`.
    #[qml(invokable)]
    fn read(cx: &Cx, service: String, key: String) -> Result<(), EngineError> {
        call(cx, move |s| {
            let found = s.read(&service, &key)?;
            Ok(Answer::Read(found.map(|f| {
                String::from_utf8_lossy(f.as_bytes()).into_owned()
            })))
        })
    }

    /// Removes the secret kept as `key` of `service`.
    #[qml(invokable)]
    fn remove(cx: &Cx, service: String, key: String) -> Result<(), EngineError> {
        call(cx, move |s| s.remove(&service, &key).map(Answer::Removed))
    }
}

/// Runs `work` against the store on a thread of its own and shows what it
/// answered on `cx.this`.
fn call(
    cx: &Cx,
    work: impl FnOnce(&SecretStore) -> Result<Answer, ljos_secrets::SecretError> + Send + 'static,
) -> Result<(), EngineError> {
    if cx.get(ScenarioSecrets::BUSY)? {
        return Err(EngineError::Invoke(
            "ScenarioSecrets: a call is still under way".into(),
        ));
    }
    cx.set(ScenarioSecrets::BUSY, true)?;
    let me = cx.this;
    let reply = Platform::of(cx.engine).expect_for("a secret from the system's store", Some(me));
    std::thread::spawn(move || {
        let answer = work(&store()).map_err(|e| e.to_string());
        reply.send(move |e| {
            if e.is_alive(me) {
                let _ = show(&Cx::new(e, me), answer);
            }
        });
    });
    Ok(())
}

fn show(cx: &Cx, answer: Result<Answer, String>) -> Result<(), EngineError> {
    let (found, secret, error) = match answer {
        Ok(Answer::Kept) => (true, String::new(), String::new()),
        Ok(Answer::Read(read)) => (read.is_some(), read.unwrap_or_default(), String::new()),
        Ok(Answer::Removed(removed)) => (removed, String::new(), String::new()),
        Err(why) => (false, String::new(), why),
    };
    cx.set(ScenarioSecrets::FOUND, found)?;
    cx.set(ScenarioSecrets::SECRET, secret)?;
    cx.set(ScenarioSecrets::ERROR, error)?;
    // Last: whoever waits on `busy` reads the answer complete.
    cx.set(ScenarioSecrets::BUSY, false)
}

/// Registers `ScenarioSecrets` (module `LjosScenarios`) on `engine`.
pub fn register(engine: &Engine) {
    engine.register_module(ljos::module!("LjosScenarios" 1.0 => [ScenarioSecrets]));
}
