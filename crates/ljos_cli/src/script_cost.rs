//! The cheaper forms `ljos run`'s inspector names in a script-cost hint
//! (`app_frames` with `script`, docs/script-cost.md).
//!
//! The inspector lives below the authoring tools, so it cannot ask them
//! itself: this binary links both and hands it an advisor. The advisor
//! reads and compiles the files away from the UI thread.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ljos_dev::mcp::{Cheaper, CheaperForm, ScriptAdvisor, ScriptPlace, set_script_advisor};
use ljos_mcp_author::{CheaperKind, Place, Project, cheaper_forms};

/// Registers the advisor for the project in `dir`.
pub(crate) fn register(dir: &Path) {
    if let Ok(project) = Project::open(dir) {
        set_script_advisor(Arc::new(Advisor { project }));
    }
}

struct Advisor {
    project: Project,
}

impl Advisor {
    /// The project a file is read in and its name there: the run's, or for
    /// a file outside it (an import from elsewhere) its own directory.
    fn locate(&self, file: &str) -> Option<(Project, String)> {
        let path = match url::Url::parse(file) {
            Ok(url) if url.scheme() == "file" => url.to_file_path().ok()?,
            Ok(_) => return None,
            Err(_) => PathBuf::from(file),
        };
        let path = path.canonicalize().ok()?;
        if let Ok(inside) = path.strip_prefix(self.project.root()) {
            return Some((self.project.clone(), inside.to_str()?.to_owned()));
        }
        let project = Project::open(path.parent()?).ok()?;
        Some((project, path.file_name()?.to_str()?.to_owned()))
    }
}

impl ScriptAdvisor for Advisor {
    fn advise(&self, places: &[ScriptPlace]) -> Vec<Option<Cheaper>> {
        let mut found = vec![None; places.len()];
        // Each file is compiled once for all of its places.
        let mut by_file: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for (i, place) in places.iter().enumerate() {
            by_file.entry(place.file.as_str()).or_default().push(i);
        }
        for (file, indices) in by_file {
            let Some((project, name)) = self.locate(file) else {
                continue;
            };
            let asked: Vec<Place> = indices
                .iter()
                .map(|&i| Place {
                    line: places[i].line,
                    column: places[i].column,
                })
                .collect();
            let Ok(forms) = cheaper_forms(&project, &name, &asked) else {
                continue;
            };
            for (i, form) in indices.into_iter().zip(forms) {
                found[i] = form.map(|form| cheaper(form, places[i].handler));
            }
        }
        found
    }
}

fn cheaper(form: ljos_mcp_author::CheaperForm, handler: bool) -> Cheaper {
    match form.kind {
        CheaperKind::Rewrite => Cheaper {
            form: CheaperForm::Rewrite,
            text: form.text,
            source: form.source,
            tier: form.tier.map(str::to_owned),
            code: None,
            guide: None,
            reason: None,
        },
        CheaperKind::Declarative => Cheaper {
            form: CheaperForm::Declarative,
            text: form.text,
            source: None,
            tier: None,
            code: form.code,
            guide: form.guide,
            reason: None,
        },
        // What the compiler says is no form in QML: the work moves into
        // Rust, and the author is told why and what else helps.
        CheaperKind::Neither => {
            let mut rust = Cheaper::rust(handler, form.reason);
            rust.text = format!("{} {}", rust.text, form.text);
            rust
        }
    }
}
