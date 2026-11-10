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
    /// A WOFF2 or WOFF file (named by the format) that could not be unpacked:
    /// cut short, or with tables that do not add up.
    #[error("the {0} data is corrupt")]
    Corrupt(&'static str),
    /// A WOFF2 or WOFF file that says it unpacks to more than any font
    /// needs; it is refused before the memory is asked for.
    #[error("the {0} data unpacks to more than {max} bytes", max = MAX_UNPACKED)]
    TooLarge(&'static str),
    /// A WOFF2 or WOFF file in a build without the `woff` feature.
    #[error("this build reads no {0} fonts (the `woff` feature is off)")]
    Unsupported(&'static str),
}

/// The most a WOFF2 or WOFF file may unpack to: the limit the WOFF2 decoder
/// keeps to, applied to WOFF as well so the two formats agree.
const MAX_UNPACKED: u64 = 128 * 1024 * 1024;

/// The TrueType or OpenType bytes of a font file. WOFF2 and WOFF, the web's
/// packings, are unpacked, and the packed bytes are dropped here, so only
/// the unpacked font is held; anything else is passed on as it is.
#[cfg(feature = "woff")]
fn sfnt(bytes: Vec<u8>) -> Result<Vec<u8>, FontError> {
    match bytes.get(..4) {
        Some(b"wOF2") => {
            // The decoder refuses a font past this limit with the error it
            // gives a broken one, so the size is read here from the header,
            // the font the file says it holds. That is a claim, not what the
            // decoder measures (its table directory, then its output), so
            // a header that understates still fails as corrupt. And one that
            // claims more than the limit is refused even if its tables would
            // unpack: no well-formed font says it is larger than it is.
            if woff2_unpacked_size(&bytes).ok_or(FontError::Corrupt("WOFF2"))? > MAX_UNPACKED {
                return Err(FontError::TooLarge("WOFF2"));
            }
            wuff::decompress_woff2(&bytes).map_err(|_| FontError::Corrupt("WOFF2"))
        }
        Some(b"wOFF") => {
            // The decoder sets aside each table's stated size before it
            // unpacks it, so a short file could otherwise ask for gigabytes.
            if woff_unpacked_size(&bytes).ok_or(FontError::Corrupt("WOFF"))? > MAX_UNPACKED {
                return Err(FontError::TooLarge("WOFF"));
            }
            wuff::decompress_woff1(&bytes).map_err(|_| FontError::Corrupt("WOFF"))
        }
        _ => Ok(bytes),
    }
}

#[cfg(not(feature = "woff"))]
fn sfnt(bytes: Vec<u8>) -> Result<Vec<u8>, FontError> {
    match bytes.get(..4) {
        Some(b"wOF2") => Err(FontError::Unsupported("WOFF2")),
        Some(b"wOFF") => Err(FontError::Unsupported("WOFF")),
        _ => Ok(bytes),
    }
}

/// What a WOFF2 file's header says the font unpacks to; `None` when the
/// header is cut short.
#[cfg(feature = "woff")]
fn woff2_unpacked_size(bytes: &[u8]) -> Option<u64> {
    // https://www.w3.org/TR/WOFF2/: totalSfntSize, at 16 in the header.
    Some(u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?).into())
}

/// What a WOFF file's tables say they unpack to, added up; `None` when its
/// table directory is cut short.
#[cfg(feature = "woff")]
fn woff_unpacked_size(bytes: &[u8]) -> Option<u64> {
    // https://www.w3.org/TR/WOFF/: a 44-byte header (the table count at 12)
    // then 20 bytes a table, the unpacked length at 12 in each.
    let count = u16::from_be_bytes(bytes.get(12..14)?.try_into().ok()?);
    (0..usize::from(count)).try_fold(0u64, |sum, i| {
        let at = 44 + i * 20 + 12;
        let len = u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?);
        Some(sum + u64::from(len))
    })
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
    if system_fonts {
        use objc2_core_text::CTFontUIFontType;
        // `monospace` as `QFontDatabase::systemFont(FixedFont)` gives it:
        // Fontique names Courier for it, which a Mac may not list under
        // that name and which is not the face macOS calls fixed pitch.
        for (generic, kind) in [
            (GenericFamily::SystemUi, CTFontUIFontType::System),
            (GenericFamily::Monospace, CTFontUIFontType::UserFixedPitch),
        ] {
            if let Some(path) = system_ui::font_path(kind) {
                lead_generic(&mut collection, generic, &path);
            }
        }
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

/// Puts the families of the font file at `path` first among `generic`'s,
/// reading the file when the collection has not. Fontique finds the
/// platform's generic families by family names it knows ("System Font" and
/// ".SF NS" for `system-ui` on macOS, Courier for `monospace`), and the
/// file of the platform's font need not be named either: then `system-ui`
/// was empty and text fell to `sans-serif` (Helvetica). The file the
/// platform names is found whatever it calls itself. `false` when it holds
/// no font.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn lead_generic(
    collection: &mut Collection,
    generic: GenericFamily,
    path: &Path,
) -> bool {
    let mut lead = families_of_file(collection, path);
    if lead.is_empty() {
        collection.load_fonts_from_paths([path]);
        lead = families_of_file(collection, path);
    }
    if lead.is_empty() {
        return false;
    }
    let rest: Vec<FamilyId> = collection
        .generic_families(generic)
        .filter(|id| !lead.contains(id))
        .collect();
    collection.set_generic_families(generic, lead.into_iter().chain(rest));
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

/// The interface fonts as CoreText gives them.
#[cfg(target_os = "macos")]
mod system_ui {
    use std::path::PathBuf;

    use objc2_core_foundation::{CFURL, CFURLPathStyle};
    use objc2_core_text::{CTFont, CTFontUIFontType, kCTFontURLAttribute};

    /// The file of the font CoreText draws that kind of interface text in.
    pub(super) fn font_path(kind: CTFontUIFontType) -> Option<PathBuf> {
        // SAFETY: CoreText calls with no pointers of ours; each result is
        // checked for absence before it is used.
        let font = unsafe { CTFont::new_ui_font_for_language(kind, 0.0, None) }?;
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
    let bytes = sfnt(bytes)?;
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
    let bytes = sfnt(bytes)?;
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
        assert!(lead_generic(
            &mut c,
            GenericFamily::SystemUi,
            bundled_file()
        ));
        let ids: Vec<FamilyId> = c.generic_families(GenericFamily::SystemUi).collect();
        let lead = families_of_file(&mut c, bundled_file());
        assert_eq!(lead.len(), 1);
        assert_eq!(ids[0], lead[0]);
        assert_eq!(c.family_name(ids[0]), Some(BUNDLED_FAMILY));
        assert_eq!(ids.iter().filter(|&&id| id == lead[0]).count(), 1);
    }

    // On a Mac without the names Fontique knows for `monospace` (Courier),
    // the generic was empty and its text fell to `sans-serif`.
    #[test]
    fn the_platforms_fixed_pitch_file_fills_an_empty_monospace() {
        let mut c = bare();
        assert_eq!(c.generic_families(GenericFamily::Monospace).count(), 0);
        assert!(lead_generic(
            &mut c,
            GenericFamily::Monospace,
            bundled_file()
        ));
        let ids: Vec<FamilyId> = c.generic_families(GenericFamily::Monospace).collect();
        assert_eq!(ids, families_of_file(&mut c, bundled_file()));
        assert_eq!(c.generic_families(GenericFamily::SystemUi).count(), 0);
    }

    #[test]
    fn a_file_with_no_font_leaves_system_ui_as_it_was() {
        let mut c = bare();
        let not_a_font = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));
        assert!(!lead_generic(&mut c, GenericFamily::SystemUi, not_a_font));
        assert_eq!(c.generic_families(GenericFamily::SystemUi).count(), 0);
    }
}
