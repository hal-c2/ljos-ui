use std::sync::Arc;

use ljos_mcp_core::{
    ErrorCode, HARD_RESULT_BYTES, ResourceContent, ResourceDef, ResourceTemplateDef, SurfaceError,
};

use crate::{FrontEnd, Project};

const MD: &str = "text/markdown";

pub(crate) fn templates() -> Vec<ResourceTemplateDef> {
    vec![
        ResourceTemplateDef {
            uri_template: "ljos://types/{module}/{type}",
            name: "type",
            description: "Type, properties, signals and methods of a QML type.",
            mime_type: "application/json",
        },
        ResourceTemplateDef {
            uri_template: "ljos://types/{module}",
            name: "module",
            description: "The types a QML module exports; read one with ljos://types/{module}/{type}.",
            mime_type: "application/json",
        },
        ResourceTemplateDef {
            uri_template: "ljos://docs/{path}",
            name: "docs",
            description: "A file from the repository's docs/ directory.",
            mime_type: MD,
        },
    ]
}

pub(crate) fn list(project: &Project) -> Vec<ResourceDef> {
    let mut out = vec![ResourceDef {
        uri: "ljos://ledger".into(),
        name: "ledger".into(),
        description: "API ledger: status of every Qt Quick type and member (TOML).".into(),
        mime_type: "application/toml",
    }];
    if project.features_readme().is_file() {
        out.push(ResourceDef {
            uri: "ljos://features/steps".into(),
            name: "feature steps".into(),
            description: "Step catalogue for writing .feature scenarios.".into(),
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

fn read_capped(path: &std::path::Path) -> Result<String, SurfaceError> {
    let bytes = std::fs::read(path)
        .map_err(|e| SurfaceError::new(ErrorCode::Internal, format!("{}: {e}", path.display())))?;
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if text.len() > HARD_RESULT_BYTES {
        let mut cut = HARD_RESULT_BYTES;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        text.push_str("\n\n[truncated: file exceeds the 80 KB response cap]\n");
    }
    Ok(text)
}

/// The `## …` (or any level) section whose heading mentions "step" and
/// "catalog(ue)", up to the next heading of the same or a higher level.
fn steps_section(readme: &str) -> Option<String> {
    let level = |l: &str| l.chars().take_while(|c| *c == '#').count();
    let lines: Vec<&str> = readme.lines().collect();
    let start = lines.iter().position(|l| {
        let lower = l.to_lowercase();
        l.starts_with('#') && lower.contains("step") && lower.contains("catalog")
    })?;
    let depth = level(lines[start]);
    let end = lines[start + 1..]
        .iter()
        .position(|l| l.starts_with('#') && level(l) <= depth)
        .map_or(lines.len(), |i| start + 1 + i);
    Some(lines[start..end].join("\n"))
}

fn pretty(value: &impl serde::Serialize) -> Result<String, SurfaceError> {
    serde_json::to_string_pretty(value)
        .map_err(|e| SurfaceError::new(ErrorCode::Internal, e.to_string()))
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
        let path = project.ledger_path();
        if !path.is_file() {
            return Err(SurfaceError::not_found(
                "ledger/qtquick.toml does not exist",
            ));
        }
        return one(read_capped(&path)?, "application/toml");
    }
    if uri == "ljos://features/steps" {
        let path = project.features_readme();
        if !path.is_file() {
            return Err(SurfaceError::not_found("features/README.md does not exist"));
        }
        let readme = read_capped(&path)?;
        return match steps_section(&readme) {
            Some(section) => one(section, MD),
            None => Err(SurfaceError::not_found(
                "features/README.md has no step catalogue section yet",
            )),
        };
    }
    if let Some(rest) = uri.strip_prefix("ljos://docs/") {
        let path = project.confine_within(&project.docs_dir(), rest)?;
        if !path.is_file() {
            return Err(SurfaceError::not_found(format!("`{rest}` is not a file")));
        }
        return one(read_capped(&path)?, MD);
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
        return one(text, "application/json");
    }
    Err(SurfaceError::not_found(format!("no resource `{uri}`")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_section_is_cut_at_the_next_peer_heading() {
        let readme =
            "# Features\n\n## Step catalogue\n\nGiven x\n\n### Sub\n\nmore\n\n## Tags\n\nno\n";
        let s = steps_section(readme).unwrap();
        assert!(s.contains("Given x") && s.contains("more") && !s.contains("Tags"));
        assert!(steps_section("# Features\n").is_none());
    }
}
