//! The pages an agent reads: the project's documents and scenarios, the
//! ledger, the Qt types, and two pages written from what they describe (the
//! steps a scenario may use, the lint rules).

use std::{path::Path, sync::Arc};

use ljos_mcp_core::{
    ErrorCode, HARD_RESULT_BYTES, ResourceContent, ResourceDef, ResourceTemplateDef, SurfaceError,
};

use crate::{FrontEnd, Ledger, LedgerRow, Project, docs, lints};

const MD: &str = "text/markdown";
const PLAIN: &str = "text/plain";
const TOML: &str = "application/toml";
const JSON: &str = "application/json";

pub(crate) fn templates() -> Vec<ResourceTemplateDef> {
    vec![
        ResourceTemplateDef {
            uri_template: "ljos://types/{module}/{type}",
            name: "type",
            description: "Type, properties, signals and methods of a QML type.",
            mime_type: JSON,
        },
        ResourceTemplateDef {
            uri_template: "ljos://types/{module}",
            name: "module",
            description: "The types a QML module exports; read one with ljos://types/{module}/{type}.",
            mime_type: JSON,
        },
        ResourceTemplateDef {
            uri_template: "ljos://docs/{path}",
            name: "docs",
            description: "A file below the project's docs/ directory, or the list of a directory. With `#L<line>` at the end, the section that starts at that line: `docs_search` answers such links.",
            mime_type: MD,
        },
        ResourceTemplateDef {
            uri_template: "ljos://features/{path}",
            name: "features",
            description: "A file below the project's features/ directory (a `.feature`, or README.md with the rules for writing one), or the list of a directory.",
            mime_type: PLAIN,
        },
        ResourceTemplateDef {
            uri_template: "ljos://ledger/{module}/{type}",
            name: "ledger rows",
            description: "The API ledger's rows for one type and its members: status, reason and the scenarios that prove it.",
            mime_type: JSON,
        },
    ]
}

pub(crate) fn list(project: &Project) -> Vec<ResourceDef> {
    let mut out = vec![
        ResourceDef {
            uri: "ljos://ledger".into(),
            name: "ledger".into(),
            description: "API ledger: status of every Qt Quick type and member (TOML).".into(),
            mime_type: TOML,
        },
        ResourceDef {
            uri: "ljos://features/steps".into(),
            name: "feature steps".into(),
            description:
                "Every step a .feature scenario may use, read from the runner's own steps.".into(),
            mime_type: MD,
        },
        ResourceDef {
            uri: "ljos://lints".into(),
            name: "lint rules".into(),
            description:
                "The rules of `qml_lint`: code, severity, what is wrong and how to fix it.".into(),
            mime_type: MD,
        },
    ];
    if project.features_readme().is_file() {
        out.push(ResourceDef {
            uri: "ljos://features/README.md".into(),
            name: "features/README.md".into(),
            description: "How scenarios are written here: tags, sources, wording.".into(),
            mime_type: MD,
        });
    }
    // Top level only: listing a whole tree would spend the agent's context.
    if let Ok(read) = std::fs::read_dir(project.docs_dir()) {
        let mut docs: Vec<String> = read
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.ends_with(".md"))
            .collect();
        docs.sort();
        out.extend(docs.into_iter().map(|n| ResourceDef {
            uri: format!("ljos://docs/{n}"),
            name: n.clone(),
            description: format!("docs/{n}"),
            mime_type: MD,
        }));
    }
    out
}

/// `text` cut to what one answer may carry, with `hint` saying how to read
/// what was left out.
fn capped(mut text: String, hint: &str) -> String {
    if text.len() > HARD_RESULT_BYTES {
        let whole = text.len();
        let mut cut = HARD_RESULT_BYTES;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        text.push_str(&format!(
            "\n\n[truncated: {} KB of {} KB shown. {hint}]\n",
            cut / 1024,
            whole / 1024
        ));
    }
    text
}

fn read_text(path: &Path) -> Result<String, SurfaceError> {
    let bytes = std::fs::read(path)
        .map_err(|e| SurfaceError::new(ErrorCode::Internal, format!("{}: {e}", path.display())))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("md") => MD,
        Some("toml") => TOML,
        Some("json") => JSON,
        _ => PLAIN,
    }
}

/// What is in a directory, as links that can be read in turn.
fn listing(dir: &Path, base: &str, shown: &str) -> Result<String, SurfaceError> {
    let read = std::fs::read_dir(dir)
        .map_err(|e| SurfaceError::new(ErrorCode::Internal, format!("{shown}: {e}")))?;
    let mut entries: Vec<(String, bool)> = read
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let is_dir = e.file_type().ok()?.is_dir();
            (!name.starts_with('.')).then_some((name, is_dir))
        })
        .collect();
    entries.sort();
    let mut out = format!("# {shown}/\n\n");
    if entries.is_empty() {
        out.push_str("(empty)\n");
    }
    for (name, is_dir) in entries {
        let slash = if base.ends_with('/') { "" } else { "/" };
        out.push_str(&format!("- {base}{slash}{name}"));
        out.push_str(if is_dir { " (directory)\n" } else { "\n" });
    }
    Ok(capped(out, "Read a directory below this one for the rest."))
}

/// A file or a directory below `dir`, as `ljos://{scheme}/{rest}`.
fn below(
    project: &Project,
    dir: &Path,
    scheme: &str,
    rest: &str,
) -> Result<(String, &'static str), SurfaceError> {
    // `#L12` after the path asks for the part that starts at that line.
    let (rest, line) = match rest.rsplit_once("#L") {
        Some((path, line)) => (
            path,
            Some(line.parse::<u32>().ok().filter(|l| *l > 0).ok_or_else(|| {
                SurfaceError::invalid_argument(format!(
                    "`#L{line}` is not a line: write `#L` and a line number from 1, as in `#L120`"
                ))
            })?),
        ),
        None => (rest, None),
    };
    let rest = rest.trim_end_matches('/');
    if !dir.is_dir() {
        return Err(SurfaceError::not_found(format!(
            "the project has no `{scheme}/` directory"
        )));
    }
    let path = project.confine_within(dir, if rest.is_empty() { "." } else { rest })?;
    let shown = project.display(&path);
    if path.is_dir() {
        let base = format!("ljos://{scheme}/{rest}");
        return Ok((listing(&path, base.trim_end_matches('/'), &shown)?, MD));
    }
    let text = read_text(&path)?;
    let text = match line {
        None => capped(
            text,
            &format!(
                "Read one section with `ljos://{scheme}/{rest}#L<line>`; `docs_search` answers the \
                 line of each section."
            ),
        ),
        Some(line) => {
            let part = if mime(&path) == MD {
                docs::section_at(&text, line)
            } else {
                // No headings to end at: from the line on.
                let skipped: usize = text.lines().count();
                (line as usize <= skipped).then(|| {
                    text.lines()
                        .skip(line as usize - 1)
                        .fold(String::new(), |mut all, l| {
                            all.push_str(l);
                            all.push('\n');
                            all
                        })
                })
            };
            let part = part.ok_or_else(|| {
                SurfaceError::not_found(format!(
                    "`{shown}` has {} lines, so it has no line {line}",
                    text.lines().count()
                ))
            })?;
            capped(part, "Ask for a line further down for the rest.")
        }
    };
    Ok((text, mime(&path)))
}

fn pretty(value: &impl serde::Serialize) -> Result<String, SurfaceError> {
    serde_json::to_string_pretty(value)
        .map_err(|e| SurfaceError::new(ErrorCode::Internal, e.to_string()))
}

fn ledger_rows(project: &Project, rest: &str) -> Result<String, SurfaceError> {
    let (module, name) = rest
        .split_once('/')
        .filter(|(module, name)| !module.is_empty() && !name.is_empty() && !name.contains('/'))
        .ok_or_else(|| {
            SurfaceError::invalid_argument(
                "expected ljos://ledger/{module}/{type}, as in ljos://ledger/QtQuick/Item",
            )
        })?;
    let ledger = Ledger::load(&project.ledger_file()?)
        .map_err(|e| SurfaceError::not_found(e.to_string()))?;
    let rows: Vec<&LedgerRow> = ledger
        .rows()
        .iter()
        .filter(|row| row.module == module && row.type_name == name)
        .collect();
    if rows.is_empty() {
        return Err(SurfaceError::not_found(format!(
            "the ledger has no row for `{module}.{name}`; `api_status` looks one up by name"
        ))
        .with_details(serde_json::json!({ "modules": ledger.modules() })));
    }
    Ok(capped(
        pretty(&rows)?,
        "`api_status` answers the rows of one member.",
    ))
}

pub(crate) fn read(
    project: &Project,
    front: &Arc<dyn FrontEnd>,
    uri: &str,
) -> Result<Vec<ResourceContent>, SurfaceError> {
    let one = |text: String, mime_type| {
        Ok(vec![ResourceContent {
            uri: uri.to_owned(),
            mime_type,
            text,
        }])
    };
    if uri == "ljos://ledger" {
        let path = project.ledger_file()?;
        if !path.is_file() {
            return Err(SurfaceError::not_found(
                "ledger/qtquick.toml does not exist",
            ));
        }
        return one(
            capped(
                read_text(&path)?,
                "Read one type with ljos://ledger/{module}/{type}.",
            ),
            TOML,
        );
    }
    if let Some(rest) = uri.strip_prefix("ljos://ledger/") {
        return one(ledger_rows(project, rest)?, JSON);
    }
    if uri == "ljos://lints" {
        return one(lints::markdown(), MD);
    }
    if uri == "ljos://features/steps" {
        return one(ljos_bdd::catalogue::markdown(), MD);
    }
    if let Some(rest) = uri.strip_prefix("ljos://features/") {
        let (text, mime_type) = below(project, &project.features_dir(), "features", rest)?;
        return one(text, mime_type);
    }
    if uri == "ljos://features" {
        let (text, mime_type) = below(project, &project.features_dir(), "features", "")?;
        return one(text, mime_type);
    }
    if let Some(rest) = uri.strip_prefix("ljos://docs/") {
        let (text, mime_type) = below(project, &project.docs_dir(), "docs", rest)?;
        return one(text, mime_type);
    }
    if uri == "ljos://docs" {
        let (text, mime_type) = below(project, &project.docs_dir(), "docs", "")?;
        return one(text, mime_type);
    }
    if let Some(rest) = uri.strip_prefix("ljos://types/") {
        let invalid = || {
            SurfaceError::invalid_argument(
                "expected ljos://types/{module} or ljos://types/{module}/{type}",
            )
        };
        let text = match rest.split_once('/') {
            None if !rest.is_empty() => pretty(&front.module_types(rest)?)?,
            Some((module, name))
                if !module.is_empty() && !name.is_empty() && !name.contains('/') =>
            {
                pretty(&front.type_lookup(module, name)?)?
            }
            _ => return Err(invalid()),
        };
        return one(text, JSON);
    }
    Err(SurfaceError::not_found(format!(
        "no resource `{uri}`; there are ljos://docs/{{path}}, ljos://features/{{path}}, \
         ljos://features/steps, ljos://ledger, ljos://ledger/{{module}}/{{type}}, ljos://lints and \
         ljos://types/{{module}}/{{type}}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_too_long_is_cut_and_says_how_to_read_the_rest() {
        let text = capped("é".repeat(HARD_RESULT_BYTES), "Read less.");
        assert!(text.len() < HARD_RESULT_BYTES + 200);
        assert!(text.ends_with("Read less.]\n"));
        assert_eq!(capped("short".into(), "x"), "short");
    }
}
