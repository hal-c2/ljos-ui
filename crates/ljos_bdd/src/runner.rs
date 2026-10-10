//! Wires the World, steps, filter and writers together.

use std::{
    fs,
    path::{Path, PathBuf},
};

use cucumber::{World as _, WriterExt as _, writer};
use futures::FutureExt as _;

use crate::{
    backend::BackendMode,
    filter::{BacklogFilter, Selection},
    world::LjosWorld,
};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root exists")
}

fn target_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map_or_else(|| workspace_root().join("target"), PathBuf::from)
}

/// Where JUnit goes (ARCHITECTURE.md §18.3).
#[must_use]
pub fn junit_path() -> PathBuf {
    target_dir().join("features/junit.xml")
}

/// Run the ledger and return whether every selected scenario passed.
///
/// # Panics
/// When `LJOS_BDD_BACKEND` is invalid or the report file cannot be created.
pub fn run() -> bool {
    let mode = BackendMode::from_env();
    // Without Ljós in the run, a scenario that Qt cannot see has nothing to run on.
    let selection = Selection {
        require_tag: (mode == BackendMode::Qt).then(|| "qt-parity".to_owned()),
        oracle_only: mode == BackendMode::Qt,
        ..Selection::default()
    };
    let junit = junit_path();
    fs::create_dir_all(junit.parent().expect("junit has a parent"))
        .expect("create target/features");
    let file = fs::File::create(&junit).expect("create junit.xml");

    // JUnit is not a Stats writer, so only the console side feeds the exit code.
    let console = writer::Basic::raw(std::io::stdout(), writer::Coloring::Auto, 0).summarized();
    let junit_out = writer::JUnit::for_tee(file, 0)
        .discard_stats_writes()
        .discard_arbitrary_writes();
    let writer = console.tee::<LjosWorld, _>(junit_out).normalized();
    let result = futures::executor::block_on(
        LjosWorld::cucumber::<PathBuf>()
            .with_parser(BacklogFilter::new(
                cucumber::parser::Basic::new(),
                selection,
            ))
            .with_writer(writer)
            .max_concurrent_scenarios(1)
            .fail_on_skipped()
            .before(|feature, rule, scenario, world| {
                let tagged = |t: &String| t == "qt-parity";
                let parity = feature.tags.iter().any(tagged)
                    || rule.is_some_and(|r| r.tags.iter().any(tagged))
                    || scenario.tags.iter().any(tagged);
                world.prepare(parity);
                async {}.boxed_local()
            })
            .run(workspace_root().join("features")),
    );
    use cucumber::writer::Stats as _;
    !result.execution_has_failed() && result.parsing_errors() == 0
}
