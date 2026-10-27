use crate::{
    git::{git, resolve},
    process::{self, Result},
    snapshot::unique_id,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub enum Action {
    Undo,
    Reset(String),
    Squash { count: usize, message: String },
    Reorder(Vec<String>),
}

pub fn require_clean(root: &Path) -> Result<()> {
    if !git(root, &["status", "--porcelain=v1", "-z"])?.is_empty() {
        Err("Commit or stash your working changes before rewriting history.".into())
    } else {
        Ok(())
    }
}
fn branch(root: &Path) -> Result<String> {
    git(root, &["symbolic-ref", "--quiet", "HEAD"])
        .map(|s| s.trim().into())
        .map_err(|_| "Switch to a local branch before rewriting history.".into())
}

pub fn linear_tip(root: &Path, count: usize) -> Result<Vec<String>> {
    if !(1..=200).contains(&count) {
        return Err("Choose between 1 and 200 commits.".into());
    }
    let text = git(
        root,
        &[
            "rev-list",
            "--first-parent",
            "--parents",
            &format!("--max-count={count}"),
            "HEAD",
        ],
    )?;
    let mut ids = vec![];
    for line in text.lines() {
        let parts: Vec<_> = line.split_whitespace().collect();
        if parts.len() > 2 {
            return Err(
                "This range contains a merge commit. Choose a linear range before the merge."
                    .into(),
            );
        }
        if let Some(id) = parts.first() {
            ids.push((*id).into());
        }
    }
    if ids.len() != count {
        return Err("The requested range is longer than the branch history.".into());
    }
    ids.reverse();
    Ok(ids)
}

fn ensure_unpublished(root: &Path, ids: &[String]) -> Result<()> {
    let mut args = vec!["for-each-ref".to_string(), "--format=%(refname)".into()];
    args.extend(ids.iter().map(|id| format!("--contains={id}")));
    args.push("refs/remotes".into());
    if !process::stdout("git", root, &args)?.trim().is_empty() {
        return Err("A selected commit is reachable from a remote-tracking branch. Fetch and inspect shared history before attempting a manual rewrite.".into());
    }

    Ok(())
}

fn recovery_ref(root: &Path, head: &str) -> Result<String> {
    let reference = format!("refs/branchroom/recovery/{}", unique_id());
    git(root, &["update-ref", &reference, head, ""])?;
    Ok(reference)
}

pub fn run(root: &Path, action: &Action) -> Result<String> {
    crate::safety::ensure_writable(root)?;
    crate::collaboration::require_idle(root)?;
    let reference = branch(root)?;
    let old = resolve(root, "HEAD")?;
    match action {
        Action::Undo => {
            linear_tip(root, 1)?;
            ensure_unpublished(root, std::slice::from_ref(&old))?;
            let backup = recovery_ref(root, &old)?;
            if let Ok(parent) = resolve(root, "HEAD^") {
                git(root, &["reset", "--mixed", &parent])?;
            } else {
                git(root, &["update-ref", "-d", &reference, &old])?;
                git(root, &["read-tree", "--empty"])?;
            }
            Ok(format!(
                "Commit undone; its changes are in the working tree. Recovery ref: {backup}"
            ))
        }
        Action::Reset(target) => {
            let target = resolve(root, target)?;
            git(root, &["merge-base", "--is-ancestor", &target, &old])
                .map_err(|_| "Choose an ancestor of the current branch.".to_string())?;
            let ids: Vec<_> = git(root, &["rev-list", &format!("{target}..{old}")])?
                .lines()
                .map(str::to_owned)
                .collect();
            if ids.is_empty() {
                return Err("The branch already points at this commit.".into());
            }
            if ids.len() > 200 {
                return Err("Choose a reset range of 200 commits or fewer.".into());
            }
            ensure_unpublished(root, &ids)?;
            let backup = recovery_ref(root, &old)?;
            git(root, &["reset", "--mixed", &target])?;
            Ok(format!(
                "Branch reset; removed commits remain as working changes. Recovery ref: {backup}"
            ))
        }
        Action::Squash { count, message } => {
            if message.trim().is_empty() {
                return Err("Enter a message for the combined commit.".into());
            }
            let ids = linear_tip(root, *count)?;
            ensure_unpublished(root, &ids)?;
            let parent = resolve(root, &format!("{}^", ids[0])).ok();
            let tree = git(root, &["rev-parse", &format!("{old}^{{tree}}")])?;
            let mut args = vec!["commit-tree", tree.trim(), "-m", message.as_str()];
            if let Some(parent) = &parent {
                args.extend(["-p", parent.as_str()]);
            }
            let commit = git(root, &args)?;
            let backup = recovery_ref(root, &old)?;
            git(root, &["update-ref", &reference, commit.trim(), &old])?;
            Ok(format!(
                "Combined {count} commits. Working files and index are unchanged. Recovery ref: {backup}"
            ))
        }
        Action::Reorder(order) => reorder(root, order, &reference, &old),
    }
}

struct Index(PathBuf);
impl Drop for Index {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
        let _ = fs::remove_file(self.0.with_extension("index.lock"));
    }
}

fn reorder(root: &Path, order: &[String], reference: &str, old: &str) -> Result<String> {
    let expected = linear_tip(root, order.len())?;
    let (mut actual, mut supplied) = (expected.clone(), order.to_vec());
    actual.sort();
    supplied.sort();
    if actual != supplied {
        return Err("Select each commit in the tip range exactly once.".into());
    }
    ensure_unpublished(root, &expected)?;
    let base = resolve(root, &format!("{}^", expected[0]))
        .map_err(|_| "Keep the root commit outside the reordered range.".to_string())?;
    let index =
        Index(std::env::temp_dir().join(format!("branchroom-reorder-{}.index", unique_id())));
    let env = vec![("GIT_INDEX_FILE".into(), index.0.display().to_string())];
    process::input(
        "git",
        root,
        &process::strings(&["read-tree", &base]),
        &[],
        &env,
    )?;
    let mut parent = base;
    for id in order {
        let original_parent = resolve(root, &format!("{id}^"))?;
        let patch = git(root, &["diff", "--binary", &original_parent, id, "--"])?;
        if !patch.is_empty() {
            process::input(
                "git",
                root,
                &process::strings(&["apply", "--cached", "--3way", "--whitespace=nowarn"]),
                patch.as_bytes(),
                &env,
            )
            .map_err(|e| {
                format!(
                    "Reordering conflicts at {id}. The current branch and index are unchanged.\n{e}"
                )
            })?;
        }
        let tree = process::input("git", root, &process::strings(&["write-tree"]), &[], &env)?;
        let metadata = git(
            root,
            &["show", "-s", "--format=%an%x00%ae%x00%aI%x00%B", id],
        )?;
        let fields: Vec<_> = metadata.splitn(4, '\0').collect();
        if fields.len() != 4 {
            return Err("Cannot read commit author information.".into());
        }
        let mut author = env.clone();
        author.extend([
            ("GIT_AUTHOR_NAME".into(), fields[0].into()),
            ("GIT_AUTHOR_EMAIL".into(), fields[1].into()),
            ("GIT_AUTHOR_DATE".into(), fields[2].into()),
        ]);
        parent = process::input(
            "git",
            root,
            &process::strings(&["commit-tree", tree.trim(), "-p", &parent, "-F", "-"]),
            fields[3].as_bytes(),
            &author,
        )?
        .trim()
        .into();
    }
    let final_tree = git(root, &["rev-parse", &format!("{parent}^{{tree}}")])?;
    let original_tree = git(root, &["rev-parse", &format!("{old}^{{tree}}")])?;
    if final_tree.trim() != original_tree.trim() {
        return Err("The requested order changes the final file tree. The branch is unchanged; choose an order that preserves the final result.".into());
    }
    let backup = recovery_ref(root, old)?;
    git(root, &["update-ref", reference, &parent, old])?;
    Ok(format!(
        "Reordered {} commits. Recovery ref: {backup}",
        order.len()
    ))
}
