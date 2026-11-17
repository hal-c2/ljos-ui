//! The sources an application's pack carries for its users (the `ljos`
//! feature `user-qml`, ARCHITECTURE.md §12.1): the text of every file
//! its QML was compiled from, so that a file its user adds or changes is
//! compiled against the application's own types as they were written.
//!
//! The engine keeps these bytes as the pack's trailer and never reads
//! them; only a program that lets its users change it decodes them, and
//! only when a user's file is there to compile.

use std::collections::BTreeMap;

use qml_front::project::ControlsStyle;

/// What a pack's trailer begins with: a pack without sources has none.
const MAGIC: &[u8; 6] = b"LJSRC\0";
const VERSION: u8 = 1;
/// The body is deflated (RFC 1951).
const DEFLATED: u8 = 1;

/// The files an application's QML was compiled from, by path from its
/// root (`/`-separated, `parts/Card.qml`): its QML, scripts, their
/// declarations, and the `qmldir` and `CMakeLists.txt` files that
/// describe its modules. With the style its `import QtQuick.Controls`
/// was compiled against.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackSources {
    pub files: BTreeMap<String, String>,
    pub style: Option<ControlsStyle>,
}

/// Why a pack's trailer is not sources this version reads.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SourcesError {
    #[error("the pack carries no sources (built without `with_sources`)")]
    Missing,
    #[error("the pack's sources are of version {0}, not {VERSION}")]
    Version(u8),
    #[error("the pack's sources are cut short or damaged")]
    Damaged,
}

impl PackSources {
    /// Whether `name` (a file's name) is one the sources carry.
    #[must_use]
    pub fn carries(name: &str) -> bool {
        name.ends_with(".qml")
            || qml_front::project::is_script(name)
            || crate::script::module::is_declarations(name)
            || name == "qmldir"
            || name == "CMakeLists.txt"
    }

    /// The bytes for a pack's trailer, deflated when `deflate`.
    #[must_use]
    pub fn encode(&self, deflate: bool) -> Vec<u8> {
        let mut body = Vec::new();
        match &self.style {
            None => body.push(0),
            Some(style) => {
                body.push(1);
                put(&mut body, style.name.as_bytes());
                match &style.fallback {
                    None => body.push(0),
                    Some(fallback) => {
                        body.push(1);
                        put(&mut body, fallback.as_bytes());
                    }
                }
            }
        }
        body.extend_from_slice(&(self.files.len() as u32).to_le_bytes());
        for (path, text) in &self.files {
            put(&mut body, path.as_bytes());
            put(&mut body, text.as_bytes());
        }
        let mut out = Vec::with_capacity(body.len() + 12);
        out.extend_from_slice(MAGIC);
        out.push(VERSION);
        if deflate {
            out.push(DEFLATED);
            out.extend_from_slice(&(body.len() as u32).to_le_bytes());
            out.extend_from_slice(&miniz_oxide::deflate::compress_to_vec(&body, 9));
        } else {
            out.push(0);
            out.extend_from_slice(&body);
        }
        out
    }

    /// The sources in a pack's trailer.
    ///
    /// # Errors
    /// [`SourcesError::Missing`] when the trailer is not sources at all.
    pub fn decode(trailer: &[u8]) -> Result<Self, SourcesError> {
        let rest = trailer.strip_prefix(MAGIC).ok_or(SourcesError::Missing)?;
        let [version, flags, rest @ ..] = rest else {
            return Err(SourcesError::Damaged);
        };
        if *version != VERSION {
            return Err(SourcesError::Version(*version));
        }
        let inflated;
        let body = if flags & DEFLATED != 0 {
            let (len, rest) = rest.split_first_chunk::<4>().ok_or(SourcesError::Damaged)?;
            let len = u32::from_le_bytes(*len) as usize;
            inflated = miniz_oxide::inflate::decompress_to_vec_with_limit(rest, len)
                .map_err(|_| SourcesError::Damaged)?;
            inflated.as_slice()
        } else {
            rest
        };
        let mut at = Reader(body);
        let style = match at.byte()? {
            0 => None,
            _ => {
                let name = at.text()?;
                let fallback = match at.byte()? {
                    0 => None,
                    _ => Some(at.text()?),
                };
                Some(ControlsStyle { name, fallback })
            }
        };
        let count = at.u32()?;
        let mut files = BTreeMap::new();
        for _ in 0..count {
            let path = at.text()?;
            let text = at.text()?;
            files.insert(path, text);
        }
        Ok(Self { files, style })
    }
}

fn put(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn byte(&mut self) -> Result<u8, SourcesError> {
        let (first, rest) = self.0.split_first().ok_or(SourcesError::Damaged)?;
        self.0 = rest;
        Ok(*first)
    }

    fn u32(&mut self) -> Result<u32, SourcesError> {
        let (len, rest) = self
            .0
            .split_first_chunk::<4>()
            .ok_or(SourcesError::Damaged)?;
        self.0 = rest;
        Ok(u32::from_le_bytes(*len))
    }

    fn text(&mut self) -> Result<String, SourcesError> {
        let len = self.u32()? as usize;
        if self.0.len() < len {
            return Err(SourcesError::Damaged);
        }
        let (text, rest) = self.0.split_at(len);
        self.0 = rest;
        String::from_utf8(text.to_vec()).map_err(|_| SourcesError::Damaged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PackSources {
        PackSources {
            files: BTreeMap::from([
                (
                    "Main.qml".to_owned(),
                    "import QtQuick\nItem {}\n".to_owned(),
                ),
                (
                    "parts/qmldir".to_owned(),
                    "module Parts\nCard 1.0 Card.qml\n".to_owned(),
                ),
            ]),
            style: Some(ControlsStyle {
                name: "Fusion".to_owned(),
                fallback: None,
            }),
        }
    }

    #[test]
    fn sources_read_back_as_they_were_written_deflated_or_not() {
        for deflate in [false, true] {
            let bytes = sample().encode(deflate);
            assert_eq!(PackSources::decode(&bytes), Ok(sample()), "{deflate}");
        }
    }

    #[test]
    fn a_trailer_that_is_not_sources_says_so() {
        assert_eq!(PackSources::decode(b""), Err(SourcesError::Missing));
        let mut bytes = sample().encode(true);
        bytes.truncate(bytes.len() - 3);
        assert_eq!(PackSources::decode(&bytes), Err(SourcesError::Damaged));
    }
}
