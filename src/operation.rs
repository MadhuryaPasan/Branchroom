//! Reviewed graph actions. Forecasts run in disposable clones, never the user's checkout.
use crate::{
    commit,
    git::{self, git},
    history,
    process::{self, Result},
    snapshot,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Undo,
    RenameBranch {
        name: String,
    },
    BranchFrom {
        name: String,
        commit: String,
    },
    Revert {
        commit: String,
        mainline: Option<usize>,
    },
    MoveCommits {
        name: String,
        count: usize,
    },
    Merge {
        source: String,
        target: String,
    },
    CherryPick {
        commit: String,
        target: String,
    },
    Reorder {
        order: Vec<String>,
    },
    Squash {
        source: String,
        target: String,
        message: String,
    },
    Reset {
        commit: String,
    },
    SquashRange {
        count: usize,
        message: String,
    },
    MoveToBranch {
        name: String,
    },
    Commit {
        message: String,
    },
    Amend {
        commit: String,
        message: String,
    },
}
#[derive(Clone, Debug, PartialEq)]
struct Fingerprint {
    head: Option<String>,
    branch: String,
    index: String,
    status: String,
    unstaged: String,
    policy: String,
    identity: (String, String),
}
#[derive(Clone, Debug)]
pub struct Preview {
    pub kind: Kind,
    pub summary: String,
    pub conflicts: Vec<String>,
    pub before: Vec<git::Commit>,
    pub after: Vec<git::Commit>,
    pub command: String,
    pub introduced: usize,
    pub default_branch: Option<String>,
    pub blocked: Option<String>,
    root: PathBuf,
    fingerprint: Fingerprint,
    watched: Vec<(String, String)>,
    commit_plan: Option<commit::Plan>,
    move_base: Option<String>,
}
impl Preview {
    pub fn current_branch(&self) -> &str {
        &self.fingerprint.branch
    }
}
/// Render learning commands for the platform's shell; execution always uses argument arrays.
pub fn display_command(args: &[String]) -> String {
    let quote = |value: &str| {
        if cfg!(windows) {
            format!("'{}'", value.replace(char::from(39), "''"))
        } else {
            format!("'{}'", value.replace(char::from(39), "'\''"))
        }
    };
    format!(
        "git {}",
        args.iter().map(|s| quote(s)).collect::<Vec<_>>().join(" ")
    )
}
fn command(args: &[&str]) -> String {
    display_command(&process::strings(args))
}
fn rebase_command(root: &Path, first: &str, autosquash: bool) -> String {
    let parent = git::resolve(root, &format!("{first}^")).ok();
    let mut args = vec!["rebase", "--interactive"];
    if autosquash {
        args.push("--autosquash");
    }
    args.push(parent.as_deref().unwrap_or("--root"));
    command(&args)
}
fn learning_command(root: &Path, review: &Preview) -> Result<String> {
    Ok(match &review.kind {
        Kind::Merge { source, .. } => {
            let target = destination(review);
            format!(
                "{}\n{}",
                command(&["switch", target]),
                command(&[
                    "merge",
                    "--no-edit",
                    "-m",
                    &format!("Merge {} into {}", reference_label(source), target),
                    &review.watched[1].1
                ])
            )
        }
        Kind::CherryPick { .. } => {
            let target = destination(review);
            format!(
                "{}\n{}",
                command(&["switch", target]),
                command(&["cherry-pick", &review.watched[1].1])
            )
        }
        Kind::Revert { commit, mainline } => {
            let mut args = vec!["revert".to_owned(), "--no-edit".into()];
            if let Some(n) = mainline {
                args.extend(["-m".into(), n.to_string()]);
            }
            args.push(commit.clone());
            display_command(&args)
        }
        Kind::RenameBranch { name } => {
            command(&["branch", "-m", "--", &review.fingerprint.branch, name])
        }
        Kind::BranchFrom { name, commit } => command(&["branch", "--", name, commit]),
        Kind::Commit { message } => command(&["commit", "-m", message]),
        Kind::Amend {
            commit: target,
            message,
        } => {
            if Some(target) == review.fingerprint.head.as_ref() {
                command(&["commit", "--amend", "-m", message])
            } else {
                format!(
                    "{}\n{}\n# In the editor, reword the combined commit to the reviewed message",
                    command(&["commit", "--fixup", target]),
                    rebase_command(root, target, true)
                )
            }
        }
        Kind::Undo => {
            if git::resolve(root, "HEAD^").is_ok() {
                command(&["reset", "--mixed", "HEAD^"])
            } else {
                format!(
                    "{}\n{}",
                    command(&[
                        "update-ref",
                        "-d",
                        &format!("refs/heads/{}", review.fingerprint.branch),
                        review.fingerprint.head.as_deref().unwrap_or("")
                    ]),
                    command(&["read-tree", "--empty"])
                )
            }
        }
        Kind::Reset { commit } => command(&["reset", "--mixed", commit]),
        Kind::Reorder { order } => {
            let ids = history::linear_tip(root, order.len())?;
            format!(
                "{}\n# Put the commits in this order, oldest first:\n{}",
                rebase_command(root, &ids[0], false),
                order
                    .iter()
                    .map(|id| format!("# {id}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        }
        Kind::Squash { source, target, .. } => {
            let ids = rewrite_range(root, source, target)?;
            format!(
                "{}\n# Move the source after the target and combine it using squash",
                rebase_command(root, &ids[0], false)
            )
        }
        Kind::SquashRange { count, .. } => {
            let ids = history::linear_tip(root, *count)?;
            format!(
                "{}\n# Keep the first commit and squash the remaining commits into it",
                rebase_command(root, &ids[0], false)
            )
        }
        Kind::MoveToBranch { name } | Kind::MoveCommits { name, .. } => {
            let original = format!("refs/heads/{}", review.fingerprint.branch);
            let reset = if let Some(base) = &review.move_base {
                command(&[
                    "update-ref",
                    &original,
                    base,
                    review.fingerprint.head.as_deref().unwrap_or(""),
                ])
            } else {
                command(&[
                    "update-ref",
                    "-d",
                    &original,
                    review.fingerprint.head.as_deref().unwrap_or(""),
                ])
            };
            format!("{}\n{reset}", command(&["switch", "-c", name]))
        }
    })
}
fn destination(review: &Preview) -> &str {
    review.watched[0]
        .0
        .strip_prefix("refs/heads/")
        .unwrap_or(&review.watched[0].0)
}
fn reference_label(name: &str) -> &str {
    name.strip_prefix("refs/remotes/")
        .or_else(|| name.strip_prefix("refs/heads/"))
        .unwrap_or(name)
}
pub fn default_branch(root: &Path) -> Option<String> {
    if let Ok(value) = git(root, &["symbolic-ref", "refs/remotes/origin/HEAD"]) {
        return value
            .trim()
            .strip_prefix("refs/remotes/origin/")
            .map(str::to_owned);
    }
    ["main", "master"]
        .into_iter()
        .find(|name| git::resolve(root, &format!("refs/heads/{name}")).is_ok())
        .map(str::to_owned)
        .or_else(|| {
            git(root, &["symbolic-ref", "--short", "HEAD"])
                .ok()
                .map(|s| s.trim().to_owned())
                .filter(|s| ["main", "master"].contains(&s.as_str()))
        })
}
fn fingerprint(root: &Path) -> Result<Fingerprint> {
    Ok(Fingerprint {
        head: git::resolve(root, "HEAD").ok(),
        branch: git::current_branch(root)?,
        index: git(root, &["write-tree"])?.trim().into(),
        status: git(
            root,
            &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
        )?,
        identity:committer_identity(root),
        policy: git(
            root,
            &[
                "config",
                "--get-regexp",
                r"^(merge\..*|branch\..*\.mergeOptions|core\.(autocrlf|eol|attributesfile)|commit\.gpgsign|gpg\..*|user\.signingkey)$",
            ],
        )
        .unwrap_or_default(),
        unstaged: git(
            root,
            &["diff", "--binary", "--no-ext-diff", "--no-textconv", "--"],
        )?,
    })
}
fn require_no_operation(root: &Path) -> Result<()> {
    for marker in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-merge",
        "rebase-apply",
        "sequencer",
    ] {
        let file = git(root, &["rev-parse", "--git-path", marker])?;
        if root.join(file.trim()).exists() {
            return Err(
                "Finish or abort the current Git operation before preparing another action".into(),
            );
        }
    }
    Ok(())
}
fn unpublished(root: &Path, ids: &[String]) -> Result<()> {
    history::ensure_unpublished(root, ids)
}
fn rewrite_range(root: &Path, a: &str, b: &str) -> Result<Vec<String>> {
    let tail = git(
        root,
        &["rev-list", "--first-parent", "--max-count=200", "HEAD"],
    )?;
    let ids: Vec<_> = tail.lines().map(str::to_owned).collect();
    let a = ids
        .iter()
        .position(|id| id == a)
        .ok_or("Choose a commit in the last 200 commits of the current branch")?;
    let b = ids
        .iter()
        .position(|id| id == b)
        .ok_or("Choose a commit in the last 200 commits of the current branch")?;
    let range = history::linear_tip(root, a.max(b) + 1)?;
    unpublished(root, &range)?;
    Ok(range)
}
fn committer_identity(root: &Path) -> (String, String) {
    if let Ok(value) = git(root, &["var", "GIT_COMMITTER_IDENT"])
        && let (Some(start), Some(end)) = (value.rfind(" <"), value.rfind('>'))
        && start + 2 < end
    {
        return (value[..start].trim().into(), value[start + 2..end].into());
    }
    (
        "Branchroom preview".into(),
        "preview@branchroom.invalid".into(),
    )
}
struct Sandbox {
    path: PathBuf,
    identity: (String, String),
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        // Only this generated, owned directory is ever removed.
        if self.path.parent() == Some(std::env::temp_dir().as_path())
            && self
                .path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("branchroom-preview-"))
        {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
impl Sandbox {
    fn run(&self, args: &[&str]) -> Result<String> {
        let mut command = process::strings(&[
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "sequence.editor=git rev-parse --git-dir",
        ]);
        command.extend([
            "-c".into(),
            format!("user.name={}", self.identity.0),
            "-c".into(),
            format!("user.email={}", self.identity.1),
        ]);
        command.extend([
            "-c".into(),
            format!(
                "core.hooksPath={}",
                self.path.join("disabled-hooks").display()
            ),
        ]);
        command.extend(args.iter().map(|s| s.to_string()));
        process::input(
            "git",
            &self.path,
            &command,
            &[],
            &[
                ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
                (
                    "GIT_CONFIG_GLOBAL".into(),
                    self.path.join("disabled-global").display().to_string(),
                ),
                ("GIT_TERMINAL_PROMPT".into(), "0".into()),
            ],
        )
    }
    fn retitle(&self, newer: usize, message: &str) -> Result<()> {
        let target = git::resolve(&self.path, &format!("HEAD~{newer}"))?;
        let existing = self.run(&["show", "-s", "--format=%B", &target])?;
        if existing.trim() == message.trim() {
            return Ok(());
        }
        let mut parent = git::resolve(&self.path, &format!("{target}^")).ok();
        let range = parent
            .as_ref()
            .map(|p| format!("{p}..HEAD"))
            .unwrap_or_else(|| "HEAD".into());
        let ids = self.run(&["rev-list", "--reverse", "--first-parent", &range])?;
        let old = git::resolve(&self.path, "HEAD")?;
        for (i, id) in ids.lines().enumerate() {
            let metadata =
                self.run(&["show", "-s", "--format=%T%x00%an%x00%ae%x00%aI%x00%B", id])?;
            let fields: Vec<_> = metadata.splitn(5, '\0').collect();
            if fields.len() != 5 {
                return Err("Cannot read replayed commit metadata".into());
            }
            let mut args = vec!["commit-tree", fields[0], "-F", "-"];
            if let Some(parent) = &parent {
                args.extend(["-p", parent]);
            }
            let output = process::input(
                "git",
                &self.path,
                &process::strings(&args),
                if i == 0 {
                    message.as_bytes()
                } else {
                    fields[4].as_bytes()
                },
                &[
                    ("GIT_AUTHOR_NAME".into(), fields[1].into()),
                    ("GIT_AUTHOR_EMAIL".into(), fields[2].into()),
                    ("GIT_AUTHOR_DATE".into(), fields[3].into()),
                ],
            )?;
            parent = Some(output.trim().into());
        }
        self.run(&[
            "update-ref",
            "refs/heads/branchroom-preview",
            parent.as_deref().ok_or("No replayed commit")?,
            &old,
        ])?;
        Ok(())
    }
    fn history(&self) -> Result<Vec<git::Commit>> {
        let _isolation = crate::settings::StateDirectory::enter_isolated(
            self.path.join(".git/branchroom-state"),
        );
        Ok(git::load(&self.path, "HEAD", 40)?.branch_commits)
    }
    fn new(root: &Path, head: Option<&str>) -> Result<Self> {
        let sandbox = Self {
            path: std::env::temp_dir()
                .join(format!("branchroom-preview-{}", snapshot::unique_id())),
            identity: committer_identity(root),
        };
        fs::create_dir(&sandbox.path).map_err(|e| e.to_string())?;
        let source = std::path::absolute(root).map_err(|e| e.to_string())?;
        sandbox.run(&[
            "clone",
            "--quiet",
            "--shared",
            "--no-checkout",
            "--",
            &source.to_string_lossy(),
            ".",
        ])?;
        sandbox.run(&["remote", "remove", "origin"])?;
        for (name, value) in [
            ("user.name", sandbox.identity.0.as_str()),
            ("user.email", sandbox.identity.1.as_str()),
            ("commit.gpgsign", "false"),
            ("core.fsmonitor", "false"),
        ] {
            sandbox.run(&["config", name, value])?;
        }
        sandbox.run(&[
            "config",
            "core.hooksPath",
            &sandbox.path.join("disabled-hooks").to_string_lossy(),
        ])?;

        for key in [
            "merge.ff",
            "merge.renormalize",
            "merge.renames",
            "merge.renameLimit",
            "merge.directoryRenames",
            "merge.conflictstyle",
            "core.autocrlf",
            "core.eol",
        ] {
            if let Ok(value) = git(root, &["config", "--get", key]) {
                sandbox.run(&["config", key, value.trim()])?;
            }
        }
        if let Some(head) = head {
            sandbox.run(&["checkout", "--quiet", "-B", "branchroom-preview", head])?;
        } else {
            sandbox.run(&["symbolic-ref", "HEAD", "refs/heads/branchroom-preview"])?;
            sandbox.run(&["read-tree", "--empty"])?;
        }
        Ok(sandbox)
    }
}
fn sandbox_action(s: &Sandbox, p: &Preview) -> Result<()> {
    let _isolation =
        crate::settings::StateDirectory::enter_isolated(s.path.join(".git/branchroom-state"));
    match &p.kind {
        Kind::Undo => {
            history::run(&s.path, &history::Action::Undo)?;
        }
        Kind::Merge { source, target } => {
            s.run(&[
                "merge",
                "--no-edit",
                "-m",
                &format!(
                    "Merge {} into {}",
                    reference_label(source),
                    reference_label(target)
                ),
                &p.watched[1].1,
            ])?;
        }
        Kind::CherryPick { .. } => {
            s.run(&["cherry-pick", &p.watched[1].1])?;
        }
        Kind::Revert { commit, mainline } => {
            crate::collaboration::revert(&s.path, commit, *mainline)?;
        }
        Kind::RenameBranch { name } => {
            s.run(&["branch", "-m", name])?;
        }
        Kind::BranchFrom { name, commit } => {
            s.run(&["branch", "--", name, commit])?;
        }
        Kind::Reset { commit } => {
            s.run(&["reset", "--mixed", commit])?;
        }
        Kind::Commit { message } => {
            s.run(&["read-tree", "--reset", "-u", &p.fingerprint.index])?;
            s.run(&["commit", "-m", message])?;
        }
        Kind::Amend {
            commit: target,
            message,
        } => {
            s.run(&["read-tree", "--reset", "-u", &p.fingerprint.index])?;
            if Some(target) == p.fingerprint.head.as_ref() {
                s.run(&["commit", "--amend", "-m", message])?;
            } else {
                s.run(&["commit", "--fixup", target])?;
                if let Ok(parent) = git::resolve(&s.path, &format!("{target}^")) {
                    s.run(&[
                        "rebase",
                        "--interactive",
                        "--autosquash",
                        "--empty=keep",
                        "--reapply-cherry-picks",
                        &parent,
                    ])?;
                } else {
                    s.run(&[
                        "rebase",
                        "--interactive",
                        "--autosquash",
                        "--empty=keep",
                        "--reapply-cherry-picks",
                        "--root",
                    ])?;
                }
                let newer = git(
                    &p.root,
                    &[
                        "rev-list",
                        "--count",
                        &format!(
                            "{target}..{}",
                            p.fingerprint.head.as_deref().unwrap_or("HEAD")
                        ),
                    ],
                )?;
                s.retitle(
                    newer.trim().parse::<usize>().map_err(|e| e.to_string())?,
                    message,
                )?;
            }
        }
        Kind::MoveToBranch { name } | Kind::MoveCommits { name, .. } => {
            s.run(&["checkout", "-b", name])?;
        }
        Kind::SquashRange { count, message } => {
            history::run(
                &s.path,
                &history::Action::Squash {
                    count: *count,
                    message: message.clone(),
                },
            )?;
        }
        Kind::Reorder { order } => {
            history::run(&s.path, &history::Action::Reorder(order.clone()))?;
        }
        Kind::Squash {
            source,
            target,
            message,
        } => {
            let range = rewrite_range(&p.root, source, target)?;
            let parent = git::resolve(&p.root, &format!("{}^", range[0]))
                .map_err(|_| "Keep the first repository commit outside this squash".to_owned())?;
            s.run(&["reset", "--hard", &parent])?;
            for id in range.iter().filter(|id| *id != source) {
                s.run(&["cherry-pick", id])?;
                if id == target {
                    s.run(&["cherry-pick", "--no-commit", source])?;
                    s.run(&["commit", "--amend", "-m", message])?;
                }
            }
        }
    }
    Ok(())
}
/// Prepare an immutable review bound to the exact checkout, index, and branch tips.
pub fn preview(root: &Path, mut kind: Kind) -> Result<Preview> {
    crate::safety::ensure_writable(root)?;
    require_no_operation(root)?;
    match &mut kind {
        Kind::Reset { commit }
        | Kind::Amend { commit, .. }
        | Kind::Revert { commit, .. }
        | Kind::BranchFrom { commit, .. } => *commit = git::resolve(root, commit)?,
        Kind::Squash { source, target, .. } => {
            *source = git::resolve(root, source)?;
            *target = git::resolve(root, target)?;
        }
        Kind::Reorder { order } => {
            for id in order {
                *id = git::resolve(root, id)?;
            }
        }
        _ => {}
    }
    match &mut kind {
        Kind::Amend {
            commit: target,
            message,
        }
        | Kind::Squash {
            target, message, ..
        } if message.trim() == git(root, &["show", "-s", "--format=%s", target])?.trim() => {
            *message = git(root, &["show", "-s", "--format=%B", target])?
                .trim_end()
                .to_owned();
        }
        _ => {}
    }
    if matches!(
        kind,
        Kind::Merge { .. }
            | Kind::CherryPick { .. }
            | Kind::Revert { .. }
            | Kind::Reorder { .. }
            | Kind::Squash { .. }
            | Kind::Amend { .. }
    ) && git(root, &["config", "--get-regexp", r"^merge\..*\.driver$"]).is_ok()
    {
        return Err("This repository uses a custom merge driver. Use the Advanced command form with your configured merge tools; an isolated forecast cannot run that driver safely".into());
    }
    let frozen = fingerprint(root)?;
    let imported_rewrite = matches!(kind, Kind::Squash { .. })
        || matches!(&kind,Kind::Amend{commit,..} if Some(commit)!=frozen.head.as_ref());
    if imported_rewrite
        && git(root, &["config", "--bool", "commit.gpgsign"])
            .unwrap_or_default()
            .trim()
            == "true"
    {
        return Err("Commit signing is enabled. Use the exact Advanced Git command for this rewrite so your signing tools can run".into());
    }

    let mut watched = vec![];
    let mut commit_plan = None;
    let mut move_base = None;
    let (summary, command) = match &kind {
        Kind::RenameBranch { name } | Kind::BranchFrom { name, .. } => {
            git::validate_branch(root, name)?;
            if git::resolve(root, &format!("refs/heads/{name}")).is_ok() {
                return Err("This branch already exists. Choose a different name".into());
            }
            (
                match &kind {
                    Kind::BranchFrom { commit, .. } => format!(
                        "Create branch {name} at {}. The current branch and working files are kept",
                        &commit[..7]
                    ),
                    _ => format!(
                        "Rename {} to {name}. Commits and working files are kept",
                        frozen.branch
                    ),
                },
                String::new(),
            )
        }

        Kind::Revert { commit, mainline } => {
            crate::collaboration::require_idle(root)?;
            if !frozen.status.is_empty() {
                return Err("Commit or shelve working changes before reverting".into());
            }
            git(root, &["merge-base", "--is-ancestor", commit, "HEAD"]).map_err(|_| {
                "Choose a commit from the current branch before reverting".to_owned()
            })?;
            let parents = git(root, &["show", "-s", "--format=%P", commit])?
                .split_whitespace()
                .count();
            if (parents > 1 && mainline.is_none())
                || mainline.is_some_and(|n| parents < 2 || n == 0 || n > parents)
            {
                return Err("Choose which side of this merge to keep before reverting".into());
            }
            (
                format!(
                    "Add a commit reversing {}. Existing history is kept and nothing will be pushed",
                    &commit[..7]
                ),
                String::new(),
            )
        }

        Kind::Undo => {
            crate::collaboration::require_idle(root)?;
            let head = frozen
                .head
                .as_ref()
                .ok_or("Create a first commit before undoing history")?;
            history::linear_tip(root, 1)?;
            unpublished(root, std::slice::from_ref(head))?;
            ("Return the latest local commit to working changes. Its old tip is kept in Time Machine".into(),if git::resolve(root,"HEAD^").is_ok(){"git reset --mixed HEAD^".into()}else{format!("git update-ref -d refs/heads/{} {head}\ngit read-tree --empty",frozen.branch)})
        }
        Kind::Merge { source, target }
        | Kind::CherryPick {
            commit: source,
            target,
        } => {
            if !frozen.status.is_empty() {
                return Err(
                    "Commit or shelve working changes before switching or combining branches"
                        .into(),
                );
            }
            let target_ref = if target.starts_with("refs/heads/") {
                target.clone()
            } else {
                format!("refs/heads/{target}")
            };
            let target = target_ref.strip_prefix("refs/heads/").unwrap();
            git::validate_branch(root, target)?;
            let target_id = git::resolve(root, &target_ref)?;
            let (source_ref, source_id) = git::resolve_named_commit(root, source)?;
            if source_id == target_id {
                return Err("Both targets already point to the same commit".into());
            }
            if target != frozen.branch.as_str() {
                let desks = git(root, &["worktree", "list", "--porcelain"])?;
                if desks
                    .lines()
                    .any(|line| line == format!("branch {target_ref}"))
                {
                    return Err("This branch is open in another parallel workspace. Open that workspace to update it".into());
                }
            }
            watched.push((target_ref.clone(), target_id));
            watched.push((source_ref, source_id));
            if matches!(kind, Kind::CherryPick { .. })
                && git(root, &["rev-list", "--parents", "-n", "1", source])?
                    .split_whitespace()
                    .count()
                    > 2
            {
                return Err(
                    "A merge commit needs a parent choice. Use Advanced to choose its main parent"
                        .into(),
                );
            }
            let verb = if matches!(kind, Kind::Merge { .. }) {
                "Merge"
            } else {
                "Copy commit"
            };
            (
                format!(
                    "{verb} {} into {target}. Nothing will be pushed.",
                    reference_label(source)
                ),
                format!(
                    "git switch {target:?}\ngit {} {source:?}",
                    if matches!(kind, Kind::Merge { .. }) {
                        "merge --no-edit"
                    } else {
                        "cherry-pick"
                    }
                ),
            )
        }
        Kind::MoveToBranch { name } | Kind::MoveCommits { name, .. } => {
            git::validate_branch(root, name)?;
            if git::resolve(root, &format!("refs/heads/{name}")).is_ok() {
                return Err("This branch already exists. Choose a new branch name".into());
            }
            let head = frozen
                .head
                .as_ref()
                .ok_or("Create a first commit before moving commits")?;
            let ids = if let Kind::MoveCommits { count, .. } = &kind {
                let ids = history::linear_tip(root, *count)?;
                move_base = git::resolve(root, &format!("HEAD~{count}")).ok();
                if let Some(base) = &move_base {
                    watched.push((format!("HEAD~{count}"), base.clone()));
                }
                ids
            } else {
                let upstream=git(root,&["rev-parse","--symbolic-full-name","@{upstream}"]).map_err(|_|"Link this branch to its remote, or choose a specific number of commits to move".to_owned())?;
                let upstream = upstream.trim().to_owned();
                let base = git::resolve(root, &upstream)?;
                git(root, &["merge-base", "--is-ancestor", &base, head]).map_err(|_| {
                    "The remote branch has diverged. Compare and update it first".to_owned()
                })?;
                let ids = git(root, &["rev-list", &format!("{base}..{head}")])?
                    .lines()
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                move_base = Some(base.clone());
                watched.push((upstream, base));
                ids
            };
            if ids.is_empty() {
                return Err("There are no unpublished commits to move".into());
            }
            unpublished(root, &ids)?;
            (
                format!(
                    "Move {} unpublished commits to {name}. {} Included and working changes follow the new branch",
                    ids.len(),
                    if move_base.is_some() {
                        format!("{} returns to the reviewed starting commit.", frozen.branch)
                    } else {
                        format!(
                            "{} has no earlier commits, so its branch name is removed.",
                            frozen.branch
                        )
                    }
                ),
                format!(
                    "git switch -c {name:?}\ngit update-ref refs/heads/{} <reviewed remote tip> <reviewed old tip>",
                    frozen.branch
                ),
            )
        }
        Kind::SquashRange { count, message } => {
            crate::collaboration::require_idle(root)?;
            let ids = history::linear_tip(root, *count)?;
            unpublished(root, &ids)?;
            if message.trim().is_empty() {
                return Err("Enter a combined commit message".into());
            }
            (
                format!("Combine the last {count} local commits without changing files"),
                "git rebase --interactive (reviewed squash range)".into(),
            )
        }
        Kind::Reset { commit: target } => {
            crate::collaboration::require_idle(root)?;
            let head = frozen
                .head
                .as_ref()
                .ok_or("Create a commit before moving the branch tip")?;
            let target_id = git::resolve(root, target)?;
            git(root, &["merge-base", "--is-ancestor", &target_id, head])
                .map_err(|_| "Move the tip to an earlier commit on this branch".to_owned())?;
            if target_id == *head {
                return Err("The branch already points here".into());
            }
            let removed: Vec<_> = git(root, &["rev-list", &format!("{target_id}..{head}")])?
                .lines()
                .map(str::to_owned)
                .collect();
            if removed.len() > 200 {
                return Err("Move at most 200 commits at once".into());
            }
            unpublished(root, &removed)?;
            (
                format!(
                    "Move {} back to {}. Removed commits stay as working changes and in Time Machine",
                    frozen.branch,
                    &target_id[..target_id.len().min(8)]
                ),
                format!("git reset --mixed {target:?}"),
            )
        }
        Kind::Reorder { order } => {
            crate::collaboration::require_idle(root)?;
            let expected = history::linear_tip(root, order.len())?;
            let mut actual = order.clone();
            let mut sorted = expected.clone();
            actual.sort();
            sorted.sort();
            if actual != sorted || order == &expected {
                return Err(
                    "Choose a different order containing each commit in the tip range once".into(),
                );
            }
            unpublished(root, &expected)?;
            (
                format!(
                    "Replay {} local commits in the displayed order",
                    order.len()
                ),
                "git rebase --interactive (reviewed commit order)".into(),
            )
        }
        Kind::Squash {
            source,
            target,
            message,
        } => {
            crate::collaboration::require_idle(root)?;
            if source == target || message.trim().is_empty() {
                return Err("Choose two different commits and a combined message".into());
            }
            rewrite_range(root, source, target)?;
            (
                format!(
                    "Combine {} into {} and replay later local commits",
                    &source[..source.len().min(8)],
                    &target[..target.len().min(8)]
                ),
                "git rebase --interactive (combine the selected commits)".into(),
            )
        }
        Kind::Commit { message } => {
            let plan = commit::prepare(root, message, false)?;
            let summary = format!(
                "Commit {} included files on {}",
                plan.files.len(),
                frozen.branch
            );
            commit_plan = Some(plan);
            (summary, "git commit -F <reviewed message>".into())
        }
        Kind::Amend {
            commit: target,
            message,
        } => {
            if frozen.head.is_none() {
                return Err("Create a first commit before amending history".into());
            }
            let range = rewrite_range(root, target, frozen.head.as_ref().unwrap())?;
            unpublished(root, &range)?;
            let plan = commit::prepare(root, message, true)?;
            if Some(target) != frozen.head.as_ref()
                && frozen.index == git(root, &["rev-parse", "HEAD^{tree}"])?.trim()
            {
                return Err("Include changed files before adding them to an earlier commit".into());
            }
            commit_plan = Some(plan);
            (
                format!(
                    "Add the included changes to {} and replay later commits",
                    &target[..target.len().min(8)]
                ),
                if Some(target) == frozen.head.as_ref() {
                    "git commit --amend".into()
                } else {
                    format!("git commit --fixup={target}\ngit rebase --interactive --autosquash")
                },
            )
        }
    };
    let start = if matches!(
        kind,
        Kind::Merge { .. } | Kind::CherryPick { .. } | Kind::Revert { .. }
    ) {
        watched
            .first()
            .map(|(_, id)| id.clone())
            .or(frozen.head.clone())
    } else {
        frozen.head.clone()
    };
    let sandbox = Sandbox::new(root, start.as_deref())?;
    if matches!(kind, Kind::Merge { .. })
        && let Ok(options) = git(
            root,
            &[
                "config",
                "--get",
                &format!(
                    "branch.{}.mergeOptions",
                    watched[0].0.strip_prefix("refs/heads/").unwrap()
                ),
            ],
        )
    {
        if options
            .split_whitespace()
            .any(|o| !["--ff", "--no-ff", "--ff-only", "--no-edit", "--no-log"].contains(&o))
        {
            return Err("The destination has custom merge options. Use the Advanced command form with those options".into());
        }
        sandbox.run(&[
            "config",
            "branch.branchroom-preview.mergeOptions",
            options.trim(),
        ])?;
    }

    let root_canonical = root.canonicalize().map_err(|e| e.to_string())?;
    let mut review = Preview {
        kind,
        summary,
        command,
        conflicts: vec![],
        before: vec![],
        after: vec![],
        introduced: 0,
        default_branch: default_branch(root),
        blocked: None,
        root: root_canonical,
        fingerprint: frozen,
        watched,
        commit_plan,
        move_base,
    };
    review.command = learning_command(root, &review)?;
    if start.is_some() {
        review.before = sandbox.history()?;
    }
    let result = sandbox_action(&sandbox, &review);
    if let Err(error) = result {
        review.conflicts = sandbox
            .run(&["diff", "--name-only", "--diff-filter=U", "-z"])?
            .split('\0')
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        if review.conflicts.is_empty() {
            return Err(error);
        }
        review.introduced = match review.kind {
            Kind::Merge { .. } => {
                git(
                    root,
                    &[
                        "rev-list",
                        "--count",
                        &format!("{}..{}", review.watched[0].1, review.watched[1].1),
                    ],
                )?
                .trim()
                .parse::<usize>()
                .unwrap_or(0)
                    + 1
            }
            Kind::CherryPick { .. } | Kind::Revert { .. } => 1,
            _ => 0,
        };
        if matches!(
            review.kind,
            Kind::Merge { .. } | Kind::CherryPick { .. } | Kind::Revert { .. }
        ) {
            review
                .summary
                .push_str(" This will pause for you to resolve the listed files");
        }
        if !matches!(
            review.kind,
            Kind::Merge { .. } | Kind::CherryPick { .. } | Kind::Revert { .. }
        ) {
            review.blocked = Some(
                "This rewrite conflicts. Resolve its dependencies before changing the order".into(),
            );
        }
    } else {
        if matches!(review.kind, Kind::Undo) && git::resolve(&sandbox.path, "HEAD").is_err() {
            return Ok(review);
        }
        let tip = git::resolve(&sandbox.path, "HEAD")?;
        let tree = sandbox.run(&["rev-parse", "HEAD^{tree}"])?;
        let expected_tree = match review.kind {
            Kind::Reorder { .. } | Kind::Squash { .. } | Kind::SquashRange { .. } => {
                Some(git(root, &["rev-parse", "HEAD^{tree}"])?.trim().to_owned())
            }
            Kind::Amend { .. } | Kind::Commit { .. } => Some(review.fingerprint.index.clone()),
            _ => None,
        };
        if expected_tree.is_some_and(|expected| expected != tree.trim()) {
            review.blocked=Some("This edit would change the final files. Keep dependent commits in their original order".into());
        }
        review.after = sandbox.history()?;
        review.introduced = if let Some(start) = &start {
            sandbox
                .run(&["rev-list", "--count", &format!("{start}..{tip}")])?
                .trim()
                .parse()
                .unwrap_or(0)
        } else {
            1
        };
    }
    Ok(review)
}
/// Revalidate, snapshot, and apply exactly the reviewed action. No ref is taken from UI text at execution.
pub fn apply(root: &Path, review: &Preview) -> Result<String> {
    crate::safety::ensure_writable(root)?;
    require_no_operation(root)?;
    if root.canonicalize().map_err(|e| e.to_string())? != review.root
        || fingerprint(root)? != review.fingerprint
    {
        return Err("The repository changed after this preview. Preview the action again".into());
    }
    for (name, id) in &review.watched {
        if git::resolve(root, name)? != *id {
            return Err("A branch changed after this preview. Preview the action again".into());
        }
    }
    if let Some(reason) = &review.blocked {
        return Err(reason.clone());
    }
    if let Kind::Amend { commit, .. } = &review.kind {
        rewrite_range(
            root,
            commit,
            review.fingerprint.head.as_deref().unwrap_or(""),
        )?;
    }
    let safety = snapshot::create(root, &format!("Before {}", review.summary))?;
    if let Some(head) = &review.fingerprint.head {
        git(
            root,
            &[
                "update-ref",
                &format!("refs/branchroom/recovery/{}", snapshot::unique_id()),
                head,
                "",
            ],
        )?;
        let staged = git(
            root,
            &[
                "commit-tree",
                &review.fingerprint.index,
                "-p",
                head,
                "-m",
                "Safety point: included files before graph action",
            ],
        )?;
        git(
            root,
            &[
                "update-ref",
                &format!("refs/branchroom/recovery/{}", snapshot::unique_id()),
                staged.trim(),
                "",
            ],
        )?;
    }
    if fingerprint(root)? != review.fingerprint {
        return Err("The repository changed while saving its safety point. The action was not applied; preview it again".into());
    }
    for (name, id) in &review.watched {
        if git::resolve(root, name)? != *id {
            return Err("A branch changed while saving the safety point. Preview again".into());
        }
    }
    let output = (|| -> Result<String> {
        match &review.kind {
            Kind::RenameBranch { name } | Kind::BranchFrom { name, .. } => {
                if git::resolve(root, &format!("refs/heads/{name}")).is_ok() {
                    return Err(
                        "The branch name was taken after this preview. Choose another name".into(),
                    );
                }
                if let Kind::BranchFrom { commit, .. } = &review.kind {
                    git(root, &["branch", "--", name, commit])
                } else {
                    git(
                        root,
                        &["branch", "-m", "--", &review.fingerprint.branch, name],
                    )
                }
            }
            Kind::Revert { commit, mainline } => {
                crate::collaboration::revert(root, commit, *mainline)
            }
            Kind::Undo => history::run_reviewed(
                root,
                &history::Action::Undo,
                review.fingerprint.head.as_deref().unwrap_or(""),
            ),
            Kind::Merge { .. } | Kind::CherryPick { .. } => {
                let target = destination(review);
                crate::collaboration::require_idle(root)?;
                if target != review.fingerprint.branch.as_str() {
                    git(root, &["switch", "--no-guess", "--", target])?;
                }
                if git::resolve(root, "HEAD")? != review.watched[0].1 {
                    return Err("The destination changed while switching branches. Safety points were kept; preview again".into());
                }
                if fingerprint(root)?.policy != review.fingerprint.policy {
                    return Err("Git merge settings changed on the destination. Preview again from this branch".into());
                }
                if git::current_branch(root)? != target {
                    return Err("The destination branch changed. Preview again".into());
                }
                let id = &review.watched[1].1;
                if matches!(review.kind, Kind::Merge { .. }) {
                    let source = match &review.kind {
                        Kind::Merge { source, .. } => source.as_str(),
                        _ => unreachable!(),
                    };
                    git(
                        root,
                        &[
                            "merge",
                            "--no-edit",
                            "-m",
                            &format!("Merge {} into {target}", reference_label(source)),
                            id,
                        ],
                    )
                } else {
                    git(root, &["cherry-pick", id])
                }
            }
            Kind::MoveToBranch { name } | Kind::MoveCommits { name, .. } => {
                if git::resolve(root, &format!("refs/heads/{name}")).is_ok() {
                    return Err("The new branch name was taken. Choose another name".into());
                }
                git(root, &["switch", "-c", name])?;
                let original = format!("refs/heads/{}", review.fingerprint.branch);
                if let Some(base) = &review.move_base {
                    git(
                        root,
                        &[
                            "update-ref",
                            &original,
                            base,
                            review.fingerprint.head.as_deref().unwrap_or(""),
                        ],
                    )
                } else {
                    git(
                        root,
                        &[
                            "update-ref",
                            "-d",
                            &original,
                            review.fingerprint.head.as_deref().unwrap_or(""),
                        ],
                    )
                }
            }
            Kind::SquashRange { count, message } => history::run_reviewed(
                root,
                &history::Action::Squash {
                    count: *count,
                    message: message.clone(),
                },
                review.fingerprint.head.as_deref().unwrap_or(""),
            ),
            Kind::Reset { commit } => history::run_reviewed(
                root,
                &history::Action::Reset(commit.clone()),
                review.fingerprint.head.as_deref().unwrap_or(""),
            ),
            Kind::Commit { .. } => commit::execute(root, review.commit_plan.as_ref().unwrap()),
            Kind::Amend { commit: target, .. }
                if Some(target) == review.fingerprint.head.as_ref() =>
            {
                commit::execute(root, review.commit_plan.as_ref().unwrap())
            }
            Kind::Reorder { order } => history::run_reviewed(
                root,
                &history::Action::Reorder(order.clone()),
                review.fingerprint.head.as_deref().unwrap_or(""),
            ),
            Kind::Squash { .. } | Kind::Amend { .. } => {
                let sandbox = Sandbox::new(root, review.fingerprint.head.as_deref())?;
                sandbox_action(&sandbox, review)?;
                let new = git::resolve(&sandbox.path, "HEAD")?;
                git(
                    root,
                    &[
                        "fetch",
                        "--no-tags",
                        "--no-write-fetch-head",
                        &sandbox.path.to_string_lossy(),
                        &new,
                    ],
                )?;
                if fingerprint(root)? != review.fingerprint {
                    return Err("Files changed while preparing the rewrite. Safety points were kept; preview again".into());
                }
                git(
                    root,
                    &[
                        "update-ref",
                        &format!("refs/heads/{}", review.fingerprint.branch),
                        &new,
                        review.fingerprint.head.as_deref().unwrap_or(""),
                    ],
                )
            }
        }
    })();
    output.map(|out|format!("{out}\nAutomatic safety point saved\n{safety}")).map_err(|e|format!("{e}\nSafety points were kept in Time Machine. Resolve or abort any conflicts before trying again"))
}

/// Fetch updates without switching checkout, then build the same merge forecast used on the canvas.
pub fn preview_update(root: &Path, branch: &str, baseline: &str) -> Result<Preview> {
    crate::collaboration::require_idle(root)?;
    let before = fingerprint(root)?;
    let remote_ref = format!("refs/remotes/{baseline}");
    let source = if git(root, &["show-ref", "--verify", &remote_ref]).is_ok() {
        let (remote, name) = baseline.split_once('/').ok_or("Choose a remote branch")?;
        git(
            root,
            &[
                "fetch",
                "--progress",
                "--no-tags",
                remote,
                &format!("+refs/heads/{name}:{remote_ref}"),
            ],
        )?;
        remote_ref
    } else {
        baseline.to_owned()
    };
    if fingerprint(root)? != before {
        return Err("The checkout changed while fetching. Preview the update again".into());
    }
    preview(
        root,
        Kind::Merge {
            source,
            target: format!("refs/heads/{branch}"),
        },
    )
}

/// Preserve conflicted working files when Git cannot write a normal snapshot tree.
/// Abort remains a typed-confirmation escape hatch for unsupported file types.
pub fn advanced_safety(root: &Path, args: &[String]) -> Result<String> {
    crate::safety::ensure_writable(root)?;
    match snapshot::create(root, "Before an Advanced Git action") {
        Ok(saved) => Ok(saved),
        Err(_) if args.iter().any(|a| a == "--abort" || a == "rm") => {
            let mut copies = 0;
            let mut unsupported = 0;
            for change in git::parse_status(&git(
                root,
                &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
            )?) {
                match crate::recovery::backup(root, &change.path) {
                    Ok(_) => copies += 1,
                    Err(_) => unsupported += 1,
                }
            }
            if let Ok(head) = git::resolve(root, "HEAD") {
                git(
                    root,
                    &[
                        "update-ref",
                        &format!("refs/branchroom/recovery/{}", snapshot::unique_id()),
                        &head,
                        "",
                    ],
                )?;
            }
            Ok(format!(
                "Saved {copies} file recovery copies and the branch tip before resolving or aborting. {unsupported} unsupported paths are covered by the explicit typed confirmation"
            ))
        }
        Err(error) => Err(error),
    }
}
