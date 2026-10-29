//! Branch and review workflows with explicit preconditions at the write boundary.
use crate::{
    git::{self, git as run},
    process::{self, Result},
    snapshot,
};
use std::path::Path;

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub head_ref_name: String,
    pub base_ref_name: String,
    pub url: String,
    pub is_draft: bool,
    pub state: String,
}
pub fn pull_requests(root: &Path) -> Result<Vec<PullRequest>> {
    let output = process::stdout(
        "gh",
        root,
        &process::strings(&[
            "pr",
            "list",
            "--limit",
            "100",
            "--json",
            "number,title,headRefName,baseRefName,url,isDraft,state",
        ]),
    )?;
    serde_json::from_str(&output)
        .map_err(|e| format!("Cannot read pull requests: {e}. Reload the list."))
}
pub fn require_idle(root: &Path) -> Result<()> {
    crate::safety::ensure_writable(root)?;
    if !run(root, &["status", "--porcelain=v1", "-z"])?.is_empty() {
        return Err("Commit or stash your working changes before continuing.".into());
    }
    for marker in [
        "MERGE_HEAD",
        "REVERT_HEAD",
        "CHERRY_PICK_HEAD",
        "rebase-merge",
        "rebase-apply",
        "sequencer",
    ] {
        let path = run(root, &["rev-parse", "--git-path", marker])?;
        if root.join(path.trim()).exists() {
            return Err(
                "Finish or abort the current operation in Conflicts before continuing.".into(),
            );
        }
    }
    run(root, &["symbolic-ref", "--quiet", "HEAD"])
        .map_err(|_| "Switch to a local branch before continuing.".to_string())?;
    Ok(())
}
pub fn revert(root: &Path, reference: &str, mainline: Option<usize>) -> Result<String> {
    require_idle(root)?;
    let id = git::resolve(root, reference)?;
    run(root, &["merge-base", "--is-ancestor", &id, "HEAD"]).map_err(|_| {
        "This commit is not in the current branch. Switch to its branch before reverting."
            .to_string()
    })?;
    let parents = run(root, &["show", "-s", "--format=%P", &id])?;
    let count = parents.split_whitespace().count();
    if (count > 1 && mainline.is_none())
        || mainline.is_some_and(|n| count < 2 || n == 0 || n > count)
    {
        return Err("Choose which parent of this merge to keep before reverting.".into());
    }
    let mut args = vec!["revert".to_string(), "--no-edit".into()];
    if let Some(n) = mainline {
        args.extend(["--mainline".into(), n.to_string()]);
    }
    args.push(id);
    process::run("git", root, &args).map_err(|e| format!("{e}\nIf there are conflicts, resolve and Continue or Abort in Conflicts. No history was rewritten."))
}
pub fn update_branch(root: &Path, branch: &str, baseline: &str) -> Result<String> {
    require_idle(root)?;
    git::validate_branch(root, branch)?;
    let original_head = git::resolve(root, "HEAD")?;
    let original_branch = run(root, &["symbolic-ref", "HEAD"])?;
    let target = format!("refs/heads/{branch}");
    run(root, &["show-ref", "--verify", &target])?;
    if baseline.starts_with('-') || baseline.trim().is_empty() {
        return Err("Choose a valid sync baseline.".into());
    }
    // Fetch before switching, so a network failure leaves the current checkout alone.
    let remote_ref = format!("refs/remotes/{baseline}");
    let source = if run(root, &["show-ref", "--verify", &remote_ref]).is_ok() {
        let (remote, name) = baseline.split_once('/').ok_or("Choose a remote branch.")?;
        let refspec = format!("+refs/heads/{name}:{remote_ref}");
        run(
            root,
            &["fetch", "--progress", "--no-tags", remote, &refspec],
        )?;
        remote_ref
    } else {
        baseline.to_string()
    };
    let base = git::resolve(root, &source)?;
    let tip = git::resolve(root, &target)?;
    require_idle(root)?;
    if git::resolve(root, "HEAD")? != original_head
        || run(root, &["symbolic-ref", "HEAD"])? != original_branch
    {
        return Err("The checkout changed while fetching. Review the branch and try again.".into());
    }
    run(root, &["switch", branch])?;
    if tip == base {
        return Ok(format!("{branch} is already up to date with the baseline."));
    }
    run(root, &["merge", "--no-edit", &base]).map_err(|e| format!("{e}\nYou are now on {branch}. Resolve the merge in Conflicts, or Abort to undo this merge. Nothing was pushed."))
}
pub fn open_pull_request(root: &Path, number: u64, destination: &Path) -> Result<String> {
    crate::safety::ensure_writable(root)?;
    crate::safety::ensure_writable(destination)?;
    if destination.exists() {
        return Err("Choose a new directory for the review worktree.".into());
    }
    let url = process::stdout(
        "gh",
        root,
        &process::strings(&["repo", "view", "--json", "url", "--jq", ".url"]),
    )?;
    let reference = format!("refs/branchroom/reviews/{}", snapshot::unique_id());
    run(
        root,
        &[
            "fetch",
            "--progress",
            "--",
            url.trim(),
            &format!("refs/pull/{number}/head:{reference}"),
        ],
    )?;
    let result = snapshot::materialize(root, &reference, destination);
    let _ = run(root, &["update-ref", "-d", &reference]);
    result
}

/// Remote-tracking refs describe the last fetch, not a live connectivity check.
pub fn branch_status(branch: &git::Branch, branches: &[git::Branch]) -> String {
    if branch.remote {
        return if branches
            .iter()
            .any(|b| !b.remote && b.upstream == branch.name)
        {
            "Remote · tracked on this PC".into()
        } else {
            "Remote-tracking · no local branch tracks it".into()
        };
    }
    if branch.upstream.is_empty() {
        return if branches.iter().any(|b| b.remote && b.id == branch.id) {
            "Local · matches a remote tip · no upstream".into()
        } else {
            "Local · no upstream".into()
        };
    }
    if !branches
        .iter()
        .any(|b| b.remote && b.name == branch.upstream)
        && !branch.tracking.contains("gone")
    {
        return format!("Local · tracks {} on this PC", branch.upstream);
    }
    if branch.tracking.contains("gone") {
        return "Local · upstream missing; fetch to verify".into();
    }
    if branch.tracking.is_empty() {
        format!("Tracks {} · in sync at last fetch", branch.upstream)
    } else {
        format!(
            "Tracks {} · {} at last fetch",
            branch.upstream, branch.tracking
        )
    }
}

pub fn commit_pull_requests(root: &Path, reference: &str) -> Result<Vec<PullRequest>> {
    let id = git::resolve(root, reference)?;
    let output = process::stdout(
        "gh",
        root,
        &process::strings(&[
            "pr",
            "list",
            "--state",
            "all",
            "--search",
            &id,
            "--limit",
            "100",
            "--json",
            "number,title,headRefName,baseRefName,url,isDraft,state",
        ]),
    )?;
    serde_json::from_str(&output)
        .map_err(|e| format!("Cannot read related pull requests: {e}. Try Reload pull requests."))
}

pub fn switch_branch(root: &Path, branch: &str, stash: bool) -> Result<String> {
    crate::safety::ensure_writable(root)?;
    git::validate_branch(root, branch)?;
    let remote = run(
        root,
        &[
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/remotes/{branch}"),
        ],
    )
    .is_ok();
    let refs = if remote {
        run(
            root,
            &[
                "for-each-ref",
                "--format=%(refname:short)%09%(upstream:short)",
                "refs/heads",
            ],
        )?
    } else {
        String::new()
    };
    let tracking: Vec<_> = refs
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .filter(|(_, upstream)| *upstream == branch)
        .map(|(name, _)| name)
        .collect();
    if tracking.len() > 1 {
        return Err(
            "Several local branches track this remote. Choose the local branch in On this PC."
                .into(),
        );
    }
    if stash {
        run(
            root,
            &[
                "stash",
                "push",
                "--include-untracked",
                "-m",
                &format!("Branchroom: before switching to {branch}"),
            ],
        )?;
    }
    let args = if let Some(local) = tracking.first() {
        vec!["switch", *local]
    } else if remote {
        vec!["switch", "--track", branch]
    } else {
        vec!["switch", branch]
    };
    run(root, &args).map_err(|e| format!("Branch switch failed. Any saved stash is retained.\n{e}"))
}

pub fn merge(root: &Path, target: &str, reference: &str) -> Result<String> {
    require_idle(root)?;
    let current = git::current_branch(root)?;
    if current.trim() != target {
        return Err("The checked-out branch changed. Review the destination and try again.".into());
    }
    let id = git::resolve(root, reference)?;
    run(root, &["merge", "--no-edit", &id]).map_err(|e| {
        format!("{e}\nResolve and Continue, or Abort, in Conflicts. Nothing was pushed.")
    })
}

/// Preserve the deleted tip in the same recovery store used by history rewrites.
pub fn delete_merged_branch(root: &Path, name: &str, expected: &str) -> Result<String> {
    crate::safety::ensure_writable(root)?;
    require_idle(root)?;
    run(root, &["check-ref-format", &format!("refs/heads/{name}")])?;
    let reference = format!("refs/heads/{name}");
    let actual = git::resolve(root, &reference)?;
    if actual != expected {
        return Err("The branch changed since review. Refresh and try again.".into());
    }
    let current = run(root, &["symbolic-ref", "HEAD"])?;
    if current.trim() == reference {
        return Err("Switch to another branch before deleting this one.".into());
    }
    run(root, &["merge-base", "--is-ancestor", &actual, "HEAD"]).map_err(|_| {
        "This branch has commits missing from the current branch. Merge or keep it.".to_owned()
    })?;
    let checkpoint = format!("refs/branchroom/recovery/{}", snapshot::unique_id());
    run(root, &["update-ref", &checkpoint, &actual, ""])?;
    run(root, &["branch", "-d", "--", name])?;
    Ok(format!(
        "Deleted {name}. Its tip is saved in Time Machine: {checkpoint}"
    ))
}
