use crate::{git::git, process::Result, snapshot::unique_id};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub root: PathBuf,
    pub path: String,
    pub existed: bool,
    #[serde(default)]
    pub head: Option<String>,
}
pub fn directory() -> PathBuf {
    crate::settings::settings_path().with_file_name("discard-recovery")
}

pub fn checked_path(root: &Path, path: &str) -> Result<PathBuf> {
    use std::path::Component;
    if path.is_empty()
        || Path::new(path)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        || path
            .split(['/', '\\'])
            .any(|p| p.eq_ignore_ascii_case(".git"))
    {
        return Err("Choose a repository-relative working file, outside .git.".into());
    }
    let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    let target = root.join(path);
    let mut parent = target.parent();
    while let Some(p) = parent {
        if p.exists() {
            if !fs::canonicalize(p)
                .map_err(|e| e.to_string())?
                .starts_with(&root)
            {
                return Err("This file path points outside the repository.".into());
            }
            break;
        }
        parent = p.parent();
    }
    if let Ok(meta) = fs::symlink_metadata(&target)
        && meta.file_type().is_symlink()
    {
        return Err("Resolve symlink changes with Git directly; the recovery tool does not follow symlinks.".into());
    }
    Ok(target)
}

pub fn discard(root: &Path, path: &str) -> Result<String> {
    crate::safety::ensure_writable(root)?;
    let target = checked_path(root, path)?;
    if target.is_dir() {
        return Err("Select a file rather than a directory.".into());
    }
    let entry = backup(root, path)?;
    let id = &entry.id;
    let existed = entry.existed;
    let status =
        crate::git::parse_status(&git(root, &["status", "--porcelain=v1", "-z", "--", path])?);
    if status.first().is_some_and(|s| s.index == '?') {
        if existed {
            fs::remove_file(target).map_err(|e| e.to_string())?;
        }
    } else {
        git(root, &["restore", "--worktree", "--", path])?;
    }
    Ok(format!(
        "Discarded working changes to {path}. Recovery copy: {id}"
    ))
}

pub(crate) fn backup(root: &Path, path: &str) -> Result<Entry> {
    let target = checked_path(root, path)?;
    if target.is_dir() {
        return Err("Choose a working file, not a directory.".into());
    }
    let id = unique_id();
    let folder = directory().join(&id);
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let existed = target.exists();
    if existed {
        fs::copy(&target, folder.join("content")).map_err(|e| e.to_string())?;
    }
    let entry = Entry {
        id: id.clone(),
        root: fs::canonicalize(root).map_err(|e| e.to_string())?,
        path: path.into(),
        existed,
        head: crate::git::resolve(root, "HEAD").ok(),
    };
    crate::storage::write_atomic(
        &folder.join("entry.json"),
        &serde_json::to_vec_pretty(&entry).map_err(|e| e.to_string())?,
    )?;
    Ok(entry)
}

pub fn entries(root: &Path) -> Vec<Entry> {
    let Ok(root) = fs::canonicalize(root) else {
        return vec![];
    };
    let mut result: Vec<_> = fs::read_dir(directory())
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter_map(|e| fs::read(e.path().join("entry.json")).ok())
        .filter_map(|b| serde_json::from_slice::<Entry>(&b).ok())
        .filter(|e| {
            e.root == root
                && !e.id.is_empty()
                && e.id.len() <= 128
                && e.id.bytes().all(|b| b.is_ascii_alphanumeric())
        })
        .collect();
    result.sort_by(|a, b| b.id.cmp(&a.id));
    result
}

pub fn restore(root: &Path, id: &str) -> Result<String> {
    crate::safety::ensure_writable(root)?;
    let entry = entries(root)
        .into_iter()
        .find(|e| e.id == id)
        .ok_or("This recovery entry does not belong to the current repository.")?;
    let target = checked_path(root, &entry.path)?;
    let replacement = backup(root, &entry.path)?;
    if entry.existed {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        crate::storage::copy_atomic(&directory().join(&entry.id).join("content"), &target)?;
    } else if target.exists() {
        fs::remove_file(target).map_err(|e| e.to_string())?;
    }
    Ok(format!(
        "Restored {} to the working directory. The previous working version is saved as recovery {}. The index was not changed.",
        entry.path, replacement.id
    ))
}

pub fn ignore(root: &Path, path: &str) -> Result<String> {
    crate::safety::ensure_writable(root)?;
    checked_path(root, path)?;
    if path.contains(['\n', '\r']) {
        return Err(
            "Ignore patterns for names containing newlines must be edited manually.".into(),
        );
    }
    let mut pattern = String::from("/");
    for ch in path.chars() {
        if matches!(ch, '#' | '!' | '*' | '?' | '[' | ']' | ' ' | '\\') {
            pattern.push('\\');
        }
        pattern.push(ch);
    }
    let ignore = root.join(".gitignore");
    if fs::symlink_metadata(&ignore).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("Edit this symlinked .gitignore manually.".into());
    }
    let mut text = if ignore.exists() {
        fs::read_to_string(&ignore).map_err(|e| e.to_string())?
    } else {
        String::new()
    };
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    if !text.lines().any(|l| l == pattern) {
        text.push_str(&pattern);
        text.push('\n');
        crate::storage::write_atomic(&ignore, text.as_bytes())?;
    }
    Ok(format!(
        "Added {pattern} to .gitignore. Already tracked files remain tracked."
    ))
}

pub fn preview(root: &Path, id: &str) -> Result<String> {
    let entry = entries(root)
        .into_iter()
        .find(|e| e.id == id)
        .ok_or("The saved file is no longer available")?;
    if !entry.existed {
        return Ok("The file did not exist at this point".into());
    }
    let bytes = fs::read(directory().join(&entry.id).join("content")).map_err(|e| e.to_string())?;
    if bytes.len() > 1_000_000 {
        return Ok("This saved file is too large for an inline preview. Recover it to inspect the complete content".into());
    }
    Ok(String::from_utf8(bytes).unwrap_or_else(|_| {
        "This saved file contains binary data. Recover it to inspect the content".into()
    }))
}
