//! Back end B runs here: the case files become Rust in `OUT_DIR`. With the
//! `scriptc` feature, back end A runs here too: scriptc compiles the
//! lowered cases to a static library this crate links.
//!
//! `AOT_CASES` (case file stems, comma separated) limits B to some cases,
//! which is how the build time of one case is taken.

#[path = "build/gen.rs"]
mod codegen;

use std::path::{Path, PathBuf};
use std::time::Instant;

fn main() {
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets it"));
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets it"));
    let cases_dir = dir.join("cases");
    println!("cargo:rerun-if-changed=build/gen.rs");
    println!("cargo:rerun-if-changed={}", cases_dir.display());
    println!("cargo:rerun-if-env-changed=AOT_CASES");
    let only: Option<Vec<String>> = std::env::var("AOT_CASES")
        .ok()
        .map(|s| s.split(',').map(str::to_owned).collect());

    let world = codegen::parse_world(&read(&cases_dir.join("world.d.ts")))
        .unwrap_or_else(|e| panic!("{e}"));
    let mut stems: Vec<String> = std::fs::read_dir(&cases_dir)
        .expect("cases/")
        .filter_map(|e| {
            let name = e.ok()?.file_name().into_string().ok()?;
            let stem = name.strip_suffix(".ts")?;
            (stem.starts_with('c') && !stem.ends_with(".d")).then(|| stem.to_owned())
        })
        .filter(|s| only.as_ref().is_none_or(|o| o.contains(s)))
        .collect();
    stems.sort();
    let cases: Vec<codegen::Case> = stems
        .iter()
        .map(|s| codegen::Case {
            stem: s.clone(),
            source: read(&cases_dir.join(format!("{s}.ts"))),
        })
        .collect();
    let output = codegen::generate(&world, &cases).unwrap_or_else(|e| panic!("back end B: {e}"));
    std::fs::write(out.join("b_cases.rs"), output.rust).expect("write b_cases.rs");
    let _ = output.timings;

    if std::env::var_os("CARGO_FEATURE_SCRIPTC").is_some() {
        scriptc(&dir, &out);
    }
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// Runs `$SCRIPTC build --lib` on `scriptc/profile.json`.
fn scriptc(dir: &Path, out: &Path) {
    println!("cargo:rerun-if-changed=scriptc");
    println!("cargo:rerun-if-env-changed=SCRIPTC");
    let scriptc = std::env::var("SCRIPTC")
        .expect("the scriptc feature needs SCRIPTC, the path of the scriptc binary");
    let lib = out.join("libaot_cases.a");
    let start = Instant::now();
    let status = std::process::Command::new(&scriptc)
        .current_dir(dir.join("scriptc"))
        .args(["build", "--lib", "--profile", "profile.json", "-o"])
        .arg(&lib)
        .status()
        .expect("run scriptc");
    assert!(status.success(), "scriptc failed");
    let ns = start.elapsed().as_nanos();
    std::fs::write(
        out.join("a_build.rs"),
        format!("pub const SCRIPTC_BUILD_NS: u64 = {ns};\n"),
    )
    .expect("write");
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=aot_cases");
}
