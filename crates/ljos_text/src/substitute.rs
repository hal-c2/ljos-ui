//! The family the platform puts in place of a family that is not installed.
//!
//! Fontique reads the system's fonts but applies the platform's
//! substitution rules to generic families only, so `font.family:
//! "Helvetica"` on a Linux machine without Helvetica fell to the default
//! face. Qt asks fontconfig instead and gets the face the system's rules
//! name for it (Nimbus Sans for Helvetica where URW's fonts are installed),
//! whose ascent and descent differ from the default's. Only an engine that
//! reads the system's fonts asks: with the bundled fonts alone, the answer
//! would name a face the engine cannot use.

/// The family the platform's rules choose for `family`, when it is another
/// one. `None` when the family is installed under that name, or the
/// platform has no such rules.
pub(crate) fn platform_substitute(family: &str) -> Option<String> {
    let found = imp::best_match(family)?;
    (!found.eq_ignore_ascii_case(family)).then_some(found)
}

/// Loads the platform's rules and its list of fonts now, so that the first
/// family asked for does not wait for them.
pub(crate) fn load_rules() {
    imp::load_rules();
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
mod imp {
    use std::ffi::{CStr, CString};
    use std::ptr;

    use fontconfig_sys::constants::FC_FAMILY;
    use fontconfig_sys::{
        FcConfigSubstitute, FcDefaultSubstitute, FcFontMatch, FcMatchPattern, FcPattern,
        FcPatternAddString, FcPatternCreate, FcPatternDestroy, FcPatternGetString, FcResultMatch,
    };

    /// Destroys the pattern when dropped.
    struct Pattern(*mut FcPattern);

    impl Drop for Pattern {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: the pointer came from fontconfig and is destroyed once.
                unsafe { FcPatternDestroy(self.0) };
            }
        }
    }

    /// fontconfig's default configuration, which `best_match` asks, is not
    /// the one Fontique loads for itself, and loads on first use.
    pub(super) fn load_rules() {
        // SAFETY: takes nothing and only loads the default configuration;
        // fontconfig guards its loading against other threads.
        unsafe { fontconfig_sys::FcInit() };
    }

    /// The family of the face `fc-match` would print for `family`.
    pub(super) fn best_match(family: &str) -> Option<String> {
        let name = CString::new(family).ok()?;
        // SAFETY: every pointer handed to fontconfig is either one it made
        // (and `Pattern` owns) or a NUL-terminated string that outlives the
        // call; a null config means the default configuration, which
        // fontconfig loads on first use. The string read back belongs to
        // `found` and is copied before `found` is destroyed.
        unsafe {
            let pattern = Pattern(FcPatternCreate());
            if pattern.0.is_null()
                || FcPatternAddString(pattern.0, FC_FAMILY.as_ptr(), name.as_ptr().cast()) == 0
            {
                return None;
            }
            FcConfigSubstitute(ptr::null_mut(), pattern.0, FcMatchPattern);
            FcDefaultSubstitute(pattern.0);
            let mut result = FcResultMatch;
            let found = Pattern(FcFontMatch(ptr::null_mut(), pattern.0, &raw mut result));
            if found.0.is_null() || result != FcResultMatch {
                return None;
            }
            let mut value = ptr::null_mut();
            if FcPatternGetString(found.0, FC_FAMILY.as_ptr(), 0, &raw mut value) != FcResultMatch
                || value.is_null()
            {
                return None;
            }
            Some(CStr::from_ptr(value.cast()).to_string_lossy().into_owned())
        }
    }
}

/// Not asked on other platforms yet.
#[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
mod imp {
    pub(super) fn load_rules() {}

    pub(super) fn best_match(_family: &str) -> Option<String> {
        None
    }
}
