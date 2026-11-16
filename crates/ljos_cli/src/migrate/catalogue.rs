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

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

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
}

/// A translation as a `.ts` file has it: its forms, or none.
type Forms = Option<Vec<String>>;

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
            .or_else(|| {
                let stem = path.file_stem()?.to_str()?;
                Some(stem.rsplit_once('_')?.1.to_owned())
            })
            .ok_or("says no language, and its name (`app_xx.ts`) does not either")?;
        Ok(Self {
            path: path.to_owned(),
            ts,
            language,
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
    /// translate differently (one not at all is different too).
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
        seen.into_iter()
            .filter(|(_, by)| by.values().collect::<BTreeSet<_>>().len() > 1)
            .map(|(text, _)| text.to_owned())
            .collect()
    }

    /// The files written as Ljós's whose translations would be lost: where
    /// calls that are one message are translated two ways, and where a
    /// translation is not one the rewrite writes. Each with where and why.
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
        let mut by_key: BTreeMap<(Option<String>, String), Vec<&Call<'_>>> = BTreeMap::new();
        for call in &calls {
            match self.entry_of(call) {
                Ok(entry) => by_key
                    .entry((entry.context, entry.id))
                    .or_default()
                    .push(call),
                Err(why) => keep(call, why),
            }
        }
        for ((context, id), calls) in by_key {
            let forms: BTreeSet<Forms> = calls.iter().map(|c| self.forms(c.entry)).collect();
            if forms.len() < 2 {
                continue;
            }
            let places: Vec<String> = calls.iter().map(|c| place(c)).collect();
            let context = context.map_or(String::new(), |c| format!(" in the context `{c}`"));
            let why = format!(
                "`{id}`{context} is one message to `t`, which {} translates two ways, at {}",
                self.path.display(),
                places.join(" and "),
            );
            for call in calls {
                keep(call, why.clone());
            }
        }
        kept
    }

    /// The `.po` entry of `call`, or why its translation cannot be one.
    fn entry_of(&self, call: &Call<'_>) -> Result<PoEntry, String> {
        let m = call.message;
        let read = |text: &str| -> Result<ljos_compiler::i18n::message::MessageCall, String> {
            match ljos_compiler::i18n::message::message_in(text) {
                Some(Ok(message)) => Ok(message),
                Some(Err(e)) => Err(format!("`{text}` is not a message: {}", e.message)),
                None => Err(format!("`{text}` is not a message")),
            }
        };
        let context = m.context.as_deref();
        let source = qml_compat::rewrite_message(&m.call, &[], context)
            .map_err(|e| e.to_string())
            .and_then(|text| read(&text))?;
        let translation = match self.forms(call.entry) {
            None => String::new(),
            Some(forms) if m.function == "qsTrId" => ljos_i18n::message::escape(&forms[0], false),
            Some(forms) => {
                let written: Vec<(&str, &str)> = if m.counted {
                    ljos_i18n::plural::categories(&self.language)
                        .ok_or_else(|| {
                            format!(
                                "the numerus forms Qt gives `{}` are not the plural categories `t` has",
                                self.language
                            )
                        })?
                        .into_iter()
                        .map(|(category, form)| (category, forms[form].as_str()))
                        .collect()
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
                read(&text)?.message.unwrap_or_default()
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
    /// replaced, the others stay.
    pub(super) fn po(
        &self,
        lowering: &Lowering,
        shown: &BTreeMap<String, &PathBuf>,
    ) -> Result<String, String> {
        let mut po = match std::fs::read_to_string(self.po_path()) {
            Ok(text) => PoFile::parse(&text).map_err(|e| e.to_string())?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => PoFile::default(),
            Err(e) => return Err(e.to_string()),
        };
        let mut written: BTreeMap<(Option<String>, String), PoEntry> = BTreeMap::new();
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
        Ok(po.write(&self.language))
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
