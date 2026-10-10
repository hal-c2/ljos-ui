//! The corpus manifest (`examples.json`) and the reference pictures beside it.
//!
//! Only the fields the comparison needs are typed; everything else is kept
//! as raw JSON and handed to renderers, so the manifest can grow without this
//! crate learning about it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("{path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("{path}: {source}")]
    Json {
        path: String,
        source: serde_json::Error,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct QtNotes {
    #[serde(default)]
    pub runnable: bool,
    /// Why the reference tool cannot render it.
    pub reason: Option<String>,
    /// Milliseconds to let it settle before the picture (default 1500).
    pub settle: Option<u64>,
    #[serde(flatten)]
    pub rest: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Example {
    pub id: String,
    pub dir: String,
    pub entry: String,
    #[serde(default)]
    pub window: Window,
    /// Why its picture is not the same twice: `time`, `animation`, `network`.
    pub volatile: Option<String>,
    pub qt: Option<QtNotes>,
    #[serde(flatten)]
    pub rest: BTreeMap<String, Value>,
}

impl Example {
    pub const DEFAULT_SETTLE_MS: u64 = 1500;

    pub fn settle_ms(&self) -> u64 {
        self.qt
            .as_ref()
            .and_then(|qt| qt.settle)
            .unwrap_or(Self::DEFAULT_SETTLE_MS)
    }

    /// Why the reference tool could not render it, if it could not.
    pub fn qt_reason(&self) -> Option<&str> {
        self.qt
            .as_ref()
            .filter(|qt| !qt.runnable)
            .and_then(|qt| qt.reason.as_deref())
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Manifest {
    /// Directory of the examples, relative to the manifest.
    #[serde(default)]
    pub root: String,
    pub screen: Option<Value>,
    pub examples: Vec<Example>,
    #[serde(skip)]
    pub file: PathBuf,
}

/// `reference/reference.json`: when each picture was taken.
#[derive(Clone, Debug, Default, Deserialize)]
struct ReferenceEntry {
    #[serde(rename = "capturedAt")]
    captured_at: Option<String>,
}

impl Manifest {
    pub fn read(file: &Path) -> Result<Self, ManifestError> {
        let mut manifest: Manifest = read_json(file)?;
        manifest.file = file.to_path_buf();
        Ok(manifest)
    }

    /// The corpus directory: where the manifest, `reference/` and `standins/`
    /// live.
    pub fn corpus(&self) -> &Path {
        self.file.parent().unwrap_or(Path::new("."))
    }

    pub fn directory(&self, example: &Example) -> PathBuf {
        self.corpus().join(&self.root).join(&example.dir)
    }

    /// The reference picture, if one has been captured. Qt-derived pictures
    /// are never committed; they are generated into the corpus checkout.
    pub fn reference(&self, example: &Example) -> Option<PathBuf> {
        let path = self
            .corpus()
            .join("reference")
            .join(format!("{}.png", example.id));
        path.is_file().then_some(path)
    }

    pub fn captured_at(&self, example: &Example) -> Option<String> {
        let index: BTreeMap<String, ReferenceEntry> =
            read_json(&self.corpus().join("reference").join("reference.json")).ok()?;
        index.get(&example.id)?.captured_at.clone()
    }
}

pub fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, ManifestError> {
    let text = std::fs::read_to_string(path).map_err(|source| ManifestError::Io {
        path: path.display().to_string(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| ManifestError::Json {
        path: path.display().to_string(),
        source,
    })
}
