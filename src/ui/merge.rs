use super::*;

impl Branchroom {
    pub(super) fn merge_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let repo = self.repo.as_ref().unwrap().clone();
        let writable = !branchroom::safety::is_protected(&repo.root);
        egui::ScrollArea::vertical().id_salt("merge_workspace").show(ui,|ui|{
            heading(ui,"Merge branches","Bring changes into the branch you are working on. Review the source and destination before merging.");
            let compact = ui.available_width() < 820.;
            if compact {
                surface().show(ui, |ui| self.merge_controls(ui, ctx, &repo, writable));
                ui.add_space(12.);
                surface().show(ui, |ui| self.merge_preflight(ui, &repo));
            } else {
                ui.columns(2, |columns| {
                    surface().show(&mut columns[0], |ui| {
                        self.merge_controls(ui, ctx, &repo, writable)
                    });
                    surface().show(&mut columns[1], |ui| self.merge_preflight(ui, &repo));
                });
            }
            ui.add_space(18.);
            surface().show(ui,|ui|{
                ui.label(RichText::new("Keep up to date with main").strong());
                subtitle(ui,&format!("Compare with {}. Updating from this branch can be done in advanced operations.",repo.base));
                if self.settings.advanced_mode {
                    ui.menu_button("Advanced branch operations",|ui|{if ui.add_enabled(writable&&self.receiver.is_none(),egui::Button::new("Update current branch from comparison branch…")).clicked(){self.propose_update(&repo.branch);ui.close();}});
                }
                if ui.button("Choose comparison branch").clicked(){self.page=Page::Branches;}
            });
            if !self.detail.is_empty()&&self.detail_page==Page::Merge{
                ui.add_space(18.);section(ui,&self.detail_title);
                egui::ScrollArea::both().id_salt("merge_preview").max_height(400.).show(ui,|ui|{ui.add(egui::Label::new(RichText::new(&self.detail).monospace()).selectable(true));});
            }
        });
    }

    fn merge_controls(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        repo: &RepoState,
        writable: bool,
    ) {
        section(ui, "DESTINATION · CURRENT BRANCH");
        ui.label(RichText::new(&repo.branch).size(20.).strong());
        subtitle(
            ui,
            "Choose a different destination using Current branch in the top bar.",
        );
        ui.add_space(20.);
        ui.label("Merge from");
        if self.merge_source.is_empty() {
            self.merge_source = repo.base.clone();
        }
        egui::ComboBox::from_id_salt("merge_source")
            .selected_text(&self.merge_source)
            .width(310.)
            .show_ui(ui, |ui| {
                for (remote, label) in [(false, "On this PC"), (true, "On remotes · last fetched")]
                {
                    section(ui, label);
                    for branch in repo
                        .branches
                        .iter()
                        .filter(|b| b.remote == remote && !b.current && !b.name.ends_with("/HEAD"))
                    {
                        ui.selectable_value(
                            &mut self.merge_source,
                            branch.name.clone(),
                            &branch.name,
                        );
                    }
                }
            })
            .response
            .on_hover_text("Merge source branch");
        ui.add_space(12.);
        ui.label(format!("{}  →  {}", self.merge_source, repo.branch));
        subtitle(
            ui,
            "Existing commits stay in history. Git may create a merge commit when branches have diverged.",
        );
        ui.add_space(12.);
        if ui
            .button("Preview changes")
            .on_hover_text("Inspect changes introduced by the source since the branches split.")
            .clicked()
        {
            self.detail_title = format!("{} → {} · merge preview", self.merge_source, repo.branch);
            self.read(
                "git",
                process::strings(&[
                    "diff",
                    "--stat",
                    "--patch",
                    &format!("HEAD...{}", self.merge_source),
                ]),
                ctx,
            );
        }
        ui.add_space(16.);
        let can_review = writable
            && self.receiver.is_none()
            && repo.changes.is_empty()
            && repo.operation.is_empty()
            && !self.merge_source.is_empty()
            && self.merge_source != repo.branch;
        if ui
            .add_enabled(
                can_review,
                egui::Button::new("Review merge…").fill(PRIMARY()),
            )
            .clicked()
        {
            self.pending = Some(Pending {
                title: "Merge branches".into(),
                explanation: format!(
                    "Merge {} into {} using the currently loaded branch tips. Existing commits are preserved. If conflicts occur, resolve them or abort the merge in Conflicts. Nothing is pushed.",
                    self.merge_source, repo.branch
                ),
                task: Task::Merge {
                    target: repo.branch.clone(),
                    reference: repo
                        .branches
                        .iter()
                        .find(|b| b.name == self.merge_source)
                        .map(|b| b.id.clone())
                        .unwrap_or_else(|| self.merge_source.clone()),
                },
            });
        }
        if !can_review {
            subtitle(
                ui,
                if !repo.changes.is_empty() {
                    "Commit or stash working changes before reviewing the merge."
                } else if !repo.operation.is_empty() {
                    "Finish or abort the current operation before reviewing the merge."
                } else if self.merge_source == repo.branch || self.merge_source.is_empty() {
                    "Choose a different source branch to continue."
                } else if !writable {
                    "This repository is read-only."
                } else {
                    "Wait for the current operation to finish."
                },
            );
        }
    }

    fn merge_preflight(&mut self, ui: &mut egui::Ui, repo: &RepoState) {
        section(ui, "BEFORE YOU MERGE");
        ui.add_space(12.);
        ui.label(RichText::new("Working tree").strong());
        if repo.changes.is_empty() {
            badge(ui, "Clean", SUCCESS());
            subtitle(ui, "Ready to merge changes into the current branch.");
        } else {
            badge(
                ui,
                &format!(
                    "{} uncommitted {}",
                    repo.changes.len(),
                    if repo.changes.len() == 1 {
                        "change"
                    } else {
                        "changes"
                    }
                ),
                WARNING(),
            );
            subtitle(ui, "Commit or stash these changes before you merge.");
            if ui.button("Review working changes").clicked() {
                self.page = Page::Changes;
            }
        }
        ui.add_space(20.);
        ui.label(RichText::new("Git operation").strong());
        if repo.operation.is_empty() {
            badge(ui, "None in progress", SUCCESS());
            subtitle(ui, "You can start a merge when the working tree is ready.");
        } else {
            badge(ui, &format!("{} in progress", repo.operation), WARNING());
            subtitle(ui, "Finish or abort the current operation before merging.");
            if ui.button("Open conflict resolution").clicked() {
                self.page = Page::Conflicts;
            }
        }
        ui.add_space(20.);
        ui.label(RichText::new("Remote changes").strong());
        subtitle(
            ui,
            "Review uses the branch tips currently loaded. Nothing is pushed automatically.",
        );
    }
}
