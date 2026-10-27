use crate::process::Result;
use std::path::{Path, PathBuf};

pub fn protection_file() -> PathBuf {
    crate::settings::settings_path().with_file_name("protected-repositories.json")
}

pub fn protected_roots() -> Vec<PathBuf> {
    std::fs::read(protection_file())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn normalized(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/")
        .trim_start_matches("//?/")
        .trim_end_matches('/')
        .to_lowercase()
}

pub fn is_protected(path: &Path) -> bool {
    let candidate = normalized(path);
    protected_roots().iter().any(|root| {
        let root = normalized(root);
        candidate == root || candidate.starts_with(&format!("{root}/"))
    })
}

pub fn ensure_writable(path: &Path) -> Result<()> {
    if protection_file().exists()
        && std::fs::read(protection_file())
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Vec<PathBuf>>(&bytes).ok())
            .is_none()
    {
        return Err("Repository protection settings could not be read. Repair protected-repositories.json before running an operation that writes.".into());
    }
    if is_protected(path) {
        Err("This repository is protected for read-only inspection. Changes, network sync, IDE launches, and repository commands that can write are blocked. Use a different test repository.".into())
    } else {
        Ok(())
    }
}

pub fn read_only_git(args: &[String]) -> bool {
    if args.iter().any(|a| {
        matches!(
            a.as_str(),
            "--delete"
                | "-d"
                | "-D"
                | "--move"
                | "--copy"
                | "--force"
                | "-f"
                | "--edit"
                | "-e"
                | "--create-reflog"
                | "--rewrite"
                | "--updateref"
        )
    }) {
        return false;
    }
    if args.iter().any(|arg| {
        arg == "--output"
            || arg.starts_with("--output=")
            || arg == "--ext-diff"
            || arg == "--textconv"
            || arg.starts_with("--open-files-in-pager")
            || arg == "--exec"
            || arg.starts_with("--exec=")
    }) {
        return false;
    }
    let Some(command) = args.first().map(String::as_str) else {
        return false;
    };
    let sub = args.get(1).map(String::as_str);
    match command {
        "status" | "log" | "show" | "diff" | "diff-tree" | "ls-files" | "ls-tree" | "rev-parse"
        | "rev-list" | "merge-base" | "for-each-ref" | "check-ref-format" | "count-objects"
        | "cat-file" | "name-rev" | "describe" | "version" | "help" | "blame" | "grep"
        | "shortlog" => true,
        "stash" => matches!(sub, Some("list" | "show")),
        "worktree" => sub == Some("list"),
        "remote" => {
            args.len() == 1 || args.get(1..) == Some(&["-v".into()][..]) || sub == Some("get-url")
        }
        "branch" => {
            args.len() == 1
                || args.get(1..) == Some(&["--show-current".into()][..])
                || sub == Some("--list")
        }
        "tag" => args.len() == 1 || sub == Some("--list"),
        "reflog" => {
            args.len() == 1
                || sub == Some("show")
                || args
                    .iter()
                    .skip(1)
                    .all(|a| a.starts_with("--date=") || a == "-100")
        }
        "config" => {
            args.iter().any(|a| {
                matches!(
                    a.as_str(),
                    "--list" | "--get" | "--get-all" | "--get-regexp"
                )
            }) && !args.iter().any(|a| {
                matches!(
                    a.as_str(),
                    "--edit"
                        | "-e"
                        | "--unset"
                        | "--unset-all"
                        | "--add"
                        | "--replace-all"
                        | "--remove-section"
                        | "--rename-section"
                )
            })
        }
        _ => false,
    }
}

pub fn read_only_gh(args: &[String]) -> bool {
    matches!(
        (
            args.first().map(String::as_str),
            args.get(1).map(String::as_str)
        ),
        (Some("auth"), Some("status"))
            | (
                Some("pr"),
                Some("list" | "view" | "diff" | "checks" | "status")
            )
            | (Some("issue"), Some("list" | "view"))
            | (Some("run"), Some("list" | "view"))
            | (Some("repo"), Some("view" | "list"))
    )
}
