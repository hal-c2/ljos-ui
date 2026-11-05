//! `ljos-webengine-bundle <executable> [options]`: makes the application
//! bundle CEF needs on macOS (`<Name>.app`) around an executable that
//! shows web pages, with the framework and the helper applications
//! (`ljos_webengine::mac_bundle`). Run the executable inside it.
//!
//! - `--helper <path>`: `ljos-webengine-helper`; by default beside the
//!   executable.
//! - `--framework <path>`: CEF's `Chromium Embedded Framework.framework`;
//!   by default the one under `$CEF_PATH`, if there is only one.
//! - `--out <dir>`: where the bundle is made; by default beside the
//!   executable.
//! - `--id <identifier>`: `CFBundleIdentifier`; by default
//!   `org.ljos.<name>`, with what an identifier may not have as `-`.
//! - `--link-framework`: link the framework rather than copy it, for a
//!   bundle that stays on this machine (a build's).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ljos_webengine::mac_bundle::{Bundle, FRAMEWORK, default_identifier, framework_under};

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
        None => {
            let root = std::env::var_os("CEF_PATH").ok_or_else(|| {
                format!("no $CEF_PATH to find {FRAMEWORK} in; give it with --framework")
            })?;
            framework_under(Path::new(&root)).map_err(|e| format!("{e} with --framework"))?
        }
    };
    let name = exe
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("the executable has no name")?
        .to_owned();
    let id = id.unwrap_or_else(|| default_identifier(&name));
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
