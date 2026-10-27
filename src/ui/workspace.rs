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
            .corner_radius(8),
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
        (location, 57., 12., MUTED(), 12.),
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
                ui.heading("Branches");
                if primary(ui,"New branch").clicked(){self.form("Create branch","Create and switch to a branch from the current HEAD.","git",&["switch","-c"],&[("Branch name","")],&[]);}
            });
            if !compact { ui.add_space(8.); }
            if compact {
                ui.horizontal_wrapped(|ui|{
                    ui.label(format!("Compare against: {}",shorten_label(&self.base_input,24)));
                    egui::containers::menu::MenuButton::new("Sync & baseline")
                        .config(egui::containers::menu::MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside))
                        .ui(ui,|ui|self.branch_sync_controls(ui,ctx,&repo));
                });
            }else{
                self.branch_sync_controls(ui,ctx,&repo);
                ui.add_space(5.);subtitle(ui,"Each card brings a branch on this computer and its linked remote together. Remote information is from the last fetch.");
            }
        });
        self.patch_drawer(ui);
        egui::CentralPanel::default().frame(egui::Frame::new().fill(PAPER()).inner_margin(20)).show(ui,|ui|{
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
                ui.painter().text(canvas.min+egui::vec2(8.,10.),egui::Align2::LEFT_TOP,"COMMON HISTORY",egui::FontId::proportional(11.),MUTED());
                ui.painter().text(canvas.min+egui::vec2(child_x+10.,10.),egui::Align2::LEFT_TOP,"BRANCHES",egui::FontId::proportional(11.),MUTED());
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
                    response.widget_info(||egui::WidgetInfo::labeled(egui::Role::Button,ui.is_enabled(),format!("Common commit {}",ancestor)));
                    if response.clicked()&&!ancestor.is_empty(){self.view_commit(&ancestor,ctx); }
                    ui.painter().text(source.min+egui::vec2(14.,12.),egui::Align2::LEFT_TOP,if ancestor.is_empty(){"Independent history"}else{"Common commit"},egui::FontId::proportional(14.),MUTED());
                    ui.painter().text(source.min+egui::vec2(14.,37.),egui::Align2::LEFT_TOP,if ancestor.is_empty(){"No common commit"}else{"View source details"},egui::FontId::monospace(12.),ACCENT());
                    for (index,branch) in branches.iter().enumerate(){
                        let card=egui::Rect::from_min_size(rect.min+egui::vec2(child_x,index as f32*108.),egui::vec2((width-child_x-15.).min(720.),88.));
                        let remote = repo.branches.iter().find(|b| b.remote && b.name == branch.upstream);
                        let (status, location, color) = branch_explanation(branch, remote, repo.sync_counts);
                        if !ancestor.is_empty(){super::experiments::connect(ui,source,card,CONNECTOR());}
                        let name=format!("{}{}",branch.name,if branch.current{"  · current"}else{""});
                        let response=branch_card(ui,card,&name,&status,&location,branch.current,color);
                        if response.on_hover_text(format!("{}\n{}\n{}",branch.name,status,location)).clicked(){self.view_commit(&branch.id,ctx);}
                        ui.scope_builder(egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(egui::pos2(card.right()-116.,card.top()+8.),egui::vec2(105.,32.))),|ui|{ui.menu_button("Actions",|ui|{
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

                            if !branch.remote {ui.menu_button("Advanced",|ui|{
                                if ui.button("Update this branch from comparison branch…").clicked(){self.propose_update(&branch.name);ui.close();}
                                if ui.button("Publish / push this branch…").clicked(){self.form("Publish branch","Push the selected local branch and set its upstream. Review the destination remote.","git",&["push","--progress","--set-upstream"],&[("Remote","origin"),("Branch",&branch.name)],&[]);ui.close();}
                            });}
                            if ui.button(if branch.current{"Show commit history"}else{"Switch & show commit history"}).clicked(){if !branch.current{self.request_switch(&branch.name,ctx);}self.page=Page::History;ui.close();}
                            if !branch.current&&ui.button("Switch branch").clicked(){self.request_switch(&branch.name,ctx);ui.close();}
                            if !branch.remote&&ui.button("Rename branch…").clicked(){self.form("Rename branch","Rename this local branch.","git",&["branch","-m",&branch.name],&[("New branch name","")],&[]);ui.close();}
                            if !branch.current&&ui.button("Merge into current").clicked(){self.propose("Merge branch","Merge this branch into the checked-out branch.",&["merge","--no-edit",&branch.name]);ui.close();}
                            if ui.button("Compare with baseline").clicked(){self.detail_title=format!("{} → {}",repo.base,branch.name);self.read("git",process::strings(&["diff","--stat",&format!("{}...{}",repo.base,branch.name)]),ctx);ui.close();}
                            if !branch.current&&!branch.remote&&ui.button(RichText::new("Delete merged branch").color(DANGER())).clicked(){self.propose("Delete local branch","Delete this branch only if it is fully merged.",&["branch","-d",&branch.name]);ui.close();}
                        });});
                    }
                }
                if repo.branches.is_empty(){ui.heading("No branches yet");subtitle(ui,"Create your first commit from Changes to establish a branch.");}
            });
        });
    }

    fn branch_sync_controls(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, repo: &RepoState) {
        ui.horizontal_wrapped(|ui|{
            ui.label("Compare against").on_hover_text("Choose the branch to stay up to date with, usually origin/main. Updates use a local baseline’s upstream when configured.");
            egui::ComboBox::from_id_salt("sync_baseline").selected_text(&self.base_input).width(170.).show_ui(ui,|ui|{
                for branch in repo.branches.iter().filter(|b|!b.name.ends_with("/HEAD")){ui.selectable_value(&mut self.base_input,branch.name.clone(),&branch.name);}
            }).response.on_hover_text("Compare against");
            if ui.button("Apply").clicked(){self.settings.projects.entry(self.path()).or_default().base=self.base_input.clone();self.save();self.refresh_at(self.path(),ctx);}
            ui.menu_button("Advanced branch operations",|ui|{
                if ui.button("Fetch latest branches & tags").clicked(){self.run_git(&["fetch","--all","--prune"],ctx);ui.close();}
                if ui.button("Update from baseline (fetch & merge)…").on_hover_text("Fetch the newest baseline and merge it without rewriting existing commits.").clicked(){self.propose_update(&repo.branch);ui.close();}
                if ui.button("Rebase current onto baseline").clicked(){self.propose("Rebase onto baseline","Replay the current commits on the baseline. Commit IDs change; avoid rewriting shared history.",&["rebase",&repo.base]);ui.close();}
            });
        });
    }

    pub(super) fn stashes(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let stashes = self.repo.as_ref().unwrap().stashes.clone();
        egui::Panel::top("stash_controls").frame(egui::Frame::new().fill(PAPER()).inner_margin(20)).show(ui,|ui|{
            ui.horizontal(|ui|{ui.heading("Stashes");badge(ui,&stashes.len().to_string(),MUTED());ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{if primary(ui,"Save stash").clicked(){self.form("Save stash","Save staged, unstaged, and non-ignored untracked files, then clean the working tree.","git",&["stash","push","--include-untracked","-m"],&[("Stash message","")],&[]);}});});subtitle(ui,"Set unfinished work aside and pick it up when the branch is ready.");
        });
        self.patch_drawer(ui);
        egui::CentralPanel::default().frame(egui::Frame::new().fill(PAPER()).inner_margin(24)).show(ui,|ui|{
            egui::ScrollArea::vertical().id_salt("stash_cards").show(ui,|ui|{
                for (reference,label) in stashes.iter(){surface().show(ui,|ui|{
                    ui.horizontal_wrapped(|ui|{badge(ui,reference,WARNING());ui.label(RichText::new(label).strong());});ui.add_space(10.);
                    ui.horizontal(|ui|{
                        if ui.button("Inspect patch").clicked(){self.detail_title=reference.clone();self.read("git",process::strings(&["stash","show","--include-untracked","-p",reference]),ctx);}
                        if ui.button("Apply & keep stash").clicked(){self.propose("Apply stash","Apply these changes and retain the saved stash. Conflicts may require resolution.",&["stash","apply",reference]);}
                        ui.menu_button("More actions",|ui|{
                            if ui.button("Apply & remove stash (pop)").clicked(){self.propose("Pop stash","Apply this stash, then remove it if application succeeds.",&["stash","pop",reference]);ui.close();}
                            if ui.button(RichText::new("Drop saved stash").color(DANGER())).clicked(){self.propose("Drop stash","Remove this saved stash. Its contents may become unrecoverable.",&["stash","drop",reference]);ui.close();}
                        });
                    });
                });ui.add_space(12.);}
                if stashes.is_empty(){ui.add_space(50.);ui.heading("No work set aside");subtitle(ui,"Save a stash to clear your working directory while keeping unfinished changes.");}
            });
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
