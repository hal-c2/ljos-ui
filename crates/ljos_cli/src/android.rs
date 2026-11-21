//! `ljos android`: build an application's library for Android, pack it
//! into the Gradle template (`android/` in the repository) and, with
//! `run`, install and start it (ARCHITECTURE.md §10.3).
//!
//! The application is a crate whose library is a `cdylib` with an
//! `android_main` (`examples/android_hello`). Nothing is installed: the
//! NDK, the SDK, `cargo-ndk`, Gradle and a JDK are found where they are,
//! or named in the error.
//!
//! | Step | What runs |
//! |---|---|
//! | the library | `cargo ndk -t <abi> -P 26 -o <project>/app/src/main/jniLibs build -p <package>`, each ABI |
//! | the project | the template, filled, in `<target>/android/<package>/` |
//! | the package | `gradle -p <project> assembleDebug` (or `assembleRelease`), on `JAVA_HOME`'s JDK |
//! | `run` | `adb install -r <apk>`, then `adb shell am start -n <id>/dev.ljos.LjosActivity` |

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail};
use clap::{Args, Subcommand};

use crate::stdout::outln;

/// The lowest Android API level Ljós runs on (ARCHITECTURE.md §10.3).
const MIN_SDK: u32 = 26;

/// The ABIs Ljós builds, as Android names them.
const ABIS: [&str; 2] = ["arm64-v8a", "x86_64"];

/// The activity the template's manifest declares.
const ACTIVITY: &str = "dev.ljos.LjosActivity";

/// The Gradle template, by the path it is written to in the project.
const TEMPLATE: [(&str, &str); 8] = [
    (
        "settings.gradle.kts",
        include_str!("../../../android/settings.gradle.kts"),
    ),
    (
        "build.gradle.kts",
        include_str!("../../../android/build.gradle.kts"),
    ),
    (
        "gradle.properties",
        include_str!("../../../android/gradle.properties"),
    ),
    (
        "app/build.gradle.kts",
        include_str!("../../../android/app/build.gradle.kts"),
    ),
    (
        "app/src/main/AndroidManifest.xml",
        include_str!("../../../android/app/src/main/AndroidManifest.xml"),
    ),
    (
        "app/src/main/java/dev/ljos/LjosActivity.java",
        include_str!("../../../android/app/src/main/java/dev/ljos/LjosActivity.java"),
    ),
    (
        "app/src/main/java/com/google/androidgamesdk/TextInputOf.java",
        include_str!(
            "../../../android/app/src/main/java/com/google/androidgamesdk/TextInputOf.java"
        ),
    ),
    ("README.md", include_str!("../../../android/README.md")),
];

#[derive(Subcommand)]
pub enum AndroidCommand {
    /// Build the application's package (an `.apk`) and print its path.
    Bundle(Build),
    /// Build the package, install it on the device or emulator `adb` sees,
    /// and start it.
    Run {
        #[command(flatten)]
        build: Build,
        /// The device to install on (`adb -s`), when there is more than one.
        #[arg(long)]
        device: Option<String>,
    },
}

#[derive(Args)]
pub struct Build {
    /// The application's crate: a directory with its `Cargo.toml`.
    #[arg(default_value = ".")]
    path: PathBuf,
    /// Build with optimisations and pack a release package.
    #[arg(long)]
    release: bool,
    /// One ABI to build (arm64-v8a or x86_64) instead of both; repeatable.
    #[arg(long = "abi")]
    abis: Vec<String>,
    /// The application's id, by default `dev.ljos.<package>`.
    #[arg(long)]
    application_id: Option<String>,
    /// The name the launcher shows, by default the package's.
    #[arg(long)]
    label: Option<String>,
    /// An argument passed on to Gradle (`--offline`, a signing property);
    /// repeatable.
    #[arg(long = "gradle-arg", allow_hyphen_values = true)]
    gradle_args: Vec<String>,
}

pub fn run(command: AndroidCommand) -> anyhow::Result<()> {
    match command {
        AndroidCommand::Bundle(build) => {
            let built = build.bundle()?;
            outln!("{}", built.apk.display());
            Ok(())
        }
        AndroidCommand::Run { build, device } => {
            let built = build.bundle()?;
            let adb = |args: &[&str]| -> anyhow::Result<()> {
                let mut c = Command::new(tool("adb", "platform-tools/adb")?);
                if let Some(d) = &device {
                    c.args(["-s", d]);
                }
                status(c.args(args), "adb")
            };
            let apk = built.apk.to_string_lossy();
            adb(&["install", "-r", &apk])?;
            adb(&[
                "shell",
                "am",
                "start",
                "-n",
                &format!("{}/{ACTIVITY}", built.application_id),
            ])?;
            outln!("started {}", built.application_id);
            Ok(())
        }
    }
}

/// What `bundle` made.
struct Built {
    apk: PathBuf,
    application_id: String,
}

/// What the template is filled with.
#[derive(Debug, PartialEq)]
struct Fill {
    application_id: String,
    label: String,
    lib_name: String,
}

impl Fill {
    fn apply(&self, text: &str) -> String {
        text.replace("@APPLICATION_ID@", &self.application_id)
            .replace("@LABEL@", &xml_text(&self.label))
            .replace("@LIB_NAME@", &self.lib_name)
    }
}

/// `text` as it may stand in an XML attribute.
fn xml_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The application id a package is given unless told: its name, as Java
/// takes a package segment and Android an application id's, which starts
/// with a letter.
fn default_application_id(package: &str) -> String {
    let segment: String = package
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    let segment = if !segment.starts_with(|c: char| c.is_ascii_alphabetic()) {
        format!("app_{segment}")
    } else {
        segment
    };
    format!("dev.ljos.{segment}")
}

/// The package `Cargo.toml` names and its library, as cargo metadata
/// gives them.
struct Package {
    name: String,
    lib: String,
    target_dir: PathBuf,
}

fn package(dir: &Path) -> anyhow::Result<Package> {
    let manifest = dir.join("Cargo.toml");
    let out = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--manifest-path",
        ])
        .arg(&manifest)
        .output()
        .context("running cargo metadata")?;
    if !out.status.success() {
        bail!(
            "cargo metadata for {}: {}",
            manifest.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let meta: serde_json::Value =
        serde_json::from_slice(&out.stdout).context("reading cargo metadata")?;
    let manifest = std::fs::canonicalize(&manifest).unwrap_or(manifest);
    let packages = meta["packages"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let found = packages
        .iter()
        .find(|p| p["manifest_path"].as_str().map(Path::new) == Some(manifest.as_path()))
        .with_context(|| format!("no package at {}", manifest.display()))?;
    let name = found["name"].as_str().unwrap_or_default().to_owned();
    let lib = found["targets"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|t| {
            t["crate_types"]
                .as_array()
                .is_some_and(|k| k.iter().any(|k| k == "cdylib"))
        })
        .and_then(|t| t["name"].as_str())
        .map(|n| n.replace('-', "_"))
        .with_context(|| {
            format!("{name} has no cdylib library for the activity to load ([lib] crate-type)")
        })?;
    let target_dir = PathBuf::from(meta["target_directory"].as_str().unwrap_or("target"));
    Ok(Package {
        name,
        lib,
        target_dir,
    })
}

impl Build {
    fn bundle(&self) -> anyhow::Result<Built> {
        let abis: Vec<&str> = if self.abis.is_empty() {
            ABIS.to_vec()
        } else {
            self.abis.iter().map(String::as_str).collect()
        };
        if let Some(bad) = abis.iter().find(|a| !ABIS.contains(a)) {
            bail!("no Android ABI {bad}; Ljós builds {}", ABIS.join(" and "));
        }
        let package = package(&self.path)?;
        let fill = Fill {
            application_id: self
                .application_id
                .clone()
                .unwrap_or_else(|| default_application_id(&package.name)),
            label: self.label.clone().unwrap_or_else(|| package.name.clone()),
            lib_name: package.lib.clone(),
        };
        let project = package.target_dir.join("android").join(&package.name);
        write_project(&project, &fill)?;
        let jni = project.join("app/src/main/jniLibs");
        keep_only_abis(&jni, &abis)?;
        for abi in &abis {
            let mut c = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
            c.args(["ndk", "-t", abi, "-P", &MIN_SDK.to_string(), "-o"])
                .arg(&jni)
                // Only the library the package is made of: a binary of the
                // package for the desktop need not build for a phone.
                .args(["build", "--lib", "-p", &package.name])
                .current_dir(&self.path);
            if self.release {
                c.arg("--release");
            }
            status(
                &mut c,
                "cargo ndk (cargo install cargo-ndk, and the NDK r27 or newer)",
            )?;
        }
        if let Some(sdk) = android_sdk() {
            std::fs::write(
                project.join("local.properties"),
                format!("sdk.dir={}\n", sdk.display()),
            )
            .context("writing local.properties")?;
        }
        let task = if self.release {
            "assembleRelease"
        } else {
            "assembleDebug"
        };
        let mut gradle = Command::new(gradle()?);
        gradle.arg("--no-daemon").arg("-q").arg("-p").arg(&project);
        if let Some(java) = std::env::var_os("JAVA_HOME") {
            let mut prop = std::ffi::OsString::from("-Dorg.gradle.java.home=");
            prop.push(java);
            gradle.arg(prop);
        }
        gradle.args(&self.gradle_args).arg(task);
        status(&mut gradle, "gradle")?;
        let kind = if self.release { "release" } else { "debug" };
        let apk = project
            .join("app/build/outputs/apk")
            .join(kind)
            .join(format!("app-{kind}.apk"));
        if !apk.is_file() {
            bail!("gradle made no {}", apk.display());
        }
        Ok(Built {
            apk,
            application_id: fill.application_id,
        })
    }
}

/// Removes the libraries of the ABIs not built now: the package takes
/// whatever is in `jni`, and an earlier build's library for another ABI
/// would be installed on a device of that ABI without a word.
fn keep_only_abis(jni: &Path, abis: &[&str]) -> anyhow::Result<()> {
    let entries = match std::fs::read_dir(jni) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", jni.display())),
    };
    for entry in entries {
        let entry = entry.with_context(|| format!("reading {}", jni.display()))?;
        let name = entry.file_name();
        if !abis.iter().any(|abi| name == *abi) {
            let path = entry.path();
            let removed = if entry.file_type()?.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
            removed.with_context(|| format!("removing {}", path.display()))?;
        }
    }
    Ok(())
}

/// Writes the filled template into `project`, leaving what Gradle and
/// cargo-ndk put there before.
fn write_project(project: &Path, fill: &Fill) -> anyhow::Result<()> {
    for (path, text) in TEMPLATE {
        let to = project.join(path);
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("making {}", dir.display()))?;
        }
        let text = fill.apply(text);
        // Unchanged files keep their time, so Gradle has nothing to redo.
        if std::fs::read_to_string(&to).ok().as_deref() != Some(text.as_str()) {
            std::fs::write(&to, text).with_context(|| format!("writing {}", to.display()))?;
        }
    }
    Ok(())
}

fn status(command: &mut Command, what: &str) -> anyhow::Result<()> {
    let ran = command
        .status()
        .with_context(|| format!("starting {what}"))?;
    if !ran.success() {
        bail!("{what} failed ({ran})");
    }
    Ok(())
}

/// The SDK: `ANDROID_HOME`, `ANDROID_SDK_ROOT`, else where Android Studio
/// puts it.
fn android_sdk() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    [
        std::env::var_os("ANDROID_HOME").map(PathBuf::from),
        std::env::var_os("ANDROID_SDK_ROOT").map(PathBuf::from),
        home.as_ref().map(|h| h.join("Android/Sdk")),
        home.as_ref().map(|h| h.join("Library/Android/sdk")),
    ]
    .into_iter()
    .flatten()
    .find(|d| d.is_dir())
}

/// A tool of the SDK's: on `PATH`, else in the SDK.
fn tool(name: &str, in_sdk: &str) -> anyhow::Result<PathBuf> {
    if let Some(found) = on_path(name) {
        return Ok(found);
    }
    android_sdk()
        .map(|sdk| sdk.join(in_sdk))
        .filter(|p| p.is_file())
        .with_context(|| {
            format!("no {name}: put the Android SDK's {in_sdk} on PATH or set ANDROID_HOME")
        })
}

/// Gradle: `GRADLE`, else on `PATH`.
fn gradle() -> anyhow::Result<PathBuf> {
    if let Some(g) = std::env::var_os("GRADLE").filter(|g| !g.is_empty()) {
        return Ok(PathBuf::from(g));
    }
    on_path("gradle")
        .context("no gradle: install Gradle 8.13 or newer, or set GRADLE to its bin/gradle")
}

fn on_path(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill() -> Fill {
        Fill {
            application_id: String::from("dev.ljos.hello"),
            label: String::from("Hello & \"you\""),
            lib_name: String::from("ljos_example_android_hello"),
        }
    }

    #[test]
    fn a_build_for_one_abi_packs_no_library_of_another() {
        let dir = tempfile::tempdir().unwrap();
        let jni = dir.path().join("jniLibs");
        for abi in ABIS {
            std::fs::create_dir_all(jni.join(abi)).unwrap();
            std::fs::write(jni.join(abi).join("libapp.so"), b"old").unwrap();
        }
        keep_only_abis(&jni, &["x86_64"]).unwrap();
        assert!(jni.join("x86_64/libapp.so").is_file());
        assert!(!jni.join("arm64-v8a").exists());
        // Nothing built before: nothing to remove.
        keep_only_abis(&dir.path().join("none"), &["x86_64"]).unwrap();
    }

    #[test]
    fn the_filled_template_has_no_placeholder_left() {
        for (path, text) in TEMPLATE {
            let filled = fill().apply(text);
            assert!(!filled.contains("@APPLICATION_ID@"), "{path}");
            assert!(!filled.contains("@LABEL@"), "{path}");
            assert!(!filled.contains("@LIB_NAME@"), "{path}");
        }
    }

    #[test]
    fn the_manifest_loads_the_library_and_takes_every_change() {
        let manifest = TEMPLATE
            .iter()
            .find(|(p, _)| p.ends_with("AndroidManifest.xml"))
            .map(|(_, t)| fill().apply(t))
            .unwrap();
        assert!(manifest.contains(r#"android:value="ljos_example_android_hello""#));
        assert!(manifest.contains(r#"android:label="Hello &amp; &quot;you&quot;""#));
        assert!(manifest.contains(&format!(r#"android:name="{ACTIVITY}""#)));
        for change in [
            "orientation",
            "screenSize",
            "density",
            "uiMode",
            "fontScale",
            "locale",
        ] {
            assert!(manifest.contains(change), "{change}");
        }
    }

    #[test]
    fn the_activity_hands_back_to_the_native_side() {
        let java = TEMPLATE
            .iter()
            .find(|(p, _)| p.ends_with("LjosActivity.java"))
            .map(|(_, t)| *t)
            .unwrap();
        // ljos_host's Java_dev_ljos_LjosActivity_nativeBack.
        assert!(java.contains("package dev.ljos;"));
        assert!(java.contains("class LjosActivity extends GameActivity"));
        assert!(java.contains("static native void nativeBack()"));
    }

    #[test]
    fn each_java_file_is_written_where_its_package_says() {
        let java: Vec<_> = TEMPLATE
            .iter()
            .filter(|(p, _)| p.ends_with(".java"))
            .collect();
        for (path, text) in &java {
            let package = text
                .lines()
                .find_map(|l| l.strip_prefix("package "))
                .and_then(|l| l.strip_suffix(';'))
                .unwrap();
            let dir = format!("app/src/main/java/{}/", package.replace('.', "/"));
            assert!(path.starts_with(&dir), "{path} is not in {dir}");
        }
        // LjosActivity reaches GameTextInput's connection through it, to
        // type what a dead key composes into the field being edited.
        assert!(
            java.iter()
                .any(|(p, _)| p.ends_with("com/google/androidgamesdk/TextInputOf.java"))
        );
    }

    #[test]
    fn a_package_name_becomes_an_application_id() {
        assert_eq!(
            default_application_id("android-hello"),
            "dev.ljos.android_hello"
        );
        assert_eq!(default_application_id("Ljos_App"), "dev.ljos.ljos_app");
        // Android takes a segment only if it starts with a letter; cargo
        // takes a package named `_foo`.
        assert_eq!(default_application_id("_foo"), "dev.ljos.app__foo");
        assert_eq!(default_application_id("3d"), "dev.ljos.app_3d");
    }
}
