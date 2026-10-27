//! GitHub operations remain explicit, reviewable requests through the signed-in CLI.
use crate::{
    git,
    process::{self, Result},
};
use serde::Deserialize;
use std::path::Path;
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestDetails {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub url: String,
    pub state: String,
    pub is_draft: bool,
    pub head_ref_name: String,
    pub base_ref_name: String,
    pub head_ref_oid: String,
    pub merge_state_status: String,
    pub review_decision: String,
    pub additions: usize,
    pub deletions: usize,
    pub changed_files: usize,
}
pub fn details(root: &Path, number: u64) -> Result<PullRequestDetails> {
    let text = process::stdout(
        "gh",
        root,
        &process::strings(&[
            "pr",
            "view",
            &number.to_string(),
            "--json",
            "number,title,body,url,state,isDraft,headRefName,baseRefName,headRefOid,mergeStateStatus,reviewDecision,additions,deletions,changedFiles",
        ]),
    )?;
    serde_json::from_str(&text)
        .map_err(|e| format!("Cannot read pull request details: {e}. Reload the pull request."))
}
#[derive(Clone, Debug)]
pub struct CreateRequest {
    pub base: String,
    pub head: String,
    pub head_oid: String,
    pub title: String,
    pub body: String,
    pub draft: bool,
    pub reviewers: Vec<String>,
    pub labels: Vec<String>,
}
impl CreateRequest {
    pub fn arguments(&self) -> Result<Vec<String>> {
        if self.title.trim().is_empty() || self.head == self.base {
            return Err("Enter a title and choose different base and compare branches.".into());
        }
        let mut args = process::strings(&[
            "pr",
            "create",
            "--base",
            &self.base,
            "--head",
            &self.head,
            "--title",
            &self.title,
            "--body-file",
            "-",
        ]);
        if self.draft {
            args.push("--draft".into());
        }
        for reviewer in &self.reviewers {
            if reviewer.trim().is_empty() || reviewer.starts_with('-') {
                return Err("Enter reviewer logins separated by commas.".into());
            }
            args.extend(["--reviewer".into(), reviewer.clone()]);
        }
        for label in &self.labels {
            if label.trim().is_empty() || label.starts_with('-') {
                return Err("Enter label names separated by commas.".into());
            }
            args.extend(["--label".into(), label.clone()]);
        }
        Ok(args)
    }
}
pub fn create(root: &Path, request: &CreateRequest) -> Result<String> {
    crate::safety::ensure_writable(root)?;
    if git::resolve(root, &format!("refs/heads/{}", request.head))? != request.head_oid {
        return Err(
            "The compare branch changed after review. Preview the pull request again.".into(),
        );
    }
    let upstream = git::git(
        root,
        &[
            "for-each-ref",
            "--format=%(upstream)%09%(upstream:track)",
            &format!("refs/heads/{}", request.head),
        ],
    )?;
    let fields: Vec<_> = upstream.trim_end_matches('\n').split('\t').collect();
    if fields
        .first()
        .is_none_or(|s| !s.starts_with("refs/remotes/"))
        || fields.get(1).is_some_and(|s| !s.is_empty())
    {
        return Err("Publish or sync the compare branch and fetch to verify its state before creating the pull request.".into());
    }
    process::input(
        "gh",
        root,
        &request.arguments()?,
        request.body.as_bytes(),
        &[],
    )
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Review {
    Comment,
    Approve,
    RequestChanges,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MergeMethod {
    Merge,
    Squash,
    Rebase,
}
#[derive(Clone, Debug)]
pub enum Action {
    Review {
        number: u64,
        kind: Review,
        body: String,
    },
    Merge {
        number: u64,
        head: String,
        method: MergeMethod,
    },
    Ready(u64),
    Close(u64),
    Reopen(u64),
}
impl Action {
    pub fn arguments(&self) -> Result<Vec<String>> {
        Ok(match self {
            Self::Review { number, kind, body } => {
                if body.trim().is_empty() && *kind != Review::Approve {
                    return Err("Write review feedback before submitting.".into());
                }
                process::strings(&[
                    "pr",
                    "review",
                    &number.to_string(),
                    match kind {
                        Review::Comment => "--comment",
                        Review::Approve => "--approve",
                        Review::RequestChanges => "--request-changes",
                    },
                    "--body-file",
                    "-",
                ])
            }
            Self::Merge {
                number,
                head,
                method,
            } => {
                if head.len() != 40 && head.len() != 64
                    || !head.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    return Err(
                        "Reload this pull request to verify its current head commit.".into(),
                    );
                }
                process::strings(&[
                    "pr",
                    "merge",
                    &number.to_string(),
                    match method {
                        MergeMethod::Merge => "--merge",
                        MergeMethod::Squash => "--squash",
                        MergeMethod::Rebase => "--rebase",
                    },
                    "--match-head-commit",
                    head,
                ])
            }
            Self::Ready(n) => process::strings(&["pr", "ready", &n.to_string()]),
            Self::Close(n) => process::strings(&["pr", "close", &n.to_string()]),
            Self::Reopen(n) => process::strings(&["pr", "reopen", &n.to_string()]),
        })
    }
    pub fn run(&self, root: &Path) -> Result<String> {
        crate::safety::ensure_writable(root)?;
        if let Self::Review { body, .. } = self {
            process::input("gh", root, &self.arguments()?, body.as_bytes(), &[])
        } else {
            process::run("gh", root, &self.arguments()?)
        }
    }
}
pub fn templates(root: &Path) -> Vec<std::path::PathBuf> {
    let mut paths: Vec<_> = [
        ".github/pull_request_template.md",
        ".github/PULL_REQUEST_TEMPLATE.md",
        "pull_request_template.md",
        "PULL_REQUEST_TEMPLATE.md",
        "docs/pull_request_template.md",
    ]
    .into_iter()
    .map(|p| root.join(p))
    .filter(|p| p.is_file())
    .collect();
    if let Ok(entries) = std::fs::read_dir(root.join(".github/PULL_REQUEST_TEMPLATE")) {
        paths.extend(
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("md"))
                }),
        );
    }
    paths.sort();
    paths
}
pub fn load_template(root: &Path, path: &Path) -> Result<String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "Choose a template from this repository.")?;
    let safe = crate::recovery::checked_path(root, &relative.to_string_lossy())?;
    if std::fs::metadata(&safe).map_err(|e| e.to_string())?.len() > 128 * 1024 {
        return Err("This template exceeds 128 KiB. Open it in your editor.".into());
    }
    std::fs::read_to_string(safe).map_err(|e| e.to_string())
}
