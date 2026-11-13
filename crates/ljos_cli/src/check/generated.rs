//! What `ljos check` writes and says for a project's `.lts` modules: their
//! declarations in `.ljos/`, and whether its `tsconfig.json` finds them.

use std::path::{Path, PathBuf};

use super::Checked;

/// Writes the `.ljos/` of the project of `dirs`, whose root is `explicit`
/// or else found, and says what is wrong with it, each file named as it is
/// below `shown_in`.
pub fn write(
    explicit: Option<&Path>,
    dirs: &[PathBuf],
    shown_in: &[PathBuf],
    checked: &mut Checked,
) -> anyhow::Result<()> {
    let Some(lts) = crate::generated::find(explicit, dirs)? else {
        return Ok(());
    };
    let shown = |path: &Path| crate::generated::shown(shown_in, path);
    // A module outside the root has no place in its `.ljos/types/`, and a
    // `.ljos/` written without it would say it is not there: each is a
    // finding, and nothing is written.
    let outside: Vec<&Path> = lts
        .modules
        .iter()
        .filter(|(_, key)| key.is_none())
        .map(|(path, _)| path.as_path())
        .collect();
    for module in &outside {
        checked.errors += 1;
        checked.diagnostics.push(format!(
            "{}:1:1: error: `{}` is outside the project root `{}`, so it can have no declaration \
             there, and nothing is written [{}]",
            shown(module),
            shown(module),
            shown(&lts.root),
            ljos_compiler::Code::LtsGenerated.as_str()
        ));
    }
    if outside.is_empty()
        && let Err(e) = lts.write()
    {
        // A directory that cannot be written is a finding beside the
        // others, not the end of the check: what it found is still said.
        checked.errors += 1;
        checked.diagnostics.push(format!(
            "{}: error: cannot write the declarations of the `.lts` modules here: {} [{}]",
            shown(&lts.generated_dir()),
            e.root_cause(),
            ljos_compiler::Code::LtsGenerated.as_str()
        ));
    }
    if let Some(warning) = lts.tsconfig_warning(shown) {
        checked.warnings += 1;
        checked.diagnostics.push(warning);
    }
    Ok(())
}
