use super::*;

impl Branchroom {
    pub(super) fn execute(&mut self, task: Task, ctx: &egui::Context) {
        let read_only = match &task {
            Task::Command { program, args, .. } => {
                if program == "git" {
                    branchroom::safety::read_only_git(args)
                } else if program == "gh" {
                    branchroom::safety::read_only_gh(args)
                } else {
                    false
                }
            }
            Task::ImagePreview { .. }
            | Task::FilePatch { .. }
            | Task::CommitFiles(_)
            | Task::PullRequestDetails(_)
            | Task::PullRequests
            | Task::CommitPullRequests(_) => true,
            _ => false,
        };
        let mutation_root = match &task {
            Task::Clone { destination, .. } | Task::Init(destination) => {
                if destination.is_absolute() {
                    destination.clone()
                } else {
                    self.path().join(destination)
                }
            }
            _ => self.path(),
        };
        if !read_only && let Err(error) = branchroom::safety::ensure_writable(&mutation_root) {
            self.report(Err(error));
            return;
        }
        if self.preview_mode {
            self.report(Err("Repository commands are disabled in UI preview. Restart without --ui-preview to use Git.".into()));
            return;
        }
        if self.page == Page::Tools {
            self.activity_open = true;
        }
        if self.receiver.is_some() {
            self.report(Err(
                "An operation is still running. Wait for Ready in the status bar, then retry."
                    .into(),
            ));
            return;
        }
        let reviewed_head = if !read_only && !matches!(task, Task::Clone { .. } | Task::Init(_)) {
            self.repo.as_ref().map(|r| r.head.clone())
        } else {
            None
        };
        let path = self.path();
        let base = self.base_input.clone();
        let limit = self.history_limit;
        let ctx = ctx.clone();
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.busy = match &task {
            Task::Command { args, .. } if args.first().is_some_and(|a| a == "push") => {
                "Uploading commits…"
            }
            Task::Command { args, .. }
                if args
                    .first()
                    .is_some_and(|a| matches!(a.as_str(), "fetch" | "pull")) =>
            {
                "Downloading remote updates…"
            }
            Task::PullRequests => "Loading pull requests…",
            Task::OpenPr { .. } => "Downloading pull request for IDE review…",
            Task::UpdateBranch { .. } => "Fetching baseline and updating branch…",
            Task::Revert { .. } => "Creating revert commit…",
            _ => "Running operation…",
        }
        .into();
        let token = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.cancellation = Some(token.clone());
        if let Ok(mut progress) = self.transfer_progress.lock() {
            progress.clear();
        }
        let progress = self.transfer_progress.clone();
        std::thread::spawn(move || {
            process::set_cancellation(token);
            process::set_progress(progress);
            if let Some(expected) = reviewed_head
                && git::resolve(&path, "HEAD").unwrap_or_default() != expected
            {
                process::reset_cancellation();
                let _=tx.send(Completed{output:Err("The branch changed after this action was reviewed. Refresh and review it again.".into()),state:Some(git::load(&path,&base,limit)),detail:false,open:None,data:JobData::None});
                ctx.request_repaint();
                return;
            }
            let mut open = None;
            let mut data = JobData::None;
            let mut refresh_path = path.clone();
            let (output, refresh, detail) = match task {
                Task::CommitPullRequests(id) => {
                    let result =
                        branchroom::collaboration::commit_pull_requests(&path, &id).map(|prs| {
                            data = JobData::PullRequests(prs);
                            "Related pull requests loaded. Reload to see all open requests.".into()
                        });
                    (result, false, false)
                }
                Task::PullRequestDetails(number) => {
                    let result = branchroom::github::details(&path, number).map(|details| {
                        data = JobData::PullRequestDetails(details);
                        String::new()
                    });
                    (result, false, false)
                }
                Task::CreatePullRequest(request) => {
                    let result = branchroom::github::create(&path, &request);
                    if result.is_ok() {
                        data = JobData::PullRequestCreated;
                    }
                    (result, false, false)
                }
                Task::GithubAction(action) => {
                    let mut result = action.run(&path);
                    if let Ok(output) = &result {
                        let number = match &action {
                            branchroom::github::Action::Review { number, .. }
                            | branchroom::github::Action::Merge { number, .. } => *number,
                            branchroom::github::Action::Ready(n)
                            | branchroom::github::Action::Close(n)
                            | branchroom::github::Action::Reopen(n) => *n,
                        };
                        let review = matches!(action, branchroom::github::Action::Review { .. });
                        match branchroom::github::details(&path, number) {
                            Ok(details) => {
                                data = JobData::GithubActionDone {
                                    details: Some(details),
                                    review,
                                }
                            }
                            Err(e) => {
                                data = JobData::GithubActionDone {
                                    details: None,
                                    review,
                                };
                                result = Ok(format!(
                                    "{output}\nThe action completed, but details could not be refreshed. Reload the pull request.\n{e}"
                                ));
                            }
                        }
                    }
                    (result, false, false)
                }
                Task::PullRequests => {
                    let result = branchroom::collaboration::pull_requests(&path).map(|prs| {
                        data = JobData::PullRequests(prs);
                        "Pull requests loaded.".into()
                    });
                    (result, false, false)
                }
                Task::Revert {
                    reference,
                    mainline,
                } => (
                    branchroom::collaboration::revert(&path, &reference, mainline),
                    true,
                    false,
                ),
                Task::UpdateBranch { branch, baseline } => (
                    branchroom::collaboration::update_branch(&path, &branch, &baseline),
                    true,
                    false,
                ),
                Task::OpenPr {
                    number,
                    destination,
                    ide,
                } => {
                    let result =
                        branchroom::collaboration::open_pull_request(&path, number, &destination);
                    if result.is_ok() {
                        open = Some((destination, ide));
                    }
                    (result, true, false)
                }
                Task::ImagePreview { path: file, staged } => {
                    let result =
                        branchroom::image_diff::compare(&path, &file, staged).map(|comparison| {
                            data = JobData::Image {
                                path: file,
                                staged,
                                comparison,
                            };
                            String::new()
                        });
                    (result, false, false)
                }
                Task::FilePatch {
                    path: file,
                    staged,
                    whitespace,
                    full,
                } => {
                    let result = branchroom::diff::file_patch(
                        &path, &file, staged, whitespace, full,
                    )
                    .map(|patch| {
                        data = JobData::Patch {
                            path: file,
                            staged,
                            patch,
                        };
                        String::new()
                    });
                    (result, false, false)
                }
                Task::ApplyPatch(patch) => (
                    branchroom::diff::apply_to_index(&path, &patch, false),
                    true,
                    false,
                ),
                Task::History(action) => (branchroom::history::run(&path, &action), true, false),
                Task::Discard(file) => (branchroom::recovery::discard(&path, &file), true, false),
                Task::RestoreDiscard(id) => {
                    (branchroom::recovery::restore(&path, &id), true, false)
                }
                Task::Ignore(file) => (branchroom::recovery::ignore(&path, &file), true, false),
                Task::SwitchBranch { branch, stash } => (
                    branchroom::collaboration::switch_branch(&path, &branch, stash),
                    true,
                    false,
                ),
                Task::CommitFiles(id) => {
                    let result = git::git(
                        &path,
                        &[
                            "diff-tree",
                            "--root",
                            "--no-commit-id",
                            "--name-only",
                            "--first-parent",
                            "-r",
                            "-z",
                            &id,
                        ],
                    )
                    .map(|output| {
                        let files = output
                            .split('\0')
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned)
                            .collect();
                        data = JobData::Files { commit: id, files };
                        String::new()
                    });
                    (result, false, false)
                }
                Task::Command {
                    program,
                    args,
                    refresh,
                } => {
                    let result = process::run(&program, &path, &args);
                    if result.is_ok()
                        && program == "git"
                        && args.first().is_some_and(|a| a == "commit")
                    {
                        data = JobData::Committed;
                    }
                    (result, refresh, !refresh)
                }
                Task::RemoveSnapshot(reference) => (
                    snapshot::remove(&path, &reference)
                        .map(|_| "Snapshot removed. Restore it from Removed snapshots.".into()),
                    true,
                    false,
                ),
                Task::RestoreSnapshot(reference) => {
                    let result = snapshot::restore(&path, &reference).map(|restored| {
                        data = JobData::SnapshotRestored(restored);
                        "Snapshot restored.".into()
                    });
                    (result, true, false)
                }
                Task::Merge { target, reference } => (
                    branchroom::collaboration::merge(&path, &target, &reference),
                    true,
                    false,
                ),
                Task::SetCommitLock { path: file, locked } => (
                    branchroom::commit_lock::set(&path, &file, locked),
                    true,
                    false,
                ),
                Task::PrepareCommit { message, amend } => {
                    let result = branchroom::commit::prepare(&path, &message, amend).map(|plan| {
                        data = JobData::CommitReview(plan);
                        String::new()
                    });
                    (result, false, false)
                }
                Task::Commit(plan) => {
                    let result = branchroom::commit::execute(&path, &plan);
                    if result.is_ok() {
                        data = JobData::Committed;
                    }
                    (result, true, false)
                }
                Task::StageUnlocked => {
                    (branchroom::commit_lock::stage_unlocked(&path), true, false)
                }
                Task::Snapshot(label) => (snapshot::create(&path, &label), true, false),
                Task::Promote { reference, branch } => {
                    (snapshot::promote(&path, &reference, &branch), true, false)
                }
                Task::Worktree {
                    reference,
                    destination,
                    ide,
                } => {
                    let destination = if destination.is_absolute() {
                        destination
                    } else {
                        path.join(destination)
                    };
                    let result = snapshot::materialize(&path, &reference, &destination);
                    if result.is_ok() {
                        open = Some((destination, ide));
                    }
                    (result, true, false)
                }
                Task::Resolve {
                    path: file,
                    content,
                    original,
                } => (
                    branchroom::conflict::save(&path, &file, &content, &original),
                    true,
                    false,
                ),

                Task::Clone { url, destination } => {
                    let destination = if destination.is_absolute() {
                        destination
                    } else {
                        path.join(destination)
                    };
                    let result = git::git(
                        destination.parent().unwrap_or(&path),
                        &["clone", "--", &url, &destination.to_string_lossy()],
                    );
                    if result.is_ok() {
                        refresh_path = destination;
                    }
                    let refresh = result.is_ok();
                    (result, refresh, false)
                }
                Task::Init(destination) => {
                    let destination = if destination.is_absolute() {
                        destination
                    } else {
                        path.join(destination)
                    };
                    let result = git::git(
                        destination.parent().unwrap_or(&path),
                        &[
                            "init",
                            "--initial-branch=main",
                            "--",
                            &destination.to_string_lossy(),
                        ],
                    );
                    if result.is_ok() {
                        refresh_path = destination;
                    }
                    let refresh = result.is_ok();
                    (result, refresh, false)
                }
            };
            // Failed merges still change repository state and can leave an operation to resolve.
            process::reset_cancellation();
            let state = refresh.then(|| git::load(&refresh_path, &base, limit));
            let _ = tx.send(Completed {
                output,
                state,
                detail,
                open,
                data,
            });
            ctx.request_repaint();
        });
    }
    pub(super) fn read(&mut self, program: &str, args: Vec<String>, ctx: &egui::Context) {
        if self.receiver.is_some() {
            self.report(Err(
                "Wait for the current operation to finish, then open this preview again.".into(),
            ));
            return;
        }
        self.detail_open = true;
        self.detail_page = self.page;
        self.detail = "Loading…".into();
        self.execute(
            Task::Command {
                program: program.into(),
                args,
                refresh: false,
            },
            ctx,
        );
    }
    pub(super) fn propose(&mut self, title: &str, explanation: &str, args: &[&str]) {
        self.pending = Some(Pending {
            title: title.into(),
            explanation: explanation.into(),
            task: Task::Command {
                program: "git".into(),
                args: process::strings(args),
                refresh: true,
            },
        });
    }
    pub(super) fn form(
        &mut self,
        title: &str,
        description: &str,
        program: &str,
        prefix: &[&str],
        fields: &[(&str, &str)],
        suffix: &[&str],
    ) {
        self.form = Some(Form {
            title: title.into(),
            description: description.into(),
            program: program.into(),
            prefix: process::strings(prefix),
            fields: fields
                .iter()
                .map(|(l, v)| Field {
                    label: l.to_string(),
                    value: v.to_string(),
                })
                .collect(),
            suffix: process::strings(suffix),
            error: String::new(),
        });
    }
}
