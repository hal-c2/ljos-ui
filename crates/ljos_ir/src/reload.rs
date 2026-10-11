//! What a dev reload carries from the compiler to the engine
//! (ARCHITECTURE.md §12.1): the files that changed, compiled, and what
//! changed in them, object by object.
//!
//! A patch is not part of a compiled file and does not change its format:
//! it wraps whole [`CompiledFile`]s. The engine takes the new bindings and
//! handler bodies from those files and never looks inside one.

use serde::{Deserialize, Serialize};

use crate::{CompiledFile, Error, KeySeg, ObjectKey};

/// One change to a running application, applied whole or not at all.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReloadPatch {
    /// The generation the application is at once the patch is applied.
    pub generation: u64,
    /// The generation the patch was computed against. An engine at another
    /// generation refuses it: its ops would describe the wrong objects.
    pub base: u64,
    /// Every file whose compiled form differs, in full.
    pub files: Vec<FilePatch>,
    /// The keys of files that are gone.
    pub removed: Vec<String>,
    /// What changed, for the engine to know which declarations were edited
    /// and for a tool to show.
    pub ops: Vec<ReloadOp>,
}

/// A file of a patch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilePatch {
    pub file: CompiledFile,
    /// Whether the application had no file of this key before.
    pub added: bool,
}

/// One difference between the application's files and the patch's.
///
/// `file` is a file key; `component` a [`crate::ComponentDef::id`]. An
/// object is named by its [`ObjectKey`] in the new file, or in the old one
/// when the new file does not have it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReloadOp {
    FileAdded {
        file: String,
    },
    FileRemoved {
        file: String,
    },
    /// `source` is the module URI or the path, as written.
    ImportAdded {
        file: String,
        source: String,
    },
    ImportRemoved {
        file: String,
        source: String,
    },
    /// A pragma was added, removed or given another value.
    PragmaChanged {
        file: String,
        name: String,
    },
    ComponentAdded {
        file: String,
        component: String,
    },
    ComponentRemoved {
        file: String,
        component: String,
    },
    /// The component's properties, signals or inline components changed:
    /// an instance keeps a property only when its name and type did not.
    SignatureChanged {
        file: String,
        component: String,
    },
    ObjectAdded {
        file: String,
        object: ObjectKey,
    },
    ObjectRemoved {
        file: String,
        object: ObjectKey,
    },
    /// The same object at another place: under another parent, or in
    /// another order among its siblings.
    ObjectMoved {
        file: String,
        from: ObjectKey,
        to: ObjectKey,
    },
    /// Another type under the same identity: the object is made anew.
    TypeChanged {
        file: String,
        object: ObjectKey,
    },
    /// The property's declaration, value, binding or object value was
    /// added, removed or changed. `property` is dotted for a group
    /// (`anchors.left`) and starts with the attaching type as written for
    /// an attached property (`Layout.fillWidth`).
    PropertyChanged {
        file: String,
        object: ObjectKey,
        property: String,
    },
    /// A handler of the signal was added, removed or changed. `signal` is
    /// dotted like a property (`Keys.pressed`, `Component.completed`).
    HandlerChanged {
        file: String,
        object: ObjectKey,
        signal: String,
    },
    /// A method, a signal or an enum of the object was added, removed or
    /// changed.
    DeclarationChanged {
        file: String,
        object: ObjectKey,
        name: String,
    },
}

impl ReloadOp {
    /// The key of the file the op is about.
    #[must_use]
    pub fn file(&self) -> &str {
        match self {
            ReloadOp::FileAdded { file }
            | ReloadOp::FileRemoved { file }
            | ReloadOp::ImportAdded { file, .. }
            | ReloadOp::ImportRemoved { file, .. }
            | ReloadOp::PragmaChanged { file, .. }
            | ReloadOp::ComponentAdded { file, .. }
            | ReloadOp::ComponentRemoved { file, .. }
            | ReloadOp::SignatureChanged { file, .. }
            | ReloadOp::ObjectAdded { file, .. }
            | ReloadOp::ObjectRemoved { file, .. }
            | ReloadOp::ObjectMoved { file, .. }
            | ReloadOp::TypeChanged { file, .. }
            | ReloadOp::PropertyChanged { file, .. }
            | ReloadOp::HandlerChanged { file, .. }
            | ReloadOp::DeclarationChanged { file, .. } => file,
        }
    }
}

impl ReloadPatch {
    /// Whether applying the patch would change nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.removed.is_empty()
    }

    /// The wire form, for an application on another device.
    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        Ok(postcard::to_allocvec(self)?)
    }

    /// Reads what [`ReloadPatch::to_bytes`] wrote. A file of another IR
    /// version is refused rather than misread.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let patch: Self = postcard::from_bytes(bytes)?;
        for f in &patch.files {
            if f.file.version != crate::IR_VERSION {
                return Err(Error::Version {
                    found: f.file.version,
                    expected: crate::IR_VERSION,
                });
            }
        }
        Ok(patch)
    }
}

impl ObjectKey {
    /// The key of the object this one is written in. None for a root.
    #[must_use]
    pub fn parent(&self) -> Option<ObjectKey> {
        let (_, rest) = self.segments.split_last()?;
        Some(ObjectKey {
            component: self.component.clone(),
            segments: rest.to_vec(),
        })
    }

    /// The object's own step. None for a root.
    #[must_use]
    pub fn last(&self) -> Option<&KeySeg> {
        self.segments.last()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(segments: Vec<KeySeg>) -> ObjectKey {
        ObjectKey {
            component: "Main".into(),
            segments,
        }
    }

    #[test]
    fn a_patch_survives_the_wire() {
        let patch = ReloadPatch {
            generation: 4,
            base: 3,
            files: Vec::new(),
            removed: vec!["Gone".into()],
            ops: vec![
                ReloadOp::FileRemoved {
                    file: "Gone".into(),
                },
                ReloadOp::PropertyChanged {
                    file: "Main".into(),
                    object: key(vec![KeySeg::Id("box".into())]),
                    property: "anchors.left".into(),
                },
            ],
        };
        let back = ReloadPatch::from_bytes(&patch.to_bytes().unwrap()).unwrap();
        assert_eq!(back, patch);
        assert!(!back.is_empty());
        assert_eq!(back.ops[1].file(), "Main");
    }

    #[test]
    fn a_key_knows_its_parent() {
        let k = key(vec![
            KeySeg::Named("list".into()),
            KeySeg::Typed {
                ty: "Rectangle".into(),
                nth: 1,
            },
        ]);
        assert_eq!(k.parent(), Some(key(vec![KeySeg::Named("list".into())])));
        assert_eq!(key(Vec::new()).parent(), None);
        assert!(key(Vec::new()).last().is_none());
    }
}
