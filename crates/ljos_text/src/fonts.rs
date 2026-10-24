//! The bundled fonts and the font collection.

use std::path::Path;

use parley::FontContext;
use parley::fontique::{
    Blob, Collection, CollectionOptions, FamilyId, GenericFamily, SourceCache, SourceKind,
};

/// The family every engine has: Liberation Sans, regular and bold
/// (SIL Open Font License 1.1, see `fonts/LICENSE-LiberationSans.txt`).
/// It covers Latin, Greek, Cyrillic and Hebrew.
pub const BUNDLED_FAMILY: &str = "Liberation Sans";

static REGULAR: &[u8] = include_bytes!("../fonts/LiberationSans-Regular.ttf");
static BOLD: &[u8] = include_bytes!("../fonts/LiberationSans-Bold.ttf");

/// The regular face of [`BUNDLED_FAMILY`], for a host that draws text of
/// its own and should not carry another font for it.
pub fn bundled_regular() -> &'static [u8] {
    REGULAR
}

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
    #[cfg(target_os = "macos")]
    if system_fonts && let Some(path) = system_ui::font_path() {
        lead_system_ui(&mut collection, &path);
    }
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

/// Puts the families of the font file at `path` first among `system-ui`'s,
/// reading the file when the collection has not. Fontique finds the
/// platform's `system-ui` by family names it knows ("System Font" and
/// ".SF NS" on macOS), and the file of the system UI font need not be
/// named either: then `system-ui` was empty and text fell to
/// `sans-serif` (Helvetica). The file the platform names is found
/// whatever it calls itself. `false` when it holds no font.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn lead_system_ui(collection: &mut Collection, path: &Path) -> bool {
    let mut lead = families_of_file(collection, path);
    if lead.is_empty() {
        collection.load_fonts_from_paths([path]);
        lead = families_of_file(collection, path);
    }
    if lead.is_empty() {
        return false;
    }
    let rest: Vec<FamilyId> = collection
        .generic_families(GenericFamily::SystemUi)
        .filter(|id| !lead.contains(id))
        .collect();
    collection.set_generic_families(GenericFamily::SystemUi, lead.into_iter().chain(rest));
    true
}

/// The families with a font read from the file at `path`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn families_of_file(collection: &mut Collection, path: &Path) -> Vec<FamilyId> {
    let ids: Vec<FamilyId> = collection.family_ids().collect();
    ids.into_iter()
        .filter(|&id| {
            collection.family(id).is_some_and(|family| {
                family
                    .fonts()
                    .iter()
                    .any(|font| matches!(font.source().kind(), SourceKind::Path(p) if **p == *path))
            })
        })
        .collect()
}

/// The system UI font as CoreText gives it.
#[cfg(target_os = "macos")]
mod system_ui {
    use std::path::PathBuf;

    use objc2_core_foundation::{CFURL, CFURLPathStyle};
    use objc2_core_text::{CTFont, CTFontUIFontType, kCTFontURLAttribute};

    /// The file of the font CoreText draws the interface in.
    pub(super) fn font_path() -> Option<PathBuf> {
        // SAFETY: CoreText calls with no pointers of ours; each result is
        // checked for absence before it is used.
        let font =
            unsafe { CTFont::new_ui_font_for_language(CTFontUIFontType::System, 0.0, None) }?;
        let url = unsafe { font.attribute(kCTFontURLAttribute) }?;
        let url = url.downcast::<CFURL>().ok()?;
        let path = url.file_system_path(CFURLPathStyle::CFURLPOSIXPathStyle)?;
        Some(PathBuf::from(path.to_string()))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn bare() -> Collection {
        Collection::new(CollectionOptions {
            shared: false,
            system_fonts: false,
        })
    }

    fn bundled_file() -> &'static Path {
        Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fonts/LiberationSans-Bold.ttf"
        ))
    }

    #[test]
    fn the_platforms_ui_font_file_leads_system_ui_whatever_it_is_named() {
        let mut c = bare();
        let other = c
            .register_fonts(Blob::new(std::sync::Arc::new(REGULAR)), None)
            .first()
            .map(|(id, _)| *id)
            .unwrap();
        c.set_generic_families(GenericFamily::SystemUi, [other].into_iter());
        // Named as nothing Fontique looks for; found by its file.
        assert!(lead_system_ui(&mut c, bundled_file()));
        let ids: Vec<FamilyId> = c.generic_families(GenericFamily::SystemUi).collect();
        let lead = families_of_file(&mut c, bundled_file());
        assert_eq!(lead.len(), 1);
        assert_eq!(ids[0], lead[0]);
        assert_eq!(c.family_name(ids[0]), Some(BUNDLED_FAMILY));
        assert_eq!(ids.iter().filter(|&&id| id == lead[0]).count(), 1);
    }

    #[test]
    fn a_file_with_no_font_leaves_system_ui_as_it_was() {
        let mut c = bare();
        let not_a_font = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));
        assert!(!lead_system_ui(&mut c, not_a_font));
        assert_eq!(c.generic_families(GenericFamily::SystemUi).count(), 0);
    }
}
