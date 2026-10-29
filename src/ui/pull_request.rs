use super::*;
use branchroom::github::{Action, MergeMethod, Review};
impl Branchroom {
    pub(super) fn pull_request_details(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(pr) = self.pr_details.clone() else {
            subtitle(
                ui,
                "Select a pull request to inspect its description, changes, checks, and review actions.",
            );
            return;
        };
        let writable = !branchroom::safety::is_protected(&self.path()) && self.receiver.is_none();
        ui.horizontal_wrapped(|ui| {
            badge(ui, if pr.is_draft { "Draft" } else { &pr.state }, ACCENT());
            ui.heading(format!("#{} {}", pr.number, pr.title));
        });
        ui.hyperlink_to("Open pull request in GitHub", &pr.url);
        subtitle(
            ui,
            &format!(
                "Compare: {} · Base: {} · {} changed files · +{} / -{}",
                pr.head_ref_name, pr.base_ref_name, pr.changed_files, pr.additions, pr.deletions
            ),
        );
        ui.add_space(12.);
        ui.horizontal_wrapped(|ui| {
            if ui.button("Reload details").clicked() {
                self.execute(Task::PullRequestDetails(pr.number), ctx);
            }
            if ui.button("Inspect changed files").clicked() {
                self.detail_title = format!("PR #{} changes", pr.number);
                self.read(
                    "gh",
                    process::strings(&["pr", "diff", &pr.number.to_string()]),
                    ctx,
                );
            }
            if ui.button("View checks").clicked() {
                self.detail_title = format!("PR #{} checks", pr.number);
                self.read(
                    "gh",
                    process::strings(&["pr", "checks", &pr.number.to_string()]),
                    ctx,
                );
            }
            if ui.button("Open code in IDE…").clicked() {
                self.begin_review(
                    &format!("PR #{} · {}", pr.number, pr.title),
                    Some(pr.number),
                );
            }
        });
        ui.add_space(16.);
        egui::CollapsingHeader::new("Description")
            .default_open(true)
            .show(ui, |ui| {
                ui.add(
                    egui::Label::new(if pr.body.is_empty() {
                        "No description provided."
                    } else {
                        &pr.body
                    })
                    .wrap()
                    .selectable(true),
                );
            });
        ui.add_space(12.);
        surface().show(ui,|ui|{
            section(ui,"MERGE STATUS");
            ui.label(format!("{} · review: {}",if pr.merge_state_status.is_empty(){"Not reported"}else{&pr.merge_state_status},if pr.review_decision.is_empty(){"Not reported"}else{&pr.review_decision}));
            subtitle(ui,"GitHub enforces required checks, reviews, permissions, and branch rules. Reload before deciding; merge status can change.");
            ui.add_enabled_ui(writable,|ui|{
                ui.horizontal_wrapped(|ui|{
                    if pr.state=="OPEN"&&pr.is_draft&&ui.button("Mark ready for review…").clicked(){self.pending=Some(Pending{title:"Mark pull request ready".into(),explanation:format!("Mark #{} {} as ready for review on GitHub.",pr.number,pr.title),task:Task::GithubAction(Action::Ready(pr.number))});}
                    if pr.state=="OPEN"&&!pr.is_draft{
                        ui.menu_button("Merge pull request…",|ui|{
                            for(method,label)in[(MergeMethod::Merge,"Create a merge commit"),(MergeMethod::Squash,"Squash and merge"),(MergeMethod::Rebase,"Rebase and merge")]{
                                if ui.button(label).clicked(){self.pending=Some(Pending{title:format!("Merge pull request #{}",pr.number),explanation:format!("{label}: compare branch {} into base branch {} on GitHub. This changes the remote repository. GitHub will refuse the request if the PR head changed or required rules are unmet. Local branches and files are not deleted.",pr.head_ref_name,pr.base_ref_name),task:Task::GithubAction(Action::Merge{number:pr.number,head:pr.head_ref_oid.clone(),method})});ui.close();}
                            }
                        });
                    }
                    if pr.state=="OPEN"&&ui.button("Close pull request…").clicked(){self.pending=Some(Pending{title:"Close pull request".into(),explanation:format!("Close #{} {} on GitHub without merging. It can be reopened later.",pr.number,pr.title),task:Task::GithubAction(Action::Close(pr.number))});}
                    if pr.state=="CLOSED"&&ui.button("Reopen pull request…").clicked(){self.pending=Some(Pending{title:"Reopen pull request".into(),explanation:format!("Reopen #{} {} on GitHub.",pr.number,pr.title),task:Task::GithubAction(Action::Reopen(pr.number))});}
                });
            });
        });
        if pr.state == "OPEN" {
            ui.add_space(16.);
            surface().show(ui,|ui|{
                ui.label(RichText::new("Review feedback").strong());
                ui.add(egui::TextEdit::multiline(&mut self.pr_review).desired_rows(4).desired_width(f32::INFINITY).hint_text("Explain your feedback for the author"));
                ui.add_enabled_ui(writable,|ui|{ui.horizontal_wrapped(|ui|{
                    for(kind,label)in[(Review::Comment,"Comment…"),(Review::Approve,"Approve…"),(Review::RequestChanges,"Request changes…")]{
                        if ui.button(label).clicked(){
                            let action=Action::Review{number:pr.number,kind,body:self.pr_review.clone()};
                            match action.arguments(){Ok(_)=>self.pending=Some(Pending{title:label.trim_end_matches('…').into(),explanation:format!("Submit this review on #{} {} in GitHub:\n\n{}",pr.number,pr.title,self.pr_review),task:Task::GithubAction(action)}),Err(e)=>self.report(Err(e))}
                        }
                    }
                });});
                subtitle(ui,"For inline code suggestions and threaded conversations, open the pull request in GitHub.");
            });
        }
    }
}
