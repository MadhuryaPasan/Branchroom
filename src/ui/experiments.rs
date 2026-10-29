use super::*;

impl Branchroom {
    pub(super) fn snapshots(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.time_machine_page(ui, ctx);
        self.capture_dialog(ctx);
        self.source_details_dialog(ctx);
    }

    fn propose_remove_snapshot(&mut self, saved: &git::Snapshot) {
        self.pending = Some(Pending {
            title: "Remove snapshot".into(),
            explanation: format!(
                "Remove “{}” from the timeline? Its saved version will remain as a deleted safety point and can be restored. Your working files, branches, and open IDE copies stay intact.",
                saved.label
            ),
            task: Task::RemoveSnapshot(saved.reference.clone()),
        });
    }
    fn source_details_dialog(&mut self, ctx: &egui::Context) {
        let Some(id) = self.source_details.clone() else {
            return;
        };
        let commit = self
            .repo
            .as_ref()
            .and_then(|repo| {
                repo.commits
                    .iter()
                    .chain(repo.branch_commits.iter())
                    .find(|c| c.id == id)
            })
            .cloned();
        let response =
            egui::Modal::new(egui::Id::unique("source_commit_metadata")).show(ctx, |ui| {
                ui.set_width(470.0_f32.min(ctx.content_rect().width() - 48.));
                ui.heading("Source commit");
                if let Some(commit) = &commit {
                    ui.add(
                        egui::Label::new(RichText::new(&commit.subject).size(18.).strong()).wrap(),
                    );
                    ui.label(&commit.author);
                    subtitle(ui, &commit.date);
                }
                ui.add_space(16.);
                section(ui, "COMMIT ID");
                ui.add(
                    egui::Label::new(RichText::new(&id).monospace())
                        .wrap()
                        .selectable(true),
                );
                if ui.button("Copy commit ID").clicked() {
                    ctx.copy_text(id.clone());
                }
                if let Some(commit) = &commit
                    && !commit.refs.is_empty()
                {
                    section(ui, "BRANCHES & TAGS");
                    ui.add(egui::Label::new(&commit.refs).wrap());
                }
                ui.add_space(16.);
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Inspect commit changes").clicked() {
                        self.source_details = None;
                        self.view_commit(&id, ctx);
                    }
                    if ui.button("Open code in IDE…").clicked() {
                        self.source_details = None;
                        self.begin_review(&id, None);
                    }
                    if ui.button("Close").clicked() {
                        self.source_details = None;
                    }
                });
            });
        if response.should_close() {
            self.source_details = None;
        }
    }

    fn capture_dialog(&mut self, ctx: &egui::Context) {
        if !self.capture_open {
            return;
        }
        let response=egui::Modal::new(egui::Id::unique("capture_snapshot_modal")).show(ctx,|ui|{
            ui.set_width(470.);ui.heading("Capture snapshot");ui.add_space(8.);
            subtitle(ui,"Save this version without changing your branch or staging area.");ui.add_space(20.);
            ui.label("Snapshot name");ui.add(egui::TextEdit::singleline(&mut self.snapshot_label).desired_width(f32::INFINITY).hint_text("Try a larger offline cache"));
            ui.add_space(12.);subtitle(ui,"Includes tracked files and non-ignored new files. Ignored files and changes inside submodules are excluded.");ui.add_space(24.);
            ui.horizontal(|ui|{
                if primary(ui,"Capture working files").clicked(){
                    if self.snapshot_label.trim().is_empty(){self.report(Err("Enter a snapshot name, then capture again.".into()));}
                    else{self.capture_open=false;self.execute(Task::Snapshot(self.snapshot_label.clone()),ctx);}
                }
                if ui.button("Cancel").clicked(){self.capture_open=false;}
            });
        });
        if response.should_close() {
            self.capture_open = false;
        }
    }

    pub(super) fn snapshot_inspector(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::ScrollArea::vertical().id_salt("snapshot_details").show(ui,|ui| {
            section(ui,"SNAPSHOT DETAILS");ui.add_space(18.);
            let saved=self.repo.as_ref().and_then(|r|r.snapshots.iter().find(|s|s.reference==self.selected_snapshot)).cloned();
            let Some(saved)=saved else{ui.heading("Select a snapshot");subtitle(ui,"Compare it, open an independent copy, or promote it to a branch.");ui.add_space(20.);badge(ui,"Local to this repository",SNAPSHOT());return;};
            ui.label(RichText::new(&saved.label).size(19.).strong());subtitle(ui,&saved.date);ui.add_space(18.);
            if ui.button("Inspect patch").clicked(){self.detail_title=format!("Snapshot · {}",saved.label);self.read("git",process::strings(&["show","--stat","--patch",&saved.id]),ctx);}
            if ui.button("Compare with working tree").clicked(){self.detail_title="Snapshot → working tree".into();self.read("git",process::strings(&["diff",&saved.reference,"--"]),ctx);}
            if ui.button(RichText::new("Remove snapshot…").color(DANGER())).on_hover_text("Keep this saved version as a deleted timeline entry so it can be recovered").clicked(){self.propose_remove_snapshot(&saved);}
            ui.add_space(22.);ui.separator();ui.add_space(12.);section(ui,"OPEN INDEPENDENT COPY");ui.add_space(8.);
            ui.label("IDE");ide_combo(ui,"experiment_ide",&self.settings,&mut self.snapshot_ide);
            ui.label("New parallel workspace directory");ui.add(egui::TextEdit::singleline(&mut self.snapshot_directory).desired_width(f32::INFINITY).hint_text("C:\\Projects\\experiment-a"));
            if ui.button("Choose parent folder…").clicked()&&let Some(path)=rfd::FileDialog::new().pick_folder(){self.snapshot_directory=path.join(format!("experiment-{}",short(&saved.id))).to_string_lossy().into();}
            ui.add_space(8.);
            if ui.button("Open snapshot in IDE").clicked(){
                if self.snapshot_directory.trim().is_empty(){self.report(Err("Choose a new parallel workspace directory before opening this snapshot.".into()));}
                else{self.pending=Some(Pending{title:"Open independent snapshot".into(),explanation:format!("Create a detached parallel workspace at {} and open it in the selected IDE. Choose a different directory and IDE to run another snapshot alongside it.",self.snapshot_directory),task:Task::Worktree{reference:saved.reference.clone(),destination:PathBuf::from(&self.snapshot_directory),ide:self.snapshot_ide}});}
            }
            ui.add_space(8.);subtitle(ui,"Build and run from the selected IDE. Each parallel workspace has its own working files.");
            ui.add_space(22.);ui.separator();ui.add_space(12.);
            egui::CollapsingHeader::new("Choose this version as final").show(ui,|ui|{
                ui.label("New branch name");ui.add(egui::TextEdit::singleline(&mut self.snapshot_branch).desired_width(f32::INFINITY).hint_text("final/offline-cache"));
                if ui.button("Promote to branch").clicked(){
                    if self.snapshot_branch.trim().is_empty(){self.report(Err("Enter a new branch name before promoting the snapshot.".into()));}
                    else{self.pending=Some(Pending{title:"Promote snapshot to branch".into(),explanation:"Create a new branch at this capture. Review and merge it from Branches when ready; your current branch is preserved.".into(),task:Task::Promote{reference:saved.reference.clone(),branch:self.snapshot_branch.clone()}});}
                }
            });
            ui.add_space(14.);
            egui::CollapsingHeader::new("Technical details").show(ui,|ui|{
                section(ui,"SNAPSHOT COMMIT");ui.add(egui::Label::new(RichText::new(&saved.id).monospace().size(12.)).wrap().selectable(true));
                if ui.button("Copy snapshot ID").clicked(){ctx.copy_text(saved.id.clone());}
                section(ui,"SOURCE COMMIT");ui.add(egui::Label::new(saved.parents.first().map(String::as_str).unwrap_or("Root snapshot")).wrap().selectable(true));
                section(ui,"SAVED REFERENCE");ui.add(egui::Label::new(&saved.reference).wrap().selectable(true));
            });
        });
    }
}

pub(super) fn connect(ui: &egui::Ui, source: egui::Rect, target: egui::Rect, color: Color32) {
    let start = source.right_center();
    let end = target.left_center();
    let mid = (start.x + end.x) / 2.;
    ui.painter().add(egui::Shape::line(
        vec![start, egui::pos2(mid, start.y), egui::pos2(mid, end.y), end],
        egui::Stroke::new(1.8, color),
    ));
    ui.painter().circle_filled(end, 3., color);
}
