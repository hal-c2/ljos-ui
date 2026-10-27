//! The command line, end to end, with synthetic pictures and a shell
//! one-liner for a renderer.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use qml_compare::image::Rgba;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_qml-compare"))
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn flat(shade: u8) -> Rgba {
    Rgba::new(8, 8, [shade, shade, shade, 255].repeat(64)).unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn diff_prints_json_and_writes_the_diff() {
    let dir = scratch("cli_diff");
    flat(255).write(&dir.join("r.png")).unwrap();
    flat(0).write(&dir.join("a.png")).unwrap();
    let o = bin()
        .args(["diff"])
        .arg(dir.join("r.png"))
        .arg(dir.join("a.png"))
        .arg("--out")
        .arg(dir.join("d.png"))
        .output()
        .unwrap();
    assert!(o.status.success());
    let v: serde_json::Value = serde_json::from_str(&stdout(&o)).unwrap();
    assert_eq!(v["pixels"], 0.0);
    assert!(dir.join("d.png").is_file());
}

#[test]
fn diff_of_different_sizes_reports_a_mismatch() {
    let dir = scratch("cli_size");
    flat(0).write(&dir.join("a.png")).unwrap();
    Rgba::new(2, 2, vec![0; 16])
        .unwrap()
        .write(&dir.join("r.png"))
        .unwrap();
    let o = bin()
        .arg("diff")
        .arg(dir.join("r.png"))
        .arg(dir.join("a.png"))
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(1));
    assert!(stdout(&o).contains("mismatch"));
}

/// A corpus of three examples: `good` renders like its reference, `bad`
/// renders the wrong colour, `broken` makes the renderer fail.
fn corpus(dir: &Path) -> PathBuf {
    let corpus = dir.join("corpus");
    std::fs::create_dir_all(corpus.join("reference")).unwrap();
    for id in ["good", "bad", "broken"] {
        flat(255)
            .write(&corpus.join("reference").join(format!("{id}.png")))
            .unwrap();
    }
    std::fs::write(
        corpus.join("reference/reference.json"),
        r#"{"good": {"capturedAt": "2026-01-02T03:04:05Z"}}"#,
    )
    .unwrap();
    std::fs::write(
        corpus.join("examples.json"),
        r#"{"root":"ex","examples":[
          {"id":"good","dir":"good","entry":"Main.qml","window":{"width":8,"height":8},"volatile":"time","qt":{"runnable":true,"settle":50}},
          {"id":"bad","dir":"bad","entry":"Main.qml","window":{"width":8,"height":8}},
          {"id":"broken","dir":"broken","entry":"Main.qml","window":{"width":8,"height":8}}
        ]}"#,
    )
    .unwrap();
    corpus.join("examples.json")
}

/// Writes a flat picture; the shade and failure come from the example id.
/// It also proves the stdin JSON and env reach the renderer.
fn renderer(dir: &Path) -> String {
    let rgba = dir.join("shade");
    // A tiny PNG made by the test itself, so the renderer needs no tools.
    flat(255).write(&rgba.with_extension("white.png")).unwrap();
    flat(0).write(&rgba.with_extension("black.png")).unwrap();
    let (white, black) = (
        rgba.with_extension("white.png"),
        rgba.with_extension("black.png"),
    );
    format!(
        r#"case {{id}} in
  good) test "$QML_COMPARE_VOLATILE" = time && test "$QML_COMPARE_SETTLE_MS" = 50 && grep -q '"capturedAt":"2026-01-02T03:04:05Z"' - && cp '{}' {{out}};;
  bad) cp '{}' {{out}};;
  *) echo 'no such luck' >&2; exit 3;;
esac"#,
        white.display(),
        black.display()
    )
}

#[test]
fn run_scores_reports_and_enforces_the_ratchet() {
    let dir = scratch("cli_run");
    let manifest = corpus(&dir);
    let renderer = renderer(&dir);
    let ratchet = dir.join("expected.json");
    std::fs::write(
        &ratchet,
        r#"{"about":"kept","examples":{"good":{"pixels":0.99,"content":0.5}}}"#,
    )
    .unwrap();
    let report = dir.join("report");
    let run = |extra: &[&str]| {
        bin()
            .args(["run", "--manifest"])
            .arg(&manifest)
            .args(["--renderer", &renderer, "--ratchet"])
            .arg(&ratchet)
            .arg("--report-dir")
            .arg(&report)
            .args(extra)
            .output()
            .unwrap()
    };

    let o = run(&["--update"]);
    let out = stdout(&o);
    assert!(
        o.status.success(),
        "{out}\n{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(out.contains("added bad"), "{out}");

    let rows: serde_json::Value =
        serde_json::from_slice(&std::fs::read(report.join("report.json")).unwrap()).unwrap();
    let row = |id: &str| {
        rows.as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id)
            .unwrap()
            .clone()
    };
    assert_eq!(row("good")["pixels"], 1.0);
    assert_eq!(row("good")["renders"], true);
    assert_eq!(row("bad")["pixels"], 0.0);
    assert_eq!(row("broken")["renders"], false);
    assert_eq!(row("broken")["error"], "no such luck");
    assert!(report.join("good.diff.png").is_file() && report.join("index.html").is_file());

    // The file keeps its other fields and gained the new example.
    let floors: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&ratchet).unwrap()).unwrap();
    assert_eq!(floors["about"], "kept");
    assert_eq!(floors["examples"]["bad"], serde_json::json!({}));
    assert!(floors["examples"].get("broken").is_none());

    // `bad` is now listed and renders; make it fall under a floor.
    std::fs::write(
        &ratchet,
        r#"{"examples":{"good":{"pixels":0.99},"bad":{"pixels":0.5},"broken":{}}}"#,
    )
    .unwrap();
    let o = run(&[]);
    let out = stdout(&o);
    assert_eq!(o.status.code(), Some(1), "{out}");
    assert!(
        out.contains("bad must stay as like its reference picture: pixels"),
        "{out}"
    );
    assert!(
        out.contains("broken must keep rendering: no such luck"),
        "{out}"
    );
    assert!(!out.contains("good must"), "{out}");
}

#[test]
fn list_prints_what_a_script_needs() {
    let dir = scratch("cli_list");
    let manifest = corpus(&dir);
    let o = bin()
        .args(["list", "--manifest"])
        .arg(&manifest)
        .output()
        .unwrap();
    let out = stdout(&o);
    let first: Vec<&str> = out.lines().next().unwrap().split('\t').collect();
    assert_eq!(first[0], "good");
    assert!(first[1].ends_with("ex/good"), "{first:?}");
    assert_eq!(&first[2..], ["Main.qml", "8", "8", "50", "true"]);
    // No `qt` notes means nothing is said against rendering it.
    assert!(out.lines().nth(1).unwrap().ends_with("1500\ttrue"), "{out}");
}

#[test]
fn raise_lifts_floors_to_the_scores_and_holds_them() {
    let dir = scratch("cli_raise");
    let manifest = corpus(&dir);
    let renderer = renderer(&dir);
    let ratchet = dir.join("expected.json");
    let report = dir.join("report");
    let run = || {
        bin()
            .args(["run", "--manifest"])
            .arg(&manifest)
            .args(["--renderer", &renderer, "--ratchet"])
            .arg(&ratchet)
            .arg("--report-dir")
            .arg(&report)
            .arg("--raise")
            .output()
            .unwrap()
    };
    let o = run();
    assert!(o.status.success(), "{}", stdout(&o));
    let floors: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&ratchet).unwrap()).unwrap();
    assert_eq!(
        floors["examples"]["good"],
        serde_json::json!({"pixels": 1.0, "content": 1.0})
    );
    assert_eq!(
        floors["examples"]["bad"],
        serde_json::json!({"pixels": 0.0, "content": 0.0})
    );
    assert!(floors["examples"].get("broken").is_none());
    // The summary is printed, and the example that does not render scores 0.
    assert!(
        stdout(&o).contains("3 examples: 2 compile their entry file, 2 render"),
        "{}",
        stdout(&o)
    );

    // A second run changes nothing; a regression stops it and writes nothing.
    std::fs::write(
        &ratchet,
        r#"{"examples":{"good":{"pixels":1.0},"bad":{"pixels":0.5}}}"#,
    )
    .unwrap();
    let o = run();
    assert_eq!(o.status.code(), Some(1), "{}", stdout(&o));
    assert!(stdout(&o).contains("not raised while an example is under its floor"));
    let floors: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&ratchet).unwrap()).unwrap();
    assert_eq!(
        floors["examples"]["bad"],
        serde_json::json!({"pixels": 0.5})
    );
}

/// One example whose name is long enough that the request on the
/// renderer's stdin is more than a pipe holds.
fn long_corpus(dir: &Path) -> PathBuf {
    let corpus = dir.join("corpus");
    std::fs::create_dir_all(&corpus).unwrap();
    let entry = format!("{}.qml", "e".repeat(100_000));
    std::fs::write(
        corpus.join("examples.json"),
        format!(
            r#"{{"root":"ex","examples":[
              {{"id":"long","dir":"long","entry":"{entry}","window":{{"width":8,"height":8}}}}
            ]}}"#
        ),
    )
    .unwrap();
    corpus.join("examples.json")
}

fn run_with(dir: &Path, manifest: &Path, renderer: &str, extra: &[&str]) -> serde_json::Value {
    let report = dir.join("report");
    let o = bin()
        .args(["run", "--manifest"])
        .arg(manifest)
        .args(["--renderer", renderer, "--report-dir"])
        .arg(&report)
        .args(extra)
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "{}\n{}",
        stdout(&o),
        String::from_utf8_lossy(&o.stderr)
    );
    serde_json::from_slice(&std::fs::read(report.join("report.json")).unwrap()).unwrap()
}

#[test]
fn a_renderer_that_writes_before_it_reads_does_not_stall_the_run() {
    let dir = scratch("cli_write_first");
    let manifest = long_corpus(&dir);
    let white = dir.join("white.png");
    flat(255).write(&white).unwrap();
    // More than a pipe holds out, before a byte of the request is read.
    let renderer = format!(
        "head -c 300000 /dev/zero | tr '\\0' x; echo; cat >/dev/null; cp '{}' {{out}}",
        white.display()
    );
    let started = std::time::Instant::now();
    let rows = run_with(&dir, &manifest, &renderer, &["--timeout", "60"]);
    assert!(started.elapsed() < std::time::Duration::from_secs(30));
    assert_eq!(rows[0]["renders"], true, "{rows}");
}

#[test]
fn a_renderer_that_takes_too_long_is_killed_with_what_it_started() {
    let dir = scratch("cli_timeout");
    let manifest = long_corpus(&dir);
    let started = std::time::Instant::now();
    // The shell waits on a child of its own, which holds the pipes too.
    let rows = run_with(&dir, &manifest, "sleep 30 & wait", &["--timeout", "1"]);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "took {:?}",
        started.elapsed()
    );
    assert_eq!(rows[0]["renders"], false, "{rows}");
    let error = rows[0]["error"].as_str().unwrap_or_default();
    assert!(error.contains("longer than 1 s"), "{error}");
}
