use crate::{
    git::{self, git},
    process::{self, Result},
};
use std::path::Path;
#[derive(Clone, Debug)]
pub struct Plan {
    pub head: Option<String>,
    pub tree: String,
    pub branch: String,
    pub files: Vec<String>,
    pub message: String,
    pub amend: bool,
}
pub fn prepare(root: &Path, message: &str, amend: bool) -> Result<Plan> {
    crate::safety::ensure_writable(root)?;
    if message.trim().is_empty() {
        return Err("Enter a commit summary.".into());
    }
    let status = git::parse_status(&git(
        root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?);
    if status.iter().any(git::Change::conflict) {
        return Err("Resolve and stage every conflicted file before committing.".into());
    }
    let files: Vec<_> = git(
        root,
        &["diff", "--cached", "--name-only", "--no-renames", "-z"],
    )?
    .split('\0')
    .filter(|s| !s.is_empty())
    .map(str::to_owned)
    .collect();
    if files.is_empty() && !amend {
        return Err("Include at least one file or line before committing.".into());
    }
    process_guard(root)?;
    Ok(Plan {
        head: git::resolve(root, "HEAD").ok(),
        tree: git(root, &["write-tree"])?.trim().into(),
        branch: git::current_branch(root)
            .unwrap_or_else(|_| "Detached HEAD".into())
            .trim()
            .into(),
        files,
        message: message.into(),
        amend,
    })
}
fn process_guard(root: &Path) -> Result<()> {
    crate::commit_lock::guard_commit(root, &process::strings(&["commit", "-F", "-"]))
}
pub fn execute(root: &Path, plan: &Plan) -> Result<String> {
    crate::safety::ensure_writable(root)?;
    if git::current_branch(root)
        .unwrap_or_else(|_| "Detached HEAD".into())
        .trim()
        != plan.branch
        || git::resolve(root, "HEAD").ok() != plan.head
        || git(root, &["write-tree"])?.trim() != plan.tree
    {
        return Err("The branch or staged files changed after the commit preview. Review the staged changes and commit again.".into());
    }
    let mut args = process::strings(&["commit", "-F", "-"]);
    if plan.amend {
        args.push("--amend".into());
    }
    process::input("git", root, &args, plan.message.as_bytes(), &[])
}
