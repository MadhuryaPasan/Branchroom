//! Local commit exclusions. Working files stay visible and editable.
use crate::{
    git::{self, git},
    process::{self, Result},
};
use std::{
    collections::BTreeSet,
    path::{Component, Path},
};
const KEY: &str = "branchroom.commitLock";

fn validate(path: &str) -> Result<()> {
    if path.is_empty()
        || path.contains('\0')
        || Path::new(path)
            .components()
            .any(|p| !matches!(p, Component::Normal(_)))
        || path
            .split(['/', '\\'])
            .any(|p| p.eq_ignore_ascii_case(".git"))
    {
        return Err("Choose a file path inside this repository, outside .git.".into());
    }
    Ok(())
}
pub fn matches(left: &str, right: &str) -> bool {
    if cfg!(windows) {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
    }
}
pub fn read(root: &Path) -> Result<Vec<String>> {
    let output = git(root, &["config", "--local", "--null", "--list"])?;
    Ok(output
        .split('\0')
        .filter_map(|entry| entry.split_once('\n'))
        .filter(|(key, _)| key.eq_ignore_ascii_case(KEY))
        .map(|(_, value)| value.to_owned())
        .collect())
}
pub fn is_locked(locks: &[String], path: &str) -> bool {
    locks.iter().any(|p| matches(p, path))
}

pub fn set(root: &Path, path: &str, locked: bool) -> Result<String> {
    crate::safety::ensure_writable(root)?;
    validate(path)?;
    let locks = read(root)?;
    if locked {
        if !is_locked(&locks, path) {
            git(root, &["config", "--local", "--add", KEY, path])?;
        }
        // Undo staging only. In particular, never restore the working copy here.
        let changed = git::parse_status(&git(
            root,
            &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
        )?);
        let mut paths = BTreeSet::new();
        for file in changed.iter().filter(|c| {
            matches(&c.path, path) || c.original.as_ref().is_some_and(|p| matches(p, path))
        }) {
            if file.index != ' ' && file.index != '?' {
                if file.conflict() {
                    return Err("The commit lock is saved. Resolve this file's conflict before unstaging it; commits remain blocked.".into());
                }
                paths.insert(file.path.clone());
                if let Some(original) = &file.original {
                    paths.insert(original.clone());
                }
            }
        }
        if !paths.is_empty() {
            let unborn = git::resolve(root, "HEAD").is_err();
            let mut args = if unborn {
                process::strings(&["rm", "--cached", "-f", "--ignore-unmatch", "--"])
            } else {
                process::strings(&["restore", "--staged", "--"])
            };
            args.extend(paths);
            process::run("git",root,&args).map_err(|e|format!("The commit lock is saved, but Git could not unstage the file. Commit is blocked until you unstage or unlock it.\n{e}"))?;
        }
        Ok(format!(
            "Excluded {path} from Branchroom commits. Local edits are kept."
        ))
    } else {
        for existing in locks.iter().filter(|p| matches(p, path)) {
            git(
                root,
                &[
                    "config",
                    "--local",
                    "--fixed-value",
                    "--unset-all",
                    KEY,
                    existing,
                ],
            )?;
        }
        Ok(format!(
            "Unlocked {path}. Select its changes to include them in a commit."
        ))
    }
}

pub fn stage_unlocked(root: &Path) -> Result<String> {
    crate::safety::ensure_writable(root)?;
    let locks = read(root)?;
    let changes = git::parse_status(&git(
        root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?);
    if changes.iter().any(git::Change::conflict) {
        return Err("Resolve conflicts before including all files.".into());
    }
    let mut paths = BTreeSet::new();
    for change in changes {
        if is_locked(&locks, &change.path)
            || change
                .original
                .as_ref()
                .is_some_and(|p| is_locked(&locks, p))
        {
            continue;
        }
        paths.insert(change.path);
    }
    if paths.is_empty() {
        return Ok("No unlocked changes to include.".into());
    }
    let mut input = Vec::new();
    for path in paths {
        input.extend(path.as_bytes());
        input.push(0);
    }
    process::input(
        "git",
        root,
        &process::strings(&["add", "--pathspec-from-file=-", "--pathspec-file-nul"]),
        &input,
        &[("GIT_LITERAL_PATHSPECS".into(), "1".into())],
    )
}

/// Check immediately before spawning commit, including commits from Command center.
pub fn guard_commit(root: &Path, args: &[String]) -> Result<()> {
    let locks = read(root)?;
    if locks.is_empty() {
        return Ok(());
    }
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-F"|"--file" if args.get(i+1).is_some_and(|value|value=="-") => {i+=1;},
            "-m"|"--message" => {i+=1; if i>=args.len(){return Err("Enter a commit message.".into());}},
            "--amend"|"--no-edit"|"--allow-empty"|"--allow-empty-message"|"--signoff"|"-s"|"--no-gpg-sign"|"-v"|"--verbose"|"-q"|"--quiet"=>{},
            arg if arg.starts_with("--message=")||arg.starts_with("--gpg-sign=")=>{},
            _=>return Err("File commit locks are active. Commit from Changes using explicit staging, or unlock files before using automatic/path-based commit options.".into()),
        }
        i += 1;
    }
    let staged = git(
        root,
        &["diff", "--cached", "--name-only", "--no-renames", "-z"],
    )?;
    let blocked: Vec<_> = staged
        .split('\0')
        .filter(|p| !p.is_empty() && is_locked(&locks, p))
        .collect();
    if blocked.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Commit blocked: excluded files are staged: {}. Unstage them or unlock them in Changes before committing.",
            blocked.join(", ")
        ))
    }
}

pub fn guard_add(root: &Path, args: &[String]) -> Result<()> {
    let locks = read(root)?;
    if locks.is_empty() {
        return Ok(());
    }
    if args.get(1).map(String::as_str) != Some("--") {
        return Err("File commit locks are active. Use Include unlocked files in Changes, or stage explicit unlocked files.".into());
    }
    for path in args.iter().skip(2) {
        validate(path)?;
        let candidate = if cfg!(windows) {
            path.replace('\\', "/").to_lowercase()
        } else {
            path.to_string()
        };
        let candidate = candidate.trim_end_matches('/');
        if locks.iter().any(|locked| {
            let locked = if cfg!(windows) {
                locked.to_lowercase()
            } else {
                locked.clone()
            };
            matches(&locked, candidate) || locked.starts_with(&format!("{candidate}/"))
        }) {
            return Err(format!(
                "{path} is excluded from commits or contains an excluded file. Unlock it before staging."
            ));
        }
    }
    Ok(())
}
