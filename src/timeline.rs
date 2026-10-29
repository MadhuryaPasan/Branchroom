//! Persistent local action history. Checkpoints include HEAD, local refs, index and working files.
use crate::{
    git::{self, git},
    process::{self, Result},
    snapshot,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct State {
    pub head: Option<String>,
    pub branch: Option<String>,
    pub working: String,
    pub index: String,
    pub refs: BTreeMap<String, String>,
    pub stashes: Vec<(String, String)>,
    #[serde(default)]
    pub branch_config: Option<BTreeMap<String, Vec<String>>>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub label: String,
    pub before: Option<State>,
    pub after: Option<State>,
    pub undone: bool,
    pub retired: bool,
    pub hidden: bool,
    pub note: String,
    #[serde(default)]
    continuation: Option<String>,
}
fn directory(root: &Path) -> Result<PathBuf> {
    Ok(
        PathBuf::from(git(root, &["rev-parse", "--absolute-git-dir"])?.trim())
            .join("branchroom-timeline"),
    )
}
fn save(root: &Path, entry: &Entry) -> Result<()> {
    let folder = directory(root)?;
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    crate::storage::write_atomic(
        &folder.join(format!("{}.json", entry.id)),
        &serde_json::to_vec(entry).map_err(|e| e.to_string())?,
    )
}
pub fn entries(root: &Path) -> Result<Vec<Entry>> {
    let folder = directory(root)?;
    if !folder.exists() {
        return Ok(vec![]);
    }
    let mut result = Vec::new();
    for path in fs::read_dir(folder).map_err(|e| e.to_string())? {
        let path = path.map_err(|e| e.to_string())?.path();
        if path.extension().is_some_and(|v| v == "json") {
            let entry: Entry = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
                .map_err(|e| format!("Cannot read action history: {e}"))?;
            if !entry.id.is_empty() && entry.id.bytes().all(|b| b.is_ascii_hexdigit()) {
                result.push(entry);
            }
        }
    }
    result.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(result)
}
fn refs(root: &Path) -> Result<BTreeMap<String, String>> {
    Ok(git(
        root,
        &[
            "for-each-ref",
            "--format=%(refname) %(objectname)",
            "refs/heads",
            "refs/tags",
            "refs/stash",
            "refs/branchroom/snapshots",
            "refs/branchroom/deleted-snapshots",
        ],
    )?
    .lines()
    .filter_map(|line| line.split_once(' '))
    .map(|(a, b)| (a.into(), b.into()))
    .collect())
}
fn ready(root: &Path) -> Result<()> {
    crate::safety::ensure_writable(root)?;
    let dir = PathBuf::from(git(root, &["rev-parse", "--absolute-git-dir"])?.trim());
    if [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-merge",
        "rebase-apply",
        "sequencer",
    ]
    .iter()
    .any(|name| dir.join(name).exists())
    {
        return Err(
            "Finish or cancel the active Git operation before restoring a safety point".into(),
        );
    }
    if !git(root, &["ls-files", "-u"])?.is_empty() {
        return Err("Resolve the conflicted files before restoring a safety point".into());
    }
    if !git(root, &["ls-files", "--stage"])?
        .lines()
        .all(|l| !l.starts_with("160000"))
    {
        return Err("This checkout contains a nested Git repository. Use the Advanced recovery commands to restore it explicitly".into());
    }
    Ok(())
}
pub fn capture(root: &Path, label: &str) -> Result<State> {
    ready(root)?;
    let reference_map = refs(root)?;
    if git(root, &["ls-files", "-v"])?
        .lines()
        .any(|l| l.starts_with('S') || l.chars().next().is_some_and(|c| c.is_ascii_lowercase()))
    {
        return Err("This checkout has hidden or assumed-unchanged files. Clear those file flags before using full-state restoration".into());
    }
    let head = git::resolve(root, "HEAD").ok();
    let branch = git(root, &["symbolic-ref", "--quiet", "HEAD"])
        .ok()
        .map(|s| s.trim().to_owned());
    let index = git(root, &["write-tree"])?.trim().to_owned();
    let created = snapshot::create(root, label)?;
    let reference = created
        .lines()
        .find(|l| l.starts_with("refs/branchroom/snapshots/"))
        .ok_or("Missing safety point")?;
    let working = git::resolve(root, reference)?;
    if git(root, &["ls-tree", "-r", &working])?
        .lines()
        .any(|l| l.starts_with("160000"))
    {
        return Err("A working folder contains a nested Git repository. Restore it with the Advanced commands explicitly".into());
    }

    let internal = format!("refs/branchroom/timeline/{}", snapshot::unique_id());
    git(
        root,
        &["update-ref", &format!("{internal}/working"), &working],
    )?;
    // Pin index trees too, including partially staged content that exists in no commit.
    let indexed = git(
        root,
        &["commit-tree", &index, "-m", "Saved included changes"],
    )?;
    git(
        root,
        &["update-ref", &format!("{internal}/index"), indexed.trim()],
    )?;
    git(root, &["update-ref", "-d", reference, &working])?;
    let stashes = git(root, &["stash", "list", "--format=%H%x00%gs"])?
        .lines()
        .filter_map(|l| l.split_once('\0'))
        .map(|(a, b)| (a.to_owned(), b.to_owned()))
        .collect::<Vec<_>>();
    let pins = reference_map
        .values()
        .chain(stashes.iter().map(|(id, _)| id))
        .collect::<std::collections::BTreeSet<_>>();
    let mut transaction = String::from("start\n");
    for (n, oid) in pins.into_iter().enumerate() {
        transaction.push_str(&format!("create {internal}/saved-{n} {oid}\n"));
    }
    transaction.push_str("prepare\ncommit\n");
    process::input(
        "git",
        root,
        &process::strings(&["update-ref", "--stdin"]),
        transaction.as_bytes(),
        &[],
    )?;
    Ok(State {
        branch_config: Some(branch_config(root)?),
        head,
        branch,
        index,
        working,
        refs: reference_map,
        stashes,
    })
}
fn tree(root: &Path, id: &str) -> Result<String> {
    Ok(
        git(root, &["rev-parse", "--verify", &format!("{id}^{{tree}}")])?
            .trim()
            .into(),
    )
}
fn equivalent(a: &State, b: &State, root: &Path) -> bool {
    a.head == b.head
        && a.branch == b.branch
        && a.index == b.index
        && a.refs == b.refs
        && a.stashes == b.stashes
        && a.branch_config
            .as_ref()
            .is_none_or(|config| b.branch_config.as_ref() == Some(config))
        && tree(root, &a.working)
            .ok()
            .zip(tree(root, &b.working).ok())
            .is_some_and(|(a, b)| a == b)
}

pub fn begin(root: &Path, label: &str) -> Result<Entry> {
    let before = capture(root, &format!("Before {label}"))?;
    let entry = Entry {
        id: snapshot::unique_id(),
        label: label.into(),
        before: Some(before),
        after: None,
        undone: false,
        retired: false,
        hidden: false,
        note: "Action started; safety point saved".into(),
        continuation: None,
    };
    save(root, &entry)?;
    Ok(entry)
}
pub fn finish(root: &Path, mut entry: Entry, success: bool) -> Result<()> {
    match capture(root, &format!("After {}", entry.label)) {
        Ok(after) => {
            if entry.before.is_none()
                && let Some(mut paused) = entries(root)?.into_iter().find(|e| {
                    Some(&e.id) == entry.continuation.as_ref()
                        && e.before.is_some()
                        && e.after.is_none()
                        && !e.retired
                })
            {
                entry.before = paused.before.clone();
                entry.retired = false;
                entry.label = format!("Complete {}", paused.label);
                paused.retired = true;
                save(root, &paused)?;
            }

            if entry
                .before
                .as_ref()
                .is_some_and(|b| equivalent(b, &after, root))
            {
                entry.hidden = true;
                entry.retired = true;
            } else {
                for mut old in entries(root)? {
                    if old.undone && !old.retired {
                        old.retired = true;
                        save(root, &old)?;
                    }
                }
            }
            if !success {
                entry.retired = true;
            }
            entry.after = Some(after);
            entry.note = if success {
                "Completed"
            } else {
                "The action reported a problem; changed state was saved"
            }
            .into();
        }
        Err(error) => {
            entry.note = format!("The action paused. {error}");
        }
    }
    save(root, &entry)
}
pub fn hide(root: &Path, id: &str) -> Result<()> {
    crate::safety::ensure_writable(root)?;
    let mut entry = entries(root)?
        .into_iter()
        .find(|e| e.id == id)
        .ok_or("Action no longer available")?;
    entry.hidden = true;
    save(root, &entry)
}
/// Reversible removal from the visible timeline. Safety data is always retained.
pub fn show_hidden(root: &Path, id: &str) -> Result<()> {
    crate::safety::ensure_writable(root)?;
    let mut entry = entries(root)?
        .into_iter()
        .find(|e| e.id == id)
        .ok_or("Action no longer available")?;
    entry.hidden = false;
    save(root, &entry)
}
pub fn candidate(root: &Path, redo: bool) -> Result<Entry> {
    let all = entries(root)?;
    if redo {
        all.into_iter()
            .rev()
            .find(|e| e.undone && !e.retired && e.after.is_some() && e.before.is_some())
            .ok_or_else(|| "There is no action to redo".into())
    } else {
        all.into_iter()
            .find(|e| !e.undone && !e.retired && e.after.is_some() && e.before.is_some())
            .ok_or_else(|| "There is no action to undo".into())
    }
}
fn restore_files(root: &Path, from: &State, to: &State) -> Result<()> {
    // Git's two-tree checkout can overwrite ignored files. Refuse every path collision first.
    let ignored = git(
        root,
        &[
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "-z",
        ],
    )?;
    let targets = git(root, &["ls-tree", "-r", "--name-only", "-z", &to.working])?;
    for ignored in ignored.split('\0').filter(|p| !p.is_empty()) {
        if targets.split('\0').filter(|p| !p.is_empty()).any(|p| {
            p == ignored
                || p.starts_with(&format!("{ignored}/"))
                || ignored.starts_with(&format!("{p}/"))
        }) {
            return Err(format!(
                "An ignored local file would be overwritten: {ignored}. Move it outside the repository before restoring"
            ));
        }
    }
    if git(root, &["ls-tree", "-r", &to.working])?
        .lines()
        .any(|l| l.starts_with("160000"))
    {
        return Err("Restore nested Git repositories explicitly with Advanced commands".into());
    }
    // The checked working tree is used as the two-tree checkout's starting index.
    git(root, &["read-tree", &from.working])?;
    if let Err(error) = git(root, &["update-index", "--refresh"]) {
        let _ = git(root, &["read-tree", &from.index]);
        return Err(error);
    }

    if let Err(error) = git(root, &["read-tree", "-m", "-u", &from.working, &to.working]) {
        let _ = git(root, &["read-tree", &from.index]);
        return Err(error);
    }
    git(root, &["read-tree", &to.index])?;
    Ok(())
}
fn ref_transaction(from: &State, to: &State) -> String {
    let mut transaction = String::from("start\n");
    for (name, old) in &from.refs {
        if name != "refs/stash" && to.refs.get(name) != Some(old) {
            if let Some(new) = to.refs.get(name) {
                transaction.push_str(&format!("update {name} {new} {old}\n"));
            } else {
                transaction.push_str(&format!("delete {name} {old}\n"));
            }
        }
    }
    for (name, new) in &to.refs {
        if name != "refs/stash" && !from.refs.contains_key(name) {
            transaction.push_str(&format!("create {name} {new}\n"));
        }
    }
    transaction.push_str("prepare\ncommit\n");
    transaction
}
fn restore_state(root: &Path, from: &State, to: &State) -> Result<()> {
    ready(root)?;
    let worktrees = git(root, &["worktree", "list", "--porcelain"])?;
    for line in worktrees.lines().filter_map(|l| l.strip_prefix("branch ")) {
        if Some(line) != from.branch.as_deref()
            && (from.refs.get(line) != to.refs.get(line) || Some(line) == to.branch.as_deref())
        {
            return Err("A branch to restore is open in another parallel workspace. Close that workspace first".into());
        }
    }
    let transaction = ref_transaction(from, to);
    restore_files(root, from, to)?;
    if let Err(error) = process::input(
        "git",
        root,
        &process::strings(&["update-ref", "--no-deref", "--stdin"]),
        transaction.as_bytes(),
        &[],
    ) {
        let _ = restore_files(root, to, from);
        return Err(error);
    }
    if let Some(saved) = &to.branch_config {
        let current = branch_config(root)?;
        for key in current
            .keys()
            .chain(saved.keys())
            .collect::<std::collections::BTreeSet<_>>()
        {
            if current.get(key) != saved.get(key) {
                if current.contains_key(key) {
                    git(root, &["config", "--local", "--unset-all", key])?;
                }
                if let Some(values) = saved.get(key) {
                    for value in values {
                        git(root, &["config", "--local", "--add", key, value])?;
                    }
                }
            }
        }
    }
    if from.stashes != to.stashes {
        if git::resolve(root, "refs/stash").is_ok() {
            git(root, &["update-ref", "-d", "refs/stash"])?;
        }
        for (id, label) in to.stashes.iter().rev() {
            git(root, &["stash", "store", "-m", label, id])?;
        }
    }
    if let Some(branch) = &to.branch {
        git(root, &["symbolic-ref", "HEAD", branch])?;
    } else if let Some(head) = &to.head {
        git(root, &["update-ref", "--no-deref", "HEAD", head])?;
    }
    Ok(())
}
pub fn undo_redo(root: &Path, redo: bool, expected_id: &str) -> Result<String> {
    let mut entry = candidate(root, redo)?;
    if entry.id != expected_id {
        return Err("The last action changed. Review it again".into());
    }
    let (from, to) = if redo {
        (entry.before.as_ref(), entry.after.as_ref())
    } else {
        (entry.after.as_ref(), entry.before.as_ref())
    };
    let (from, to) = (
        from.ok_or("No before state")?,
        to.ok_or("No completed state")?,
    );
    let current = capture(root, "Before restoring last action")?;
    if !equivalent(from, &current, root) {
        return Err("Your repository changed after this action. Your newer work was kept. Use Time Machine to compare or restore an individual safety point".into());
    }
    restore_state(root, &current, to)?;
    entry.undone = !redo;
    save(root, &entry)?;
    Ok(format!(
        "{} {}",
        if redo { "Redid" } else { "Undid" },
        entry.label
    ))
}
#[derive(Clone, Debug, Default)]
pub struct Item {
    pub key: String,
    pub label: String,
    pub date: String,
    pub kind: String,
    pub target: String,
    pub reference: String,
    pub action: String,
    pub timestamp: u128,
    pub hidden: bool,
}
fn timestamp(id: &str) -> u128 {
    u128::from_str_radix(id.get(..32).unwrap_or(""), 16).unwrap_or(0) / 1_000_000_000
}
pub fn items(root: &Path, limit: usize) -> Result<Vec<Item>> {
    let mut result = Vec::new();
    for entry in entries(root)? {
        if entry.hidden {
            continue;
        }
        result.push(Item {
            key: format!("action:{}", entry.id),
            label: entry.label.clone(),
            date: format!(
                "{} · {}",
                age(timestamp(&entry.id)),
                if entry.undone {
                    "Undone".into()
                } else {
                    entry.note.clone()
                }
            ),
            kind: "Action".into(),
            target: entry
                .after
                .as_ref()
                .or(entry.before.as_ref())
                .map(|s| s.working.clone())
                .unwrap_or_default(),
            reference: String::new(),
            timestamp: timestamp(&entry.id),
            action: entry.id,
            hidden: false,
        });
    }
    let refs = git(
        root,
        &[
            "for-each-ref",
            "--format=%(refname)%00%(objectname)%00%(subject)%00%(creatordate:iso8601)%00%(creatordate:unix)",
            "refs/branchroom/snapshots",
            "refs/branchroom/deleted-snapshots",
            "refs/branchroom/recovery",
        ],
    )?;
    for line in refs.lines() {
        let f = line.split('\0').collect::<Vec<_>>();
        if f.len() != 5 {
            continue;
        }
        let kind = if f[0].contains("/deleted-snapshots/") {
            "Deleted safety point"
        } else if f[0].contains("/recovery/") {
            "History safety point"
        } else {
            "Safety point"
        };
        result.push(Item {
            key: f[0].into(),
            reference: f[0].into(),
            label: f[2].into(),
            target: f[1].into(),
            date: f[3].into(),
            kind: kind.into(),
            timestamp: f[4].parse().unwrap_or(0),
            ..Default::default()
        });
    }
    let log = git(
        root,
        &[
            "log",
            "--branches",
            "--remotes",
            "--tags",
            &format!("--max-count={limit}"),
            "--format=%H%x00%s%x00%ci%x00%ct",
        ],
    )
    .or_else(|e| {
        if git::resolve(root, "HEAD").is_err() {
            Ok(String::new())
        } else {
            Err(e)
        }
    })?;
    for line in log.lines() {
        let f = line.split('\0').collect::<Vec<_>>();
        if f.len() != 4 {
            continue;
        }
        result.push(Item {
            key: format!("commit:{}", f[0]),
            target: f[0].into(),
            label: f[1].into(),
            date: f[2].into(),
            kind: "Commit".into(),
            timestamp: f[3].parse().unwrap_or(0),
            ..Default::default()
        });
    }
    for entry in crate::recovery::entries(root) {
        result.push(Item {
            key: format!("discard:{}", entry.id),
            reference: entry.id.clone(),
            target: entry.head.unwrap_or_default(),
            label: format!("Recover {}", entry.path),
            kind: "Discarded file".into(),
            date: format!("{} · Saved working file", age(timestamp(&entry.id))),
            timestamp: timestamp(&entry.id),
            ..Default::default()
        });
    }
    let shelves = git(
        root,
        &["stash", "list", "--format=%gd%x00%H%x00%gs%x00%ci%x00%ct"],
    )?;
    for line in shelves.lines() {
        let f = line.split('\0').collect::<Vec<_>>();
        if f.len() != 5 {
            continue;
        }
        result.push(Item {
            key: format!("shelf:{}", f[1]),
            reference: f[0].into(),
            target: f[1].into(),
            label: f[2].into(),
            date: f[3].into(),
            kind: "Shelved work".into(),
            timestamp: f[4].parse().unwrap_or(0),
            ..Default::default()
        });
    }
    let hidden = hidden_keys(root)?;
    for item in &mut result {
        item.hidden = hidden.contains(&item.key);
    }
    result.sort_by(|a, b| b.timestamp.cmp(&a.timestamp).then(b.key.cmp(&a.key)));
    Ok(result)
}
pub fn graph(root: &Path, target: &str) -> Result<Vec<git::Commit>> {
    let id = git::resolve(root, target)?;
    Ok(git::parse_commits(&git(
        root,
        &[
            "log",
            "--max-count=30",
            "--format=%H%x00%P%x00%D%x00%aN%x00%ar%x00%s%x00%aE",
            "-z",
            &id,
        ],
    )?))
}

pub fn unavailable(root: &Path, label: &str, error: &str) -> Result<Entry> {
    let dir = PathBuf::from(git(root, &["rev-parse", "--absolute-git-dir"])?.trim());
    let head = git::resolve(root, "HEAD").ok();
    let active = [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-merge",
        "rebase-apply",
        "sequencer",
    ]
    .iter()
    .any(|name| dir.join(name).exists());
    let continuation = if active {
        entries(root)?
            .into_iter()
            .find(|e| {
                e.after.is_none() && !e.retired && e.before.as_ref().is_some_and(|s| s.head == head)
            })
            .map(|e| e.id)
    } else {
        None
    };
    let entry = Entry {
        id: snapshot::unique_id(),
        label: label.into(),
        before: None,
        after: None,
        undone: false,
        retired: true,
        hidden: false,
        note: format!("Use individual recovery points: {error}"),
        continuation,
    };
    save(root, &entry)?;
    Ok(entry)
}
#[derive(Clone, Debug)]
pub struct RestorePlan {
    root: PathBuf,
    pub from: State,
    pub to: State,
    pub label: String,
    pub graph: Vec<git::Commit>,
    pub before_graph: Vec<git::Commit>,
    pub summary: String,
    pub changes: String,
}
pub fn plan_restore(root: &Path, item: &Item, before: bool) -> Result<RestorePlan> {
    let from = capture(root, "Before reviewing restore")?;
    let to = if !item.action.is_empty() {
        let entry = entries(root)?
            .into_iter()
            .find(|e| e.id == item.action)
            .ok_or("The action is no longer available")?;
        if before { entry.before } else { entry.after }
            .ok_or("This action has no complete safety point. Choose an individual saved version")?
    } else {
        let mut state = from.clone();
        let id = git::resolve(root, &item.target)?;
        if item.kind == "Commit" || item.kind == "History safety point" {
            state.head = Some(id.clone());
            if let Some(branch) = &state.branch {
                state.refs.insert(branch.clone(), id.clone());
            }
        }
        state.index = tree(root, state.head.as_deref().unwrap_or(&id))?;
        state.working = id;
        state
    };
    let changes = git(root, &["diff", "--stat", &from.working, &to.working])?;
    let graph = state_graph(root, &to)?;
    let before_graph = state_graph(root, &from)?;
    let summary = if from.head != to.head {
        format!(
            "Move {} from {} to {}. Nothing will be pushed",
            to.branch
                .as_deref()
                .unwrap_or("the detached history")
                .trim_start_matches("refs/heads/"),
            from.head
                .as_deref()
                .and_then(|id| id.get(..7))
                .unwrap_or("before the first commit"),
            to.head
                .as_deref()
                .and_then(|id| id.get(..7))
                .unwrap_or("before the first commit")
        )
    } else {
        "Restore the saved files and included changes. Branch history stays at the same commit"
            .into()
    };
    Ok(RestorePlan {
        root: fs::canonicalize(root).map_err(|e| e.to_string())?,
        before_graph,
        summary,
        from,
        to,
        label: format!(
            "Restore {}{}",
            if before { "before " } else { "" },
            item.label
        ),
        graph,
        changes,
    })
}
pub fn apply_restore(root: &Path, plan: &RestorePlan) -> Result<String> {
    if fs::canonicalize(root).map_err(|e| e.to_string())? != plan.root {
        return Err("This restore preview belongs to another repository".into());
    }
    let current = capture(root, "Before restoring a selected version")?;
    if !equivalent(&current, &plan.from, root) {
        return Err("Your repository changed after this preview. Review the restore again".into());
    }
    restore_state(root, &current, &plan.to)?;
    Ok(plan.label.clone())
}

fn hidden_keys(root: &Path) -> Result<std::collections::BTreeSet<String>> {
    let path = directory(root)?.join("visibility");
    if !path.exists() {
        return Ok(Default::default());
    }
    serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
pub fn set_visible(root: &Path, key: &str, visible: bool) -> Result<()> {
    crate::safety::ensure_writable(root)?;
    let mut hidden = hidden_keys(root)?;
    if visible {
        hidden.remove(key);
    } else {
        hidden.insert(key.into());
    }
    crate::storage::write_atomic(
        &directory(root)?.join("visibility"),
        &serde_json::to_vec(&hidden).map_err(|e| e.to_string())?,
    )
}

fn age(seconds: u128) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as u128;
    let elapsed = now.saturating_sub(seconds);
    if elapsed < 60 {
        "Just now".into()
    } else if elapsed < 3600 {
        format!("{} min ago", elapsed / 60)
    } else if elapsed < 86400 {
        format!("{} hr ago", elapsed / 3600)
    } else {
        format!("{} days ago", elapsed / 86400)
    }
}

impl RestorePlan {
    pub fn git_command(&self) -> String {
        let command = |args: &[&str]| {
            crate::operation::display_command(
                &args.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>(),
            )
        };
        let mut lines = vec![
            command(&["read-tree", &self.from.working]),
            command(&["update-index", "--refresh"]),
            command(&[
                "read-tree",
                "-m",
                "-u",
                &self.from.working,
                &self.to.working,
            ]),
            command(&["read-tree", &self.to.index]),
        ];
        let transaction = ref_transaction(&self.from, &self.to);
        if cfg!(windows) {
            lines.push(format!(
                "@'\n{transaction}'@ | git update-ref --no-deref --stdin"
            ));
        } else {
            lines.push(format!("git update-ref --no-deref --stdin <<'BRANCHROOM_RESTORE'\n{transaction}BRANCHROOM_RESTORE"));
        }
        if let Some(saved) = &self.to.branch_config {
            let current = self.from.branch_config.clone().unwrap_or_default();
            for key in current
                .keys()
                .chain(saved.keys())
                .collect::<std::collections::BTreeSet<_>>()
            {
                if current.get(key) != saved.get(key) {
                    if current.contains_key(key) {
                        lines.push(command(&["config", "--local", "--unset-all", key]));
                    }
                    if let Some(values) = saved.get(key) {
                        for value in values {
                            lines.push(command(&["config", "--local", "--add", key, value]));
                        }
                    }
                }
            }
        }
        if self.from.stashes != self.to.stashes {
            if self.from.refs.contains_key("refs/stash") {
                lines.push(command(&["update-ref", "-d", "refs/stash"]));
            }
            for (id, label) in self.to.stashes.iter().rev() {
                lines.push(command(&["stash", "store", "-m", label, id]));
            }
        }
        if let Some(branch) = &self.to.branch {
            lines.push(command(&["symbolic-ref", "HEAD", branch]));
        } else if let Some(head) = &self.to.head {
            lines.push(command(&["update-ref", "--no-deref", "HEAD", head]));
        }
        lines.join("\n")
    }
}

pub fn state_graph(root: &Path, state: &State) -> Result<Vec<git::Commit>> {
    let tips = state
        .refs
        .iter()
        .filter(|(name, _)| name.starts_with("refs/heads/"))
        .map(|(_, id)| id)
        .chain(state.head.iter())
        .collect::<std::collections::BTreeSet<_>>();
    if tips.is_empty() {
        return Ok(vec![]);
    }
    let mut args = vec![
        "log",
        "--topo-order",
        "--max-count=30",
        "--format=%H%x00%P%x00%D%x00%aN%x00%ar%x00%s%x00%aE",
        "-z",
    ];
    args.extend(tips.into_iter().map(String::as_str));
    let mut commits = git::parse_commits(&git(root, &args)?);
    for commit in &mut commits {
        commit.refs = state
            .refs
            .iter()
            .filter(|(name, id)| name.starts_with("refs/heads/") && **id == commit.id)
            .map(|(name, _)| {
                format!(
                    "{}{}",
                    if state.branch.as_ref() == Some(name) {
                        "HEAD -> "
                    } else {
                        ""
                    },
                    name.trim_start_matches("refs/heads/")
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
    }
    Ok(commits)
}

fn branch_config(root: &Path) -> Result<BTreeMap<String, Vec<String>>> {
    // Listing an existing local config succeeds even when no tracked sections exist.
    // Do not infer Git's exit code from platform-dependent ExitStatus display text.
    let text = git(root, &["config", "--local", "--null", "--list"])?;
    let mut result: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for record in text.split('\0') {
        if let Some((key, value)) = record.split_once('\n')
            && (key.starts_with("branch.") || key.starts_with("remote."))
        {
            result.entry(key.into()).or_default().push(value.into());
        }
    }
    Ok(result)
}
