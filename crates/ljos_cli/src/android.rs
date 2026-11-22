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
//! | `run --mcp=<caps>` | the library built with `ljos/mcp`, a debug package, and `am start -S … --es dev.ljos.MCP <caps>`: the activity hands the capabilities to the application as `LJOS_MCP`, with its files directory as `LJOS_MCP_DIR` |
//! | `connect` | `adb shell run-as <id>` lists and reads the inspector's token file, `adb forward tcp:<port> tcp:<port>` reaches its port at the same port here, the one its Host check takes, and a token file for this computer is written (ARCHITECTURE.md §19.7) |

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail};
use clap::{Args, Subcommand};
use ljos_mcp_core::CapSet;
use ljos_mcp_core::protocol::TokenFileBody;

use crate::stdout::outln;

/// The lowest Android API level Ljós runs on (ARCHITECTURE.md §10.3).
const MIN_SDK: u32 = 26;

/// The ABIs Ljós builds, as Android names them.
const ABIS: [&str; 2] = ["arm64-v8a", "x86_64"];

/// The activity the template's manifest declares.
const ACTIVITY: &str = "dev.ljos.LjosActivity";

/// The extra of the intent that starts the activity with the inspector:
/// the capabilities, as `--mcp=` takes them. LjosActivity reads it only in
/// a debuggable package.
const MCP_EXTRA: &str = "dev.ljos.MCP";

/// Where the inspector of an application on the phone writes its token
/// file, below the application's own directory (`LJOS_MCP_DIR` is its
/// files directory, and the token file goes to `target/ljos/` below it).
const TOKEN_DIR: &str = "files/target/ljos";

/// The Gradle template, by the path it is written to in the project.
const TEMPLATE: [(&str, &str); 11] = [
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
    (
        "app/src/main/java/dev/ljos/LjosServices.java",
        include_str!("../../../android/app/src/main/java/dev/ljos/LjosServices.java"),
    ),
    (
        "app/src/debug/AndroidManifest.xml",
        include_str!("../../../android/app/src/debug/AndroidManifest.xml"),
    ),
    (
        "app/src/main/res/xml/ljos_clipboard_paths.xml",
        include_str!("../../../android/app/src/main/res/xml/ljos_clipboard_paths.xml"),
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
        /// Serve the application's inspector to clients holding these
        /// capabilities (inspect, input, mutate, eval), as `ljos run --mcp`
        /// does; `ljos android connect` then reaches it. Builds the library
        /// with `ljos/mcp`; a debug package only.
        #[arg(long, value_name = "CAPS", value_parser = crate::run::parse_caps)]
        mcp: Option<CapSet>,
    },
    /// Reach the inspector of an application running on the phone, one
    /// started with `ljos android run --mcp`: forward a port of this
    /// computer to it over adb, read its token with `run-as`, and write a
    /// token file that `ljos observe`, `ljos attach` and agents take.
    Connect {
        /// The application's crate, for its id, as `run` takes it.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// The application's id, instead of the one its crate gets.
        #[arg(long)]
        application_id: Option<String>,
        /// The device (`adb -s`), when there is more than one.
        #[arg(long)]
        device: Option<String>,
        /// The token file to write, by default
        /// `target/ljos/android-<id>.json` here.
        #[arg(long)]
        out: Option<PathBuf>,
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
        AndroidCommand::Run { build, device, mcp } => {
            if mcp.is_some() && build.release {
                bail!(
                    "--mcp serves the inspector from a debug package only \
                     (ARCHITECTURE.md §19.7): leave out --release"
                );
            }
            let features: &[&str] = if mcp.is_some() { &["ljos/mcp"] } else { &[] };
            let built = build.bundle_with(features)?;
            let adb = Adb::found(device)?;
            let apk = built.apk.to_string_lossy();
            adb.run(&["install", "-r", &apk])?;
            adb.run(&start_args(&built.application_id, mcp))?;
            outln!("started {}", built.application_id);
            Ok(())
        }
        AndroidCommand::Connect {
            path,
            application_id,
            device,
            out,
        } => {
            let id = match application_id {
                Some(id) => id,
                None => default_application_id(&package(&path)?.name),
            };
            let out = out.unwrap_or_else(|| {
                PathBuf::from("target")
                    .join("ljos")
                    .join(format!("android-{id}.json"))
            });
            let url = connect(&Adb::found(device)?, &id, &out)?;
            outln!("{} at {url}, token in {}", id, out.display());
            Ok(())
        }
    }
}

/// What `adb shell am start` is given to start the application, with the
/// inspector when `mcp` asks for it. `-S` stops a running one first: the
/// application reads `LJOS_MCP` once, as its process starts.
fn start_args(application_id: &str, mcp: Option<CapSet>) -> Vec<String> {
    let mut args: Vec<String> = ["shell", "am", "start"].map(String::from).into();
    if let Some(caps) = mcp {
        let caps: Vec<String> = caps.iter().map(|c| c.to_string()).collect();
        args.extend(["-S".into(), "--es".into(), MCP_EXTRA.into(), caps.join(",")]);
    }
    args.extend(["-n".into(), format!("{application_id}/{ACTIVITY}")]);
    args
}

/// adb, talking to one device.
struct Adb {
    program: PathBuf,
    device: Option<String>,
}

impl Adb {
    /// adb: `ADB`, on `PATH`, else in the SDK.
    fn found(device: Option<String>) -> anyhow::Result<Self> {
        let program = match std::env::var_os("ADB").filter(|a| !a.is_empty()) {
            Some(adb) => PathBuf::from(adb),
            None => tool("adb", "platform-tools/adb")?,
        };
        Ok(Self { program, device })
    }

    fn command(&self, args: &[impl AsRef<std::ffi::OsStr>]) -> Command {
        let mut c = Command::new(&self.program);
        if let Some(d) = &self.device {
            c.args(["-s", d]);
        }
        c.args(args);
        c
    }

    fn run(&self, args: &[impl AsRef<std::ffi::OsStr>]) -> anyhow::Result<()> {
        status(&mut self.command(args), "adb")
    }

    /// What adb printed, without the carriage returns a device's shell
    /// puts before each newline.
    fn output(&self, args: &[&str]) -> anyhow::Result<String> {
        let out = self.command(args).output().context("starting adb")?;
        if !out.status.success() {
            bail!(
                "adb {}: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).replace('\r', ""))
    }
}

/// Reaches the inspector of `application_id` on the device and writes a
/// token file for this computer to `out`; returns the address there.
fn connect(adb: &Adb, application_id: &str, out: &Path) -> anyhow::Result<String> {
    let run_as = |args: &[&str]| {
        let mut all = vec!["shell", "run-as", application_id];
        all.extend_from_slice(args);
        adb.output(&all)
    };
    const START: &str = "start it with `ljos android run --mcp=inspect` \
                         (a debug package; run-as reads only those)";
    let listed = run_as(&["ls", "-t", TOKEN_DIR])
        .with_context(|| format!("no inspector of {application_id} on the device: {START}"))?;
    let names: Vec<&str> = listed
        .lines()
        .map(str::trim)
        // Only an inspector's name: the application names the files, and
        // a name reaches the device's shell as it is.
        .filter(|n| {
            n.strip_prefix("mcp-")
                .and_then(|n| n.strip_suffix(".json"))
                .is_some_and(|pid| !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()))
        })
        .collect();
    if names.is_empty() {
        bail!("{application_id} serves no inspector: {START}");
    }
    // An inspector removes its file when it stops, but one the system
    // killed (`am start -S` kills the last run) leaves it: only the file of
    // the process running now is the inspector's.
    // What runs is asked before anything is removed: a file is removed
    // only when `pidof` answered and its process is not among them.
    let running = running(adb, application_id)?;
    let mut found = None;
    for name in names {
        let path = format!("{TOKEN_DIR}/{name}");
        let text = run_as(&["cat", &path])?;
        let body =
            TokenFileBody::parse(&text).with_context(|| format!("{path} of {application_id}"))?;
        if body.pid.is_none_or(|pid| running.contains(&pid)) {
            found = Some(body);
            break;
        }
        // Left by a process that is gone: no one else removes it.
        let _ = run_as(&["rm", "-f", &path]);
    }
    let Some(body) = found else {
        bail!("the inspector of {application_id} is not running: {START}");
    };
    let (port, rest) = loopback_port(&body.url)?;
    // The inspector answers only a request whose Host names its own port,
    // and a client names the port of the address it was given: the port
    // here is the device's.
    let forward = format!("tcp:{port}");
    let listed = device_forwards(adb);
    let goes = |local: &str| {
        listed
            .iter()
            .find(|(here, _)| here == local)
            .map(|(_, there)| there.as_str())
    };
    // The last connect's forward, while adb still lists it as this
    // device's and going where that connect made it go: after adb
    // restarted, or once rebound, the port may be another's forward, and
    // `--remove` names the port alone.
    let last = last_port(out).filter(|old| {
        let local = format!("tcp:{old}");
        goes(&local) == Some(local.as_str())
    });
    let made = if goes(&forward) == Some(forward.as_str()) {
        // One reaches it already: the last connect's, even when its token
        // file went with `target/`.
        false
    } else {
        // Over the last connect's own forward at this port only: a port
        // that is not this device's forward is another's.
        let mut args = vec!["forward"];
        if last != Some(port) {
            args.push("--no-rebind");
        }
        args.extend([forward.as_str(), forward.as_str()]);
        adb.output(&args).with_context(|| {
            format!(
                "port {port} of this computer could not be forwarded to the inspector: \
                 the inspector answers only at its own port, so that port must be free here"
            )
        })?;
        true
    };
    let url = format!("http://127.0.0.1:{port}{rest}");
    // The process is the phone's, not one of this computer's.
    let here = TokenFileBody {
        url: url.clone(),
        pid: None,
        ..body
    };
    // Whole, renamed into place: `attach` reads it again for every
    // message, and must not find it half written.
    if let Err(e) = ljos_mcp_core::write_token_text(out, &here.to_text()) {
        // No file records this forward, so no later connect would remove
        // it: each retry would leave one more. The last connect's at
        // another port, which its file still names, stays; one at this
        // port the new forward has replaced already.
        if made {
            let _ = adb.output(&["forward", "--remove", &forward]);
        }
        return Err(e.into());
    }
    // Nothing names the last connect's forward now, and nothing else
    // removes it: adb keeps it until it restarts. Unless another
    // application's connect took it: its inspector got the port on the
    // device the forward goes to.
    if let Some(old) = last.filter(|old| *old != port && !named_beside(out, *old)) {
        let _ = adb.output(&["forward", "--remove", &format!("tcp:{old}")]);
    }
    Ok(url)
}

/// The port here of the forward the last connect to `out` made.
fn last_port(out: &Path) -> Option<u16> {
    std::fs::read_to_string(out)
        .ok()
        .and_then(|text| TokenFileBody::parse(&text).ok())
        .and_then(|last| loopback_port(&last.url).ok().map(|(p, _)| p))
}

/// Whether a token file of another application's connect, beside `out`,
/// names `port` here.
fn named_beside(out: &Path, port: u16) -> bool {
    let Some(entries) = out.parent().and_then(|dir| std::fs::read_dir(dir).ok()) else {
        return false;
    };
    entries.flatten().map(|entry| entry.path()).any(|path| {
        path != out
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("android-") && name.ends_with(".json"))
            && last_port(&path) == Some(port)
    })
}

/// The forwards adb holds for this device, by their port here and where
/// they go on it (`tcp:<port>` both); none when adb does not say.
fn device_forwards(adb: &Adb) -> Vec<(String, String)> {
    let serial = match &adb.device {
        Some(device) => Some(device.clone()),
        None => adb
            .output(&["get-serialno"])
            .ok()
            .map(|s| s.trim().to_owned()),
    };
    let Some(serial) = serial.filter(|s| !s.is_empty()) else {
        return Vec::new();
    };
    let Ok(listed) = adb.output(&["forward", "--list"]) else {
        return Vec::new();
    };
    listed
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            (fields.next() == Some(serial.as_str()))
                .then(|| Some((fields.next()?.to_owned(), fields.next()?.to_owned())))
                .flatten()
        })
        .collect()
}

/// The processes of `application_id` on the device. `pidof` exits 1 when
/// none runs; anything else it or adb says is an error, not "none".
fn running(adb: &Adb, application_id: &str) -> anyhow::Result<Vec<u32>> {
    let asked = format!("pidof {application_id}; echo pidof-exit:$?");
    let out = adb
        .output(&["shell", &asked])
        .context("asking the device which processes run")?;
    let (pids, status) = out
        .trim_end()
        .rsplit_once("pidof-exit:")
        .unwrap_or((out.as_str(), ""));
    match status.trim() {
        "0" => Ok(pids
            .split_whitespace()
            .filter_map(|p| p.parse().ok())
            .collect()),
        "1" => Ok(Vec::new()),
        _ => bail!(
            "the device's pidof did not say which processes run: {}",
            out.trim()
        ),
    }
}

/// The port of an inspector's address on the device's loopback, and what
/// follows it.
fn loopback_port(url: &str) -> anyhow::Result<(u16, &str)> {
    let not_reachable = || {
        anyhow::anyhow!(
            "the inspector on the device is at {url}: adb forwards only a port of the \
             device's loopback address"
        )
    };
    let after = url.strip_prefix("http://").ok_or_else(not_reachable)?;
    let (authority, rest) = after.find('/').map_or((after, ""), |i| after.split_at(i));
    let (host, port) = authority.rsplit_once(':').ok_or_else(not_reachable)?;
    if !matches!(host, "127.0.0.1" | "localhost" | "[::1]") {
        return Err(not_reachable());
    }
    Ok((port.parse().map_err(|_| not_reachable())?, rest))
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
        self.bundle_with(&[])
    }

    /// As [`bundle`](Self::bundle), with the library built with
    /// `features` too (`ljos/mcp`).
    fn bundle_with(&self, features: &[&str]) -> anyhow::Result<Built> {
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
            if !features.is_empty() {
                c.args(["--features", &features.join(",")]);
            }
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
    fn the_activity_takes_where_the_text_is_as_the_native_side_passes_it() {
        let java = |name: &str| {
            TEMPLATE
                .iter()
                .find(|(p, _)| p.ends_with(name))
                .map(|(_, t)| *t)
                .unwrap()
        };
        // ljos_host's `cursorAnchorChanged`, `([F[FIIILjava/lang/String;[F)V`.
        let activity: String = java("LjosActivity.java").split_whitespace().collect();
        assert!(activity.contains(
            "publicvoidcursorAnchorChanged(float[]editor,float[]cursor,intselStart,intselEnd,intcomposeStart,Stringcomposing,float[]chars)"
        ));
        // `cursorAnchorCleared`, `()V`.
        assert!(activity.contains("publicvoidcursorAnchorCleared()"));
        assert!(activity.contains("TextInputOf.surface(this,"));
        // GameTextInput's own connection refuses an input method that asks.
        assert!(java("TextInputOf.java").contains("public boolean requestCursorUpdates(int mode)"));
    }

    #[test]
    fn a_keyboard_that_moves_the_cursor_tells_the_native_side() {
        let java = |name: &str| {
            TEMPLATE
                .iter()
                .find(|(p, _)| p.ends_with(name))
                .map(|(_, t)| t.split_whitespace().collect::<String>())
                .unwrap()
        };
        // GameTextInput's own setSelection and setComposingRegion change
        // only its Editable, so a keyboard moving the cursor by itself (a
        // space-bar swipe) would never reach the field.
        let of = java("TextInputOf.java");
        assert!(of.contains(
            "publicbooleansetSelection(intstart,intend){booleanset=super.setSelection(start,end);tell();returnset;}"
        ));
        assert!(of.contains(
            "publicbooleansetComposingRegion(intstart,intend){composing=true;booleanset=super.setComposingRegion(start,end);composing=false;tell();returnset;}"
        ));
        // Told once the call is done, unless it was GameTextInput setting
        // the native side's own state: its final setState sets the cursor
        // through those same two calls, then restarts the input, and an
        // echo of that state arriving late would undo a newer edit.
        // Ahead of the keyboard's next call, which may be a key for the
        // cursor it just moved.
        assert!(of.contains(
            "privatevoidtell(){if(!moved){moved=true;main.postAtFrontOfQueue(this::told);}}"
        ));
        assert!(of.contains("privatevoidtold(){if(moved){endBatchEdit();}}"));
        assert!(of.contains("publicvoidrestartInput(){moved=false;super.restartInput();}"));
        // BaseInputConnection sets a composing region inside a batch, whose
        // end would report a setState before its restart cancels that.
        assert!(of.contains(
            "publicbooleanendBatchEdit(){if(composing){returnfalse;}moved=false;returnsuper.endBatchEdit();}"
        ));
        let activity = java("LjosActivity.java");
        assert!(activity.contains("TextInputOf.surface(this,"));
        // The native side writes through GameActivity_setTextInputState
        // straight to the connection; GameActivity's Java one is not on
        // that path.
        assert!(!activity.contains("setTextInputState"));
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
    fn the_services_are_what_the_native_side_calls() {
        let java = TEMPLATE
            .iter()
            .find(|(p, _)| p.ends_with("LjosServices.java"))
            .map(|(_, t)| *t)
            .unwrap();
        // ljos_jni::SERVICES, and the methods ljos_platform and
        // ljos_secrets call by name and signature.
        assert!(java.contains("package dev.ljos;"));
        assert!(java.contains("public final class LjosServices"));
        for method in [
            "public static String clipboardText(Context context)",
            "public static void setClipboardText(Context context, String text)",
            "public static void clearClipboard(Context context)",
            "public static boolean openUrl(Context context, String url)",
            "public static String filesDir(Context context)",
            "public static String cacheDir(Context context)",
            "public static void secretWrite(Context context, String service, String key, byte[] secret)",
            "public static byte[] secretRead(Context context, String service, String key)",
            "public static boolean secretRemove(Context context, String service, String key)",
            "public static String clipboardHtml(Context context)",
            "public static void setClipboardHtml(Context context, String html, String text)",
            "public static int[] clipboardImage(Context context)",
            "public static void setClipboardImage(Context context, int[] argb, int width, int height)",
        ] {
            assert!(java.contains(method), "{method}");
        }
    }

    #[test]
    fn the_inspector_is_asked_for_by_the_intent() {
        let plain = start_args("dev.ljos.hello", None);
        assert_eq!(
            plain,
            [
                "shell",
                "am",
                "start",
                "-n",
                "dev.ljos.hello/dev.ljos.LjosActivity"
            ]
        );
        let caps = CapSet::parse_list("input,inspect").unwrap();
        let served = start_args("dev.ljos.hello", Some(caps));
        assert_eq!(
            served,
            [
                "shell",
                "am",
                "start",
                "-S",
                "--es",
                "dev.ljos.MCP",
                "inspect,input",
                "-n",
                "dev.ljos.hello/dev.ljos.LjosActivity"
            ]
        );
        let java = TEMPLATE
            .iter()
            .find(|(p, _)| p.ends_with("LjosActivity.java"))
            .map(|(_, t)| *t)
            .unwrap();
        assert!(java.contains(&format!("\"{MCP_EXTRA}\"")));
        assert!(java.contains("FLAG_DEBUGGABLE"));
        // Before super.onCreate, which starts the native side.
        let serve = java.find("serveInspector();").unwrap();
        assert!(serve < java.find("super.onCreate(").unwrap());
        let debug = TEMPLATE
            .iter()
            .find(|(p, _)| *p == "app/src/debug/AndroidManifest.xml")
            .map(|(_, t)| *t)
            .unwrap();
        assert!(debug.contains("android.permission.INTERNET"));
    }

    #[test]
    fn only_the_device_loopback_is_forwarded() {
        assert_eq!(
            loopback_port("http://127.0.0.1:4410/mcp").unwrap(),
            (4410, "/mcp")
        );
        assert_eq!(loopback_port("http://localhost:9").unwrap(), (9, ""));
        assert!(loopback_port("http://10.0.2.15:4410/mcp").is_err());
        assert!(loopback_port("unix:/data/x.sock").is_err());
        assert!(loopback_port("http://127.0.0.1/mcp").is_err());
    }

    /// An adb that answers as one device whose application runs as process
    /// 4321 and serves its inspector at port 4410 would, and logs what it
    /// was asked. `stale` is a token file a killed process 1200 left,
    /// listed first.
    #[cfg(unix)]
    fn fake_adb(dir: &Path, token_file: &str, stale: Option<&str>) -> PathBuf {
        fake_adb_writing(dir, token_file, stale).0
    }

    /// [`fake_adb`], and every file this process opened for writing on
    /// the way: a test thread that forks meanwhile holds those open in
    /// its child until the child's `exec`.
    #[cfg(unix)]
    fn fake_adb_writing(
        dir: &Path,
        token_file: &str,
        stale: Option<&str>,
    ) -> (PathBuf, Vec<PathBuf>) {
        let mut written = Vec::new();
        let mut write = |name: &str, text: &str| {
            let path = dir.join(name);
            std::fs::write(&path, text).unwrap();
            written.push(path.clone());
            path
        };
        write("token.json", token_file);
        write("stale.json", stale.unwrap_or(""));
        write("pidof", "printf '4321\\r\\npidof-exit:0\\r\\n'\n");
        write("forwards", "");
        // The application names its files: one that is no inspector's
        // would run what it names in the device's shell.
        let listed = if stale.is_some() {
            r"mcp-1;id;.json\r\nmcp-1200.json\r\nmcp-4321.json\r\n"
        } else {
            r"mcp-1;id;.json\r\nmcp-4321.json\r\nmcp-1200.json\r\n"
        };
        let script = format!(
            r#"#!/bin/sh
echo "$*" >> '{log}'
case "$*" in
  "-s phone shell run-as dev.ljos.hello ls -t files/target/ljos")
    printf '{listed}' ;;
  "-s phone shell run-as dev.ljos.hello cat files/target/ljos/mcp-4321.json")
    cat '{token}' ;;
  "-s phone shell run-as dev.ljos.hello cat files/target/ljos/mcp-1200.json")
    cat '{stale}' ;;
  "-s phone shell run-as dev.ljos.hello rm -f files/target/ljos/mcp-1200.json") ;;
  "-s phone shell pidof dev.ljos.hello; echo pidof-exit:\$?")
    . '{pidof}' ;;
  "-s phone forward --no-rebind tcp:4410 tcp:4410"|"-s phone forward tcp:4410 tcp:4410")
    [ -e '{taken}' ] && {{ echo "adb: error: cannot bind listener: Address already in use" >&2; exit 1; }}
    echo "phone tcp:4410 tcp:4410" >> '{forwards}' ;;
  "-s phone forward --remove tcp:4410"|"-s phone forward --remove tcp:39001") ;;
  "-s phone forward --list")
    cat '{forwards}' ;;
  *) echo "adb: unexpected $*" >&2; exit 1 ;;
esac
"#,
            log = dir.join("log").display(),
            token = dir.join("token.json").display(),
            stale = dir.join("stale.json").display(),
            pidof = dir.join("pidof").display(),
            forwards = dir.join("forwards").display(),
            taken = dir.join("taken").display(),
        );
        // The script this process writes is only read. What runs is a copy
        // a child process made and closed, a file no descriptor of this
        // process ever pointed at, so no fork can keep it busy. Renaming
        // would not do: the name changes, the open file is the same.
        let source = write("adb.sh", &script);
        let adb = dir.join("adb");
        let installed = std::process::Command::new("install")
            .args(["-m", "755"])
            .arg(&source)
            .arg(&adb)
            .status()
            .unwrap();
        assert!(installed.success(), "install the fake adb: {installed}");
        (adb, written)
    }

    /// The race behind #718, made certain: another test thread forked
    /// while `fake_adb` wrote, and its child still holds what it wrote.
    #[cfg(unix)]
    #[test]
    fn a_fake_adb_runs_while_a_forked_child_holds_what_was_written() {
        let dir = tempfile::tempdir().unwrap();
        let (program, written) = fake_adb_writing(dir.path(), "", None);
        let _held: Vec<_> = written
            .iter()
            .map(|p| std::fs::OpenOptions::new().append(true).open(p).unwrap())
            .collect();
        let adb = Adb {
            program,
            device: Some("phone".into()),
        };
        assert_eq!(adb.output(&["forward", "--list"]).unwrap(), "");
    }

    fn on_phone(pid: u32, port: u16) -> TokenFileBody {
        TokenFileBody {
            url: format!("http://127.0.0.1:{port}/mcp"),
            token: "ab".repeat(32),
            pid: Some(pid),
            capabilities: CapSet::parse_list("inspect").unwrap(),
            calls_per_second: Some(50),
        }
    }

    #[cfg(unix)]
    #[test]
    fn connect_forwards_a_port_and_writes_a_token_file_here() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let on_phone = on_phone(4321, 4410);
        let adb = Adb {
            program: fake_adb(dir.path(), &on_phone.to_text(), None),
            device: Some("phone".into()),
        };
        let out = dir.path().join("target/ljos/android-dev.ljos.hello.json");
        let url = connect(&adb, "dev.ljos.hello", &out).unwrap();
        assert_eq!(url, "http://127.0.0.1:4410/mcp");
        let written = TokenFileBody::parse(&std::fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(written.url, url);
        assert_eq!(written.token, on_phone.token);
        assert_eq!(written.capabilities, on_phone.capabilities);
        assert_eq!(written.calls_per_second, Some(50));
        // The phone's process is no process of this computer.
        assert_eq!(written.pid, None);
        let mode = std::fs::metadata(&out).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        // Its directory is the owner's alone; the one above it is not
        // made so.
        let mode = |d: &str| {
            std::fs::metadata(dir.path().join(d))
                .unwrap()
                .permissions()
                .mode()
                & 0o777
        };
        assert_eq!(mode("target/ljos"), 0o700);
        // As any directory is made under this umask.
        std::fs::create_dir(dir.path().join("plain")).unwrap();
        assert_eq!(mode("target"), mode("plain"));
    }

    /// A token file here that a connect wrote when the inspector's port on
    /// this computer was 39001, in a directory that is its owner's alone
    /// unless it is there already, as connect makes it.
    #[cfg(unix)]
    fn connected_before(out: &Path) {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(out.parent().unwrap())
            .unwrap();
        let here = TokenFileBody {
            pid: None,
            ..on_phone(0, 39001)
        };
        std::fs::write(out, here.to_text()).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn connect_again_keeps_the_forward_the_last_connect_made() {
        let dir = tempfile::tempdir().unwrap();
        let adb = Adb {
            program: fake_adb(dir.path(), &on_phone(4321, 4410).to_text(), None),
            device: Some("phone".into()),
        };
        let out = dir.path().join("target/ljos/android-dev.ljos.hello.json");
        connect(&adb, "dev.ljos.hello", &out).unwrap();
        connect(&adb, "dev.ljos.hello", &out).unwrap();
        let log = std::fs::read_to_string(dir.path().join("log")).unwrap();
        // The same port of the device, at the same port here: one forward.
        assert_eq!(log.matches("forward --no-rebind").count(), 1, "{log}");
        assert!(!log.contains("--remove"), "{log}");
    }

    #[cfg(unix)]
    #[test]
    fn connect_takes_a_forward_that_reaches_the_inspector_already() {
        let dir = tempfile::tempdir().unwrap();
        let adb = Adb {
            program: fake_adb(dir.path(), &on_phone(4321, 4410).to_text(), None),
            device: Some("phone".into()),
        };
        // Its token file is gone with `target/`, but adb, not restarted
        // since, keeps the forward the last connect made.
        std::fs::write(dir.path().join("forwards"), "phone tcp:4410 tcp:4410\n").unwrap();
        std::fs::write(dir.path().join("taken"), "").unwrap();
        let out = dir.path().join("target/ljos/android-dev.ljos.hello.json");
        let url = connect(&adb, "dev.ljos.hello", &out).unwrap();
        assert_eq!(url, "http://127.0.0.1:4410/mcp");
        let log = std::fs::read_to_string(dir.path().join("log")).unwrap();
        assert!(
            !log.contains("forward tcp:") && !log.contains("--no-rebind"),
            "{log}"
        );
        assert!(!log.contains("--remove"), "{log}");
    }

    #[cfg(unix)]
    #[test]
    fn connect_removes_the_last_connects_forward_once_its_own_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let adb = Adb {
            program: fake_adb(dir.path(), &on_phone(4321, 4410).to_text(), None),
            device: Some("phone".into()),
        };
        let out = dir.path().join("target/ljos/android-dev.ljos.hello.json");
        connected_before(&out);
        std::fs::write(dir.path().join("forwards"), "phone tcp:39001 tcp:39001\n").unwrap();
        connect(&adb, "dev.ljos.hello", &out).unwrap();
        let log = std::fs::read_to_string(dir.path().join("log")).unwrap();
        let made = log.find("forward --no-rebind").expect(&log);
        let removed = log.find("forward --remove tcp:39001").expect(&log);
        assert!(made < removed, "{log}");
    }

    #[cfg(unix)]
    #[test]
    fn connect_keeps_the_last_connects_forward_when_it_fails() {
        let dir = tempfile::tempdir().unwrap();
        let adb = Adb {
            program: fake_adb(dir.path(), &on_phone(4321, 4410).to_text(), None),
            device: Some("phone".into()),
        };
        let out = dir.path().join("target/ljos/android-dev.ljos.hello.json");
        connected_before(&out);
        std::fs::write(dir.path().join("forwards"), "phone tcp:39001 tcp:39001\n").unwrap();
        // The inspector's port is another's on this computer.
        std::fs::write(dir.path().join("taken"), "").unwrap();
        let err = connect(&adb, "dev.ljos.hello", &out).unwrap_err();
        assert!(format!("{err:#}").contains("4410"), "{err:#}");
        let log = std::fs::read_to_string(dir.path().join("log")).unwrap();
        assert!(!log.contains("--remove"), "{log}");
        // The token file still names the forward that works.
        let kept = TokenFileBody::parse(&std::fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(kept.url, "http://127.0.0.1:39001/mcp");
    }

    #[cfg(unix)]
    #[test]
    fn connect_leaves_a_forward_adb_now_holds_for_another_device() {
        let dir = tempfile::tempdir().unwrap();
        let adb = Adb {
            program: fake_adb(dir.path(), &on_phone(4321, 4410).to_text(), None),
            device: Some("phone".into()),
        };
        let out = dir.path().join("target/ljos/android-dev.ljos.hello.json");
        connected_before(&out);
        // adb restarted since, and the port is another device's forward.
        std::fs::write(dir.path().join("forwards"), "tablet tcp:39001 tcp:5000\n").unwrap();
        connect(&adb, "dev.ljos.hello", &out).unwrap();
        let log = std::fs::read_to_string(dir.path().join("log")).unwrap();
        assert!(!log.contains("--remove"), "{log}");
    }

    #[cfg(unix)]
    #[test]
    fn connect_leaves_the_last_connects_port_once_it_goes_elsewhere() {
        let dir = tempfile::tempdir().unwrap();
        let adb = Adb {
            program: fake_adb(dir.path(), &on_phone(4321, 4410).to_text(), None),
            device: Some("phone".into()),
        };
        let out = dir.path().join("target/ljos/android-dev.ljos.hello.json");
        connected_before(&out);
        // Rebound since to another of the device's ports: not the forward
        // the last connect made.
        std::fs::write(dir.path().join("forwards"), "phone tcp:39001 tcp:5000\n").unwrap();
        connect(&adb, "dev.ljos.hello", &out).unwrap();
        let log = std::fs::read_to_string(dir.path().join("log")).unwrap();
        assert!(!log.contains("--remove"), "{log}");
    }

    #[cfg(unix)]
    #[test]
    fn connect_leaves_the_last_connects_forward_to_another_application_that_took_it() {
        let dir = tempfile::tempdir().unwrap();
        let adb = Adb {
            program: fake_adb(dir.path(), &on_phone(4321, 4410).to_text(), None),
            device: Some("phone".into()),
        };
        let out = dir.path().join("target/ljos/android-dev.ljos.hello.json");
        connected_before(&out);
        std::fs::write(dir.path().join("forwards"), "phone tcp:39001 tcp:39001\n").unwrap();
        // The inspector the forward reached stopped, another application's
        // took its port on the device, and that one's connect took the
        // forward as it was.
        connected_before(&dir.path().join("target/ljos/android-dev.ljos.other.json"));
        connect(&adb, "dev.ljos.hello", &out).unwrap();
        let log = std::fs::read_to_string(dir.path().join("log")).unwrap();
        assert!(!log.contains("--remove"), "{log}");
    }

    #[cfg(unix)]
    #[test]
    fn connect_replaces_a_link_at_its_token_file_and_writes_nothing_through_it() {
        let dir = tempfile::tempdir().unwrap();
        let adb = Adb {
            program: fake_adb(dir.path(), &on_phone(4321, 4410).to_text(), None),
            device: Some("phone".into()),
        };
        let out = dir.path().join("target/ljos/android-dev.ljos.hello.json");
        connect(&adb, "dev.ljos.hello", &out).unwrap();
        let victim = dir.path().join("victim");
        std::fs::write(&victim, "keep").unwrap();
        std::fs::remove_file(&out).unwrap();
        std::os::unix::fs::symlink(&victim, &out).unwrap();
        connect(&adb, "dev.ljos.hello", &out).unwrap();
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep");
        assert!(!std::fs::symlink_metadata(&out).unwrap().is_symlink());
        TokenFileBody::parse(&std::fs::read_to_string(&out).unwrap()).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn connect_refuses_a_token_directory_others_can_read() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let open = dir.path().join("open");
        std::fs::create_dir(&open).unwrap();
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o755)).unwrap();
        let adb = Adb {
            program: fake_adb(dir.path(), &on_phone(4321, 4410).to_text(), None),
            device: Some("phone".into()),
        };
        let out = open.join("out.json");
        connected_before(&out);
        std::fs::write(dir.path().join("forwards"), "phone tcp:39001 tcp:39001\n").unwrap();
        let err = connect(&adb, "dev.ljos.hello", &out).unwrap_err();
        assert!(
            format!("{err:#}").contains(&open.display().to_string()),
            "{err:#}"
        );
        // No file records the forward it made, so it is removed now: no
        // later connect would. The last connect's, which its file still
        // names, stays.
        let log = std::fs::read_to_string(dir.path().join("log")).unwrap();
        assert!(log.contains("forward --remove tcp:4410"), "{log}");
        assert!(!log.contains("forward --remove tcp:39001"), "{log}");
        let kept = TokenFileBody::parse(&std::fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(kept.url, "http://127.0.0.1:39001/mcp");
    }

    #[cfg(unix)]
    #[test]
    fn connect_removes_nothing_when_it_cannot_tell_what_runs() {
        let dir = tempfile::tempdir().unwrap();
        let stale = on_phone(1200, 4999).to_text();
        let adb = Adb {
            program: fake_adb(dir.path(), &on_phone(4321, 4410).to_text(), Some(&stale)),
            device: Some("phone".into()),
        };
        // The device has no pidof.
        std::fs::write(
            dir.path().join("pidof"),
            "printf 'sh: pidof: not found\\r\\npidof-exit:127\\r\\n'\n",
        )
        .unwrap();
        let err = connect(&adb, "dev.ljos.hello", &dir.path().join("out.json")).unwrap_err();
        assert!(format!("{err:#}").contains("pidof"), "{err:#}");
        let log = std::fs::read_to_string(dir.path().join("log")).unwrap();
        assert!(!log.contains("rm -f"), "{log}");
        // Nor when adb itself fails.
        std::fs::write(
            dir.path().join("pidof"),
            "echo 'adb: device offline' >&2; exit 1\n",
        )
        .unwrap();
        let err = connect(&adb, "dev.ljos.hello", &dir.path().join("out.json")).unwrap_err();
        assert!(format!("{err:#}").contains("offline"), "{err:#}");
        let log = std::fs::read_to_string(dir.path().join("log")).unwrap();
        assert!(!log.contains("rm -f"), "{log}");
    }

    #[cfg(unix)]
    #[test]
    fn connect_says_how_to_start_an_inspector_when_there_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let adb = Adb {
            program: fake_adb(dir.path(), "", None),
            device: Some("phone".into()),
        };
        let err = connect(&adb, "dev.ljos.other", &dir.path().join("out.json")).unwrap_err();
        assert!(
            format!("{err:#}").contains("ljos android run --mcp"),
            "{err:#}"
        );
        assert!(!dir.path().join("out.json").exists());
    }

    #[cfg(unix)]
    #[test]
    fn connect_passes_over_and_removes_what_a_killed_process_left() {
        let dir = tempfile::tempdir().unwrap();
        // The killed process listened at another port.
        let stale = on_phone(1200, 4999).to_text();
        let adb = Adb {
            program: fake_adb(dir.path(), &on_phone(4321, 4410).to_text(), Some(&stale)),
            device: Some("phone".into()),
        };
        // In a directory connect makes its owner's alone, as a token's is.
        let out = dir.path().join("target/ljos/out.json");
        let url = connect(&adb, "dev.ljos.hello", &out).unwrap();
        assert_eq!(url, "http://127.0.0.1:4410/mcp");
        let log = std::fs::read_to_string(dir.path().join("log")).unwrap();
        assert!(
            log.contains("rm -f files/target/ljos/mcp-1200.json"),
            "{log}"
        );
        assert!(!log.contains("tcp:4999"), "{log}");
    }

    #[cfg(unix)]
    #[test]
    fn connect_says_an_application_is_not_running_when_only_its_files_are_left() {
        let dir = tempfile::tempdir().unwrap();
        // Both files were left by the killed process 1200; 4321 serves none.
        let stale = on_phone(1200, 4999).to_text();
        let adb = Adb {
            program: fake_adb(dir.path(), &stale, Some(&stale)),
            device: Some("phone".into()),
        };
        let out = dir.path().join("out.json");
        let err = connect(&adb, "dev.ljos.hello", &out).unwrap_err();
        assert!(format!("{err:#}").contains("not running"), "{err:#}");
        assert!(!out.exists());
    }

    #[test]
    fn run_refuses_capabilities_of_the_authoring_server() {
        use clap::Parser as _;
        #[derive(clap::Parser)]
        struct Line {
            #[command(subcommand)]
            android: AndroidCommand,
        }
        let parse = |caps: &str| Line::try_parse_from(["ljos-android", "run", caps]).map(|_| ());
        assert!(parse("--mcp=inspect,input").is_ok());
        for caps in ["--mcp=fs", "--mcp=run", "--mcp=inspect,fs"] {
            assert!(parse(caps).is_err(), "{caps} is refused");
        }
    }

    #[test]
    fn a_copied_picture_is_lent_by_the_templates_provider() {
        let file = |name: &str| {
            TEMPLATE
                .iter()
                .find(|(p, _)| p.ends_with(name))
                .map(|(_, t)| fill().apply(t))
                .unwrap_or_else(|| panic!("the template has no {name}"))
        };
        let manifest = file("main/AndroidManifest.xml");
        let services = file("LjosServices.java");
        let paths = file("res/xml/ljos_clipboard_paths.xml");
        // The authority LjosServices asks FileProvider for is the one the
        // manifest declares, which Gradle fills with the application's id.
        assert!(services.contains(r#"context.getPackageName() + ".ljos.clipboard""#));
        assert!(manifest.contains(r#"android:authorities="${applicationId}.ljos.clipboard""#));
        assert!(manifest.contains(r#"android:resource="@xml/ljos_clipboard_paths""#));
        assert!(manifest.contains(r#"android:exported="false""#));
        // It lends the directory the pictures are written to, and only it.
        assert!(services.contains(r#"new File(context.getCacheDir(), "ljos-clipboard")"#));
        assert!(paths.contains(r#"<cache-path name="clipboard" path="ljos-clipboard/" />"#));
        assert_eq!(paths.matches("-path ").count(), 1);
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
