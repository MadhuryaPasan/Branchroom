use super::*;

const STATES: [&str; 7] = [
    "Typical history",
    "No commits",
    "One commit",
    "Loading",
    "Operation error",
    "Long labels & many files",
    "Merge conflict",
];

impl Branchroom {
    pub(super) fn preview_switcher(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("preview_controls")
            .frame(egui::Frame::new().fill(SELECTED()).inner_margin(10))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("UI PREVIEW").strong());
                    ui.label("Fixture data · repository commands disabled");
                    let before = self.preview_state;
                    egui::ComboBox::from_id_salt("preview_state")
                        .selected_text(STATES[self.preview_state])
                        .show_ui(ui, |ui| {
                            for (index, state) in STATES.iter().enumerate() {
                                ui.selectable_value(&mut self.preview_state, index, *state);
                            }
                        })
                        .response
                        .on_hover_text("Preview scenario");
                    if before != self.preview_state {
                        self.load_preview_state();
                    }
                    if ui.button("Reset view").clicked() {
                        self.load_preview_state();
                    }
                });
            });
    }

    pub(super) fn load_preview_state(&mut self) {
        self.receiver = None;
        self.preview_sender = None;
        self.error = false;
        self.activity_open = false;
        self.busy.clear();
        self.detail.clear();
        self.detail_open = false;
        self.selected_commit.clear();
        self.selected_snapshot.clear();
        self.query.clear();
        let long = self.preview_state == 5;
        // Render fixtures must not expose the developer's profile path.
        let root = if cfg!(windows) {
            PathBuf::from("C:/Projects/sample-project")
        } else {
            PathBuf::from("/projects/sample-project")
        };
        let id = |n: usize| {
            format!(
                "{:08x}{:032x}",
                (n as u64 * 0x9e3779b1) as u32,
                n as u128 * 0xa14f9a2c
            )
        };
        let names = [
            "Keep notes available without a network connection",
            "Merge the search index",
            "Normalize search queries before indexing",
            "Choose the default workspace theme",
            "Create the notes workspace",
        ];
        let commits:Vec<_>=names.iter().enumerate().map(|(i,name)|git::Commit{id:id(5-i),parents:if i==4{vec![]}else if i==1{vec![id(3),id(2)]}else{vec![id(4-i)]},refs:if i==0{"HEAD -> feature/offline".into()}else if i==1{"main, tag: v0.2.0".into()}else{String::new()},subject:if long&&i==0{format!("{name} with incremental reconciliation across every disconnected account and background synchronization job")}else{(*name).into()},author:if long{"Sample Developer — Infrastructure Engineering".into()}else{"Sample Developer".into()},date:"2 hours ago".into()}).collect();
        let snapshots:Vec<_>=["Larger offline cache","Streaming cache writer","Keep the smaller memory budget"].iter().enumerate().map(|(i,label)|git::Snapshot{reference:format!("refs/branchroom/snapshots/{}",i+1),id:id(100+i),label:if long{format!("{label} — experiment with an unusually descriptive name for the disconnected synchronization path")}else{(*label).into()},date:if i==0{"1 hour ago".into()}else{format!("{} hours ago",i+1)},parents:vec![id(if i<2{5}else{4})]}).collect();
        let mut repo = RepoState {
            timeline: vec![],
            default_branch: Some("main".into()),
            head: id(5),
            root,
            branch: if long {
                "feature/offline-cache/reconcile-disconnected-account-metadata-without-losing-local-edits".into()
            } else {
                "feature/offline".into()
            },
            base: "main".into(),
            commits,
            snapshots,
            branches: vec![
                git::Branch {
                    name: "main".into(),
                    id: id(4),
                    fork_point: id(4),
                    ..Default::default()
                },
                git::Branch {
                    name: "feature/offline".into(),
                    current: true,
                    id: id(5),
                    fork_point: id(4),
                    ahead: 2,
                    behind: 1,
                    ..Default::default()
                },
                git::Branch {
                    name: "origin/main".into(),
                    remote: true,
                    id: id(4),
                    fork_point: id(4),
                    ..Default::default()
                },
            ],
            stashes: vec![("stash@{0}".into(), "WIP: try a plain text export".into())],
            ..Default::default()
        };
        let count = if long { 80 } else { 3 };
        repo.changes=(0..count).map(|i|git::Change{index:if i==0{'A'}else{' '},worktree:'M',path:if long{format!("src/synchronization/disconnected_accounts/metadata_reconciliation/incremental_snapshot_queue_{i:03}.rs")}else{["src/cache.rs","src/search.rs","docs/offline.md"][i].into()},original:None}).collect();
        match self.preview_state {
            1 => {
                repo.commits.clear();
                repo.branches.clear();
                repo.snapshots.clear();
                repo.changes.clear();
                repo.stashes.clear();
            }
            2 => {
                repo.commits.truncate(1);
                repo.commits[0].parents.clear();
                repo.snapshots.truncate(1);
            }
            3 => {
                let (tx, rx) = mpsc::channel();
                self.preview_sender = Some(tx);
                self.receiver = Some(rx);
                self.busy = "Reading repository".into();
            }
            4 => {
                self.error = true;
                self.activity_open = true;
                self.output="Fetch failed: could not resolve the remote host. Check the network connection and remote URL, then retry Fetch.".into();
            }
            6 => {
                repo.operation = "merge".into();
                repo.changes = vec![git::Change {
                    index: 'U',
                    worktree: 'U',
                    path: "src/cache.rs".into(),
                    original: None,
                }];
                self.conflict_path = "src/cache.rs".into();
                self.conflict_original="<<<<<<< HEAD\nconst CACHE_SIZE: usize = 128;\n=======\nconst CACHE_SIZE: usize = 256;\n>>>>>>> feature/offline\n".into();
                self.conflict_content = self.conflict_original.clone();
                self.page = Page::Conflicts;
            }
            _ => {}
        }
        if self.preview_state != 4 {
            self.output="Preview data loaded. Open History, Snapshots, Branches, or Changes to inspect this state.".into();
        }
        self.prs_loaded = true;
        self.pull_requests = vec![
            branchroom::collaboration::PullRequest {
                number: 42,
                title: "Keep notes available offline".into(),
                head_ref_name: "feature/offline".into(),
                base_ref_name: "main".into(),
                url: "https://example.invalid/pull/42".into(),
                is_draft: false,
                state: "OPEN".into(),
            },
            branchroom::collaboration::PullRequest {
                number: 43,
                title: "Reduce the memory used by the background cache".into(),
                head_ref_name: "feature/cache-budget".into(),
                base_ref_name: "main".into(),
                url: "https://example.invalid/pull/43".into(),
                is_draft: true,
                state: "OPEN".into(),
            },
        ];
        repo.branch_commits = graph::ancestors(&repo.commits, &repo.head);
        self.selected_commit = repo
            .branch_commits
            .first()
            .map(|c| c.id.clone())
            .unwrap_or_default();
        self.graph = std::sync::Arc::new(graph::layout(&repo.commits));
        self.root_input = repo.root.display().to_string();
        self.base_input = repo.base.clone();
        self.repo = Some(std::sync::Arc::new(repo));
    }
}
