use super::*;

#[derive(Default)]
pub(super) struct ComparisonView {
    pub(super) advanced: bool,
    repository: std::path::PathBuf,
    key: String,
    receiver: Option<Receiver<Result<Comparison>>>,
    data: Option<Result<Comparison>>,
}
struct Comparison {
    left: Vec<(String, String)>,
    right: Vec<(String, String)>,
    files: Vec<CommitFile>,
    ancestor: String,
}
impl Branchroom {
    pub(super) fn comparison_canvas(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let repo = self.repo.as_ref().unwrap().clone();
        if self.comparison.repository != repo.root {
            self.comparison.repository = repo.root.clone();
            self.compare_base = if repo.base != "HEAD" {
                repo.base.clone()
            } else {
                repo.branches
                    .iter()
                    .find(|branch| branch.id != repo.head)
                    .map(|branch| {
                        format!(
                            "refs/{}/{}",
                            if branch.remote { "remotes" } else { "heads" },
                            branch.name
                        )
                    })
                    .or_else(|| {
                        repo.commits
                            .iter()
                            .find(|commit| commit.id != repo.head)
                            .map(|commit| commit.id.clone())
                    })
                    .unwrap_or_else(|| "HEAD".into())
            };
            self.compare_head = "HEAD".into();
            self.comparison.key.clear();
            self.comparison.data = None;
        }
        let tips = repo
            .branches
            .iter()
            .map(|b| b.id.as_str())
            .collect::<String>();
        let key = format!(
            "{}:{}:{tips}:{}:{}",
            repo.root.display(),
            repo.head,
            self.compare_base,
            self.compare_head
        );
        if let Some(rx) = &self.comparison.receiver
            && let Ok(result) = rx.try_recv()
        {
            self.comparison.data = Some(result);
            self.comparison.receiver = None;
        }
        if self.comparison.key != key && self.comparison.receiver.is_none() {
            self.comparison.key = key;
            self.comparison.data = None;
            if self.preview_mode {
                let resolve = |name: &str| {
                    if name == "HEAD" {
                        repo.head.clone()
                    } else {
                        repo.branches
                            .iter()
                            .find(|b| {
                                b.name
                                    == name
                                        .strip_prefix("refs/heads/")
                                        .or_else(|| name.strip_prefix("refs/remotes/"))
                                        .unwrap_or(name)
                            })
                            .map(|b| b.id.clone())
                            .unwrap_or_else(|| name.to_owned())
                    }
                };
                let left = graph::ancestors(&repo.commits, &resolve(&self.compare_base));
                let right = graph::ancestors(&repo.commits, &resolve(&self.compare_head));
                let left_ids: std::collections::HashSet<_> =
                    left.iter().map(|c| c.id.as_str()).collect();
                let right_ids: std::collections::HashSet<_> =
                    right.iter().map(|c| c.id.as_str()).collect();
                self.comparison.data = Some(Ok(Comparison {
                    ancestor: right
                        .iter()
                        .find(|c| left_ids.contains(c.id.as_str()))
                        .map(|c| c.id.clone())
                        .unwrap_or_default(),
                    left: left
                        .iter()
                        .filter(|c| !right_ids.contains(c.id.as_str()))
                        .map(|c| (c.id.clone(), c.subject.clone()))
                        .collect(),
                    right: right
                        .iter()
                        .filter(|c| !left_ids.contains(c.id.as_str()))
                        .map(|c| (c.id.clone(), c.subject.clone()))
                        .collect(),
                    files: vec![],
                }));
            } else {
                let (tx, rx) = mpsc::channel();
                self.comparison.receiver = Some(rx);
                let root = repo.root.clone();
                let left = self.compare_base.clone();
                let right = self.compare_head.clone();
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let result = (|| {
                        let left = git::resolve_named_commit(&root, &left)?.1;
                        let right = git::resolve_named_commit(&root, &right)?.1;
                        let ancestor = git::git(&root, &["merge-base", &left, &right])
                            .unwrap_or_default()
                            .trim()
                            .to_owned();
                        let log = |range: &str| -> Result<Vec<(String, String)>> {
                            Ok(git::git(&root, &["log", "--format=%H%x00%s", "-z", range])?
                                .split_terminator('\0')
                                .collect::<Vec<_>>()
                                .as_chunks::<2>()
                                .0
                                .iter()
                                .map(|p| {
                                    (p[0].trim_start_matches('\n').to_owned(), p[1].to_owned())
                                })
                                .collect())
                        };
                        let files = CommitFile::parse(&git::git(
                            &root,
                            &[
                                "diff",
                                "--no-renames",
                                "--numstat",
                                "-z",
                                &left,
                                &right,
                                "--",
                            ],
                        )?);
                        Ok(Comparison {
                            left: log(&format!("{right}..{left}"))?,
                            right: log(&format!("{left}..{right}"))?,
                            files,
                            ancestor,
                        })
                    })();
                    let _ = tx.send(result);
                    ctx.request_repaint();
                });
            }
        }
        self.graph_inspector(ui, ctx);
        egui::ScrollArea::vertical()
            .id_salt("comparison_workspace")
            .show(ui, |ui| {
                heading(
                    ui,
                    "Compare branches",
                    "See which commits and files differ between two branch tips.",
                );
                ui.horizontal_wrapped(|ui| {
                    ui.label("Base branch");
                    let base_picker = egui::ComboBox::from_id_salt("compare_base")
                        .selected_text(
                            self.compare_base
                                .strip_prefix("refs/heads/")
                                .or_else(|| self.compare_base.strip_prefix("refs/remotes/"))
                                .unwrap_or(&self.compare_base),
                        )
                        .width(180.)
                        .show_ui(ui, |ui| {
                            for branch in &repo.branches {
                                ui.selectable_value(
                                    &mut self.compare_base,
                                    format!(
                                        "refs/{}/{}",
                                        if branch.remote { "remotes" } else { "heads" },
                                        branch.name
                                    ),
                                    &branch.name,
                                );
                            }
                            for commit in &repo.commits {
                                ui.selectable_value(
                                    &mut self.compare_base,
                                    commit.id.clone(),
                                    format!("{} {}", short(&commit.id), commit.subject),
                                );
                            }
                        });
                    base_picker.response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::Role::ComboBox,
                            true,
                            format!("Base branch: {}", self.compare_base),
                        )
                    });
                    ui.label("Compare with");
                    let compare_picker = egui::ComboBox::from_id_salt("compare_head")
                        .selected_text(
                            self.compare_head
                                .strip_prefix("refs/heads/")
                                .or_else(|| self.compare_head.strip_prefix("refs/remotes/"))
                                .unwrap_or(&self.compare_head),
                        )
                        .width(180.)
                        .show_ui(ui, |ui| {
                            for branch in &repo.branches {
                                ui.selectable_value(
                                    &mut self.compare_head,
                                    format!(
                                        "refs/{}/{}",
                                        if branch.remote { "remotes" } else { "heads" },
                                        branch.name
                                    ),
                                    &branch.name,
                                );
                            }
                            for commit in &repo.commits {
                                ui.selectable_value(
                                    &mut self.compare_head,
                                    commit.id.clone(),
                                    format!("{} {}", short(&commit.id), commit.subject),
                                );
                            }
                        });
                    compare_picker.response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::Role::ComboBox,
                            true,
                            format!("Compare with: {}", self.compare_head),
                        )
                    });
                    ui.checkbox(&mut self.inspector_open, "Commit details");
                });

                let mut selected = None;
                let mut selected_file = None;
                match &self.comparison.data {
                    None => {
                        ui.spinner();
                        ui.label("Loading branch differences…");
                    }
                    Some(Err(error)) => {
                        ui.colored_label(DANGER(), error);
                        ui.label("Choose two existing branches or commits and retry");
                        if ui.button("Retry comparison").clicked() {
                            self.comparison.key.clear();
                        }
                    }
                    Some(Ok(data)) => {
                        let base_name = self
                            .compare_base
                            .strip_prefix("refs/heads/")
                            .or_else(|| self.compare_base.strip_prefix("refs/remotes/"))
                            .unwrap_or(&self.compare_base);
                        let compare_name = self
                            .compare_head
                            .strip_prefix("refs/heads/")
                            .or_else(|| self.compare_head.strip_prefix("refs/remotes/"))
                            .unwrap_or(&self.compare_head);
                        surface().show(ui, |ui| {
                            section(ui, "COMPARISON SUMMARY");
                            ui.add_space(8.);
                            ui.horizontal_wrapped(|ui| {
                                badge(ui, "BASE", MUTED());
                                ui.label(RichText::new(base_name).strong());
                                ui.label("compared with");
                                badge(ui, "COMPARE", ACCENT());
                                ui.label(RichText::new(compare_name).strong());
                            });
                            ui.add_space(8.);
                            ui.horizontal_wrapped(|ui| {
                                badge(
                                    ui,
                                    &format!(
                                        "{} unique {}",
                                        data.left.len(),
                                        if data.left.len() == 1 {
                                            "commit"
                                        } else {
                                            "commits"
                                        }
                                    ),
                                    MUTED(),
                                );
                                badge(
                                    ui,
                                    &format!(
                                        "{} unique {}",
                                        data.right.len(),
                                        if data.right.len() == 1 {
                                            "commit"
                                        } else {
                                            "commits"
                                        }
                                    ),
                                    SUCCESS(),
                                );
                                if data.ancestor.is_empty() {
                                    ui.colored_label(
                                        WARNING(),
                                        "No common ancestor; these histories are unrelated.",
                                    );
                                } else {
                                    ui.label(format!(
                                        "Shared history begins at {}",
                                        short(&data.ancestor)
                                    ));
                                }
                            });
                        });
                        ui.add_space(12.);

                        let mut show_commit_list =
                            |ui: &mut egui::Ui, title: &str, commits: &[(String, String)]| {
                                surface().show(ui, |ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(RichText::new(title).size(TEXT_SUBHEAD).strong());
                                        badge(ui, &commits.len().to_string(), MUTED());
                                    });
                                    if commits.is_empty() {
                                        subtitle(ui, "No commits unique to this branch.");
                                    } else {
                                        egui::ScrollArea::vertical()
                                            .id_salt(("compare_commits", title))
                                            .max_height(240.)
                                            .show_rows(ui, 40., commits.len(), |ui, range| {
                                                for i in range {
                                                    let (id, subject) = &commits[i];
                                                    let response = ui
                                                        .add_sized(
                                                            [ui.available_width(), 36.],
                                                            egui::Button::selectable(
                                                                self.selected_commit == *id,
                                                                format!(
                                                                    "{}  {}",
                                                                    short(id),
                                                                    subject
                                                                ),
                                                            )
                                                            .wrap(),
                                                        )
                                                        .on_hover_text(id);
                                                    if response.clicked() {
                                                        selected = Some(id.clone());
                                                    }
                                                }
                                            });
                                    }
                                });
                            };
                        let available_width = ui.available_width();
                        if available_width >= 720. {
                            ui.columns(2, |columns| {
                                show_commit_list(
                                    &mut columns[0],
                                    &format!("Only in {base_name}"),
                                    &data.left,
                                );
                                show_commit_list(
                                    &mut columns[1],
                                    &format!("Only in {compare_name}"),
                                    &data.right,
                                );
                            });
                        } else {
                            show_commit_list(ui, &format!("Only in {base_name}"), &data.left);
                            ui.add_space(8.);
                            show_commit_list(ui, &format!("Only in {compare_name}"), &data.right);
                        }
                        ui.add_space(12.);
                        surface().show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new("Files changed between branches")
                                        .size(TEXT_SUBHEAD)
                                        .strong(),
                                );
                                badge(ui, &data.files.len().to_string(), MUTED());
                            });
                            subtitle(ui, "Files changed between the two selected branch tips.");
                            if data.files.is_empty() {
                                subtitle(
                                    ui,
                                    if self.preview_mode {
                                        "File differences load in a live repository."
                                    } else {
                                        "These branch tips have the same file content."
                                    },
                                );
                            } else {
                                egui::ScrollArea::vertical()
                                    .id_salt("comparison_files")
                                    .max_height(260.)
                                    .show_rows(ui, 34., data.files.len(), |ui, range| {
                                        for i in range {
                                            let file = &data.files[i];
                                            let response = ui
                                                .add_sized(
                                                    [ui.available_width(), 30.],
                                                    egui::Button::selectable(
                                                        self.selected_path == file.path,
                                                        file.label(),
                                                    )
                                                    .truncate(),
                                                )
                                                .on_hover_text(&file.path);
                                            if response.clicked() {
                                                self.selected_path = file.path.clone();
                                                selected_file = Some(file.path.clone());
                                            }
                                        }
                                    });
                            }
                        });
                    }
                }
                if let Some(id) = selected {
                    self.selected_commit = id;
                    self.inspector_open = true;
                }
                if let Some(path) = selected_file {
                    let args = process::strings(&[
                        "diff",
                        "--no-ext-diff",
                        &self.compare_base,
                        &self.compare_head,
                        "--",
                        &path,
                    ]);
                    self.detail_title = format!("Compare · {path}");
                    self.read("git", args, ctx);
                }
            });
    }
}
