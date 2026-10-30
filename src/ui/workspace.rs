use super::*;
// A tracked remote is displayed with its local branch, not as another branch card.
fn visible_branches(branches: &[git::Branch], filter: &str) -> Vec<git::Branch> {
    let tracked: std::collections::HashSet<&str> = branches
        .iter()
        .filter(|b| !b.remote && !b.upstream.is_empty())
        .map(|b| b.upstream.as_str())
        .collect();
    let filter = filter.trim().to_lowercase();
    branches
        .iter()
        .filter(|b| !(b.remote && tracked.contains(b.name.as_str())))
        .filter(|b| {
            filter.is_empty()
                || b.name.to_lowercase().contains(&filter)
                || (!b.remote && b.upstream.to_lowercase().contains(&filter))
        })
        .cloned()
        .collect()
}

// Git supplies tracking status separately from counts against the comparison branch.
// Unrecognized or missing tracking information is shown as unknown, never as in sync.
fn tracking_counts(tracking: &str) -> Option<(usize, usize)> {
    if tracking.is_empty() {
        return Some((0, 0));
    }
    let value = tracking.strip_prefix('[')?.strip_suffix(']')?;
    let mut result = (0, 0);
    for part in value.split(',').map(str::trim) {
        if let Some(n) = part.strip_prefix("ahead ") {
            result.0 = n.parse().ok()?;
        } else {
            let n = part.strip_prefix("behind ")?;
            result.1 = n.parse().ok()?;
        }
    }
    Some(result)
}

fn branch_explanation(
    branch: &git::Branch,
    remote: Option<&git::Branch>,
    current_counts: Option<(usize, usize)>,
) -> (String, String, Color32) {
    if branch.remote {
        return (
            "This branch is only on the remote".into(),
            "Switch to it to create a branch on this computer".into(),
            ACCENT(),
        );
    }
    let Some(remote) = remote else {
        if branch.upstream.is_empty() {
            return (
                "Only on this computer".into(),
                "No remote branch linked".into(),
                MUTED(),
            );
        }
        if branch.tracking.contains("gone") {
            return (
                "Linked remote branch is missing".into(),
                format!(
                    "Previously linked to {} · check for updates",
                    branch.upstream
                ),
                WARNING(),
            );
        }
        return (
            "Only on this computer".into(),
            format!("Compared with local branch {}", branch.upstream),
            MUTED(),
        );
    };
    let counts = if !branch.id.is_empty() && branch.id == remote.id {
        Some((0, 0))
    } else if branch.current {
        current_counts.or_else(|| {
            if branch.tracking.is_empty() {
                None
            } else {
                tracking_counts(&branch.tracking)
            }
        })
    } else if branch.tracking.is_empty() {
        None
    } else {
        tracking_counts(&branch.tracking)
    };
    let counts = if branch.id != remote.id && counts == Some((0, 0)) {
        None
    } else {
        counts
    };
    let (status, color) = match counts {
        Some((0, 0)) => ("This computer and remote match".into(), SUCCESS()),
        Some((a, 0)) => (
            format!("{a} {} to send", if a == 1 { "commit" } else { "commits" }),
            ACCENT(),
        ),
        Some((0, b)) => (
            format!(
                "{b} {} to receive",
                if b == 1 { "commit" } else { "commits" }
            ),
            WARNING(),
        ),
        Some((a, b)) => (
            format!("{a} to send · {b} to receive · needs attention"),
            WARNING(),
        ),
        None => ("Remote comparison is unavailable".into(), WARNING()),
    };
    (
        status,
        format!("Remote: {} · as of last fetch", remote.name),
        color,
    )
}

fn branch_card(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    title: &str,
    status: &str,
    location: &str,
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
            .corner_radius(8)
            .sense(egui::Sense::click_and_drag()),
    );
    theme::diagram_focus(ui, &response);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Button,
            ui.is_enabled(),
            format!("{title}, {status}, {location}"),
        )
    });
    ui.painter().circle_stroke(
        rect.min + egui::vec2(17., 20.),
        4.,
        egui::Stroke::new(1.5, color),
    );
    for (text, y, size, text_color, reserve) in [
        (title, 10., 14., INK(), 125.),
        (status, 34., 12., color, 12.),
        (location, 57., 12., MUTED(), 125.),
    ] {
        let clip = egui::Rect::from_min_max(
            rect.min + egui::vec2(31., y),
            egui::pos2(
                (rect.right() - reserve).max(rect.left() + 32.),
                rect.bottom(),
            ),
        );
        let mut job = egui::text::LayoutJob::simple(
            text.to_owned(),
            egui::FontId::proportional(size),
            text_color,
            clip.width(),
        );
        job.wrap.max_rows = 1;
        job.wrap.overflow_character = Some('…');
        let galley = ui.painter().layout_job(job);
        ui.painter()
            .with_clip_rect(clip)
            .galley(clip.min, galley, text_color);
    }
    response
}

impl Branchroom {
    pub(super) fn branches(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let repo = self.repo.as_ref().unwrap().clone();
        let compact = ui.available_width() < 700.;
        egui::Panel::top("branch_controls").frame(egui::Frame::new().fill(PAPER()).inner_margin(if compact{10}else{20})).show(ui,|ui|{
            ui.horizontal_wrapped(|ui|{
                ui.label(RichText::new("Branch map").size(TEXT_HEADER).strong());
                ui.menu_button("Clean up branches",|ui| {
                    subtitle(ui,"Review merged branches below. Deleting saves a Time Machine checkpoint first.");ui.checkbox(&mut self.cleanup_branches,"Highlight merged branches");
                    for branch in repo.branches.iter().filter(|b|!b.current&&!b.remote&&b.ahead==0) {
                        if ui.button(format!("Delete {}",branch.name)).clicked(){self.pending=Some(Pending{title:"Delete merged branch".into(),explanation:format!("Save {} in Time Machine, then delete it only if all its commits are in the current branch.",branch.name),task:Task::DeleteMerged{branch:branch.name.clone(),expected:branch.id.clone()}});ui.close();}
                    }
                });
                if primary(ui,"New branch").clicked(){self.form("Create branch","Create and switch to a branch from the current HEAD.","git",&["switch","-c"],&[("Branch name","")],&[]);}
            });
            if !compact { ui.add_space(8.); }
            if compact {
                ui.horizontal_wrapped(|ui|{
                    ui.label(format!("Compare against: {}",shorten_label(&self.base_input,24)));
                    egui::containers::menu::MenuButton::new("Sync & comparison")
                        .config(egui::containers::menu::MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside))
                        .ui(ui,|ui|self.branch_sync_controls(ui,ctx,&repo));
                });
            }else{
                self.branch_sync_controls(ui,ctx,&repo);
                ui.add_space(5.);subtitle(ui,"Each card brings a branch on this computer and its linked remote together. Remote information is from the last fetch.");
            }
        });
        self.patch_drawer(ui, false);
        self.graph_inspector(ui, ctx);
        egui::CentralPanel::default().frame(egui::Frame::new().fill(PAPER()).inner_margin(16)).show(ui,|ui|{
            ui.horizontal_wrapped(|ui|{ui.label("Filter branches");ui.add(egui::TextEdit::singleline(&mut self.branch_filter).desired_width(220.).hint_text("feature/search"));if !self.branch_filter.is_empty()&&ui.button("Clear").clicked(){self.branch_filter.clear();}
                ui.menu_button("Tree layout",|ui|{
                    subtitle(ui,"Set the width of the ancestor column.");
                    let changed=super::history::column_input(ui,"Ancestor",&mut self.settings.branch_source_width,140.0..=420.0);
                    let reset=ui.button("Reset tree layout").clicked();if reset{self.settings.branch_source_width=205.;}
                    if (changed||reset)&&!self.preview_mode&&let Err(error)=self.settings.save(){self.report(Err(error));}
                });
            });ui.add_space(12.);
            let branches = visible_branches(&repo.branches, &self.branch_filter);
            let mut groups:Vec<(String,Vec<git::Branch>)>=vec![];
            for branch in branches {if let Some(group)=groups.iter_mut().find(|g|g.0==branch.fork_point){group.1.push(branch);}else{groups.push((branch.fork_point.clone(),vec![branch]));}}
            egui::ScrollArea::both().id_salt("branch_forest").auto_shrink([false,false]).show(ui,|ui|{
                let source_width=self.settings.branch_source_width.clamp(140.,420.);
                let child_x=source_width+60.;
                let total_height=groups.iter().map(|(_,branches)|branches.len() as f32*108.+22.).sum::<f32>()+36.;
                let canvas_width=ui.available_width().max(child_x+375.);
                let (canvas,_)=ui.allocate_exact_size(egui::vec2(canvas_width,total_height),egui::Sense::hover());
                ui.painter().text(canvas.min+egui::vec2(8.,10.),egui::Align2::LEFT_TOP,"Starting points",egui::FontId::proportional(11.),MUTED());
                ui.painter().text(canvas.min+egui::vec2(child_x+10.,10.),egui::Align2::LEFT_TOP,"Branches",egui::FontId::proportional(11.),MUTED());
                let divider=theme::tree_divider(ui,"branch_source_divider",canvas.left()+source_width+30.,canvas.top());
                if divider.dragged(){self.settings.branch_source_width=(source_width+divider.drag_delta().x).clamp(140.,420.);ui.ctx().request_repaint();}
                if divider.drag_stopped()&&!self.preview_mode&&let Err(error)=self.settings.save(){self.report(Err(error));}
                let mut group_top=36.;
                for (ancestor,branches) in groups {
                    let group_height=branches.len() as f32*108.;let width=canvas_width;
                    let rect=egui::Rect::from_min_size(canvas.min+egui::vec2(0.,group_top),egui::vec2(width,group_height+22.));
                    group_top+=group_height+22.;
                    let source=egui::Rect::from_min_size(rect.min+egui::vec2(0.,group_height/2.-36.),egui::vec2(source_width,72.));
                    let response=ui.put(source,egui::Button::new("").fill(SURFACE()).stroke(egui::Stroke::new(1.,CONNECTOR())).corner_radius(8));
                    theme::diagram_focus(ui,&response);
                    response.widget_info(||egui::WidgetInfo::labeled(egui::Role::Button,ui.is_enabled(),format!("Branched off here {}",ancestor)));
                    if response.clicked()&&!ancestor.is_empty(){self.view_commit(&ancestor,ctx); }
                    ui.painter().text(source.min+egui::vec2(14.,12.),egui::Align2::LEFT_TOP,if ancestor.is_empty(){"Independent history"}else{"Branched off here"},egui::FontId::proportional(14.),MUTED());
                    ui.painter().text(source.min+egui::vec2(14.,37.),egui::Align2::LEFT_TOP,if ancestor.is_empty(){"No common commit"}else{"View source details"},egui::FontId::monospace(12.),ACCENT());
                    for (index,branch) in branches.iter().enumerate(){
                        let branch_ref=format!("refs/{}/{}",if branch.remote{"remotes"}else{"heads"},branch.name);
                        let card=egui::Rect::from_min_size(rect.min+egui::vec2(child_x,index as f32*108.),egui::vec2((width-child_x-15.).min(720.),88.));
                        let remote = repo.branches.iter().find(|b| b.remote && b.name == branch.upstream);
                        let (status, mut location, color) = branch_explanation(branch, remote, repo.sync_counts);
                        if let Some(risk)=self.conflict_risk(&branch_ref,&format!("refs/heads/{}",repo.branch)).or_else(||self.conflict_risk(&format!("refs/heads/{}",repo.branch),&branch_ref)){location.push_str(&format!(" - Forecast: {risk} conflicts against {}",repo.branch));}
                        if let Some(Ok(data))=&self.graph_context.data { for pr in data.reviews.iter().filter(|p|p.branch==branch.name){location.push_str(&format!(" - PR #{} {} - {}",pr.number,pr.state,pr.ci));} }
                        if !ancestor.is_empty(){
                            let lane=repo.commits.iter().position(|c|c.id==branch.id).and_then(|i|self.graph.get(i)).map(|r|r.lane).unwrap_or(index);
                            let risk=self.conflict_risk(&branch_ref,&format!("refs/heads/{}",repo.branch)).or_else(||self.conflict_risk(&format!("refs/heads/{}",repo.branch),&branch_ref));
                            super::experiments::connect(ui,source,card,risk.map(|n|if n==0{SUCCESS()}else{DANGER()}).unwrap_or_else(||lane_color(lane)));
                        }
                        if !branch.current&&!branch.remote&&branch.ahead==0&&branch.name!=repo.base { location.push_str(" - Merged into comparison branch"); }
                        if let Some(commit)=repo.commits.iter().find(|c|c.id==branch.id) {
                            let age=commit.date.split_whitespace().next().and_then(|n|n.parse::<usize>().ok()).unwrap_or(0);
                            if commit.date.contains("year")||(commit.date.contains("month")&&age>=3){location.push_str(" - Stale");}
                        }
                        let name=format!("{}{}",branch.name,if branch.current{"  · current"}else{""});
                        let response=branch_card(ui,card,&name,&status,&location,self.selected_commit==branch.id||(self.cleanup_branches&&!branch.current&&!branch.remote&&branch.ahead==0),color);
                        response.dnd_set_drag_payload(manipulation::Payload::Branch{name:branch.name.clone(),current:branch.current,reference:branch_ref.clone()});
                        self.graph_drop(ui,&response,manipulation::Target::Branch(&branch_ref));
                        if let Some(workspace) = repo.workspaces.iter().find(|workspace| workspace.branch == branch.name) {
                            let desk_name = workspace.path.file_name().unwrap_or(workspace.path.as_os_str()).to_string_lossy();
                            ui.scope_builder(egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(egui::pos2(card.right()-245.,card.top()+8.),egui::vec2(120.,32.))),|ui| {
                                let open = ui.button("Open folder").on_hover_text(format!(
                                    "Open this branch's separate Git worktree in its IDE: {} ({desk_name})",
                                    workspace.path.display()
                                ));
                                open.widget_info(|| egui::WidgetInfo::labeled(
                                    egui::Role::Button,
                                    ui.is_enabled(),
                                    format!("Open worktree folder {desk_name} for {}", branch.name),
                                ));
                                if open.clicked() {
                                    let result = self.settings.open(&workspace.path,self.settings.ide_for(&workspace.path));
                                    self.report(result);
                                }
                            });
                        }
                        let main=repo.default_branch.as_ref().and_then(|name|repo.branches.iter().find(|b|b.remote&&b.name==format!("origin/{name}")).or_else(||repo.branches.iter().find(|b|!b.remote&&b.name==*name)));
                        let source=main.map(|b|format!("refs/{}/{}",if b.remote{"remotes"}else{"heads"},b.name)).unwrap_or_else(||repo.base.clone());
                        if !branch.remote&&main.is_none_or(|b|b.id!=branch.id) {
                            ui.scope_builder(egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(egui::pos2(card.right()-128.,card.top()+48.),egui::vec2(118.,28.))),|ui| {
                                if ui.button("Update branch").on_hover_text(format!("Preview updating {} from the default branch",branch.name)).clicked(){self.begin_graph_action(branchroom::operation::Kind::Merge{source:source.clone(),target:format!("refs/heads/{}",branch.name)});}
                            });
                        }
                        let total=(branch.ahead+branch.behind).max(1) as f32;
                        let bar=egui::Rect::from_min_size(card.left_bottom()+egui::vec2(30.,-6.),egui::vec2((card.width()-48.).max(0.),3.));
                        ui.painter().rect_filled(bar,1.,LINE());
                        if branch.ahead>0 { ui.painter().rect_filled(egui::Rect::from_min_size(bar.min,egui::vec2(bar.width()*branch.ahead as f32/total,3.)),1.,SUCCESS()); }
                        if branch.behind>0 { ui.painter().rect_filled(egui::Rect::from_min_size(egui::pos2(bar.right()-bar.width()*branch.behind as f32/total,bar.top()),egui::vec2(bar.width()*branch.behind as f32/total,3.)),1.,WARNING()); }
                        if response.on_hover_text(format!("{}\n{}\n{}",branch.name,status,location)).clicked(){self.selected_commit=branch.id.clone();self.inspector_open=true;}
                        ui.scope_builder(egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(egui::pos2(card.right()-116.,card.top()+8.),egui::vec2(105.,32.))),|ui|{
                            let actions = ui.menu_button("Actions",|ui|{
                            ui.label(branchroom::collaboration::branch_status(branch,&repo.branches));
                            if ui.button("Open branch code in IDE…").clicked(){self.begin_review(&branch.id,None);ui.close();}
                            if let Some(remote) = remote { ui.menu_button(format!("Remote branch: {}", remote.name), |ui| {
                                subtitle(ui, "The remote branch as of your last fetch.");
                                if ui.button("View remote commit").clicked(){self.view_commit(&remote.id,ctx);ui.close();}
                                if ui.button("Open remote code in IDE…").clicked(){self.begin_review(&remote.id,None);ui.close();}
                                if ui.button("Switch to remote branch…").clicked(){self.request_switch(&remote.name,ctx);ui.close();}
                                if ui.add_enabled(remote.id != repo.head, egui::Button::new("Merge remote into current…")).on_disabled_hover_text("The remote and current branch already point to the same commit.").clicked(){self.propose("Merge branch","Merge this remote branch into the checked-out branch.",&["merge","--no-edit",&remote.name]);ui.close();}
                                if ui.button("Compare remote with comparison branch").clicked(){self.detail_title=format!("{} → {}",repo.base,remote.name);self.read("git",process::strings(&["diff","--stat",&format!("{}...{}",repo.base,remote.name)]),ctx);ui.close();}
                            }); }

                            if !branch.remote && self.settings.advanced_mode {
                                let advanced = ui.menu_button("Advanced",|ui|{
                                    if ui.button("Update this branch from comparison branch…").clicked(){self.propose_update(&branch.name);ui.close();}
                                    if ui.button("Publish / push this branch…").clicked(){self.form("Publish branch","Push the selected local branch and set its upstream. Review the destination remote.","git",&["push","--progress","--set-upstream"],&[("Remote","origin"),("Branch",&branch.name)],&[]);ui.close();}
                                });
                                advanced.response.widget_info(|| egui::WidgetInfo::labeled(
                                    egui::Role::Button,
                                    ui.is_enabled(),
                                    format!("Advanced actions for {}", branch.name),
                                ));
                            }
                            if ui.button(if branch.current{"Show commit history"}else{"Switch & show commit history"}).clicked(){if !branch.current{self.request_switch(&branch.name,ctx);}self.page=Page::History;ui.close();}
                            if !branch.current&&ui.button("Switch branch").clicked(){self.request_switch(&branch.name,ctx);ui.close();}
                            if !branch.remote&&ui.button("Rename branch…").clicked(){self.form("Rename branch","Rename this local branch.","git",&["branch","-m",&branch.name],&[("New branch name","")],&[]);ui.close();}
                            if !branch.current&&ui.button("Merge into current").clicked(){self.propose("Merge branch","Merge this branch into the checked-out branch.",&["merge","--no-edit",&branch.name]);ui.close();}
                            if ui.button("Compare branches").clicked(){self.detail_title=format!("{} → {}",repo.base,branch.name);self.read("git",process::strings(&["diff","--stat",&format!("{}...{}",repo.base,branch.name)]),ctx);ui.close();}
                            if !branch.current&&!branch.remote&&ui.button(RichText::new("Delete merged branch").color(DANGER())).clicked(){self.pending=Some(Pending { title:"Delete merged branch".into(), explanation:format!("Delete {} after saving its tip in Time Machine. Git will refuse if its commits are missing from the current branch.",branch.name), task:Task::DeleteMerged {branch:branch.name.clone(),expected:branch.id.clone()} });ui.close();}
                            });
                            actions.response.widget_info(|| egui::WidgetInfo::labeled(
                                egui::Role::Button,
                                ui.is_enabled(),
                                format!("Actions for {}", branch.name),
                            ));
                        });
                    }
                }
                if repo.branches.is_empty(){ui.label(RichText::new("No branches yet").size(TEXT_HEADER).strong());subtitle(ui,"Create your first commit from Changes to establish a branch.");}
            });
        });
    }

    fn branch_sync_controls(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, repo: &RepoState) {
        ui.horizontal_wrapped(|ui|{
            ui.label("Compare against").on_hover_text("Choose the branch to stay up to date with, usually origin/main. Updates use a comparison branch's remote when configured.");
            egui::ComboBox::from_id_salt("sync_baseline").selected_text(&self.base_input).width(170.).show_ui(ui,|ui|{
                for branch in repo.branches.iter().filter(|b|!b.name.ends_with("/HEAD")){ui.selectable_value(&mut self.base_input,branch.name.clone(),&branch.name);}
            }).response.on_hover_text("Compare against");
            if ui.button("Apply").clicked(){self.settings.projects.entry(self.path()).or_default().base=self.base_input.clone();self.save();self.refresh_at(self.path(),ctx);}
            if self.settings.advanced_mode {
                ui.menu_button("Advanced branch operations",|ui|{
                    if ui.button("Fetch latest branches & tags").clicked(){self.run_git(&["fetch","--all","--prune"],ctx);ui.close();}
                    if ui.button("Update from comparison branch…").on_hover_text("Fetch the newest comparison branch and merge it without rewriting existing commits.").clicked(){self.propose_update(&repo.branch);ui.close();}
                    if ui.button("Replay commits on comparison branch").clicked(){self.propose("Replay commits","Replay the current commits on the comparison branch. Commit IDs change; avoid rewriting shared history.",&["rebase",&repo.base]);ui.close();}
                });
            }
        });
    }
}

#[cfg(test)]
mod branch_display_tests {
    use super::*;
    fn pair() -> (git::Branch, git::Branch) {
        (
            git::Branch {
                name: "main".into(),
                id: "one".into(),
                current: true,
                upstream: "origin/main".into(),
                ..Default::default()
            },
            git::Branch {
                name: "origin/main".into(),
                id: "one".into(),
                remote: true,
                ..Default::default()
            },
        )
    }
    #[test]
    fn tracked_remote_is_one_card_and_filter_finds_either_name() {
        let (local, remote) = pair();
        let remote_only = git::Branch {
            name: "origin/other".into(),
            remote: true,
            ..Default::default()
        };
        let branches = vec![local, remote, remote_only];
        let visible = visible_branches(&branches, "");
        assert_eq!(
            visible.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(),
            ["main", "origin/other"]
        );
        assert_eq!(visible_branches(&branches, "origin/main")[0].name, "main");
        assert_eq!(
            visible_branches(&branches, "origin/other")[0].name,
            "origin/other"
        );
    }
    #[test]
    fn status_uses_remote_relationship_and_does_not_guess() {
        let (mut local, mut remote) = pair();
        local.ahead = 99; // This number is against the comparison branch, not the remote.
        assert_eq!(
            branch_explanation(&local, Some(&remote), None).0,
            "This computer and remote match"
        );
        remote.id = "two".into();
        assert_eq!(
            branch_explanation(&local, Some(&remote), Some((2, 1))).0,
            "2 to send · 1 to receive · needs attention"
        );
        assert_eq!(
            branch_explanation(&local, Some(&remote), Some((0, 2))).0,
            "2 commits to receive"
        );
        assert_eq!(
            branch_explanation(&local, Some(&remote), None).0,
            "Remote comparison is unavailable"
        );
        local.current = false;
        local.tracking = "[ahead 2, behind 1]".into();
        assert_eq!(
            branch_explanation(&local, Some(&remote), None).0,
            "2 to send · 1 to receive · needs attention"
        );
        local.tracking = "[unrecognized]".into();
        assert_eq!(
            branch_explanation(&local, Some(&remote), None).0,
            "Remote comparison is unavailable"
        );
    }
    #[test]
    fn unlinked_and_missing_remotes_are_not_reported_as_in_sync() {
        let (mut local, remote) = pair();
        local.upstream.clear();
        assert_eq!(visible_branches(&[local.clone(), remote], "").len(), 2);
        assert_eq!(
            branch_explanation(&local, None, None).0,
            "Only on this computer"
        );
        local.upstream = "origin/main".into();
        local.tracking = "[gone]".into();
        assert_eq!(
            branch_explanation(&local, None, None).0,
            "Linked remote branch is missing"
        );
    }
}
