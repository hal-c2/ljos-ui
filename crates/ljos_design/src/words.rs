//! Words the content rules hold an interface to.

/// Ways of naming one action that one window should not mix. Each group
/// is a set of whole labels (lower case, without a trailing `…`) that say
/// the same thing; a window using two of one group says it two ways
/// (`content-inconsistent-words`). Kept small and certain: words that
/// differ in meaning anywhere (Delete and Remove, Close and Cancel) are
/// not here.
pub const SYNONYMS: &[&[&str]] = &[
    &["retry", "try again"],
    &["sign in", "log in", "login", "log on"],
    &["sign out", "log out", "logout", "log off"],
    &["settings", "preferences"],
    &["ok", "okay"],
];
