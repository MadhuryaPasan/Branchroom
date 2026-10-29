use super::*;

#[derive(Default)]
pub(super) struct ComparisonView {
    pub(super) advanced: bool,
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
        ui.horizontal_wrapped(|ui| {
            ui.heading("Compare");
            for (label, value) in [
                ("Compare against", &mut self.compare_base),
                ("With", &mut self.compare_head),
            ] {
                egui::ComboBox::from_label(label)
                    .selected_text(
                        value
                            .strip_prefix("refs/heads/")
                            .or_else(|| value.strip_prefix("refs/remotes/"))
                            .unwrap_or(value),
                    )
                    .width(150.)
                    .show_ui(ui, |ui| {
                        for branch in &repo.branches {
                            ui.selectable_value(
                                value,
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
                                value,
                                commit.id.clone(),
                                format!("{} {}", short(&commit.id), commit.subject),
                            );
                        }
                    });
            }
            ui.checkbox(&mut self.inspector_open, "Inspector");
        });
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
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                let mut selected = None;
                match &self.comparison.data {
                    None => {
                        ui.spinner();
                        ui.label("Comparing branches and files");
                    }
                    Some(Err(error)) => {
                        ui.colored_label(DANGER(), error);
                        ui.label("Choose two existing branches or commits and retry");
                        if ui.button("Retry comparison").clicked() {
                            self.comparison.key.clear();
                        }
                    }
                    Some(Ok(data)) => {
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), 100.),
                            egui::Sense::hover(),
                        );
                        let origin = rect.left_center() + egui::vec2(20., 0.);
                        let end = rect.right() - 20.;
                        for (offset, count, color) in [
                            (-25., data.left.len(), ACCENT()),
                            (25., data.right.len(), SUCCESS()),
                        ] {
                            let tip = egui::pos2(end, origin.y + offset);
                            let start = if data.ancestor.is_empty() {
                                egui::pos2(origin.x, tip.y)
                            } else {
                                origin
                            };
                            ui.painter()
                                .line_segment([start, tip], egui::Stroke::new(2., color));
                            ui.painter().circle_filled(tip, 5., color);
                            ui.painter().text(
                                tip - egui::vec2(10., 8.),
                                egui::Align2::RIGHT_BOTTOM,
                                format!(
                                    "{count} unique {}",
                                    if count == 1 { "commit" } else { "commits" }
                                ),
                                egui::FontId::proportional(12.),
                                color,
                            );
                        }
                        ui.painter().circle_filled(origin, 5., MUTED());
                        if !data.ancestor.is_empty() {
                            let response = ui.interact(
                                egui::Rect::from_center_size(origin, egui::vec2(24., 24.)),
                                egui::Id::unique("compare_starting_commit"),
                                egui::Sense::click(),
                            );
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::Role::Button,
                                    true,
                                    "Select starting commit",
                                )
                            });
                            if response.clicked() {
                                selected = Some(data.ancestor.clone());
                            }
                            if self.selected_commit == data.ancestor {
                                ui.painter().circle_stroke(
                                    origin,
                                    9.,
                                    egui::Stroke::new(2., ACCENT()),
                                );
                            }
                        }

                        ui.label(if data.ancestor.is_empty() {
                            "These histories have no common starting commit".into()
                        } else {
                            format!("Branched off here: {}", short(&data.ancestor))
                        });
                        for (label, commits) in [
                            ("Only in comparison branch", &data.left),
                            ("Only in selected branch", &data.right),
                        ] {
                            ui.heading(label);
                            if commits.is_empty() {
                                subtitle(ui, "No unique commits");
                            }
                            egui::ScrollArea::vertical()
                                .id_salt(label)
                                .max_height(220.)
                                .show_rows(ui, 28., commits.len(), |ui, range| {
                                    for i in range {
                                        let (id, subject) = &commits[i];
                                        if ui
                                            .selectable_label(self.selected_commit == *id, subject)
                                            .clicked()
                                        {
                                            selected = Some(id.clone());
                                        }
                                    }
                                });
                        }
                        ui.heading("Changed files between tips");
                        if data.files.is_empty() {
                            subtitle(
                                ui,
                                if self.preview_mode {
                                    "File differences load in a live repository"
                                } else {
                                    "No file differences between these tips"
                                },
                            );
                        }
                        egui::ScrollArea::vertical()
                            .id_salt("comparison_files")
                            .max_height(220.)
                            .show_rows(ui, 24., data.files.len(), |ui, range| {
                                for i in range {
                                    let file = &data.files[i];
                                    if ui
                                        .selectable_label(
                                            self.selected_path == file.path,
                                            file.label(),
                                        )
                                        .clicked()
                                    {
                                        self.selected_path = file.path.clone();
                                    }
                                }
                            });
                    }
                }
                if let Some(id) = selected {
                    self.selected_commit = id;
                    self.inspector_open = true;
                }
            });
        });
    }
}
