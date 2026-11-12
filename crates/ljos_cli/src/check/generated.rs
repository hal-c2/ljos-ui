//! What `ljos check` writes and says for a project's `.lts` modules: their
//! declarations in `.ljos/`, and whether its `tsconfig.json` finds them.

use std::path::{Path, PathBuf};

use super::Checked;

pub fn write(
    explicit: Option<&Path>,
    dirs: &[PathBuf],
    checked: &mut Checked,
) -> anyhow::Result<()> {
    let Some(lts) = crate::generated::find(explicit, dirs)? else {
        return Ok(());
    };
    lts.write()?;
    if let Some(warning) = lts.tsconfig_warning(|path| crate::generated::shown(dirs, path)) {
        checked.warnings += 1;
        checked.diagnostics.push(warning);
    }
    Ok(())
}
