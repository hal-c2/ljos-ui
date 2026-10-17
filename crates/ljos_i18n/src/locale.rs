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
/// `LC_ALL`, then `LC_MESSAGES`, then `LANG`. `None` means the `C`
/// locale. [`SystemLocale`] has every category.
#[must_use]
pub fn system_locale_name() -> Option<String> {
    let system = SystemLocale::from_env();
    parse(&system.messages).and_then(|l| qt_name(&l))
}

/// What the process environment names for each locale category, as
/// POSIX resolves them: `LC_ALL`, then the category's own variable, then
/// `LANG`. Qt's system locale takes its name and UI languages from the
/// messages category (with `LANGUAGE` before it, as gettext reads it),
/// its numbers from `LC_NUMERIC`, its dates and times from `LC_TIME` and
/// its collation from `LC_COLLATE`. An empty value is the `C` locale.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemLocale {
    pub messages: String,
    pub numeric: String,
    pub time: String,
    pub collate: String,
}

impl SystemLocale {
    /// The categories of this process's environment.
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_vars(|k| std::env::var(k).ok())
    }

    /// The categories an environment names, `var` reading its variables.
    #[must_use]
    pub fn from_vars(var: impl Fn(&str) -> Option<String>) -> Self {
        let set = |k: &str| var(k).filter(|v| !v.is_empty());
        let category = |own: &str| {
            set("LC_ALL")
                .or_else(|| set(own))
                .or_else(|| set("LANG"))
                .unwrap_or_default()
        };
        let mut messages = category("LC_MESSAGES");
        // `LANGUAGE` lists the languages messages are wanted in, but only
        // when messages are not in the `C` locale.
        if parse(&messages).is_some()
            && let Some(first) = set("LANGUAGE").and_then(|l| {
                l.split(':')
                    .find(|n| parse(n).is_some())
                    .map(ToOwned::to_owned)
            })
        {
            messages = first;
        }
        Self {
            messages,
            numeric: category("LC_NUMERIC"),
            time: category("LC_TIME"),
            collate: category("LC_COLLATE"),
        }
    }

    /// The application's locale for these categories: named by the
    /// messages category, writing numbers, dates and times and comparing
    /// text by the others.
    #[must_use]
    pub fn locale_info(&self) -> LocaleInfo {
        LocaleInfo::mixed(&self.messages, &self.numeric, &self.time, &self.collate)
    }

    /// `Qt.uiLanguage` for these categories: the messages locale's
    /// shortest tag (`de` for `de_DE`, `pt-PT` for `pt_PT`). `None` for
    /// the `C` locale.
    #[must_use]
    pub fn ui_language(&self) -> Option<String> {
        let locale = parse(&self.messages)?;
        let mut id = maximized(&locale)?;
        LocaleExpander::new_extended().minimize(&mut id);
        Some(id.to_string())
    }
}

/// The units `Locale.formattedDataSize` counts in, numbered as
/// `Locale.DataSizeFormat` numbers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DataSizeFormat {
    /// Powers of 1024, called KiB, MiB, …
    #[default]
    Iec,
    /// Powers of 1024, called kB, MB, …
    Traditional,
    /// Powers of 1000, called kB, MB, …
    Si,
}

impl DataSizeFormat {
    /// The format `Locale.DataSizeFormat`'s value names; IEC for any
    /// other.
    #[must_use]
    pub fn from_qt(value: i32) -> Self {
        match value {
            2 => Self::Traditional,
            3 => Self::Si,
            _ => Self::Iec,
        }
    }
}

/// What `Qt.locale(name)` describes.
#[derive(Debug, Clone)]
pub struct LocaleInfo {
    /// `en_US`, or `C`.
    name: String,
    /// What the locale is named by, and its text direction and UI
    /// languages. `None` is the `C` locale.
    locale: Option<Locale>,
    /// What numbers are written by. `None` is the `C` locale: ASCII
    /// digits, `.` and no grouping.
    numeric: Option<Locale>,
    /// What dates and times are written by; `None` writes them as `C`.
    time: Option<Locale>,
    /// What text is compared by; `None` compares code points.
    collate: Option<Locale>,
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
        Self::mixed(name, name, name, name)
    }

    /// A locale whose categories differ, as a system's can: named by
    /// `messages`, writing numbers by `numeric` and dates and times by
    /// `time`, comparing text by `collate`. Each is read as
    /// [`new`](Self::new) reads a name.
    #[must_use]
    pub fn mixed(messages: &str, numeric: &str, time: &str, collate: &str) -> Self {
        let known = |name: &str| parse(name).filter(|l| qt_name(l).is_some());
        let locale = known(messages);
        let numeric = known(numeric);
        let formatter = |grouping| {
            let locale = numeric.as_ref()?;
            let mut options = DecimalFormatterOptions::default();
            options.grouping_strategy = Some(grouping);
            DecimalFormatter::try_new(locale.into(), options).ok()
        };
        Self {
            name: locale
                .as_ref()
                .and_then(qt_name)
                .unwrap_or_else(|| "C".to_owned()),
            grouped: formatter(GroupingStrategy::Auto),
            ungrouped: formatter(GroupingStrategy::Never),
            locale,
            numeric,
            time: known(time),
            collate: known(collate),
        }
    }

    /// Whether `other` names, writes and compares as this locale does.
    #[must_use]
    pub fn same_as(&self, other: &Self) -> bool {
        self.name == other.name
            && self.numeric == other.numeric
            && self.time == other.time
            && self.collate == other.collate
    }

    /// The `C` locale.
    #[must_use]
    pub fn c() -> Self {
        Self {
            name: "C".to_owned(),
            locale: None,
            numeric: None,
            time: None,
            collate: None,
            grouped: None,
            ungrouped: None,
        }
    }

    /// The locale dates and times are written by; `None` is the `C` locale.
    #[must_use]
    pub(crate) fn icu(&self) -> Option<&Locale> {
        self.time.as_ref()
    }

    /// The locale text is compared by; `None` is the `C` locale.
    #[must_use]
    pub fn collation_locale(&self) -> Option<&Locale> {
        self.collate.as_ref()
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

    /// `Number.toLocaleString(locale, format, precision)`: C's `printf`
    /// conversion in the locale's digits and separators, the exponent's
    /// letter in the case `format` has.
    #[must_use]
    pub fn format_double(&self, value: f64, format: char, precision: usize) -> String {
        let text = self.localize_number(&crate::arg::printf(value, format, precision));
        if format.is_ascii_uppercase() {
            text.to_uppercase()
        } else {
            text
        }
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
        let Some(locale) = &self.numeric else {
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

    /// `Locale.formattedDataSize(bytes, precision, format)`: a count of
    /// bytes in the largest unit it makes at least one of, with
    /// `precision` decimals (a negative one is six), and under one such
    /// unit as a whole number of bytes. The units' names are English
    /// whatever the locale; the number is the locale's.
    #[must_use]
    pub fn format_data_size(&self, bytes: i64, precision: i32, format: DataSizeFormat) -> String {
        let (base, units): (u64, [&str; 6]) = match format {
            DataSizeFormat::Iec => (1024, ["KiB", "MiB", "GiB", "TiB", "PiB", "EiB"]),
            DataSizeFormat::Traditional => (1024, ["kB", "MB", "GB", "TB", "PB", "EB"]),
            DataSizeFormat::Si => (1000, ["kB", "MB", "GB", "TB", "PB", "EB"]),
        };
        let size = bytes.unsigned_abs();
        let mut power = 0;
        let mut scale = 1u64;
        while power < units.len() && size / scale >= base {
            scale *= base;
            power += 1;
        }
        if power == 0 {
            return format!("{} bytes", self.format_integer(bytes));
        }
        let precision = usize::try_from(precision).unwrap_or(6);
        let value = bytes as f64 / scale as f64;
        format!(
            "{} {}",
            self.format_double(value, 'f', precision.min(99)),
            units[power - 1]
        )
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
    fn a_system_locale_takes_each_category_from_its_own_variable() {
        let env = |vars: &'static [(&'static str, &'static str)]| {
            SystemLocale::from_vars(move |k| {
                vars.iter()
                    .find(|(n, _)| *n == k)
                    .map(|(_, v)| (*v).to_owned())
            })
        };
        let s = env(&[("LANG", "en_US.UTF-8"), ("LC_NUMERIC", "de_DE.UTF-8")]);
        assert_eq!(s.messages, "en_US.UTF-8");
        assert_eq!(s.numeric, "de_DE.UTF-8");
        let info = s.locale_info();
        assert_eq!(info.name(), "en_US");
        assert_eq!(info.format_double(1234.5, 'f', 2), "1.234,50");
        assert!(info.time_format(false).starts_with("h:mm"));
        let s = env(&[
            ("LANG", "en_US.UTF-8"),
            ("LC_TIME", "de_DE"),
            ("LC_ALL", ""),
        ]);
        let info = s.locale_info();
        assert_eq!(info.format_double(1234.5, 'f', 2), "1,234.50");
        assert_eq!(info.date_format(false), "dd.MM.yy");
        let s = env(&[
            ("LANG", "en_US"),
            ("LC_ALL", "fr_FR"),
            ("LC_NUMERIC", "sv_SE"),
        ]);
        assert_eq!(s.locale_info().name(), "fr_FR");
        assert_eq!(
            env(&[("LANG", "en_US"), ("LC_MESSAGES", "de_DE")])
                .ui_language()
                .as_deref(),
            Some("de")
        );
        assert_eq!(
            env(&[("LANG", "en_US"), ("LANGUAGE", "de:sv")])
                .ui_language()
                .as_deref(),
            Some("de")
        );
        assert_eq!(
            env(&[("LC_ALL", "C"), ("LANGUAGE", "de")]).ui_language(),
            None
        );
        assert_eq!(env(&[]).locale_info().name(), "C");
    }

    #[test]
    fn the_ui_language_is_the_shortest_tag() {
        let ui = |lang: &str| {
            SystemLocale {
                messages: lang.to_owned(),
                ..SystemLocale::default()
            }
            .ui_language()
        };
        assert_eq!(ui("pt_BR.UTF-8").as_deref(), Some("pt"));
        assert_eq!(ui("pt_PT.UTF-8").as_deref(), Some("pt-PT"));
        assert_eq!(ui("en_GB").as_deref(), Some("en-GB"));
        assert_eq!(ui("zh_TW").as_deref(), Some("zh-TW"));
        assert_eq!(ui("de_DE").as_deref(), Some("de"));
    }

    #[test]
    fn data_sizes_are_counted_in_the_largest_unit_there_is_one_of() {
        let en = LocaleInfo::new("en_US");
        let iec = |b| en.format_data_size(b, 2, DataSizeFormat::Iec);
        assert_eq!(iec(0), "0 bytes");
        assert_eq!(iec(1023), "1,023 bytes");
        assert_eq!(iec(1024), "1.00 KiB");
        assert_eq!(iec(1_234_567), "1.18 MiB");
        assert_eq!(iec(1_000_000_000_000), "931.32 GiB");
        assert_eq!(iec(1_500_000_000_000_000_000), "1.30 EiB");
        assert_eq!(iec(-2048), "-2.00 KiB");
        assert_eq!(en.format_data_size(1000, 1, DataSizeFormat::Si), "1.0 kB");
        assert_eq!(
            en.format_data_size(1536, 0, DataSizeFormat::Traditional),
            "2 kB"
        );
        assert_eq!(
            en.format_data_size(1536, -1, DataSizeFormat::Iec),
            "1.500000 KiB"
        );
        let de = LocaleInfo::new("de_DE");
        assert_eq!(
            de.format_data_size(1000, 2, DataSizeFormat::Iec),
            "1.000 bytes"
        );
        assert_eq!(
            de.format_data_size(1536, 2, DataSizeFormat::Iec),
            "1,50 KiB"
        );
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
