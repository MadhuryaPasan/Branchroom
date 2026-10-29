use super::*;
#[derive(Default)]
pub(super) struct Timeline {
    pub selected: String,
    pub target: String,
    pub graph: Vec<git::Commit>,
    pub patch: String,
    pub loading: bool,
    pub plan: Option<Box<branchroom::timeline::RestorePlan>>,
    filter: String,
    before: bool,
    advanced: bool,
}
impl Branchroom {
    pub(super) fn timeline_label(task: &Task) -> String {
        match task {
            Task::GraphApply(p) => match &p.kind {
                branchroom::operation::Kind::Merge { .. } => "Merge branches",
                branchroom::operation::Kind::CherryPick { .. } => "Copy commit to branch",
                branchroom::operation::Kind::Commit { .. } => "Commit changes",
                branchroom::operation::Kind::Amend { .. } => "Update commit",
                branchroom::operation::Kind::Reset { .. } | branchroom::operation::Kind::Undo => {
                    "Move branch tip"
                }
                branchroom::operation::Kind::Reorder { .. } => "Reorder commits",
                branchroom::operation::Kind::MoveToBranch { .. }
                | branchroom::operation::Kind::MoveCommits { .. } => "Move commits to a new branch",
                _ => "Combine commits",
            }
            .into(),
            Task::Snapshot(label) => format!("Save safety point: {label}"),
            Task::TimelineRestore(p) => p.label.clone(),
            Task::Shelved { action, .. } => format!(
                "{} shelved work",
                match action.as_str() {
                    "pop" => "Apply",
                    "apply" => "Apply and keep",
                    _ => "Delete",
                }
            ),
            Task::Discard(path) => format!("Discard changes to {path}"),
            Task::RestoreDiscard(_) => "Recover discarded file".into(),
            Task::Command { program, args, .. } => format!(
                "{} {}",
                program,
                args.first().map(String::as_str).unwrap_or("action")
            ),
            Task::RawGit(args) => format!(
                "Git {}",
                args.first().map(String::as_str).unwrap_or("action")
            ),
            Task::Resolve { .. } => "Save conflict resolution".into(),
            Task::RemoveSnapshot(_) => "Delete safety point".into(),
            Task::RestoreSnapshot(_) => "Recover safety point".into(),
            Task::Promote { branch, .. } => format!("Create branch {branch}"),
            Task::Revert { .. } => "Revert commit".into(),
            _ => "Update repository".into(),
        }
    }
    pub(super) fn timeline_undo(&mut self, redo: bool, ctx: &egui::Context) {
        if self.receiver.is_some() {
            return;
        }
        match branchroom::timeline::candidate(&self.path(), redo) {
            Ok(entry) => self.execute(Task::TimelineUndo { redo, id: entry.id }, ctx),
            Err(error) => self.report(Err(error)),
        }
    }
    pub(super) fn time_machine_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(repo) = self.repo.clone() else {
            return;
        };
        if !self.settings.advanced_mode {
            self.time_machine.advanced = false;
        }
        let writable = self.receiver.is_none() && !branchroom::safety::is_protected(&repo.root);
        let mut items = repo.timeline.clone();
        // Preview mode stays offline, with existing sample captures mapped into entries.
        if self.preview_mode && items.is_empty() {
            for saved in repo
                .snapshots
                .iter()
                .chain(&repo.recovery_heads)
                .chain(&repo.removed_snapshots)
            {
                items.push(branchroom::timeline::Item {
                    key: saved.reference.clone(),
                    reference: saved.reference.clone(),
                    target: saved.id.clone(),
                    label: saved.label.clone(),
                    date: saved.date.clone(),
                    kind: "Safety point".into(),
                    ..Default::default()
                });
            }
            for (reference, label) in &repo.stashes {
                items.push(branchroom::timeline::Item {
                    key: reference.clone(),
                    reference: reference.clone(),
                    label: label.clone(),
                    kind: "Shelved work".into(),
                    ..Default::default()
                });
            }
        }
        items.retain(|item| !item.hidden);
        if self.time_machine.selected.is_empty() && !self.selected_snapshot.is_empty() {
            self.time_machine.selected = self.selected_snapshot.clone();
        }
        let selected = items
            .iter()
            .find(|item| item.key == self.time_machine.selected)
            .cloned();
        if let Some(plan) = self.time_machine.plan.clone() {
            egui::Panel::bottom("timeline_restore_confirmation").show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(
                            writable,
                            egui::Button::new("Confirm restore").fill(PRIMARY()),
                        )
                        .clicked()
                    {
                        self.time_machine.plan = None;
                        self.execute(Task::TimelineRestore(plan), ctx);
                    }
                    if ui.button("Cancel restore").clicked() {
                        self.time_machine.plan = None;
                    }
                });
            });
        }
        let short = ctx.content_rect().height() < 500.;
        egui::Panel::top("timeline_controls")
            .frame(
                egui::Frame::new()
                    .fill(PAPER())
                    .inner_margin(if short { 12 } else { 20 }),
            )
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.heading("Time Machine");
                    if ui
                        .add_enabled(
                            writable,
                            egui::Button::new("Save safety point").fill(
                                if self.time_machine.plan.is_some() {
                                    SURFACE()
                                } else {
                                    PRIMARY()
                                },
                            ),
                        )
                        .clicked()
                    {
                        self.capture_open = true;
                    }
                    if ui
                        .add_enabled(writable, egui::Button::new("Undo last action"))
                        .on_hover_text(
                            "Restore the state before the last Branchroom action · Ctrl+Z",
                        )
                        .clicked()
                    {
                        self.timeline_undo(false, ctx);
                    }
                    if ui
                        .add_enabled(writable, egui::Button::new("Redo action"))
                        .on_hover_text("Repeat the last undone Branchroom action · Ctrl+Shift+Z")
                        .clicked()
                    {
                        self.timeline_undo(true, ctx);
                    }
                });
                if !short {
                    subtitle(
                        ui,
                        "Your actions, saved versions and recovered files in one timeline",
                    );
                }
                ui.horizontal_wrapped(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.time_machine.filter)
                            .desired_width(230.)
                            .hint_text("Find an action, file or saved version"),
                    );
                    if ui.button("Shelve work").clicked() {
                        self.form(
                            "Shelve work",
                            "Save included, working and untracked changes for later",
                            "git",
                            &["stash", "push", "--include-untracked", "-m"],
                            &[("Name", "")],
                            &[],
                        );
                    }
                    if self.settings.advanced_mode
                        && ui
                            .selectable_label(self.time_machine.advanced, "Advanced")
                            .clicked()
                    {
                        self.time_machine.advanced = !self.time_machine.advanced;
                    }
                });
            });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(PAPER()).inner_margin(16)).show(ui,|ui|{
            egui::ScrollArea::vertical().id_salt("action_timeline").show(ui,|ui|{
                if let Some(plan)=self.time_machine.plan.clone(){
                    surface().show(ui,|ui|{
                        ui.heading("Preview restore");ui.label(&plan.label);
                        subtitle(ui,"A safety point is saved before restoring. Newer changes must still match this preview");
                        ui.label(format!("{} → {}",plan.from.branch.as_deref().unwrap_or("Detached commit").trim_start_matches("refs/heads/"),plan.to.branch.as_deref().unwrap_or("Detached commit").trim_start_matches("refs/heads/")));
                        ui.label(&plan.summary);
                        section(ui,"Before restore");timeline_graph(ui,&plan.before_graph);
                        section(ui,"After restore");timeline_graph(ui,&plan.graph);
                        if plan.changes.is_empty(){subtitle(ui,"No working file differences");}else{ui.add(egui::Label::new(RichText::new(&plan.changes).monospace()).wrap());}
                        egui::CollapsingHeader::new("Show Git command").show(ui,|ui|{ui.label("Branchroom restores saved branch tips, included changes and working files using a checked reference transaction and a two-tree checkout");ui.add(egui::Label::new(RichText::new(plan.git_command()).monospace()).wrap().selectable(true));});
                    });ui.add_space(16.);
                }
                if let Some(item)=&selected{
                    let target=if self.time_machine.target.is_empty(){&item.target}else{&self.time_machine.target}.clone();
                    let branch_target=if !item.action.is_empty(){self.time_machine.graph.first().map(|c|c.id.as_str()).unwrap_or(&target)}else{&target}.to_owned();
                    surface().show(ui,|ui|{
                        ui.label(RichText::new(&item.label).strong().size(18.));subtitle(ui,&item.date);
                        if self.time_machine.loading{ui.spinner();ui.label("Loading saved graph and changes");}else if !self.time_machine.graph.is_empty(){timeline_graph(ui,&self.time_machine.graph);}
                        ui.horizontal_wrapped(|ui|{
                            if !item.action.is_empty()&&ui.checkbox(&mut self.time_machine.before,"Before this action").changed(){self.time_machine.loading= !self.preview_mode;self.execute(Task::TimelineLoad{item:item.clone(),before:self.time_machine.before},ctx);}
                            if ui.add_enabled(writable&&!item.target.is_empty()&&item.kind!="Shelved work"&&item.kind!="Discarded file",egui::Button::new("Restore…")).clicked(){self.execute(Task::TimelinePlan{item:item.clone(),before:self.time_machine.before},ctx);}
                            if ui.add_enabled(!self.time_machine.loading&&!item.target.is_empty(),egui::Button::new("Compare")).clicked(){self.page=Page::Compare;self.compare_base=target.clone();self.compare_head="HEAD".into();self.comparison.advanced=false;}
                            if ui.add_enabled(writable&&!item.target.is_empty(),egui::Button::new("Branch from here…")).clicked(){self.form("Branch from here","Create a new branch at this saved version","git",&["branch"],&[("Branch name","")],&[&branch_target]);}
                            if item.kind!="Shelved work"&&ui.add_enabled(writable,egui::Button::new(RichText::new("Delete entry").color(DANGER()))).on_hover_text("Hide this entry while retaining its recovery data").clicked(){self.execute(Task::TimelineHide{id:item.key.clone(),hidden:true},ctx);}
                            if item.kind=="Safety point"&&ui.add_enabled(writable,egui::Button::new(RichText::new("Delete safety point").color(DANGER()))).clicked(){self.pending=Some(Pending{title:"Delete safety point".into(),explanation:"Keep its data as a deleted safety point on this timeline so it can be recovered".into(),task:Task::RemoveSnapshot(item.reference.clone())});}
                            if item.kind=="Deleted safety point"&&ui.add_enabled(writable,egui::Button::new("Recover safety point")).clicked(){self.execute(Task::RestoreSnapshot(item.reference.clone()),ctx);}
                            if item.kind=="Discarded file"&&ui.add_enabled(writable,egui::Button::new("Recover file")).clicked(){self.pending=Some(Pending{title:"Recover file".into(),explanation:"Restore this saved file. The current working version is saved first".into(),task:Task::RestoreDiscard(item.reference.clone())});}
                        });
                        if item.kind=="Shelved work" {
                            ui.horizontal_wrapped(|ui|{
                                for (label,action) in [("Apply","pop"),("Apply and keep","apply"),("Delete","drop")]{
                                    if ui.add_enabled(writable,egui::Button::new(if action=="drop"{RichText::new(label).color(DANGER())}else{RichText::new(label)})).clicked(){self.pending=Some(Pending{title:format!("{label} shelved work"),explanation:"Save the current state before changing shelved work. Conflicts may need resolution".into(),task:Task::Shelved{reference:item.reference.clone(),id:item.target.clone(),action:action.into()}});}
                                }
                            });
                            section(ui,"Saved changes");
                            egui::ScrollArea::both().id_salt("shelved_inline_diff").max_height(220.).show(ui,|ui|{ui.add(egui::Label::new(RichText::new(&self.time_machine.patch).monospace()).selectable(true));});
                        }else if !self.time_machine.patch.is_empty(){egui::CollapsingHeader::new("Changed files and patch").show(ui,|ui|{egui::ScrollArea::both().max_height(200.).show(ui,|ui|{ui.add(egui::Label::new(RichText::new(&self.time_machine.patch).monospace()).selectable(true));});});}
                    });ui.add_space(18.);
                }
                let query=self.time_machine.filter.to_lowercase();
                let filtered=items.iter().filter(|item|format!("{} {} {}",item.label,item.kind,item.date).to_lowercase().contains(&query)).collect::<Vec<_>>();
                if filtered.is_empty(){ui.heading("Your next action starts the timeline");subtitle(ui,"Save a safety point, commit changes, or shelve unfinished work. Existing saved versions stay available here");}
                for item in filtered {
                    ui.horizontal_top(|ui|{
                        let (rect,_)=ui.allocate_exact_size(egui::vec2(20.,68.),egui::Sense::hover());
                        ui.painter().line_segment([rect.center_top(),rect.center_bottom()],egui::Stroke::new(1.,LINE()));
                        ui.painter().circle_filled(rect.center_top()+egui::vec2(0.,16.),4.,if item.key==self.time_machine.selected{ACCENT()}else{MUTED()});
                        ui.vertical(|ui|{
                            ui.set_min_width(ui.available_width().max(0.));
                            let response=ui.add(egui::Button::selectable(item.key==self.time_machine.selected,RichText::new(&item.label).strong()).wrap());
                            if response.clicked(){self.time_machine.selected=item.key.clone();self.time_machine.target.clear();self.time_machine.graph.clear();self.time_machine.patch.clear();self.time_machine.loading= !self.preview_mode;self.time_machine.plan=None;self.time_machine.before=false;self.selected_snapshot=item.reference.clone();self.execute(Task::TimelineLoad{item:item.clone(),before:false},ctx);}
                            ui.add(egui::Label::new(RichText::new(format!("{} · {}",item.kind,item.date)).color(MUTED())).wrap());
                        });
                    });
                }
                if self.time_machine.advanced {
                    egui::CollapsingHeader::new("Open a saved version in a separate workspace").default_open(true).show(ui,|ui|self.snapshot_inspector(ui,ctx));
                    egui::CollapsingHeader::new("Show deleted action entries").show(ui,|ui|{
                        for item in repo.timeline.iter().filter(|e|e.hidden){if ui.button(format!("Recover entry: {}",item.label)).clicked(){self.execute(Task::TimelineHide{id:item.key.clone(),hidden:false},ctx);}}
                    });
                }
            });
        });
    }
}
fn timeline_graph(ui: &mut egui::Ui, commits: &[git::Commit]) {
    let rows = graph::layout(commits);
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(width, (rows.len().min(12) * 28 + 12) as f32),
        egui::Sense::hover(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Image,
            ui.is_enabled(),
            format!(
                "Saved commit graph: {}",
                commits
                    .iter()
                    .take(12)
                    .map(|c| format!("{} {}", c.subject, c.refs))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        )
    });
    let gutter = ((rows.iter().map(|r| r.lane).max().unwrap_or(0) + 1) as f32 * 14. + 20.)
        .min(width * 0.4)
        .max(35.);
    for (index, (row, commit)) in rows.iter().zip(commits).take(12).enumerate() {
        let y = rect.top() + 14. + index as f32 * 28.;
        let x = rect.left() + 10. + row.lane as f32 * 14.;
        for (from, to) in &row.edges {
            let a = egui::pos2(rect.left() + 10. + *from as f32 * 14., y);
            let b = egui::pos2(rect.left() + 10. + *to as f32 * 14., y + 28.);
            if b.y <= rect.bottom() {
                ui.painter()
                    .line_segment([a, b], egui::Stroke::new(1.5, lane_color(*from)));
            }
        }
        ui.painter()
            .circle_filled(egui::pos2(x, y), 3.5, lane_color(row.lane));
        let text = egui::Rect::from_min_max(
            egui::pos2(rect.left() + gutter, y - 8.),
            egui::pos2(rect.right(), y + 14.),
        );
        ui.painter().with_clip_rect(text).text(
            text.left_top(),
            egui::Align2::LEFT_TOP,
            if commit.refs.is_empty() {
                commit.subject.clone()
            } else {
                format!("{} · {}", commit.subject, commit.refs)
            },
            egui::FontId::proportional(13.),
            INK(),
        );
    }
    if commits.len() > 12 {
        subtitle(ui, "Showing the nearest 12 commits at this point");
    }
}
