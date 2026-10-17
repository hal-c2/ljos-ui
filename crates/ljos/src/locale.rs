//! Numbers, sizes, dates and times as a locale writes them, from Rust:
//! the same text `Qt.locale()` gives QML, for the application's locale
//! ([`Locale::of`]) or a named one ([`Locale::named`]).
//!
//! An application run through [`App`](crate::App) starts in the system's
//! locale: its name and UI language from the messages category
//! (`LANGUAGE`, `LC_ALL`, `LC_MESSAGES`, `LANG`), its numbers from
//! `LC_NUMERIC`, its dates and times from `LC_TIME` and its collation
//! from `LC_COLLATE`, each with `LC_ALL` before and `LANG` after it.
//! [`App::locale`](crate::App::locale) or [`set_locale`] chooses another,
//! and [`on_change`] follows a change.
//!
//! ```
//! use ljos::locale::{DateTime, FormatType, Locale};
//!
//! let de = Locale::named("de_DE");
//! assert_eq!(de.format_number(1234.5, 'f', 2), "1.234,50");
//! assert_eq!(de.format_data_size(1536, 2, Default::default()), "1,50 KiB");
//! let moon = DateTime::from_unix_ms(-14_159_040_000, 0);
//! assert_eq!(de.format_date(&moon, FormatType::Short), "21.07.69");
//! assert_eq!(de.format_pattern(&moon, "d. MMMM yyyy, HH:mm"), "21. Juli 1969, 02:56");
//! ```

use ljos_engine::{Engine, EngineError};
use ljos_i18n::LocaleInfo;
use ljos_i18n::datetime::Part;

pub use ljos_i18n::DataSizeFormat;
pub use ljos_i18n::SystemLocale;
/// The local fields of a date and time (`year`, `month` from 1, …, and
/// the zone's `offset` in seconds east of UTC), as a script's `Date`
/// has them. [`DateTime::from_unix_ms`] makes them from an instant.
pub use ljos_i18n::datetime::Fields as DateTime;

/// A locale's long or short format (`Locale.LongFormat`,
/// `Locale.ShortFormat`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FormatType {
    #[default]
    Long,
    Short,
}

/// How a locale writes numbers, sizes, dates and times: what
/// `Qt.locale(name)` describes.
#[derive(Debug, Clone)]
pub struct Locale(LocaleInfo);

impl Locale {
    /// The locale a name selects, as `Qt.locale(name)` does: a language
    /// alone gets its likely region (`is` is `is_IS`), and a name that is
    /// not a known language is the `C` locale.
    #[must_use]
    pub fn named(name: &str) -> Self {
        Self(LocaleInfo::new(name))
    }

    /// The application's locale (`Qt.locale()`). Tracked: a binding or an
    /// effect that asks runs again when it changes.
    #[must_use]
    pub fn of(engine: &Engine) -> Self {
        Self(engine.locale())
    }

    /// The locale this process's environment names.
    #[must_use]
    pub fn system() -> Self {
        Self(SystemLocale::from_env().locale_info())
    }

    /// `language_TERRITORY` (`de_DE`), or `C`.
    #[must_use]
    pub fn name(&self) -> &str {
        self.0.name()
    }

    /// Whether text in this locale's language runs right to left.
    #[must_use]
    pub fn is_right_to_left(&self) -> bool {
        self.0.is_right_to_left()
    }

    /// `Locale.uiLanguages`: the tags a translation may be named by, most
    /// specific first.
    #[must_use]
    pub fn ui_languages(&self) -> Vec<String> {
        self.0.ui_languages()
    }

    #[must_use]
    pub fn decimal_point(&self) -> String {
        self.0.decimal_point()
    }

    #[must_use]
    pub fn group_separator(&self) -> String {
        self.0.group_separator()
    }

    #[must_use]
    pub fn zero_digit(&self) -> String {
        self.0.zero_digit()
    }

    #[must_use]
    pub fn negative_sign(&self) -> String {
        self.0.negative_sign()
    }

    /// `Number.toLocaleString(locale, format, precision)`: C's `printf`
    /// conversion `format` (`f`, `e`, `g` and their capitals) with
    /// `precision`, in the locale's digits and separators.
    #[must_use]
    pub fn format_number(&self, value: f64, format: char, precision: usize) -> String {
        self.0.format_double(value, format, precision.min(99))
    }

    /// An integer in the locale's digits and grouping.
    #[must_use]
    pub fn format_integer(&self, n: i64) -> String {
        self.0.format_integer(n)
    }

    /// `Locale.formattedDataSize(bytes, precision, format)`: `1,50 KiB`.
    /// A negative `precision` is six decimals.
    #[must_use]
    pub fn format_data_size(&self, bytes: i64, precision: i32, format: DataSizeFormat) -> String {
        self.0.format_data_size(bytes, precision, format)
    }

    /// `Locale.dateFormat(format)`: the format string a date is written with.
    #[must_use]
    pub fn date_format(&self, format: FormatType) -> String {
        self.0.date_format(format == FormatType::Long)
    }

    /// `Locale.timeFormat(format)`.
    #[must_use]
    pub fn time_format(&self, format: FormatType) -> String {
        self.0.time_format(format == FormatType::Long)
    }

    /// `Locale.dateTimeFormat(format)`.
    #[must_use]
    pub fn date_time_format(&self, format: FormatType) -> String {
        self.0.date_time_format(format == FormatType::Long)
    }

    /// `date.toLocaleDateString(locale, format)`.
    #[must_use]
    pub fn format_date(&self, date: &DateTime, format: FormatType) -> String {
        self.0
            .format_date_time(&self.date_format(format), date, Part::Date)
    }

    /// `date.toLocaleTimeString(locale, format)`.
    #[must_use]
    pub fn format_time(&self, date: &DateTime, format: FormatType) -> String {
        self.0
            .format_date_time(&self.time_format(format), date, Part::Time)
    }

    /// `date.toLocaleString(locale, format)`.
    #[must_use]
    pub fn format_date_time(&self, date: &DateTime, format: FormatType) -> String {
        self.0
            .format_date_time(&self.date_time_format(format), date, Part::DateTime)
    }

    /// `date.toLocaleString(locale, pattern)`: a Qt format string
    /// (`dd.MM.yyyy HH:mm`) in this locale's names and digits.
    #[must_use]
    pub fn format_pattern(&self, date: &DateTime, pattern: &str) -> String {
        self.0.format_date_time(pattern, date, Part::DateTime)
    }
}

/// Sets the application's locale (`de_DE`, `is`): what `Qt.locale()`
/// describes from then on. Every binding that wrote something by it
/// runs again; the UI language stays as it was.
pub fn set_locale(engine: &Engine, name: &str) {
    engine.set_locale(name);
}

/// Takes the application's locale and UI language from this process's
/// environment, as [`App`](crate::App) does when it starts.
pub fn use_system_locale(engine: &Engine) {
    engine.use_system_locale();
}

/// What [`on_change`] returns: the subscription, until it is stopped.
#[derive(Debug)]
#[must_use = "a subscription runs until it is stopped"]
pub struct LocaleWatch(ljos_engine::NodeId);

impl LocaleWatch {
    /// Ends the subscription.
    pub fn stop(self, engine: &Engine) {
        engine.stop_effect(self.0);
    }
}

/// Calls `f` with the application's new locale whenever it changes (not
/// now): by [`set_locale`], by QML, or by the system's.
pub fn on_change(
    engine: &Engine,
    f: impl Fn(&Engine, &Locale) + 'static,
) -> Result<LocaleWatch, EngineError> {
    engine
        .on_locale_changed(move |e, info| f(e, &Locale(info.clone())))
        .map(LocaleWatch)
}
