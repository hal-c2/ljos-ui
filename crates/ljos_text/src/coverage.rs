//! The last fallback: a font for a character by what fonts have it.
//!
//! Parley takes a cluster's font from the style's families, then from the
//! families Fontique gives the cluster's script as a fallback. A character
//! neither has is drawn as a box, though another font the application knows
//! may have it. The private-use characters of symbol fonts (a shell prompt's
//! Nerd Font arrows) belong to no script, so a fallback by script never finds
//! them. Here such a character is looked up by coverage, as Qt goes on through
//! every font it knows, and the family found joins the fallback of the
//! script Parley gives the character, after what the platform names there.
//!
//! Nothing is read until a text has a box in it. Then an index of which
//! 256-character blocks each family has characters in is made once, from the
//! character map of each family's default font, and a character's lookup reads
//! only the character maps of the families with its block. What a character
//! was found in, or that nothing has it, is kept: a text laid out again, or
//! another with the same character, costs nothing more.
//!
//! With fontconfig (Linux, FreeBSD) the index holds only the fonts the
//! engine holds itself, bundled or added: fontconfig already keeps every
//! system font's characters in its cache and lists the families that have a
//! character at once, while reading a system family through Fontique costs
//! a fontconfig sort per family (120 ms for 334 families).

use std::sync::OnceLock;

use icu_properties::props::Script as IcuScript;
use icu_properties::{CodePointMapData, PropertyNamesShort};
use parley::FontContext;
use parley::fontique::{FallbackKey, FamilyId, Script};
use rustc_hash::{FxHashMap, FxHashSet};
use skrifa::raw::tables::cmap::CmapSubtable;
use skrifa::raw::{FontRef, TableProvider};

/// Characters by block: a font that has one character of a block usually has
/// its neighbours, and an index by block is small.
const BLOCK_SHIFT: u32 = 8;

/// What the engine knows of which fonts have which characters.
#[derive(Default)]
pub(crate) struct Coverage {
    /// The family each character looked up was found in; `None` when no
    /// font has it.
    found: FxHashMap<char, Option<FamilyId>>,
    /// Made on the first lookup, dropped when the fonts change.
    index: Option<Index>,
    /// The families added to a script's fallback, so each is added once.
    added: FxHashSet<(Script, FamilyId)>,
    /// Indexes made ([`Self::indexes`]).
    indexes: u64,
    /// The families of the fonts the engine holds itself, bundled or added.
    own: Vec<FamilyId>,
    /// Whether the platform's fonts are read, and on fontconfig asked of it.
    system: bool,
}

/// For each block, the families with characters in it, by family name.
#[derive(Default)]
struct Index {
    blocks: FxHashMap<u16, Vec<FamilyId>>,
}

impl Coverage {
    pub(crate) fn new(system: bool) -> Self {
        Self {
            system,
            ..Self::default()
        }
    }

    /// `family` is one of the fonts the engine holds itself.
    pub(crate) fn hold(&mut self, family: FamilyId) {
        if !self.own.contains(&family) {
            self.own.push(family);
        }
    }

    /// Fonts were added: a character no font had may now have one.
    pub(crate) fn fonts_changed(&mut self) {
        self.found.retain(|_, family| family.is_some());
        self.index = None;
    }

    /// How many times the index of every family was made: once, unless
    /// fonts are added after a lookup.
    pub(crate) fn indexes(&self) -> u64 {
        self.indexes
    }

    /// Looks up the characters `layout` (Parley's layout of `text`, its
    /// lines broken) draws as boxes, and adds each family found to the
    /// fallback of the script Parley gives the character. True when a
    /// family was added, and laying the text out again would set a box in
    /// it.
    pub(crate) fn cover(
        &mut self,
        fcx: &mut FontContext,
        text: &str,
        layout: &parley::Layout<u32>,
    ) -> bool {
        let missing = missing(text, layout);
        let mut added = false;
        for (at, c) in missing {
            let Some(family) = self.family_for(fcx, c) else {
                continue;
            };
            let key = FallbackKey::new(script_at(text, at), None);
            if !self.added.insert((key.script(), family)) {
                continue;
            }
            // The platform's own fallback for the script is read first, so
            // the family found comes after it rather than in its place.
            if fcx.collection.fallback_families(key).any(|f| f == family) {
                continue;
            }
            added |= fcx
                .collection
                .append_fallbacks(key, std::iter::once(family));
        }
        added
    }

    fn family_for(&mut self, fcx: &mut FontContext, c: char) -> Option<FamilyId> {
        if let Some(known) = self.found.get(&c) {
            return *known;
        }
        if self.index.is_none() {
            let only = system_lists().then_some(self.own.as_slice());
            self.index = Some(Index::read(fcx, only));
            self.indexes += 1;
        }
        let block = (c as u32 >> BLOCK_SHIFT) as u16;
        let mut candidates: Vec<(String, FamilyId)> = self
            .index
            .as_ref()
            .and_then(|index| index.blocks.get(&block))
            .into_iter()
            .flatten()
            .filter_map(|&id| Some((fcx.collection.family_name(id)?.to_lowercase(), id)))
            .collect();
        if self.system && system_lists() {
            for name in system::families_with(c) {
                if let Some(id) = fcx.collection.family_id(&name) {
                    candidates.push((name.to_lowercase(), id));
                }
            }
        }
        // The first family by name, whatever order the platform lists them
        // in, so the same fonts give the same choice everywhere.
        candidates.sort();
        candidates.dedup_by_key(|(_, id)| *id);
        let found = candidates
            .into_iter()
            .map(|(_, id)| id)
            .find(|&family| family_has(fcx, family, c));
        self.found.insert(c, found);
        found
    }
}

impl Index {
    /// The index of `only` those families, or of every family.
    fn read(fcx: &mut FontContext, only: Option<&[FamilyId]>) -> Self {
        let collection = &mut fcx.collection;
        let families = match only {
            Some(only) => only.to_vec(),
            None => collection.family_ids().collect(),
        };
        let mut index = Self::default();
        let mut blocks = Vec::new();
        for id in families {
            let Some(font) = collection
                .family(id)
                .and_then(|f| f.default_font().cloned())
            else {
                continue;
            };
            // Not through the source cache: only the families found are
            // kept open.
            let Some(blob) = font.load(None) else {
                continue;
            };
            blocks.clear();
            blocks_of(blob.as_ref(), font.index(), &mut blocks);
            for &block in &blocks {
                index.blocks.entry(block).or_default().push(id);
            }
        }
        index
    }
}

/// Whether fontconfig lists the system families with a character, so the
/// index leaves them out.
fn system_lists() -> bool {
    cfg!(any(target_os = "linux", target_os = "freebsd"))
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
mod system {
    use std::ffi::CStr;
    use std::ptr;

    use fontconfig_sys::constants::{FC_CHARSET, FC_FAMILY};
    use fontconfig_sys::{
        FcCharSetAddChar, FcCharSetCreate, FcCharSetDestroy, FcFontList, FcFontSetDestroy,
        FcObjectSetAdd, FcObjectSetCreate, FcObjectSetDestroy, FcPatternAddCharSet,
        FcPatternCreate, FcPatternDestroy, FcPatternGetString, FcResultMatch,
    };

    /// The names of the installed families with a font that has `c`, each
    /// of a font's names, as fontconfig's cache of their character maps
    /// says.
    pub(super) fn families_with(c: char) -> Vec<String> {
        let mut names = Vec::new();
        // SAFETY: every object is made by fontconfig, checked for null and
        // destroyed once, after the last read of what it owns; the pattern
        // copies the character set it is given. A null config is the
        // default configuration, which fontconfig loads on first use. The
        // strings read back belong to the font set and are copied before it
        // is destroyed.
        unsafe {
            let pattern = FcPatternCreate();
            let chars = FcCharSetCreate();
            let objects = FcObjectSetCreate();
            if !pattern.is_null()
                && !chars.is_null()
                && !objects.is_null()
                && FcCharSetAddChar(chars, c as u32) != 0
                && FcPatternAddCharSet(pattern, FC_CHARSET.as_ptr(), chars) != 0
                && FcObjectSetAdd(objects, FC_FAMILY.as_ptr()) != 0
            {
                let set = FcFontList(ptr::null_mut(), pattern, objects);
                if !set.is_null() {
                    let fonts = (*set).fonts;
                    for i in 0..usize::try_from((*set).nfont).unwrap_or(0) {
                        let font = *fonts.add(i);
                        for n in 0.. {
                            let mut value = ptr::null_mut();
                            if FcPatternGetString(font, FC_FAMILY.as_ptr(), n, &raw mut value)
                                != FcResultMatch
                                || value.is_null()
                            {
                                break;
                            }
                            names.push(CStr::from_ptr(value.cast()).to_string_lossy().into_owned());
                        }
                    }
                    FcFontSetDestroy(set);
                }
            }
            if !objects.is_null() {
                FcObjectSetDestroy(objects);
            }
            if !chars.is_null() {
                FcCharSetDestroy(chars);
            }
            if !pattern.is_null() {
                FcPatternDestroy(pattern);
            }
        }
        names
    }
}

#[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
mod system {
    pub(super) fn families_with(_c: char) -> Vec<String> {
        Vec::new()
    }
}

/// The blocks the font's character map has characters in, sorted, each once.
fn blocks_of(data: &[u8], index: u32, out: &mut Vec<u16>) {
    let Ok(font) = FontRef::from_index(data, index) else {
        return;
    };
    let Ok(cmap) = font.cmap() else {
        return;
    };
    let Some((_, _, subtable)) = cmap.best_subtable() else {
        return;
    };
    let mut range = |start: u32, end: u32| {
        let end = end.min(char::MAX as u32);
        if start > end {
            return;
        }
        for block in (start >> BLOCK_SHIFT)..=(end >> BLOCK_SHIFT) {
            out.push(block as u16);
        }
    };
    match &subtable {
        CmapSubtable::Format4(t) => {
            for (start, end) in t.start_code().iter().zip(t.end_code()) {
                range(u32::from(start.get()), u32::from(end.get()));
            }
        }
        CmapSubtable::Format12(t) => {
            for g in t.groups() {
                range(g.start_char_code(), g.end_char_code());
            }
        }
        CmapSubtable::Format13(t) => {
            for g in t.groups() {
                range(g.start_char_code(), g.end_char_code());
            }
        }
        other => {
            for (c, _) in other.iter() {
                range(c, c);
            }
        }
    }
    out.sort_unstable();
    out.dedup();
}

/// Whether the default font of `family` has a glyph for `c`.
fn family_has(fcx: &mut FontContext, family: FamilyId, c: char) -> bool {
    let Some(font) = fcx
        .collection
        .family(family)
        .and_then(|f| f.default_font().cloned())
    else {
        return false;
    };
    let Some(blob) = font.load(Some(&mut fcx.source_cache)) else {
        return false;
    };
    font.charmap_index()
        .charmap(blob.as_ref())
        .and_then(|map| map.map(c))
        .is_some_and(|glyph| glyph != 0)
}

/// The characters `layout` sets as a font's glyph 0 (its box), each once,
/// with where in `text` the first of them is. A character that draws as
/// nothing (a control, a space, a joiner or a variation selector) is not
/// one of them.
fn missing(text: &str, layout: &parley::Layout<u32>) -> Vec<(usize, char)> {
    let mut out: Vec<(usize, char)> = Vec::new();
    let mut seen = FxHashSet::default();
    for line in layout.lines() {
        for item in line.items() {
            let parley::PositionedLayoutItem::GlyphRun(run) = item else {
                continue;
            };
            let run = run.run();
            let mut charmap = None;
            for cluster in run.clusters() {
                if !cluster.glyphs().any(|g| g.id == 0) {
                    continue;
                }
                let range = cluster.text_range();
                let Some(part) = text.get(range.clone()) else {
                    continue;
                };
                let font = run.font();
                let map = charmap.get_or_insert_with(|| {
                    FontRef::from_index(font.data.as_ref(), font.index)
                        .ok()
                        .map(|f| skrifa::MetadataProvider::charmap(&f))
                });
                for (i, c) in part.char_indices() {
                    if draws_nothing(c) {
                        continue;
                    }
                    // In a cluster of several characters, only those the
                    // run's font lacks.
                    let has = map.as_ref().and_then(|m| m.map(c));
                    if has.is_some_and(|g| g.to_u32() != 0) || !seen.insert(c) {
                        continue;
                    }
                    out.push((range.start + i, c));
                }
            }
        }
    }
    out
}

/// Characters that are not drawn, which a font need not have.
fn draws_nothing(c: char) -> bool {
    c.is_control()
        || c.is_whitespace()
        || matches!(c,
            '\u{00AD}' | '\u{034F}' | '\u{061C}' | '\u{115F}' | '\u{1160}' | '\u{17B4}'
            | '\u{17B5}' | '\u{180B}'..='\u{180F}' | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{206F}' | '\u{3164}'
            | '\u{FE00}'..='\u{FE0F}' | '\u{FEFF}' | '\u{FFA0}' | '\u{FFF0}'..='\u{FFF8}'
            | '\u{1BCA0}'..='\u{1BCA3}' | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0000}'..='\u{E0FFF}')
}

/// The script Parley's itemiser gives the character at `at`: its own when
/// it has one, else that of the nearest character before it with one, else
/// that of the first character of the text with one, else Latin.
fn script_at(text: &str, at: usize) -> Script {
    let scripts = CodePointMapData::<IcuScript>::new();
    let real = |c: char| {
        let s = scripts.get(c);
        (s != IcuScript::Common && s != IcuScript::Unknown && s != IcuScript::Inherited)
            .then_some(s)
    };
    let own = text[at..].chars().next().and_then(real);
    let script = own
        .or_else(|| text[..at].chars().rev().find_map(real))
        .or_else(|| text.chars().find_map(real))
        .unwrap_or(IcuScript::Latin);
    static NAMES: OnceLock<icu_properties::PropertyNamesShortBorrowed<'static, IcuScript>> =
        OnceLock::new();
    NAMES
        .get_or_init(PropertyNamesShort::new)
        .get(script)
        .and_then(|name| Script::parse(name).ok())
        .unwrap_or(Script::from_bytes(*b"Latn"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_character_without_a_script_takes_the_script_before_it() {
        assert_eq!(script_at("\u{E0B0}", 0), Script::from_bytes(*b"Latn"));
        assert_eq!(script_at("ab \u{E0B0}", 3), Script::from_bytes(*b"Latn"));
        assert_eq!(script_at("αβ \u{E0B0}", 5), Script::from_bytes(*b"Grek"));
        assert_eq!(script_at("\u{E0B0} αβ", 0), Script::from_bytes(*b"Grek"));
        assert_eq!(script_at("ab αβ", 3), Script::from_bytes(*b"Grek"));
    }

    #[test]
    fn spaces_joiners_and_selectors_are_never_looked_up() {
        for c in [' ', '\n', '\u{200D}', '\u{FE0F}', '\u{00AD}', '\u{E0041}'] {
            assert!(draws_nothing(c), "{c:?}");
        }
        for c in ['a', '\u{E0B0}', '\u{10FFF0}', '\u{2603}'] {
            assert!(!draws_nothing(c), "{c:?}");
        }
    }
}
