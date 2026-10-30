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

pub(crate) fn target_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map_or_else(|| workspace_root().join("target"), PathBuf::from)
}

/// Where JUnit goes (ARCHITECTURE.md §18.3).
#[must_use]
pub fn junit_path() -> PathBuf {
    target_dir().join("features/junit.xml")
}

/// Whether a scenario changes files under the running application. Only
/// then is what a reload needs noted, as only a development build notes
/// it. `LJOS_BDD_RELOAD_TRACKING=1` notes it for every scenario: a run of
/// the whole ledger that way shows that noting changes nothing else.
fn changes_files(
    feature: &gherkin::Feature,
    rule: Option<&gherkin::Rule>,
    scenario: &gherkin::Scenario,
) -> bool {
    let backgrounds = [
        feature.background.as_ref(),
        rule.and_then(|r| r.background.as_ref()),
    ];
    std::env::var_os("LJOS_BDD_RELOAD_TRACKING").is_some_and(|v| v == "1")
        || backgrounds
            .into_iter()
            .flatten()
            .flat_map(|b| &b.steps)
            .chain(&scenario.steps)
            .any(|s| {
                let text = s.value.as_str();
                text.contains("reload")
                    || text == "the files are watched"
                    || (text.starts_with("the image file ") && text.contains(" is saved as "))
                    || (text.starts_with("the theme file ")
                        && (text.ends_with(" is changed to:") || text.ends_with(" is removed")))
                    || (text.starts_with("the file ")
                        && (text.ends_with(" is changed to:")
                            || text.ends_with(" is saved and closed as:")
                            || text.ends_with(" is removed")
                            || text.ends_with(" by a save that has not finished")))
            })
}

/// Run the ledger and return whether every selected scenario passed.
///
/// # Panics
/// When `LJOS_BDD_BACKEND` is invalid or the report file cannot be created.
pub fn run() -> bool {
    let mode = BackendMode::from_env();
    // Without Ljós in the run, a scenario that Qt cannot see has nothing to run on.
    let mut selection = Selection {
        require_tag: (mode == BackendMode::Qt).then(|| "qt-parity".to_owned()),
        oracle_only: mode == BackendMode::Qt,
        ..Selection::default()
    };
    // The desktop surface is Ljós on the desktop sandbox; without one its
    // scenarios are left out, and the run says why.
    if mode != BackendMode::Qt {
        match crate::backend::desktop::unavailable() {
            None => selection.surfaces.push("desktop".into()),
            // Empty when none was selected.
            Some(why) if why.is_empty() => {}
            Some(why) => eprintln!("@desktop scenarios not run: {why}"),
        }
    }
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
                let tagged = |tag: &str| {
                    let is = |t: &String| t == tag;
                    feature.tags.iter().any(is)
                        || rule.is_some_and(|r| r.tags.iter().any(is))
                        || scenario.tags.iter().any(is)
                };
                world.prepare(tagged("qt-parity"), tagged("desktop"));
                if changes_files(feature, rule, scenario) {
                    world.watch_files();
                }
                async {}.boxed_local()
            })
            .run(workspace_root().join("features")),
    );
    use cucumber::writer::Stats as _;
    !result.execution_has_failed() && result.parsing_errors() == 0
}
