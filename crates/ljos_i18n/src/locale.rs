//! Locale names and the few locale facts M1 needs: the Qt-style name, the
//! number symbols, the text direction and the UI language list.
//!
//! Everything here comes from ICU4X (CLDR), so a value can differ from Qt's
//! where Qt's own locale data differs from CLDR's.

use icu_decimal::{
    DecimalFormatter,
    input::Decimal,
    options::{DecimalFormatterOptions, GroupingStrategy},
};
use icu_locale::{
    LanguageIdentifier, Locale, LocaleCanonicalizer, LocaleDirectionality, LocaleExpander,
};

/// Parses what Qt and POSIX call a locale: `is`, `is_IS`, `is-IS`,
/// `is_IS.UTF-8`, `sr_RS@latin`. `None` for `C`, `POSIX`, the empty string
/// and anything that is not a language tag.
#[must_use]
pub fn parse(name: &str) -> Option<Locale> {
    let name = name.split(['.', '@']).next().unwrap_or_default().trim();
    if name.is_empty() || name.eq_ignore_ascii_case("C") || name.eq_ignore_ascii_case("POSIX") {
        return None;
    }
    let tag = name.replace('_', "-");
    let mut locale = Locale::try_from_str(&tag).ok()?;
    // Old and three-letter codes name the same languages (`iw` is Hebrew,
    // `deu` is German); CLDR data is keyed by the current code only.
    LocaleCanonicalizer::new_extended().canonicalize(&mut locale);
    (!locale.id.language.is_unknown()).then_some(locale)
}

/// `is-Latn-IS` for `is`: the tag with the script and region CLDR calls
/// likely. `None` when CLDR does not know the language.
#[must_use]
pub fn maximized(locale: &Locale) -> Option<LanguageIdentifier> {
    let mut id = locale.id.clone();
    // The transform reports `Unmodified` both for a complete tag and for an
    // unknown language, so an unknown language is told by what is missing.
    LocaleExpander::new_extended().maximize(&mut id);
    (id.script.is_some() && id.region.is_some()).then_some(id)
}

/// The Qt-style name (`is_IS`) of a language tag, with the likely region
/// filled in. `None` when the language is unknown to CLDR.
#[must_use]
pub fn qt_name(locale: &Locale) -> Option<String> {
    let id = maximized(locale)?;
    Some(format!("{}_{}", id.language.as_str(), id.region?.as_str()))
}

/// Whether text in this language is written right to left.
#[must_use]
pub fn is_right_to_left(language: &str) -> bool {
    parse(language).is_some_and(|l| LocaleDirectionality::new_extended().is_right_to_left(&l.id))
}

/// The language of the process environment, as POSIX resolves it:
/// `LC_ALL`, then `LC_MESSAGES`, then `LANG`. A host calls this once at
/// start-up to seed the UI language; `None` means the `C` locale.
#[must_use]
pub fn system_locale_name() -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|v| !v.is_empty())
        .and_then(|v| parse(&v).and_then(|l| qt_name(&l)))
}

/// What `Qt.locale(name)` describes.
#[derive(Debug, Clone)]
pub struct LocaleInfo {
    /// `en_US`, or `C`.
    name: String,
    /// `None` is the `C` locale: ASCII digits, `.` and no grouping.
    locale: Option<Locale>,
    grouped: Option<DecimalFormatter>,
    ungrouped: Option<DecimalFormatter>,
}

impl Default for LocaleInfo {
    fn default() -> Self {
        Self::c()
    }
}

impl LocaleInfo {
    /// The locale a name selects, as `Qt.locale(name)` does: a language
    /// alone gets its likely region (`is` is `is_IS`), and a name that is
    /// not a known language is the `C` locale.
    #[must_use]
    pub fn new(name: &str) -> Self {
        let Some(locale) = parse(name) else {
            return Self::c();
        };
        let Some(name) = qt_name(&locale) else {
            return Self::c();
        };
        let formatter = |grouping| {
            let mut options = DecimalFormatterOptions::default();
            options.grouping_strategy = Some(grouping);
            DecimalFormatter::try_new((&locale).into(), options).ok()
        };
        Self {
            name,
            grouped: formatter(GroupingStrategy::Auto),
            ungrouped: formatter(GroupingStrategy::Never),
            locale: Some(locale),
        }
    }

    /// The `C` locale.
    #[must_use]
    pub fn c() -> Self {
        Self {
            name: "C".to_owned(),
            locale: None,
            grouped: None,
            ungrouped: None,
        }
    }

    /// `language_TERRITORY`, or `C`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn is_right_to_left(&self) -> bool {
        self.locale
            .as_ref()
            .is_some_and(|l| LocaleDirectionality::new_extended().is_right_to_left(&l.id))
    }

    /// The tags a catalogue for this locale may be named by, most specific
    /// first: `de-Latn-DE`, `de-DE`, `de-Latn`, `de`.
    #[must_use]
    pub fn ui_languages(&self) -> Vec<String> {
        let Some(id) = self.locale.as_ref().and_then(maximized) else {
            return vec!["C".to_owned()];
        };
        let language = id.language.as_str();
        let script = id.script.map(|s| s.as_str().to_owned()).unwrap_or_default();
        let region = id.region.map(|r| r.as_str().to_owned()).unwrap_or_default();
        vec![
            format!("{language}-{script}-{region}"),
            format!("{language}-{region}"),
            format!("{language}-{script}"),
            language.to_owned(),
        ]
    }

    /// Formats a number written in ASCII (`-1234.5`, `1.23457e+06`) the way
    /// the locale writes it: its digits, decimal separator, minus sign and,
    /// for a number without an exponent, its grouping.
    #[must_use]
    pub fn localize_number(&self, ascii: &str) -> String {
        let Some(grouped) = &self.grouped else {
            return ascii.to_owned();
        };
        let plain = |f: &DecimalFormatter, text: &str| {
            Decimal::try_from_str(text)
                .ok()
                .map(|d| f.format(&d).to_string())
        };
        let out = match ascii.split_once(['e', 'E']) {
            None => plain(grouped, ascii),
            Some((mantissa, exponent)) => {
                let ungrouped = self.ungrouped.as_ref().unwrap_or(grouped);
                let (sign, digits) = match exponent.strip_prefix('-') {
                    Some(d) => ("-", d),
                    None => ("+", exponent.strip_prefix('+').unwrap_or(exponent)),
                };
                // The exponent keeps its leading zero (`e+06`), which a
                // number formatter would drop, so its digits are mapped
                // one by one.
                let digits: Option<String> = digits
                    .chars()
                    .map(|c| plain(ungrouped, &c.to_string()))
                    .collect();
                plain(ungrouped, mantissa)
                    .zip(digits)
                    .map(|(m, d)| format!("{m}e{sign}{d}"))
            }
        };
        // Not a number (`nan`, `inf`): Qt writes those as they are.
        out.unwrap_or_else(|| ascii.to_owned())
    }

    /// An integer with the locale's digits and grouping.
    #[must_use]
    pub fn format_integer(&self, n: i64) -> String {
        self.localize_number(&n.to_string())
    }

    /// The digit zero: `0`, or `٠` in a locale that counts in Arabic-Indic digits.
    #[must_use]
    pub fn zero_digit(&self) -> String {
        self.format_plain("0")
    }

    /// The separator between the whole and the fractional part.
    #[must_use]
    pub fn decimal_point(&self) -> String {
        // "1.5" is a digit, the separator and a digit, whatever the digits are.
        let text = self.format_plain("1.5");
        let mut chars = text.chars();
        chars.next();
        chars.next_back();
        let sep = chars.as_str();
        if sep.is_empty() {
            ".".to_owned()
        } else {
            sep.to_owned()
        }
    }

    /// The separator between groups of digits; empty when the locale has none.
    #[must_use]
    pub fn group_separator(&self) -> String {
        let Some(locale) = &self.locale else {
            return ",".to_owned();
        };
        let mut options = DecimalFormatterOptions::default();
        options.grouping_strategy = Some(GroupingStrategy::Always);
        let Ok(always) = DecimalFormatter::try_new(locale.into(), options) else {
            return ",".to_owned();
        };
        let Ok(million) = Decimal::try_from_str("1111111") else {
            return ",".to_owned();
        };
        let text = always.format(&million).to_string();
        let one = text.chars().next().unwrap_or('1');
        // Everything between the first two runs of the digit is one separator.
        text.trim_start_matches(one)
            .chars()
            .take_while(|c| *c != one)
            .collect()
    }

    /// The sign written before a negative number.
    #[must_use]
    pub fn negative_sign(&self) -> String {
        let text = self.format_plain("-1");
        let one = self.format_plain("1");
        text.strip_suffix(one.as_str())
            .or_else(|| text.strip_prefix(one.as_str()))
            .filter(|s| !s.is_empty())
            .map_or_else(|| "-".to_owned(), ToOwned::to_owned)
    }

    fn format_plain(&self, ascii: &str) -> String {
        match (&self.ungrouped, Decimal::try_from_str(ascii)) {
            (Some(f), Ok(d)) => f.format(&d).to_string(),
            _ => ascii.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_language_alone_gets_its_likely_region() {
        assert_eq!(LocaleInfo::new("is").name(), "is_IS");
        assert_eq!(LocaleInfo::new("ar").name(), "ar_EG");
        assert_eq!(LocaleInfo::new("zh").name(), "zh_CN");
        assert_eq!(LocaleInfo::new("en-GB").name(), "en_GB");
        assert_eq!(LocaleInfo::new("en_US.UTF-8").name(), "en_US");
    }

    #[test]
    fn what_is_not_a_language_is_the_c_locale() {
        for name in ["", "C", "POSIX", "xx", "not a locale"] {
            assert_eq!(LocaleInfo::new(name).name(), "C", "{name:?}");
        }
    }

    #[test]
    fn number_symbols_follow_the_locale() {
        let is = LocaleInfo::new("is_IS");
        assert_eq!(is.decimal_point(), ",");
        assert_eq!(is.group_separator(), ".");
        let en = LocaleInfo::new("en_US");
        assert_eq!(en.decimal_point(), ".");
        assert_eq!(en.group_separator(), ",");
        assert_eq!(en.zero_digit(), "0");
        assert_eq!(en.negative_sign(), "-");
        let c = LocaleInfo::c();
        assert_eq!(c.decimal_point(), ".");
        assert_eq!(c.zero_digit(), "0");
    }

    #[test]
    fn numbers_are_grouped_unless_they_have_an_exponent() {
        let en = LocaleInfo::new("en_US");
        assert_eq!(en.localize_number("1234567"), "1,234,567");
        assert_eq!(en.localize_number("1234.5"), "1,234.5");
        assert_eq!(en.localize_number("1.23457e+06"), "1.23457e+06");
        assert_eq!(en.localize_number("nan"), "nan");
        let de = LocaleInfo::new("de");
        assert_eq!(de.localize_number("1234567.5"), "1.234.567,5");
        assert_eq!(de.localize_number("1.5e-07"), "1,5e-07");
        assert_eq!(LocaleInfo::c().localize_number("1234567"), "1234567");
    }

    #[test]
    fn arabic_is_written_right_to_left_in_its_own_digits() {
        let ar = LocaleInfo::new("ar_EG");
        assert!(ar.is_right_to_left());
        assert_eq!(ar.zero_digit(), "٠");
        assert!(!LocaleInfo::new("is").is_right_to_left());
        assert!(is_right_to_left("he"));
        assert!(!is_right_to_left("xx"));
    }

    #[test]
    fn ui_languages_run_from_specific_to_general() {
        assert_eq!(
            LocaleInfo::new("de").ui_languages(),
            ["de-Latn-DE", "de-DE", "de-Latn", "de"]
        );
        assert_eq!(LocaleInfo::c().ui_languages(), ["C"]);
    }
}
