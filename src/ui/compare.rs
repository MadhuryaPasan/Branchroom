use super::*;

impl Branchroom {
    pub(super) fn compare(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.comparison.advanced {
            if ui.button("Back to visual comparison").clicked() {
                self.comparison.advanced = false;
            }
            self.compare_advanced(ui, ctx);
        } else {
            self.comparison_canvas(ui, ctx);
        }
    }
    pub(super) fn compare_advanced(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let repo = self.repo.as_ref().unwrap().clone();
        let writable = !branchroom::safety::is_protected(&repo.root);
        egui::ScrollArea::vertical().id_salt("compare_page").show(ui,|ui|{
            heading(ui,"Advanced history operations","Compare branches or refine local commits. Saved versions and recovery are in Time Machine");
            surface().show(ui,|ui|{
                ui.label(RichText::new("Compare branches or commits").size(18.).strong());ui.add_space(12.);
                ui.horizontal_wrapped(|ui|{ui.label("Base");ui.add(egui::TextEdit::singleline(&mut self.compare_base).hint_text("Base branch, tag, or commit").desired_width(200.));ui.label("Head");ui.add(egui::TextEdit::singleline(&mut self.compare_head).hint_text("Compare branch, tag, or commit").desired_width(200.));});
                ui.horizontal_wrapped(|ui|{
                    if primary(ui,"Preview changes").clicked(){if let(Ok(base),Ok(head))=(git::resolve(&repo.root,&self.compare_base),git::resolve(&repo.root,&self.compare_head)){self.detail_title=format!("{} → {}",self.compare_base,self.compare_head);self.read("git",process::strings(&["diff","--stat","--patch",&format!("{base}...{head}")]),ctx);}else{self.report(Err("Enter two existing branch names, tags, or commit IDs.".into()));}}
                    if ui.button("Commits unique to head").clicked() && let(Ok(base),Ok(head))=(git::resolve(&repo.root,&self.compare_base),git::resolve(&repo.root,&self.compare_head)){self.detail_title="Commits in head but not base".into();self.read("git",process::strings(&["log","--oneline",&format!("{base}..{head}")]),ctx);}
                });
            });ui.add_space(18.);
            surface().show(ui,|ui|{
                ui.label(RichText::new("Manage local commit history").size(18.).strong());subtitle(ui,"History rewrites require a clean working tree and refuse commits reachable from remote-tracking branches. Every successful rewrite keeps a recovery ref.");
                ui.add_enabled_ui(writable,|ui|{
                    ui.horizontal_wrapped(|ui|{if ui.button("Undo latest commit").clicked(){self.pending=Some(Pending{title:"Undo latest commit".into(),explanation:"Move the latest unpublished commit back into working changes. A recovery ref preserves the original history.".into(),task:Task::History(branchroom::history::Action::Undo)});}
if ui.button("Reset to selected history commit").clicked()&&!self.selected_commit.is_empty(){self.pending=Some(Pending{title:"Reset branch to commit".into(),explanation:format!("Move the branch back to {} and keep removed commits as working changes. Published commits cannot be reset here.",self.selected_commit),task:Task::History(branchroom::history::Action::Reset(self.selected_commit.clone()))});}});
                    ui.add_space(12.);ui.horizontal(|ui|{ui.label("Recent commits");ui.add(egui::DragValue::new(&mut self.history_steps).range(2..=200)).on_hover_text("Number of recent commits");});
                    ui.label("Combined commit message");ui.add(egui::TextEdit::multiline(&mut self.squash_message).hint_text("Combined commit message").desired_rows(3).desired_width(f32::INFINITY));
                    if ui.button("Review squash").clicked(){self.pending=Some(Pending{title:"Squash local commits".into(),explanation:format!("Combine the last {} linear, unpublished commits. File content and index stay unchanged; a recovery ref preserves the old commits.",self.history_steps),task:Task::History(branchroom::history::Action::Squash{count:self.history_steps,message:self.squash_message.clone()})});}
                    ui.add_space(14.);
                    if ui.button("Load commits for reordering").clicked(){match branchroom::history::linear_tip(&repo.root,self.history_steps){Ok(ids)=>self.reorder_ids=ids,Err(e)=>self.report(Err(e))}}
                    let mut movement=None;
                    for(index,id)in self.reorder_ids.clone().iter().enumerate(){ui.horizontal(|ui|{ui.monospace(format!("{}  {}",index+1,short(id)));ui.label(repo.commits.iter().find(|c|c.id==*id).map(|c|c.subject.as_str()).unwrap_or("Commit"));if index>0&&ui.small_button("Move earlier").clicked(){movement=Some((index,index-1));}
if index+1<self.reorder_ids.len()&&ui.small_button("Move later").clicked(){movement=Some((index,index+1));}});}
                    if let Some((a,b))=movement{self.reorder_ids.swap(a,b);}
                    if !self.reorder_ids.is_empty()&&ui.button("Review reordered history").clicked(){self.pending=Some(Pending{title:"Reorder local commits".into(),explanation:"Replay this oldest-to-newest order using a private index. If a conflict occurs or the final tree differs, your branch is left unchanged.".into(),task:Task::History(branchroom::history::Action::Reorder(self.reorder_ids.clone()))});}
                });
            });ui.add_space(18.);
            if ui.button("Open Time Machine").clicked(){self.page=Page::Snapshots;}
            if !self.detail.is_empty()&&self.detail_page==Page::Compare{section(ui,&self.detail_title);egui::ScrollArea::both().id_salt("compare_patch").max_height(450.).show(ui,|ui|{ui.add(egui::Label::new(RichText::new(&self.detail).monospace()).selectable(true));});}
        });
    }

    pub(super) fn request_switch(&mut self, branch: &str, ctx: &egui::Context) {
        if self
            .repo
            .as_ref()
            .is_some_and(|r| r.changes.is_empty() && r.operation.is_empty())
        {
            self.execute(
                Task::SwitchBranch {
                    branch: branch.into(),
                    stash: false,
                },
                ctx,
            );
            return;
        }
        self.switch_target = Some(branch.into());
        self.switch_stash = false;
    }
    pub(super) fn switch_dialog(&mut self, ctx: &egui::Context) {
        let Some(branch) = self.switch_target.clone() else {
            return;
        };
        let response = egui::Modal::new(egui::Id::unique("branch_switch")).show(ctx, |ui| {
            ui.set_width(470.);
            ui.label(RichText::new(format!("Switch to {branch}")).size(TEXT_HEADER).strong());
            let dirty = self.repo.as_ref().is_some_and(|r| !r.changes.is_empty());
            if dirty {
                ui.label("You have uncommitted changes. Choose what happens to them.");
                ui.radio_value(
                    &mut self.switch_stash,
                    false,
                    "Bring changes to the new branch (Git checks for collisions)",
                );
                ui.radio_value(
                    &mut self.switch_stash,
                    true,
                    "Stash changes on the current branch before switching",
                );
            } else {
                subtitle(ui, "The working tree is clean.");
            }
            ui.add_space(16.);
            ui.horizontal(|ui| {
                if primary(ui, "Switch branch").clicked() {
                    self.switch_target = None;
                    self.execute(
                        Task::SwitchBranch {
                            branch: branch.clone(),
                            stash: dirty && self.switch_stash,
                        },
                        ctx,
                    );
                }
                if ui.button("Cancel").clicked() {
                    self.switch_target = None;
                }
            });
        });
        if response.should_close() {
            self.switch_target = None;
        }
    }
}
