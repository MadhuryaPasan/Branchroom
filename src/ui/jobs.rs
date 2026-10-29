use super::*;

impl Branchroom {
    pub(super) fn execute(&mut self, task: Task, ctx: &egui::Context) {
        if let Some(kind) = self.reviewed_graph_kind(&task) {
            self.begin_graph_action(kind);
            return;
        }
        let read_only = match &task {
            Task::RawGit(args) => branchroom::safety::read_only_git(args),
            Task::Command { program, args, .. } => {
                if program == "git" {
                    branchroom::safety::read_only_git(args)
                } else if program == "gh" {
                    branchroom::safety::read_only_gh(args)
                } else {
                    false
                }
            }
            Task::TimelineLoad { .. }
            | Task::TimelinePlan { .. }
            | Task::ImagePreview { .. }
            | Task::FilePatch { .. }
            | Task::CommitFiles(_)
            | Task::ConflictSides(_)
            | Task::PullRequestDetails(_)
            | Task::PullRequests(_)
            | Task::PullRequestPreview { .. }
            | Task::CommitPullRequests(_) => true,
            _ => false,
        };
        if self.manipulation.active() && !read_only && !matches!(&task, Task::GraphApply(_)) {
            self.cancel_graph_action(ctx);
        }
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
        if matches!(&task, Task::Command { program, args, .. } if program == "git" && args.first().is_some_and(|a| a == "fetch"))
        {
            self.fetching = true;
            self.fetch_attempt = Some(Instant::now());
        }
        let path = self.path();
        let base = self.base_input.clone();
        let limit = self.history_limit;
        let ctx = ctx.clone();
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.job_started = Some(Instant::now());
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
            Task::PullRequests(_) => "Loading pull requests…",
            Task::PullRequestPreview { .. } => "Previewing pull request…",
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
            let needs_safety = matches!(&task, Task::History(_) | Task::Revert { .. })
                || matches!(&task,Task::RawGit(args) if !read_only&&args.iter().any(|a|["merge","cherry-pick","reset","rebase","revert","commit","clean","rm","restore","apply","am","checkout","switch","stash","branch","worktree"].contains(&a.as_str())))
                || matches!(&task,Task::Command{program,args,..} if program=="git"&&!read_only&&args.iter().any(|a|["merge","cherry-pick","reset","rebase","revert","commit","clean","rm","restore","apply","am","checkout","switch","stash","branch","worktree"].contains(&a.as_str())));
            if needs_safety {
                let args = match &task {
                    Task::Command { args, .. } | Task::RawGit(args) => args.as_slice(),
                    _ => &[],
                };
                if let Err(error) = branchroom::operation::advanced_safety(&path, args) {
                    let _ = tx.send(Completed {
                        output: Err(format!(
                            "Could not save a safety point. The action was not run. {error}"
                        )),
                        state: None,
                        detail: false,
                        open: None,
                        data: JobData::None,
                    });
                    ctx.request_repaint();
                    process::reset_cancellation();
                    return;
                }
            }
            let journal = if !read_only
                && !matches!(
                    &task,
                    Task::TimelineUndo { .. }
                        | Task::TimelineHide { .. }
                        | Task::Clone { .. }
                        | Task::Init(_)
                        | Task::PrepareCommit { .. }
                        | Task::UpdateBranch { .. }
                        | Task::CreatePullRequest(_)
                        | Task::OpenPr { .. }
                ) {
                let label = Self::timeline_label(&task);
                match branchroom::timeline::begin(&path, &label) {
                    Ok(entry) => Some(entry),
                    Err(error) => branchroom::timeline::unavailable(&path, &label, &error).ok(),
                }
            } else {
                None
            };
            if matches!(&task, Task::Shelved { .. } | Task::TimelineRestore(_))
                && journal.as_ref().is_none_or(|entry| entry.before.is_none())
            {
                process::reset_cancellation();
                let _=tx.send(Completed{output:Err("A complete safety point could not be saved. Finish active conflicts or Git operations before changing this saved work".into()),state:Some(git::load(&path,&base,limit)),detail:false,open:None,data:JobData::None});
                ctx.request_repaint();
                return;
            }
            let mut open = None;
            let mut data = JobData::None;
            let mut refresh_path = path.clone();
            let (mut output, refresh, detail) = match task {
                Task::TimelineLoad { item, before } => {
                    let saved_state = if !item.action.is_empty() {
                        branchroom::timeline::entries(&path)
                            .ok()
                            .and_then(|entries| entries.into_iter().find(|e| e.id == item.action))
                            .and_then(|e| {
                                if before {
                                    e.before
                                } else {
                                    e.after.or(e.before)
                                }
                            })
                    } else {
                        None
                    };
                    let target = saved_state
                        .as_ref()
                        .map(|s| s.working.clone())
                        .unwrap_or_else(|| item.target.clone());
                    let history_target = saved_state
                        .as_ref()
                        .map(|s| s.head.clone().unwrap_or_default())
                        .unwrap_or_else(|| item.target.clone());
                    let graph = if let Some(state) = &saved_state {
                        branchroom::timeline::state_graph(&path, state)
                    } else if history_target.is_empty() {
                        Ok(vec![])
                    } else {
                        branchroom::timeline::graph(&path, &history_target)
                    };
                    let patch = if item.kind == "Discarded file" {
                        branchroom::recovery::preview(&path, &item.reference)
                    } else if item.kind == "Shelved work" {
                        git::git(
                            &path,
                            &[
                                "stash",
                                "show",
                                "--include-untracked",
                                "--patch",
                                &item.target,
                            ],
                        )
                    } else if target.is_empty() {
                        Ok(String::new())
                    } else {
                        git::git(&path, &["show", "--stat", "--patch", "--format=", &target])
                    };
                    let result = graph.and_then(|graph| {
                        patch.map(|patch| {
                            data = JobData::TimelineDetail {
                                root: path.clone(),
                                key: item.key,
                                target,
                                graph,
                                patch,
                            };
                            String::new()
                        })
                    });
                    (result, false, false)
                }
                Task::TimelinePlan { item, before } => {
                    let result =
                        branchroom::timeline::plan_restore(&path, &item, before).map(|plan| {
                            data = JobData::TimelinePlan {
                                root: path.clone(),
                                plan: Box::new(plan),
                            };
                            String::new()
                        });
                    (result, false, false)
                }
                Task::TimelineRestore(plan) => (
                    branchroom::timeline::apply_restore(&path, &plan),
                    true,
                    false,
                ),
                Task::TimelineUndo { redo, id } => (
                    branchroom::timeline::undo_redo(&path, redo, &id),
                    true,
                    false,
                ),
                Task::TimelineHide { id, hidden } => {
                    let result = branchroom::timeline::set_visible(&path, &id, !hidden);
                    (
                        result.map(|_| "Updated timeline visibility".into()),
                        true,
                        false,
                    )
                }
                Task::Shelved {
                    reference,
                    id,
                    action,
                } => {
                    let result = git::resolve(&path, &reference).and_then(|current| {
                        if current != id {
                            return Err("The shelved work list changed. Select it again".into());
                        }
                        git::git(&path, &["stash", &action, &reference])
                    });
                    (result, true, false)
                }

                Task::RawGit(args) => (process::run("git", &path, &args), true, false),
                Task::GraphApply(review) => {
                    let result = branchroom::operation::apply(&path, &review);
                    if result.is_ok()
                        && matches!(
                            review.kind,
                            branchroom::operation::Kind::Commit { .. }
                                | branchroom::operation::Kind::Amend { .. }
                        )
                    {
                        data = JobData::Committed;
                    }
                    (result, true, false)
                }
                Task::ConflictSides(file) => {
                    let versions = std::array::from_fn(|index| {
                        git::git(&path, &["show", &format!(":{}:{file}", index + 1)])
                            .unwrap_or_else(|_| "This version does not contain the file".into())
                    });
                    data = JobData::ConflictSides {
                        root: path.clone(),
                        path: file,
                        versions,
                    };
                    (Ok(String::new()), false, false)
                }
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
                Task::PullRequestPreview { head, base } => {
                    let result = (|| {
                        let (_, head_id) = git::resolve_named_commit(&path, &head)?;
                        let (_, base_id) = git::resolve_named_commit(&path, &base)?;
                        let range = format!("{base_id}..{head_id}");
                        let commits = git::git(
                            &path,
                            &["log", "--format=%h%x00%s", "-z", "--max-count=8", &range],
                        )?
                        .split_terminator('\0')
                        .collect::<Vec<_>>()
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|pair| format!("{} · {}", pair[0].trim_start_matches('\n'), pair[1]))
                        .collect();
                        let comparison = format!("{base_id}...{head_id}");
                        let files = git::git(&path, &["diff", "--name-status", &comparison])?
                            .lines()
                            .take(100)
                            .map(str::to_owned)
                            .collect();
                        data = JobData::PullRequestPreview {
                            key: format!("{head}\u{0}{base}"),
                            commits,
                            files,
                        };
                        Ok(String::new())
                    })();
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
                Task::PullRequests(filter) => {
                    let result =
                        branchroom::collaboration::pull_requests(&path, filter).map(|prs| {
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
                    branchroom::operation::preview_update(&path, &branch, &baseline).map(
                        |review| {
                            data = JobData::GraphReview(Box::new(review));
                            String::new()
                        },
                    ),
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
                Task::DeleteMerged { branch, expected } => (
                    branchroom::collaboration::delete_merged_branch(&path, &branch, &expected),
                    true,
                    false,
                ),
                Task::CommitFiles(id) => {
                    let result = super::inspector::files(&path, &id).map(|files| {
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
                    snapshot::remove(&path, &reference).map(|_| {
                        "Safety point deleted from view. Restore it from its timeline entry.".into()
                    }),
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
            if let Some(entry) = journal
                && let Err(error) = branchroom::timeline::finish(&path, entry, output.is_ok())
            {
                output = Err(format!(
                    "{}\nThe action history could not be updated: {error}. Existing safety points were retained",
                    output.unwrap_or_else(|e| e)
                ));
            }
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
