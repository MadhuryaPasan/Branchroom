use super::*;

impl Branchroom {
    pub(super) fn snapshots(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let repo = self.repo.as_ref().unwrap().clone();
        let compact = ui.available_width() < 700.;
        egui::Panel::top("snapshot_controls")
            .frame(
                egui::Frame::new()
                    .fill(PAPER())
                    .inner_margin(egui::Margin::symmetric(22, 16)),
            )
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.heading("Snapshots");
                    badge(ui, &format!("{} saved", repo.snapshots.len()), SNAPSHOT());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(
                                !branchroom::safety::is_protected(&repo.root),
                                egui::Button::new(if compact{"Capture…"}else{"Capture snapshot"}).fill(PRIMARY()),
                            )
                            .clicked()
                        {
                            self.capture_open = true;
                        }
                        ui.menu_button(if compact{"Removed".to_string()}else{format!("Removed snapshots ({})",repo.removed_snapshots.len())},|ui|{
                            ui.set_width(310.);
                            subtitle(ui,"Restore a snapshot removed from this repository. Open parallel workspaces and branches were kept.");
                            egui::ScrollArea::vertical().max_height(260.).show(ui,|ui|{
                                for saved in &repo.removed_snapshots {
                                    if ui.add_enabled(self.receiver.is_none()&&!branchroom::safety::is_protected(&repo.root),egui::Button::new(format!("Restore {}",saved.label)).truncate()).on_hover_text(&saved.label).clicked(){self.execute(Task::RestoreSnapshot(saved.reference.clone()),ctx);ui.close();}
                                }
                                if repo.removed_snapshots.is_empty(){subtitle(ui,"No removed snapshots.");}
                            });
                        });
                        if !compact
                            && ui
                                .selectable_label(self.inspector_open, "Inspector")
                                .clicked()
                        {
                            self.inspector_open = !self.inspector_open;
                        }
                        if !compact {
                            ui.selectable_value(&mut self.snapshot_list, true, "List");
                            ui.selectable_value(&mut self.snapshot_list, false, "Tree");
                        }
                    });
                });
                if !compact {
                    subtitle(
                        ui,
                        "Keep an experiment. Compare versions. Open each in its own IDE.",
                    );
                }
                ui.add_space(8.);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Filter").color(MUTED()));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.query)
                            .desired_width(if compact { 120. } else { 260. })
                            .hint_text("Snapshot name or SHA"),
                    );
                    if !self.query.is_empty() && ui.button("Clear").clicked() {
                        self.query.clear();
                    }
                    if compact {
                        ui.selectable_value(&mut self.snapshot_list, false, "Tree");
                        ui.selectable_value(&mut self.snapshot_list, true, "List");
                        if ui
                            .selectable_label(self.inspector_open, "Inspector")
                            .clicked()
                        {
                            self.inspector_open = !self.inspector_open;
                        }
                    }
                    if !compact && !self.snapshot_list {
                        ui.menu_button("Tree layout",|ui|{
                            subtitle(ui,"Set the width of the source column.");
                            let changed=super::history::column_input(ui,"Source",&mut self.settings.snapshot_source_width,140.0..=420.0);
                            let reset=ui.button("Reset tree layout").clicked();
                            if reset{self.settings.snapshot_source_width=194.;}
                            if (changed||reset)&&!self.preview_mode&&let Err(error)=self.settings.save(){self.report(Err(error));}
                        });
                    }
                    if !compact {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new("Source commit → saved working files")
                                    .size(12.)
                                    .color(MUTED()),
                            );
                        });
                    }
                });
            });
        self.patch_drawer(ui);
        egui::Panel::bottom("compare_tray")
            .frame(
                egui::Frame::new()
                    .fill(SURFACE())
                    .inner_margin(egui::Margin::symmetric(20, 12)),
            )
            .show(ui, |ui| {
                let compare_open = egui::CollapsingHeader::new("Compare versions")
                    .id_salt(("comparison", compact))
                    .default_open(!compact);
                compare_open.show(ui, |ui| {
                    let picker_width = ((ui.available_width() - 190.) / 2.).clamp(95., 180.);
                    ui.horizontal(|ui| {
                        snapshot_picker(
                            ui,
                            "compare_a",
                            "A",
                            &repo.snapshots,
                            &mut self.left_snapshot,
                            picker_width,
                        );
                        ui.label(RichText::new("→").color(MUTED()));
                        snapshot_picker(
                            ui,
                            "compare_b",
                            "B",
                            &repo.snapshots,
                            &mut self.right_snapshot,
                            picker_width,
                        );
                        if ui.button("Compare").clicked() {
                            if self.left_snapshot.is_empty() || self.right_snapshot.is_empty() {
                                self.report(Err(
                                    "Choose a snapshot for both A and B, then compare again."
                                        .into(),
                                ));
                            } else {
                                self.detail_title = "Snapshot comparison · A → B".into();
                                self.read(
                                    "git",
                                    process::strings(&[
                                        "diff",
                                        &self.left_snapshot,
                                        &self.right_snapshot,
                                        "--",
                                    ]),
                                    ctx,
                                );
                            }
                        }
                    });
                });
            });
        if self.inspector_open {
            if ui.available_width() >= 850. {
                egui::Panel::right("snapshot_inspector")
                    .resizable(true)
                    .default_size(300.)
                    .size_range(265.0..=420.0)
                    .frame(egui::Frame::new().fill(SURFACE()).inner_margin(20))
                    .show(ui, |ui| {
                        self.snapshot_inspector(ui, ctx);
                    });
            } else {
                let mut open = true;
                egui::Window::new("Snapshot inspector")
                    .open(&mut open)
                    .default_width(310.)
                    .max_height((ctx.content_rect().height() - 120.).max(150.))
                    .show(ctx, |ui| {
                        self.snapshot_inspector(ui, ctx);
                    });
                if !open {
                    self.inspector_open = false;
                }
            }
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(PAPER()).inner_margin(0)).show(ui,|ui| {
            let snapshots:Vec<_>=repo.snapshots.iter().filter(|s|self.query.is_empty()||format!("{} {}",s.label,s.id).to_lowercase().contains(&self.query.to_lowercase())).cloned().collect();
            let mut groups=graph::snapshot_groups(&snapshots);
            let head=(!repo.head.is_empty()).then(||repo.head.clone());
            if self.query.is_empty()&&!groups.iter().any(|g|g.parent==head){groups.insert(0,graph::SnapshotGroup{parent:head.clone(),snapshots:vec![]});}
            if self.snapshot_list {
                egui::ScrollArea::vertical().id_salt("snapshot_outline").auto_shrink([false,false]).show(ui,|ui|{
                    egui::Frame::new().inner_margin(22).show(ui,|ui|{
                        for group in &groups {
                            let name=source_title(&repo,group.parent.as_deref());
                            egui::CollapsingHeader::new(format!("{}  ·  {} snapshots",name,group.snapshots.len())).default_open(true).id_salt(&group.parent).show(ui,|ui|{
                                for saved in &group.snapshots {
                                    let selected=self.selected_snapshot==saved.reference;
                                    if ui.add_sized([ui.available_width(),54.],egui::Button::new(format!("{}\n{}",saved.label,saved.date)).selected(selected).wrap()).clicked(){self.selected_snapshot=saved.reference.clone();self.inspector_open=true;}
                                }
                                if group.snapshots.is_empty(){subtitle(ui,"Capture the working files to add an experiment here.");}
                            });ui.add_space(14.);
                        }
                    });
                });
            } else if compact {
                egui::ScrollArea::vertical().id_salt("compact_snapshot_tree").auto_shrink([false,false]).show(ui,|ui|{
                    for group in &groups {
                        egui::Frame::new().inner_margin(12).show(ui,|ui|{
                            let source=source_title(&repo,group.parent.as_deref());
                            if ui.add_sized([ui.available_width(),52.],egui::Button::new(format!("Source · {source}\nView commit details")).wrap()).clicked(){self.source_details=group.parent.clone();}
                            ui.indent((&group.parent,"captures"),|ui|{
                                if self.query.is_empty()&&group.parent==head&&ui.add_sized([ui.available_width(),52.],egui::Button::new("Working copy\nReview current changes").wrap()).clicked(){self.page=Page::Changes;}
                                for saved in &group.snapshots {
                                    let response=ui.add_sized([ui.available_width(),60.],egui::Button::new(format!("{}\n{}",saved.label,saved.date)).selected(self.selected_snapshot==saved.reference).wrap());
                                    if response.clicked(){self.selected_snapshot=saved.reference.clone();self.inspector_open=true;}
                                    response.context_menu(|ui|{
                                        if ui.button("Select as A").clicked(){self.left_snapshot=saved.reference.clone();ui.close();}
                                        if ui.button("Select as B").clicked(){self.right_snapshot=saved.reference.clone();ui.close();}
                                        if ui.button("Remove snapshot…").clicked(){self.propose_remove_snapshot(saved);ui.close();}
                                    });
                                }
                            });
                        });
                    }
                });
            } else {
                egui::ScrollArea::both().id_salt("snapshot_forest").auto_shrink([false,false]).show(ui,|ui| {
                    let source_width=self.settings.snapshot_source_width.clamp(140.,420.);
                    let child_x=source_width+92.;
                    let canvas_width=ui.available_width().max(child_x+364.);
                    let live_visible=self.query.is_empty();
                    let height: f32=groups.iter().map(|g|((g.snapshots.len()+usize::from(live_visible&&g.parent==head)).max(1) as f32)*112.+50.).sum::<f32>()+120.;
                    let (canvas,_)=ui.allocate_exact_size(egui::vec2(canvas_width,height.max(250.)),egui::Sense::hover());
                    let origin=canvas.min;
                    ui.painter().text(origin+egui::vec2(24.,24.),egui::Align2::LEFT_TOP,"SOURCE COMMITS",egui::FontId::proportional(12.),MUTED());
                    ui.painter().text(origin+egui::vec2(child_x,24.),egui::Align2::LEFT_TOP,"WORKING COPY & SNAPSHOTS",egui::FontId::proportional(12.),MUTED());
                    let divider=theme::tree_divider(ui,"snapshot_source_divider",origin.x+source_width+58.,origin.y+4.);
                    if divider.dragged(){self.settings.snapshot_source_width=(source_width+divider.drag_delta().x).clamp(140.,420.);ui.ctx().request_repaint();}
                    if divider.drag_stopped()&&!self.preview_mode&&let Err(error)=self.settings.save(){self.report(Err(error));}
                    let mut y=64.;
                    for group in &groups {
                        let live=live_visible&&group.parent==head;
                        let children=(group.snapshots.len()+usize::from(live)).max(1);
                        let block=children as f32*112.;
                        let source=egui::Rect::from_min_size(origin+egui::vec2(24.,y+block/2.-40.),egui::vec2(source_width,80.));
                        let source_response=ui.put(source,egui::Button::new("").fill(SURFACE()).stroke(egui::Stroke::new(1.,CONNECTOR())).corner_radius(8));
                        theme::diagram_focus(ui,&source_response);
                        source_response.widget_info(||egui::WidgetInfo::labeled(egui::Role::Button,ui.is_enabled(),format!("Source commit: {}. Show details",source_title(&repo,group.parent.as_deref()))));
                        if source_response.on_hover_text("Show source commit details").clicked()&&let Some(id)=&group.parent{self.source_details=Some(id.clone());}
                        tree_text(ui,source.shrink2(egui::vec2(14.,10.)),&source_title(&repo,group.parent.as_deref()),14.,INK(),0.);
                        tree_text(ui,source.shrink2(egui::vec2(14.,10.)),if group.parent.is_some(){"View commit details"}else{"No commits yet"},12.,MUTED(),26.);
                        let mut slot=0;
                        if live {
                            let card=egui::Rect::from_min_size(origin+egui::vec2(child_x,y),egui::vec2((canvas_width-child_x-24.).min(450.),86.));
                            connect(ui,source,card,CONNECTOR());
                            let response=tree_node(ui,card,"Working copy",&format!("{} pending changes · {}",repo.changes.len(),repo.branch),"LIVE",false,SUCCESS());
                            if response.on_hover_text("Review current working changes").clicked(){self.page=Page::Changes;}
                            slot+=1;
                        }
                        for saved in &group.snapshots {
                            let card=egui::Rect::from_min_size(origin+egui::vec2(child_x,y+slot as f32*112.),egui::vec2((canvas_width-child_x-24.).min(450.),86.));
                            let selected=self.selected_snapshot==saved.reference;
                            connect(ui,source,card,if selected{SNAPSHOT()}else{CONNECTOR()});
                            let assignment=if self.left_snapshot==saved.reference&&self.right_snapshot==saved.reference{"A + B"}else if self.left_snapshot==saved.reference{"A"}else if self.right_snapshot==saved.reference{"B"}else{"SAVED"};
                            let response=tree_node(ui,card,&saved.label,&saved.date,assignment,selected,SNAPSHOT());
                            let response=response.on_hover_text(format!("{}\n{}\nSelect to inspect this snapshot",saved.label,saved.date));
                            if response.clicked(){self.selected_snapshot=saved.reference.clone();self.inspector_open=true;}
                            response.context_menu(|ui|{
                                if ui.button("Select as A").clicked(){self.left_snapshot=saved.reference.clone();ui.close();}
                                if ui.button("Select as B").clicked(){self.right_snapshot=saved.reference.clone();ui.close();}
                                if ui.button("Inspect snapshot").clicked(){self.selected_snapshot=saved.reference.clone();self.inspector_open=true;ui.close();}
                                if ui.button(RichText::new("Remove snapshot…").color(DANGER())).clicked(){self.propose_remove_snapshot(saved);ui.close();}
                            });
                            slot+=1;
                        }
                        y+=block+50.;
                    }
                    if snapshots.is_empty(){
                        let text=if self.query.is_empty(){"No saved experiments yet. Capture a snapshot to keep this version."}else{"No matching snapshots. Clear the filter to see every capture."};
                        ui.painter().text(origin+egui::vec2(24.,y+10.),egui::Align2::LEFT_TOP,text,egui::FontId::proportional(14.),MUTED());
                    }
                });
            }
        });
        self.capture_dialog(ctx);
        self.source_details_dialog(ctx);
    }

    fn propose_remove_snapshot(&mut self, saved: &git::Snapshot) {
        self.pending = Some(Pending {
            title: "Remove snapshot".into(),
            explanation: format!(
                "Remove “{}” from the saved snapshot tree? It will move to Removed snapshots, where it can be restored. Your working files, branches, and open IDE copies stay intact.",
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

    fn snapshot_inspector(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::ScrollArea::vertical().id_salt("snapshot_details").show(ui,|ui| {
            section(ui,"SNAPSHOT DETAILS");ui.add_space(18.);
            let saved=self.repo.as_ref().and_then(|r|r.snapshots.iter().find(|s|s.reference==self.selected_snapshot)).cloned();
            let Some(saved)=saved else{ui.heading("Select a snapshot");subtitle(ui,"Compare it, open an independent copy, or promote it to a branch.");ui.add_space(20.);badge(ui,"Local to this repository",SNAPSHOT());return;};
            ui.label(RichText::new(&saved.label).size(19.).strong());subtitle(ui,&saved.date);ui.add_space(18.);
            ui.add_space(18.);ui.horizontal(|ui|{
                if ui.button("Use as A").clicked(){self.left_snapshot=saved.reference.clone();}
                if ui.button("Use as B").clicked(){self.right_snapshot=saved.reference.clone();}
            });
            if primary(ui,"Inspect patch").clicked(){self.detail_title=format!("Snapshot · {}",saved.label);self.read("git",process::strings(&["show","--stat","--patch",&saved.id]),ctx);}
            if ui.button("Compare with working tree").clicked(){self.detail_title="Snapshot → working tree".into();self.read("git",process::strings(&["diff",&saved.reference,"--"]),ctx);}
            if ui.button(RichText::new("Remove snapshot…").color(DANGER())).on_hover_text("Move this capture to Removed snapshots. You can restore it later; working files and branches stay intact.").clicked(){self.propose_remove_snapshot(&saved);}
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

fn source_title(repo: &git::RepoState, parent: Option<&str>) -> String {
    parent
        .and_then(|id| {
            repo.commits
                .iter()
                .chain(repo.branch_commits.iter())
                .find(|c| c.id == id)
        })
        .map(|c| c.subject.clone())
        .unwrap_or_else(|| {
            parent
                .map(|_| "Source commit".into())
                .unwrap_or_else(|| "Before first commit".into())
        })
}
fn snapshot_picker(
    ui: &mut egui::Ui,
    id: &str,
    label: &str,
    snapshots: &[git::Snapshot],
    selected: &mut String,
    width: f32,
) {
    ui.label(RichText::new(label).strong().color(SNAPSHOT()));
    egui::ComboBox::from_id_salt(id)
        .width(width)
        .selected_text(
            snapshots
                .iter()
                .find(|s| s.reference == *selected)
                .map(|s| s.label.as_str())
                .unwrap_or("Choose snapshot"),
        )
        .show_ui(ui, |ui| {
            ui.selectable_value(selected, String::new(), "Choose snapshot");
            for s in snapshots {
                ui.selectable_value(selected, s.reference.clone(), &s.label);
            }
        })
        .response
        .on_hover_text(format!("Comparison snapshot {label}"));
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
pub(super) fn tree_node(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    title: &str,
    description: &str,
    state: &str,
    selected: bool,
    color: Color32,
) -> egui::Response {
    let response = ui.put(
        rect,
        egui::Button::new("")
            .fill(if selected { SELECTED() } else { SURFACE() })
            .stroke(egui::Stroke::new(
                if selected { 1.5 } else { 1. },
                if selected { color } else { LINE() },
            ))
            .corner_radius(8),
    );
    theme::diagram_focus(ui, &response);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Button,
            ui.is_enabled(),
            format!("{title}, {description}, {state}"),
        )
    });
    let dot = egui::pos2(rect.left() + 17., rect.top() + 20.);
    ui.painter()
        .circle_stroke(dot, 4., egui::Stroke::new(1.5, color));
    let text = egui::Rect::from_min_max(
        rect.min + egui::vec2(31., 10.),
        rect.max - egui::vec2(12., 10.),
    );
    let mut title_rect = text;
    if matches!(
        state,
        "Remote-tracking" | "No upstream" | "Upstream missing" | "Tracks remote" | "Local upstream"
    ) {
        title_rect.max.x -= 105.;
    }
    tree_text(ui, title_rect, title, 14., INK(), 0.);
    let mut description_rect = text;
    description_rect.max.x = (description_rect.max.x - 135.).max(description_rect.min.x);
    tree_text(ui, description_rect, description, 12., MUTED(), 34.);
    ui.painter().text(
        egui::pos2(rect.right() - 12., rect.top() + 50.),
        egui::Align2::RIGHT_CENTER,
        state,
        egui::FontId::proportional(11.),
        color,
    );
    response
}
fn tree_text(ui: &egui::Ui, rect: egui::Rect, text: &str, size: f32, color: Color32, y: f32) {
    let mut job = egui::text::LayoutJob::simple(
        text.into(),
        egui::FontId::proportional(size),
        color,
        rect.width(),
    );
    job.wrap.max_rows = 1;
    job.wrap.overflow_character = Some('…');
    let galley = ui.painter().layout_job(job);
    ui.painter()
        .with_clip_rect(rect)
        .galley(rect.min + egui::vec2(0., y), galley, color);
}
