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
            ui.heading("Open code in an IDE");
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
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.pr_tab, 0, "Browse pull requests");
            ui.selectable_value(&mut self.pr_tab, 1, "Create pull request");
            if self.pr_details.is_some() {
                ui.selectable_value(&mut self.pr_tab, 2, "Review pull request");
            }
        });
        ui.add_space(16.);
        if self.pr_tab == 2 {
            self.pull_request_details(ui, ctx);
            return;
        }
        if self.pr_tab == 0 {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        self.receiver.is_none(),
                        egui::Button::new("Reload pull requests"),
                    )
                    .on_hover_text(
                        "Read the latest open pull requests from GitHub. Requires gh sign-in.",
                    )
                    .clicked()
                {
                    self.execute(Task::PullRequests, ctx);
                }
            });
            subtitle(
                ui,
                "Remote branches are last-fetched copies. Reload pull requests to check GitHub; fetch to update branch tips.",
            );
            ui.add_space(12.);
            if self.prs_loaded && self.pull_requests.is_empty() {
                subtitle(
                    ui,
                    "No matching pull requests. Use Create pull request to propose a change.",
                );
            }
            if !self.prs_loaded {
                subtitle(
                    ui,
                    "Load pull requests to browse titles, branches, and IDE review actions.",
                );
            }
            egui::ScrollArea::vertical().id_salt("pr_list").max_height(420.).show(ui, |ui| {
            for pr in self.pull_requests.clone() {
                ui.push_id(pr.number, |ui| {
                    surface().show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            badge(ui, if pr.is_draft { "Draft" } else { &pr.state }, if pr.is_draft || pr.state == "CLOSED" { MUTED() } else { SUCCESS() });
                            ui.hyperlink_to(format!("#{} {}", pr.number, pr.title), &pr.url);
                        });
                        subtitle(ui, &format!("{}  →  {}", pr.head_ref_name, pr.base_ref_name));
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
        ui.add_space(8.);
        surface().show(ui, |ui| {
            ui.heading("Create a pull request");
            subtitle(ui, "Compare your published branch with the branch that should receive the changes.");
            if self.pr_base.is_empty() { self.pr_base = if repo.branches.iter().any(|b|b.remote && b.name==repo.base){repo.base.split_once('/').map(|(_,name)|name).unwrap_or(&repo.base).into()}else{repo.base.clone()}; }
            if self.pr_head.is_empty() { self.pr_head = repo.branch.clone(); }
            ui.horizontal_wrapped(|ui| {
                ui.label("Base (destination)").on_hover_text("The branch that will receive your changes, usually main.");
                egui::ComboBox::from_id_salt("pr_base").selected_text(&self.pr_base).show_ui(ui, |ui| {
                    let mut names: Vec<_> = repo.branches.iter().map(|b| if b.remote { b.name.split_once('/').map(|(_, n)|n).unwrap_or(&b.name).to_string() } else { b.name.clone() }).filter(|n|n != "HEAD").collect();
                    names.sort(); names.dedup();
                    for name in names { ui.selectable_value(&mut self.pr_base, name.clone(), name); }
                }).response.on_hover_text("Pull request base branch");
                ui.label("← Compare (source)").on_hover_text("The branch containing the commits you want reviewed.");
                egui::ComboBox::from_id_salt("pr_head").selected_text(&self.pr_head).show_ui(ui, |ui| {
                    for b in repo.branches.iter().filter(|b| !b.remote) { ui.selectable_value(&mut self.pr_head, b.name.clone(), &b.name); }
                }).response.on_hover_text("Pull request compare branch");
            });
            let branch = repo.branches.iter().find(|b| !b.remote && b.name == self.pr_head);
            let ready = branch.is_some_and(|b| !b.upstream.is_empty() && b.tracking.is_empty() && repo.branches.iter().any(|remote|remote.remote&&remote.name==b.upstream));
            if let Some(branch) = branch { ui.label(branchroom::collaboration::branch_status(branch, &repo.branches)); }
            if !ready { subtitle(ui, "Publish or sync this branch before creating the pull request. Fetch to verify its remote status."); }
            ui.horizontal_wrapped(|ui| {
                if ui.button("Preview commits & files").clicked() {
                    let base = if repo.branches.iter().any(|b| b.name == format!("origin/{}",self.pr_base)) { format!("origin/{}",self.pr_base) } else { self.pr_base.clone() };
                    self.detail_title = format!("{} → {} · comparison", self.pr_head, self.pr_base);
                    self.read("git", process::strings(&["log", "--oneline", "--stat", &format!("{base}..{}",self.pr_head)]), ctx);
                }
                if ui.button("Publish / sync branch…").clicked() { self.page = Page::Branches; self.branch_filter = self.pr_head.clone(); }
            });
            ui.horizontal_wrapped(|ui|{
                ui.menu_button("Load description template",|ui|{
                    let templates=branchroom::github::templates(&repo.root);
                    if templates.is_empty(){subtitle(ui,"No pull request templates in this repository.");}
                    for path in templates{
                        let label=path.strip_prefix(&repo.root).unwrap_or(&path).display().to_string();
                        if ui.button(label).clicked(){match branchroom::github::load_template(&repo.root,&path){Ok(text)=>{if self.pr_body.trim().is_empty(){self.pr_body=text;}else{self.pr_body.push_str("\n\n");self.pr_body.push_str(&text);}},Err(e)=>self.report(Err(e))}ui.close();}
                    }
                });
            });
            ui.label("Title");
            ui.add(egui::TextEdit::singleline(&mut self.pr_title).desired_width(f32::INFINITY).hint_text("Describe the change"));
            ui.label("Description (Markdown)");
            ui.add(egui::TextEdit::multiline(&mut self.pr_body).desired_rows(5).desired_width(f32::INFINITY).hint_text("What changed? Why? How was it tested?"));
            egui::CollapsingHeader::new("Reviewers and labels").show(ui,|ui|{
                ui.label("Reviewers");ui.add(egui::TextEdit::singleline(&mut self.pr_reviewers).desired_width(f32::INFINITY).hint_text("GitHub logins, separated by commas"));
                ui.label("Labels");ui.add(egui::TextEdit::singleline(&mut self.pr_labels).desired_width(f32::INFINITY).hint_text("Label names, separated by commas"));
            });
            ui.checkbox(&mut self.pr_draft, "Create as draft").on_hover_text("A draft invites early feedback and is not ready to merge.");
            if primary(ui, "Review pull request…").clicked() {
                if self.pr_title.trim().is_empty() || self.pr_head == self.pr_base || !ready {
                    self.report(Err("Enter a title, choose different base and compare branches, and publish/sync the compare branch before continuing.".into()));
                } else {
                    let request=branchroom::github::CreateRequest{base:self.pr_base.clone(),head:self.pr_head.clone(),head_oid:branch.unwrap().id.clone(),title:self.pr_title.clone(),body:self.pr_body.clone(),draft:self.pr_draft,reviewers:self.pr_reviewers.split(',').map(str::trim).filter(|s|!s.is_empty()).map(str::to_owned).collect(),labels:self.pr_labels.split(',').map(str::trim).filter(|s|!s.is_empty()).map(str::to_owned).collect()};
                    match request.arguments(){Ok(_)=>self.pending=Some(Pending{title:"Create pull request".into(),explanation:format!("Publish {} → {} on the GitHub repository configured for this checkout.\nTitle: {}\n{}\nReviewers: {}\nLabels: {}\n\n{}",self.pr_head,self.pr_base,self.pr_title,if self.pr_draft{"Draft: not ready to merge"}else{"Ready for review"},self.pr_reviewers,self.pr_labels,self.pr_body),task:Task::CreatePullRequest(request)}),Err(e)=>self.report(Err(e))}

                }
            }
        });
    }
}
