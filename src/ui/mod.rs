mod changes;
mod collaboration;
mod compare;
mod dialogs;
mod drafts;
mod experiments;
mod history;
mod images;
mod jobs;
mod merge;
mod patch_preview;
#[cfg(feature = "ui-preview")]
mod preview;
mod pull_request;
#[cfg(all(test, feature = "ui-preview"))]
mod qa;
mod shell;
mod theme;
mod views;
mod workspace;
use branchroom::{
    git::{self, RepoState},
    graph,
    process::{self, Result},
    settings::Settings,
    snapshot,
};
use eframe::egui::{self, Color32, RichText};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};
use theme::*;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Page {
    Changes,
    History,
    Branches,
    Snapshots,
    Stashes,
    Conflicts,
    Remotes,
    Github,
    Tools,
    Settings,
    Compare,
    Merge,
}

#[derive(Clone)]
enum Task {
    Command {
        program: String,
        args: Vec<String>,
        refresh: bool,
    },
    Snapshot(String),
    SetCommitLock {
        path: String,
        locked: bool,
    },
    StageUnlocked,
    PrepareCommit {
        message: String,
        amend: bool,
    },
    Commit(branchroom::commit::Plan),
    RemoveSnapshot(String),
    RestoreSnapshot(String),
    Merge {
        target: String,
        reference: String,
    },
    Worktree {
        reference: String,
        destination: PathBuf,
        ide: usize,
    },
    Promote {
        reference: String,
        branch: String,
    },
    Resolve {
        path: String,
        content: String,
        original: String,
    },
    Clone {
        url: String,
        destination: PathBuf,
    },
    Init(PathBuf),
    ImagePreview {
        path: String,
        staged: bool,
    },
    FilePatch {
        path: String,
        staged: bool,
        whitespace: bool,
        full: bool,
    },
    ApplyPatch(String),
    History(branchroom::history::Action),
    Discard(String),
    RestoreDiscard(String),
    Ignore(String),
    SwitchBranch {
        branch: String,
        stash: bool,
    },
    CommitFiles(String),
    PullRequests,
    PullRequestDetails(u64),
    CreatePullRequest(branchroom::github::CreateRequest),
    GithubAction(branchroom::github::Action),
    CommitPullRequests(String),
    OpenPr {
        number: u64,
        destination: PathBuf,
        ide: usize,
    },
    Revert {
        reference: String,
        mainline: Option<usize>,
    },
    UpdateBranch {
        branch: String,
        baseline: String,
    },
}
enum JobData {
    Image {
        path: String,
        staged: bool,
        comparison: branchroom::image_diff::Comparison,
    },
    GithubActionDone {
        details: Option<branchroom::github::PullRequestDetails>,
        review: bool,
    },
    PullRequestDetails(branchroom::github::PullRequestDetails),
    PullRequestCreated,
    CommitReview(branchroom::commit::Plan),
    SnapshotRestored(String),
    PullRequests(Vec<branchroom::collaboration::PullRequest>),
    None,
    Committed,
    Patch {
        path: String,
        staged: bool,
        patch: branchroom::diff::Patch,
    },
    Files {
        commit: String,
        files: Vec<String>,
    },
}
#[derive(Clone)]
struct Pending {
    title: String,
    explanation: String,
    task: Task,
}
struct Completed {
    output: Result<String>,
    state: Option<Result<RepoState>>,
    detail: bool,
    open: Option<(PathBuf, usize)>,
    data: JobData,
}
struct Field {
    label: String,
    value: String,
}
struct Form {
    title: String,
    description: String,
    program: String,
    prefix: Vec<String>,
    fields: Vec<Field>,
    suffix: Vec<String>,
    error: String,
}

pub struct Branchroom {
    settings: Settings,
    repo: Option<std::sync::Arc<RepoState>>,
    page: Page,
    root_input: String,
    repository_filter: String,
    receiver: Option<Receiver<Completed>>,
    busy: String,
    transfer_progress: process::Progress,
    last_refresh: Instant,
    last_draft_save: Instant,
    close_pending: bool,
    close_after_job: bool,
    output: String,
    detail: String,
    error: bool,
    pending: Option<Pending>,
    confirmation_input: String,
    command_bar_open: bool,
    command_bar_query: String,
    form: Option<Form>,
    selected_commit: String,
    selected_path: String,
    query: String,
    history_limit: usize,
    commit_message: String,
    amend: bool,
    snapshot_label: String,
    left_snapshot: String,
    right_snapshot: String,
    snapshot_branch: String,
    snapshot_ide: usize,
    snapshot_directory: String,
    base_input: String,
    conflict_content: String,
    conflict_original: String,
    conflict_path: String,
    conflict_side: usize,
    remote_name: String,
    remote_url: String,
    clone_url: String,
    clone_directory: String,
    command_json: String,
    command_program: String,
    github_number: String,
    github_available: bool,
    pr_title: String,
    pr_body: String,
    pr_reviewers: String,
    pr_labels: String,
    pr_details: Option<branchroom::github::PullRequestDetails>,
    pr_review: String,
    pr_base: String,
    pr_head: String,
    pr_draft: bool,
    pr_tab: usize,
    pull_requests: Vec<branchroom::collaboration::PullRequest>,
    prs_loaded: bool,
    review_open: Option<(String, Option<u64>)>,
    review_directory: String,
    review_ide: usize,
    review_error: String,
    ide_name: String,
    ide_executable: String,
    ide_arguments: String,
    graph: std::sync::Arc<Vec<graph::GraphRow>>,
    history_view: history::HistoryView,
    activity_open: bool,
    inspector_open: bool,
    detail_open: bool,
    patch_raw: bool,
    detail_title: String,
    detail_page: Page,
    selected_snapshot: String,
    source_details: Option<String>,
    merge_source: String,
    snapshot_list: bool,
    history_list: bool,
    branch_filter: String,
    branch_picker_requested: bool,
    density: usize,
    auto_refresh: bool,
    capture_open: bool,
    preview_mode: bool,
    was_compact: bool,
    file_patch: Option<std::sync::Arc<branchroom::diff::Patch>>,
    image_data: Option<branchroom::image_diff::Comparison>,
    image_view: Option<images::ImageView>,
    diff_staged: bool,
    diff_split: bool,
    diff_whitespace: bool,
    diff_full: bool,
    diff_refresh: bool,
    selected_hunks: std::collections::BTreeSet<usize>,
    selected_lines: std::collections::BTreeSet<(usize, usize)>,
    file_filter: String,
    files_tree: bool,
    compact_changes_tab: usize,
    commit_description: String,
    coauthor: String,
    compare_base: String,
    compare_head: String,
    history_steps: usize,
    squash_message: String,
    reorder_ids: Vec<String>,
    commit_files: Vec<String>,
    commit_files_for: String,
    switch_target: Option<String>,
    switch_stash: bool,
    cancellation: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    #[cfg(feature = "ui-preview")]
    preview_state: usize,
    #[cfg(feature = "ui-preview")]
    preview_sender: Option<mpsc::Sender<Completed>>,
}

impl Branchroom {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = &cc.egui_ctx;
        let preview_mode = cfg!(feature = "ui-preview")
            && (std::env::args().any(|a| a == "--ui-preview")
                || std::env::current_exe()
                    .ok()
                    .is_some_and(|p| p.file_stem().is_some_and(|n| n == "branchroom-preview")));
        Self::with_context(ctx, preview_mode)
    }
    fn with_context(ctx: &egui::Context, preview_mode: bool) -> Self {
        let loaded = if preview_mode {
            Ok(Settings::default())
        } else {
            Settings::load()
        };
        let error = loaded.as_ref().err().cloned();
        let settings = loaded.unwrap_or_default();
        theme::install(ctx, settings.light_theme);
        let density = settings.ui_density.min(2);
        let auto_refresh = settings.auto_refresh;
        ctx.set_zoom_factor(settings.ui_zoom.clamp(0.75, 2.));
        let initial = if preview_mode {
            None
        } else {
            std::env::args()
                .nth(1)
                .map(PathBuf::from)
                .or_else(|| settings.recent.first().cloned())
        };
        let mut app = Self {
            settings,
            repo: None,
            page: Page::History,
            root_input: String::new(),
            repository_filter: String::new(),
            receiver: None,
            busy: String::new(),
            transfer_progress: Default::default(),
            last_refresh: Instant::now(),
            last_draft_save: Instant::now(),
            close_pending: false,
            close_after_job: false,
            output: error
                .clone()
                .unwrap_or_else(|| "Open a repository or clone one to get started.".into()),
            detail: String::new(),
            error: error.is_some(),
            pending: None,
            confirmation_input: String::new(),
            command_bar_open: false,
            command_bar_query: String::new(),
            form: None,
            selected_commit: String::new(),
            selected_path: String::new(),
            query: String::new(),
            history_limit: 500,
            commit_message: String::new(),
            amend: false,
            snapshot_label: String::new(),
            left_snapshot: String::new(),
            right_snapshot: String::new(),
            snapshot_branch: String::new(),
            snapshot_ide: 0,
            snapshot_directory: String::new(),
            base_input: "origin/main".into(),
            conflict_content: String::new(),
            conflict_original: String::new(),
            conflict_path: String::new(),
            conflict_side: 3,
            remote_name: "origin".into(),
            remote_url: String::new(),
            clone_url: String::new(),
            clone_directory: String::new(),
            command_json: "[\"status\", \"--short\", \"--branch\"]".into(),
            command_program: "git".into(),
            github_number: String::new(),
            github_available: preview_mode || process::find_executable("gh").is_some(),
            pr_title: String::new(),
            pr_body: String::new(),
            pr_reviewers: String::new(),
            pr_labels: String::new(),
            pr_details: None,
            pr_review: String::new(),
            pr_base: String::new(),
            pr_head: String::new(),
            pr_draft: true,
            pr_tab: 0,
            pull_requests: vec![],
            prs_loaded: false,
            review_open: None,
            review_directory: String::new(),
            review_ide: 0,
            review_error: String::new(),
            ide_name: String::new(),
            ide_executable: String::new(),
            ide_arguments: "[\"{path}\"]".into(),
            graph: Default::default(),
            history_view: Default::default(),
            activity_open: false,
            inspector_open: true,
            detail_open: false,
            patch_raw: false,
            detail_title: "Patch preview".into(),
            detail_page: Page::History,
            selected_snapshot: String::new(),
            source_details: None,
            merge_source: String::new(),
            snapshot_list: false,
            history_list: false,
            branch_filter: String::new(),
            branch_picker_requested: false,
            density,
            auto_refresh,
            capture_open: false,
            preview_mode,
            was_compact: false,
            file_patch: None,
            image_data: None,
            image_view: None,
            diff_staged: false,
            diff_split: false,
            diff_whitespace: false,
            diff_full: false,
            diff_refresh: false,
            selected_hunks: Default::default(),
            selected_lines: Default::default(),
            file_filter: String::new(),
            files_tree: true,
            compact_changes_tab: 0,
            commit_description: String::new(),
            coauthor: String::new(),
            compare_base: "main".into(),
            compare_head: "HEAD".into(),
            history_steps: 2,
            squash_message: String::new(),
            reorder_ids: vec![],
            commit_files: vec![],
            commit_files_for: String::new(),
            switch_target: None,
            switch_stash: false,
            cancellation: None,
            #[cfg(feature = "ui-preview")]
            preview_state: 0,
            #[cfg(feature = "ui-preview")]
            preview_sender: None,
        };
        if let Some(path) = initial {
            app.open_repo(path, ctx);
        }
        #[cfg(feature = "ui-preview")]
        if preview_mode {
            app.load_preview_state();
        }
        app
    }
    fn path(&self) -> PathBuf {
        self.repo
            .as_ref()
            .map(|r| r.root.clone())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
    }
    fn report(&mut self, result: Result<String>) {
        self.error = result.is_err();
        if self.error {
            self.activity_open = true;
        }
        self.output = result.unwrap_or_else(|e| e);
    }
    fn save(&mut self) {
        self.settings.ui_density = self.density;
        self.settings.auto_refresh = self.auto_refresh;
        if self.preview_mode {
            return;
        }
        if let Err(e) = self.settings.save() {
            self.report(Err(e));
        }
    }
    fn open_repo(&mut self, path: PathBuf, ctx: &egui::Context) {
        if self.receiver.is_some() {
            return;
        }
        self.persist_draft();
        self.pull_requests.clear();
        self.prs_loaded = false;
        self.pr_base.clear();
        self.pr_head.clear();
        self.pr_title.clear();
        self.pr_body.clear();
        self.pr_details = None;
        self.pr_review.clear();
        self.review_open = None;
        self.source_details = None;
        self.merge_source.clear();

        if self.preview_mode {
            self.report(Err("Repository switching is disabled in UI preview. Restart without --ui-preview to open a project.".into()));
            return;
        }
        if self.receiver.is_some() {
            return;
        }
        self.repo = None;
        self.detail.clear();
        self.selected_path.clear();
        self.file_patch = None;
        self.diff_refresh = false;
        self.selected_hunks.clear();
        self.selected_lines.clear();
        self.commit_files.clear();
        self.commit_files_for.clear();
        self.commit_message.clear();
        self.commit_description.clear();
        self.coauthor.clear();
        self.amend = false;
        self.conflict_path.clear();
        self.conflict_content.clear();
        self.conflict_original.clear();
        self.selected_commit.clear();
        self.selected_snapshot.clear();
        self.left_snapshot.clear();
        self.right_snapshot.clear();
        self.detail_open = false;
        self.query.clear();
        self.root_input = path.to_string_lossy().into();
        self.base_input = self
            .settings
            .projects
            .get(&path)
            .map(|p| p.base.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "origin/main".into());
        self.refresh_at(path, ctx);
    }
    fn refresh_at(&mut self, path: PathBuf, ctx: &egui::Context) {
        if self.receiver.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        let base = self.base_input.clone();
        let limit = self.history_limit;
        let opening = self.repo.is_none();
        self.receiver = Some(rx);
        self.busy = "Reading repository".into();
        self.last_refresh = Instant::now();
        let token = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.cancellation = Some(token.clone());
        if let Ok(mut progress) = self.transfer_progress.lock() {
            progress.clear();
        }
        let progress = self.transfer_progress.clone();
        std::thread::spawn(move || {
            process::set_cancellation(token);
            process::set_progress(progress);
            let state = git::load(&path, &base, limit);
            let _ = tx.send(Completed {
                output: Ok(if opening {
                    "Repository ready".into()
                } else {
                    String::new()
                }),
                state: Some(state),
                detail: false,
                open: None,
                data: JobData::None,
            });
            ctx.request_repaint();
        });
    }
    fn poll(&mut self) {
        let done = match self.receiver.as_ref().map(Receiver::try_recv) {
            Some(Ok(done)) => done,
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                self.receiver = None;
                self.cancellation = None;
                self.busy.clear();
                self.report(Err("The background operation stopped unexpectedly. Refresh the repository before retrying.".into()));
                return;
            }
            _ => return,
        };
        {
            self.receiver = None;
            self.cancellation = None;
            self.busy.clear();
            self.last_refresh = Instant::now();
            if done.detail {
                self.detail = done.output.clone().unwrap_or_else(|e| e);
            }
            match done.data {
                JobData::GithubActionDone { details, review } => {
                    if review {
                        self.pr_review.clear();
                    }
                    self.pr_details = details;
                    self.pr_tab = if self.pr_details.is_some() { 2 } else { 0 };
                    self.prs_loaded = false;
                }
                JobData::PullRequestDetails(details) => {
                    self.pr_details = Some(details);
                    self.pr_tab = 2;
                }
                JobData::PullRequestCreated => {
                    self.pr_title.clear();
                    self.pr_body.clear();
                    self.pr_reviewers.clear();
                    self.pr_labels.clear();
                    self.pr_tab = 0;
                    self.prs_loaded = false;
                }
                JobData::CommitReview(plan) => {
                    self.pending = Some(Pending {
                        title: if plan.amend {
                            "Amend latest commit"
                        } else {
                            "Commit staged changes"
                        }
                        .into(),
                        explanation: format!(
                            "Branch: {}\n{} files included\n\n{}{}",
                            plan.branch,
                            plan.files.len(),
                            plan.message,
                            if plan.amend {
                                "\n\nAmend rewrites the latest commit. Shared commits may require a force push."
                            } else {
                                ""
                            }
                        ),
                        task: Task::Commit(plan),
                    });
                }
                JobData::SnapshotRestored(reference) => {
                    self.selected_snapshot = reference;
                    self.inspector_open = true;
                }
                JobData::PullRequests(prs) => {
                    self.pull_requests = prs;
                    self.prs_loaded = true;
                }
                JobData::Image {
                    path,
                    staged,
                    comparison,
                } => {
                    if self.selected_path == path && self.diff_staged == staged {
                        self.image_data = Some(comparison);
                        self.image_view = None;
                    }
                }
                JobData::Patch {
                    path,
                    staged,
                    patch,
                } => {
                    if self.selected_path == path && self.diff_staged == staged {
                        let changed = self
                            .file_patch
                            .as_ref()
                            .is_none_or(|previous| previous.raw != patch.raw);
                        self.file_patch = Some(std::sync::Arc::new(patch));
                        if changed {
                            self.image_data = None;
                            self.image_view = None;
                            self.selected_hunks.clear();
                            self.selected_lines.clear();
                        }
                    }
                }
                JobData::Files { commit, files } => {
                    self.commit_files_for = commit;
                    self.commit_files = files;
                }
                JobData::Committed => {
                    self.commit_message.clear();
                    self.commit_description.clear();
                    self.coauthor.clear();
                    self.amend = false;
                }
                JobData::None => {}
            }
            if done.output.as_ref().map_or(true, |s| !s.is_empty()) {
                self.report(done.output);
            }
            self.persist_draft();
            if let Some(state) = done.state {
                match state {
                    Ok(repo) => {
                        if !self.selected_path.is_empty() {
                            self.diff_refresh = true;
                        }
                        let switched = self
                            .repo
                            .as_ref()
                            .is_some_and(|old| old.branch != repo.branch);
                        if switched {
                            self.query.clear();
                        }
                        if switched
                            || !repo
                                .branch_commits
                                .iter()
                                .any(|c| c.id == self.selected_commit)
                        {
                            self.selected_commit = repo
                                .branches
                                .iter()
                                .find(|b| b.current)
                                .map(|b| b.id.clone())
                                .or_else(|| repo.branch_commits.first().map(|c| c.id.clone()))
                                .unwrap_or_default();
                        }
                        if !repo
                            .snapshots
                            .iter()
                            .any(|s| s.reference == self.selected_snapshot)
                        {
                            self.selected_snapshot.clear();
                        }
                        if !repo
                            .snapshots
                            .iter()
                            .any(|s| s.reference == self.left_snapshot)
                        {
                            self.left_snapshot.clear();
                        }
                        if !repo
                            .snapshots
                            .iter()
                            .any(|s| s.reference == self.right_snapshot)
                        {
                            self.right_snapshot.clear();
                        }
                        let load_draft = self
                            .repo
                            .as_ref()
                            .is_none_or(|old| old.root != repo.root || old.branch != repo.branch);
                        if self.repo.as_ref().is_some_and(|old| old.root != repo.root) {
                            self.pull_requests.clear();
                            self.pr_details = None;
                            self.pr_review.clear();
                            self.prs_loaded = false;
                            self.github_number.clear();
                            self.selected_path.clear();
                            self.file_patch = None;
                            self.detail.clear();
                            self.detail_open = false;
                            self.history_limit = 500;
                        }
                        if load_draft {
                            self.load_draft(&repo);
                            if repo.changes.len() > 500 {
                                self.files_tree = false;
                            }
                        }
                        self.settings.remember(&repo.root);
                        self.root_input = repo.root.to_string_lossy().into();
                        self.base_input = repo.base.clone();
                        self.graph = std::sync::Arc::new(graph::layout(&repo.branch_commits));
                        self.repo = Some(std::sync::Arc::new(repo));
                        self.save();
                    }
                    Err(e) => self.report(Err(e)),
                }
            }
            if let Some((path, ide)) = done.open {
                let result = self.settings.open(&path, ide);
                self.report(result);
            }
        }
    }
}

impl eframe::App for Branchroom {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.persist_draft();
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll();
        if ctx.input(|i| i.viewport().close_requested()) && self.receiver.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_pending = true;
        }
        if self.close_after_job && self.receiver.is_none() {
            self.persist_draft();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            self.close_after_job = false;
            self.close_pending = false;
        }
        #[cfg(feature = "ui-preview")]
        if self.preview_mode && !cfg!(test) {
            self.preview_switcher(ui);
        }
        self.shortcuts(&ctx);
        self.chrome(ui, &ctx);
        self.command_bar(&ctx);
        self.dialogs(&ctx);
        self.switch_dialog(&ctx);
        self.review_dialog(&ctx);
        if self.close_pending {
            egui::Modal::new(egui::Id::unique("close_during_operation")).show(&ctx,|ui|{
                ui.set_width(420.);ui.heading("An operation is still running");
                ui.label("Wait for it to finish, or cancel it before closing. Your draft text will be saved.");
                ui.horizontal_wrapped(|ui|{
                    if ui.button("Keep app open").clicked(){self.close_pending=false;self.close_after_job=false;}
                    if ui.button("Cancel operation & close").clicked(){if let Some(token)=&self.cancellation{token.store(true,std::sync::atomic::Ordering::Relaxed);}self.close_after_job=true;}
                });
            });
        }
        if self.page == Page::Changes
            && self.diff_refresh
            && self.receiver.is_none()
            && !self.selected_path.is_empty()
        {
            self.diff_refresh = false;
            self.execute(
                Task::FilePatch {
                    path: self.selected_path.clone(),
                    staged: self.diff_staged,
                    whitespace: self.diff_whitespace,
                    full: self.diff_full,
                },
                &ctx,
            );
        }
        if self.last_draft_save.elapsed() > Duration::from_secs(2) {
            self.persist_draft();
            self.last_draft_save = Instant::now();
        }
        ctx.request_repaint_after(Duration::from_secs(1));
        if !self.preview_mode
            && self.auto_refresh
            && self.repo.is_some()
            && self.receiver.is_none()
            && self.pending.is_none()
            && self.form.is_none()
            && self.last_refresh.elapsed() > Duration::from_secs(30)
        {
            self.refresh_at(self.path(), &ctx);
        }
    }
}

fn subtitle(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(MUTED()));
}
fn heading(ui: &mut egui::Ui, title: &str, description: &str) {
    ui.heading(RichText::new(title).size(24.).strong());
    subtitle(ui, description);
    ui.add_space(16.);
}
fn code(ui: &mut egui::Ui, text: &str) {
    let lines: Vec<_> = text.lines().collect();
    ui.spacing_mut().item_spacing.y = 0.;
    egui::ScrollArea::both()
        .id_salt("code_scroll")
        .auto_shrink([false, false])
        .show_rows(ui, 20., lines.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.;
            for line in &lines[range] {
                let color = if line.starts_with('+') && !line.starts_with("+++") {
                    SUCCESS()
                } else if line.starts_with('-') && !line.starts_with("---") {
                    DANGER()
                } else if line.starts_with("@@") {
                    ACCENT()
                } else {
                    INK()
                };
                ui.add_sized(
                    [ui.available_width(), 20.],
                    egui::Label::new(RichText::new(*line).monospace().size(12.).color(color))
                        .extend()
                        .selectable(true),
                );
            }
        });
}
