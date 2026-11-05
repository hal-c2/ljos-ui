//! `ljos-webengine-bundle <executable> [options]`: makes the application
//! bundle CEF needs on macOS (`<Name>.app`) around an executable that
//! shows web pages, with the framework and the helper applications
//! (`ljos_webengine::mac_bundle`). Run the executable inside it.
//!
//! - `--helper <path>`: `ljos-webengine-helper`; by default beside the
//!   executable.
//! - `--framework <path>`: CEF's `Chromium Embedded Framework.framework`;
//!   by default the one under `$CEF_PATH`.
//! - `--out <dir>`: where the bundle is made; by default beside the
//!   executable.
//! - `--id <identifier>`: `CFBundleIdentifier`; by default
//!   `org.ljos.<name>`.
//! - `--link-framework`: link the framework rather than copy it, for a
//!   bundle that stays on this machine (a build's).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ljos_webengine::mac_bundle::{Bundle, FRAMEWORK};

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(app) => {
            println!("{}", app.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("ljos-webengine-bundle: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<PathBuf, String> {
    let mut exe = None;
    let (mut helper, mut framework, mut out, mut id) = (None, None, None, None);
    let mut link = false;
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{a} needs a value"));
        match a.as_str() {
            "--helper" => helper = Some(PathBuf::from(value()?)),
            "--framework" => framework = Some(PathBuf::from(value()?)),
            "--out" => out = Some(PathBuf::from(value()?)),
            "--id" => id = Some(value()?),
            "--link-framework" => link = true,
            _ if a.starts_with("--") => return Err(format!("unknown option {a}")),
            _ => exe = Some(PathBuf::from(a)),
        }
    }
    let exe = exe.ok_or("usage: ljos-webengine-bundle <executable> [--helper P] [--framework P] [--out DIR] [--id ID] [--link-framework]")?;
    let beside = exe.parent().unwrap_or(Path::new(".")).to_path_buf();
    let helper = helper.unwrap_or_else(|| beside.join("ljos-webengine-helper"));
    let framework = match framework {
        Some(f) => f,
        None => under_cef_path()
            .ok_or_else(|| format!("no {FRAMEWORK} under $CEF_PATH; give it with --framework"))?,
    };
    let name = exe
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("the executable has no name")?
        .to_owned();
    let id = id.unwrap_or_else(|| format!("org.ljos.{name}"));
    Bundle {
        executable: &exe,
        helper: &helper,
        framework: &framework,
        out_dir: &out.unwrap_or(beside),
        identifier: &id,
        link_framework: link,
    }
    .make()
    .map_err(|e| e.to_string())
}

/// The framework of the CEF distribution the `cef` crate unpacked into
/// `$CEF_PATH` (`<version>/cef_macos_<arch>/`), or one at its top.
fn under_cef_path() -> Option<PathBuf> {
    let root = PathBuf::from(std::env::var_os("CEF_PATH")?);
    let mut dirs = vec![root];
    for _ in 0..3 {
        let mut next = Vec::new();
        for d in dirs {
            let fw = d.join(FRAMEWORK);
            if fw.is_dir() {
                return Some(fw);
            }
            if let Ok(entries) = std::fs::read_dir(&d) {
                next.extend(entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()));
            }
        }
        dirs = next;
    }
    None
}
