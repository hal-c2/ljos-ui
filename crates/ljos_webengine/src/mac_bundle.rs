//! The application bundle CEF needs on macOS, made around any executable
//! that shows web pages.
//!
//! On macOS CEF is a framework loaded at run time from inside the
//! application's bundle, and it starts its other processes from helper
//! applications of their own, whose names it derives from the
//! application's (CEF's GeneralUsage, "macOS"):
//!
//! ```text
//! <Name>.app/Contents/
//!   Info.plist
//!   MacOS/<Name>                                   the application
//!   Frameworks/Chromium Embedded Framework.framework
//!   Frameworks/<Name> Helper.app/Contents/MacOS/<Name> Helper
//!   Frameworks/<Name> Helper (GPU).app/…            and Renderer, Plugin, Alerts
//! ```
//!
//! Each helper application runs `ljos-webengine-helper`. The view finds
//! all of it from where the running executable is ([`helper_executable`]),
//! so nothing in the bundle names a path of the machine it was made on.
//! Nothing here depends on CEF or on macOS, so a bundle can be made, and
//! the layout tested, anywhere.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The framework's directory name, as CEF's distribution has it.
pub const FRAMEWORK: &str = "Chromium Embedded Framework.framework";

/// The helper applications CEF starts its processes from, by the suffix
/// of their names.
pub const HELPERS: [&str; 5] = [
    " Helper",
    " Helper (GPU)",
    " Helper (Renderer)",
    " Helper (Plugin)",
    " Helper (Alerts)",
];

#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    #[error("{0} is not there")]
    Missing(PathBuf),
    #[error("{0}: {1}")]
    Io(PathBuf, io::Error),
    #[error("no {FRAMEWORK} under {0}")]
    NoFramework(PathBuf),
    #[error("more than one {FRAMEWORK}: {}; name one", .0.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", "))]
    Frameworks(Vec<PathBuf>),
}

/// What to bundle and where.
#[derive(Clone, Debug)]
pub struct Bundle<'a> {
    /// The application's executable; its file name names the bundle.
    pub executable: &'a Path,
    /// `ljos-webengine-helper`.
    pub helper: &'a Path,
    /// CEF's `Chromium Embedded Framework.framework`.
    pub framework: &'a Path,
    /// Where `<Name>.app` is made.
    pub out_dir: &'a Path,
    /// `CFBundleIdentifier`; the helpers' are this with `.helper…`.
    pub identifier: &'a str,
    /// Link the framework rather than copy it: a bundle for this machine
    /// only, made in a moment and taking no room.
    pub link_framework: bool,
}

/// The executable CEF starts its main helper from, in the bundle of the
/// executable at `exe` (`<Name>.app/Contents/MacOS/<Name>`).
pub fn helper_executable(exe: &Path) -> Option<PathBuf> {
    let name = exe.file_name()?.to_str()?;
    let contents = exe.parent()?.parent()?;
    let helper = format!("{name}{}", HELPERS[0]);
    Some(
        contents
            .join("Frameworks")
            .join(format!("{helper}.app"))
            .join("Contents/MacOS")
            .join(helper),
    )
}

/// The framework in the bundle of the executable at `exe`.
pub fn framework_of(exe: &Path) -> Option<PathBuf> {
    Some(exe.parent()?.parent()?.join("Frameworks").join(FRAMEWORK))
}

/// The bundle the executable at `exe` runs in, if it runs in one.
pub fn bundle_of(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let app = contents.parent()?;
    (macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && app.extension()? == "app")
        .then(|| app.to_path_buf())
}

impl Bundle<'_> {
    /// Makes `<Name>.app` in `out_dir`, in place of one that was there,
    /// and returns its path.
    pub fn make(&self) -> Result<PathBuf, BundleError> {
        for p in [self.executable, self.helper, self.framework] {
            if !p.exists() {
                return Err(BundleError::Missing(p.to_path_buf()));
            }
        }
        let name = self
            .executable
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| BundleError::Missing(self.executable.to_path_buf()))?;
        let app = self.out_dir.join(format!("{name}.app"));
        if fs::symlink_metadata(&app).is_ok() {
            fs::remove_dir_all(&app).map_err(|e| BundleError::Io(app.clone(), e))?;
        }
        app_dir(
            &app,
            name,
            self.executable,
            &plist(name, self.identifier, false),
        )?;
        let frameworks = app.join("Contents/Frameworks");
        fs::create_dir_all(&frameworks).map_err(|e| BundleError::Io(frameworks.clone(), e))?;
        let framework = frameworks.join(FRAMEWORK);
        if self.link_framework {
            link_dir(self.framework, &framework)?;
        } else {
            copy_dir(self.framework, &framework)?;
        }
        for suffix in HELPERS {
            let helper = format!("{name}{suffix}");
            let id = format!(
                "{}.helper{}",
                self.identifier,
                suffix
                    .trim_start_matches(" Helper")
                    .trim_matches([' ', '(', ')'])
                    .to_lowercase()
                    .split_whitespace()
                    .map(|w| format!(".{w}"))
                    .collect::<String>()
            );
            app_dir(
                &frameworks.join(format!("{helper}.app")),
                &helper,
                self.helper,
                &plist(&helper, &id, true),
            )?;
        }
        Ok(app)
    }
}

/// `<app>/Contents` with its Info.plist and `MacOS/<name>`, a link of
/// `exe` where the file system allows one (a build replaces its outputs
/// rather than writing into them, so the link keeps what was bundled).
fn app_dir(app: &Path, name: &str, exe: &Path, plist: &str) -> Result<(), BundleError> {
    let contents = app.join("Contents");
    let macos = contents.join("MacOS");
    fs::create_dir_all(&macos).map_err(|e| BundleError::Io(macos.clone(), e))?;
    let info = contents.join("Info.plist");
    fs::write(&info, plist).map_err(|e| BundleError::Io(info, e))?;
    let to = macos.join(name);
    if fs::hard_link(exe, &to).is_err() {
        fs::copy(exe, &to).map_err(|e| BundleError::Io(to.clone(), e))?;
    }
    Ok(())
}

#[cfg(unix)]
fn link_dir(from: &Path, to: &Path) -> Result<(), BundleError> {
    let from = fs::canonicalize(from).map_err(|e| BundleError::Io(from.to_path_buf(), e))?;
    std::os::unix::fs::symlink(&from, to).map_err(|e| BundleError::Io(to.to_path_buf(), e))
}

#[cfg(not(unix))]
fn link_dir(from: &Path, to: &Path) -> Result<(), BundleError> {
    copy_dir(from, to)
}

/// Copies a directory as it is, its symbolic links as links: a framework
/// is a tree of them (`Versions/Current`).
fn copy_dir(from: &Path, to: &Path) -> Result<(), BundleError> {
    let io = |p: &Path| {
        let p = p.to_path_buf();
        move |e| BundleError::Io(p, e)
    };
    fs::create_dir_all(to).map_err(io(to))?;
    for entry in fs::read_dir(from).map_err(io(from))? {
        let entry = entry.map_err(io(from))?;
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        let kind = entry.file_type().map_err(io(&src))?;
        if kind.is_symlink() {
            #[cfg(unix)]
            {
                let target = fs::read_link(&src).map_err(io(&src))?;
                std::os::unix::fs::symlink(target, &dst).map_err(io(&dst))?;
            }
            #[cfg(not(unix))]
            fs::copy(&src, &dst).map_err(io(&dst))?;
        } else if kind.is_dir() {
            copy_dir(&src, &dst)?;
        } else {
            fs::copy(&src, &dst).map_err(io(&dst))?;
        }
    }
    Ok(())
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The Info.plist of the application or one of its helpers. High
/// resolution, or the page is drawn at half the screen's density; a
/// helper is an agent, with no Dock icon or menu bar of its own.
fn plist(name: &str, identifier: &str, helper: bool) -> String {
    let (name, identifier) = (escape(name), escape(identifier));
    let agent = if helper {
        "\t<key>LSUIElement</key>\n\t<true/>\n"
    } else {
        ""
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n\
         \t<key>CFBundleExecutable</key>\n\t<string>{name}</string>\n\
         \t<key>CFBundleIdentifier</key>\n\t<string>{identifier}</string>\n\
         \t<key>CFBundleName</key>\n\t<string>{name}</string>\n\
         \t<key>CFBundlePackageType</key>\n\t<string>APPL</string>\n\
         \t<key>CFBundleInfoDictionaryVersion</key>\n\t<string>6.0</string>\n\
         \t<key>LSMinimumSystemVersion</key>\n\t<string>11.0</string>\n\
         \t<key>NSHighResolutionCapable</key>\n\t<true/>\n\
         {agent}</dict>\n</plist>\n"
    )
}

/// The one framework of a CEF distribution under `root`, as the `cef`
/// crate unpacks one into `$CEF_PATH` (`<version>/cef_macos_<arch>/`), or
/// at its top. A bump of the crate leaves the old version's there too, and
/// one whose API does not match fails only when it runs, so with more than
/// one this does not choose.
pub fn framework_under(root: &Path) -> Result<PathBuf, BundleError> {
    let mut found = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    for _ in 0..3 {
        let mut next = Vec::new();
        for d in dirs {
            let fw = d.join(FRAMEWORK);
            if fw.is_dir() {
                found.push(fw);
            } else if let Ok(entries) = fs::read_dir(&d) {
                next.extend(entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()));
            }
        }
        dirs = next;
    }
    match found.len() {
        0 => Err(BundleError::NoFramework(root.to_path_buf())),
        1 => Ok(found.remove(0)),
        _ => {
            found.sort();
            Err(BundleError::Frameworks(found))
        }
    }
}

/// `org.ljos.<name>`, with what an identifier may not have (anything but
/// letters, digits, `-` and `.`) as `-`.
pub fn default_identifier(name: &str) -> String {
    let name: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!("org.ljos.{name}")
}

/// The framework's library that the helper at `helper` loads, from
/// `../../..` of itself: there only when it runs inside a bundle's
/// Frameworks (`<Name> Helper.app/Contents/MacOS/<Name> Helper`).
pub fn framework_of_helper(helper: &Path) -> Option<PathBuf> {
    let library = helper
        .parent()?
        .join("../../..")
        .join(FRAMEWORK)
        .join("Chromium Embedded Framework");
    let library = fs::canonicalize(library).ok()?;
    library.is_file().then_some(library)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bundle_has_the_framework_and_five_helpers_that_run_the_helper() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        fs::write(d.join("demo"), b"app").unwrap();
        fs::write(d.join("ljos-webengine-helper"), b"helper").unwrap();
        let fw = d.join("cef").join(FRAMEWORK);
        fs::create_dir_all(fw.join("Resources")).unwrap();
        fs::write(fw.join("Resources/resources.pak"), b"pak").unwrap();
        let out = d.join("out");
        fs::create_dir_all(&out).unwrap();
        let bundle = Bundle {
            executable: &d.join("demo"),
            helper: &d.join("ljos-webengine-helper"),
            framework: &fw,
            out_dir: &out,
            identifier: "org.example.demo",
            link_framework: false,
        };
        let app = bundle.make().unwrap();
        assert_eq!(app, out.join("demo.app"));
        let exe = app.join("Contents/MacOS/demo");
        assert_eq!(fs::read(&exe).unwrap(), b"app");
        assert_eq!(bundle_of(&exe), Some(app.clone()));
        let fw_in = framework_of(&exe).unwrap();
        assert_eq!(
            fs::read(fw_in.join("Resources/resources.pak")).unwrap(),
            b"pak"
        );
        let helper = helper_executable(&exe).unwrap();
        assert_eq!(
            helper,
            app.join("Contents/Frameworks/demo Helper.app/Contents/MacOS/demo Helper")
        );
        for suffix in HELPERS {
            let h = app
                .join("Contents/Frameworks")
                .join(format!("demo{suffix}.app/Contents/MacOS/demo{suffix}"));
            assert_eq!(fs::read(&h).unwrap(), b"helper", "{suffix}");
        }
        let gpu = fs::read_to_string(
            app.join("Contents/Frameworks/demo Helper (GPU).app/Contents/Info.plist"),
        )
        .unwrap();
        assert!(gpu.contains("<string>org.example.demo.helper.gpu</string>"));
        assert!(gpu.contains("<key>LSUIElement</key>"));
        let main = fs::read_to_string(app.join("Contents/Info.plist")).unwrap();
        assert!(main.contains("<string>demo</string>") && !main.contains("LSUIElement"));

        // Made again over the first, with the framework linked.
        Bundle {
            link_framework: true,
            ..bundle
        }
        .make()
        .unwrap();
        assert!(helper.is_file());
        assert_eq!(
            fs::read(fw_in.join("Resources/resources.pak")).unwrap(),
            b"pak"
        );
    }

    /// A bump of the `cef` crate leaves the old version's distribution
    /// under `$CEF_PATH`; which one to bundle is not to be guessed.
    #[test]
    fn two_frameworks_under_cef_path_are_not_chosen_between() {
        let dir = tempfile::tempdir().unwrap();
        for v in ["155.0.1", "156.0.15"] {
            fs::create_dir_all(dir.path().join(v).join("cef_macos_aarch64").join(FRAMEWORK))
                .unwrap();
        }
        match framework_under(dir.path()) {
            Err(BundleError::Frameworks(both)) => assert_eq!(both.len(), 2),
            other => panic!("chose {other:?}"),
        }
        fs::remove_dir_all(dir.path().join("155.0.1")).unwrap();
        assert_eq!(
            framework_under(dir.path()).unwrap(),
            dir.path()
                .join("156.0.15/cef_macos_aarch64")
                .join(FRAMEWORK)
        );
    }

    /// `CFBundleIdentifier` takes letters, digits, `-` and `.` only.
    #[test]
    fn a_default_identifier_has_only_what_an_identifier_may_have() {
        assert_eq!(default_identifier("My App"), "org.ljos.My-App");
        assert_eq!(default_identifier("my_app"), "org.ljos.my-app");
        assert_eq!(default_identifier("ljos"), "org.ljos.ljos");
    }

    /// The helper loads the framework from `../../..` of itself, which is
    /// there only inside a bundle's Frameworks.
    #[test]
    fn a_helper_finds_the_framework_only_inside_a_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let loose = dir.path().join("ljos-webengine-helper");
        fs::write(&loose, b"helper").unwrap();
        assert_eq!(framework_of_helper(&loose), None);
        let frameworks = dir.path().join("demo.app/Contents/Frameworks");
        let binary = frameworks
            .join(FRAMEWORK)
            .join("Chromium Embedded Framework");
        fs::create_dir_all(binary.parent().unwrap()).unwrap();
        fs::write(&binary, b"fw").unwrap();
        let helper = frameworks.join("demo Helper.app/Contents/MacOS/demo Helper");
        fs::create_dir_all(helper.parent().unwrap()).unwrap();
        fs::write(&helper, b"helper").unwrap();
        assert_eq!(framework_of_helper(&helper), Some(binary));
    }

    #[test]
    fn an_executable_outside_a_bundle_has_none() {
        assert_eq!(bundle_of(Path::new("/x/target/debug/ljos")), None);
        assert_eq!(
            bundle_of(Path::new("/x/ljos.app/Contents/MacOS/ljos")),
            Some(PathBuf::from("/x/ljos.app"))
        );
    }
}
