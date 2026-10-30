use super::*;

impl Page {
    fn title(self) -> &'static str {
        match self {
            Self::History => "Graph",
            Self::Changes => "Changes",
            Self::Github => "Pull requests",
            Self::Snapshots => "Time Machine",
            Self::Stashes => "Shelved work",
            Self::Branches => "Branches",
            Self::Compare => "Compare",
            Self::Merge => "Merge",
            Self::Conflicts => "Conflicts",
            Self::Remotes => "Repositories",
            Self::Tools => "Command center",
            Self::Settings => "Settings",
        }
    }
}
impl Branchroom {
    pub(super) fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.cancel_graph_action(ctx);
            self.pending = None;
            self.confirmation_input.clear();
            self.command_bar_open = false;
            self.form = None;
            self.capture_open = false;
            self.review_open = None;
            self.source_details = None;
            self.switch_target = None;
            self.close_pending = false;
            self.close_after_job = false;
        }
        if self.pending.is_some()
            || self.form.is_some()
            || self.review_open.is_some()
            || self.source_details.is_some()
            || self.capture_open
            || self.switch_target.is_some()
            || self.close_pending
            || self.manipulation.advanced
        {
            return;
        }
        if self.manipulation.active() {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Enter)) {
                self.confirm_graph_action(ctx);
            }
            return;
        }
        if !ctx
            .memory(|m| m.focused())
            .is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some())
            && self.repo.is_some()
            && self.receiver.is_none()
        {
            if ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::Z)
            }) {
                self.timeline_undo(true, ctx);
                return;
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Z)) {
                self.timeline_undo(false, ctx);
                return;
            }
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::O))
            && self.receiver.is_none()
            && let Some(path) = rfd::FileDialog::new().pick_folder()
        {
            self.open_repo(path, ctx);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::R))
            && self.repo.is_some()
        {
            self.refresh_at(self.path(), ctx);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::K)) {
            self.command_bar_open = true;
            self.command_bar_query.clear();
            ctx.memory_mut(|m| m.request_focus(egui::Id::unique("command_bar_search")));
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Num1)) {
            self.page = Page::History;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Num2)) {
            self.page = Page::Changes;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Num3)) {
            self.page = Page::Snapshots;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::B)) {
            self.branch_picker_requested = true;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::G)) {
            self.page = Page::Changes;
            self.compact_changes_tab = 2;
            ctx.memory_mut(|m| m.request_focus(egui::Id::unique("commit_summary")));
        }
        if self.page == Page::Changes
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Enter))
        {
            self.queue_commit(ctx);
        }
    }
    pub(super) fn chrome(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let writable = !branchroom::safety::is_protected(&self.path());
        let compact = ui.available_width() < 1200.;
        egui::Panel::top("workspace_header")
            .frame(egui::Frame::new().fill(SIDEBAR()).inner_margin(12))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ctx.content_rect().width() < 1000. {
                        ui.menu_button("Modes", |ui| {
                            for page in [
                                Page::History,
                                Page::Changes,
                                Page::Github,
                                Page::Snapshots,
                                Page::Branches,
                                Page::Merge,
                                Page::Compare,
                            ] {
                                if ui.button(page.title()).clicked() {
                                    self.page = page;
                                    self.query.clear();
                                    ui.close();
                                }
                            }
                            if self.settings.advanced_mode && ui.button("Command center").clicked()
                            {
                                self.page = Page::Tools;
                                ui.close();
                            }
                        });
                    }
                    let repo_label = self
                        .repo
                        .as_ref()
                        .map(|r| {
                            r.root
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned()
                        })
                        .unwrap_or_else(|| "Choose a repository".into());
                    let picker = theme::toolbar_selector(
                        ui,
                        if ctx.content_rect().width() < 800. {
                            100.
                        } else {
                            190.
                        },
                        "Current repository",
                        &repo_label,
                        |ui| self.repository_menu(ui, ctx),
                    );
                    picker.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::Role::Button,
                            ui.is_enabled(),
                            self.repo
                                .as_ref()
                                .map(|r| format!("Current repository: {}", r.root.display()))
                                .unwrap_or_else(|| "Open repository".into()),
                        )
                    });
                    self.branch_picker(ui, writable);
                    if self.repo.is_some() {
                        self.smart_sync(ui, ctx, writable);
                    } else {
                        ui.add_enabled(false, egui::Button::new("Sync").min_size(egui::vec2(0., 32.)));
                    }
                    if ui
                        .add_sized(
                            [if compact { 110. } else { 220. }, 32.],
                            egui::Button::new(
                                RichText::new(if compact {
                                    "Search  Ctrl+K"
                                } else {
                                    "Search or run a command   Ctrl+K"
                                })
                                .color(MUTED()),
                            )
                            .fill(PAPER())
                            .stroke(egui::Stroke::new(1., INPUT_BORDER())),
                        )
                        .on_hover_text("Open command center · Ctrl+K")
                        .clicked()
                    {
                        self.command_bar_open = true;
                        self.command_bar_query.clear();
                        ctx.memory_mut(|m| m.request_focus(egui::Id::unique("command_bar_search")));
                    }
                    if ui
                        .add_enabled(self.repo.is_some(), egui::Button::new("Open in IDE").min_size(egui::vec2(0., 32.)))
                        .on_hover_text("Open this repository in its preferred IDE")
                        .clicked()
                    {
                        let result = self
                            .settings
                            .open(&self.path(), self.settings.ide_for(&self.path()));
                        self.report(result);
                    }
                });
            });
        egui::Panel::bottom("workspace_status")
            .exact_size(44.)
            .frame(egui::Frame::new().fill(SIDEBAR()).inner_margin(6))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let state = if self.receiver.is_some() {
                        self.busy.as_str()
                    } else if self.error {
                        "Operation needs attention"
                    } else {
                        self.output
                            .lines()
                            .next()
                            .filter(|s| !s.is_empty())
                            .unwrap_or("Ready")
                    };
                    ui.add_sized(
                        [if compact { 120. } else { 300. }, 22.],
                        egui::Label::new(RichText::new(state).color(if self.error {
                            DANGER()
                        } else {
                            MUTED()
                        }))
                        .truncate(),
                    )
                    .on_hover_text(state);
                    if self.receiver.is_some() {
                        ui.add(egui::Spinner::new().size(14.).color(ACCENT()))
                            .widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::Role::ProgressIndicator,
                                    true,
                                    "Operation in progress",
                                )
                            });
                        if ui.button("Cancel operation").clicked()
                            && let Some(token) = &self.cancellation
                        {
                            token.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                    if ui
                        .selectable_label(self.activity_open, "Activity log")
                        .clicked()
                    {
                        self.activity_open = !self.activity_open;
                    }
                });
            });
        if self.receiver.is_some() {
            let progress = self
                .transfer_progress
                .lock()
                .map(|p| p.clone())
                .unwrap_or_default();
            if !progress.is_empty()
                || self
                    .job_started
                    .is_some_and(|started| started.elapsed() > Duration::from_millis(800))
            {
                egui::Area::new(egui::Id::unique("operation_progress_toast"))
                    .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16., -48.))
                    .order(egui::Order::Foreground)
                    .show(ctx, |ui| {
                        surface().show(ui, |ui| {
                            ui.set_max_width((ctx.content_rect().width() - 64.).clamp(160., 340.));
                            ui.label(RichText::new(&self.busy).strong());
                            if !progress.is_empty() {
                                ui.label(progress);
                            } else {
                                ui.spinner();
                            }
                        });
                    });
            }
        }
        self.graph_preview_bar(ui, ctx);
        if self.activity_open {
            egui::Panel::bottom("activity_drawer")
                .resizable(true)
                .default_size(150.)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        section(ui, "Activity log");
                        if ui.button("Copy output").clicked() {
                            ctx.copy_text(self.output.clone());
                        }
                        if ui.button("Close log").clicked() {
                            self.activity_open = false;
                        }
                    });
                    egui::ScrollArea::both().show(ui, |ui| {
                        ui.add(
                            egui::Label::new(RichText::new(&self.output).monospace())
                                .selectable(true),
                        );
                    });
                });
        }
        if ctx.content_rect().width() >= 1000. {
            egui::Panel::left("repository_navigator")
                .default_size(200.)
                .frame(egui::Frame::new().fill(SIDEBAR()).inner_margin(12))
                .show(ui, |ui| {
                    section(ui, "WORKSPACE");
                    ui.add_space(6.);
                    for (page, kind) in [
                        (Page::History, Icon::Graph),
                        (Page::Changes, Icon::Files),
                        (Page::Github, Icon::Cloud),
                        (Page::Snapshots, Icon::Layers),
                    ] {
                        self.navigation_row(ui, page, kind);
                    }
                    if self.settings.advanced_mode {
                        self.navigation_row(ui, Page::Tools, Icon::Code);
                    }
                    ui.add_space(16.);
                    section(ui, "SYSTEM");
                    ui.add_space(6.);
                    self.navigation_row(ui, Page::Remotes, Icon::Folder);
                    self.navigation_row(ui, Page::Settings, Icon::Settings);
                });
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(PAPER()).inner_margin(0)).show(ui, |ui| {
            ui.add_enabled_ui(self.pending.is_none() && self.form.is_none() && (!self.manipulation.active()||egui::DragAndDrop::has_payload_of_type::<manipulation::Payload>(ctx)), |ui| {
                if self.repo.is_none() && !matches!(self.page, Page::Remotes | Page::Settings) { self.welcome(ui, ctx); return; }
                if self.error {
                    egui::Frame::new().fill(SURFACE()).inner_margin(12).show(ui,|ui|{
                        ui.horizontal_wrapped(|ui|{ui.colored_label(DANGER(),"Action needs attention");if ui.button("Show details").clicked(){self.activity_open=true;}
                        if ui.button("Dismiss error").clicked(){self.error=false;}});
                        if ctx.content_rect().height()>=500.{ui.add(egui::Label::new(human_error(&self.output)).wrap());}
                    });
                }
                if let Some(repo) = &self.repo {
                    let count = repo.changes.iter().filter(|c| c.conflict()).count();
                    if count > 0 || !repo.operation.is_empty() {
                        egui::Frame::new()
                            .fill(SELECTED())
                            .inner_margin(12)
                            .show(ui, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    if count > 0 {
                                        badge(ui, &format!("{count} {}",if count==1{"conflict"}else{"conflicts"}), DANGER());
                                        ui.label(
                                            "Some files need your attention before you can continue.",
                                        );
                                    } else {
                                        badge(ui, "Operation in progress", WARNING());
                                        ui.label("Continue or cancel the operation.");
                                    }
                                    if ui
                                        .button(if count > 0 {
                                            "Resolve conflicts"
                                        } else {
                                            "Review operation"
                                        })
                                        .clicked()
                                    {
                                        self.page = Page::Conflicts;
                                    }
                                });
                            });
                    }
                }

                if matches!(self.page, Page::History | Page::Branches | Page::Merge | Page::Compare) {
                    self.load_graph_context(ctx);
                    egui::Frame::new().inner_margin(egui::Margin::symmetric(16,0)).show(ui,|ui|{ui.horizontal_wrapped(|ui| {
                        for (page, label) in [(Page::History, "Commits"), (Page::Branches, "Branch map"), (Page::Compare, "Compare")] {
                            if ui.selectable_label(self.page == page, label).clicked() { self.page = page; }
                        }
                        if self.settings.advanced_mode {
                        ui.menu_button("Advanced", |ui| {
                            if ui.button("Preview a graph action").clicked(){self.open_graph_form();ui.close();}
                            if ui.button("History operation forms").clicked(){self.page=Page::Compare;self.comparison.advanced=true;ui.close();}
                            if ui.button("Merge branches").clicked() { self.page = Page::Merge; ui.close(); }
                        });
                        }
                    });});
                }
                match self.page {
                    Page::History => self.history(ui, ctx), Page::Snapshots => self.snapshots(ui, ctx), Page::Changes => self.changes(ui, ctx),
                    Page::Compare => { egui::Frame::new().inner_margin(16).show(ui, |ui| self.compare(ui, ctx)); },
                    Page::Merge => { egui::Frame::new().inner_margin(16).show(ui, |ui| self.merge_page(ui, ctx)); },
                    Page::Branches => self.branches(ui, ctx), Page::Stashes => self.snapshots(ui, ctx),
                    page => { egui::Frame::new().inner_margin(16).show(ui, |ui| { egui::ScrollArea::vertical().id_salt("secondary_page").show(ui, |ui| match page { Page::Conflicts => self.conflicts(ui,ctx), Page::Remotes => self.remotes(ui,ctx), Page::Github => self.github(ui,ctx), Page::Tools => self.tools(ui,ctx), Page::Settings => self.preferences(ui), _ => {} }); }); }
                }
            });
        });
    }
    fn repository_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.set_width(350.);
        ui.label(RichText::new("Repositories").size(18.).strong());
        subtitle(ui, "Switch projects or open another repository.");
        ui.add_space(10.);
        if let Some(repo) = &self.repo {
            section(ui, "CURRENT REPOSITORY");
            let name = repo.root.file_name().unwrap_or_default().to_string_lossy();
            theme::picker_row(ui, &name, &repo.root.display().to_string(), true, true)
                .on_hover_text("The repository currently open in Branchroom");
            ui.add_space(10.);
        }
        section(ui, "RECENT REPOSITORIES");
        ui.add(
            egui::TextEdit::singleline(&mut self.repository_filter)
                .hint_text("Search recent repositories")
                .desired_width(f32::INFINITY),
        )
        .on_hover_text("Search recent repositories by name or path");
        let filter = self.repository_filter.trim().to_lowercase();
        let recent: Vec<_> = self
            .settings
            .recent
            .iter()
            .filter(|path| {
                !self.repo.as_ref().is_some_and(|repo| repo.root == **path)
                    && (filter.is_empty()
                        || path.to_string_lossy().to_lowercase().contains(&filter))
            })
            .cloned()
            .collect();
        if recent.is_empty() {
            subtitle(
                ui,
                if filter.is_empty() {
                    "Other recent repositories will appear here."
                } else {
                    "No matching recent repositories."
                },
            );
        }
        egui::ScrollArea::vertical()
            .id_salt("recent_repositories")
            .max_height(180.)
            .show(ui, |ui| {
                for path in recent {
                    let name = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    if theme::picker_row(
                        ui,
                        &name,
                        &path.display().to_string(),
                        false,
                        self.receiver.is_none(),
                    )
                    .clicked()
                    {
                        self.repository_filter.clear();
                        self.open_repo(path, ctx);
                        ui.close();
                    }
                }
            });
        ui.add_space(8.);
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    self.receiver.is_none(),
                    egui::Button::new("Browse repositories…"),
                )
                .clicked()
                && let Some(path) = rfd::FileDialog::new().pick_folder()
            {
                self.open_repo(path, ctx);
                ui.close();
            }
            if ui.button("Clone or initialize…").clicked() {
                self.page = Page::Remotes;
                ui.close();
            }
        });
        egui::CollapsingHeader::new("Open a path").show(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.root_input)
                    .desired_width(f32::INFINITY)
                    .hint_text("C:\\Projects\\repository"),
            );
            if ui
                .add_enabled(self.receiver.is_none(), egui::Button::new("Open directory"))
                .clicked()
            {
                self.open_repo(PathBuf::from(&self.root_input), ctx);
                ui.close();
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("Settings…").clicked() {
                self.page = Page::Settings;
                ui.close();
            }
        });
    }

    pub(super) fn command_bar(&mut self, ctx: &egui::Context) {
        if !self.command_bar_open {
            return;
        }
        let response = egui::Modal::new(egui::Id::unique("command_bar")).show(ctx, |ui| {
            ui.set_width(480_f32.min((ctx.content_rect().width() - 48.).max(200.)));
            ui.label(RichText::new("What would you like to do?").size(TEXT_HEADER).strong());
            ui.add(
                egui::TextEdit::singleline(&mut self.command_bar_query)
                    .id(egui::Id::unique("command_bar_search"))
                    .hint_text("Search actions or describe what you need")
                    .desired_width(f32::INFINITY),
            );
            let query = self.command_bar_query.to_lowercase();
            let commands = [
                (
                    "Workspace",
                    "Open graph",
                    "graph commits history",
                    Page::History,
                    "git log --graph --oneline --decorate --all",
                ),
                (
                    "Workspace",
                    "Review changes",
                    "changes stage files diff",
                    Page::Changes,
                    "git status --short",
                ),
                (
                    "Workspace",
                    "Open pull requests",
                    "pull requests github review",
                    Page::Github,
                    "gh pr list",
                ),
                (
                    "Workspace",
                    "Create a pull request",
                    "create pull request propose",
                    Page::Github,
                    "gh pr create",
                ),
                (
                    "Workspace",
                    "Open Time Machine",
                    "time machine snapshots undo recovery",
                    Page::Snapshots,
                    "git reflog --date=relative",
                ),
                (
                    "Workspace",
                    "Manage branches",
                    "branch map rename delete",
                    Page::Branches,
                    "git branch --list",
                ),
                (
                    "Workspace",
                    "Open repositories",
                    "clone remotes worktrees",
                    Page::Remotes,
                    "git remote -v",
                ),
                (
                    "Workspace",
                    "Open settings",
                    "preferences appearance",
                    Page::Settings,
                    "git config --list",
                ),
                (
                    "Advanced",
                    "Advanced commands",
                    "terminal raw git",
                    Page::Tools,
                    "git help -a",
                ),
                (
                    "Fix it",
                    "Move unpublished commits to a new branch",
                    "move unpublished commits new branch",
                    Page::History,
                    "git rebase -i <base-commit>",
                ),
                (
                    "Fix it",
                    "I committed to the wrong branch",
                    "wrong branch move commits",
                    Page::History,
                    "git rebase -i <base-commit>",
                ),
                (
                    "Fix it",
                    "Undo the last commit safely",
                    "undo last commit revert",
                    Page::History,
                    "git revert <commit>",
                ),
                (
                    "Fix it",
                    "Undo a pushed commit safely",
                    "undo pushed commit revert",
                    Page::History,
                    "git revert <commit>",
                ),
                (
                    "Fix it",
                    "Split a commit",
                    "split commit separate changes",
                    Page::History,
                    "git reset --mixed HEAD^",
                ),
                (
                    "Fix it",
                    "Rename a branch",
                    "rename branch",
                    Page::Branches,
                    "git branch -m <new-name>",
                ),
                (
                    "Fix it",
                    "Recover a deleted branch",
                    "recover deleted branch",
                    Page::Snapshots,
                    "git reflog",
                ),
            ];
            egui::ScrollArea::vertical()
                .id_salt(("command_results",&query))
                .max_height((ctx.content_rect().height() - 150.).max(140.))
                .show(ui, |ui| {
                    for (category, label, keywords, page, git_command) in commands {
                        if category == "Advanced" && !self.settings.advanced_mode {
                            continue;
                        }
                        let matches=command_matches(&query,label,keywords);
                        if matches {
                            if ui.button(label).clicked() {
                                if category == "Fix it" {
                                    self.launch_fix(label, ctx);
                                    self.command_bar_open = false;
                                    continue;
                                }
                                self.page = page;
                                if page == Page::Github {
                                    self.pr_tab =
                                        if label == "Create a pull request" { 3 } else { 0 };
                                    self.prs_requested = false;
                                }
                                self.command_bar_open = false;
                            }
                            egui::CollapsingHeader::new("Show Git command")
                                .id_salt(("command_learning", label))
                                .show(ui, |ui| {
                                    ui.label(RichText::new(git_command).monospace());
                                    subtitle(
                                        ui,
                                        "Review placeholders and effects before running commands in Advanced mode.",
                                    );
                                });
                        }
                    }
                    if !query.is_empty()
                        && !commands.iter().any(|entry| {
                            if entry.0 == "Advanced" && !self.settings.advanced_mode {
                                return false;
                            }
                            command_matches(&query,entry.1,entry.2)
                        })
                    {
                        subtitle(ui, "No matching actions. Try a shorter search.");
                    }
                });
        });
        if response.should_close() {
            self.command_bar_open = false;
        }
    }

    fn launch_fix(&mut self, label: &str, ctx: &egui::Context) {
        let Some(repo) = self.repo.as_ref().cloned() else {
            self.report(Err("Open a repository before starting a fix.".into()));
            return;
        };
        match label {
            "I committed to the wrong branch" | "Move unpublished commits to a new branch" => {
                let count = self
                    .command_bar_query
                    .split_whitespace()
                    .find_map(|w| w.parse::<usize>().ok());
                let words = self
                    .command_bar_query
                    .split_whitespace()
                    .collect::<Vec<_>>();
                let name = words
                    .windows(2)
                    .find(|w| {
                        w[0].eq_ignore_ascii_case("branch") && !["a", "new", "to"].contains(&w[1])
                    })
                    .map(|w| w[1].trim_matches(['\'', '"']).to_owned())
                    .unwrap_or_else(|| "recovered-work".into());
                if let Some(count) = count.or(if label == "I committed to the wrong branch" {
                    Some(1)
                } else {
                    None
                }) {
                    self.begin_graph_action(branchroom::operation::Kind::MoveCommits {
                        name,
                        count,
                    });
                } else {
                    self.begin_graph_action(branchroom::operation::Kind::MoveToBranch { name });
                }
            }
            "Undo the last commit safely" => {
                self.begin_graph_action(branchroom::operation::Kind::Undo)
            }
            "Undo a pushed commit safely" => {
                let Some(commit) = repo.branch_commits.first() else {
                    self.report(Err(
                        "There is no commit on the current branch to revert.".into()
                    ));
                    return;
                };
                self.pending = Some(Pending {
                    title: "Revert the latest commit".into(),
                    explanation: format!(
                        "Create a new commit that reverses {} on {}. The original history stays intact. If it was pushed, publish the revert commit afterward.",
                        commit.id.chars().take(12).collect::<String>(),
                        repo.branch
                    ),
                    task: Task::Revert {
                        reference: commit.id.clone(),
                        mainline: None,
                    },
                });
            }
            "Split a commit" => {
                let Some(commit) = repo.branch_commits.first() else {
                    self.report(Err(
                        "There is no commit on the current branch to split.".into()
                    ));
                    return;
                };
                if commit.parents.len() > 1 {
                    self.report(Err(
                        "A merge commit cannot be split with this guided action. Use the reviewed graph tools instead."
                            .into(),
                    ));
                    return;
                }
                self.page = Page::Changes;
                self.begin_graph_action(if let Some(parent) = commit.parents.first() {
                    branchroom::operation::Kind::Reset {
                        commit: parent.clone(),
                    }
                } else {
                    branchroom::operation::Kind::Undo
                });
            }
            "Rename a branch" => {
                if repo.branch == "Detached HEAD" {
                    self.report(Err("Switch to a branch before renaming it.".into()));
                    return;
                }
                self.form(
                    "Rename branch",
                    "Review the new name, then rename this local branch.",
                    "git",
                    &["branch", "-m", &repo.branch],
                    &[("New branch name", "")],
                    &[],
                );
            }
            "Recover a deleted branch" => {
                self.page = Page::Snapshots;
                if let Some(item) = repo.timeline.iter().find(|item| {
                    let kind = item.kind.to_lowercase();
                    kind.contains("branch") && (kind.contains("delet") || kind.contains("recover"))
                }) {
                    self.time_machine.selected = item.key.clone();
                    self.time_machine.target.clear();
                    self.time_machine.graph.clear();
                    self.time_machine.patch.clear();
                    self.time_machine.loading = !self.preview_mode;
                    self.execute(
                        Task::TimelineLoad {
                            item: item.clone(),
                            before: false,
                        },
                        ctx,
                    );
                } else if let Some(saved) = repo.recovery_heads.first() {
                    self.time_machine.selected = saved.reference.clone();
                    self.selected_snapshot = saved.reference.clone();
                    let item = branchroom::timeline::Item {
                        key: saved.reference.clone(),
                        reference: saved.reference.clone(),
                        target: saved.id.clone(),
                        label: saved.label.clone(),
                        date: saved.date.clone(),
                        kind: "Safety point".into(),
                        ..Default::default()
                    };
                    self.time_machine.loading = !self.preview_mode;
                    self.execute(
                        Task::TimelineLoad {
                            item,
                            before: false,
                        },
                        ctx,
                    );
                } else {
                    self.report(Err(
                        "No deleted branch recovery point is available in Time Machine.".into(),
                    ));
                    return;
                }
            }
            _ => self.report(Err(format!("This fix is not available: {label}"))),
        }
        ctx.request_repaint();
    }

    fn branch_picker(&mut self, ui: &mut egui::Ui, writable: bool) {
        let ctx = ui.ctx().clone();
        let Some(repo) = self.repo.clone() else {
            ui.add_enabled(false, egui::Button::new("Choose a branch"));
            return;
        };
        let picker = theme::toolbar_selector(
            ui,
            if ctx.content_rect().width() < 800. {
                100.
            } else {
                190.
            },
            "Current branch",
            &repo.branch,
            |ui| {
                ui.set_width(340.);
                ui.label(RichText::new("Switch branch").strong());
                ui.add(
                    egui::TextEdit::singleline(&mut self.branch_filter)
                        .hint_text("Filter branches")
                        .desired_width(f32::INFINITY),
                );
                egui::ScrollArea::vertical()
                    .id_salt("branch_picker_rows")
                    .max_height(320.)
                    .show(ui, |ui| {
                        let filter = self.branch_filter.to_lowercase();
                        for branch in repo.branches.iter().filter(|b| {
                            !b.name.ends_with("/HEAD") && b.name.to_lowercase().contains(&filter)
                        }) {
                            let detail =
                                branchroom::collaboration::branch_status(branch, &repo.branches);
                            if theme::picker_row(
                                ui,
                                &branch.name,
                                &detail,
                                branch.current,
                                branch.current || (writable && self.receiver.is_none()),
                            )
                            .clicked()
                            {
                                if !branch.current {
                                    self.request_switch(&branch.name, &ctx);
                                }
                                ui.close();
                            }
                        }
                    });
                if ui.button("New branch…").clicked() {
                    self.form(
                        "Create branch",
                        "Create and switch to a branch from the current commit.",
                        "git",
                        &["switch", "-c"],
                        &[("Branch name", "")],
                        &[],
                    );
                    ui.close();
                }
                if ui.button("View branch map").clicked() {
                    self.page = Page::Branches;
                    ui.close();
                }
            },
        );
        picker.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::Role::Button,
                ui.is_enabled(),
                format!("Current branch\n{}", repo.branch),
            )
        });
        if self.branch_picker_requested {
            egui::Popup::open_id(&ctx, egui::Popup::default_response_id(&picker));
            self.branch_picker_requested = false;
            ctx.request_repaint();
        }
    }
    fn smart_sync(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, writable: bool) {
        let Some(repo) = self.repo.clone() else {
            return;
        };
        let branch = repo.branches.iter().find(|b| b.current);
        let upstream = branch.map(|b| b.upstream.clone()).unwrap_or_default();
        let (ahead, behind) = repo.sync_counts.unwrap_or_default();
        let remote_upstream = repo.branches.iter().any(|b| b.remote && b.name == upstream);
        let label = if self.fetch_error {
            "Offline - retry".to_owned()
        } else if !remote_upstream {
            "Sync · no remote".to_owned()
        } else if repo.sync_counts.is_none() {
            "Sync · unavailable".to_owned()
        } else if ahead > 0 && behind > 0 {
            format!("Diverged · {ahead} up · {behind} down")
        } else if behind > 0 {
            format!("Pull {behind}")
        } else if ahead > 0 {
            format!("Push {ahead}")
        } else {
            "Up to date".to_owned()
        };
        let state_color = if self.fetch_error {
            DANGER()
        } else if ahead > 0 && behind > 0 {
            WARNING()
        } else if ahead == 0 && behind == 0 && remote_upstream && repo.sync_counts.is_some() {
            SUCCESS()
        } else {
            ACCENT()
        };
        let sync = egui::containers::menu::MenuButton::from_button(
            egui::Button::new(RichText::new(label).color(state_color).strong())
                .min_size(egui::vec2(0., 32.)),
        )
        .ui(ui, |ui| {
                ui.set_max_width((ctx.content_rect().width() - 48.).clamp(180., 360.));
                ui.label(RichText::new("Sync this branch").strong());
                ui.horizontal_wrapped(|ui| {
                    badge(ui, "This computer", ACCENT());
                    ui.label(if repo.sync_counts.is_some() { format!("{ahead} to push · {behind} to pull") } else { "Status unavailable".to_owned() });
                    badge(
                        ui,
                        if remote_upstream {
                            &upstream
                        } else {
                            "No remote"
                        },
                        MUTED(),
                    );
                });
                let message = if !remote_upstream {
                    "Choose a remote to publish this branch.".to_owned()
                } else if repo.sync_counts.is_none() {
                    "Could not compare this branch with the remote. Check for updates and try again.".to_owned()
                } else if ahead > 0 && behind > 0 {
                    "Both sides have new commits. Resolve the divergence before syncing.".to_owned()
                } else if behind > 0 {
                    format!("Pull {behind} commit(s) to this computer.")
                } else if ahead > 0 {
                    format!("Push {ahead} commit(s) to the remote.")
                } else {
                    "The branch matches the last fetched remote state.".to_owned()
                };
                subtitle(ui, &message);
                subtitle(ui, &self.last_fetch.map(|t| format!("Checked {} min ago", t.elapsed().as_secs() / 60)).unwrap_or_else(|| "Not checked yet".into()));
                if self.fetch_error { subtitle(ui, "Could not reach the remote. Check your connection and credentials, then retry."); }
                let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width().min(300.), 44.), egui::Sense::hover());
                let left = rect.left_center() + egui::vec2(12., 0.);
                let right = rect.right_center() - egui::vec2(12., 0.);
                ui.painter().line_segment([left, right], egui::Stroke::new(2., CONNECTOR()));
                ui.painter().circle_filled(left, 5., ACCENT());
                ui.painter().circle_filled(right, 5., if ahead == 0 && behind == 0 { SUCCESS() } else { WARNING() });
                ui.painter().text(rect.center(), egui::Align2::CENTER_BOTTOM, format!("{ahead} outgoing / {behind} incoming"), egui::FontId::proportional(12.), INK());
                ui.add_enabled_ui(writable && self.receiver.is_none(), |ui| {
                    if ui.button("Check for updates").clicked() {
                        self.execute(
                            Task::Command {
                                program: "git".into(),
                                args: process::strings(&["fetch", "--all", "--prune"]),
                                refresh: true,
                            },
                            ctx,
                        );
                        ui.close();
                    }
                    if remote_upstream
                        && repo.sync_counts.is_some()
                        && behind > 0
                        && ahead == 0
                        && primary(ui, "Pull updates…").clicked()
                    {
                        self.propose(
                            "Pull updates",
                            "Bring the current branch up to date if it can be fast-forwarded.",
                            &["pull", "--ff-only"],
                        );
                        ui.close();
                    }
                    if remote_upstream
                        && repo.sync_counts.is_some()
                        && ahead > 0
                        && behind == 0
                        && primary(ui, "Push commits…").clicked()
                    {
                        let mut remotes: Vec<_> = repo
                            .remotes
                            .lines()
                            .filter_map(|line| line.split_whitespace().next())
                            .collect();
                        remotes.sort_by_key(|name| std::cmp::Reverse(name.len()));
                        if let Some((remote, branch)) = remotes.into_iter().find_map(|remote| {
                            upstream
                                .strip_prefix(&format!("{remote}/"))
                                .map(|branch| (remote, branch))
                        }) {
                            let refspec = format!("refs/heads/{}:refs/heads/{branch}", repo.branch);
                            self.propose(
                                "Push commits",
                                &format!("Send {} to {remote}/{branch}.", repo.branch),
                                &["push", remote, &refspec],
                            );
                        } else {
                            self.report(Err(
                            "The tracked remote could not be found. Check the repository settings."
                                .into(),
                        ));
                        }
                        ui.close();
                    }
                    if !remote_upstream && ui.button("Publish this branch…").clicked() {
                        self.form(
                            "Publish branch",
                            "Push this branch to the selected remote and remember its upstream.",
                            "git",
                            &["push", "--set-upstream"],
                            &[("Remote", "origin"), ("Branch", &repo.branch)],
                            &[],
                        );
                        ui.close();
                    }
                    if self.settings.advanced_mode {
                        ui.menu_button("Advanced", |ui| {
                            if ui.button("Pull (fast-forward)…").clicked() {
                                self.propose(
                                    "Pull updates",
                                    "Bring the current branch up to date if it can be fast-forwarded.",
                                    &["pull", "--ff-only"],
                                );
                                ui.close();
                            }
                            self.publish_controls(ui, writable);
                        });
                    }
                });
            },
        );
        sync.0
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), "Sync"));
    }
    fn publish_controls(&mut self, ui: &mut egui::Ui, writable: bool) {
        let Some(repo) = self.repo.clone() else {
            return;
        };
        ui.add_enabled_ui(writable,|ui| {
            let upstream=repo.branches.iter().find(|b|b.current).map(|b|b.upstream.as_str()).unwrap_or("");
            let mut remotes:Vec<_>=repo.remotes.lines().filter_map(|line|line.split_whitespace().next()).collect(); remotes.sort_by_key(|name|std::cmp::Reverse(name.len())); remotes.dedup();
            let destination=remotes.into_iter().find_map(|remote|upstream.strip_prefix(&format!("{remote}/")).map(|branch|(remote,branch)));
            if let Some((remote,branch))=destination {
                let target=format!("refs/heads/{branch}"); let refspec=format!("refs/heads/{}:{target}",repo.branch);
                subtitle(ui, &format!("Destination: {remote}/{branch}"));
                if ui.button("Push current branch…").clicked(){self.propose("Push current branch",&format!("Push {} to {remote}/{branch}. Other branches are not included.",repo.branch),&["push",remote,&refspec]);ui.close();}
                let expected=repo.branches.iter().find(|b|b.remote&&b.name==upstream).map(|b|b.id.as_str());
                ui.separator();
                ui.label(RichText::new("Dangerous").color(DANGER()).strong());
                if ui.add_enabled(expected.is_some(),egui::Button::new(RichText::new("Force push with lease…").color(DANGER()))).clicked(){let lease=format!("--force-with-lease={target}:{}",expected.unwrap());self.propose("Force push with lease",&format!("Replace only {remote}/{branch} if its head still matches the last fetched commit."),&["push",&lease,remote,&refspec]);ui.close();}
                if expected.is_none(){subtitle(ui,"Fetch the remote branch before using a force push lease.");}
            } else { ui.label("This branch has no remote upstream."); if ui.button("Publish branch…").clicked(){self.form("Publish branch","Push this branch to the selected remote and remember its upstream.","git",&["push","--set-upstream"],&[("Remote","origin"),("Branch",&repo.branch)],&[]);ui.close();} }
        });
    }
    fn navigation_row(&mut self, ui: &mut egui::Ui, page: Page, kind: Icon) {
        let selected = self.page == page
            || match page {
                Page::History => matches!(self.page, Page::Branches | Page::Merge | Page::Compare),
                Page::Snapshots => self.page == Page::Stashes,
                _ => false,
            };
        let response = ui.add_sized(
            [ui.available_width(), 36.],
            egui::Button::new("")
                .selected(selected)
                .frame_when_inactive(selected),
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), selected, page.title())
        });
        let rect = response.rect;
        if selected {
            ui.painter().rect_filled(
                egui::Rect::from_min_size(rect.left_top(), egui::vec2(3., rect.height())),
                1.5,
                ACCENT(),
            );
        }
        icon(
            ui.painter(),
            egui::Rect::from_center_size(
                egui::pos2(rect.left() + 19., rect.center().y),
                egui::vec2(18., 18.),
            ),
            kind,
            if selected { ACCENT() } else { MUTED() },
        );
        ui.painter().text(
            egui::pos2(rect.left() + 38., rect.center().y),
            egui::Align2::LEFT_CENTER,
            page.title(),
            egui::FontId::proportional(14.),
            if selected { INK() } else { MUTED() },
        );
        if response.clicked() {
            if page == Page::Github {
                self.pr_tab = 0;
            }
            self.page = page;
            self.query.clear();
        }
    }
    fn welcome(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::Frame::new().inner_margin(36).show(ui, |ui| {
            ui.label(RichText::new("Open a repository").size(TEXT_HEADER).strong());
            subtitle(
                ui,
                "Choose a repository on this computer or clone one to get started.",
            );
            if ui.button("Open repository…").clicked()
                && let Some(path) = rfd::FileDialog::new().pick_folder()
            {
                self.open_repo(path, ctx);
            }
            if ui.button("Clone repository…").clicked() {
                self.page = Page::Remotes;
            }
            for path in self.settings.recent.clone().iter().take(5) {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if theme::picker_row(
                    ui,
                    &name,
                    &path.display().to_string(),
                    false,
                    self.receiver.is_none(),
                )
                .clicked()
                {
                    self.open_repo(path.clone(), ctx);
                }
            }
        });
    }
    pub(super) fn patch_drawer(&mut self, ui: &mut egui::Ui, wide_region_reserved: bool) {
        if !self.detail_open || self.detail_page != self.page {
            return;
        }
        let ctx = ui.ctx().clone();
        if ctx.content_rect().width() < 800. || ctx.content_rect().height() < 500. {
            let mut open = true;
            egui::Window::new("Change preview")
                .id(egui::Id::unique("compact_patch_preview"))
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_width((ctx.content_rect().width() - 32.).max(200.))
                .default_height((ctx.content_rect().height() - 48.).max(160.))
                .show(&ctx, |ui| self.patch_preview_content(ui, false));
            if !open {
                self.detail_open = false;
            }
        } else if !wide_region_reserved {
            let max_height = (ui.available_height() * 0.65).max(120.);
            egui::Panel::bottom("patch_drawer")
                .resizable(true)
                .default_size(300_f32.min(max_height))
                .size_range(110.0..=max_height)
                .frame(egui::Frame::new().fill(SURFACE()).inner_margin(12))
                .show(ui, |ui| self.patch_preview_content(ui, true));
        }
    }
    pub(super) fn patch_preview_content(&mut self, ui: &mut egui::Ui, show_heading: bool) {
        ui.horizontal_wrapped(|ui| {
            if show_heading {
                ui.label(RichText::new("Change preview").strong().size(14.));
            }
            ui.label(RichText::new(&self.detail_title).color(MUTED()));
            if ui
                .button(if self.patch_raw {
                    "Show formatted patch"
                } else {
                    "Show raw patch"
                })
                .clicked()
            {
                self.patch_raw = !self.patch_raw;
            }
            if ui
                .button("Copy patch")
                .on_hover_text("Copy the full original patch")
                .clicked()
            {
                ui.ctx().copy_text(self.detail.clone());
            }
            if ui.button("Close preview").clicked() {
                self.detail_open = false;
            }
        });
        ui.add_space(8.);
        if !self.patch_raw
            && let Some(summary) = patch_preview::summary(&self.detail)
        {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(summary).color(MUTED()).size(12.));
                ui.label(
                    RichText::new("Line numbers: old / new")
                        .color(MUTED())
                        .size(12.),
                );
            });
            ui.add_space(4.);
        }
        patch_preview::show(ui, &self.detail, self.patch_raw);
    }
}

fn command_matches(query: &str, label: &str, keywords: &str) -> bool {
    let haystack = format!("{} {keywords}", label.to_lowercase());
    let is_move = label.contains("new branch") || label.contains("wrong branch");
    let words = query.split_whitespace();
    for word in words {
        if ["i", "the", "a", "to", "my", "on", "last", "new", "please"].contains(&word)
            || word.parse::<usize>().is_ok()
        {
            continue;
        }
        if is_move && word == "branch" {
            return haystack.contains("branch");
        }
        if !haystack.contains(word)
            && !haystack.split_whitespace().any(|candidate| {
                let mut needle = word.chars();
                let mut next = needle.next();
                for c in candidate.chars() {
                    if next == Some(c) {
                        next = needle.next();
                    }
                }
                next.is_none()
            })
        {
            return false;
        }
    }
    true
}

fn human_error(raw: &str) -> String {
    let lower = raw.to_lowercase();
    if lower.contains("not a git repository") {
        return "This folder is not a Git repository. Open an existing repository or create one in Repositories".into();
    }
    if lower.contains("authentication")
        || lower.contains("not logged")
        || lower.contains("bad credentials")
    {
        return "GitHub could not authenticate this request. Sign in from Pull requests and try again".into();
    }
    if lower.contains("could not resolve host")
        || lower.contains("unable to access")
        || lower.contains("network is unreachable")
    {
        return "The remote could not be reached. Check your connection and remote address, then try again".into();
    }
    if lower.contains("permission denied") {
        return "The action was refused. Check your file or repository permissions, then try again"
            .into();
    }
    raw.lines()
        .find(|line| {
            !line.trim().is_empty()
                && !line.contains("exited with")
                && !line.starts_with("git exited")
                && !line.starts_with("gh exited")
        })
        .map(|line| {
            line.trim_start_matches("fatal: ")
                .trim_start_matches("error: ")
                .to_owned()
        })
        .unwrap_or_else(|| {
            "The action could not finish. Open the details, fix the reported issue and try again"
                .into()
        })
}
