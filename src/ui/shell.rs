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
        if compact && !self.was_compact {
            self.inspector_open = false;
        }
        self.was_compact = compact;
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
                            95.
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
                        ui.add_enabled(false, egui::Button::new("Sync"));
                    }
                    if ui
                        .add_sized(
                            [if compact { 100. } else { 220. }, 28.],
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
                        .add_enabled(self.repo.is_some(), egui::Button::new("Open in IDE"))
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
            if !progress.is_empty() {
                egui::Area::new(egui::Id::unique("operation_progress_toast"))
                    .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16., -48.))
                    .order(egui::Order::Foreground)
                    .show(ctx, |ui| {
                        surface().show(ui, |ui| {
                            ui.set_max_width((ctx.content_rect().width() - 64.).clamp(160., 340.));
                            ui.label(RichText::new(&self.busy).strong());
                            ui.label(progress);
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
                        section(ui, "Activity");
                        if ui
                            .small_button("+")
                            .on_hover_text("Increase interface scale")
                            .clicked()
                        {
                            ctx.set_zoom_factor((ctx.zoom_factor() + 0.1).min(2.));
                            self.settings.ui_zoom = ctx.zoom_factor();
                        }
                        if ui
                            .small_button("−")
                            .on_hover_text("Decrease interface scale")
                            .clicked()
                        {
                            ctx.set_zoom_factor((ctx.zoom_factor() - 0.1).max(0.75));
                            self.settings.ui_zoom = ctx.zoom_factor();
                        }

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
                });
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(PAPER()).inner_margin(0)).show(ui, |ui| {
            ui.add_enabled_ui(self.pending.is_none() && self.form.is_none() && (!self.manipulation.active()||egui::DragAndDrop::has_payload_of_type::<manipulation::Payload>(ctx)), |ui| {
                if self.repo.is_none() && !matches!(self.page, Page::Remotes | Page::Settings) { self.welcome(ui, ctx); return; }
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
                    ui.horizontal_wrapped(|ui| {
                        for (page, label) in [(Page::History, "Commits"), (Page::Branches, "Branch map"), (Page::Compare, "Compare")] {
                            if ui.selectable_label(self.page == page, label).clicked() { self.page = page; }
                        }
                        ui.menu_button("Advanced", |ui| {
                            if ui.button("Preview a graph action").clicked(){self.open_graph_form();ui.close();}
                            if ui.button("History operation forms").clicked(){self.page=Page::Compare;self.comparison.advanced=true;ui.close();}
                            if ui.button("Merge branches").clicked() { self.page = Page::Merge; ui.close(); }
                        });
                    });
                }
                match self.page {
                    Page::History => self.history(ui, ctx), Page::Snapshots => self.snapshots(ui, ctx), Page::Changes => self.changes(ui, ctx),
                    Page::Compare => { egui::Frame::new().inner_margin(20).show(ui, |ui| self.compare(ui, ctx)); },
                    Page::Merge => { egui::Frame::new().inner_margin(20).show(ui, |ui| self.merge_page(ui, ctx)); },
                    Page::Branches => self.branches(ui, ctx), Page::Stashes => self.snapshots(ui, ctx),
                    page => { egui::Frame::new().inner_margin(24).show(ui, |ui| { egui::ScrollArea::vertical().id_salt("secondary_page").show(ui, |ui| match page { Page::Conflicts => self.conflicts(ui,ctx), Page::Remotes => self.remotes(ui,ctx), Page::Github => self.github(ui,ctx), Page::Tools => self.tools(ui,ctx), Page::Settings => self.preferences(ui), _ => {} }); }); }
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
            ui.heading("What would you like to do?");
            ui.add(
                egui::TextEdit::singleline(&mut self.command_bar_query)
                    .id(egui::Id::unique("command_bar_search"))
                    .hint_text("Search actions")
                    .desired_width(f32::INFINITY),
            );
            let query = self.command_bar_query.to_lowercase();
            for (label, page) in [
                ("Open graph", Page::History),
                ("Review changes", Page::Changes),
                ("Open pull requests", Page::Github),
                ("Open Time Machine", Page::Snapshots),
                ("Manage branches", Page::Branches),
                ("Open repositories", Page::Remotes),
                ("Open settings", Page::Settings),
                ("Advanced commands", Page::Tools),
            ] {
                if label.to_lowercase().contains(&query) && ui.button(label).clicked() {
                    self.page = page;
                    self.command_bar_open = false;
                }
            }
            if !query.is_empty()
                && ![
                    "open graph",
                    "review changes",
                    "open pull requests",
                    "open time machine",
                    "manage branches",
                    "open repositories",
                    "open settings",
                    "advanced commands",
                ]
                .iter()
                .any(|label| label.contains(&query))
            {
                subtitle(ui, "No matching actions. Try a shorter search.");
            }
        });
        if response.should_close() {
            self.command_bar_open = false;
        }
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
                95.
            } else {
                180.
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
        let sync = ui.menu_button(
            RichText::new(label).color(state_color),
            |ui| {
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
                });
            },
        );
        sync.response
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
            ui.heading("Open a repository");
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
    pub(super) fn patch_drawer(&mut self, ui: &mut egui::Ui) {
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
            return;
        }
        let max_height = (ui.available_height() * 0.65).max(120.);
        egui::Panel::bottom("patch_drawer")
            .resizable(true)
            .default_size(300_f32.min(max_height))
            .size_range(110.0..=max_height)
            .frame(egui::Frame::new().fill(SURFACE()).inner_margin(12))
            .show(ui, |ui| self.patch_preview_content(ui, true));
    }
    fn patch_preview_content(&mut self, ui: &mut egui::Ui, show_heading: bool) {
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
