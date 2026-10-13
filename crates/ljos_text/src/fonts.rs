//! The bundled fonts and the font collection.

use parley::FontContext;
use parley::fontique::{Blob, Collection, CollectionOptions, GenericFamily, SourceCache};

/// The family every engine has: Liberation Sans, regular and bold
/// (SIL Open Font License 1.1, see `fonts/LICENSE-LiberationSans.txt`).
/// It covers Latin, Greek, Cyrillic and Hebrew.
pub const BUNDLED_FAMILY: &str = "Liberation Sans";

static REGULAR: &[u8] = include_bytes!("../fonts/LiberationSans-Regular.ttf");
static BOLD: &[u8] = include_bytes!("../fonts/LiberationSans-Bold.ttf");

/// A font file could not be used.
#[derive(Debug, thiserror::Error)]
pub enum FontError {
    /// The bytes hold no font Fontique can read.
    #[error("the data contains no usable font")]
    NoFont,
}

/// Generic families a style sheet or a platform theme may name. With bundled
/// fonts only, all of them resolve to the bundled family so text never
/// disappears.
const GENERICS: [GenericFamily; 6] = [
    GenericFamily::SansSerif,
    GenericFamily::Serif,
    GenericFamily::Monospace,
    GenericFamily::SystemUi,
    GenericFamily::UiSansSerif,
    GenericFamily::Emoji,
];

pub(crate) fn font_context(system_fonts: bool) -> FontContext {
    let mut collection = Collection::new(CollectionOptions {
        shared: false,
        system_fonts,
    });
    let mut bundled = Vec::new();
    for bytes in [REGULAR, BOLD] {
        for (id, _) in collection.register_fonts(Blob::new(std::sync::Arc::new(bytes)), None) {
            if !bundled.contains(&id) {
                bundled.push(id);
            }
        }
    }
    for generic in GENERICS {
        if system_fonts {
            // After the platform's own choice: a last resort, not an override.
            collection.append_generic_families(generic, bundled.iter().copied());
        } else {
            collection.set_generic_families(generic, bundled.iter().copied());
        }
    }
    FontContext {
        collection,
        source_cache: SourceCache::default(),
    }
}

/// One face a registered font file added: what `FontLoader.font` reports.
#[derive(Clone, Debug, PartialEq)]
pub struct RegisteredFace {
    pub family: String,
    /// CSS weight, 1 to 1000 (400 regular, 700 bold).
    pub weight: f32,
    pub italic: bool,
}

/// Register a font file and return each face it adds.
pub(crate) fn register_faces(
    fcx: &mut FontContext,
    bytes: Vec<u8>,
) -> Result<Vec<RegisteredFace>, FontError> {
    let added = fcx
        .collection
        .register_fonts(Blob::new(std::sync::Arc::new(bytes)), None);
    let mut faces = Vec::new();
    for (id, infos) in &added {
        let Some(family) = fcx.collection.family_name(*id).map(str::to_owned) else {
            continue;
        };
        for info in infos {
            faces.push(RegisteredFace {
                family: family.clone(),
                weight: info.weight().value(),
                italic: !matches!(info.style(), parley::fontique::FontStyle::Normal),
            });
        }
    }
    if faces.is_empty() {
        return Err(FontError::NoFont);
    }
    Ok(faces)
}

/// Register a font file and return the family names it adds.
pub(crate) fn register(fcx: &mut FontContext, bytes: Vec<u8>) -> Result<Vec<String>, FontError> {
    let added = fcx
        .collection
        .register_fonts(Blob::new(std::sync::Arc::new(bytes)), None);
    let names: Vec<String> = added
        .iter()
        .filter_map(|(id, _)| fcx.collection.family_name(*id).map(str::to_owned))
        .collect();
    if names.is_empty() {
        return Err(FontError::NoFont);
    }
    Ok(names)
}
