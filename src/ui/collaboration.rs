use super::*;

impl Branchroom {
    pub(super) fn begin_review(&mut self, reference: &str, number: Option<u64>) {
        self.review_open = Some((reference.into(), number));
        self.review_error.clear();
        self.review_ide = self.settings.ide_for(&self.path());
        self.review_directory = self
            .path()
            .parent()
            .unwrap_or(&self.path())
            .join(format!("branchroom-review-{}", snapshot::unique_id()))
            .display()
            .to_string();
    }
    pub(super) fn review_dialog(&mut self, ctx: &egui::Context) {
        let Some((reference, number)) = self.review_open.clone() else {
            return;
        };
        let response = egui::Modal::new(egui::Id::unique("open_review")).show(ctx, |ui| {
            ui.set_width(480.0_f32.min(ctx.content_rect().width()-48.));
            ui.label(RichText::new("Open code in an IDE").size(TEXT_HEADER).strong());
            ui.label(format!("Version: {reference}"));
            subtitle(ui, "Create a separate working directory at this version. Your current branch and uncommitted changes stay where they are. Create a branch in the review directory before keeping new commits.");
            ui.add_space(12.);
            ui.label("New directory");
            ui.add(egui::TextEdit::singleline(&mut self.review_directory).desired_width(f32::INFINITY));
            if ui.button("Choose parent folder…").clicked() && let Some(parent) = rfd::FileDialog::new().pick_folder() {
                self.review_directory = parent.join(format!("review-{}", snapshot::unique_id())).display().to_string();
            }
            ui.label("Open with");
            egui::ComboBox::from_id_salt("review_ide").selected_text(self.settings.ides.get(self.review_ide).map(|i|i.name.as_str()).unwrap_or("Choose an IDE")).show_ui(ui, |ui| {
                for (index, ide) in self.settings.ides.iter().enumerate() { ui.selectable_value(&mut self.review_ide, index, &ide.name); }
            }).response.on_hover_text("Review IDE");
            subtitle(ui, "The project default is preselected. Add other editors in Settings. Dependencies are not installed automatically.");
            ui.add_space(16.);
            if !self.review_error.is_empty(){ui.label(RichText::new(&self.review_error).color(DANGER()));}
            ui.horizontal_wrapped(|ui| {
                if primary(ui, "Create parallel workspace & open IDE").clicked() {
                    let destination = PathBuf::from(self.review_directory.trim());
                    if !destination.is_absolute() || destination.exists() {
                        self.review_error="Choose an absolute path to a new directory, then try again.".into();
                    } else {
                        self.review_open = None;
                        let task = if let Some(number) = number { Task::OpenPr { number, destination, ide: self.review_ide } }
                        else { Task::Worktree { reference, destination, ide: self.review_ide } };
                        self.execute(task, ctx);
                    }
                }
                if ui.button("Cancel").clicked() { self.review_open = None; }
            });
        });
        if response.should_close() {
            self.review_open = None;
        }
    }
    pub(super) fn propose_update(&mut self, branch: &str) {
        let base = self
            .repo
            .as_ref()
            .and_then(|r| {
                r.branches
                    .iter()
                    .find(|b| !b.remote && b.name == self.base_input && !b.upstream.is_empty())
            })
            .map(|b| b.upstream.clone())
            .unwrap_or_else(|| self.base_input.clone());
        self.pending = Some(Pending {
            title: format!("Update {branch} from {base}"),
            explanation: format!(
                "Fetch the baseline if it is a remote branch, switch to {branch}, then merge {base}. Commit or stash all changes first. Existing commit IDs are preserved. If conflicts occur, resolve or abort in Conflicts. You will stay on {branch}; nothing is pushed."
            ),
            task: Task::UpdateBranch {
                branch: branch.into(),
                baseline: base,
            },
        });
    }
    pub(super) fn pr_workspace(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(repo) = self.repo.clone() else {
            return;
        };
        let mut selected_tab = self.pr_tab;
        ui.horizontal_wrapped(|ui| {
            for (index, label) in [
                (0, "Open"),
                (1, "Mine"),
                (2, "Review requested"),
                (3, "Create"),
            ] {
                ui.selectable_value(&mut selected_tab, index, label);
            }
        });
        if selected_tab != self.pr_tab {
            self.pr_tab = selected_tab;
            self.prs_loaded = false;
            self.prs_requested = false;
            self.pull_requests.clear();
        }
        ui.add_space(12.);
        if self.pr_tab == 4 {
            self.pull_request_details(ui, ctx);
            return;
        }
        if self.pr_tab < 3 {
            if !self.preview_mode && !self.prs_requested && self.receiver.is_none() {
                self.prs_requested = true;
                let filter = match self.pr_tab {
                    1 => branchroom::collaboration::PullRequestFilter::Mine,
                    2 => branchroom::collaboration::PullRequestFilter::ReviewRequested,
                    _ => branchroom::collaboration::PullRequestFilter::Open,
                };
                self.execute(Task::PullRequests(filter), ctx);
            }
            if !self.prs_loaded && self.receiver.is_some() {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new());
                    ui.label("Loading pull requests…");
                });
            }
            if self.prs_loaded && self.pull_requests.is_empty() {
                subtitle(
                    ui,
                    match self.pr_tab {
                        1 => "You have no open pull requests.",
                        2 => "No pull requests are waiting for your review.",
                        _ => "No open pull requests. Create one to propose a change.",
                    },
                );
            }
            if !self.prs_loaded && self.receiver.is_none() {
                ui.horizontal_wrapped(|ui| {
                    subtitle(
                        ui,
                        "Pull requests could not be loaded. Check the GitHub connection and retry.",
                    );
                    if ui.button("Retry").clicked() {
                        self.prs_requested = true;
                        let filter = match self.pr_tab {
                            1 => branchroom::collaboration::PullRequestFilter::Mine,
                            2 => branchroom::collaboration::PullRequestFilter::ReviewRequested,
                            _ => branchroom::collaboration::PullRequestFilter::Open,
                        };
                        self.execute(Task::PullRequests(filter), ctx);
                    }
                });
            }
            egui::ScrollArea::vertical().id_salt("pr_list").max_height(420.).show(ui, |ui| {
            for pr in self.pull_requests.clone() {
                ui.push_id(pr.number, |ui| {
                    surface().show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            badge(ui, if pr.is_draft { "Draft" } else { &pr.state }, if pr.is_draft || pr.state == "CLOSED" { MUTED() } else { SUCCESS() });
                            ui.hyperlink_to(format!("#{} {}", pr.number, pr.title), &pr.url);
                        });
                        subtitle(
                            ui,
                            &format!(
                                "Compare: {} · Base: {}",
                                pr.head_ref_name, pr.base_ref_name
                            ),
                        );
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Review changes").clicked() { self.execute(Task::PullRequestDetails(pr.number),ctx); }
                            if ui.button("Open code in IDE…").on_hover_text("Download the PR into a separate parallel workspace; keep your current checkout unchanged.").clicked() { self.begin_review(&format!("PR #{} · {}", pr.number, pr.title), Some(pr.number)); }
                            if ui.button("Checks").clicked() { self.detail_title = format!("PR #{} checks", pr.number); self.read("gh", process::strings(&["pr", "checks", &pr.number.to_string()]), ctx); }
                        });
                    });
                    ui.add_space(8.);
                });
            }
        });
            return;
        }
        if self.pr_base.is_empty() {
            self.pr_base = if repo
                .branches
                .iter()
                .any(|b| b.remote && b.name == repo.base)
            {
                repo.base
                    .split_once('/')
                    .map(|(_, name)| name)
                    .unwrap_or(&repo.base)
                    .into()
            } else {
                repo.base.clone()
            };
        }
        if self.pr_head.is_empty() {
            self.pr_head = repo.branch.clone();
        }
        if self.pr_base == self.pr_head {
            let default_base = repo
                .base
                .split_once('/')
                .map(|(_, name)| name)
                .unwrap_or(&repo.base);
            self.pr_base = if default_base != self.pr_head {
                default_base.into()
            } else {
                repo.branches
                    .iter()
                    .filter(|branch| !branch.name.ends_with("/HEAD"))
                    .map(|branch| {
                        if branch.remote {
                            branch
                                .name
                                .split_once('/')
                                .map(|(_, name)| name)
                                .unwrap_or(&branch.name)
                        } else {
                            &branch.name
                        }
                    })
                    .find(|name| *name != self.pr_head)
                    .unwrap_or("")
                    .into()
            };
        }
        if self.pr_prefill_branch != self.pr_head {
            self.pr_prefill_branch = self.pr_head.clone();
            let suggested_title = repo
                .branch_commits
                .first()
                .filter(|_| self.pr_head == repo.branch)
                .map(|commit| commit.subject.clone())
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| {
                    self.pr_head
                        .rsplit('/')
                        .next()
                        .unwrap_or(&self.pr_head)
                        .replace(['-', '_'], " ")
                });
            if self.pr_title.trim().is_empty() || self.pr_title == self.pr_auto_title {
                self.pr_title = suggested_title.clone();
                self.pr_auto_title = suggested_title;
            }
            if self.pr_body.trim().is_empty()
                && let Some(template) = branchroom::github::templates(&repo.root).first()
            {
                match branchroom::github::load_template(&repo.root, template) {
                    Ok(body) => self.pr_body = body,
                    Err(error) => self.report(Err(format!(
                        "Could not load the pull request template: {error}"
                    ))),
                }
            }
        }
        let branch = repo
            .branches
            .iter()
            .find(|b| !b.remote && b.name == self.pr_head);
        let ready = branch.is_some_and(|b| {
            !b.upstream.is_empty()
                && b.tracking.is_empty()
                && repo
                    .branches
                    .iter()
                    .any(|remote| remote.remote && remote.name == b.upstream)
        });
        let publication_summary = if ready {
            "The branch is already published and up to date.".into()
        } else {
            branch
                .map(|branch| {
                    format!(
                        "Publish commit {} from {} to {}, then create the pull request.",
                        branch.id.chars().take(12).collect::<String>(),
                        self.pr_head,
                        self.pr_publish_remote
                    )
                })
                .unwrap_or_else(|| "Choose a local compare branch before publishing.".into())
        };
        let preview_key = format!("{}\u{0}{}", self.pr_head, self.pr_base);
        let preview_selection_valid = !self.pr_head.trim().is_empty()
            && !self.pr_base.trim().is_empty()
            && self.pr_head != self.pr_base;
        if preview_selection_valid
            && !self.preview_mode
            && preview_key != self.pr_preview_key
            && preview_key != self.pr_preview_requested
            && self.receiver.is_none()
        {
            self.pr_preview_requested = preview_key.clone();
            self.execute(
                Task::PullRequestPreview {
                    head: self.pr_head.clone(),
                    base: self.pr_base.clone(),
                },
                ctx,
            );
        }
        let mut remotes: Vec<String> = repo
            .remotes
            .lines()
            .filter_map(|line| line.split_whitespace().next().map(str::to_owned))
            .collect();
        remotes.sort();
        remotes.dedup();
        if !remotes
            .iter()
            .any(|remote| remote == &self.pr_publish_remote)
        {
            self.pr_publish_remote = remotes.first().cloned().unwrap_or_else(|| "origin".into());
        }
        let column_count = if ui.available_width() >= 860. { 2 } else { 1 };
        ui.columns(column_count, |columns| {
            surface().show(&mut columns[0], |ui| {
                ui.label(RichText::new("Create a pull request").strong());
                subtitle(
                    ui,
                    "Choose where these changes should go. Branch names cannot match.",
                );
                ui.label("Base branch");
                egui::ComboBox::from_id_salt("pr_base")
                    .selected_text(&self.pr_base)
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        let mut names: Vec<_> = repo
                            .branches
                            .iter()
                            .map(|branch| {
                                if branch.remote {
                                    branch
                                        .name
                                        .split_once('/')
                                        .map(|(_, name)| name)
                                        .unwrap_or(&branch.name)
                                        .to_string()
                                } else {
                                    branch.name.clone()
                                }
                            })
                            .filter(|name| name != "HEAD" && name != &self.pr_head)
                            .collect();
                        names.sort();
                        names.dedup();
                        for name in names {
                            ui.selectable_value(&mut self.pr_base, name.clone(), name);
                        }
                    })
                    .response
                    .widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::Role::Button,
                            true,
                            format!("Pull request base branch: {}", self.pr_base),
                        )
                    });
                ui.label("Compare branch");
                egui::ComboBox::from_id_salt("pr_head")
                    .selected_text(&self.pr_head)
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for branch in repo.branches.iter().filter(|branch| !branch.remote) {
                            ui.selectable_value(
                                &mut self.pr_head,
                                branch.name.clone(),
                                &branch.name,
                            );
                        }
                    })
                    .response
                    .widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::Role::Button,
                            true,
                            format!("Pull request compare branch: {}", self.pr_head),
                        )
                    });
                ui.label("Title");
                ui.add(
                    egui::TextEdit::singleline(&mut self.pr_title)
                        .desired_width(f32::INFINITY)
                        .hint_text("Describe the change"),
                );
                ui.label("Description");
                ui.add(
                    egui::TextEdit::multiline(&mut self.pr_body)
                        .desired_rows(5)
                        .desired_width(f32::INFINITY)
                        .hint_text("What changed? Why? How was it tested?"),
                );
                egui::CollapsingHeader::new("Reviewers and labels").show(ui, |ui| {
                    ui.label("Reviewers");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.pr_reviewers)
                            .desired_width(f32::INFINITY)
                            .hint_text("GitHub logins, separated by commas"),
                    );
                    ui.label("Labels");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.pr_labels)
                            .desired_width(f32::INFINITY)
                            .hint_text("Label names, separated by commas"),
                    );
                });
                ui.checkbox(&mut self.pr_draft, "Create as draft");
                if !ready {
                    ui.checkbox(
                        &mut self.pr_publish_if_needed,
                        "Publish compare branch automatically",
                    );
                    if self.pr_publish_if_needed {
                        ui.horizontal(|ui| {
                            ui.label("Remote");
                            egui::ComboBox::from_id_salt("pr_publish_remote")
                                .selected_text(&self.pr_publish_remote)
                                .show_ui(ui, |ui| {
                                    for remote in &remotes {
                                        ui.selectable_value(
                                            &mut self.pr_publish_remote,
                                            remote.clone(),
                                            remote,
                                        );
                                    }
                                })
                                .response
                                .widget_info(|| {
                                    egui::WidgetInfo::labeled(
                                        egui::Role::Button,
                                        true,
                                        format!("Publish to remote: {}", self.pr_publish_remote),
                                    )
                                });
                        });
                    }
                }
                let can_publish = ready || (self.pr_publish_if_needed && !remotes.is_empty());
                let can_create = can_publish
                    && !self.pr_base.trim().is_empty()
                    && !self.pr_title.trim().is_empty()
                    && self.pr_head != self.pr_base
                    && branch.is_some();
                let review_clicked = ui
                    .add_enabled_ui(can_create, |ui| primary(ui, "Review pull request…"))
                    .inner
                    .clicked();
                if review_clicked {
                    let request = branchroom::github::CreateRequest {
                        base: self.pr_base.clone(),
                        head: self.pr_head.clone(),
                        head_oid: branch.unwrap().id.clone(),
                        title: self.pr_title.clone(),
                        body: self.pr_body.clone(),
                        draft: self.pr_draft,
                        publish_if_needed: self.pr_publish_if_needed,
                        publish_remote: self.pr_publish_remote.clone(),
                        reviewers: self
                            .pr_reviewers
                            .split(',')
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned)
                            .collect(),
                        labels: self
                            .pr_labels
                            .split(',')
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned)
                            .collect(),
                    };
                    match request.arguments() {
                        Ok(_) => {
                            self.pending = Some(Pending {
                                title: "Create pull request".into(),
                                explanation: format!(
                                    "Base branch: {}\nCompare branch: {}\nTitle: {}\n{}\n{}\nReviewers: {}\nLabels: {}\n\n{}",
                                    self.pr_base,
                                    self.pr_head,
                                    self.pr_title,
                                    if self.pr_draft {
                                        "Create as draft"
                                    } else {
                                        "Ready for review"
                                    },
                                    publication_summary,
                                    if self.pr_reviewers.trim().is_empty() {
                                        "none"
                                    } else {
                                        self.pr_reviewers.trim()
                                    },
                                    if self.pr_labels.trim().is_empty() {
                                        "none"
                                    } else {
                                        self.pr_labels.trim()
                                    },
                                    self.pr_body
                                ),
                                task: Task::CreatePullRequest(request),
                            })
                        }
                        Err(error) => self.report(Err(error)),
                    }
                }
                if !can_create {
                    subtitle(
                        ui,
                        if self.pr_base.trim().is_empty() {
                            "Choose an existing base branch."
                        } else if branch.is_none() {
                            "Choose an existing local compare branch."
                        } else if self.pr_head == self.pr_base {
                            "Choose different base and compare branches."
                        } else if self.pr_title.trim().is_empty() {
                            "Enter a title to continue."
                        } else if !ready && !self.pr_publish_if_needed {
                            "Publish the compare branch manually or enable automatic publishing."
                        } else if remotes.is_empty() {
                            "Add a remote before publishing the compare branch."
                        } else {
                            "Choose a remote to publish this branch."
                        },
                    );
                }
            });
            surface().show(&mut columns[column_count-1], |ui| {
                ui.label(RichText::new("Pull request preview").strong());
                subtitle(ui, "Not created yet · review these details before creating it.");
                ui.horizontal_wrapped(|ui| {
                    badge(ui, "BASE", MUTED());
                    ui.label(RichText::new(&self.pr_base).strong());
                });
                ui.horizontal_wrapped(|ui| {
                    badge(ui, "COMPARE", ACCENT());
                    ui.label(RichText::new(&self.pr_head).strong());
                });
                ui.horizontal_wrapped(|ui| {
                    badge(
                        ui,
                        if self.pr_draft {
                            "DRAFT"
                        } else {
                            "READY FOR REVIEW"
                        },
                        if self.pr_draft {
                            MUTED()
                        } else {
                            SUCCESS()
                        },
                    );
                    if !self.pr_title.trim().is_empty() {
                        ui.label(RichText::new(&self.pr_title).strong());
                    } else {
                        subtitle(ui, "Add a title for this pull request.");
                    }
                });
                if !self.pr_reviewers.trim().is_empty() {
                    subtitle(ui, &format!("Reviewers: {}", self.pr_reviewers.trim()));
                }
                if !self.pr_labels.trim().is_empty() {
                    subtitle(ui, &format!("Labels: {}", self.pr_labels.trim()));
                }
                ui.add_space(12.);
                ui.label(RichText::new("Changes in this pull request").size(TEXT_SUBHEAD).strong());
                if !preview_selection_valid {
                    subtitle(
                        ui,
                        "Choose different existing base and compare branches to preview changes.",
                    );
                } else if self.preview_mode && self.pr_preview_key != preview_key {
                    subtitle(
                        ui,
                        "Live pull request preview is unavailable in UI preview.",
                    );
                } else if self.pr_preview_key != preview_key {
                    if self.pr_preview_requested == preview_key && self.receiver.is_none() {
                        subtitle(
                            ui,
                            "The preview did not load. Check the activity log and retry.",
                        );
                        if ui.button("Retry preview").clicked() {
                            self.pr_preview_requested.clear();
                        }
                    } else {
                        ui.add(egui::Spinner::new());
                        subtitle(ui, "Loading commits and changed files…");
                    }
                } else {
                    section(ui, "COMMITS");
                    badge(
                        ui,
                        &format!("{} shown", self.pr_preview_commits.len()),
                        MUTED(),
                    );
                    if self.pr_preview_commits.len() == 8 {
                        subtitle(ui, "Showing the 8 most recent commits.");
                    }
                    if self.pr_preview_commits.is_empty() {
                        subtitle(ui, "No commits ahead of the base branch.");
                    } else {
                        for commit in &self.pr_preview_commits {
                            ui.label(format!("• {commit}"));
                        }
                    }
                    ui.add_space(12.);
                    section(ui, "CHANGED FILES");
                    badge(
                        ui,
                        &format!("{} shown", self.pr_preview_files.len()),
                        MUTED(),
                    );
                    if self.pr_preview_files.len() == 100 {
                        subtitle(ui, "Showing the first 100 changed files.");
                    }
                    if self.pr_preview_files.is_empty() {
                        subtitle(ui, "No changed files.");
                    } else {
                        egui::ScrollArea::vertical()
                            .max_height(240.)
                            .show(ui, |ui| {
                                for file in &self.pr_preview_files {
                                    ui.label(file);
                                }
                            });
                    }
                }
            });
        });
    }
}
