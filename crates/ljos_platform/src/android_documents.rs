//! What Android's document picker (the Storage Access Framework) is asked
//! for a dialog, and what its answer means: the part of
//! `AndroidDialogs` (built on Android only) that needs no phone,
//! so it is built and tested everywhere.
//!
//! A file or folder dialog becomes one of the system's intents:
//!
//! | Dialog | Intent |
//! |---|---|
//! | `FileDialog.OpenFile` | `ACTION_OPEN_DOCUMENT` |
//! | `FileDialog.OpenFiles` | `ACTION_OPEN_DOCUMENT` with `EXTRA_ALLOW_MULTIPLE` |
//! | `FileDialog.SaveFile` | `ACTION_CREATE_DOCUMENT`, the suggested name as `EXTRA_TITLE` |
//! | `FolderDialog` | `ACTION_OPEN_DOCUMENT_TREE` |
//!
//! The picker filters by MIME type, not by name: each `*.ext` of the name
//! filters is turned into its type on the Java side (`MimeTypeMap`), and a
//! filter that takes every file, or an extension with no known type, offers
//! every document.
//!
//! A document opened is copied into the application's cache, so whatever
//! reads a file (`XMLHttpRequest`, an `Image`) reads it, and the dialog's
//! URL is the copy's `file://`. A document to save to, and a folder, are
//! the provider's `content://` URL, kept across restarts with the
//! system's persistable grant: nothing else can stand for them.

use std::path::Path;

use crate::dialogs::{DialogKind, DialogRequest};
use crate::paths::file_url;

/// Which picker a dialog is, as the Java side numbers them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Picker {
    Open = 0,
    OpenMany = 1,
    Save = 2,
    Folder = 3,
}

impl Picker {
    /// The picker for a kind of dialog, or `None` for one the system has
    /// no picker for (a message, a colour, a font), which the application
    /// draws.
    pub fn of(kind: DialogKind) -> Option<Picker> {
        match kind {
            DialogKind::OpenFile => Some(Picker::Open),
            DialogKind::OpenFiles => Some(Picker::OpenMany),
            DialogKind::SaveFile => Some(Picker::Save),
            DialogKind::Folder => Some(Picker::Folder),
            DialogKind::Message | DialogKind::Color | DialogKind::Font => None,
        }
    }
}

/// The extensions the picker offers documents of, each once and in the
/// order the filters give them; `None` when it offers every document.
pub fn extensions(request: &DialogRequest) -> Option<Vec<String>> {
    if request.filters.is_empty() || request.filters.iter().any(|f| f.takes_every_file()) {
        return None;
    }
    let mut out: Vec<String> = Vec::new();
    for ext in request.filters.iter().flat_map(|f| f.extensions()) {
        let ext = ext.to_ascii_lowercase();
        if !out.contains(&ext) {
            out.push(ext);
        }
    }
    // A filter of names alone (`README`), which the picker cannot offer
    // by type: offer every document.
    (!out.is_empty()).then_some(out)
}

/// What the picker's answer, as the Java side hands it, chose: one line a
/// document, the absolute path of a copy or the provider's URL. `None`
/// when the user left the picker without choosing.
pub fn chosen(answer: Option<&str>) -> Option<Vec<String>> {
    let urls: Vec<String> = answer?
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            if l.starts_with('/') {
                file_url(Path::new(l))
            } else {
                l.to_owned()
            }
        })
        .collect();
    (!urls.is_empty()).then_some(urls)
}

/// The name a save picker suggests: the dialog's, or none.
pub fn suggested_name(request: &DialogRequest) -> String {
    request.file_name.clone().unwrap_or_default()
}

#[cfg(target_os = "android")]
pub use self::java::AndroidDialogs;

/// The picker itself, through `dev.ljos.LjosDocuments` of Ljós's Android
/// template.
#[cfg(target_os = "android")]
mod java {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicI32, Ordering};
    use std::sync::{Mutex, MutexGuard, OnceLock};

    use ljos_jni::jni::objects::{JClass, JString, JValue};
    use ljos_jni::jni::sys::jboolean;
    use ljos_jni::jni::{EnvUnowned, jni_sig, jni_str};
    use ljos_jni::{app_class, with_java};

    use super::{Picker, chosen, extensions, suggested_name};
    use crate::dialogs::{DialogAnswer, DialogReply, DialogRequest, DialogService};

    /// Android's document picker as the system's file and folder dialogs.
    /// A message, a colour and a font have no picker, and are drawn.
    #[derive(Debug, Default, Clone, Copy)]
    pub struct AndroidDialogs;

    /// A picker on screen, by its request code.
    struct Pending {
        ticket: u64,
        answer: DialogAnswer,
    }

    fn pending() -> MutexGuard<'static, HashMap<i32, Pending>> {
        static PENDING: OnceLock<Mutex<HashMap<i32, Pending>>> = OnceLock::new();
        PENDING
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// A request code for the activity's result, which an activity of
    /// AndroidX takes only in the low 16 bits.
    fn next_code() -> i32 {
        static NEXT: AtomicI32 = AtomicI32::new(0);
        NEXT.fetch_add(1, Ordering::Relaxed).rem_euclid(0xFFFF) + 1
    }

    /// Starts the picker; false when there is no activity to start it from
    /// or the Java side refused.
    fn start(picker: Picker, request: &DialogRequest, code: i32) -> bool {
        let types = extensions(request).unwrap_or_default().join("\n");
        let name = suggested_name(request);
        with_java(|env, context| {
            let class = app_class(env, context, jni_str!("dev.ljos.LjosDocuments"))?;
            let types = env.new_string(types)?;
            let name = env.new_string(name)?;
            env.call_static_method(
                &class,
                jni_str!("show"),
                jni_sig!("(Landroid/content/Context;ILjava/lang/String;Ljava/lang/String;I)Z"),
                &[
                    JValue::Object(context),
                    JValue::Int(picker as i32),
                    JValue::from(&types),
                    JValue::from(&name),
                    JValue::Int(code),
                ],
            )?
            .z()
        })
        .unwrap_or_else(|e| {
            log::warn!("the document picker was not shown: {e}");
            false
        })
    }

    impl DialogService for AndroidDialogs {
        fn can_show(&self, request: &DialogRequest) -> bool {
            request.kind.and_then(Picker::of).is_some()
        }

        fn show(&self, request: DialogRequest, answer: DialogAnswer) {
            // Dropped, an answer is a dismissal: the dialog is rejected.
            let Some(picker) = request.kind.and_then(Picker::of) else {
                return;
            };
            let code = next_code();
            let ticket = answer.ticket();
            pending().insert(code, Pending { ticket, answer });
            if !start(picker, &request, code) {
                let dropped = pending().remove(&code);
                drop(dropped);
            }
        }

        fn close(&self, ticket: u64) {
            let taken = {
                let mut pending = pending();
                let code = pending
                    .iter()
                    .find(|(_, p)| p.ticket == ticket)
                    .map(|(code, _)| *code);
                code.and_then(|c| pending.remove(&c).map(|p| (c, p)))
            };
            // Its answer, dropped, is not heard: QML closed the dialog
            // already.
            let Some((code, _closed)) = taken else {
                return;
            };
            let closed = with_java(|env, context| {
                let class = app_class(env, context, jni_str!("dev.ljos.LjosDocuments"))?;
                env.call_static_method(
                    &class,
                    jni_str!("close"),
                    jni_sig!("(I)V"),
                    &[JValue::Int(code)],
                )?;
                Ok(())
            });
            if let Err(e) = closed {
                log::warn!("the document picker was not taken away: {e}");
            }
        }
    }

    /// The picker's answer, from `LjosDocuments`: what was chosen, a line
    /// each, or null when the user left it. False when no dialog waited for
    /// it, so that the Java side removes the copies it made.
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_dev_ljos_LjosDocuments_nativeAnswered<'caller>(
        mut unowned: EnvUnowned<'caller>,
        _class: JClass<'caller>,
        code: i32,
        answer: JString<'caller>,
    ) -> jboolean {
        unowned
            .with_env(|env| -> ljos_jni::jni::errors::Result<jboolean> {
                let answer = if answer.is_null() {
                    None
                } else {
                    Some(answer.try_to_string(env)?)
                };
                // Taken out of the lock before it is sent, which posts to
                // the application's inbox.
                let Some(waiting) = pending().remove(&code) else {
                    return Ok(false);
                };
                match chosen(answer.as_deref()) {
                    Some(urls) => waiting.answer.send(DialogReply::Documents(urls)),
                    None => waiting.answer.send(DialogReply::Dismissed),
                }
                Ok(true)
            })
            .resolve::<ljos_jni::jni::errors::ThrowRuntimeExAndDefault>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dialogs::NameFilter;

    fn filtered(filters: &[&str]) -> DialogRequest {
        DialogRequest {
            kind: Some(DialogKind::OpenFile),
            filters: filters.iter().map(|f| NameFilter::parse(f)).collect(),
            ..DialogRequest::default()
        }
    }

    #[test]
    fn each_file_or_folder_dialog_has_a_picker() {
        assert_eq!(Picker::of(DialogKind::OpenFile), Some(Picker::Open));
        assert_eq!(Picker::of(DialogKind::OpenFiles), Some(Picker::OpenMany));
        assert_eq!(Picker::of(DialogKind::SaveFile), Some(Picker::Save));
        assert_eq!(Picker::of(DialogKind::Folder), Some(Picker::Folder));
        assert_eq!(Picker::of(DialogKind::Message), None);
        assert_eq!(Picker::of(DialogKind::Color), None);
        assert_eq!(Picker::of(DialogKind::Font), None);
    }

    #[test]
    fn the_name_filters_become_the_extensions_offered() {
        assert_eq!(
            extensions(&filtered(&["Text (*.txt *.MD)", "Images (*.png *.txt)"])),
            Some(vec!["txt".to_owned(), "md".to_owned(), "png".to_owned()])
        );
    }

    #[test]
    fn a_filter_that_takes_every_file_offers_every_document() {
        assert_eq!(extensions(&filtered(&[])), None);
        assert_eq!(
            extensions(&filtered(&["Text (*.txt)", "All files (*)"])),
            None
        );
        assert_eq!(extensions(&filtered(&["Readme (README)"])), None);
    }

    #[test]
    fn a_copy_is_its_file_and_a_document_its_url() {
        assert_eq!(
            chosen(Some(
                "/data/user/0/dev.ljos.app/cache/documents/3/notes one.txt\n\
                 content://com.android.providers.downloads.documents/document/msf%3A31\n"
            )),
            Some(vec![
                "file:///data/user/0/dev.ljos.app/cache/documents/3/notes one.txt".to_owned(),
                "content://com.android.providers.downloads.documents/document/msf%3A31".to_owned(),
            ])
        );
    }

    #[test]
    fn a_picker_left_without_choosing_chose_nothing() {
        assert_eq!(chosen(None), None);
        assert_eq!(chosen(Some("")), None);
    }

    #[test]
    fn a_save_suggests_the_dialogs_name() {
        let request = DialogRequest {
            file_name: Some("report.txt".to_owned()),
            ..DialogRequest::default()
        };
        assert_eq!(suggested_name(&request), "report.txt");
        assert_eq!(suggested_name(&DialogRequest::default()), "");
    }
}
