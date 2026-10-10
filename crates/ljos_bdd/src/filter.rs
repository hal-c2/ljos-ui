//! `BacklogFilter`: a [`Parser`] wrapper that keeps backlog scenarios out of a run.
//!
//! It has to wrap the parser, not use cucumber's `filter_run`, because `--tags`
//! *replaces* the `filter_run` closure: a naive filter would run `@backlog`
//! scenarios again as soon as someone passes `--tags`.

use std::path::Path;

use cucumber::{Parser, parser};
use futures::{StreamExt as _, stream::LocalBoxStream};
use gherkin::{Feature, Rule, Scenario};

/// Surfaces the headless runner provides. A scenario with no surface tag is `@headless`.
const SURFACES: &[&str] = &["headless", "desktop", "android", "ios", "wgpu"];

/// What a run keeps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    /// Surfaces this runner provides; `@backlog-<surface>` only counts for these.
    pub surfaces: Vec<String>,
    /// `--backlog`: run only the work still missing, instead of everything else.
    pub backlog_only: bool,
    /// Keep only scenarios carrying this tag (`qt-parity` when the Qt oracle is the only backend).
    pub require_tag: Option<String>,
    /// The Qt oracle is the only backend: Ljós's backlog status says nothing about whether
    /// Qt agrees, and verifying backlog expectations on Qt is the point of the oracle.
    pub oracle_only: bool,
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            surfaces: vec!["headless".into()],
            backlog_only: false,
            require_tag: None,
            oracle_only: false,
        }
    }
}

fn bare(tag: &str) -> &str {
    tag.trim_start_matches('@')
}

impl Selection {
    /// `tags` are the merged Feature + Rule + Scenario tags.
    #[must_use]
    pub fn keeps(&self, tags: &[String]) -> bool {
        let has = |t: &str| tags.iter().any(|x| bare(x) == t);
        if let Some(req) = &self.require_tag
            && !has(req)
        {
            return false;
        }
        // A scenario for another surface does not belong in this runner at all.
        let surface_tags: Vec<&str> = tags
            .iter()
            .map(|t| bare(t))
            .filter(|t| SURFACES.contains(t))
            .collect();
        if !surface_tags.is_empty()
            && !surface_tags
                .iter()
                .any(|t| self.surfaces.iter().any(|s| s == t))
        {
            return false;
        }
        let backlog = tags.iter().map(|t| bare(t)).any(|t| {
            t == "backlog"
                || t.strip_prefix("backlog-")
                    .is_some_and(|s| self.surfaces.iter().any(|own| own == s))
        });
        let parked = has("dropped") || has("blocked");
        if self.oracle_only {
            return !parked;
        }
        if self.backlog_only {
            // Dropped and blocked work cannot be picked up, so it is not "the backlog".
            backlog && !parked
        } else {
            !backlog && !parked
        }
    }

    fn filter(&self, mut feature: Feature) -> Feature {
        let base = feature.tags.clone();
        let merged =
            |extra: &[String]| -> Vec<String> { base.iter().chain(extra).cloned().collect() };
        let keep_scenarios =
            |this: &Self, tags: &[String], scenarios: Vec<Scenario>| -> Vec<Scenario> {
                scenarios
                    .into_iter()
                    .filter(|s| this.keeps(&[tags, &s.tags].concat()))
                    .collect()
            };
        feature.scenarios = keep_scenarios(self, &base, std::mem::take(&mut feature.scenarios));
        feature.rules = std::mem::take(&mut feature.rules)
            .into_iter()
            .map(|mut rule: Rule| {
                let rule_tags = merged(&rule.tags);
                rule.scenarios =
                    keep_scenarios(self, &rule_tags, std::mem::take(&mut rule.scenarios));
                rule
            })
            .collect();
        feature
    }
}

/// Command line flags the filter adds to the runner's own.
#[derive(Clone, Debug, Default, clap::Args)]
#[group(skip)]
pub struct FilterCli<C: clap::Args> {
    #[command(flatten)]
    pub inner: C,
    /// Run only `@backlog` work (not dropped or blocked) instead of the passing ledger.
    #[arg(long)]
    pub backlog: bool,
}

/// Wraps a parser and drops scenarios per [`Selection`].
#[derive(Clone, Debug)]
pub struct BacklogFilter<P> {
    inner: P,
    selection: Selection,
}

impl<P> BacklogFilter<P> {
    #[must_use]
    pub fn new(inner: P, selection: Selection) -> Self {
        Self { inner, selection }
    }
}

impl<I: AsRef<Path>, P: Parser<I>> Parser<I> for BacklogFilter<P>
where
    P::Cli: Clone + std::fmt::Debug,
{
    type Cli = FilterCli<P::Cli>;
    type Output = LocalBoxStream<'static, parser::Result<Feature>>;

    fn parse(self, input: I, cli: Self::Cli) -> Self::Output {
        let mut selection = self.selection;
        selection.backlog_only |= cli.backlog;
        self.inner
            .parse(input, cli.inner)
            .map(move |f| f.map(|f| selection.filter(f)))
            .boxed_local()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(t: &[&str]) -> Vec<String> {
        t.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn default_run_drops_parked_and_backlog_work() {
        let sel = Selection::default();
        assert!(sel.keeps(&tags(&["qt-parity"])));
        for t in ["backlog", "backlog-headless", "dropped", "blocked"] {
            assert!(!sel.keeps(&tags(&[t])), "{t} must be dropped");
        }
    }

    #[test]
    fn other_surface_backlog_does_not_hide_a_scenario() {
        let sel = Selection::default();
        assert!(sel.keeps(&tags(&["backlog-android"])));
        assert!(
            !sel.keeps(&tags(&["android"])),
            "an android scenario is not for the headless runner"
        );
        assert!(sel.keeps(&tags(&["headless", "android"])));
    }

    #[test]
    fn backlog_run_is_the_inverse_minus_parked() {
        let sel = Selection {
            backlog_only: true,
            ..Selection::default()
        };
        assert!(sel.keeps(&tags(&["backlog"])));
        assert!(sel.keeps(&tags(&["backlog-headless"])));
        assert!(!sel.keeps(&tags(&["qt-parity"])));
        assert!(!sel.keeps(&tags(&["backlog", "blocked"])));
        assert!(!sel.keeps(&tags(&["backlog", "dropped"])));
    }

    #[test]
    fn oracle_run_verifies_backlog_expectations_too() {
        let sel = Selection {
            oracle_only: true,
            require_tag: Some("qt-parity".into()),
            ..Selection::default()
        };
        assert!(sel.keeps(&tags(&["qt-parity", "backlog"])));
        assert!(!sel.keeps(&tags(&["qt-parity", "dropped"])));
        assert!(
            !sel.keeps(&tags(&["backlog"])),
            "still needs the oracle tag"
        );
    }

    #[test]
    fn tags_may_be_written_with_the_at_sign() {
        assert!(!Selection::default().keeps(&tags(&["@backlog"])));
    }

    #[test]
    fn required_tag_limits_the_run() {
        let sel = Selection {
            require_tag: Some("qt-parity".into()),
            ..Selection::default()
        };
        assert!(sel.keeps(&tags(&["qt-parity"])));
        assert!(!sel.keeps(&tags(&["ljos-only"])));
        assert!(!sel.keeps(&tags(&["qt-parity", "backlog"])));
    }

    fn feature(src: &str) -> Feature {
        Feature::parse(src, gherkin::GherkinEnv::default()).unwrap()
    }

    fn names(f: &Feature) -> Vec<String> {
        f.scenarios
            .iter()
            .map(|s| s.name.clone())
            .chain(
                f.rules
                    .iter()
                    .flat_map(|r| r.scenarios.iter().map(|s| s.name.clone())),
            )
            .collect()
    }

    const SRC: &str = "\
@qt-parity
Feature: F
  Scenario: kept
    Given x
  @backlog
  Scenario: parked
    Given x
  Rule: R
    @blocked
    Scenario: blocked in rule
      Given x
    Scenario: kept in rule
      Given x
";

    #[test]
    fn filters_scenarios_and_rules() {
        let f = Selection::default().filter(feature(SRC));
        assert_eq!(names(&f), ["kept", "kept in rule"]);
    }

    #[test]
    fn feature_level_backlog_drops_everything() {
        let f = Selection::default().filter(feature(
            "@backlog\nFeature: F\n  Scenario: a\n    Given x\n",
        ));
        assert!(names(&f).is_empty());
        let f = Selection {
            backlog_only: true,
            ..Selection::default()
        }
        .filter(feature(
            "@backlog\nFeature: F\n  Scenario: a\n    Given x\n",
        ));
        assert_eq!(names(&f), ["a"]);
    }

    #[test]
    fn example_table_tags_reach_the_expanded_scenario() {
        // cucumber's Basic parser expands outlines before we see them; the
        // expansion must carry the Examples tags or `@backlog` rows would leak.
        let src = "Feature: F\n  Scenario Outline: o\n    Given <a>\n    @backlog\n    Examples:\n      | a |\n      | 1 |\n";
        let f = gherkin::Feature::parse(src, gherkin::GherkinEnv::default()).unwrap();
        let expanded = cucumber::feature::Ext::expand_examples(f).unwrap();
        assert_eq!(expanded.scenarios.len(), 1);
        assert!(Selection::default().filter(expanded).scenarios.is_empty());
    }
}
