use super::*;

impl Branchroom {
    pub(super) fn merge_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let repo = self.repo.as_ref().unwrap().clone();
        let writable = !branchroom::safety::is_protected(&repo.root);
        egui::ScrollArea::vertical().id_salt("merge_workspace").show(ui,|ui|{
            heading(ui,"Merge branches","Bring changes into the branch you are working on. Review the source and destination before merging.");
            surface().show(ui,|ui|{
                section(ui,"DESTINATION · CURRENT BRANCH");
                ui.label(RichText::new(&repo.branch).size(20.).strong());
                subtitle(ui,"Choose a different destination using Current branch in the top bar.");
                ui.add_space(20.);
                ui.label("Merge from");
                if self.merge_source.is_empty(){self.merge_source=repo.base.clone();}
                egui::ComboBox::from_id_salt("merge_source").selected_text(&self.merge_source).width(310.).show_ui(ui,|ui|{
                    for(remote,label)in[(false,"On this PC"),(true,"On remotes · last fetched")]{
                        section(ui,label);
                        for branch in repo.branches.iter().filter(|b|b.remote==remote&&!b.current&&!b.name.ends_with("/HEAD")){
                            ui.selectable_value(&mut self.merge_source,branch.name.clone(),&branch.name);
                        }
                    }
                }).response.on_hover_text("Merge source branch");
                ui.add_space(12.);
                ui.label(format!("{}  →  {}",self.merge_source,repo.branch));
                subtitle(ui,"A merge keeps existing commits. If the branches diverged, Git may create a merge commit. Nothing is pushed automatically.");
                ui.add_space(12.);
                ui.horizontal_wrapped(|ui|{
                    if ui.button("Preview changes").on_hover_text("Inspect changes introduced by the source since the point where the branches split.").clicked(){
                        self.detail_title=format!("{} → {} · merge preview",self.merge_source,repo.branch);
                        self.read("git",process::strings(&["diff","--stat","--patch",&format!("HEAD...{}",self.merge_source)]),ctx);
                    }
                });
                ui.add_space(20.);
                if !repo.changes.is_empty(){
                    ui.label(RichText::new("Commit or stash working changes before merging.").color(WARNING()));
                    if ui.button("Review working changes").clicked(){self.page=Page::Changes;}
                }
                if !repo.operation.is_empty(){
                    ui.label(RichText::new(format!("Finish the {} already in progress.",repo.operation)).color(WARNING()));
                    if ui.button("Open conflict resolution").clicked(){self.page=Page::Conflicts;}
                }
                if ui.add_enabled(writable&&self.receiver.is_none(),egui::Button::new("Review merge…").fill(PRIMARY())).clicked(){
                    if self.merge_source==repo.branch || self.merge_source.is_empty(){self.report(Err("Choose a different source branch before merging.".into()));}
                    else if !repo.changes.is_empty()||!repo.operation.is_empty(){self.report(Err("Commit or stash changes and finish the current operation, then review the merge again.".into()));}
                    else{self.pending=Some(Pending{title:"Merge branches".into(),explanation:format!("Merge {} into {} using the currently loaded branch tips. Existing commits are preserved. If conflicts occur, resolve them or abort the merge in Conflicts. Nothing is pushed.",self.merge_source,repo.branch),task:Task::Merge{target:repo.branch.clone(),reference:repo.branches.iter().find(|b|b.name==self.merge_source).map(|b|b.id.clone()).unwrap_or_else(||self.merge_source.clone())}});}
                }
            });
            ui.add_space(18.);
            surface().show(ui,|ui|{
                ui.label(RichText::new("Keep up to date with main").strong());
                subtitle(ui,&format!("Compare with {}. Updating from this branch can be done in advanced operations.",repo.base));
                ui.menu_button("Advanced branch operations",|ui|{if ui.add_enabled(writable&&self.receiver.is_none(),egui::Button::new("Update current branch from comparison branch…")).clicked(){self.propose_update(&repo.branch);ui.close();}});
                if ui.button("Choose comparison branch").clicked(){self.page=Page::Branches;}
            });
            if !self.detail.is_empty()&&self.detail_page==Page::Merge{
                ui.add_space(18.);section(ui,&self.detail_title);
                egui::ScrollArea::both().id_salt("merge_preview").max_height(400.).show(ui,|ui|{ui.add(egui::Label::new(RichText::new(&self.detail).monospace()).selectable(true));});
            }
        });
    }
}
