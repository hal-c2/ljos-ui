//! Which graphics drivers the system's loaders load.
//!
//! glvnd's libEGL loads every vendor library it is told of when the first
//! display is made, and the Vulkan loader every driver (ICD) it finds when
//! an instance is made, whether or not the driver has a device on this
//! machine. A machine with NVIDIA's user-space package and no NVIDIA card
//! (or its kernel driver not loaded) maps NVIDIA's libraries into every
//! process that draws: through EGL about 34 MiB mapped and 4.4 MiB of it
//! private, through Vulkan most of the instance's start (about 450 ms),
//! for a driver that answers no display. Neither loader has a call that
//! leaves a driver out, only its environment, so the shell sets that
//! before anything loads either.

use std::path::{Path, PathBuf};

/// Present while NVIDIA's kernel driver is loaded.
const NVIDIA_DRIVER: &str = "/proc/driver/nvidia";

/// Variables by which the user chose the Vulkan drivers themselves.
const VULKAN_CHOSEN: [&str; 5] = [
    "VK_DRIVER_FILES",
    "VK_ICD_FILENAMES",
    "VK_ADD_DRIVER_FILES",
    "VK_LOADER_DRIVERS_SELECT",
    "VK_LOADER_DRIVERS_DISABLE",
];

/// The first Vulkan loader that reads `VK_LOADER_DRIVERS_DISABLE`.
const DISABLE_SINCE: (u32, u32, u32) = (1, 3, 234);

/// Leaves NVIDIA's EGL vendor and Vulkan driver out when its kernel
/// driver is not loaded. Leaves each loader alone when the user chose its
/// drivers themselves.
pub(crate) fn leave_out_unused() {
    if Path::new(NVIDIA_DRIVER).exists() {
        return;
    }
    #[cfg(gl_painter)]
    let egl = egl_vendors();
    let vulkan = vulkan_drivers();
    // SAFETY: called by `run` on the main thread before the shell starts
    // any thread of its own or loads anything that reads the environment
    // from C. Rust's own readers take std's lock; glibc (2.40 on) never
    // frees an environment block a concurrent `getenv` could be reading.
    unsafe {
        #[cfg(gl_painter)]
        if let Some(kept) = egl {
            std::env::set_var("__EGL_VENDOR_LIBRARY_FILENAMES", kept);
        }
        if let Some((name, value)) = vulkan {
            std::env::set_var(name, value);
        }
    }
}

/// The EGL vendors to name, or `None` to leave glvnd as it is.
#[cfg(gl_painter)]
fn egl_vendors() -> Option<String> {
    if std::env::var_os("__EGL_VENDOR_LIBRARY_FILENAMES").is_some()
        || std::env::var_os("__EGL_VENDOR_LIBRARY_DIRS").is_some()
    {
        return None;
    }
    // Where glvnd looks when it is not told: its configuration directory,
    // then its data directory, by name within each.
    let dirs = ["/etc/glvnd/egl_vendor.d", "/usr/share/glvnd/egl_vendor.d"];
    without_nvidia(&manifests(dirs.iter().map(PathBuf::from)))
}

/// The variable to set and its value, or `None` to leave the Vulkan
/// loader as it is.
fn vulkan_drivers() -> Option<(&'static str, String)> {
    if VULKAN_CHOSEN.iter().any(|v| std::env::var_os(v).is_some()) {
        return None;
    }
    let drivers = manifests(vulkan_dirs());
    // A loader that reads it still finds drivers wherever it looks, so
    // only NVIDIA's are named; an older one is given the others' files.
    if loader_version().is_some_and(|v| v >= DISABLE_SINCE) {
        nvidia_names(&drivers).map(|names| ("VK_LOADER_DRIVERS_DISABLE", names))
    } else {
        without_nvidia(&drivers).map(|files| ("VK_ICD_FILENAMES", files))
    }
}

/// Where the Vulkan loader looks for drivers on Linux, as its
/// documentation lists them.
fn vulkan_dirs() -> Vec<PathBuf> {
    let var = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
    let home = var("HOME").map(PathBuf::from);
    let mut roots: Vec<PathBuf> = Vec::new();
    roots.extend(
        var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home.as_ref().map(|h| h.join(".config"))),
    );
    let config_dirs = var("XDG_CONFIG_DIRS").unwrap_or_else(|| "/etc/xdg".into());
    roots.extend(config_dirs.split(':').map(PathBuf::from));
    roots.push(PathBuf::from("/etc"));
    roots.extend(
        var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| home.as_ref().map(|h| h.join(".local/share"))),
    );
    let data_dirs = var("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    roots.extend(data_dirs.split(':').map(PathBuf::from));
    let mut dirs: Vec<PathBuf> = Vec::new();
    for root in roots {
        let dir = root.join("vulkan/icd.d");
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs
}

/// The JSON manifests in `dirs` with their contents, by name within each
/// directory.
fn manifests(dirs: impl IntoIterator<Item = PathBuf>) -> Vec<(PathBuf, String)> {
    let mut found = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        files.sort();
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap_or_default();
            found.push((file, text));
        }
    }
    found
}

fn is_nvidia(manifest: &str) -> bool {
    manifest.to_ascii_lowercase().contains("nvidia")
}

/// The files of `manifests` (path, contents) that are not NVIDIA's,
/// joined as the loaders read them, when one of them is; `None` when there
/// is nothing to leave out or nothing would be left.
fn without_nvidia(manifests: &[(PathBuf, String)]) -> Option<String> {
    let kept: Vec<&Path> = manifests
        .iter()
        .filter(|(_, text)| !is_nvidia(text))
        .map(|(path, _)| path.as_path())
        .collect();
    if kept.len() == manifests.len() || kept.is_empty() {
        return None;
    }
    let joined: Vec<&str> = kept.iter().filter_map(|p| p.to_str()).collect();
    (joined.len() == kept.len()).then(|| joined.join(":"))
}

/// The file names of NVIDIA's manifests, as `VK_LOADER_DRIVERS_DISABLE`
/// matches them; `None` when there is none or nothing else would be left.
fn nvidia_names(manifests: &[(PathBuf, String)]) -> Option<String> {
    let names: Vec<&str> = manifests
        .iter()
        .filter(|(_, text)| is_nvidia(text))
        .filter_map(|(path, _)| path.file_name()?.to_str())
        .collect();
    let others = manifests.iter().any(|(_, text)| !is_nvidia(text));
    (!names.is_empty() && others).then(|| names.join(","))
}

/// The installed Vulkan loader's version, from the name of the file
/// `libvulkan.so.1` resolves to (`libvulkan.so.1.4.357`). The loader is
/// opened for its path and closed again; that loads no driver.
#[cfg(target_env = "gnu")]
fn loader_version() -> Option<(u32, u32, u32)> {
    use std::ffi::CStr;
    // SAFETY: dlopen, dlinfo and dlclose are called with a valid name, the
    // handle dlopen gave and a pointer of the type RTLD_DI_LINKMAP writes;
    // the link map's name is read before the handle is closed.
    let path = unsafe {
        let handle = libc::dlopen(
            c"libvulkan.so.1".as_ptr(),
            libc::RTLD_LAZY | libc::RTLD_LOCAL,
        );
        if handle.is_null() {
            return None;
        }
        let mut map: *mut LinkMap = std::ptr::null_mut();
        let found = libc::dlinfo(handle, libc::RTLD_DI_LINKMAP, (&raw mut map).cast()) == 0
            && !map.is_null()
            && !(*map).name.is_null();
        let path = found.then(|| CStr::from_ptr((*map).name).to_string_lossy().into_owned());
        libc::dlclose(handle);
        path?
    };
    let file = std::fs::canonicalize(path).ok()?;
    version_of(file.file_name()?.to_str()?)
}

#[cfg(not(target_env = "gnu"))]
fn loader_version() -> Option<(u32, u32, u32)> {
    None
}

/// The start of glibc's `struct link_map`, as `<link.h>` gives it.
#[cfg(target_env = "gnu")]
#[repr(C)]
struct LinkMap {
    addr: usize,
    name: *const libc::c_char,
}

/// `1.4.357` from `libvulkan.so.1.4.357`.
fn version_of(file: &str) -> Option<(u32, u32, u32)> {
    let mut parts = file.strip_prefix("libvulkan.so.")?.split('.');
    let mut next = || parts.next()?.parse::<u32>().ok();
    Some((next()?, next()?, next()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(path: &str, library: &str) -> (PathBuf, String) {
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
            manifest(
                "/usr/share/glvnd/egl_vendor.d/10_nvidia.json",
                "libEGL_nvidia.so.0",
            ),
            manifest(
                "/usr/share/glvnd/egl_vendor.d/50_mesa.json",
                "libEGL_mesa.so.0",
            ),
            manifest(
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
        let mesa = [manifest(
            "/usr/share/glvnd/egl_vendor.d/50_mesa.json",
            "libEGL_mesa.so.0",
        )];
        assert_eq!(without_nvidia(&mesa), None);
        assert_eq!(nvidia_names(&mesa), None);
        let nvidia = [manifest(
            "/usr/share/vulkan/icd.d/nvidia_icd.json",
            "libGLX_nvidia.so.0",
        )];
        assert_eq!(without_nvidia(&nvidia), None);
        assert_eq!(nvidia_names(&nvidia), None);
        assert_eq!(without_nvidia(&[]), None);
        assert_eq!(nvidia_names(&[]), None);
    }

    #[test]
    fn a_new_loader_is_told_which_drivers_to_leave_out_by_name() {
        let drivers = [
            manifest(
                "/usr/share/vulkan/icd.d/nvidia_icd.json",
                "libGLX_nvidia.so.0",
            ),
            manifest(
                "/usr/share/vulkan/icd.d/radeon_icd.json",
                "libvulkan_radeon.so",
            ),
            manifest("/etc/vulkan/icd.d/nvidia_layers.json", "libnvidia-vk.so"),
        ];
        assert_eq!(
            nvidia_names(&drivers).as_deref(),
            Some("nvidia_icd.json,nvidia_layers.json")
        );
    }

    #[test]
    fn the_loader_version_is_read_from_its_file_name() {
        assert_eq!(version_of("libvulkan.so.1.4.357"), Some((1, 4, 357)));
        assert!(version_of("libvulkan.so.1.3.234").is_some_and(|v| v >= DISABLE_SINCE));
        assert!(version_of("libvulkan.so.1.3.231").is_some_and(|v| v < DISABLE_SINCE));
        assert_eq!(version_of("libvulkan.so.1"), None);
    }
}
