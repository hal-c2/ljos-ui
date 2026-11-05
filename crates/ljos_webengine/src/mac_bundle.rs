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

    #[test]
    fn an_executable_outside_a_bundle_has_none() {
        assert_eq!(bundle_of(Path::new("/x/target/debug/ljos")), None);
        assert_eq!(
            bundle_of(Path::new("/x/ljos.app/Contents/MacOS/ljos")),
            Some(PathBuf::from("/x/ljos.app"))
        );
    }
}
