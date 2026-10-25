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
//! The adapter's `main` calls [`main`]. An application that runs its own
//! ledger against itself brings its own World instead (its engine, types,
//! controllers and fakes) and calls [`main_with`], choosing which tags
//! leave a scenario out ([`Selection`]). Reading those files and judging
//! a run is `ljos_ratchet`, re-exported here, which a runner of its own
//! can use without this crate. Ljós never writes into the suite's
//! checkout; the only file written is `RATCHET`, and only on `--raise`.

mod world;

use std::{
    cell::RefCell,
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::ExitCode,
    rc::Rc,
};

use cucumber::{cli, event, parser};
use futures::stream;
use gherkin::{Feature, GherkinEnv, Rule, Scenario};

pub use ljos_ratchet::{
    BACKLOG_TAG, Choice, GlobError, Listed, Outcome, Pin, PinError, Ratchet, RatchetError, Run,
    ScenarioIds, Selection, Suite, SuiteError, Verdict, Why, judge, tree_digest,
};
pub use world::AcceptanceWorld;

/// A World a suite runs on: one `#[derive(cucumber::World)]` gives, with
/// the steps written for it.
pub trait SuiteWorld:
    cucumber::World + std::fmt::Debug + cucumber::codegen::WorldInventory
{
}

impl<W> SuiteWorld for W where
    W: cucumber::World + std::fmt::Debug + cucumber::codegen::WorldInventory
{
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
/// scenarios a skip tag left out.
struct Plan {
    features: Vec<Feature>,
    ids: BTreeMap<Key, String>,
    skipped: Vec<String>,
}

fn plan(root: &Path, listed: &[Listed], selection: &Selection) -> Result<Plan, SuiteError> {
    use cucumber::feature::Ext as _;
    let mut out = Plan {
        features: Vec::new(),
        ids: BTreeMap::new(),
        skipped: Vec::new(),
    };
    let mut ids = ScenarioIds::default();
    for entry in listed {
        let rel = &entry.file;
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
        let mut names = entry.names()?;
        let mut sort = |rule: Option<&Rule>, scenarios: Vec<Scenario>, inherited: &[String]| {
            let mut kept = Vec::new();
            for scenario in scenarios {
                let row = scenario
                    .examples
                    .first()
                    .and_then(|e| e.table.as_ref())
                    .and_then(|t| t.rows.get(1));
                let rule_name = rule.map(|r| r.name.as_str());
                let id = ids.next(rel, rule_name, &scenario.name, row.map(Vec::as_slice));
                let tags = inherited.iter().chain(&scenario.tags).map(String::as_str);
                if !names.takes(&scenario.name) {
                    continue;
                }
                match selection.choose(tags) {
                    Choice::Run => {
                        out.ids.insert(key(&path, &scenario), id);
                        kept.push(scenario);
                    }
                    Choice::Skip => out.skipped.push(id),
                    Choice::Out => {}
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
        names.finish()?;
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

/// Runs the selected scenarios of `listed` (relative to `root`), each on a
/// fresh `W` (made by its `#[world(init)]`), with whatever steps for `W`
/// are linked into this program: [`AcceptanceWorld`] for the Ljós
/// backend, or an application's own World.
///
/// # Errors
/// A feature file does not parse, or a name glob matches no scenario.
pub fn run_features<W>(
    root: &Path,
    listed: &[Listed],
    selection: &Selection,
) -> Result<Run, SuiteError>
where
    W: SuiteWorld,
{
    let Plan {
        features,
        ids,
        skipped,
    } = plan(root, listed, selection)?;
    let ids = Rc::new(ids);
    let outcomes: Rc<RefCell<Vec<Outcome>>> = Rc::default();
    let (seen, record) = (Rc::clone(&ids), Rc::clone(&outcomes));
    futures::executor::block_on(
        W::cucumber::<PathBuf>()
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
        files: listed.len(),
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
/// this repository, from the adapter's point of view. Scenarios run on
/// [`AcceptanceWorld`] and `@backlog-ljos` ones are left out.
#[must_use]
pub fn main(suite_dir: &Path) -> ExitCode {
    main_with::<AcceptanceWorld>(suite_dir, &Selection::default())
}

/// [`main`] with the program's own World and tag selection: an
/// application's runner for its own ledger.
#[must_use]
pub fn main_with<W>(suite_dir: &Path, selection: &Selection) -> ExitCode
where
    W: SuiteWorld,
{
    use clap::Parser as _;
    let cli = Cli::parse();
    let suite_dir = cli.suite.clone().unwrap_or_else(|| suite_dir.to_path_buf());
    match execute::<W>(&cli, &suite_dir, selection) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("acceptance: {e}");
            ExitCode::from(2)
        }
    }
}

fn execute<W>(cli: &Cli, suite_dir: &Path, selection: &Selection) -> Result<bool, SuiteError>
where
    W: SuiteWorld,
{
    if cli.digest {
        let (_, root) = ljos_ratchet::checkout(suite_dir, cli.path.as_deref())?;
        println!("{}", tree_digest(&root)?);
        return Ok(true);
    }
    let mut suite = Suite::open(suite_dir, cli.path.as_deref(), cli.allow_dirty)?;
    let run = run_features::<W>(&suite.root, &suite.listed, selection)?;
    ljos_ratchet::conclude(&mut suite, &run, selection, cli.raise)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An application's own World: state of its own, set up before any
    /// step, and steps that reach it directly.
    #[derive(Debug, cucumber::World)]
    #[world(init = Self::new)]
    struct Tally {
        count: i64,
    }

    impl Tally {
        fn new() -> Self {
            Self { count: 10 }
        }
    }

    #[cucumber::given(expr = "the tally adds {int}")]
    fn tally_adds(w: &mut Tally, n: i64) {
        w.count += n;
    }

    #[cucumber::then(expr = "the tally is {int}")]
    fn tally_is(w: &mut Tally, n: i64) -> Result<(), String> {
        (w.count == n).then_some(()).ok_or(format!("{}", w.count))
    }

    const TALLY: &str = "@kiosk\nFeature: T\n\
        \x20 Scenario: adds one\n    Given the tally adds 1\n    Then the tally is 11\n\
        \x20 Scenario: adds two\n    Given the tally adds 2\n    Then the tally is 99\n\
        \x20 @backlog @slow\n  Scenario: adds three\n    Given the tally adds 3\n\
        \x20 @backlog-kiosk\n  Scenario: adds four\n    Given the tally adds 4\n\
        \x20 Scenario: other\n    Given the tally adds 5\n";

    #[test]
    fn an_app_runs_its_own_world_with_its_own_tags_and_names() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("t.feature"), TALLY).unwrap();
        let selection = Selection {
            skip: vec!["backlog".into(), "@backlog-kiosk".into()],
            require: Some("kiosk".into()),
        };
        let listed = Listed {
            only: vec!["adds *".into()],
            ..Listed::file("t.feature")
        };
        let run = run_features::<Tally>(dir.path(), &[listed], &selection).unwrap();
        let outcome = |id: &str, passed| Outcome {
            id: format!("t.feature :: {id}"),
            passed,
        };
        assert_eq!(
            run.outcomes,
            [outcome("adds one", true), outcome("adds two", false)]
        );
        assert_eq!(
            run.skipped,
            ["t.feature :: adds three", "t.feature :: adds four"]
        );
        let elsewhere = Selection {
            require: Some("@desk".into()),
            ..selection
        };
        let run = run_features::<Tally>(dir.path(), &[Listed::file("t.feature")], &elsewhere);
        assert!(run.unwrap().outcomes.is_empty(), "not for this surface");
    }

    #[test]
    fn a_name_glob_that_matches_no_scenario_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("t.feature"), TALLY).unwrap();
        let listed = Listed {
            only: vec!["adds one".into(), "renamed".into()],
            ..Listed::file("t.feature")
        };
        let e = plan(dir.path(), &[listed], &Selection::default()).err();
        assert!(e.unwrap().to_string().contains("`renamed`"));
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
        let planned = plan(
            dir.path(),
            &[Listed::file("a.feature")],
            &Selection::default(),
        )
        .unwrap();
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
