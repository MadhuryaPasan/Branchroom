use super::*;

#[derive(Default)]
pub(super) struct ComparisonView {
    pub(super) advanced: bool,
    selected_commit: String,
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
        if self.comparison.key != key {
            self.comparison.selected_commit.clear();
            self.selected_commit.clear();
        }
        if let Some(rx) = &self.comparison.receiver
            && let Ok(result) = rx.try_recv()
        {
            self.comparison.data = Some(result);
            self.comparison.receiver = None;
        }
        if self.comparison.key != key && self.comparison.receiver.is_none() {
            self.comparison.key = key.clone();
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
                    files: vec![
                        CommitFile {
                            path: "src/cache.rs".into(),
                            added: "12".into(),
                            removed: "4".into(),
                        },
                        CommitFile {
                            path: "docs/offline.md".into(),
                            added: "6".into(),
                            removed: "1".into(),
                        },
                    ],
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
        let w = ui.available_width();
        ui.set_max_width(w);
        ui.style_mut().spacing.scroll.floating = false;
        egui::ScrollArea::vertical()
            .id_salt(("comparison_workspace", &key))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
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
                    if self.selected_commit.is_empty() {
                        ui.add_sized(
                            [220., 36.],
                            egui::Label::new(
                                RichText::new("Select a commit to view details").color(MUTED()),
                            )
                            .wrap(),
                        );
                    } else if ctx.content_rect().width() < 1200. {
                        ui.toggle_value(&mut self.inspector_drawer_open, "Commit details");
                    } else {
                        ui.checkbox(&mut self.inspector_open, "Commit details");
                    }
                });
                if ctx.content_rect().width() < 1200. && self.inspector_drawer_open {
                    surface().show(ui, |ui| self.commit_inspector(ui, ctx));
                    ui.add_space(12.);
                }

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
                        let diagram_height = 126.;
                        let (diagram, _) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), diagram_height),
                            egui::Sense::hover(),
                        );
                        let center = diagram.center();
                        let shared_end =
                            egui::pos2(diagram.left() + diagram.width() * 0.32, center.y);
                        let left_end = egui::pos2(
                            diagram.left() + diagram.width() * 0.68,
                            diagram.top() + 28.,
                        );
                        let right_end = egui::pos2(
                            diagram.left() + diagram.width() * 0.68,
                            diagram.bottom() - 28.,
                        );
                        let painter = ui.painter().with_clip_rect(diagram);
                        painter.line_segment(
                            [egui::pos2(diagram.left() + 24., center.y), shared_end],
                            egui::Stroke::new(4., MUTED()),
                        );
                        painter
                            .line_segment([shared_end, left_end], egui::Stroke::new(4., ACCENT()));
                        painter.line_segment(
                            [shared_end, right_end],
                            egui::Stroke::new(4., SUCCESS()),
                        );
                        painter.circle_filled(shared_end, 7., MUTED());
                        painter.circle_filled(left_end, 7., ACCENT());
                        painter.circle_filled(right_end, 7., SUCCESS());
                        painter.text(
                            egui::pos2(diagram.left() + 24., center.y - 22.),
                            egui::Align2::LEFT_CENTER,
                            "Shared history",
                            egui::FontId::proportional(12.),
                            MUTED(),
                        );
                        for (point, text, color) in [
                            (
                                left_end,
                                format!("{} commits in {}", data.left.len(), base_name),
                                ACCENT(),
                            ),
                            (
                                right_end,
                                format!("{} commits in {}", data.right.len(), compare_name),
                                SUCCESS(),
                            ),
                        ] {
                            let clip = egui::Rect::from_min_max(
                                egui::pos2(point.x + 10., point.y - 20.),
                                egui::pos2(diagram.right() - 8., point.y + 20.),
                            );
                            let mut job = egui::text::LayoutJob::simple(
                                text,
                                egui::FontId::proportional(12.),
                                color,
                                clip.width(),
                            );
                            job.wrap.max_rows = 2;
                            job.wrap.overflow_character = Some('…');
                            let galley = painter.layout_job(job);
                            painter
                                .with_clip_rect(clip)
                                .galley(clip.left_top(), galley, color);
                        }
                        if data.left.is_empty() && data.right.is_empty() {
                            painter.text(
                                center,
                                egui::Align2::CENTER_CENTER,
                                "These branches are identical",
                                egui::FontId::proportional(14.),
                                INK(),
                            );
                        }

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
                                let row_height = 34.;
                                let max_height = ui.available_height().max(240.);
                                egui::ScrollArea::vertical()
                                    .id_salt("compare_changed_files")
                                    .auto_shrink([false, false])
                                    .max_height(max_height)
                                    .show_rows(ui, row_height, data.files.len(), |ui, range| {
                                        for index in range {
                                            let file = &data.files[index];
                                            let response = ui
                                                .with_layout(
                                                    egui::Layout::left_to_right(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        ui.add_sized(
                                                            [ui.available_width(), row_height],
                                                            egui::Button::selectable(
                                                                self.selected_path == file.path,
                                                                file.label(),
                                                            )
                                                            .truncate(),
                                                        )
                                                    },
                                                )
                                                .inner
                                                .on_hover_text(&file.path);
                                            if response.clicked() {
                                                self.selected_path = file.path.clone();
                                                selected_file = Some(file.path.clone());
                                            }
                                        }
                                    });
                            }
                        });
                        ui.add_space(12.);

                        let mut show_commit_list =
                            |ui: &mut egui::Ui, title: &str, commits: &[(String, String)]| {
                                surface().show(ui, |ui| {
                                    egui::CollapsingHeader::new(
                                        RichText::new(title).size(TEXT_SUBHEAD).strong(),
                                    )
                                    .id_salt(("compare_commit_list", &key, title))
                                    .default_open(commits.len() <= 5)
                                    .show(ui, |ui| {
                                        badge(ui, &commits.len().to_string(), MUTED());
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
                                                            .with_layout(
                                                                egui::Layout::left_to_right(
                                                                    egui::Align::Center,
                                                                ),
                                                                |ui| {
                                                                    ui.add_sized(
                                                                        [ui.available_width(), 36.],
                                                                        egui::Button::selectable(
                                                                            self.selected_commit
                                                                                == *id,
                                                                            format!(
                                                                                "{}  {}",
                                                                                short(id),
                                                                                subject
                                                                            ),
                                                                        )
                                                                        .wrap(),
                                                                    )
                                                                },
                                                            )
                                                            .inner
                                                            .on_hover_text(id);
                                                        if response.clicked() {
                                                            selected = Some(id.clone());
                                                        }
                                                    }
                                                });
                                        }
                                    });
                                });
                            };
                        let available_width = ui.available_width();
                        if available_width >= 900. {
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
                    }
                }
                if let Some(id) = selected {
                    self.comparison.selected_commit = id.clone();
                    self.selected_commit = id;
                    if ctx.content_rect().width() < 1200. {
                        self.inspector_drawer_open = true;
                    } else {
                        self.inspector_open = true;
                    }
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
