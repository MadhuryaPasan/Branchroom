mod changes;
mod collaboration;
mod compare;
mod comparison_canvas;
mod connection;
mod dialogs;
mod drafts;
mod experiments;
mod graph_context;
mod history;
mod images;
mod inspector;
mod jobs;
mod manipulation;
mod merge;
mod patch_preview;
#[cfg(feature = "ui-preview")]
mod preview;
mod pull_request;
#[cfg(all(test, feature = "ui-preview"))]
mod qa;
mod shell;
mod theme;
mod time_machine;
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
    TimelineLoad {
        item: branchroom::timeline::Item,
        before: bool,
    },
    TimelinePlan {
        item: branchroom::timeline::Item,
        before: bool,
    },
    TimelineRestore(Box<branchroom::timeline::RestorePlan>),
    TimelineUndo {
        redo: bool,
        id: String,
    },
    TimelineHide {
        id: String,
        hidden: bool,
    },
    Shelved {
        reference: String,
        id: String,
        action: String,
    },
    RawGit(Vec<String>),
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
    GraphApply(Box<branchroom::operation::Preview>),
    ConflictSides(String),
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
    DeleteMerged {
        branch: String,
        expected: String,
    },
    PullRequests(branchroom::collaboration::PullRequestFilter),
    PullRequestPreview {
        head: String,
        base: String,
    },
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
    TimelineDetail {
        root: PathBuf,
        key: String,
        target: String,
        graph: Vec<git::Commit>,
        patch: String,
    },
    TimelinePlan {
        root: PathBuf,
        plan: Box<branchroom::timeline::RestorePlan>,
    },
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
    PullRequestPreview {
        key: String,
        commits: Vec<String>,
        files: Vec<String>,
    },
    None,
    Committed,
    GraphReview(Box<branchroom::operation::Preview>),
    ConflictSides {
        root: PathBuf,
        path: String,
        versions: [String; 3],
    },
    Patch {
        path: String,
        staged: bool,
        patch: branchroom::diff::Patch,
    },
    Files {
        commit: String,
        files: Vec<CommitFile>,
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

#[derive(Clone)]
struct CommitFile {
    path: String,
    added: String,
    removed: String,
}

impl CommitFile {
    fn parse(output: &str) -> Vec<Self> {
        output
            .split('\0')
            .filter_map(|record| {
                let mut fields = record.splitn(3, '\t');
                Some(Self {
                    added: fields.next()?.into(),
                    removed: fields.next()?.into(),
                    path: fields.next()?.into(),
                })
            })
            .collect()
    }
    fn label(&self) -> String {
        if self.added == "-" || self.removed == "-" {
            format!("{} - Binary file", self.path)
        } else {
            format!("{}  +{} / -{}", self.path, self.added, self.removed)
        }
    }
}

pub struct Branchroom {
    settings: Settings,
    repo: Option<std::sync::Arc<RepoState>>,
    page: Page,
    root_input: String,
    repository_filter: String,
    receiver: Option<Receiver<Completed>>,
    busy: String,
    job_started: Option<Instant>,
    transfer_progress: process::Progress,
    last_refresh: Instant,
    last_fetch: Option<Instant>,
    fetch_attempt: Option<Instant>,
    fetching: bool,
    fetch_error: bool,
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
    time_machine: time_machine::Timeline,
    snapshot_label: String,
    snapshot_branch: String,
    snapshot_ide: usize,
    snapshot_directory: String,
    base_input: String,
    conflict_content: String,
    conflict_original: String,
    conflict_path: String,
    conflict_side: usize,
    conflict_versions: [String; 3],
    remote_name: String,
    remote_url: String,
    clone_url: String,
    clone_directory: String,
    command_json: String,
    command_program: String,
    github_number: String,
    github_available: bool,
    pr_title: String,
    pr_auto_title: String,
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
    prs_requested: bool,
    pr_prefill_branch: String,
    pr_preview_requested: String,
    pr_preview_key: String,
    pr_preview_commits: Vec<String>,
    pr_preview_files: Vec<String>,
    pr_publish_remote: String,
    pr_publish_if_needed: bool,
    review_open: Option<(String, Option<u64>)>,
    review_directory: String,
    review_ide: usize,
    review_error: String,
    ide_name: String,
    ide_executable: String,
    ide_arguments: String,
    graph: std::sync::Arc<Vec<graph::GraphRow>>,
    history_view: history::HistoryView,
    inspector: inspector::Inspector,
    connection: connection::Connection,
    graph_context: graph_context::GraphContext,
    manipulation: manipulation::Manipulation,
    activity_open: bool,
    inspector_open: bool,
    detail_open: bool,
    patch_raw: bool,
    detail_title: String,
    detail_page: Page,
    selected_snapshot: String,
    source_details: Option<String>,
    merge_source: String,
    history_list: bool,
    graph_zoom: f32,
    branch_filter: String,
    cleanup_branches: bool,
    branch_picker_requested: bool,
    density: usize,
    auto_refresh: bool,
    capture_open: bool,
    preview_mode: bool,
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
    comparison: comparison_canvas::ComparisonView,
    history_steps: usize,
    squash_message: String,
    reorder_ids: Vec<String>,
    commit_files: Vec<CommitFile>,
    commit_files_for: String,
    commit_files_requested: String,
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
            job_started: None,
            transfer_progress: Default::default(),
            last_refresh: Instant::now(),
            last_fetch: None,
            fetch_attempt: None,
            fetching: false,
            fetch_error: false,
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
            time_machine: Default::default(),
            snapshot_label: String::new(),
            snapshot_branch: String::new(),
            snapshot_ide: 0,
            snapshot_directory: String::new(),
            base_input: "origin/main".into(),
            conflict_content: String::new(),
            conflict_original: String::new(),
            conflict_path: String::new(),
            conflict_side: 3,
            conflict_versions: Default::default(),
            remote_name: "origin".into(),
            remote_url: String::new(),
            clone_url: String::new(),
            clone_directory: String::new(),
            command_json: "[\"status\", \"--short\", \"--branch\"]".into(),
            command_program: "git".into(),
            github_number: String::new(),
            github_available: preview_mode || process::find_executable("gh").is_some(),
            pr_title: String::new(),
            pr_auto_title: String::new(),
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
            prs_requested: false,
            pr_prefill_branch: String::new(),
            pr_preview_requested: String::new(),
            pr_preview_key: String::new(),
            pr_preview_commits: vec![],
            pr_preview_files: vec![],
            pr_publish_remote: "origin".into(),
            pr_publish_if_needed: true,
            review_open: None,
            review_directory: String::new(),
            review_ide: 0,
            review_error: String::new(),
            ide_name: String::new(),
            ide_executable: String::new(),
            ide_arguments: "[\"{path}\"]".into(),
            graph: Default::default(),
            history_view: Default::default(),
            inspector: Default::default(),
            connection: Default::default(),
            graph_context: Default::default(),
            manipulation: Default::default(),
            activity_open: false,
            inspector_open: true,
            detail_open: false,
            patch_raw: false,
            detail_title: "Patch preview".into(),
            detail_page: Page::History,
            selected_snapshot: String::new(),
            source_details: None,
            merge_source: String::new(),
            history_list: false,
            graph_zoom: 1.,
            branch_filter: String::new(),
            cleanup_branches: false,
            branch_picker_requested: false,
            density,
            auto_refresh,
            capture_open: false,
            preview_mode,
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
            comparison: Default::default(),
            history_steps: 2,
            squash_message: String::new(),
            reorder_ids: vec![],
            commit_files: vec![],
            commit_files_for: String::new(),
            commit_files_requested: String::new(),
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
        self.prs_requested = false;
        self.pr_prefill_branch.clear();
        self.pr_preview_requested.clear();
        self.pr_preview_key.clear();
        self.pr_preview_commits.clear();
        self.pr_preview_files.clear();
        self.pr_base.clear();
        self.pr_head.clear();
        self.pr_title.clear();
        self.pr_auto_title.clear();
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
        self.inspector = Default::default();
        self.commit_files_for.clear();
        self.commit_files_requested.clear();
        self.commit_message.clear();
        self.commit_description.clear();
        self.coauthor.clear();
        self.amend = false;
        self.conflict_path.clear();
        self.conflict_content.clear();
        self.conflict_original.clear();
        self.selected_commit.clear();
        self.time_machine = Default::default();
        self.selected_snapshot.clear();
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
        self.last_fetch = None;
        self.fetch_attempt = None;
        self.fetch_error = false;
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
                self.fetching = false;
                self.receiver = None;
                self.cancellation = None;
                self.busy.clear();
                self.report(Err("The background operation stopped unexpectedly. Refresh the repository before retrying.".into()));
                return;
            }
            _ => return,
        };
        {
            if self.fetching {
                self.fetching = false;
                self.fetch_error = done.output.is_err();
                if !self.fetch_error {
                    self.last_fetch = Some(Instant::now());
                }
            }
            self.receiver = None;
            self.job_started = None;
            self.cancellation = None;
            self.busy.clear();
            self.last_refresh = Instant::now();
            if done.detail {
                self.detail = done.output.clone().unwrap_or_else(|e| e);
            }
            self.time_machine.loading = false;
            match done.data {
                JobData::TimelineDetail {
                    root,
                    key,
                    target,
                    graph,
                    patch,
                } => {
                    if root == self.path() && self.time_machine.selected == key {
                        self.time_machine.target = target;
                        self.time_machine.graph = graph;
                        self.time_machine.patch = patch;
                        self.time_machine.loading = false;
                    }
                }
                JobData::TimelinePlan { root, plan } => {
                    if root == self.path() {
                        self.time_machine.plan = Some(plan);
                    }
                }

                JobData::GithubActionDone { details, review } => {
                    if review {
                        self.pr_review.clear();
                    }
                    self.pr_details = details;
                    self.pr_tab = if self.pr_details.is_some() { 4 } else { 0 };
                    self.prs_loaded = false;
                    self.prs_requested = false;
                }
                JobData::PullRequestDetails(details) => {
                    self.pr_details = Some(details);
                    self.pr_tab = 4;
                }
                JobData::PullRequestCreated => {
                    self.pr_title.clear();
                    self.pr_auto_title.clear();
                    self.pr_body.clear();
                    self.pr_reviewers.clear();
                    self.pr_labels.clear();
                    self.pr_tab = 0;
                    self.prs_loaded = false;
                    self.prs_requested = false;
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
                JobData::PullRequestPreview {
                    key,
                    commits,
                    files,
                } => {
                    if self.pr_preview_requested == key {
                        self.pr_preview_key = key;
                        self.pr_preview_commits = commits;
                        self.pr_preview_files = files;
                    }
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
                    if commit == self.selected_commit {
                        self.commit_files_for = commit;
                        self.commit_files = files;
                    } else {
                        self.commit_files_requested.clear();
                    }
                }
                JobData::GraphReview(review) => self.accept_graph_review(*review),
                JobData::ConflictSides {
                    root,
                    path,
                    versions,
                } => {
                    if root == self.path() && path == self.conflict_path {
                        self.conflict_versions = versions;
                    }
                }
                JobData::Committed => {
                    self.manipulation.committed();
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
                        if switched || !repo.commits.iter().any(|c| c.id == self.selected_commit) {
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
                        let load_draft = self
                            .repo
                            .as_ref()
                            .is_none_or(|old| old.root != repo.root || old.branch != repo.branch);
                        if self.repo.as_ref().is_some_and(|old| old.root != repo.root) {
                            self.time_machine = Default::default();
                            self.pull_requests.clear();
                            self.pr_details = None;
                            self.pr_review.clear();
                            self.prs_loaded = false;
                            self.prs_requested = false;
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
                        self.graph = std::sync::Arc::new(graph::layout(&repo.commits));
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
            && !self.manipulation.active()
            && self.receiver.is_none()
            && self.pending.is_none()
            && self.form.is_none()
            && self
                .repo
                .as_ref()
                .is_some_and(|r| !r.remotes.trim().is_empty())
            && self
                .fetch_attempt
                .is_none_or(|t| t.elapsed() > Duration::from_secs(300))
            && branchroom::safety::ensure_writable(&self.path()).is_ok()
        {
            self.execute(
                Task::Command {
                    program: "git".into(),
                    args: process::strings(&["fetch", "--all", "--prune"]),
                    refresh: true,
                },
                &ctx,
            );
        }
        if !self.preview_mode
            && self.auto_refresh
            && !self.manipulation.active()
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
    ui.add(egui::Label::new(RichText::new(text).color(MUTED())).wrap());
}
fn heading(ui: &mut egui::Ui, title: &str, description: &str) {
    ui.heading(RichText::new(title).size(TEXT_TITLE).strong());
    if ui.ctx().content_rect().height() >= 500. {
        subtitle(ui, description);
    }
    ui.add_space(if ui.ctx().content_rect().height() < 500. {
        4.
    } else {
        12.
    });
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
