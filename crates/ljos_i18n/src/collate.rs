//! Text in the order a locale's alphabet puts it: letters first, then
//! accents, then case, so `a ä b B z` in English and `a b B z ä` in
//! Swedish. What `String.prototype.localeCompare` and `QCollator` do.
//!
//! The `C` locale has no alphabet: its texts are ordered by their UTF-16
//! code units, as a script's strings hold them. So is every locale's
//! without the `collation` feature, which carries the alphabets.

use std::cmp::Ordering;

#[cfg(feature = "collation")]
use icu_collator::options::{CollatorOptions, Strength};
#[cfg(feature = "collation")]
use icu_collator::{CollatorBorrowed, CollatorPreferences};
use icu_locale::Locale;

#[cfg(feature = "collation")]
type Icu = CollatorBorrowed<'static>;
#[cfg(not(feature = "collation"))]
type Icu = std::convert::Infallible;

/// Compares texts as a locale orders them.
#[derive(Debug)]
pub struct Collator {
    locale: Option<Locale>,
    case_sensitive: bool,
    icu: Option<Icu>,
}

impl Collator {
    /// A collator for `locale` (`None` is the `C` locale) that tells a
    /// capital from its small letter.
    #[must_use]
    pub fn new(locale: Option<&Locale>) -> Self {
        Self::build(locale.cloned(), true)
    }

    fn build(locale: Option<Locale>, case_sensitive: bool) -> Self {
        Self {
            icu: Self::icu(locale.as_ref(), case_sensitive),
            locale,
            case_sensitive,
        }
    }

    #[cfg(feature = "collation")]
    fn icu(locale: Option<&Locale>, case_sensitive: bool) -> Option<Icu> {
        locale.and_then(|l| {
            let mut options = CollatorOptions::default();
            // The tertiary level is the one that tells case apart; QCollator
            // drops it for a case-insensitive comparison.
            if !case_sensitive {
                options.strength = Some(Strength::Secondary);
            }
            CollatorBorrowed::try_new(CollatorPreferences::from(l), options).ok()
        })
    }

    #[cfg(not(feature = "collation"))]
    fn icu(_: Option<&Locale>, _: bool) -> Option<Icu> {
        None
    }

    /// The same collator, telling case apart or not.
    #[must_use]
    pub fn with_case_sensitive(self, case_sensitive: bool) -> Self {
        if case_sensitive == self.case_sensitive {
            return self;
        }
        Self::build(self.locale, case_sensitive)
    }

    #[must_use]
    pub fn is_case_sensitive(&self) -> bool {
        self.case_sensitive
    }

    /// The locale this collator orders by; `None` is the `C` locale.
    #[must_use]
    pub fn locale(&self) -> Option<&Locale> {
        self.locale.as_ref()
    }

    /// Where `a` goes against `b`.
    #[must_use]
    pub fn compare(&self, a: &str, b: &str) -> Ordering {
        self.order(a, b).cmp(&0)
    }

    /// What `a.localeCompare(b)` returns: -1, 0 or 1 by a locale, and in
    /// the `C` locale the difference of the first code units that differ
    /// (or of the lengths), as a C `strcmp` does.
    #[must_use]
    pub fn order(&self, a: &str, b: &str) -> i32 {
        match &self.icu {
            #[cfg(feature = "collation")]
            Some(icu) => icu.compare(a, b) as i32,
            #[cfg(not(feature = "collation"))]
            Some(never) => match *never {},
            None => self.code_units(a, b),
        }
    }

    fn code_units(&self, a: &str, b: &str) -> i32 {
        if self.case_sensitive {
            units(a.encode_utf16(), b.encode_utf16())
        } else {
            let fold = |s: &str| s.chars().flat_map(char::to_lowercase).collect::<String>();
            let (a, b) = (fold(a), fold(b));
            units(a.encode_utf16(), b.encode_utf16())
        }
    }
}

fn units(mut a: impl Iterator<Item = u16>, mut b: impl Iterator<Item = u16>) -> i32 {
    let rest = |n: usize| i32::try_from(n).unwrap_or(i32::MAX);
    loop {
        match (a.next(), b.next()) {
            (Some(x), Some(y)) if x == y => {}
            (Some(x), Some(y)) => return i32::from(x) - i32::from(y),
            // One is the start of the other: the difference of the lengths.
            (Some(_), None) => return 1 + rest(a.count()),
            (None, Some(_)) => return -1 - rest(b.count()),
            (None, None) => return 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locale::parse;

    fn sorted(locale: &str, words: &[&str]) -> String {
        let c = Collator::new(parse(locale).as_ref());
        let mut v = words.to_vec();
        v.sort_by(|a, b| c.compare(a, b));
        v.join(" ")
    }

    #[cfg(feature = "collation")]
    #[test]
    fn a_locale_orders_by_its_alphabet() {
        let words = ["b", "a", "B", "ä", "z"];
        assert_eq!(sorted("en_US", &words), "a ä b B z");
        assert_eq!(sorted("sv_SE", &words), "a b B z ä");
        let nordic = ["ö", "o", "z", "å", "a", "ä"];
        assert_eq!(sorted("en_US", &nordic), "a å ä o ö z");
        assert_eq!(sorted("sv", &nordic), "a o z å ä ö");
    }

    #[cfg(feature = "collation")]
    #[test]
    fn a_comparison_is_a_sign_by_a_locale() {
        let en = Collator::new(parse("en_US").as_ref());
        assert_eq!(en.order("a", "b"), -1);
        assert_eq!(en.order("a", "a"), 0);
        assert_eq!(en.order("a", "A"), -1);
        assert_eq!(en.order("zz", "a"), 1);
        let en = en.with_case_sensitive(false);
        assert_eq!(en.compare("Apple", "apple"), Ordering::Equal);
        assert_eq!(en.compare("apple", "Banana"), Ordering::Less);
        assert_eq!(en.compare("e", "é"), Ordering::Less);
    }

    #[test]
    fn a_comparison_in_c_is_a_difference_of_code_units() {
        assert_eq!(sorted("C", &["b", "a", "B", "ä", "z"]), "B a b z ä");
        let c = Collator::new(None);
        assert_eq!(c.order("a", "A"), 32);
        assert_eq!(c.order("zz", "a"), 25);
        assert_eq!(c.order("e", "é"), -132);
        assert_eq!(c.order("", "a"), -1);
        assert_eq!(c.order("ab", "a"), 1);
        assert_eq!(c.order("a", "abc"), -2);
    }

    #[test]
    fn case_can_be_ignored_in_c() {
        let c = Collator::new(None).with_case_sensitive(false);
        assert_eq!(c.compare("ABC", "abc"), Ordering::Equal);
        assert!(!c.is_case_sensitive());
    }
}
