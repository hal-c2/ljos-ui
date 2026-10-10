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
