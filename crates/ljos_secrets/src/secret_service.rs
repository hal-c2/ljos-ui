//! The Secret Service (freedesktop.org) on the session bus: GNOME Keyring,
//! KWallet and KeePassXC all serve it.
//!
//! A secret is an item of the default collection, found by the attributes
//! `service` and `username`: the names other programs (`secret-tool`,
//! libsecret's users, Rust's `keyring`) find theirs by, so the same secret
//! can be looked up from a shell.
//!
//! Each call opens a connection and a session and closes both when done:
//! the store keeps no thread and no socket between calls.

use std::collections::HashMap;
use std::time::Duration;

use zbus::blocking::fdo::DBusProxy;
use zbus::blocking::{Connection, MessageIterator, connection};
use zbus::message::Type as MessageType;
use zbus::zvariant::{DynamicDeserialize, DynamicType, OwnedObjectPath, OwnedValue, Value};
use zbus::{DBusError, MatchRule, Message};

use crate::{Secret, SecretBackend, SecretError};

const BUS_NAME: &str = "org.freedesktop.secrets";
const SERVICE_PATH: &str = "/org/freedesktop/secrets";
const SERVICE: &str = "org.freedesktop.Secret.Service";
const COLLECTION: &str = "org.freedesktop.Secret.Collection";
const ITEM: &str = "org.freedesktop.Secret.Item";
const PROMPT: &str = "org.freedesktop.Secret.Prompt";
const SESSION: &str = "org.freedesktop.Secret.Session";

/// The spec's path for "no object": no prompt is needed, or no collection
/// has the alias.
const NONE: &str = "/";

/// A secret as the spec sends it: the session, the algorithm's parameters
/// (none for `plain`), the value, and its content type.
type WireSecret = (OwnedObjectPath, Vec<u8>, Vec<u8>, String);

type Connect = Box<dyn Fn() -> zbus::Result<Connection> + Send + Sync>;

/// How long a call to the service may take before it is an error. zbus
/// waits for good unless told; this is the reference bus's own default.
/// The user answering an unlock dialog is not a call, and is not limited.
const REPLY_WAIT: Duration = Duration::from_secs(25);

/// The Secret Service backend.
pub struct SecretService {
    connect: Connect,
}

impl std::fmt::Debug for SecretService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretService")
    }
}

impl SecretService {
    /// The Secret Service on the user's session bus.
    pub fn session() -> Self {
        Self {
            connect: Box::new(|| {
                connection::Builder::session()?
                    .method_timeout(REPLY_WAIT)
                    .build()
            }),
        }
    }

    /// The Secret Service on the bus at `address` (a D-Bus address, such
    /// as `unix:path=/run/user/1000/bus`): a sandbox's bus, or a test's.
    pub fn at_address(address: impl Into<String>) -> Self {
        let address = address.into();
        Self {
            connect: Box::new(move || {
                connection::Builder::address(address.as_str())?
                    .method_timeout(REPLY_WAIT)
                    .build()
            }),
        }
    }

    fn open(&self) -> Result<Session, SecretError> {
        let bus = (self.connect)().map_err(|e| SecretError::Unavailable(e.to_string()))?;
        // `plain` sends the secret as it is, over the user's own bus
        // socket; the encrypted algorithm would only guard it from
        // programs that can already read that socket.
        let (_, path): (OwnedValue, OwnedObjectPath) = reply(&call(
            &bus,
            SERVICE_PATH,
            SERVICE,
            "OpenSession",
            &("plain", Value::from("")),
        )?)?;
        Ok(Session { bus, path })
    }
}

/// An open connection and session; the session is closed when dropped.
struct Session {
    bus: Connection,
    path: OwnedObjectPath,
}

impl Drop for Session {
    fn drop(&mut self) {
        // The service closes it when the connection goes in any case.
        let _ = call(&self.bus, self.path.as_str(), SESSION, "Close", &());
    }
}

impl Session {
    fn default_collection(&self) -> Result<OwnedObjectPath, SecretError> {
        let collection: OwnedObjectPath = reply(&call(
            &self.bus,
            SERVICE_PATH,
            SERVICE,
            "ReadAlias",
            &("default",),
        )?)?;
        if collection.as_str() == NONE {
            return Err(SecretError::Unavailable(
                "the Secret Service has no default collection".to_owned(),
            ));
        }
        Ok(collection)
    }

    /// The unlocked and the locked items with these attributes.
    fn search(
        &self,
        attributes: &HashMap<&str, &str>,
    ) -> Result<(Vec<OwnedObjectPath>, Vec<OwnedObjectPath>), SecretError> {
        reply(&call(
            &self.bus,
            SERVICE_PATH,
            SERVICE,
            "SearchItems",
            &(attributes,),
        )?)
    }

    /// Unlocks `objects`, asking the user when the service says to; a
    /// prompt the user dismisses leaves them locked.
    fn unlock(&self, objects: &[OwnedObjectPath]) -> Result<(), SecretError> {
        let (_, prompt): (Vec<OwnedObjectPath>, OwnedObjectPath) = reply(&call(
            &self.bus,
            SERVICE_PATH,
            SERVICE,
            "Unlock",
            &(objects,),
        )?)?;
        match self.prompt(&prompt)? {
            Answered::Completed => Ok(()),
            Answered::Dismissed => Err(SecretError::Locked),
        }
    }

    /// Shows the prompt at `path`, if there is one, and waits for the
    /// user, for as long as they take. The service going away while it
    /// waits (a crash, a restart, the user quitting it) ends the wait as
    /// [`SecretError::Unavailable`]: nothing would ever answer.
    fn prompt(&self, path: &OwnedObjectPath) -> Result<Answered, SecretError> {
        if path.as_str() == NONE {
            return Ok(Answered::Completed);
        }
        let completed = MatchRule::builder()
            .msg_type(MessageType::Signal)
            .path(path.as_str())
            .and_then(|b| b.interface(PROMPT))
            .and_then(|b| b.member("Completed"))
            .map_err(failed)?
            .build();
        let gone = MatchRule::builder()
            .msg_type(MessageType::Signal)
            .sender("org.freedesktop.DBus")
            .and_then(|b| b.interface("org.freedesktop.DBus"))
            .and_then(|b| b.member("NameOwnerChanged"))
            .and_then(|b| b.arg(0, BUS_NAME))
            .map_err(failed)?
            .build();
        // One iterator over everything the connection receives, so both
        // can be waited for at once, made before the prompt is shown so
        // neither can come first and be missed.
        let messages = MessageIterator::from(&self.bus);
        let bus = DBusProxy::new(&self.bus).map_err(from_bus)?;
        bus.add_match_rule(completed.clone())
            .map_err(|e| from_bus(e.into()))?;
        bus.add_match_rule(gone.clone())
            .map_err(|e| from_bus(e.into()))?;
        // No window to attach the dialog to: the service shows its own.
        call(&self.bus, path.as_str(), PROMPT, "Prompt", &("",))?;
        for message in messages {
            let message = message.map_err(from_bus)?;
            if completed.matches(&message).unwrap_or(false) {
                let (dismissed, _): (bool, OwnedValue) = reply(&message)?;
                return Ok(if dismissed {
                    Answered::Dismissed
                } else {
                    Answered::Completed
                });
            }
            if gone.matches(&message).unwrap_or(false) {
                return Err(SecretError::Unavailable(
                    "the Secret Service went away before the prompt was answered".to_owned(),
                ));
            }
        }
        Err(SecretError::Unavailable(
            "the bus closed before the prompt was answered".to_owned(),
        ))
    }

    /// Every item with these attributes, unlocked (asking the user if need
    /// be); none when there are none.
    fn unlocked_items(
        &self,
        attributes: &HashMap<&str, &str>,
    ) -> Result<Vec<OwnedObjectPath>, SecretError> {
        let (mut unlocked, locked) = self.search(attributes)?;
        if !locked.is_empty() {
            self.unlock(&locked)?;
            unlocked.extend(locked);
        }
        Ok(unlocked)
    }

    /// One item with these attributes: an unlocked one when there is one,
    /// so the user is asked only when every copy is locked.
    fn unlocked_item(
        &self,
        attributes: &HashMap<&str, &str>,
    ) -> Result<Option<OwnedObjectPath>, SecretError> {
        let (unlocked, locked) = self.search(attributes)?;
        if let Some(item) = unlocked.into_iter().next() {
            return Ok(Some(item));
        }
        let Some(item) = locked.into_iter().next() else {
            return Ok(None);
        };
        self.unlock(std::slice::from_ref(&item))?;
        Ok(Some(item))
    }
}

enum Answered {
    Completed,
    Dismissed,
}

fn attributes<'a>(service: &'a str, key: &'a str) -> HashMap<&'static str, &'a str> {
    HashMap::from([("service", service), ("username", key)])
}

impl SecretBackend for SecretService {
    fn write(&self, service: &str, key: &str, secret: &[u8]) -> Result<(), SecretError> {
        let session = self.open()?;
        let collection = session.default_collection()?;
        session.unlock(std::slice::from_ref(&collection))?;
        let properties = HashMap::from([
            (
                "org.freedesktop.Secret.Item.Label",
                Value::from(format!("{service}: {key}")),
            ),
            (
                "org.freedesktop.Secret.Item.Attributes",
                Value::from(attributes(service, key)),
            ),
        ]);
        let content_type = if std::str::from_utf8(secret).is_ok() {
            "text/plain"
        } else {
            "application/octet-stream"
        };
        let wire = (&session.path, Vec::<u8>::new(), secret, content_type);
        let (_, prompt): (OwnedObjectPath, OwnedObjectPath) = reply(&call(
            &session.bus,
            collection.as_str(),
            COLLECTION,
            "CreateItem",
            &(properties, wire, true),
        )?)?;
        match session.prompt(&prompt)? {
            Answered::Completed => Ok(()),
            Answered::Dismissed => Err(SecretError::Denied),
        }
    }

    fn read(&self, service: &str, key: &str) -> Result<Option<Secret>, SecretError> {
        let session = self.open()?;
        let Some(item) = session.unlocked_item(&attributes(service, key))? else {
            return Ok(None);
        };
        let (_, _, value, _): WireSecret = reply(&call(
            &session.bus,
            item.as_str(),
            ITEM,
            "GetSecret",
            &(&session.path,),
        )?)?;
        Ok(Some(Secret::new(value)))
    }

    fn remove(&self, service: &str, key: &str) -> Result<bool, SecretError> {
        let session = self.open()?;
        let items = session.unlocked_items(&attributes(service, key))?;
        for item in &items {
            let prompt: OwnedObjectPath =
                reply(&call(&session.bus, item.as_str(), ITEM, "Delete", &())?)?;
            if let Answered::Dismissed = session.prompt(&prompt)? {
                return Err(SecretError::Denied);
            }
        }
        Ok(!items.is_empty())
    }
}

fn call<B>(
    bus: &Connection,
    path: &str,
    interface: &str,
    method: &str,
    body: &B,
) -> Result<Message, SecretError>
where
    B: zbus::export::serde::Serialize + DynamicType,
{
    bus.call_method(Some(BUS_NAME), path, Some(interface), method, body)
        .map_err(from_bus)
}

fn reply<R>(message: &Message) -> Result<R, SecretError>
where
    R: for<'d> DynamicDeserialize<'d>,
{
    message.body().deserialize().map_err(failed)
}

fn failed(error: zbus::Error) -> SecretError {
    SecretError::Failed(error.to_string())
}

fn from_bus(error: zbus::Error) -> SecretError {
    let name = match &error {
        zbus::Error::MethodError(name, _, _) => name.as_str().to_owned(),
        zbus::Error::FDO(fdo) => fdo.name().as_str().to_owned(),
        zbus::Error::InputOutput(_)
        | zbus::Error::Address(_)
        | zbus::Error::Handshake(_)
        | zbus::Error::Connection(..) => {
            return SecretError::Unavailable(error.to_string());
        }
        _ => return failed(error),
    };
    match name.as_str() {
        // Nobody owns the service's name, and nothing on the bus starts it.
        "org.freedesktop.DBus.Error.ServiceUnknown"
        | "org.freedesktop.DBus.Error.NameHasNoOwner" => {
            SecretError::Unavailable(error.to_string())
        }
        "org.freedesktop.Secret.Error.IsLocked" => SecretError::Locked,
        "org.freedesktop.DBus.Error.AccessDenied" => SecretError::Denied,
        _ => failed(error),
    }
}
