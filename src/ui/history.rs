use super::*;

#[derive(Default)]
pub(super) struct HistoryView {
    dataset: std::sync::Weak<RepoState>,
    query: String,
    search_key: String,
    search_receiver: Option<Receiver<Result<std::collections::HashSet<String>>>>,
    search_ids: Option<Result<std::collections::HashSet<String>>>,
    visible: std::sync::Arc<Vec<usize>>,
    pub(super) resolved_widths: [f32; 4],
}
impl HistoryView {
    fn visible(
        &mut self,
        repo: &std::sync::Arc<RepoState>,
        query: &str,
    ) -> std::sync::Arc<Vec<usize>> {
        let same = self
            .dataset
            .upgrade()
            .is_some_and(|cached| std::sync::Arc::ptr_eq(&cached, repo));
        let structured = filter_terms(query).iter().any(|t| {
            ["file:", "after:", "before:", "author:", "branch:"]
                .iter()
                .any(|p| t.starts_with(p))
        });
        if structured {
            let tips = repo
                .branches
                .iter()
                .map(|b| b.id.as_str())
                .collect::<String>();
            let key = format!("{}:{}:{}:{}", repo.root.display(), repo.head, tips, query);
            if let Some(rx) = &self.search_receiver
                && let Ok(result) = rx.try_recv()
            {
                self.search_ids = Some(result);
                self.search_receiver = None;
                self.query.clear();
            }
            if self.search_key != key && self.search_receiver.is_none() {
                self.search_key = key;
                self.search_ids = None;
                let (tx, rx) = mpsc::channel();
                self.search_receiver = Some(rx);
                let root = repo.root.clone();
                let query = query.to_owned();
                std::thread::spawn(move || {
                    let mut args = process::strings(&[
                        "log",
                        "--branches",
                        "--remotes",
                        "--tags",
                        "HEAD",
                        "--format=%H",
                        "-z",
                        "--fixed-strings",
                    ]);
                    let mut files = vec![];
                    for token in filter_terms(&query) {
                        if let Some(file) = token.strip_prefix("file:") {
                            files.push(file.to_owned());
                        } else if let Some(author) = token.strip_prefix("author:") {
                            args.push(format!("--author={author}"));
                        } else if let Some(branch) = token.strip_prefix("branch:") {
                            match git::resolve(&root, branch) {
                                Ok(id) => {
                                    args.retain(|a| {
                                        !["--branches", "--remotes", "--tags", "HEAD"]
                                            .contains(&a.as_str())
                                    });
                                    args.push(id);
                                }
                                Err(error) => {
                                    let _ = tx.send(Err(error));
                                    return;
                                }
                            }
                        } else if let Some(date) = token.strip_prefix("after:") {
                            args.push(format!("--since={date}"));
                        } else if let Some(date) = token.strip_prefix("before:") {
                            args.push(format!("--until={date}"));
                        } else {
                            args.push(format!("--grep={token}"));
                        }
                    }
                    args.push("--".into());
                    args.extend(files);
                    let result = process::stdout("git", &root, &args).map(|out| {
                        out.split_terminator('\0')
                            .map(|s| s.trim().to_owned())
                            .collect()
                    });
                    let _ = tx.send(result);
                });
            }
        }
        if !same || query != self.query {
            let needle = query.to_lowercase();
            self.visible = std::sync::Arc::new(
                repo.commits
                    .iter()
                    .enumerate()
                    .filter_map(|(i, c)| {
                        (if structured {
                            self.search_ids
                                .as_ref()
                                .and_then(|r| r.as_ref().ok())
                                .is_some_and(|ids| ids.contains(&c.id))
                        } else {
                            needle.is_empty()
                                || format!(
                                    "{} {} {} {} {}",
                                    c.subject, c.author, c.refs, c.id, c.date
                                )
                                .to_lowercase()
                                .contains(&needle)
                        })
                        .then_some(i)
                    })
                    .collect(),
            );
            self.dataset = std::sync::Arc::downgrade(repo);
            self.query = query.into();
        }
        self.visible.clone()
    }
}
impl Branchroom {
    pub(super) fn history(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let repo = self.repo.as_ref().unwrap().clone();
        egui::Panel::top("history_controls")
            .frame(
                egui::Frame::new()
                    .fill(PAPER())
                    .inner_margin(egui::Margin::symmetric(22, 16)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Graph");
                    badge(
                        ui,
                        &format!(
                            "{} {}",
                            repo.commits.len(),
                            if repo.commits.len() == 1 {
                                "commit"
                            } else {
                                "commits"
                            }
                        ),
                        MUTED(),
                    );
                });
                ui.add_space(12.);
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Filter").color(MUTED()));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.query)
                            .desired_width((ui.available_width() * 0.38).clamp(140., 360.))
                            .hint_text("Message, author, branch, file:path, after:YYYY-MM-DD"),
                    );
                    if ctx.content_rect().width()>=800. {
                        badge(ui, &repo.branch, ACCENT());
                        ui.label(RichText::new("Checked-out branch").size(12.).color(MUTED())).on_hover_text("All loaded branches share this graph. Select a branch label to inspect its tip.");
                    }
                    ui.menu_button("View", |ui| {
                        egui::CollapsingHeader::new("Graph gestures").show(ui,|ui| {
                            ui.set_max_width(300.);
                            ui.label("Drag a branch label onto another branch to merge");
                            ui.label("Drag a commit onto a branch to copy it");
                            ui.label("Drop near a commit's top or bottom edge to reorder");
                            ui.label("Drop onto a commit's center to combine commits");
                            ui.label("Drag the current branch label to an earlier commit to undo");
                            ui.label("Drag included working changes onto a commit to amend or fix up");
                            if ui.button("Open keyboard form").clicked(){self.manipulation.advanced=true;ui.close();}
                        });
                    ui.vertical(|ui| {
                        if ui
                            .selectable_label(self.inspector_open, "Inspector")
                            .clicked()
                        {
                            self.inspector_open = !self.inspector_open;
                        }
                        ui.selectable_value(&mut self.history_list, true, "List");
                        ui.selectable_value(&mut self.history_list, false, "Graph");
                    });

                        ui.add(egui::Slider::new(&mut self.graph_zoom, 0.75..=2.5).text("Graph zoom"));
                    ui.menu_button("Columns", |ui| {
                        ui.set_min_width(270.);
                        subtitle(ui,"Drag the header dividers or enter widths here.");
                        let graph_auto=(self.graph.iter().map(|r|r.width).max().unwrap_or(1) as f32*16.+34.).clamp(72.,4096.);
                        let mut changed=false;
                        let mut graph=if self.settings.history_columns.graph>0.{self.settings.history_columns.graph}else{graph_auto};
                        if column_input(ui,"Graph",&mut graph,72.0..=4096.0){self.settings.history_columns.graph=graph;changed=true;}
                        let mut commit=if self.settings.history_columns.commit>0.{self.settings.history_columns.commit}else{self.history_view.resolved_widths[1].max(240.)};
                        if column_input(ui,"Commit",&mut commit,200.0..=1800.0){self.settings.history_columns.commit=commit;changed=true;}
                        if column_input(ui,"Author",&mut self.settings.history_columns.author,96.0..=600.0){changed=true;}
                        if column_input(ui,"Date",&mut self.settings.history_columns.date,90.0..=300.0){changed=true;}
                        if ui.button("Reset column widths").clicked(){self.settings.history_columns=Default::default();changed=true;}
                        if changed&&!self.preview_mode && let Err(error)=self.settings.save(){self.report(Err(error));}
                    });
                    ui.menu_button("Density", |ui| {
                        ui.selectable_value(&mut self.density, 0, "Compact");
                        ui.selectable_value(&mut self.density, 1, "Comfortable");
                        ui.selectable_value(&mut self.density, 2, "Spacious");
                    });
                    });
                    if !self.query.is_empty() && ui.small_button("Clear").clicked() {
                        self.query.clear();
                    }
                });
            });
        self.patch_drawer(ui);
        self.graph_inspector(ui, ctx);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(PAPER())
                    .inner_margin(egui::Margin::symmetric(12, 0)),
            )
            .show(ui, |ui| {
                let working = surface().show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(24., 24.), egui::Sense::hover());
                        for segment in 0..8 {
                            let angle = segment as f32 * std::f32::consts::TAU / 8.;
                            let point = |a: f32| rect.center() + egui::vec2(a.cos(), a.sin()) * 7.;
                            ui.painter().line_segment(
                                [point(angle), point(angle + 0.45)],
                                egui::Stroke::new(1.5, MUTED()),
                            );
                        }
                        let source = ui.add(
                            egui::Label::new(format!(
                                "Working copy - {} changed files",
                                repo.changes.len()
                            ))
                            .sense(egui::Sense::drag()),
                        );
                        source.dnd_set_drag_payload(manipulation::Payload::Working);
                        if ui
                            .add_enabled(
                                !repo.changes.is_empty(),
                                egui::Button::new("Commit changes").fill(PRIMARY()),
                            )
                            .clicked()
                        {
                            self.page = Page::Changes;
                            self.compact_changes_tab = 2;
                        }
                    });
                });
                self.graph_drop(ui, &working.response, manipulation::Target::Working);
                ui.horizontal_wrapped(|ui| {
                    for branch in repo.branches.iter().filter(|b| !b.remote) {
                        let lane = repo
                            .commits
                            .iter()
                            .position(|c| c.id == branch.id)
                            .and_then(|i| self.graph.get(i))
                            .map(|r| r.lane);
                        if let Some(lane) = lane {
                            let selected = self.selected_commit == branch.id;
                            let response = ui.add(
                                egui::Button::selectable(
                                    selected,
                                    RichText::new(&branch.name).color(lane_color(lane)),
                                )
                                .sense(egui::Sense::click_and_drag()),
                            );
                            response.dnd_set_drag_payload(manipulation::Payload::Branch {
                                name: branch.name.clone(),
                                current: branch.current,
                                reference: format!("refs/heads/{}", branch.name),
                            });
                            self.graph_drop(
                                ui,
                                &response,
                                manipulation::Target::Branch(&format!(
                                    "refs/heads/{}",
                                    branch.name
                                )),
                            );
                            if response.clicked() {
                                self.selected_commit = branch.id.clone();
                                self.inspector_open = true;
                            }
                        }
                    }
                });
                let commits = &repo.commits;
                let visible = self.history_view.visible(&repo, &self.query);
                let rows = self.graph.clone();
                if visible.is_empty() {
                    ui.add_space(40.);
                    if let Some(Err(error)) = &self.history_view.search_ids {
                        ui.colored_label(DANGER(), error);
                    }
                    ui.heading(if self.history_view.search_receiver.is_some() {
                        "Searching history"
                    } else if self.query.is_empty() {
                        "No commits yet"
                    } else {
                        "No matching commits"
                    });
                    subtitle(
                        ui,
                        if self.query.is_empty() {
                            "Stage your files in Changes to create the first commit."
                        } else {
                            "Try a different message, author, branch, tag, file or date."
                        },
                    );
                    if !self.query.is_empty() && ui.button("Clear filter").clicked() {
                        self.query.clear();
                    }
                } else {
                    let view_width = ui.available_width();
                    let required_graph = (rows.iter().map(|r| r.width).max().unwrap_or(1) as f32
                        * 16.
                        * self.graph_zoom
                        + 34.)
                        .clamp(72., 4096.);
                    let columns = &self.settings.history_columns;
                    let graph_width = if self.history_list {
                        24.
                    } else if columns.graph > 0. {
                        columns.graph.clamp(72., 4096.)
                    } else {
                        required_graph
                    };
                    let show_author = view_width >= 620.;
                    let author_width = if show_author {
                        columns.author.clamp(96., 600.)
                    } else {
                        0.
                    };
                    let date_width = columns.date.clamp(90., 300.);
                    let commit_width = if columns.commit > 0. {
                        columns.commit.clamp(200., 1800.)
                    } else {
                        (view_width - graph_width - author_width - date_width).max(240.)
                    };
                    let widths = [graph_width, commit_width, author_width, date_width];
                    self.history_view.resolved_widths = widths;
                    let table_width = widths.iter().sum::<f32>();
                    let row_height = [36., 48., 60.][self.density.min(2)] * self.graph_zoom;
                    egui::ScrollArea::horizontal()
                        .scroll_source(egui::scroll_area::ScrollSource {
                            drag: egui::scroll_area::DragScroll::Always,
                            ..Default::default()
                        })
                        .id_salt("history_horizontal")
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.set_min_width(table_width);
                            let (header, _) = ui.allocate_exact_size(
                                egui::vec2(table_width, 36.),
                                egui::Sense::hover(),
                            );
                            ui.painter().rect_filled(header, 0., SURFACE());
                            let x = [
                                header.left(),
                                header.left() + widths[0],
                                header.left() + widths[0] + widths[1],
                                header.left() + widths[0] + widths[1] + widths[2],
                                header.right(),
                            ];
                            for (column, label) in
                                ["Graph", "Commit", "Author", "When"].iter().enumerate()
                            {
                                if (column == 0 && self.history_list)
                                    || (column == 2 && !show_author)
                                {
                                    continue;
                                }
                                let cell = egui::Rect::from_min_max(
                                    egui::pos2(x[column] + 10., header.top()),
                                    egui::pos2(x[column + 1] - 8., header.bottom()),
                                );
                                text_at(ui, cell, label, 11., MUTED(), false);
                            }
                            for index in 0..4 {
                                if (index == 0 && self.history_list) || (index == 2 && !show_author)
                                {
                                    continue;
                                }
                                let boundary = x[index + 1];
                                let hit = egui::Rect::from_center_size(
                                    egui::pos2(boundary, header.center().y),
                                    egui::vec2(24., header.height()),
                                );
                                let response = ui.interact(
                                    hit,
                                    egui::Id::unique(("history_column_resize", index)),
                                    egui::Sense::drag(),
                                );
                                let response = response
                                    .on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
                                    .on_hover_text(
                                        "Drag to resize. Use Columns for keyboard controls.",
                                    );
                                let color = if response.hovered() || response.dragged() {
                                    ACCENT()
                                } else {
                                    LINE()
                                };
                                ui.painter().line_segment(
                                    [
                                        egui::pos2(boundary, header.top() + 7.),
                                        egui::pos2(boundary, header.bottom() - 7.),
                                    ],
                                    egui::Stroke::new(
                                        if response.hovered() { 2. } else { 1. },
                                        color,
                                    ),
                                );
                                if response.dragged() {
                                    let new = (widths[index] + response.drag_delta().x).clamp(
                                        [72., 200., 96., 90.][index],
                                        [4096., 1800., 600., 300.][index],
                                    );
                                    match index {
                                        0 => self.settings.history_columns.graph = new,
                                        1 => self.settings.history_columns.commit = new,
                                        2 => self.settings.history_columns.author = new,
                                        _ => self.settings.history_columns.date = new,
                                    }
                                    ui.ctx().request_repaint();
                                }
                                if response.drag_stopped()
                                    && !self.preview_mode
                                    && let Err(error) = self.settings.save()
                                {
                                    self.report(Err(error));
                                }
                            }
                            ui.spacing_mut().item_spacing.y = 0.;
                            egui::ScrollArea::vertical()
                                .scroll_source(egui::scroll_area::ScrollSource {
                                    drag: egui::scroll_area::DragScroll::Always,
                                    ..Default::default()
                                })
                                .id_salt("history_rows")
                                .max_height((ui.available_height() - 48.).max(100.))
                                .auto_shrink([false, false])
                                .show_rows(ui, row_height, visible.len(), |ui, range| {
                                    for &index in &visible[range] {
                                        let commit = &commits[index];
                                        let selected = self.selected_commit == commit.id;
                                        let mut decorations = commit.refs.replace("HEAD -> ", "");
                                        if let Some(Ok(data)) = &self.graph_context.data {
                                            for pr in data.reviews.iter().filter(|p| {
                                                p.commit == commit.id
                                                    || self
                                                        .graph_context
                                                        .linked_to(&repo.root, &commit.id, p.number)
                                            }) {
                                                decorations.push_str(&format!(
                                                    "  PR #{} {} - {}",
                                                    pr.number, pr.state, pr.ci
                                                ));
                                            }
                                        }

                                        let response = ui.add_sized(
                                            [table_width, row_height],
                                            egui::Button::new("")
                                                .selected(selected)
                                                .frame_when_inactive(selected)
                                                .corner_radius(0)
                                                .sense(egui::Sense::click_and_drag()),
                                        );
                                        response.widget_info(|| {
                                            egui::WidgetInfo::labeled(
                                                egui::Role::Button,
                                                ui.is_enabled(),
                                                format!(
                                                    "{} — {} — {}",
                                                    commit.subject, commit.author, commit.date
                                                ),
                                            )
                                        });
                                        response.dnd_set_drag_payload(
                                            manipulation::Payload::Commit(commit.id.clone()),
                                        );
                                        let pointer_y = ui
                                            .input(|i| i.pointer.interact_pos())
                                            .map(|p| p.y - response.rect.top())
                                            .unwrap_or(response.rect.height() / 2.);
                                        let edge = if pointer_y < response.rect.height() * 0.25 {
                                            -1
                                        } else if pointer_y > response.rect.height() * 0.75 {
                                            1
                                        } else {
                                            0
                                        };
                                        self.graph_drop(
                                            ui,
                                            &response,
                                            manipulation::Target::Commit {
                                                id: &commit.id,
                                                edge,
                                            },
                                        );
                                        let rect = response.rect;
                                        let row = &rows[index];
                                        let mid = rect.center().y;
                                        if !self.history_list {
                                            let x = |lane: usize| {
                                                rect.left()
                                                    + 17.
                                                    + lane as f32 * 16. * self.graph_zoom
                                            };
                                            let painter = ui.painter().with_clip_rect(
                                                egui::Rect::from_min_max(
                                                    rect.min,
                                                    egui::pos2(
                                                        rect.left() + graph_width,
                                                        rect.bottom(),
                                                    ),
                                                ),
                                            );
                                            if self.query.is_empty() {
                                                for (from, to) in &row.edges {
                                                    let start = egui::pos2(x(*from), mid);
                                                    let end = egui::pos2(x(*to), rect.bottom());
                                                    painter.line_segment(
                                                        [start, end],
                                                        egui::Stroke::new(1.8, lane_color(*from)),
                                                    );
                                                    if *from != row.lane {
                                                        painter.line_segment(
                                                            [
                                                                egui::pos2(x(*from), rect.top()),
                                                                start,
                                                            ],
                                                            egui::Stroke::new(
                                                                1.8,
                                                                lane_color(*from),
                                                            ),
                                                        );
                                                    }
                                                }
                                            }
                                            painter.line_segment(
                                                [
                                                    egui::pos2(x(row.lane), rect.top()),
                                                    egui::pos2(x(row.lane), mid),
                                                ],
                                                egui::Stroke::new(1.8, lane_color(row.lane)),
                                            );
                                            if selected {
                                                painter.circle_stroke(
                                                    egui::pos2(x(row.lane), mid),
                                                    7.2,
                                                    egui::Stroke::new(1., ACCENT()),
                                                );
                                            }
                                            painter.circle_filled(
                                                egui::pos2(x(row.lane), mid),
                                                4.,
                                                lane_color(row.lane),
                                            );
                                        }
                                        let start = rect.left() + widths[0] + 10.;
                                        let commit_end = rect.left() + widths[0] + widths[1];
                                        let author_end = commit_end + widths[2];
                                        let title_rect = egui::Rect::from_min_max(
                                            egui::pos2(start, rect.top()),
                                            egui::pos2(
                                                commit_end - 10.,
                                                rect.bottom()
                                                    - if !decorations.is_empty()
                                                        && row_height >= 48.
                                                    {
                                                        14.
                                                    } else {
                                                        0.
                                                    },
                                            ),
                                        );
                                        text_at(ui, title_rect, &commit.subject, 14., INK(), false);
                                        if show_author {
                                            let center =
                                                egui::pos2(commit_end + 18., rect.center().y);
                                            ui.painter().circle_filled(center, 10., SELECTED());
                                            ui.painter().text(
                                                center,
                                                egui::Align2::CENTER_CENTER,
                                                commit.author.chars().next().unwrap_or('?'),
                                                egui::FontId::proportional(12.),
                                                INK(),
                                            );
                                        }
                                        text_at(
                                            ui,
                                            egui::Rect::from_min_max(
                                                egui::pos2(commit_end + 34., rect.top()),
                                                egui::pos2(author_end - 8., rect.bottom()),
                                            ),
                                            &commit.author,
                                            12.,
                                            MUTED(),
                                            false,
                                        );
                                        text_at(
                                            ui,
                                            egui::Rect::from_min_max(
                                                egui::pos2(author_end + 10., rect.top()),
                                                egui::pos2(rect.right() - 8., rect.bottom()),
                                            ),
                                            &commit.date,
                                            12.,
                                            MUTED(),
                                            false,
                                        );
                                        if !decorations.is_empty() && row_height >= 48. {
                                            let label = decorations;
                                            let ref_width = (label.chars().count() as f32 * 6.5
                                                + 12.)
                                                .min((commit_end - start - 10.).max(0.));
                                            let badge_rect = egui::Rect::from_min_size(
                                                egui::pos2(start, rect.bottom() - 17.),
                                                egui::vec2(ref_width, 15.),
                                            );
                                            ui.painter().rect_filled(badge_rect, 3., SELECTED());
                                            text_at(
                                                ui,
                                                badge_rect.shrink2(egui::vec2(5., 0.)),
                                                &label,
                                                11.,
                                                ACCENT(),
                                                false,
                                            );
                                        }
                                        if response.hovered()
                                            && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO)
                                        {
                                            self.selected_commit = commit.id.clone();
                                        }
                                        if response.double_clicked() {
                                            self.selected_commit = commit.id.clone();
                                            self.view_commit(&commit.id, ctx);
                                        } else if response.clicked() {
                                            self.selected_commit = commit.id.clone();
                                            self.inspector_open = true;
                                            self.commit_files.clear();
                                            self.commit_files_for = String::new();
                                            self.commit_files_requested.clear();
                                        }
                                        response.context_menu(|ui| {
                                            if ui.button("View patch").clicked() {
                                                self.selected_commit = commit.id.clone();
                                                self.view_commit(&commit.id, ctx);
                                                ui.close();
                                            }
                                            if ui.button("Copy SHA").clicked() {
                                                ctx.copy_text(commit.id.clone());
                                                ui.close();
                                            }
                                        });
                                    }
                                });
                        });
                }
                ui.add_space(16.);
                if repo.commits.len() >= self.history_limit {
                    if ui
                        .button(format!(
                            "Load 500 more commits · {} loaded",
                            repo.commits.len()
                        ))
                        .clicked()
                    {
                        self.history_limit += 500;
                        self.refresh_at(self.path(), ctx);
                    }
                } else if !repo.commits.is_empty() {
                    subtitle(ui, "Beginning of this branch's history");
                }
            });
    }

    pub(super) fn graph_inspector(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.inspector_open {
            if ui.available_width() >= 800. {
                egui::Panel::right("commit_inspector")
                    .resizable(true)
                    .default_size(300.)
                    .size_range(260.0..=420.0)
                    .frame(egui::Frame::new().fill(SURFACE()).inner_margin(20))
                    .show(ui, |ui| {
                        self.commit_inspector(ui, ctx);
                    });
            } else {
                let mut open = true;
                egui::Window::new("Commit inspector")
                    .open(&mut open)
                    .default_width(310.)
                    .max_height((ctx.content_rect().height() - 120.).max(150.))
                    .show(ctx, |ui| {
                        self.commit_inspector(ui, ctx);
                    });
                if !open {
                    self.inspector_open = false;
                }
            }
        }
    }

    pub(super) fn view_commit(&mut self, id: &str, ctx: &egui::Context) {
        self.detail_title = format!("Commit {} · patch", short(id));
        self.read(
            "git",
            process::strings(&["show", "--format=fuller", "--stat", "--patch", id]),
            ctx,
        );
    }

    fn commit_inspector(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if !self.preview_mode
            && !self.selected_commit.is_empty()
            && self.commit_files_for != self.selected_commit
            && self.commit_files_requested != self.selected_commit
            && self.receiver.is_none()
        {
            self.commit_files_requested = self.selected_commit.clone();
            self.execute(Task::CommitFiles(self.selected_commit.clone()), ctx);
        }
        egui::ScrollArea::vertical().id_salt("commit_details").show(ui,|ui| {
            section(ui,"Commit details");
            ui.add_space(8.);
            let commit=self.repo.as_ref().and_then(|r|r.commits.iter().chain(r.branch_commits.iter()).find(|c|c.id==self.selected_commit)).cloned();
            let Some(commit)=commit else{ui.heading("Commit details");if self.selected_commit.is_empty(){subtitle(ui,"Choose a commit to review its changes and details.");}else{ui.label(&self.selected_commit);subtitle(ui,"This commit is outside the loaded graph. Open its patch or load more history.");let id=self.selected_commit.clone();if ui.button("View patch").clicked(){self.view_commit(&id,ctx);}}return;};
            ui.add(egui::Label::new(RichText::new(&commit.subject).size(18.).strong()).wrap());
            ui.add_space(4.);
            ui.add(egui::Label::new(RichText::new(format!("{} · {}",commit.author,commit.date)).color(MUTED())).wrap());
            if !commit.refs.is_empty(){ui.add_space(8.);ui.horizontal_wrapped(|ui|{for name in commit.refs.split(", "){badge(ui,name,ACCENT());}});}
            if let Some(Err(_))=&self.graph_context.data {subtitle(ui,"GitHub status is unavailable. Check the GitHub connection in Pull requests.");}
            if let Some(Ok(data))=&self.graph_context.data {
                if let Some(warning)=&data.identity_warning {ui.colored_label(WARNING(),warning);}
                for pr in data.reviews.iter().filter(|p|p.commit==commit.id||self.graph_context.linked_to(&self.path(),&commit.id,p.number)){ui.hyperlink_to(format!("PR #{} - {} - {}",pr.number,pr.state,pr.ci),&pr.url);}
            }
            ui.add_space(16.);
            if ui.button("View patch").clicked(){self.view_commit(&commit.id,ctx);}
            if ui.button("Open this commit in IDE…").on_hover_text("Inspect or run this exact version in a separate directory, without changing your current checkout.").clicked(){self.begin_review(&commit.id,None);}
            ui.menu_button("Commit actions…",|ui| {
                if ui.button("Find related pull requests").on_hover_text("Search GitHub for pull requests containing this commit; review or open their code in an IDE.").clicked(){self.page=Page::Github;self.pr_tab=0;self.pull_requests.clear();self.prs_loaded=false;self.execute(Task::CommitPullRequests(commit.id.clone()),ctx);ui.close();}
                if ui.add_enabled(commit.id == self.repo.as_ref().unwrap().head,egui::Button::new("Undo latest local commit")).on_hover_text("Only for the selected branch tip. Keep its changes locally; published commits are refused.").clicked(){self.pending=Some(Pending{title:"Undo latest commit".into(),explanation:"Move the latest unpublished commit back to working changes. A recovery ref preserves the original history.".into(),task:Task::History(branchroom::history::Action::Undo)});ui.close();}
                if ui.button("Reset branch to this commit…").clicked(){self.pending=Some(Pending{title:"Reset branch to commit".into(),explanation:"Move this branch back while retaining removed commits as working changes. Published commits are refused.".into(),task:Task::History(branchroom::history::Action::Reset(commit.id.clone()))});ui.close();}
                if ui.button("Squash or reorder local commits…").clicked(){self.page=Page::Compare;ui.close();}
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
            ui.add_space(12.);
            section(ui,"Changed files");
            if self.commit_files_for!=commit.id {
                if self.preview_mode { subtitle(ui,"Changed files are available in a live repository"); }
                else if self.receiver.is_some(){subtitle(ui,"Loading changed files…");}
                else if ui.button("Retry loading changed files").clicked(){self.commit_files_requested=commit.id.clone();self.execute(Task::CommitFiles(commit.id.clone()),ctx);}
            } else if self.commit_files.is_empty(){subtitle(ui,"No changed files in this commit.");}
            else {
                for file in self.commit_files.clone(){
                    if ui.add(egui::Button::new(file.label()).selected(self.selected_path==file.path).truncate()).on_hover_text(&file.path).clicked(){
                        self.detail_title=format!("{} · {}",short(&commit.id),file.path);
                        let mut args=process::strings(&["show","--format=","--first-parent",&commit.id,"--"]);self.selected_path=file.path.clone();args.push(file.path);self.read("git",args,ctx);
                    }
                }
            }
            ui.add_space(16.);
            egui::CollapsingHeader::new("Technical details").show(ui,|ui|{
                section(ui,"Commit ID");
                ui.add(egui::Label::new(RichText::new(&commit.id).monospace().size(12.)).wrap().selectable(true));
                if ui.button("Copy full SHA").clicked(){ctx.copy_text(commit.id.clone());}
                ui.add_space(8.);section(ui,"Parents");
                if commit.parents.is_empty(){subtitle(ui,"Root commit");}
                for parent in &commit.parents {if ui.button(short(parent)).on_hover_text(parent).clicked(){self.selected_commit=parent.clone();}}
            });
        });
    }
}

pub(super) fn column_input(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
) -> bool {
    ui.horizontal(|ui| {
        ui.add_sized([80., 24.], egui::Label::new(label));
        let response = ui.add(
            egui::DragValue::new(value)
                .range(range)
                .speed(2.)
                .suffix(" px"),
        );
        response.widget_info(|| {
            let mut info = egui::WidgetInfo::drag_value(ui.is_enabled(), f64::from(*value));
            info.label = Some(format!("{label} column width"));
            info
        });
        response
            .on_hover_text("Click to type a width, or drag to adjust it.")
            .changed()
    })
    .inner
}

fn text_at(ui: &egui::Ui, rect: egui::Rect, text: &str, size: f32, color: Color32, mono: bool) {
    if rect.width() <= 0. {
        return;
    }
    let mut job = egui::text::LayoutJob::simple(
        text.into(),
        if mono {
            egui::FontId::monospace(size)
        } else {
            egui::FontId::proportional(size)
        },
        color,
        rect.width(),
    );
    job.wrap.max_rows = 1;
    job.wrap.overflow_character = Some('…');
    let galley = ui.painter().layout_job(job);
    ui.painter().with_clip_rect(rect).galley(
        egui::pos2(rect.left(), rect.center().y - galley.size().y / 2.),
        galley,
        color,
    );
}

fn filter_terms(query: &str) -> Vec<String> {
    let mut terms = vec![];
    let mut current = String::new();
    let mut quoted = false;
    for c in query.chars() {
        if c == '"' {
            quoted = !quoted;
        } else if c.is_whitespace() && !quoted {
            if !current.is_empty() {
                terms.push(std::mem::take(&mut current));
            }
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        terms.push(current);
    }
    terms
}
#[cfg(test)]
mod filter_tests {
    #[test]
    fn quoted_file_paths_and_author_names_stay_single_filters() {
        assert_eq!(
            super::filter_terms(
                "file:\"docs/Release notes.md\" author:\"Ada Lovelace\" after:2026-01-01"
            ),
            [
                "file:docs/Release notes.md",
                "author:Ada Lovelace",
                "after:2026-01-01"
            ]
        );
    }
}
