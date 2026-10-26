//! Every fixture of `crates/ljos_design/fixtures` whose rule is checked
//! at run time, through the audit.
//!
//! A `.bad.qml` must give each finding its `// expect:` line names, at
//! the line it names or the object that holds that line ([`same_place`]),
//! and no other of its directory's code; a `.good.qml`
//! none of its directory's code. A fixture is audited at the point its
//! `// audit:` line names, else as it is. Fixtures of a rule the audit
//! does not implement yet are listed as pending, not failed, and so are
//! the cases [`KNOWN_MISSES`] names, which the audit cannot see as it is
//! built; one of them passing fails the run, so that it is taken off.

use std::fmt::Write as _;
use std::path::Path;

use std::rc::Rc;
use std::sync::Arc;

use ljos_audit::{Matrix, MatrixPoint, Options, Placement, Report, audit, rules};
use ljos_design::{Check, fixture};
use ljos_engine::{Engine, ObjectId};
use ljos_quick::{Item, QuickScene};
use ljos_render::kurbo;
use ljos_runtime::{Clock, VirtualClock};

/// Bad cases of implemented rules the audit does not catch, and why.
const KNOWN_MISSES: &[(&str, &str)] = &[
    (
        "a11y-tab-trap/ux-textarea-takes-tab.bad.qml",
        "the trap is a `Keys.onPressed` that takes Tab; seeing it means pressing \
         Tab, and the audit acts on nothing",
    ),
    (
        "layout-offscreen/ux-selected-row-below-view.bad.qml",
        "a current row scrolled out of its list is also what a user who scrolled \
         away sees; the audit cannot tell the two apart, and the rule is an error",
    ),
    (
        "motion-duration/flicker.bad.qml",
        "an animation shorter than a frame of the display ends between two frames; the audit cannot time what it does not see, and the source says it",
    ),
    (
        "motion-duration/micro.bad.qml",
        "the animation runs on hover, and the audit moves no pointer",
    ),
    (
        "motion-ease-in-enter/behavior.bad.qml",
        "the `Behavior` runs when its opacity changes, which nothing in the scene does",
    ),
    (
        "motion-ease-in-enter/state-shows.bad.qml",
        "the panel shows in a state nothing enters; the audit enters no states, which would run the application's code",
    ),
    (
        "motion-frequent/behavior.bad.qml",
        "the row takes focus but is no Tab stop, and the audit gives focus only where Tab goes",
    ),
    (
        "motion-frequent/highlight.bad.qml",
        "the highlight moves when the current row changes, which the audit does not change in a list",
    ),
    (
        "layout-offscreen/ux-tabbar-overflow.bad.qml",
        "a tab bar wider than its tabs' room flicks, as Qt documents it; what can \
         be scrolled into view is left out of the rule",
    ),
    (
        "motion-reduced-ignored/dial-spins.bad.qml",
        "what spins is an `Item` that draws nothing: there is no movement on \
         screen to see, and the static lint reads the animation",
    ),
];

/// The window a fixture is shown in when its root has no size.
const WINDOW: (f64, f64) = (640.0, 480.0);

/// Application time a fixture is let run before the audit, as `ljos
/// audit` lets it by default: a timer's load lands, and what it moved is
/// a layout shift.
const SETTLE_MS: u64 = 500;
/// One frame of that time.
const FRAME_MS: u64 = 16;

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

/// What a fixture's header comments say, read by the reader the static
/// lint tests share (`ljos_design::fixture`).
struct Header {
    expect: Vec<(String, u32)>,
    point: Option<MatrixPoint>,
}

fn header(source: &str, bad: bool) -> Result<Header, String> {
    let expect = if bad {
        fixture::expected(source)?
            .into_iter()
            .map(|e| (e.code, e.line))
            .collect()
    } else {
        Vec::new()
    };
    let pairs = fixture::audit(source)?;
    let point = if pairs.is_empty() {
        None
    } else {
        let text: Vec<String> = pairs.iter().map(|(k, v)| format!("{k}={v}")).collect();
        Some(text.join(" ").parse().map_err(|e| format!("{e}"))?)
    };
    Ok(Header { expect, point })
}

/// The line of a place like `file:///Main.qml:8:5`.
fn line_of(place: &str) -> Option<u32> {
    let mut parts = place.rsplitn(3, ':');
    let _column = parts.next()?;
    parts.next()?.parse().ok()
}

/// Whether a finding at `found` is the one expected at `line`. The static
/// lint of a rule checked both ways anchors where the author writes (the
/// `color:` of a Text, a MouseArea), the audit on the item that shows the
/// defect (the Text, the item the MouseArea makes a stop of): the object
/// whose braces hold the expected line is the same place.
fn same_place(source: &str, line: u32, found: Option<u32>) -> bool {
    found == Some(line) || (found.is_some() && found == holder(source, line))
}

/// The line of the object declaration whose braces hold `line`.
fn holder(source: &str, line: u32) -> Option<u32> {
    let lines: Vec<&str> = source.lines().collect();
    let mut depth = 0i32;
    for at in (1..line).rev() {
        let text = lines.get(at as usize - 1)?;
        let code = text.split("//").next().unwrap_or_default();
        for ch in code.chars().rev() {
            match ch {
                '}' => depth += 1,
                '{' if depth == 0 => return Some(at),
                '{' => depth -= 1,
                _ => {}
            }
        }
    }
    None
}

/// Mounts the fixture as a viewer does, the root filling the window, and
/// audits it.
fn run(source: &str, point: Option<MatrixPoint>) -> Result<Report, String> {
    let e = engine();
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(Arc::clone(&clock)));
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
    s.frame(&e, clock.now());
    let before = Placement::of(&e, &s);
    for _ in 0..SETTLE_MS / FRAME_MS {
        clock.advance_ms(FRAME_MS);
        e.advance_timers(clock.now());
        s.frame(&e, clock.now());
    }
    let options = Options {
        matrix: point.map_or(Matrix::Current, |p| Matrix::Points(vec![p])),
        before: Some(Arc::new(before)),
        // A fixture is the test's own scene; the acting checks have
        // fixtures of their own.
        act: true,
        ..Options::desktop()
    };
    Ok(audit(&e, &mut s, clock.now(), &options))
}

/// Whether the fixture's root is an item a viewer can show. A root
/// `Popup` is a component only the source reads; the static lint holds it.
fn root_is_item(path: &Path) -> bool {
    let Ok(source) = std::fs::read_to_string(path) else {
        return true;
    };
    let e = engine();
    load(&e, &source).map_or(true, |root| ljos_quick::is_item(&e, root))
}

/// What is wrong with one fixture, or nothing.
fn check(path: &Path, code: &str) -> Option<String> {
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(err) => return Some(err.to_string()),
    };
    let bad = path.to_string_lossy().ends_with(".bad.qml");
    let head = match header(&source, bad) {
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
    let mut wrong = String::new();
    if bad {
        for (c, at) in &head.expect {
            let hit = report
                .of(c)
                .any(|f| same_place(&source, *at, f.place.as_deref().and_then(line_of)));
            if !hit {
                let _ = writeln!(wrong, "  expected {c} at {at}");
            }
        }
        for (at, f) in &found {
            if !head
                .expect
                .iter()
                .any(|(c, l)| c == code && same_place(&source, *l, Some(*at)))
            {
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
    let (mut ran, mut pending, mut failed) = (0, Vec::new(), Vec::new());
    let mut missed = Vec::new();
    for case in fixture::cases() {
        let code = case.code.as_str();
        let Some(rule) = ljos_design::rule(code) else {
            failed.push(format!("{code}: no such rule"));
            continue;
        };
        if rule.check == Check::Static {
            continue;
        }
        for file in [&case.bad, &case.good] {
            let id = format!("{code}/{}", name(file));
            if !implemented.contains(&code) {
                pending.push(id);
                continue;
            }
            if !root_is_item(file) {
                pending.push(format!("{id} (the root is no item: the source's alone)"));
                continue;
            }
            ran += 1;
            let known = KNOWN_MISSES.iter().find(|(c, _)| *c == id);
            match (check(file, code), known) {
                (Some(_), Some((_, why))) => missed.push(format!("{id}: {why}")),
                (Some(why), None) => failed.push(format!("{id}:\n{why}")),
                (None, Some(_)) => {
                    failed.push(format!("{id}: caught now, take it off KNOWN_MISSES"))
                }
                (None, None) => {}
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
    if !missed.is_empty() {
        eprintln!(
            "known misses ({}):\n  {}",
            missed.len(),
            missed.join("\n  ")
        );
    }
    for (id, _) in KNOWN_MISSES {
        if !fixture::dir().join(id).exists() {
            failed.push(format!("{id}: in KNOWN_MISSES and no such case"));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

fn name(p: &Path) -> String {
    p.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}
