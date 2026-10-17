//! Localisation for Ljós (ARCHITECTURE.md §15).
//!
//! - [`Catalog`]: the translations of one language, keyed as Qt keys them.
//! - [`ts`]: the translator-facing `.ts` XML, read and written (feature `ts`).
//! - [`Ljl`]: the binary runtime format, read in place from mapped bytes.
//! - [`PluralForms`]: which numerus form a count selects.
//! - [`I18n`]: catalogues, UI language and locale together; what `qsTr` asks.
//! - [`arg`][]: `String.prototype.arg`.

pub mod arg;
pub mod calendar;
pub mod catalog;
pub mod collate;
pub mod datetime;
pub mod i18n;
pub mod ljl;
pub mod locale;
pub mod plural;
#[cfg(feature = "ts")]
pub mod ts;

pub use catalog::{Catalog, Entry, Messages, Translation};
pub use collate::Collator;
pub use i18n::I18n;
pub use ljl::{Ljl, LjlError};
pub use locale::{DataSizeFormat, LocaleInfo, SystemLocale};
pub use plural::PluralForms;
