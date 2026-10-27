//! Every back end gives the same property values and signal order as the
//! dynamic tiers on the same inputs, and so does a run that swaps a
//! compiled case for its dynamic version halfway through.

use ljos_example_aot_spike::{Mode, Run, drive, load, snapshot, trace};

const STEPS: usize = 24;

// Back end A's library is one instance per process; the tests take turns.
static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn observe(mode: Mode, swap_at: Option<usize>) -> (Vec<String>, Vec<String>, Run) {
    let mut run = load(mode, true);
    let log = trace(&run);
    match swap_at {
        Some(k) => {
            drive(&run, 0..k);
            run.swap_to_dynamic();
            drive(&run, k..STEPS);
        }
        None => drive(&run, 0..STEPS),
    }
    let seen = log.borrow().clone();
    // Wherever tier 1 ran, QuickJS ran too and agreed (LJOS_TIER1_CHECK).
    let stats = run.engine.tier1_stats();
    assert_eq!(
        stats.mismatches,
        0,
        "{}: tier 1 and QuickJS differ",
        mode.name()
    );
    assert_eq!(
        stats.checked,
        stats.runs,
        "{}: every tier-1 run is checked",
        mode.name()
    );
    if mode == Mode::Tier1 {
        eprintln!("tier 1 ran and was checked {} times", stats.checked);
        assert!(stats.checked > 0, "tier 1 ran some of the cases");
    }
    (seen, snapshot(&run), run)
}

fn same(name: &str, (t, s): (&[String], &[String]), (rt, rs): (&[String], &[String])) {
    if let Some(i) = t.iter().zip(rt).position(|(a, b)| a != b) {
        panic!(
            "{name}: trace differs at {i}: {:?} against {:?}",
            &t[i..(i + 4).min(t.len())],
            &rt[i..(i + 4).min(rt.len())]
        );
    }
    assert_eq!(t.len(), rt.len(), "{name}: trace lengths");
    assert_eq!(s, rs, "{name}: the end state differs");
}

#[test]
fn every_back_end_matches_the_dynamic_tiers() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|p| p.into_inner());
    let (qt, qs, _) = observe(Mode::QuickJs, None);
    assert!(qt.len() > 100, "the trace sees the cases run: {}", qt.len());
    for mode in Mode::all() {
        let (t, s, run) = observe(mode, None);
        same(mode.name(), (&t, &s), (&qt, &qs));
        let warnings: Vec<_> = run
            .engine
            .take_log()
            .into_iter()
            .filter(|e| e.level >= ljos_engine::LogLevel::Warning)
            .map(|e| e.message)
            .collect();
        assert!(warnings.is_empty(), "{}: {warnings:?}", mode.name());
    }
}

#[test]
fn a_case_swapped_for_its_dynamic_version_mid_run_changes_nothing() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|p| p.into_inner());
    let (qt, qs, _) = observe(Mode::QuickJs, None);
    for mode in Mode::all()
        .into_iter()
        // The shipped mode has no slots: its fallback is a file edited
        // while it runs, which features/language/compiled-shipped-files
        // holds to.
        .filter(|m| !matches!(m, Mode::Tier1 | Mode::QuickJs | Mode::Shipped))
    {
        let (t, s, run) = observe(mode, Some(STEPS / 2));
        same(&format!("{} swapped", mode.name()), (&t, &s), (&qt, &qs));
        let warnings: Vec<_> = run
            .engine
            .take_log()
            .into_iter()
            .filter(|e| e.level >= ljos_engine::LogLevel::Warning)
            .map(|e| e.message)
            .collect();
        assert!(warnings.is_empty(), "{}: {warnings:?}", mode.name());
    }
}
