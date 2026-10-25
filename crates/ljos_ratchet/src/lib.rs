//! The judging half of an acceptance suite (ARCHITECTURE.md §18.4).
//!
//! A suite keeps three small files about somebody's Gherkin features:
//! `PIN` (which revision, verified), `features.globs` (which files and
//! scenarios) and `RATCHET` (which scenarios have to keep passing). This
//! crate reads them, says which scenarios a run is about ([`Selection`],
//! [`Listed::names`]) and under which id ([`ScenarioIds`]), judges a
//! [`Run`] against the ratchet ([`judge`]) and prints the verdict, raising
//! the ratchet when asked ([`conclude`]).
//!
//! It runs nothing. `ljos_bdd::acceptance` runs scenarios on the Ljós
//! backend or an application's own World with it; an application's own
//! runner can parse and run its features however it likes and depend on
//! this crate alone, with no Gherkin, cucumber or Ljós runtime crate in its
//! build. The formats are in `acceptance/README.md`.

mod globs;
mod pin;
mod ratchet;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub use globs::{GlobError, Listed, NameFilter, expand};
pub use pin::{Pin, PinError, files_below, tree_digest};
pub use ratchet::{Ratchet, RatchetError};

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

/// One scenario's outcome.
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
    /// Ids of the scenarios a skip tag left out.
    pub skipped: Vec<String>,
}

/// Why a held scenario is not holding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Why {
    Failed,
    /// The suite no longer has a scenario by this name (it was renamed or
    /// its file left the allowlist).
    Missing,
    /// It has a skip tag and so was not run.
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

/// What a run does with one scenario, by its tags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Run,
    /// Not run, and counted as skipped.
    Skip,
    /// Not part of the run at all: it lacks the required tag.
    Out,
}

/// Which of the listed scenarios a run is about, by their tags (with or
/// without the `@`; a feature's and a rule's tags count for their
/// scenarios).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    /// A scenario with any of these is not run; the summary counts it.
    pub skip: Vec<String>,
    /// When set, a scenario without this tag is not part of the run at all
    /// (a suite that tags which surface each scenario is for).
    pub require: Option<String>,
}

impl Default for Selection {
    /// Leaves out `@backlog-ljos`, requires nothing.
    fn default() -> Self {
        Self {
            skip: vec![BACKLOG_TAG.to_owned()],
            require: None,
        }
    }
}

impl Selection {
    /// What to do with a scenario that carries `tags`, its feature's and
    /// rule's included.
    pub fn choose<'a>(&self, tags: impl IntoIterator<Item = &'a str>) -> Choice {
        let bare = |t: &'a str| t.trim_start_matches('@');
        let tags: Vec<&str> = tags.into_iter().map(bare).collect();
        let has = |t: &str| tags.contains(&t.trim_start_matches('@'));
        if self.require.as_deref().is_some_and(|t| !has(t)) {
            Choice::Out
        } else if self.skip.iter().any(|t| has(t)) {
            Choice::Skip
        } else {
            Choice::Run
        }
    }

    /// The skip tags as the summary shows them: `@a or @b`.
    #[must_use]
    pub fn skip_words(&self) -> String {
        let tags: Vec<String> = self
            .skip
            .iter()
            .map(|t| format!("@{}", t.trim_start_matches('@')))
            .collect();
        tags.join(" or ")
    }
}

/// Gives each scenario of a run its id, as `RATCHET` holds it:
/// `file :: Rule :: Scenario [example row]`, and ` #2`, ` #3` for later
/// scenarios of one file that share a name. Ask in the order the
/// scenarios are in their files, every scenario, skipped ones included,
/// so the numbering does not depend on the selection.
#[derive(Clone, Debug, Default)]
pub struct ScenarioIds {
    taken: BTreeMap<String, usize>,
}

impl ScenarioIds {
    /// `row` is an outline's example row (its values, not the header); it
    /// is part of the id because every row is a scenario of its own.
    pub fn next(
        &mut self,
        file: &str,
        rule: Option<&str>,
        scenario: &str,
        row: Option<&[String]>,
    ) -> String {
        let mut id = format!("{file} :: ");
        if let Some(rule) = rule {
            id.push_str(&format!("{rule} :: "));
        }
        id.push_str(scenario);
        if let Some(row) = row {
            id.push_str(&format!(" [{}]", row.join(" | ")));
        }
        let count = self.taken.entry(id.clone()).or_insert(0);
        *count += 1;
        if *count > 1 {
            id.push_str(&format!(" #{count}"));
        }
        id
    }
}

/// Where a suite's checkout is, from its `PIN`: `path` wins over the
/// variable the PIN names and the PIN's own path. Not verified yet.
///
/// # Errors
/// The PIN cannot be read, or no checkout is found.
pub fn checkout(dir: &Path, path: Option<&Path>) -> Result<(Pin, PathBuf), SuiteError> {
    let pin = Pin::read(&dir.join("PIN"))?;
    let root = pin.checkout(path, dir, |v| std::env::var(v).ok())?;
    Ok((pin, root))
}

/// A suite ready to run: its checkout verified against the PIN, what the
/// globs list, and the ratchet.
#[derive(Debug)]
pub struct Suite {
    /// The suite directory's name, for the summary.
    pub name: String,
    /// What the scenarios run on, for the summary ("Ljos").
    pub target: String,
    pub dir: PathBuf,
    pub root: PathBuf,
    pub listed: Vec<Listed>,
    pub ratchet: Ratchet,
}

impl Suite {
    /// Reads `dir` (`PIN`, `features.globs`, `RATCHET`) and verifies the
    /// checkout. `allow_dirty` lets uncommitted changes of a git checkout
    /// pass.
    ///
    /// # Errors
    /// A file is missing or malformed, the checkout is not the pinned
    /// revision, or a glob matches nothing.
    pub fn open(dir: &Path, path: Option<&Path>, allow_dirty: bool) -> Result<Self, SuiteError> {
        let (pin, root) = checkout(dir, path)?;
        pin.verify(&root, allow_dirty)?;
        let globs_file = dir.join("features.globs");
        let text = std::fs::read_to_string(&globs_file)
            .map_err(|e| SuiteError::Io(globs_file.display().to_string(), e))?;
        let listed = expand(&text, &root)?;
        let ratchet = Ratchet::read(&dir.join("RATCHET"))?;
        let name = dir
            .canonicalize()
            .unwrap_or_else(|_| dir.to_path_buf())
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        Ok(Self {
            name,
            target: "Ljos".to_owned(),
            dir: dir.to_path_buf(),
            root,
            listed,
            ratchet,
        })
    }
}

/// Judges `run`, prints the summary and each scenario's standing, and with
/// `raise` adds what passes to `RATCHET` (never while a held scenario
/// fails). Returns whether the run holds: exit 0, else 1.
///
/// # Errors
/// `RATCHET` cannot be written.
pub fn conclude(
    suite: &mut Suite,
    run: &Run,
    selection: &Selection,
    raise: bool,
) -> Result<bool, SuiteError> {
    let verdict = judge(&suite.ratchet, run);
    let passing = run.outcomes.iter().filter(|o| o.passed).count();
    let skip = selection.skip_words();
    println!();
    println!(
        "acceptance {}: {} feature files, {} scenarios run on {}: {passing} pass, {} fail; \
         {} skipped ({skip}); ratchet holds {}",
        suite.name,
        run.files,
        run.outcomes.len(),
        suite.target,
        run.outcomes.len() - passing,
        run.skipped.len(),
        suite.ratchet.ids.len(),
    );
    for (id, why) in &verdict.regressions {
        let what = match why {
            Why::Failed => "fails".to_owned(),
            Why::Missing => "is not in the suite any more".to_owned(),
            Why::Skipped => format!("is tagged {skip}"),
        };
        println!("  REGRESSION  {id}: {what}");
    }
    for id in &verdict.not_yet {
        println!("  not yet     {id}");
    }
    for id in &verdict.unheld_passing {
        println!("  can be held {id}");
    }
    if raise {
        if !verdict.holds() {
            println!("RATCHET not raised while a held scenario fails");
            return Ok(false);
        }
        let added = suite.ratchet.raise(verdict.unheld_passing.iter().cloned());
        if added.is_empty() {
            println!("RATCHET unchanged");
        } else {
            suite.ratchet.write(&suite.dir.join("RATCHET"))?;
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
    fn tags_skip_or_leave_out_a_scenario() {
        let s = Selection {
            skip: vec!["backlog".into(), "@backlog-desk".into()],
            require: Some("@desk".into()),
        };
        assert_eq!(s.choose(["@desk", "@ok"]), Choice::Run);
        assert_eq!(s.choose(["desk", "@backlog-desk"]), Choice::Skip);
        assert_eq!(s.choose(["@backlog"]), Choice::Out, "not for this surface");
        assert_eq!(Selection::default().choose(["@backlog-ljos"]), Choice::Skip);
        assert_eq!(s.skip_words(), "@backlog or @backlog-desk");
    }

    #[test]
    fn ids_name_the_file_rule_scenario_row_and_repeat() {
        let mut ids = ScenarioIds::default();
        let row = ["1".to_owned(), "a".to_owned()];
        assert_eq!(
            ids.next("a.feature", None, "same", None),
            "a.feature :: same"
        );
        assert_eq!(
            ids.next("a.feature", None, "same", None),
            "a.feature :: same #2"
        );
        assert_eq!(
            ids.next("a.feature", Some("R"), "row", Some(&row)),
            "a.feature :: R :: row [1 | a]"
        );
    }
}
