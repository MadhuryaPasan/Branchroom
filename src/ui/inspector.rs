use super::*;
#[derive(Clone)]
struct Details {
    commit: git::Commit,
    files: Vec<CommitFile>,
}
#[derive(Default)]
pub(super) struct Inspector {
    key: String,
    revision: u64,
    receiver: Option<Receiver<(String, Result<Details>)>>,
    cache: std::collections::HashMap<String, Result<Details>>,
    pub expanded: bool,
}
pub(super) fn files(root: &std::path::Path, id: &str) -> Result<Vec<CommitFile>> {
    git::git(
        root,
        &[
            "diff-tree",
            "--root",
            "--no-commit-id",
            "--numstat",
            "--no-renames",
            "--diff-merges=first-parent",
            "-r",
            "-z",
            id,
        ],
    )
    .map(|out| CommitFile::parse(&out))
}
impl Branchroom {
    fn inspector_data(&mut self, ctx: &egui::Context) -> Option<Result<Details>> {
        let key = format!("{}:{}", self.path().display(), self.selected_commit);
        if self.inspector.key != key {
            self.inspector.key = key.clone();
            self.inspector.revision += 1;
            if self.commit_files_for != self.selected_commit {
                self.commit_files.clear();
            }
        }
        if let Some(rx) = &self.inspector.receiver {
            match rx.try_recv() {
                Ok((finished, result)) => {
                    self.inspector.receiver = None;
                    if self.inspector.cache.len() >= 64 {
                        self.inspector.cache.clear();
                    }
                    self.inspector.cache.insert(finished, result);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.inspector.receiver = None;
                    self.inspector.cache.insert(
                        key.clone(),
                        Err("The details reader stopped. Retry loading this commit".into()),
                    );
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if !self.preview_mode
            && !self.selected_commit.is_empty()
            && !self.inspector.cache.contains_key(&key)
            && self.inspector.receiver.is_none()
        {
            let (tx, rx) = mpsc::channel();
            self.inspector.receiver = Some(rx);
            let (root, id, ctx) = (self.path(), self.selected_commit.clone(), ctx.clone());
            std::thread::spawn(move || {
                let result = git::commit_details(&root, &id).and_then(|commit| {
                    files(&root, &commit.id).map(|files| Details { commit, files })
                });
                let _ = tx.send((key, result));
                ctx.request_repaint();
            });
        }
        self.inspector.cache.get(&self.inspector.key).cloned()
    }
    pub(super) fn commit_inspector(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let data = self.inspector_data(ctx);
        let known = self
            .repo
            .as_ref()
            .and_then(|r| {
                r.commits
                    .iter()
                    .chain(r.branch_commits.iter())
                    .find(|c| c.id == self.selected_commit)
            })
            .cloned();
        let commit = known.or_else(|| {
            data.as_ref()
                .and_then(|d| d.as_ref().ok())
                .map(|d| d.commit.clone())
        });
        ui.horizontal_wrapped(|ui| {
            section(ui, "Commit details");
            if !self.selected_commit.is_empty() {
                ui.label(
                    RichText::new(short(&self.selected_commit))
                        .monospace()
                        .color(MUTED()),
                )
                .on_hover_text(&self.selected_commit);
            }
        });
        let Some(commit) = commit else {
            if self.selected_commit.is_empty() {
                subtitle(ui, "Choose a commit to review its changes and details");
            } else if let Some(Err(error)) = data {
                ui.colored_label(DANGER(), "Could not load this commit");
                ui.add(egui::Label::new(error).wrap());
                if ui.button("Retry commit details").clicked() {
                    self.inspector.cache.remove(&self.inspector.key);
                }
            } else {
                ui.spinner();
                subtitle(ui, "Loading commit details…");
            }
            return;
        };
        ui.horizontal_wrapped(|ui| {
            if ui.button("View patch").clicked() {
                self.view_commit(&commit.id, ctx);
            }
            if ui
                .button("Open this commit in IDE…")
                .on_hover_text("Open this exact version in a separate directory")
                .clicked()
            {
                self.begin_review(&commit.id, None);
            }
            self.commit_actions(ui, ctx, &commit);
        });
        ui.add_space(SPACE[1]);
        egui::ScrollArea::vertical()
            .id_salt(("commit_details", self.inspector.revision))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add(
                    egui::Label::new(RichText::new(&commit.subject).size(18.).strong())
                        .wrap()
                        .selectable(true),
                );
                ui.add(
                    egui::Label::new(
                        RichText::new(format!("{} · {}", commit.author, commit.date))
                            .color(MUTED()),
                    )
                    .wrap(),
                );
                if !commit.refs.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        for name in commit.refs.split(", ") {
                            badge(ui, name, ACCENT());
                        }
                    });
                }
                ui.add_space(SPACE[3]);
                section(ui, "Changed files");
                let loaded = if let Some(Ok(details)) = &data {
                    Some(details.files.clone())
                } else if self.commit_files_for == commit.id {
                    Some(self.commit_files.clone())
                } else {
                    None
                };
                if let Some(files) = loaded {
                    ui.label(format!(
                        "{} {}",
                        files.len(),
                        if files.len() == 1 { "file" } else { "files" }
                    ));
                    if files.is_empty() {
                        subtitle(ui, "No changed files in this commit");
                    }
                    for file in files {
                        if ui
                            .add(
                                egui::Button::selectable(
                                    self.selected_path == file.path,
                                    file.label(),
                                )
                                .truncate(),
                            )
                            .on_hover_text(&file.path)
                            .clicked()
                        {
                            self.detail_title = format!("{} · {}", short(&commit.id), file.path);
                            let mut args = process::strings(&[
                                "show",
                                "--format=",
                                "--first-parent",
                                &commit.id,
                                "--",
                            ]);
                            self.selected_path = file.path.clone();
                            args.push(file.path);
                            self.read("git", args, ctx);
                        }
                    }
                } else if let Some(Err(error)) = &data {
                    ui.colored_label(DANGER(), "Could not load changed files");
                    ui.add(egui::Label::new(error).wrap());
                    if ui.button("Retry loading changed files").clicked() {
                        self.inspector.cache.remove(&self.inspector.key);
                        self.execute(Task::CommitFiles(commit.id.clone()), ctx);
                    }
                } else if self.preview_mode {
                    subtitle(ui, "Changed files are available in a live repository");
                } else {
                    ui.spinner();
                    subtitle(ui, "Loading changed files…");
                }
                ui.add_space(SPACE[3]);
                section(ui, "Pull requests and checks");
                let root = self.path();
                if let Some(Ok(context)) = &self.graph_context.data {
                    if let Some(warning) = &context.identity_warning {
                        ui.add(egui::Label::new(RichText::new(warning).color(WARNING())).wrap());
                    }
                    let mut linked = false;
                    for pr in context.reviews.iter().filter(|pr| {
                        pr.commit == commit.id
                            || self.graph_context.linked_to(&root, &commit.id, pr.number)
                    }) {
                        linked = true;
                        ui.hyperlink_to(format!("PR #{} · {}", pr.number, pr.title), &pr.url);
                        subtitle(
                            ui,
                            &format!(
                                "{} → {} · {} · {}",
                                pr.branch, pr.base_branch, pr.state, pr.ci
                            ),
                        );
                    }
                    if !linked {
                        subtitle(ui, "No linked pull request reported");
                    }
                } else {
                    subtitle(
                        ui,
                        "GitHub details are unavailable. Check the connection in Pull requests",
                    );
                }
                ui.add_space(SPACE[3]);
                egui::CollapsingHeader::new("Technical details").show(ui, |ui| {
                    section(ui, "Commit ID");
                    ui.add(
                        egui::Label::new(RichText::new(&commit.id).monospace())
                            .wrap()
                            .selectable(true),
                    );
                    if ui.button("Copy full SHA").clicked() {
                        ctx.copy_text(commit.id.clone());
                    }
                    section(ui, "Parents");
                    if commit.parents.is_empty() {
                        subtitle(ui, "First commit");
                    }
                    for parent in &commit.parents {
                        if ui.button(short(parent)).on_hover_text(parent).clicked() {
                            self.selected_commit = parent.clone();
                        }
                    }
                });
            });
    }
    fn commit_actions(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, commit: &git::Commit) {
        ui.menu_button("Commit actions…",|ui| {
                if ui.button("Find related pull requests").on_hover_text("Search GitHub for pull requests containing this commit; review or open their code in an IDE.").clicked(){self.page=Page::Github;self.pr_tab=0;self.pull_requests.clear();self.prs_loaded=false;self.execute(Task::CommitPullRequests(commit.id.clone()),ctx);ui.close();}
                if ui.add_enabled(commit.id == self.repo.as_ref().unwrap().head,egui::Button::new("Undo latest local commit")).on_hover_text("Only for the selected branch tip. Keep its changes locally; published commits are refused.").clicked(){self.pending=Some(Pending{title:"Undo latest commit".into(),explanation:"Move the latest unpublished commit back to working changes. A recovery ref preserves the original history.".into(),task:Task::History(branchroom::history::Action::Undo)});ui.close();}
                if ui.button("Reset branch to this commit…").clicked(){self.pending=Some(Pending{title:"Reset branch to commit".into(),explanation:"Move this branch back while retaining removed commits as working changes. Published commits are refused.".into(),task:Task::History(branchroom::history::Action::Reset(commit.id.clone()))});ui.close();}
                if ui.button("Squash or reorder local commits…").clicked(){self.open_graph_form();ui.close();}
                if ui.button("Create branch here…").clicked(){self.form("Create branch","Create a local branch at this commit.","git",&["branch"],&[("Branch name","")],&[&commit.id]);ui.close();}
                if ui.button("Create tag…").clicked(){self.form("Tag commit","Create a lightweight tag at this commit.","git",&["tag"],&[("Tag name","")],&[&commit.id]);ui.close();}
                if ui.button("Cherry-pick into current branch").clicked(){self.propose("Cherry-pick commit","Apply this commit on the checked-out branch. Merge commits need a mainline option in Command center.",&["cherry-pick",&commit.id]);ui.close();}
                if commit.parents.len() > 1 {
                    ui.menu_button("Revert merge…", |ui| {
                        ui.label("Choose the parent whose history should be kept:");
                        for (index,parent) in commit.parents.iter().enumerate() {
                            if ui.button(format!("Keep parent {} · {}",index+1,short(parent))).clicked() {
                                self.pending=Some(Pending{title:"Revert merge commit".into(),explanation:format!("Create an inverse commit on {} for {} relative to parent {} ({}). Later merges will not automatically reintroduce the reverted changes. Commit or stash changes first. Conflicts can be resolved or aborted in Conflicts.",self.repo.as_ref().unwrap().branch,short(&commit.id),index+1,parent),task:Task::Revert{reference:commit.id.clone(),mainline:Some(index+1)}});ui.close();
                            }
                        }
                    });
                } else if ui.button("Revert this commit…").on_hover_text("Add an inverse commit to the current branch. Preserves shared history; unlike reset, it does not remove commits.").clicked(){self.pending=Some(Pending{title:"Revert commit".into(),explanation:format!("Undo the changes introduced by {} on branch {} by creating a new commit. Later commits remain. Commit or stash working changes first. If conflicts occur, resolve or abort in Conflicts. Nothing is pushed.",short(&commit.id),self.repo.as_ref().unwrap().branch),task:Task::Revert{reference:commit.id.clone(),mainline:None}});ui.close();}
            });
    }
}

#[cfg(all(test, feature = "ui-preview"))]
mod tests {
    use super::*;
    #[test]
    fn stale_details_are_cached_without_replacing_the_current_commit() {
        let ctx = egui::Context::default();
        let mut app = Branchroom::with_context(&ctx, true);
        let commits = app.repo.as_ref().unwrap().commits.clone();
        let old = commits[0].clone();
        app.selected_commit = commits[1].id.clone();
        let (tx, rx) = mpsc::channel();
        app.inspector.receiver = Some(rx);
        let old_key = format!("{}:{}", app.path().display(), old.id);
        tx.send((
            old_key.clone(),
            Ok(Details {
                commit: old,
                files: vec![],
            }),
        ))
        .unwrap();
        assert!(app.inspector_data(&ctx).is_none());
        assert!(app.inspector.cache.contains_key(&old_key));
        assert_eq!(app.selected_commit, commits[1].id);
    }
    #[test]
    fn disconnected_details_worker_offers_retry_instead_of_loading_forever() {
        let ctx = egui::Context::default();
        let mut app = Branchroom::with_context(&ctx, true);
        let (tx, rx) = mpsc::channel();
        drop(tx);
        app.inspector.receiver = Some(rx);
        assert!(matches!(app.inspector_data(&ctx), Some(Err(_))));
        assert!(app.inspector.receiver.is_none());
    }
}
