//! Which numerus form of a translation a number selects.
//!
//! A `.ts` file stores the plural forms of a message as an ordered list of
//! `<numerusform>` elements, and how many there are depends on the target
//! language. A catalogue written by Qt Linguist has the forms Qt gives the
//! language, so those are the forms Ljós must count: a Hungarian `.ts` file
//! has one form per message, whatever CLDR says about Hungarian.
//!
//! The rules come from CLDR through ICU4X: a language's forms are its CLDR
//! plural categories, in CLDR's order (zero, one, two, few, many, other),
//! leaving out the categories no ordinary whole number selects. CLDR has
//! categories for fractions (Russian `other`, Czech `many`) and, in the
//! Romance languages, a `many` used only for whole millions; a `.ts` file
//! has no form for either. A count that falls into such a category uses the
//! last form. For most languages that is exactly Qt's list.
//!
//! Where it is not, [`QT_RULES`] names the rule Qt uses instead, in the
//! shapes of Qt's "Translation Rules for Plural Forms" table
//! (<https://doc.qt.io/qt-6/i18n-plural-rules.html>).
//!
//! `tests/qt_plurals.rs` compares the result with what `lupdate`,
//! `lrelease` and `qml6` do, run as black boxes, for every language code
//! either side has plural forms for.

use icu_locale::{LanguageIdentifier, Locale};
use icu_plurals::{PluralCategory, PluralRules};

use crate::locale;

/// Whole numbers below this are "ordinary": a category none of them selects
/// has no numerus form. Every CLDR rule about tens and hundreds repeats
/// within it; only the rules about whole millions lie beyond.
const ORDINARY: u32 = 1000;

/// The numerus forms of one language.
#[derive(Debug)]
pub struct PluralForms(Rule);

#[derive(Debug)]
enum Rule {
    Cldr {
        rules: PluralRules,
        forms: Vec<PluralCategory>,
    },
    Qt(QtRule),
}

/// A rule Qt uses where CLDR says something else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QtRule {
    /// One form for every number.
    One,
    /// `n == 1`, otherwise.
    Singular,
    /// `n < 2`, otherwise.
    SingularWithZero,
    /// `n == 1`, `n == 2`, otherwise.
    Dual,
    /// `n % 10 == 1 && n % 100 != 11`, `n != 0`, otherwise.
    Latvian,
    /// `n % 10 == 1`, `n % 10 == 2`, otherwise.
    Macedonian,
    /// `n == 1`, `n == 0 || n % 100` in 1 to 10, `n % 100` in 11 to 19, otherwise.
    Maltese,
    /// `n == 0`, `n == 1`, 2 to 5, `n == 6`, otherwise.
    Welsh,
}

impl QtRule {
    const fn count(self) -> usize {
        match self {
            Self::One => 1,
            Self::Singular | Self::SingularWithZero => 2,
            Self::Dual | Self::Latvian | Self::Macedonian => 3,
            Self::Maltese => 4,
            Self::Welsh => 5,
        }
    }

    const fn index(self, n: u32) -> usize {
        match self {
            Self::One => 0,
            Self::Singular => (n != 1) as usize,
            Self::SingularWithZero => (n >= 2) as usize,
            Self::Dual => match n {
                1 => 0,
                2 => 1,
                _ => 2,
            },
            Self::Latvian => {
                if n % 10 == 1 && n % 100 != 11 {
                    0
                } else if n != 0 {
                    1
                } else {
                    2
                }
            }
            Self::Macedonian => match n % 10 {
                1 => 0,
                2 => 1,
                _ => 2,
            },
            Self::Maltese => match (n, n % 100) {
                (1, _) => 0,
                (0, _) | (_, 1..=10) => 1,
                (_, 11..=19) => 2,
                _ => 3,
            },
            Self::Welsh => match n {
                0 => 0,
                1 => 1,
                2..=5 => 2,
                6 => 3,
                _ => 4,
            },
        }
    }
}

/// The languages whose Qt rule is not what CLDR gives, by language subtag.
///
/// Read off Qt's tools by `tests/qt_plurals.rs`, which fails when a row is
/// missing, wrong or no longer needed. Qt knows `ay`, `ik` and `sm` only in
/// `lupdate`, which tells how many forms they have and not which number
/// picks which; they take the documented rule with that many forms.
const QT_RULES: &[(QtRule, &[&str])] = &[
    (
        QtRule::One,
        &[
            "ak", "ast", "bho", "blo", "brx", "ceb", "chr", "csw", "cv", "doi", "dsb", "ee", "fa",
            "hsb", "ht", "hu", "kok", "lij", "om", "pcm", "sat", "sc", "scn", "syr", "tr", "vec",
        ],
    ),
    (
        QtRule::Singular,
        &[
            "aa", "ab", "am", "as", "ay", "ba", "bn", "co", "fur", "gu", "he", "hi", "kl", "km",
            "kn", "kw", "la", "lg", "ln", "lo", "mg", "nso", "oc", "pa", "qu", "rn", "rw", "si",
            "sn", "ss", "tg", "to", "ts", "vo", "wo", "yi", "zu",
        ],
    ),
    (QtRule::SingularWithZero, &["br", "fil", "wa"]),
    (
        QtRule::Dual,
        &["dv", "ga", "gv", "ik", "iu", "mi", "sa", "se", "sm"],
    ),
    (QtRule::Latvian, &["lv"]),
    (QtRule::Macedonian, &["mk"]),
    (QtRule::Maltese, &["mt"]),
    (QtRule::Welsh, &["cy"]),
];

impl PluralForms {
    /// The forms of a language named as Qt or BCP 47 name it (`is_IS`,
    /// `pt-BR`). An unknown language has one form, like Japanese.
    #[must_use]
    pub fn for_language(language: &str) -> Self {
        let Some(locale) = locale::parse(language) else {
            return Self(Rule::Qt(QtRule::One));
        };
        let subtag = locale.id.language.as_str();
        if let Some((rule, _)) = QT_RULES.iter().find(|(_, l)| l.contains(&subtag)) {
            return Self(Rule::Qt(*rule));
        }
        // Plural rules belong to a language, now and then to a region of it
        // (`pt-PT`), never to a script. Asking with one (`sr-Latn`) would
        // give the root locale's single form, because that is where CLDR
        // sends a script that is not the language's default.
        let mut id = LanguageIdentifier::from(locale.id.language);
        id.region = locale.id.region;
        let language = Locale::from(id);
        let Ok(rules) = PluralRules::try_new_cardinal((&language).into()) else {
            return Self(Rule::Qt(QtRule::One));
        };
        let mut used = [false; 6];
        for n in 0..ORDINARY {
            used[slot(rules.category_for(n))] = true;
        }
        let forms = ORDER.into_iter().filter(|c| used[slot(*c)]).collect();
        Self(Rule::Cldr { rules, forms })
    }

    /// How many `<numerusform>` elements a message in this language has.
    #[must_use]
    pub fn count(&self) -> usize {
        match &self.0 {
            Rule::Cldr { forms, .. } => forms.len(),
            Rule::Qt(rule) => rule.count(),
        }
    }

    /// The form `n` selects.
    #[must_use]
    pub fn index(&self, n: u32) -> usize {
        match &self.0 {
            Rule::Cldr { rules, forms } => {
                let category = rules.category_for(n);
                forms
                    .iter()
                    .position(|c| *c == category)
                    .unwrap_or(forms.len() - 1)
            }
            Rule::Qt(rule) => rule.index(n),
        }
    }
}

const ORDER: [PluralCategory; 6] = [
    PluralCategory::Zero,
    PluralCategory::One,
    PluralCategory::Two,
    PluralCategory::Few,
    PluralCategory::Many,
    PluralCategory::Other,
];

fn slot(c: PluralCategory) -> usize {
    match c {
        PluralCategory::Zero => 0,
        PluralCategory::One => 1,
        PluralCategory::Two => 2,
        PluralCategory::Few => 3,
        PluralCategory::Many => 4,
        PluralCategory::Other => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn indices(language: &str, numbers: &[u32]) -> Vec<usize> {
        let forms = PluralForms::for_language(language);
        numbers.iter().map(|n| forms.index(*n)).collect()
    }

    #[test]
    fn icelandic_counts_by_the_last_digit() {
        let is = PluralForms::for_language("is_IS");
        assert_eq!(is.count(), 2);
        assert_eq!(
            indices("is", &[0, 1, 2, 11, 21, 101, 111]),
            [1, 0, 1, 1, 0, 0, 1]
        );
    }

    #[test]
    fn arabic_has_six_forms() {
        assert_eq!(PluralForms::for_language("ar").count(), 6);
        assert_eq!(
            indices("ar", &[0, 1, 2, 3, 11, 100, 101, 102, 103, 1_234_567]),
            [0, 1, 2, 3, 4, 5, 5, 5, 3, 4]
        );
    }

    #[test]
    fn russian_has_no_form_for_fractions() {
        assert_eq!(PluralForms::for_language("ru").count(), 3);
        assert_eq!(
            indices("ru", &[1, 2, 5, 11, 21, 22, 25]),
            [0, 1, 2, 2, 0, 1, 2]
        );
    }

    #[test]
    fn french_millions_use_the_plural_form() {
        let fr = PluralForms::for_language("fr");
        assert_eq!(fr.count(), 2);
        assert_eq!(indices("fr", &[0, 1, 2, 1_000_000]), [0, 0, 1, 1]);
    }

    #[test]
    fn languages_without_plurals_have_one_form() {
        assert_eq!(PluralForms::for_language("ja").count(), 1);
        assert_eq!(PluralForms::for_language("").count(), 1);
        assert_eq!(indices("ja", &[0, 1, 5]), [0, 0, 0]);
    }

    #[test]
    fn english_and_latvian() {
        assert_eq!(indices("en", &[0, 1, 2]), [1, 0, 1]);
        assert_eq!(PluralForms::for_language("lv").count(), 3);
        assert_eq!(indices("lv", &[0, 1, 2, 11, 21]), [2, 0, 1, 1, 0]);
    }

    #[test]
    fn hungarian_has_the_one_form_qt_linguist_writes() {
        assert_eq!(PluralForms::for_language("hu_HU").count(), 1);
        assert_eq!(indices("hu", &[0, 1, 2]), [0, 0, 0]);
    }

    #[test]
    fn old_and_three_letter_codes_name_the_same_language() {
        assert_eq!(PluralForms::for_language("iw").count(), 2);
        assert_eq!(PluralForms::for_language("deu").count(), 2);
        assert_eq!(indices("cym", &[0, 1, 2, 6, 7]), [0, 1, 2, 3, 4]);
        assert_eq!(PluralForms::for_language("sr_Latn_RS").count(), 3);
        assert_eq!(indices("pt_PT", &[0, 1, 2]), [1, 0, 1]);
        assert_eq!(indices("pt_BR", &[0, 1, 2]), [0, 0, 1]);
    }

    #[test]
    fn no_language_has_two_rules() {
        let mut seen = std::collections::BTreeSet::new();
        for (_, languages) in QT_RULES {
            for language in *languages {
                assert!(seen.insert(*language), "{language} is listed twice");
            }
        }
    }
}
