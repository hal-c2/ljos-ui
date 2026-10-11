//! External acceptance suites (ARCHITECTURE.md §18.4).
//!
//! A suite is somebody else's Gherkin features, kept in their repository and
//! read in place. Ljós keeps three small files about it under
//! `acceptance/<suite>/`:
//!
//! - `PIN`: which revision of the suite the results are about (verified),
//! - `features.globs`: which of its feature files Ljós runs,
//! - `RATCHET`: which scenarios have to keep passing.
//!
//! and one adapter crate, `steps/`, that maps the suite's UI-facing steps
//! onto the [`crate::backend::Backend`] trait through [`AcceptanceWorld`].
//! The adapter's `main` calls [`main`]. Ljós never writes into the suite's
//! checkout; the only file this module writes is `RATCHET`, and only on
//! `--raise`.

mod globs;
mod pin;
mod ratchet;
mod world;

use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::ExitCode,
    rc::Rc,
};

use cucumber::{World as _, cli, event, parser};
use futures::stream;
use gherkin::{Feature, GherkinEnv, Rule, Scenario};

pub use globs::GlobError;
pub use pin::{Pin, PinError, tree_digest};
pub use ratchet::{Ratchet, RatchetError};
pub use world::AcceptanceWorld;

/// A scenario with this tag is known not to work on Ljós yet. It is not run,
/// and the summary counts it.
pub const BACKLOG_TAG: &str = "backlog-ljos";

#[derive(Debug, thiserror::Error)]
pub enum SuiteError {
    #[error(transparent)]
    Pin(#[from] PinError),
    #[error(transparent)]
    Globs(#[from] GlobError),
    #[error(transparent)]
    Ratchet(#[from] RatchetError),
    #[error("{path}: {why}")]
    Feature { path: String, why: String },
    #[error("{0}: {1}")]
    Io(String, std::io::Error),
}

/// One scenario's outcome on Ljós.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub id: String,
    pub passed: bool,
}

/// What the run found, before anything is judged.
#[derive(Clone, Debug, Default)]
pub struct Run {
    pub files: usize,
    pub outcomes: Vec<Outcome>,
    /// Ids of the `@backlog-ljos` scenarios that were left out.
    pub skipped: Vec<String>,
}

/// Why a held scenario is not holding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Why {
    Failed,
    /// The suite no longer has a scenario by this name (it was renamed or
    /// its file left the allowlist).
    Missing,
    /// It is tagged `@backlog-ljos` and so was not run.
    Skipped,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Verdict {
    /// Held scenarios that do not pass. The run fails when there are any.
    pub regressions: Vec<(String, Why)>,
    /// Passing scenarios the ratchet does not hold yet.
    pub unheld_passing: Vec<String>,
    /// Failing scenarios the ratchet does not hold.
    pub not_yet: Vec<String>,
}

impl Verdict {
    #[must_use]
    pub fn holds(&self) -> bool {
        self.regressions.is_empty()
    }
}

#[must_use]
pub fn judge(ratchet: &Ratchet, run: &Run) -> Verdict {
    let outcome: BTreeMap<&str, bool> = run
        .outcomes
        .iter()
        .map(|o| (o.id.as_str(), o.passed))
        .collect();
    let skipped: BTreeSet<&str> = run.skipped.iter().map(String::as_str).collect();
    let mut verdict = Verdict::default();
    for id in &ratchet.ids {
        match outcome.get(id.as_str()) {
            Some(true) => {}
            Some(false) => verdict.regressions.push((id.clone(), Why::Failed)),
            None if skipped.contains(id.as_str()) => {
                verdict.regressions.push((id.clone(), Why::Skipped));
            }
            None => verdict.regressions.push((id.clone(), Why::Missing)),
        }
    }
    for o in &run.outcomes {
        if ratchet.ids.contains(&o.id) {
            continue;
        }
        if o.passed {
            verdict.unheld_passing.push(o.id.clone());
        } else {
            verdict.not_yet.push(o.id.clone());
        }
    }
    verdict
}

/// `feature.feature :: Rule :: Scenario [example row]`. The row of an outline
/// is part of the name because every row is a scenario of its own.
fn base_id(file: &str, rule: Option<&Rule>, scenario: &Scenario) -> String {
    let mut id = format!("{file} :: ");
    if let Some(rule) = rule {
        id.push_str(&format!("{} :: ", rule.name));
    }
    id.push_str(&scenario.name);
    let row = scenario
        .examples
        .first()
        .and_then(|e| e.table.as_ref())
        .and_then(|t| t.rows.get(1));
    if let Some(row) = row {
        id.push_str(&format!(" [{}]", row.join(" | ")));
    }
    id
}

/// Where a scenario is, as the runner reports it back.
type Key = (PathBuf, usize, usize, String);

fn key(path: &Path, scenario: &Scenario) -> Key {
    (
        path.to_path_buf(),
        scenario.position.line,
        scenario.position.col,
        scenario.name.clone(),
    )
}

/// The features that will run, the ids of their scenarios, and the
/// scenarios left out for being on Ljós's backlog.
struct Plan {
    features: Vec<Feature>,
    ids: BTreeMap<Key, String>,
    skipped: Vec<String>,
}

fn plan(root: &Path, files: &[String]) -> Result<Plan, SuiteError> {
    use cucumber::feature::Ext as _;
    let mut out = Plan {
        features: Vec::new(),
        ids: BTreeMap::new(),
        skipped: Vec::new(),
    };
    let mut taken: BTreeMap<String, usize> = BTreeMap::new();
    for rel in files {
        let path = root.join(rel);
        let fail = |why: String| SuiteError::Feature {
            path: rel.clone(),
            why,
        };
        let mut feature = Feature::parse_path(&path, GherkinEnv::default())
            .map_err(|e| fail(e.to_string()))?
            .expand_examples()
            .map_err(|e| fail(e.to_string()))?;
        let feature_tags = feature.tags.clone();
        let mut sort = |rule: Option<&Rule>, scenarios: Vec<Scenario>, inherited: &[String]| {
            let mut kept = Vec::new();
            for scenario in scenarios {
                let mut id = base_id(rel, rule, &scenario);
                // Two scenarios of one name stay two ids: #2, #3, ...
                let count = taken.entry(id.clone()).or_insert(0);
                *count += 1;
                if *count > 1 {
                    id.push_str(&format!(" #{count}"));
                }
                let backlog = inherited
                    .iter()
                    .chain(&scenario.tags)
                    .any(|t| t.trim_start_matches('@') == BACKLOG_TAG);
                if backlog {
                    out.skipped.push(id);
                } else {
                    out.ids.insert(key(&path, &scenario), id);
                    kept.push(scenario);
                }
            }
            kept
        };
        let top = std::mem::take(&mut feature.scenarios);
        feature.scenarios = sort(None, top, &feature_tags);
        for rule in &mut feature.rules {
            let inherited: Vec<String> = feature_tags.iter().chain(&rule.tags).cloned().collect();
            let scenarios = std::mem::take(&mut rule.scenarios);
            let shown = Rule {
                scenarios: Vec::new(),
                ..rule.clone()
            };
            rule.scenarios = sort(Some(&shown), scenarios, &inherited);
        }
        out.features.push(feature);
    }
    Ok(out)
}

/// A parser with nothing left to parse: the features are already read.
struct Prepared(Vec<Feature>);

impl<I> cucumber::Parser<I> for Prepared {
    type Cli = cli::Empty;
    type Output = stream::Iter<std::vec::IntoIter<parser::Result<Feature>>>;

    fn parse(self, _: I, _: Self::Cli) -> Self::Output {
        stream::iter(self.0.into_iter().map(Ok).collect::<Vec<_>>())
    }
}

/// Runs the scenarios of `files` (relative to `root`) on the Ljós backend
/// with whatever steps are linked into this program.
///
/// # Errors
/// A feature file does not parse.
pub fn run_features(root: &Path, files: &[String]) -> Result<Run, SuiteError> {
    let Plan {
        features,
        ids,
        skipped,
    } = plan(root, files)?;
    let ids = Rc::new(ids);
    let outcomes: Rc<RefCell<Vec<Outcome>>> = Rc::default();
    let (seen, record) = (Rc::clone(&ids), Rc::clone(&outcomes));
    futures::executor::block_on(
        AcceptanceWorld::cucumber::<PathBuf>()
            .with_parser(Prepared(features))
            .with_default_cli()
            .max_concurrent_scenarios(1)
            .after(move |feature, _rule, scenario, finished, _world| {
                let path = feature.path.clone().unwrap_or_default();
                if let Some(id) = seen.get(&key(&path, scenario)) {
                    record.borrow_mut().push(Outcome {
                        id: id.clone(),
                        passed: matches!(finished, event::ScenarioFinished::StepPassed),
                    });
                }
                futures::FutureExt::boxed_local(async {})
            })
            .run(PathBuf::new()),
    );
    let mut outcomes = outcomes.take();
    outcomes.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(Run {
        files: files.len(),
        outcomes,
        skipped,
    })
}

#[derive(Debug, clap::Parser)]
#[command(about = "Run an external acceptance suite on Ljós")]
struct Cli {
    /// The suite's checkout. Wins over the PIN's environment variable.
    #[arg(long)]
    path: Option<PathBuf>,
    /// The suite's directory in this repository (PIN, features.globs,
    /// RATCHET). The adapter's own default is used when absent.
    #[arg(long)]
    suite: Option<PathBuf>,
    /// Add what passes to RATCHET. Refuses while a held scenario fails.
    #[arg(long)]
    raise: bool,
    /// Do not mind uncommitted changes to tracked files of the checkout.
    #[arg(long)]
    allow_dirty: bool,
    /// Print the checkout's digest (for a `digest` PIN) and stop.
    #[arg(long)]
    digest: bool,
}

/// The program an adapter crate is: `suite_dir` is the suite's directory in
/// this repository, from the adapter's point of view.
#[must_use]
pub fn main(suite_dir: &Path) -> ExitCode {
    use clap::Parser as _;
    let cli = Cli::parse();
    let suite_dir = cli.suite.clone().unwrap_or_else(|| suite_dir.to_path_buf());
    match execute(&cli, &suite_dir) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("acceptance: {e}");
            ExitCode::from(2)
        }
    }
}

fn execute(cli: &Cli, suite_dir: &Path) -> Result<bool, SuiteError> {
    let name = suite_dir
        .canonicalize()
        .unwrap_or_else(|_| suite_dir.to_path_buf())
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let pin = Pin::read(&suite_dir.join("PIN"))?;
    let root = pin.checkout(cli.path.as_deref(), suite_dir, |v| std::env::var(v).ok())?;
    if cli.digest {
        println!("{}", tree_digest(&root)?);
        return Ok(true);
    }
    pin.verify(&root, cli.allow_dirty)?;
    let globs_file = suite_dir.join("features.globs");
    let text = std::fs::read_to_string(&globs_file)
        .map_err(|e| SuiteError::Io(globs_file.display().to_string(), e))?;
    let files = globs::expand(&text, &root)?;
    let ratchet_file = suite_dir.join("RATCHET");
    let mut ratchet = Ratchet::read(&ratchet_file)?;

    let run = run_features(&root, &files)?;
    let verdict = judge(&ratchet, &run);
    let passing = run.outcomes.iter().filter(|o| o.passed).count();
    println!();
    println!(
        "acceptance {name}: {} feature files, {} scenarios run on Ljos: {passing} pass, {} fail; \
         {} skipped (@{BACKLOG_TAG}); ratchet holds {}",
        run.files,
        run.outcomes.len(),
        run.outcomes.len() - passing,
        run.skipped.len(),
        ratchet.ids.len(),
    );
    for (id, why) in &verdict.regressions {
        let what = match why {
            Why::Failed => "fails",
            Why::Missing => "is not in the suite any more",
            Why::Skipped => "is tagged @backlog-ljos",
        };
        println!("  REGRESSION  {id}: {what}");
    }
    for id in &verdict.not_yet {
        println!("  not yet     {id}");
    }
    for id in &verdict.unheld_passing {
        println!("  can be held {id}");
    }

    if cli.raise {
        if !verdict.holds() {
            println!("RATCHET not raised while a held scenario fails");
            return Ok(false);
        }
        let added = ratchet.raise(verdict.unheld_passing.iter().cloned());
        if added.is_empty() {
            println!("RATCHET unchanged");
        } else {
            ratchet.write(&ratchet_file)?;
            println!("RATCHET raised by {}", added.len());
        }
        return Ok(true);
    }
    if !verdict.holds() {
        println!(
            "FAILED: {} held scenarios do not pass",
            verdict.regressions.len()
        );
        return Ok(false);
    }
    println!("ok");
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(passing: &[&str], failing: &[&str], skipped: &[&str]) -> Run {
        let outcome = |id: &&str, passed| Outcome {
            id: (*id).to_owned(),
            passed,
        };
        Run {
            files: 1,
            outcomes: passing
                .iter()
                .map(|i| outcome(i, true))
                .chain(failing.iter().map(|i| outcome(i, false)))
                .collect(),
            skipped: skipped.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    #[test]
    fn a_held_scenario_that_fails_or_vanishes_is_a_regression() {
        let ratchet = Ratchet::parse("a\nb\nc\nd\n");
        let verdict = judge(&ratchet, &run(&["a", "e"], &["b", "f"], &["c"]));
        assert_eq!(
            verdict.regressions,
            [
                ("b".to_owned(), Why::Failed),
                ("c".to_owned(), Why::Skipped),
                ("d".to_owned(), Why::Missing),
            ]
        );
        assert_eq!(verdict.unheld_passing, ["e"]);
        assert_eq!(verdict.not_yet, ["f"]);
        assert!(!verdict.holds());
    }

    #[test]
    fn failures_outside_the_ratchet_do_not_fail_the_run() {
        let verdict = judge(&Ratchet::parse("a\n"), &run(&["a"], &["x"], &[]));
        assert!(verdict.holds());
    }

    #[test]
    fn ids_name_the_file_rule_scenario_and_outline_row() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("a.feature"),
            "Feature: A\n  Scenario: same\n    Given x\n  Scenario: same\n    Given x\n\
             \x20 @backlog-ljos\n  Scenario: later\n    Given x\n\
             \x20 Rule: R\n    Scenario Outline: row <n>\n      Given x\n      Examples:\n        | n |\n        | 1 |\n        | 2 |\n",
        )
        .unwrap();
        let planned = plan(dir.path(), &["a.feature".to_owned()]).unwrap();
        let mut ids: Vec<&String> = planned.ids.values().collect();
        ids.sort();
        assert_eq!(
            ids,
            [
                "a.feature :: R :: row 1 [1]",
                "a.feature :: R :: row 2 [2]",
                "a.feature :: same",
                "a.feature :: same #2",
            ]
        );
        assert_eq!(planned.skipped, ["a.feature :: later"]);
    }
}
