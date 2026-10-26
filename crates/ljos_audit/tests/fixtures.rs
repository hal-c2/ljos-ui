//! Every fixture of `crates/ljos_design/fixtures` whose rule is checked
//! at run time, through the audit.
//!
//! A `.bad.qml` must give each finding its `// expect:` line names, at
//! the line it names, and no other of its directory's code; a `.good.qml`
//! none of its directory's code. A fixture is audited at the point its
//! `// audit:` line names, else as it is. Fixtures of a rule the audit
//! does not implement yet are listed as pending, not failed.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use ljos_audit::{Matrix, MatrixPoint, Options, Report, audit, rules};
use ljos_design::Check;
use ljos_engine::{Engine, ObjectId};
use ljos_quick::{Item, QuickScene};
use ljos_render::kurbo;
use ljos_runtime::Time;

/// The window a fixture is shown in when its root has no size.
const WINDOW: (f64, f64) = (640.0, 480.0);

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../ljos_design/fixtures")
}

fn engine() -> Engine {
    let e = Engine::new();
    ljos_quick::register(&e);
    ljos_controls::register(&e);
    e
}

fn load(e: &Engine, source: &str) -> Result<ObjectId, String> {
    use ljos_compiler::{Project, compile_file};
    let mut project = Project::new(qml_front::QtTypeOracle);
    project.add("Main", source);
    let c = compile_file(&project, "Main").map_err(|d| format!("{d:?}"))?;
    if c.has_errors() {
        return Err(format!("{:?}", c.diagnostics));
    }
    let mut files = ljos_engine::MapSource::new();
    files.insert("Main", c.file);
    e.set_component_source(files);
    e.create_from_url("Main", None, Vec::new())
        .map_err(|err| err.to_string())
}

/// What a fixture's header comments say.
struct Header {
    expect: Vec<(String, u32)>,
    point: Option<MatrixPoint>,
}

fn header(source: &str) -> Result<Header, String> {
    let mut h = Header {
        expect: Vec::new(),
        point: None,
    };
    for line in source
        .lines()
        .take_while(|l| l.trim_start().starts_with("//"))
    {
        let line = line.trim_start().trim_start_matches('/').trim();
        if let Some(rest) = line.strip_prefix("expect:") {
            for one in rest.split(',') {
                let (code, at) = one
                    .trim()
                    .split_once(" at ")
                    .ok_or_else(|| format!("expect `{one}`: no `at <line>`"))?;
                let at = at.trim().parse().map_err(|_| format!("expect `{one}`"))?;
                h.expect.push((code.trim().to_string(), at));
            }
        } else if let Some(rest) = line.strip_prefix("audit:") {
            h.point = Some(rest.trim().parse().map_err(|e| format!("{e}"))?);
        }
    }
    Ok(h)
}

/// The line of a place like `file:///Main.qml:8:5`.
fn line_of(place: &str) -> Option<u32> {
    let mut parts = place.rsplitn(3, ':');
    let _column = parts.next()?;
    parts.next()?.parse().ok()
}

/// Mounts the fixture as a viewer does, the root filling the window, and
/// audits it.
fn run(source: &str, point: Option<MatrixPoint>) -> Result<Report, String> {
    let e = engine();
    let root = load(&e, source)?;
    let size = |p| e.read(root, p).to_number();
    let (mut w, mut h) = (size(Item::WIDTH.index), size(Item::HEIGHT.index));
    if w <= 0.0 || h <= 0.0 {
        (w, h) = WINDOW;
        e.set(root, Item::WIDTH, w).map_err(|err| err.to_string())?;
        e.set(root, Item::HEIGHT, h)
            .map_err(|err| err.to_string())?;
    }
    let mut s = QuickScene::new(kurbo::Size::new(w, h));
    s.set_root(&e, root).map_err(|err| format!("{err:?}"))?;
    s.frame(&e, Time::ZERO);
    let options = Options {
        matrix: point.map_or(Matrix::Current, |p| Matrix::Points(vec![p])),
        ..Options::desktop()
    };
    Ok(audit(&e, &mut s, Time::ZERO, &options))
}

/// What is wrong with one fixture, or nothing.
fn check(path: &Path, code: &str) -> Option<String> {
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(err) => return Some(err.to_string()),
    };
    let head = match header(&source) {
        Ok(h) => h,
        Err(err) => return Some(err),
    };
    let report = match run(&source, head.point) {
        Ok(r) => r,
        Err(err) => return Some(err),
    };
    let found: Vec<(u32, String)> = report
        .of(code)
        .map(|f| {
            (
                f.place.as_deref().and_then(line_of).unwrap_or(0),
                f.to_string(),
            )
        })
        .collect();
    let bad = path.to_string_lossy().ends_with(".bad.qml");
    let mut wrong = String::new();
    if bad {
        if head.expect.is_empty() {
            return Some("no `// expect:` line".into());
        }
        for (c, at) in &head.expect {
            let hit = report
                .of(c)
                .any(|f| f.place.as_deref().and_then(line_of) == Some(*at));
            if !hit {
                let _ = writeln!(wrong, "  expected {c} at {at}");
            }
        }
        for (at, f) in &found {
            if !head.expect.iter().any(|(c, l)| c == code && l == at) {
                let _ = writeln!(wrong, "  not expected: {f}");
            }
        }
    } else {
        for (_, f) in &found {
            let _ = writeln!(wrong, "  not expected: {f}");
        }
    }
    (!wrong.is_empty()).then(|| format!("{wrong}  the report:\n{report}"))
}

#[test]
fn every_runtime_fixture_gives_what_it_expects() {
    let implemented = rules::implemented();
    let Ok(dirs) = std::fs::read_dir(fixtures()) else {
        eprintln!("no fixtures at {}", fixtures().display());
        return;
    };
    let mut dirs: Vec<PathBuf> = dirs.flatten().map(|d| d.path()).collect();
    dirs.sort();
    let (mut ran, mut pending, mut failed) = (0, Vec::new(), Vec::new());
    for dir in dirs.iter().filter(|d| d.is_dir()) {
        let code = dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let Some(rule) = ljos_design::rule(&code) else {
            failed.push(format!("{code}: no such rule"));
            continue;
        };
        if rule.check == Check::Static {
            continue;
        }
        let mut cases: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "qml"))
            .collect();
        cases.sort();
        if !implemented.contains(&code.as_str()) {
            pending.extend(cases.iter().map(|p| format!("{code}/{}", name(p))));
            continue;
        }
        for case in &cases {
            ran += 1;
            if let Some(why) = check(case, &code) {
                failed.push(format!("{code}/{}:\n{why}", name(case)));
            }
        }
    }
    eprintln!("{ran} fixtures audited");
    if !pending.is_empty() {
        eprintln!(
            "pending, no check yet ({}):\n  {}",
            pending.len(),
            pending.join("\n  ")
        );
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

fn name(p: &Path) -> String {
    p.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}
