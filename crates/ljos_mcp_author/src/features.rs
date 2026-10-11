//! `features_list`: the scenarios under `features/`, and the choice of them
//! that `features_run` runs.
//!
//! A scenario is read as the runner reads it (the same Gherkin parser, the
//! same rule for what is backlog), so what is listed here is what a run
//! with the same arguments picks up.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use gherkin::{Feature, GherkinEnv, Scenario, tagexpr::TagOperation};
use ljos_mcp_core::{CallCtx, SurfaceError, ToolHandler, typed};
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Project, tools};

const PAGE: usize = 100;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListArgs {
    /// `.feature` files or directories under `features/`, with or without
    /// that prefix: `items`, `features/tooling/mcp`. All of them if absent.
    #[serde(default)]
    paths: Vec<String>,
    /// Tag expression: `@qt-parity`, `@a and not @b`, `(@a or @b) and @c`.
    tags: Option<String>,
    /// Regular expression the scenario's name must match.
    name: Option<String>,
    /// `next_cursor` of the previous answer.
    cursor: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum Status {
    /// Runs in a plain `features_run`.
    Active,
    /// Tagged `@backlog`: behaviour that is specified and not built yet.
    /// Runs only with `backlog: true`.
    Backlog,
    /// Never runs headless: `@dropped`, `@blocked`, or for another surface.
    Skipped,
}

#[derive(Serialize, JsonSchema)]
struct ScenarioOut {
    /// Project-relative path of the `.feature` file.
    file: String,
    /// 1-based line of the scenario.
    line: usize,
    feature: String,
    /// The `Rule:` the scenario is under, if any.
    rule: Option<String>,
    name: String,
    /// The feature's, the rule's and the scenario's own tags.
    tags: Vec<String>,
    status: Status,
    /// Rows of examples a scenario outline runs for; 0 for a scenario.
    examples: usize,
}

#[derive(Serialize, JsonSchema)]
struct Unreadable {
    file: String,
    /// What the Gherkin parser said, with the line where it gave up.
    error: String,
}

#[derive(Serialize, JsonSchema)]
pub(crate) struct ListOut {
    scenarios: Vec<ScenarioOut>,
    /// Scenarios chosen, over every page of the answer.
    total: usize,
    /// Feature files those scenarios are in.
    files: usize,
    /// Feature files that are not valid Gherkin. The runner refuses them.
    unreadable: Vec<Unreadable>,
    next_cursor: Option<String>,
}

/// Something `paths` named.
pub(crate) enum Chosen {
    /// A directory, as its path below `features/` (empty for all of it).
    Dir(PathBuf, String),
    /// One `.feature` file.
    File(PathBuf),
}

/// Which scenarios are meant: what `features_list` lists and `features_run`
/// runs.
pub(crate) struct Choice {
    pub(crate) paths: Vec<Chosen>,
    /// The tag expression as it was given, and parsed.
    pub(crate) tags: Option<(String, TagOperation)>,
    pub(crate) name: Option<Regex>,
}

fn has(tags: &[String], tag: &str) -> bool {
    tags.iter().any(|own| own.trim_start_matches('@') == tag)
}

fn holds(expression: &TagOperation, tags: &[String]) -> bool {
    match expression {
        TagOperation::And(a, b) => holds(a, tags) && holds(b, tags),
        TagOperation::Or(a, b) => holds(a, tags) || holds(b, tags),
        TagOperation::Not(a) => !holds(a, tags),
        TagOperation::Tag(tag) => has(tags, tag),
    }
}

impl Choice {
    /// Reads the arguments the two tools share. A path that is not there is
    /// `not_found` before anything is listed or run.
    pub(crate) fn of(
        project: &Project,
        paths: &[String],
        tags: Option<&str>,
        name: Option<&str>,
    ) -> Result<Self, SurfaceError> {
        let dir = project.features_dir();
        if !dir.is_dir() {
            return Err(SurfaceError::not_found(
                "the project has no `features/` directory, so it has no scenarios",
            ));
        }
        let mut chosen = Vec::new();
        for requested in paths {
            let path = resolve(project, &dir, requested)?;
            if path.is_dir() {
                let below = path.strip_prefix(dir.canonicalize().unwrap_or(dir.clone()));
                let below = below.unwrap_or(Path::new(""));
                chosen.push(Chosen::Dir(
                    path.clone(),
                    below.to_string_lossy().replace('\\', "/"),
                ));
            } else if path.extension().is_some_and(|e| e == "feature") {
                chosen.push(Chosen::File(path));
            } else {
                return Err(SurfaceError::invalid_argument(format!(
                    "`{requested}` is not a `.feature` file or a directory"
                )));
            }
        }
        let tags = match tags.map(str::trim).filter(|t| !t.is_empty()) {
            Some(text) => Some((
                text.to_owned(),
                text.parse::<TagOperation>().map_err(|e| {
                    SurfaceError::invalid_argument(format!(
                        "`{text}` is not a tag expression (column {}): write tags with their `@` \
                         and join them with `and`, `or`, `not` and brackets, as in `@a and not @b`",
                        e.location.column
                    ))
                })?,
            )),
            None => None,
        };
        let name = match name.filter(|n| !n.is_empty()) {
            Some(pattern) => Some(Regex::new(pattern).map_err(|e| {
                SurfaceError::invalid_argument(format!(
                    "`name` is not a regular expression: {}",
                    e.to_string().lines().last().unwrap_or_default().trim()
                ))
            })?),
            None => None,
        };
        Ok(Self {
            paths: chosen,
            tags,
            name,
        })
    }

    fn keeps(&self, name: &str, tags: &[String]) -> bool {
        self.tags.as_ref().is_none_or(|(_, e)| holds(e, tags))
            && self.name.as_ref().is_none_or(|n| n.is_match(name))
    }

    /// The feature files the paths cover, each once, in path order.
    fn files(&self, project: &Project) -> Vec<PathBuf> {
        let mut out = Vec::new();
        if self.paths.is_empty() {
            tools::collect(&project.features_dir(), "feature", &mut out);
        }
        for chosen in &self.paths {
            match chosen {
                Chosen::Dir(path, _) => tools::collect(path, "feature", &mut out),
                Chosen::File(path) => out.push(path.clone()),
            }
        }
        // A project root that is a link is canonical in a chosen path and
        // may not be in a walked one; one form, so a file is listed once.
        for path in &mut out {
            if let Ok(canonical) = path.canonicalize() {
                *path = canonical;
            }
        }
        out.sort();
        out.dedup();
        out
    }
}

/// `requested` as a path below `features/`. It may be given from the
/// project root (`features/items`) or from `features/` (`items`).
fn resolve(project: &Project, dir: &Path, requested: &str) -> Result<PathBuf, SurfaceError> {
    let trimmed = requested.trim_end_matches('/');
    if trimmed == "features" || trimmed.is_empty() || trimmed == "." {
        return project.confine_within(dir, ".");
    }
    let below = trimmed.strip_prefix("features/").unwrap_or(trimmed);
    match project.confine_within(dir, below) {
        Ok(path) => Ok(path),
        // `features/features/x` is a directory a project may have.
        Err(first) if below != trimmed => project.confine_within(dir, trimmed).or(Err(first)),
        Err(error) => Err(error.message_for(requested, below)),
    }
}

/// Names the path as the agent wrote it, and says where it was looked for.
trait MessageFor {
    fn message_for(self, requested: &str, below: &str) -> Self;
}

impl MessageFor for SurfaceError {
    fn message_for(self, requested: &str, below: &str) -> Self {
        if self.code == ljos_mcp_core::ErrorCode::NotFound {
            SurfaceError::not_found(format!(
                "`features/{below}` does not exist (asked for `{requested}`); `features_list` \
                 without `paths` shows what there is"
            ))
        } else {
            self
        }
    }
}

fn with_at(tags: &[String]) -> impl Iterator<Item = String> + '_ {
    tags.iter()
        .map(|tag| format!("@{}", tag.trim_start_matches('@')))
}

fn status(tags: &[String]) -> Status {
    use ljos_bdd::filter::Selection;
    if Selection::default().keeps(tags) {
        Status::Active
    } else if (Selection {
        backlog_only: true,
        ..Selection::default()
    })
    .keeps(tags)
    {
        Status::Backlog
    } else {
        Status::Skipped
    }
}

fn rows(scenario: &Scenario) -> usize {
    scenario
        .examples
        .iter()
        .filter_map(|e| e.table.as_ref())
        .map(|table| table.rows.len().saturating_sub(1))
        .sum()
}

fn list(project: &Project, args: &ListArgs) -> Result<ListOut, SurfaceError> {
    let choice = Choice::of(
        project,
        &args.paths,
        args.tags.as_deref(),
        args.name.as_deref(),
    )?;
    let mut scenarios = Vec::new();
    let mut unreadable = Vec::new();
    let mut files = 0;
    for path in choice.files(project) {
        let file = project.display(&path);
        let feature = match Feature::parse_path(&path, GherkinEnv::default()) {
            Ok(feature) => feature,
            Err(error) => {
                unreadable.push(Unreadable {
                    file,
                    error: error.to_string(),
                });
                continue;
            }
        };
        let before = scenarios.len();
        let mut take = |rule: Option<&gherkin::Rule>, scenario: &Scenario| {
            let tags: Vec<String> = with_at(&feature.tags)
                .chain(rule.into_iter().flat_map(|r| with_at(&r.tags)))
                .chain(with_at(&scenario.tags))
                .collect();
            if choice.keeps(&scenario.name, &tags) {
                scenarios.push(ScenarioOut {
                    file: file.clone(),
                    line: scenario.position.line,
                    feature: feature.name.clone(),
                    rule: rule.map(|r| r.name.clone()),
                    name: scenario.name.clone(),
                    status: status(&tags),
                    tags,
                    examples: rows(scenario),
                });
            }
        };
        for scenario in &feature.scenarios {
            take(None, scenario);
        }
        for rule in &feature.rules {
            for scenario in &rule.scenarios {
                take(Some(rule), scenario);
            }
        }
        // Rules and bare scenarios are read apart; the file's own order is
        // what an agent that opens it sees.
        scenarios[before..].sort_by_key(|s| s.line);
        if scenarios.len() > before {
            files += 1;
        }
    }
    let total = scenarios.len();
    unreadable.truncate(20);
    let (scenarios, next_cursor) =
        tools::page(scenarios, args.cursor.as_deref(), PAGE, tools::BUDGET)?;
    Ok(ListOut {
        scenarios,
        total,
        files,
        unreadable,
        next_cursor,
    })
}

pub(crate) const LIST: &str = "List the scenarios under features/: file, line, feature, rule, name, tags and status (active, backlog or skipped). Narrow by `paths`, a tag expression (`@a and not @b`) or a `name` regex. Paginated.";

pub(crate) fn handler(project: &Arc<Project>) -> ToolHandler {
    let project = project.clone();
    typed(move |_: &CallCtx, args: ListArgs| list(&project, &args))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(list: &[&str]) -> Vec<String> {
        list.iter().map(|t| (*t).to_owned()).collect()
    }

    #[test]
    fn a_tag_expression_is_held_against_tags_with_or_without_their_at() {
        let expression: TagOperation = "@a and not (@b or @c)".parse().unwrap();
        assert!(holds(&expression, &tags(&["@a", "@d"])));
        assert!(holds(&expression, &tags(&["a"])));
        assert!(!holds(&expression, &tags(&["@a", "@c"])));
        assert!(!holds(&expression, &tags(&["@d"])));
        let dotted: TagOperation = "@api-QtQuick.Item.width".parse().unwrap();
        assert!(holds(&dotted, &tags(&["@api-QtQuick.Item.width"])));
        assert!(!holds(&dotted, &tags(&["@api-QtQuick.Item.height"])));
    }

    #[test]
    fn status_is_the_runners_own_rule() {
        assert!(status(&tags(&["@qt-parity"])) == Status::Active);
        assert!(status(&tags(&["@backlog"])) == Status::Backlog);
        assert!(status(&tags(&["@backlog-headless"])) == Status::Backlog);
        assert!(status(&tags(&["@backlog-android"])) == Status::Active);
        assert!(status(&tags(&["@dropped"])) == Status::Skipped);
        assert!(status(&tags(&["@desktop"])) == Status::Skipped);
    }
}
