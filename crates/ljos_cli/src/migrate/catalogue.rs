//! `ljos migrate --catalogue app_xx.ts`: the translations of a catalogue of
//! Qt's, written as the `.po` catalogue `xx.po` beside it, for the files
//! migrate writes as Ljós's (docs/DIALECT.md §4.13 Lowering).
//!
//! Each translation is written by the rewrite its call is
//! ([`qml_compat::rewrite_message`]), with the translation's text in place of
//! the call's, so it says each value by the name the call's message does
//! (`%1` is `{name}` where the call's `.arg` is `name`), its numerus forms
//! are the forms of a `plural`, and the message is read from it as the
//! compiler reads the call ([`ljos_compiler::i18n::message::message_in`]).
//!
//! What Qt tells apart that Ljós's message would not is kept apart:
//! - A text that files translate differently (`qsTr` looks it up in the
//!   file's context) is written with its file's name as its context
//!   ([`qml_compat::Translations::named`]).
//! - Calls that would be one message (`qsTr("Open", "menu")` and
//!   `qsTranslate("menu", "Open")`) translated two ways keep their files as
//!   Qt's, and so does a file with a translation the rewrite cannot write.
//!
//! A project migrated a few files at a time writes one `xx.po` over several
//! runs, so a run asks the whole catalogue whether a text is translated two
//! ways, not only the files it migrates, and an entry `xx.po` holds already
//! is a translation as much as one of this run's.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use ljos_compiler::i18n::message::MessageCall;
use ljos_i18n::{
    po::{PoEntry, PoFile},
    ts::{TsFile, TsMessage},
};
use qml_compat::{Lowering, Outcome, Translated, Translations};

/// A catalogue of Qt's, read.
pub(super) struct Catalogue {
    path: PathBuf,
    ts: TsFile,
    language: String,
    /// What `xx.po` holds already.
    po: PoFile,
}

/// A translation as a `.ts` file has it: its forms, or none.
type Forms = Option<Vec<String>>;

/// A `.po` entry's context and id: what makes it one message.
type Key = (Option<String>, String);

/// A call of a file written as Ljós's, with what the catalogue holds for it.
struct Call<'l> {
    key: &'l str,
    message: &'l Translated,
    /// Its `.ts` entry, if the catalogue has one.
    entry: Option<&'l TsMessage>,
}

impl Catalogue {
    pub(super) fn read(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let ts = TsFile::read(&text).map_err(|e| e.to_string())?;
        // `app_is.ts` is `is` where the file does not say.
        let language = ts
            .language
            .clone()
            .filter(|l| !l.is_empty())
            .or_else(|| language_of(path.file_stem()?.to_str()?))
            .ok_or("says no language, and its name (`app_xx.ts`) does not either")?;
        // The language names the `.po` beside the catalogue, so it may not
        // name a path: `language="../is"` would write outside it.
        if !language
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(format!("`{language}` is not a language"));
        }
        let po_path = path.with_file_name(format!("{language}.po"));
        let po = match std::fs::read_to_string(&po_path) {
            Ok(text) => PoFile::parse(&text).map_err(|e| format!("{}: {e}", po_path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => PoFile::default(),
            Err(e) => return Err(format!("{}: {e}", po_path.display())),
        };
        Ok(Self {
            path: path.to_owned(),
            ts,
            language,
            po,
        })
    }

    /// The `.po` file written beside the catalogue: `xx.po`.
    pub(super) fn po_path(&self) -> PathBuf {
        self.path.with_file_name(format!("{}.po", self.language))
    }

    /// Settles `lowering` again as the catalogue says: with the texts files
    /// translate differently in their files' contexts, then with the files
    /// whose translations would be lost kept. `shown` names each file
    /// written, by its key.
    pub(super) fn translate(
        &self,
        lowering: &mut Lowering,
        project: &dyn qml_compat::Project,
        shown: &BTreeMap<String, &PathBuf>,
    ) {
        let named = self.named(&self.calls(lowering, shown));
        if !named.is_empty() {
            lowering.translate(Translations {
                named: named.clone(),
                kept: BTreeMap::new(),
            });
            let _ = lowering.settle(project);
        }
        let kept = self.kept(lowering, shown);
        if !kept.is_empty() {
            lowering.translate(Translations { named, kept });
            let _ = lowering.settle(project);
        }
    }

    /// The calls of the files written as Ljós's.
    fn calls<'l>(
        &'l self,
        lowering: &'l Lowering,
        shown: &'l BTreeMap<String, &PathBuf>,
    ) -> Vec<Call<'l>> {
        shown
            .keys()
            .filter_map(|key| match lowering.lowering(key) {
                Some(Outcome::Lowered(lowered)) => Some((key, lowered)),
                _ => None,
            })
            .flat_map(|(key, lowered)| {
                lowered.messages.iter().map(move |message| Call {
                    key,
                    message,
                    entry: self.entry(message),
                })
            })
            .collect()
    }

    /// The `.ts` entry Qt shows for `message`.
    fn entry(&self, message: &Translated) -> Option<&TsMessage> {
        let all = || {
            self.ts
                .contexts
                .iter()
                .flat_map(|c| c.messages.iter().map(move |m| (c, m)))
        };
        if message.function == "qsTrId" {
            return all()
                .find(|(_, m)| m.id.as_deref() == Some(message.source.as_str()))
                .map(|(_, m)| m);
        }
        let context = message.qt_context.as_deref().unwrap_or_default();
        all()
            .find(|(c, m)| {
                c.name == context
                    && m.id.is_none()
                    && m.source == message.source
                    && m.comment.as_deref().unwrap_or_default() == message.disambiguation
            })
            .map(|(_, m)| m)
    }

    /// The forms Qt shows for `entry`, as `TsFile::catalog` has them: none
    /// for a message not translated, or obsolete, or short of a form the
    /// language has.
    fn forms(&self, entry: Option<&TsMessage>) -> Forms {
        let entry = entry.filter(|m| m.state.is_current())?;
        if entry.numerus {
            let count = ljos_i18n::PluralForms::for_language(&self.language).count();
            let forms = entry.translation.get(..count)?;
            (!forms.iter().any(String::is_empty)).then(|| forms.to_vec())
        } else {
            let text = entry.translation.first().filter(|t| !t.is_empty())?;
            Some(vec![text.clone()])
        }
    }

    /// The texts of `qsTr` without a disambiguation that two contexts
    /// translate differently (one not at all is different too): the
    /// contexts of the calls, and every context of the catalogue's that
    /// holds the text, for a file migrated in another run says it too.
    fn named(&self, calls: &[Call<'_>]) -> BTreeSet<String> {
        let mut seen: BTreeMap<&str, BTreeMap<Option<&str>, Forms>> = BTreeMap::new();
        for call in calls {
            let m = call.message;
            if m.function == "qsTr" && m.disambiguation.is_empty() {
                seen.entry(&m.source)
                    .or_default()
                    .insert(m.qt_context.as_deref(), self.forms(call.entry));
            }
        }
        for (text, by) in &mut seen {
            for context in &self.ts.contexts {
                let held = context.messages.iter().find(|m| {
                    m.id.is_none()
                        && m.source == *text
                        && m.comment.as_deref().unwrap_or_default().is_empty()
                        && m.state.is_current()
                });
                if let Some(m) = held {
                    by.entry(Some(context.name.as_str()))
                        .or_insert_with(|| self.forms(Some(m)));
                }
            }
        }
        seen.into_iter()
            .filter(|(_, by)| by.values().collect::<BTreeSet<_>>().len() > 1)
            .map(|(text, _)| text.to_owned())
            .collect()
    }

    /// The files written as Ljós's whose translations would be lost: where
    /// calls that are one message are translated two ways, or one way and
    /// `xx.po` another, and where a translation is not one the rewrite
    /// writes. Each with where and why.
    fn kept(
        &self,
        lowering: &Lowering,
        shown: &BTreeMap<String, &PathBuf>,
    ) -> BTreeMap<String, (usize, String)> {
        let calls = self.calls(lowering, shown);
        let place = |call: &Call<'_>| {
            let text = lowering.written(call.key).unwrap_or_default();
            let (line, column) = super::line_column(text, call.message.at);
            format!("{}:{line}:{column}", shown[call.key].display())
        };
        let mut kept: BTreeMap<String, (usize, String)> = BTreeMap::new();
        let mut keep = |call: &Call<'_>, why: String| {
            kept.entry(call.key.to_owned())
                .or_insert((call.message.at, why));
        };
        let mut by_key: BTreeMap<Key, Vec<(&Call<'_>, String)>> = BTreeMap::new();
        for call in &calls {
            match self.entry_of(call) {
                Ok(entry) => by_key
                    .entry((entry.context, entry.id))
                    .or_default()
                    .push((call, entry.translation)),
                Err(why) => keep(call, why),
            }
        }
        for ((context, id), calls) in by_key {
            let ways: BTreeSet<&str> = calls.iter().map(|(_, t)| t.as_str()).collect();
            // An empty translation in `xx.po` is one not made yet, as
            // `ljos i18n extract` writes it, so this run's may fill it. A
            // fuzzy one is not settled and an obsolete one is only kept for
            // translators to see: neither is a translation this run undoes.
            let held = self.po.entries.iter().find(|e| {
                e.context == context
                    && e.id == id
                    && !e.translation.is_empty()
                    && !e.obsolete
                    && !e.is_fuzzy()
            });
            let places: Vec<String> = calls.iter().map(|(c, _)| place(c)).collect();
            let message = match &context {
                Some(c) => format!("`{id}` in the context `{c}` is one message to `t`"),
                None => format!("`{id}` is one message to `t`"),
            };
            let why = if ways.len() > 1 {
                format!(
                    "{message}, which {} translates two ways, at {}",
                    self.path.display(),
                    places.join(" and "),
                )
            } else if let Some(held) = held.filter(|e| !ways.contains(e.translation.as_str())) {
                format!(
                    "{message}, which {} translates already as \"{}\", and {} otherwise at {}",
                    self.po_path().display(),
                    held.translation,
                    self.path.display(),
                    places.join(" and "),
                )
            } else {
                continue;
            };
            for (call, _) in calls {
                keep(call, why.clone());
            }
        }
        kept
    }

    /// The `.po` entry of `call`, or why its translation cannot be one.
    fn entry_of(&self, call: &Call<'_>) -> Result<PoEntry, String> {
        let m = call.message;
        // A `.ts` not updated since the call gained or lost its count.
        if let Some(entry) = call.entry.filter(|e| e.numerus != m.counted) {
            let (has, call_is) = if entry.numerus {
                ("numerus forms", "not counted")
            } else {
                ("one form", "counted")
            };
            return Err(format!(
                "{} translates `{}` with {has}, and the call is {call_is}",
                self.path.display(),
                entry.source,
            ));
        }
        let read = |text: &str| -> Result<ljos_compiler::i18n::message::MessageCall, String> {
            match ljos_compiler::i18n::message::message_in(text) {
                Some(Ok(message)) => Ok(message),
                Some(Err(e)) => Err(format!("`{text}` is not a message: {}", e.message)),
                None => Err(format!("`{text}` is not a message")),
            }
        };
        let context = m.context.as_deref();
        let source_text =
            qml_compat::rewrite_message(&m.call, &[], context).map_err(|e| e.to_string())?;
        let source = read(&source_text)?;
        let translation = match self.forms(call.entry) {
            None => String::new(),
            Some(forms) if m.function == "qsTrId" => ljos_i18n::message::escape(&forms[0], false),
            Some(forms) => {
                let written: Vec<(&str, &str)> = if m.counted {
                    let not = || {
                        format!(
                            "the numerus forms Qt gives `{}` are not the plural categories `t` has",
                            self.language
                        )
                    };
                    ljos_i18n::plural::categories(&self.language)
                        .ok_or_else(not)?
                        .into_iter()
                        .map(|(category, form)| Some((category, forms.get(form)?.as_str())))
                        .collect::<Option<_>>()
                        .ok_or_else(not)?
                } else {
                    vec![("other", forms[0].as_str())]
                };
                let text =
                    qml_compat::rewrite_message(&m.call, &written, context).map_err(|e| {
                        format!(
                            "its translation in {} is not one `t` says as Qt does: {e}",
                            self.path.display()
                        )
                    })?;
                if forms.iter().any(|f| f.contains(STAND_IN)) {
                    return Err(format!(
                        "its translation in {} holds `{STAND_IN}`",
                        self.path.display()
                    ));
                }
                let numbered = renumbered(&source_text, &source, &text).ok_or_else(|| {
                    format!(
                        "its translation in {} shows a value the call does not",
                        self.path.display()
                    )
                })?;
                read(&numbered)?
                    .message
                    .unwrap_or_default()
                    .replace(&format!("{{{STAND_IN}"), "{")
            }
        };
        let entry = call.entry;
        let lines = |text: Option<&String>| -> Vec<String> {
            text.map(|t| t.lines().map(str::to_owned).collect())
                .unwrap_or_default()
        };
        Ok(match source.id {
            Some(id) => PoEntry {
                id,
                translation,
                explicit_id: true,
                default: entry
                    .map(|e| e.source.clone())
                    .filter(|s| !s.is_empty())
                    .map(|s| ljos_i18n::message::escape(&s, false)),
                comments: lines(entry.and_then(|e| e.extra_comment.as_ref())),
                translator_comments: lines(entry.and_then(|e| e.translator_comment.as_ref())),
                ..PoEntry::default()
            },
            None => PoEntry {
                context: source.context,
                id: source.message.unwrap_or_default(),
                translation,
                comments: lines(entry.and_then(|e| e.extra_comment.as_ref())),
                translator_comments: lines(entry.and_then(|e| e.translator_comment.as_ref())),
                ..PoEntry::default()
            },
        })
    }

    /// The `.po` file of the calls of the files written as Ljós's, put over
    /// what `xx.po` holds already: an entry of the same context and id is
    /// replaced (`kept` kept the files that would change its translation),
    /// the others stay.
    pub(super) fn po(&self, lowering: &Lowering, shown: &BTreeMap<String, &PathBuf>) -> String {
        let mut po = self.po.clone();
        let mut written: BTreeMap<Key, PoEntry> = BTreeMap::new();
        for call in self.calls(lowering, shown) {
            // A file kept was settled out of `calls`; what is left converts.
            if let Ok(entry) = self.entry_of(&call) {
                written
                    .entry((entry.context.clone(), entry.id.clone()))
                    .or_insert(entry);
            }
        }
        po.entries
            .retain(|e| !written.contains_key(&(e.context.clone(), e.id.clone())));
        po.entries.extend(written.into_values());
        po.write(Some(&self.language))
    }
}

/// What a value the translation shows is named while it is numbered again.
const STAND_IN: &str = "__ljos_value_";

/// `translation`, a call `t` is written as, with each value `t` numbers
/// (one that is not a name: `${10}`, `${row.count}`) named for the number
/// the source gives the same value. `t` numbers values in the order a text
/// shows them, so a translation that shows them in another order would
/// otherwise number them otherwise: `%2 of %1` of `.arg(10).arg(20)` would
/// show 10 where the translator wrote 20. A value is matched by how it is
/// written, which is its value: `ljos migrate` reorders only plain values.
/// `None` where the translation shows a value the source does not.
fn renumbered(source_text: &str, source: &MessageCall, translation: &str) -> Option<String> {
    let numbers: Vec<(&str, &str)> = source
        .placeholders
        .iter()
        .filter(|p| p.name.parse::<u32>().is_ok())
        .map(|p| {
            (
                &source_text[p.span.start as usize..p.span.end as usize],
                p.name.as_str(),
            )
        })
        .collect();
    let read = ljos_compiler::i18n::message::message_in(translation)?.ok()?;
    let mut numbered = read
        .placeholders
        .iter()
        .filter(|p| p.name.parse::<u32>().is_ok())
        .collect::<Vec<_>>();
    numbered.sort_by_key(|p| p.span.start);
    // A value written twice the same way has two numbers in the source:
    // the translation's first showing takes the first, and so on, so each
    // number the source has is still there.
    let mut taken = vec![false; numbers.len()];
    let mut named = Vec::with_capacity(numbered.len());
    for placeholder in numbered {
        let span = placeholder.span.start as usize..placeholder.span.end as usize;
        let written = &translation[span.clone()];
        let at = (0..numbers.len())
            .find(|&i| !taken[i] && numbers[i].0 == written)
            .or_else(|| numbers.iter().position(|(w, _)| *w == written))?;
        taken[at] = true;
        named.push((span, numbers[at].1));
    }
    let mut out = translation.to_owned();
    for (span, number) in named.into_iter().rev() {
        out.replace_range(span, &format!("{STAND_IN}{number}"));
    }
    Some(out)
}

/// The locale a catalogue's name ends with: `app_is` is `is`, `app_pt_BR`
/// `pt_BR` and `app_zh_Hant_TW` `zh_Hant_TW`, as Qt names catalogues
/// `<app>_<language>[_<script>][_<REGION>].ts`.
fn language_of(stem: &str) -> Option<String> {
    let parts: Vec<&str> = stem.split('_').collect();
    let region = |p: &str| {
        (p.len() == 2 && p.bytes().all(|b| b.is_ascii_uppercase()))
            || (p.len() == 3 && p.bytes().all(|b| b.is_ascii_digit()))
    };
    let script = |p: &str| {
        p.len() == 4
            && p.as_bytes()[0].is_ascii_uppercase()
            && p.bytes().skip(1).all(|b| b.is_ascii_lowercase())
    };
    let language =
        |p: &str| (2..=3).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_lowercase());
    let after = parts
        .iter()
        .rev()
        .take_while(|p| region(p) || script(p))
        .count();
    let at = parts.len().checked_sub(after + 1).filter(|&at| at >= 1);
    match at {
        Some(at) if language(parts[at]) => Some(parts[at..].join("_")),
        _ if parts.len() > 1 => parts.last().map(|p| (*p).to_owned()),
        _ => None,
    }
}

/// Whether `path` is a catalogue of Qt Linguist's, which is XML, not a
/// TypeScript module of the same `.ts` name.
pub(super) fn is_linguist(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|text| {
        text.trim_start_matches('\u{feff}')
            .trim_start()
            .starts_with('<')
    })
}

#[cfg(test)]
mod tests {
    use super::language_of;

    #[test]
    fn a_catalogue_is_named_for_its_whole_locale() {
        assert_eq!(language_of("app_is").as_deref(), Some("is"));
        assert_eq!(language_of("my_app_is").as_deref(), Some("is"));
        assert_eq!(language_of("app_pt_BR").as_deref(), Some("pt_BR"));
        assert_eq!(language_of("app_zh_Hant_TW").as_deref(), Some("zh_Hant_TW"));
        assert_eq!(language_of("app_es_419").as_deref(), Some("es_419"));
        assert_eq!(language_of("app"), None);
    }
}
