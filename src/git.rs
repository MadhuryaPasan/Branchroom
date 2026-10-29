use crate::process::{self, Result, strings};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default)]
pub struct Change {
    pub index: char,
    pub worktree: char,
    pub path: String,
    pub original: Option<String>,
}
impl Change {
    pub fn conflict(&self) -> bool {
        self.index == 'U'
            || self.worktree == 'U'
            || matches!((self.index, self.worktree), ('A', 'A') | ('D', 'D'))
    }
}
#[derive(Clone, Debug, Default)]
pub struct Commit {
    pub id: String,
    pub parents: Vec<String>,
    pub refs: String,
    pub author: String,
    pub date: String,
    pub subject: String,
}
#[derive(Clone, Debug, Default)]
pub struct Branch {
    pub name: String,
    pub current: bool,
    pub upstream: String,
    pub tracking: String,
    pub ahead: usize,
    pub behind: usize,
    pub remote: bool,
    pub id: String,
    pub fork_point: String,
}
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub reference: String,
    pub id: String,
    pub label: String,
    pub date: String,
    pub parents: Vec<String>,
}
#[derive(Clone, Debug, Default)]
pub struct RepoState {
    pub timeline: Vec<crate::timeline::Item>,
    pub default_branch: Option<String>,
    pub root: PathBuf,
    pub branch: String,
    pub base: String,
    pub tracking: String,
    /// Current branch compared with its remote-tracking branch, not the selected comparison base.
    pub sync_counts: Option<(usize, usize)>,
    pub changes: Vec<Change>,
    pub commit_locks: Vec<String>,
    pub commits: Vec<Commit>,
    pub branch_commits: Vec<Commit>,
    pub branches: Vec<Branch>,
    pub snapshots: Vec<Snapshot>,
    pub removed_snapshots: Vec<Snapshot>,
    pub recovery_heads: Vec<Snapshot>,
    pub stashes: Vec<(String, String)>,
    pub remotes: String,
    pub worktrees: String,
    pub tags: Vec<String>,
    pub operation: String,
    pub head: String,
}

pub fn git(path: &Path, args: &[&str]) -> Result<String> {
    // Diagnostics must not become machine-readable records (notably CRLF warnings on Windows).
    process::stdout("git", path, &strings(args))
}
/// Literal local branch name, unaffected by tags with the same short name.
pub fn current_branch(root: &Path) -> Result<String> {
    let reference = git(root, &["symbolic-ref", "--quiet", "HEAD"])?;
    reference
        .trim()
        .strip_prefix("refs/heads/")
        .map(str::to_owned)
        .ok_or_else(|| "Switch to a local branch".into())
}
/// Branch selectors prefer a named branch; explicit refs and object IDs retain Git's meaning.
pub fn resolve_named_commit(root: &Path, name: &str) -> Result<(String, String)> {
    if !name.starts_with("refs/")
        && name != "HEAD"
        && !((name.len() == 40 || name.len() == 64) && name.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        for prefix in ["refs/heads/", "refs/remotes/"] {
            let reference = format!("{prefix}{name}");
            if let Ok(id) = resolve(root, &reference) {
                return Ok((reference, id));
            }
        }
    }
    Ok((name.into(), resolve(root, name)?))
}
pub fn resolve(path: &Path, reference: &str) -> Result<String> {
    Ok(git(
        path,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{reference}^{{commit}}"),
        ],
    )?
    .trim()
    .to_owned())
}

pub fn parse_status(output: &str) -> Vec<Change> {
    let mut records = output.split('\0').filter(|s| !s.is_empty());
    let mut result = vec![];
    while let Some(record) = records.next() {
        if record.len() < 4 {
            continue;
        }
        let bytes = record.as_bytes();
        let renamed = matches!(bytes[0], b'R' | b'C') || matches!(bytes[1], b'R' | b'C');
        result.push(Change {
            index: bytes[0] as char,
            worktree: bytes[1] as char,
            path: record[3..].to_owned(),
            original: if renamed {
                records.next().map(str::to_owned)
            } else {
                None
            },
        });
    }
    result
}

pub fn load(path: &Path, preferred_base: &str, limit: usize) -> Result<RepoState> {
    let root = PathBuf::from(git(path, &["rev-parse", "--show-toplevel"])?.trim());
    let head = resolve(&root, "HEAD").unwrap_or_default();
    let branch = current_branch(&root).unwrap_or_else(|_| "Detached HEAD".into());
    let base = if resolve(&root, preferred_base).is_ok() {
        preferred_base.to_owned()
    } else {
        ["origin/main", "main", "origin/master", "master"]
            .iter()
            .find(|r| resolve(&root, r).is_ok())
            .unwrap_or(&"HEAD")
            .to_string()
    };
    let changes = parse_status(&git(
        &root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?);
    let count = format!("--max-count={limit}");
    // User history excludes internal agent/tool refs, which can contain private checkpoints.
    let mut history_args = vec![
        "log",
        "--branches",
        "--remotes",
        "--tags",
        "--glob=refs/branchroom/snapshots/*",
        "--topo-order",
        &count,
        "-z",
        "--format=%H%x00%P%x00%D%x00%aN%x00%ar%x00%s%x00%aE",
    ];
    if !head.is_empty() {
        history_args.push("HEAD");
    }
    let history = git(&root, &history_args);
    let commits = parse_commits(&match history {
        Ok(text) => text,
        Err(_) if head.is_empty() => String::new(),
        Err(e) => return Err(e),
    });
    // Query HEAD independently: busy unrelated branches must not consume this branch's limit.
    let branch_commits = if head.is_empty() {
        vec![]
    } else {
        parse_commits(&git(
            &root,
            &[
                "log",
                "--topo-order",
                &count,
                "-z",
                "--format=%H%x00%P%x00%D%x00%aN%x00%ar%x00%s%x00%aE",
                &head,
            ],
        )?)
    };
    let base_id = resolve_named_commit(&root, &base).ok().map(|(_, id)| id);
    let format = format!(
        "--format=%(refname:short)%09%(HEAD)%09%(upstream)%09%(upstream:track)%09%(refname)%09%(objectname)%09{}%09%(symref)",
        base_id
            .as_ref()
            .map(|id| format!("%(ahead-behind:{id})"))
            .unwrap_or_default()
    );
    let refs = git(
        &root,
        &["for-each-ref", &format, "refs/heads", "refs/remotes"],
    )
    .map_err(|e| format!("Cannot load branch comparisons. Git 2.41 or newer is required.\n{e}"))?;
    let mut branches = Vec::new();
    for line in refs.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() < 8 {
            return Err("Git returned an incomplete branch record. Refresh the repository.".into());
        }
        // Remote HEAD is a symbolic pointer, not a branch that can be checked out.
        if fields[4].starts_with("refs/remotes/") && !fields[7].is_empty() {
            continue;
        }
        let counts: Vec<usize> = fields[6]
            .split_whitespace()
            .filter_map(|n| n.parse().ok())
            .collect();
        if base_id.is_some() && counts.len() != 2 {
            return Err(
                "Git returned incomplete ahead/behind counts. Refresh the repository.".into(),
            );
        }
        let ahead = counts.first().copied().unwrap_or(0);
        let behind = counts.get(1).copied().unwrap_or(0);
        let fork_point = if let Some(base_id) = &base_id {
            if ahead == 0 {
                fields[5].to_string()
            } else if behind == 0 {
                base_id.clone()
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        branches.push(Branch {
            name: fields[4]
                .strip_prefix("refs/heads/")
                .or_else(|| fields[4].strip_prefix("refs/remotes/"))
                .unwrap_or(fields[0])
                .into(),
            current: fields[1] == "*",
            upstream: fields[2]
                .strip_prefix("refs/heads/")
                .or_else(|| fields[2].strip_prefix("refs/remotes/"))
                .unwrap_or(fields[2])
                .into(),
            tracking: fields[3].into(),
            remote: fields[4].starts_with("refs/remotes/"),
            id: fields[5].into(),
            ahead,
            behind,
            fork_point,
        });
    }
    if let Some(base_id) = &base_id {
        fill_merge_bases(&root, base_id, &mut branches)?;
    }
    let sync_counts = branches.iter().find(|b| b.current).and_then(|current| {
        let remote = branches
            .iter()
            .find(|b| b.remote && b.name == current.upstream)?;
        let range = format!("{}...{}", current.id, remote.id);
        let output = git(&root, &["rev-list", "--left-right", "--count", &range]).ok()?;
        let mut counts = output.split_whitespace().map(str::parse::<usize>);
        let ahead = counts.next()?.ok()?;
        let behind = counts.next()?.ok()?;
        if counts.next().is_some() {
            return None;
        }
        Some((ahead, behind))
    });
    let snapshots = snapshot_refs(&root, "refs/branchroom/snapshots")?;
    let removed_snapshots = snapshot_refs(&root, "refs/branchroom/deleted-snapshots")?;
    let recovery_heads = snapshot_refs(&root, "refs/branchroom/recovery")?;
    let stashes = git(&root, &["stash", "list", "--format=%gd%x09%s"])?
        .lines()
        .filter_map(|s| s.split_once('\t').map(|(a, b)| (a.into(), b.into())))
        .collect();
    let git_dir = PathBuf::from(git(&root, &["rev-parse", "--absolute-git-dir"])?.trim());
    let operation =
        if git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists() {
            "rebase"
        } else if git_dir.join("MERGE_HEAD").exists() {
            "merge"
        } else if git_dir.join("CHERRY_PICK_HEAD").exists()
            || git_dir.join("sequencer").exists()
                && !std::fs::read_to_string(git_dir.join("sequencer/todo"))
                    .is_ok_and(|todo| todo.lines().any(|line| line.starts_with("revert ")))
        {
            "cherry-pick"
        } else if git_dir.join("REVERT_HEAD").exists()
            || std::fs::read_to_string(git_dir.join("sequencer/todo"))
                .is_ok_and(|todo| todo.lines().any(|line| line.starts_with("revert ")))
        {
            "revert"
        } else {
            ""
        }
        .to_owned();
    let commit_locks = crate::commit_lock::read(&root)?;
    Ok(RepoState {
        timeline: crate::timeline::items(&root, limit)?,
        default_branch: crate::operation::default_branch(&root),
        head,
        tracking: git(
            &root,
            &["status", "--short", "--branch", "--untracked-files=no"],
        )?
        .lines()
        .next()
        .unwrap_or_default()
        .into(),
        remotes: git(&root, &["remote", "-v"])?,
        worktrees: git(&root, &["worktree", "list"])?,
        tags: git(&root, &["tag", "--list"])?
            .lines()
            .map(str::to_owned)
            .collect(),
        root,
        branch,
        base,
        changes,
        commit_locks,
        commits,
        branch_commits,
        branches,
        sync_counts,
        snapshots,
        removed_snapshots,
        recovery_heads,
        stashes,
        operation,
    })
}

pub fn validate_branch(path: &Path, name: &str) -> Result<()> {
    if name.starts_with('-') || name.trim().is_empty() {
        return Err("Enter a branch name.".into());
    }
    git(path, &["check-ref-format", "--branch", name]).map(|_| ())
}

pub(crate) fn parse_commits(text: &str) -> Vec<Commit> {
    let fields: Vec<_> = text.split_terminator('\0').collect();
    let mut authors = std::collections::HashMap::<String, String>::new();
    fields
        .as_chunks::<7>()
        .0
        .iter()
        .map(|f| Commit {
            id: f[0].trim_start_matches('\n').into(),
            parents: f[1].split_whitespace().map(str::to_owned).collect(),
            refs: f[2].into(),
            author: if f[6].trim().is_empty() {
                f[3].into()
            } else {
                authors
                    .entry(f[6].trim().to_lowercase())
                    .or_insert_with(|| f[3].to_owned())
                    .clone()
            },
            date: f[4].into(),
            subject: f[5].into(),
        })
        .collect()
}

fn snapshot_refs(root: &Path, prefix: &str) -> Result<Vec<Snapshot>> {
    let output = git(
        root,
        &[
            "for-each-ref",
            "--sort=-refname",
            "--format=%(refname)%00%(objectname)%00%(subject)%00%(creatordate:relative)%00%(parent)%00",
            prefix,
        ],
    )?;
    let fields: Vec<_> = output.split('\0').collect();
    Ok(fields
        .as_chunks::<5>()
        .0
        .iter()
        .map(|f| Snapshot {
            reference: f[0].trim_start_matches('\n').into(),
            id: f[1].into(),
            label: f[2].into(),
            date: f[3].into(),
            parents: f[4].split_whitespace().map(str::to_owned).collect(),
        })
        .collect())
}

type MergeBaseCache = std::collections::HashMap<(PathBuf, String, String), String>;
fn cached_merge_base(root: &Path, base: &str, tip: &str) -> Result<String> {
    let cache = merge_base_cache();
    let key = (root.to_path_buf(), base.to_string(), tip.to_string());
    if let Ok(entries) = cache.lock()
        && let Some(id) = entries.get(&key)
    {
        return Ok(id.clone());
    }
    let id = match git(root, &["merge-base", base, tip]) {
        Ok(id) => id.trim().to_owned(),
        Err(e)
            if !process::is_cancelled()
                && e.lines().next().is_some_and(|s| s.ends_with('1'))
                && e.lines().skip(1).all(|s| s.trim().is_empty()) =>
        {
            String::new()
        }
        Err(e) => return Err(e),
    };
    // Object IDs make invalidation explicit: changed tips always use a new cache key.
    if let Ok(mut entries) = cache.lock() {
        if entries.len() >= 4096 {
            entries.clear();
        }
        entries.insert(key, id.clone());
    }
    Ok(id)
}

fn merge_base_cache() -> &'static std::sync::Mutex<MergeBaseCache> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<MergeBaseCache>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(Default::default)
}
fn fill_merge_bases(root: &Path, base: &str, branches: &mut [Branch]) -> Result<()> {
    let mut pending = Vec::new();
    for (index, branch) in branches
        .iter_mut()
        .enumerate()
        .filter(|(_, b)| b.ahead > 0 && b.behind > 0)
    {
        let key = (root.to_path_buf(), base.to_string(), branch.id.clone());
        if let Ok(cache) = merge_base_cache().lock()
            && let Some(id) = cache.get(&key)
        {
            branch.fork_point = id.clone();
            continue;
        }
        pending.push(index);
    }
    if pending.is_empty() {
        return Ok(());
    }
    let mut tips: std::collections::BTreeSet<&str> = pending
        .iter()
        .map(|index| branches[*index].id.as_str())
        .collect();
    tips.insert(base);
    let input = tips.into_iter().collect::<Vec<_>>().join("\n") + "\n";
    let text = process::input(
        "git",
        root,
        &strings(&[
            "rev-list",
            "--parents",
            "--topo-order",
            "--max-count=100000",
            "--stdin",
        ]),
        input.as_bytes(),
        &[],
    )?;
    let mut parents = std::collections::HashMap::new();
    let mut order = Vec::new();
    for line in text.lines() {
        let mut ids = line.split_whitespace();
        if let Some(id) = ids.next() {
            order.push(id);
            parents.insert(id, ids.collect::<Vec<_>>());
        }
    }
    let reachable = |tip: &str| {
        let mut seen = std::collections::HashSet::new();
        let mut queue: Vec<_> = parents
            .get_key_value(tip)
            .map(|(id, _)| *id)
            .into_iter()
            .collect();
        while let Some(id) = queue.pop() {
            if seen.insert(id)
                && let Some(ids) = parents.get(id)
            {
                queue.extend(ids.iter().copied());
            }
        }
        seen
    };
    let base_ancestors = reachable(base);
    for index in pending {
        let tip_ancestors = reachable(&branches[index].id);
        // Topological order puts every common descendant before its ancestors.
        let shared = order
            .iter()
            .find(|id| base_ancestors.contains(**id) && tip_ancestors.contains(**id))
            .map(|id| (*id).to_string());
        let id = if let Some(id) = shared {
            id
        } else {
            cached_merge_base(root, base, &branches[index].id)?
        };
        if let Ok(mut cache) = merge_base_cache().lock() {
            if cache.len() >= 4096 {
                cache.clear();
            }
            cache.insert(
                (
                    root.to_path_buf(),
                    base.to_string(),
                    branches[index].id.clone(),
                ),
                id.clone(),
            );
        }
        branches[index].fork_point = id;
    }
    Ok(())
}

#[cfg(test)]
mod author_tests {
    #[test]
    fn names_are_canonicalized_by_email_without_merging_distinct_people() {
        let rows = [
            [
                "a",
                "",
                "",
                "Ada Lovelace",
                "now",
                "First",
                "ada@example.test",
            ],
            [
                "b",
                "",
                "",
                "Ada",
                "yesterday",
                "Second",
                "ada@example.test",
            ],
            [
                "c",
                "",
                "",
                "Ada",
                "yesterday",
                "Third",
                "someone@example.test",
            ],
        ]
        .map(|row| row.join("\0") + "\0")
        .concat();
        let commits = super::parse_commits(&rows);
        assert_eq!(commits.len(), 3);
        assert_eq!(commits[0].author, commits[1].author);
        assert_eq!(commits[2].author, "Ada");
        assert_eq!(commits[1].subject, "Second");
    }
}
