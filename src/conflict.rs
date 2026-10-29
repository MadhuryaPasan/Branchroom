use crate::{
    git::git,
    process::{self, Result},
};
use std::path::Path;
pub fn save(root: &Path, path: &str, content: &str, original: &str) -> Result<String> {
    crate::safety::ensure_writable(root)?;
    let target = crate::recovery::checked_path(root, path)?;
    if content.lines().any(|line| {
        line.starts_with("<<<<<<< ")
            || line == "======="
            || line.starts_with(">>>>>>> ")
            || line.starts_with("||||||| ")
    }) {
        return Err("Remove every conflict marker before saving the resolution.".into());
    }
    crate::commit_lock::guard_add(root, &process::strings(&["add", "--", path]))?;
    if std::fs::read_to_string(&target).map_err(|e| e.to_string())? != original {
        return Err("This file changed outside the editor. Reload it before saving.".into());
    }
    let safety = crate::recovery::backup(root, path)?;
    crate::storage::write_atomic(&target, content.as_bytes())?;
    git(root, &["add", "--", path]).map(|output| {
        format!(
            "{output}\nPrevious file saved in Time Machine: {}",
            safety.id
        )
    })
}
