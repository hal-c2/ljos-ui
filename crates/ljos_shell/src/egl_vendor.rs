//! Which EGL drivers glvnd loads.
//!
//! glvnd's libEGL loads every vendor library it is told of when the first
//! display is made, whether or not the vendor has a device on this
//! machine. A machine with NVIDIA's user-space package and no NVIDIA card
//! (or its kernel driver not loaded) maps NVIDIA's EGL libraries into
//! every process that draws with EGL: about 34 MiB mapped and 4.4 MiB of
//! it private, for a vendor that answers no display. glvnd has no call
//! that leaves a vendor out, only its environment, so the shell names the
//! vendors to load there, before anything loads libEGL.

use std::path::{Path, PathBuf};

/// Where glvnd looks for vendors when it is not told: its configuration
/// directory, then its data directory.
const VENDOR_DIRS: [&str; 2] = ["/etc/glvnd/egl_vendor.d", "/usr/share/glvnd/egl_vendor.d"];

/// Present while NVIDIA's kernel driver is loaded.
const NVIDIA_DRIVER: &str = "/proc/driver/nvidia";

/// Leaves NVIDIA's EGL vendor out when its driver is not loaded. Does
/// nothing when the user named vendors themselves.
pub(crate) fn leave_out_unused() {
    if std::env::var_os("__EGL_VENDOR_LIBRARY_FILENAMES").is_some()
        || std::env::var_os("__EGL_VENDOR_LIBRARY_DIRS").is_some()
        || Path::new(NVIDIA_DRIVER).exists()
    {
        return;
    }
    let mut vendors = Vec::new();
    for dir in VENDOR_DIRS {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        // glvnd's own order: by name within each directory.
        files.sort();
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap_or_default();
            vendors.push((file, text));
        }
    }
    let Some(kept) = without_nvidia(&vendors) else {
        return;
    };
    // SAFETY: called by `run` on the main thread before the shell starts
    // any thread of its own or loads anything that reads the environment
    // from C. Rust's own readers take std's lock; glibc (2.40 on) never
    // frees an environment block a concurrent `getenv` could be reading.
    unsafe {
        std::env::set_var("__EGL_VENDOR_LIBRARY_FILENAMES", kept);
    }
}

/// The vendor files to load, joined as glvnd reads them, when one of
/// `vendors` (path, contents) is NVIDIA's; `None` when there is nothing
/// to leave out or nothing would be left.
fn without_nvidia(vendors: &[(PathBuf, String)]) -> Option<String> {
    let is_nvidia = |text: &str| text.to_ascii_lowercase().contains("nvidia");
    let kept: Vec<&Path> = vendors
        .iter()
        .filter(|(_, text)| !is_nvidia(text))
        .map(|(path, _)| path.as_path())
        .collect();
    if kept.len() == vendors.len() || kept.is_empty() {
        return None;
    }
    let joined: Vec<&str> = kept.iter().filter_map(|p| p.to_str()).collect();
    (joined.len() == kept.len()).then(|| joined.join(":"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vendor(path: &str, library: &str) -> (PathBuf, String) {
        (
            PathBuf::from(path),
            format!(
                r#"{{ "file_format_version": "1.0.0", "ICD": {{ "library_path": "{library}" }} }}"#
            ),
        )
    }

    #[test]
    fn nvidia_is_left_out_and_the_rest_kept_in_order() {
        let vendors = [
            vendor(
                "/usr/share/glvnd/egl_vendor.d/10_nvidia.json",
                "libEGL_nvidia.so.0",
            ),
            vendor(
                "/usr/share/glvnd/egl_vendor.d/50_mesa.json",
                "libEGL_mesa.so.0",
            ),
            vendor(
                "/usr/share/glvnd/egl_vendor.d/60_other.json",
                "libEGL_other.so.0",
            ),
        ];
        assert_eq!(
            without_nvidia(&vendors).as_deref(),
            Some(
                "/usr/share/glvnd/egl_vendor.d/50_mesa.json:/usr/share/glvnd/egl_vendor.d/60_other.json"
            )
        );
    }

    #[test]
    fn nothing_changes_without_nvidia_or_with_nothing_else() {
        let mesa = [vendor(
            "/usr/share/glvnd/egl_vendor.d/50_mesa.json",
            "libEGL_mesa.so.0",
        )];
        assert_eq!(without_nvidia(&mesa), None);
        let nvidia = [vendor(
            "/usr/share/glvnd/egl_vendor.d/10_nvidia.json",
            "libEGL_nvidia.so.0",
        )];
        assert_eq!(without_nvidia(&nvidia), None);
        assert_eq!(without_nvidia(&[]), None);
    }
}
