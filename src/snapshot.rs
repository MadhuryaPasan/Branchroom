use crate::{
    git::{git, resolve, validate_branch},
    process::{self, Result},
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn unique_id() -> String {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    format!(
        "{:032x}{:08x}{:016x}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        std::process::id(),
        SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

struct TemporaryIndex(PathBuf);
impl Drop for TemporaryIndex {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
        let _ = fs::remove_file(self.0.with_extension("index.lock"));
    }
}

pub fn create(root: &Path, label: &str) -> Result<String> {
    crate::safety::ensure_writable(root)?;
    if label.trim().is_empty() {
        return Err("Give this snapshot a name.".into());
    }
    if !git(root, &["diff", "--name-only", "--diff-filter=U"])?
        .trim()
        .is_empty()
    {
        return Err("Resolve conflicts before capturing a snapshot.".into());
    }
    let id = unique_id();
    let index = TemporaryIndex(std::env::temp_dir().join(format!("branchroom-{id}.index")));
    // A private index captures working files without staging them in the user's index.
    let indexed = |args: &[&str]| -> Result<String> {
        process::input(
            "git",
            root,
            &process::strings(args),
            &[],
            &[("GIT_INDEX_FILE".into(), index.0.display().to_string())],
        )
        .map(|text| text.trim().to_owned())
    };
    let head = resolve(root, "HEAD").ok();
    if let Some(head) = &head {
        indexed(&["read-tree", head])?;
    } else {
        indexed(&["read-tree", "--empty"])?;
    }
    indexed(&["add", "-A", "--", "."])?;
    let tree = indexed(&["write-tree"])?;
    let mut args = vec!["commit-tree", tree.as_str(), "-m", label];
    if let Some(head) = &head {
        args.extend(["-p", head.as_str()]);
    }
    let commit = git(root, &args)?;
    let reference = format!("refs/branchroom/snapshots/{id}");
    git(root, &["update-ref", &reference, commit.trim(), ""])?;
    Ok(format!("Saved {label}\n{reference}\n{}", commit.trim()))
}

pub fn materialize(root: &Path, reference: &str, destination: &Path) -> Result<String> {
    crate::safety::ensure_writable(root)?;
    crate::safety::ensure_writable(destination)?;
    let id = resolve(root, reference)?;
    if destination.exists() {
        return Err("Choose a new directory for the snapshot worktree.".into());
    }
    git(
        root,
        &[
            "worktree",
            "add",
            "--detach",
            &destination.to_string_lossy(),
            &id,
        ],
    )
}

pub fn promote(root: &Path, reference: &str, branch: &str) -> Result<String> {
    crate::safety::ensure_writable(root)?;
    validate_branch(root, branch)?;
    let id = resolve(root, reference)?;
    git(root, &["branch", branch, &id])
}

fn managed_ref(root: &Path, reference: &str, prefix: &str) -> Result<String> {
    if !reference
        .strip_prefix(prefix)
        .is_some_and(|suffix| !suffix.is_empty() && !suffix.contains('/'))
    {
        return Err("Choose a saved snapshot from this repository.".into());
    }
    git(root, &["check-ref-format", reference])?;
    // Refuse symbolic refs; only the saved capture itself may move.
    if git(root, &["symbolic-ref", "--quiet", reference]).is_ok() {
        return Err("This snapshot reference is symbolic and cannot be removed here.".into());
    }
    Ok(git(root, &["show-ref", "--verify", "--hash", reference])?
        .trim()
        .into())
}

/// Move the capture ref into recovery without changing any worktree or branch.
pub fn remove(root: &Path, reference: &str) -> Result<String> {
    crate::safety::ensure_writable(root)?;
    let id = managed_ref(root, reference, "refs/branchroom/snapshots/")?;
    let recovery = format!("refs/branchroom/deleted-snapshots/{}", unique_id());
    let transaction =
        format!("start\ncreate {recovery} {id}\ndelete {reference} {id}\nprepare\ncommit\n");
    crate::process::input(
        "git",
        root,
        &crate::process::strings(&["update-ref", "--no-deref", "--stdin"]),
        transaction.as_bytes(),
        &[],
    )?;
    Ok(recovery)
}

pub fn restore(root: &Path, recovery: &str) -> Result<String> {
    crate::safety::ensure_writable(root)?;
    let id = managed_ref(root, recovery, "refs/branchroom/deleted-snapshots/")?;
    let reference = format!("refs/branchroom/snapshots/{}", unique_id());
    let transaction =
        format!("start\ncreate {reference} {id}\ndelete {recovery} {id}\nprepare\ncommit\n");
    crate::process::input(
        "git",
        root,
        &crate::process::strings(&["update-ref", "--no-deref", "--stdin"]),
        transaction.as_bytes(),
        &[],
    )?;
    Ok(reference)
}
