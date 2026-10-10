//! What `qsTr` asks: the installed catalogues, the UI language that picks
//! among them, and the locale numbers are written in.
//!
//! `I18n` is plain state. The engine keeps the reactive signals beside it
//! and bumps them when a setter here says something changed, so this crate
//! does not depend on the reactive graph and tools can use it too.

use crate::{
    LocaleInfo, PluralForms,
    catalog::{Entry, Messages},
    locale,
};

struct Installed {
    /// The catalogue's language in canonical form, to compare with the
    /// names in the fallback chain.
    name: String,
    plural: PluralForms,
    messages: Box<dyn Messages>,
}

/// The localisation state of one application.
pub struct I18n {
    /// Most recently installed last; looked up from the back, so a later
    /// catalogue overrides an earlier one, as with Qt's translators.
    catalogs: Vec<Installed>,
    ui_language: String,
    locale: LocaleInfo,
    /// The catalogues the UI language selects, in lookup order.
    chain: Vec<usize>,
}

impl Default for I18n {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for I18n {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("I18n")
            .field("ui_language", &self.ui_language)
            .field("locale", &self.locale.name())
            .field(
                "catalogs",
                &self.catalogs.iter().map(|c| &c.name).collect::<Vec<_>>(),
            )
            .finish_non_exhaustive()
    }
}

/// A language name in the one spelling names are compared in.
fn canonical(name: &str) -> String {
    locale::parse(name).map_or_else(|| name.to_ascii_lowercase(), |l| l.to_string())
}

/// The names a UI language is looked for under, most specific first: the
/// name itself, without its script, with the region the language is most
/// likely spoken in, and the language alone.
fn fallbacks(language: &str) -> Vec<String> {
    let Some(parsed) = locale::parse(language) else {
        return vec![canonical(language)];
    };
    let mut out = vec![parsed.to_string()];
    let lang = parsed.id.language.as_str();
    if let Some(region) = parsed.id.region {
        out.push(format!("{lang}-{}", region.as_str()));
    }
    if let Some(region) = locale::maximized(&parsed).and_then(|id| id.region) {
        out.push(format!("{lang}-{}", region.as_str()));
    }
    out.push(lang.to_owned());
    let mut seen = Vec::new();
    out.retain(|name| {
        let new = !seen.contains(name);
        if new {
            seen.push(name.clone());
        }
        new
    });
    out
}

impl I18n {
    /// English in the United States, no catalogue: what Qt starts with
    /// under `LANG=en_US.UTF-8`. A host that follows the system calls
    /// [`set_locale`](Self::set_locale) and
    /// [`set_ui_language`](Self::set_ui_language) with
    /// [`locale::system_locale_name`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            catalogs: Vec::new(),
            ui_language: "en".to_owned(),
            locale: LocaleInfo::new("en_US"),
            chain: Vec::new(),
        }
    }

    /// Adds a catalogue. It is used whenever the UI language is, or falls
    /// back to, the catalogue's language.
    pub fn install(&mut self, messages: Box<dyn Messages>) {
        let language = messages.language();
        self.catalogs.push(Installed {
            name: canonical(language),
            plural: PluralForms::for_language(language),
            messages,
        });
        self.rechain();
    }

    /// Removes every catalogue of a language, so a changed file can be
    /// installed in its place. Says whether there was one.
    pub fn uninstall(&mut self, language: &str) -> bool {
        let name = canonical(language);
        let before = self.catalogs.len();
        self.catalogs.retain(|c| c.name != name);
        self.rechain();
        self.catalogs.len() != before
    }

    fn rechain(&mut self) {
        self.chain.clear();
        for name in fallbacks(&self.ui_language) {
            for (i, catalog) in self.catalogs.iter().enumerate().rev() {
                if catalog.name == name {
                    self.chain.push(i);
                }
            }
        }
    }

    #[must_use]
    pub fn ui_language(&self) -> &str {
        &self.ui_language
    }

    /// Sets the language translations are looked up in. `false` when it
    /// already was that language, so nothing needs translating again.
    pub fn set_ui_language(&mut self, language: &str) -> bool {
        if self.ui_language == language {
            return false;
        }
        language.clone_into(&mut self.ui_language);
        self.rechain();
        true
    }

    /// Whether some installed catalogue serves the UI language.
    #[must_use]
    pub fn has_catalog(&self) -> bool {
        !self.chain.is_empty()
    }

    /// Whether the UI language is written right to left.
    #[must_use]
    pub fn is_right_to_left(&self) -> bool {
        locale::is_right_to_left(&self.ui_language)
    }

    /// The locale numbers are written in (`Qt.locale()`).
    #[must_use]
    pub const fn locale(&self) -> &LocaleInfo {
        &self.locale
    }

    /// Sets the default locale; `false` when it already was that one.
    pub fn set_locale(&mut self, name: &str) -> bool {
        let locale = LocaleInfo::new(name);
        if locale.name() == self.locale.name() {
            return false;
        }
        self.locale = locale;
        true
    }

    /// `qsTr(source, comment, n)` called in `context`.
    ///
    /// The message with that disambiguation is tried first, then the one
    /// without, in each catalogue of the fallback chain; a message nobody
    /// translated is the source text.
    #[must_use]
    pub fn translate(&self, context: &str, source: &str, comment: &str, n: Option<u32>) -> String {
        let found = self.chain.iter().find_map(|&i| {
            let catalog = &self.catalogs[i];
            let exact = catalog.messages.text(context, source, comment);
            let plain = || {
                (!comment.is_empty())
                    .then(|| catalog.messages.text(context, source, ""))
                    .flatten()
            };
            let entry = exact.or_else(plain)?;
            form(catalog, entry, n)
        });
        self.with_count(found.unwrap_or(source), n)
    }

    /// `qsTrId(id, n)`: the id itself when nobody translated it.
    #[must_use]
    pub fn translate_id(&self, id: &str, n: Option<u32>) -> String {
        let found = self.chain.iter().find_map(|&i| {
            let catalog = &self.catalogs[i];
            form(catalog, catalog.messages.id(id)?, n)
        });
        self.with_count(found.unwrap_or(id), n)
    }

    /// Puts the count where the text says `%n`, and in the locale's digits
    /// and separators where it says `%Ln`.
    fn with_count(&self, text: &str, n: Option<u32>) -> String {
        let Some(n) = n else {
            return text.to_owned();
        };
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(at) = rest.find('%') {
            out.push_str(&rest[..at]);
            let after = &rest[at + 1..];
            if let Some(tail) = after.strip_prefix('n') {
                out.push_str(&n.to_string());
                rest = tail;
            } else if let Some(tail) = after.strip_prefix("Ln") {
                out.push_str(&self.locale.format_integer(i64::from(n)));
                rest = tail;
            } else {
                out.push('%');
                rest = after;
            }
        }
        out.push_str(rest);
        out
    }
}

/// The form of a message that `n` selects in the catalogue's language.
///
/// `None` when the message has no such form: a translation written without
/// plural forms, asked for a count that needs another one. The caller then
/// goes on down the chain to the source text, as Qt does.
fn form(catalog: &Installed, entry: Entry, n: Option<u32>) -> Option<&str> {
    let index = n.map_or(0, |n| catalog.plural.index(n));
    catalog.messages.form(entry, index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Catalog, Translation};

    fn icelandic() -> Box<Catalog> {
        let mut c = Catalog::new("is");
        c.insert("Main", "Open", "", Translation::Text("Opna".into()));
        c.insert(
            "Main",
            "Open",
            "adjective",
            Translation::Text("Opinn".into()),
        );
        c.insert("Main", "Single %n", "", Translation::Text("Einn %n".into()));
        c.insert(
            "Main",
            "%n file(s)",
            "",
            Translation::Numerus(vec!["%n skrá".into(), "%n skrár".into()]),
        );
        c.insert_id("app-title", Translation::Text("Ljós".into()));
        Box::new(c)
    }

    fn i18n(language: &str) -> I18n {
        let mut i = I18n::new();
        i.install(icelandic());
        i.set_ui_language(language);
        i
    }

    #[test]
    fn without_a_catalogue_the_source_text_is_the_translation() {
        let i = I18n::new();
        assert_eq!(i.translate("Main", "Open", "", None), "Open");
        assert_eq!(i.translate("Main", "%n file(s)", "", Some(2)), "2 file(s)");
        assert_eq!(i.translate("Main", "%n file(s)", "", None), "%n file(s)");
        assert_eq!(
            i.translate("Main", "%Ln bytes", "", Some(12_345)),
            "12,345 bytes"
        );
        assert_eq!(i.translate_id("app-title", None), "app-title");
    }

    #[test]
    fn a_catalogue_is_used_only_in_its_language() {
        assert_eq!(i18n("is").translate("Main", "Open", "", None), "Opna");
        assert_eq!(i18n("de").translate("Main", "Open", "", None), "Open");
        assert_eq!(I18n::new().translate("Main", "Open", "", None), "Open");
    }

    #[test]
    fn a_region_falls_back_to_its_language_and_a_language_finds_its_likely_region() {
        assert_eq!(i18n("is_IS").translate("Main", "Open", "", None), "Opna");
        assert_eq!(i18n("is-IS").translate("Main", "Open", "", None), "Opna");
        let mut regional = I18n::new();
        let mut c = Catalog::new("is_IS");
        c.insert("Main", "Open", "", Translation::Text("Opna".into()));
        regional.install(Box::new(c));
        regional.set_ui_language("is");
        assert_eq!(regional.translate("Main", "Open", "", None), "Opna");
    }

    #[test]
    fn disambiguation_picks_its_own_message_and_falls_back_to_the_plain_one() {
        let i = i18n("is");
        assert_eq!(i.translate("Main", "Open", "adjective", None), "Opinn");
        assert_eq!(i.translate("Main", "Open", "noun", None), "Opna");
        assert_eq!(i.translate("Other", "Open", "", None), "Open");
    }

    #[test]
    fn the_count_selects_the_form_of_the_language() {
        let i = i18n("is");
        let say = |n| i.translate("Main", "%n file(s)", "", Some(n));
        assert_eq!(say(1), "1 skrá");
        assert_eq!(say(21), "21 skrá");
        assert_eq!(say(11), "11 skrár");
        assert_eq!(say(0), "0 skrár");
        assert_eq!(i.translate("Main", "%n file(s)", "", None), "%n skrá");
    }

    #[test]
    fn a_translation_without_forms_serves_only_the_first_form() {
        let i = i18n("is");
        assert_eq!(i.translate("Main", "Single %n", "", Some(1)), "Einn 1");
        assert_eq!(i.translate("Main", "Single %n", "", Some(21)), "Einn 21");
        assert_eq!(i.translate("Main", "Single %n", "", Some(3)), "Single 3");
        assert_eq!(i.translate("Main", "Single %n", "", None), "Einn %n");
    }

    #[test]
    fn text_ids_are_looked_up_by_id() {
        assert_eq!(i18n("is").translate_id("app-title", None), "Ljós");
        assert_eq!(i18n("is").translate_id("missing-id", None), "missing-id");
    }

    #[test]
    fn a_later_catalogue_overrides_an_earlier_one_and_can_be_removed() {
        let mut i = i18n("is");
        let mut patch = Catalog::new("is");
        patch.insert("Main", "Open", "", Translation::Text("Opna skrá".into()));
        i.install(Box::new(patch));
        assert_eq!(i.translate("Main", "Open", "", None), "Opna skrá");
        assert_eq!(i.translate("Main", "Open", "adjective", None), "Opna skrá");
        assert!(i.uninstall("is"));
        assert!(!i.has_catalog());
        assert_eq!(i.translate("Main", "Open", "", None), "Open");
    }

    #[test]
    fn setting_the_same_language_or_locale_changes_nothing() {
        let mut i = I18n::new();
        assert!(!i.set_ui_language("en"));
        assert!(i.set_ui_language("ar"));
        assert!(i.is_right_to_left());
        assert!(!i.set_ui_language("ar"));
        assert!(!i.set_locale("en_US"));
        assert!(i.set_locale("is"));
        assert_eq!(i.locale().name(), "is_IS");
    }
}
