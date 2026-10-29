use super::*;
use branchroom::operation::{Kind, Preview};
#[derive(Clone, Debug)]
pub(super) enum Payload {
    Branch {
        name: String,
        current: bool,
        reference: String,
    },
    Commit(String),
    Working,
}
pub(super) enum Target<'a> {
    Branch(&'a str),
    Commit { id: &'a str, edge: i8 },
    Working,
}
#[derive(Default)]
pub(super) struct Manipulation {
    pub(super) draft: Option<Kind>,
    pub(super) review: Option<Preview>,
    root: PathBuf,
    receiver: Option<Receiver<(u64, Result<Preview>)>>,
    cancellation: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    generation: u64,
    requested_at: Option<Instant>,
    error: Option<String>,
    released: bool,
    hover_invalid: bool,
    was_dragging: bool,
    pub(super) advanced: bool,
    mode: usize,
    source: String,
    target: String,
    order: String,
    message: String,
    new_branch: String,
    risks: std::collections::HashMap<String, usize>,
}
impl Manipulation {
    pub(super) fn committed(&mut self) {
        self.message.clear();
    }
    pub(super) fn active(&self) -> bool {
        self.draft.is_some() || self.receiver.is_some()
    }
    fn cancel(&mut self) {
        if let Some(token) = self.cancellation.take() {
            token.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.receiver = None;
        self.draft = None;
        self.review = None;
        self.error = None;
        self.generation += 1;
        self.requested_at = None;
        self.was_dragging = false;
        self.released = false;
        self.hover_invalid = false;
    }
}
fn empty_message(kind: &Kind) -> bool {
    match kind {
        Kind::Commit { message }
        | Kind::Amend { message, .. }
        | Kind::Squash { message, .. }
        | Kind::SquashRange { message, .. } => message.trim().is_empty(),
        _ => false,
    }
}
fn title(kind: &Kind) -> &'static str {
    match kind {
        Kind::Undo => "Undo latest commit",
        Kind::Merge { .. } => "Merge branches",
        Kind::CherryPick { .. } => "Copy a commit",
        Kind::Reorder { .. } => "Reorder commits",
        Kind::Squash { .. } | Kind::SquashRange { .. } => "Combine commits",
        Kind::Reset { .. } => "Move the branch tip",
        Kind::Commit { .. } => "Commit included changes",
        Kind::Amend { .. } => "Add changes to a commit",
        Kind::MoveToBranch { .. } => "Move commits to a new branch",
    }
}
fn ghost(
    ui: &mut egui::Ui,
    label: &str,
    commits: &[git::Commit],
    color: Color32,
    dashed: bool,
    empty_notice: &str,
) {
    section(ui, label);
    let count = commits.len().min(5);
    let height = (count.max(1) * 24 + 8) as f32;
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().max(80.), height),
        egui::Sense::hover(),
    );
    let painter = ui.painter().with_clip_rect(rect);
    let rows = graph::layout(&commits[..count]);
    for (index, commit) in commits.iter().take(count).enumerate() {
        let row = &rows[index];
        let y = rect.top() + 16. + index as f32 * 24.;
        let x = rect.left() + 10. + row.lane as f32 * 12.;
        let lane_x = |lane: usize| rect.left() + 10. + lane as f32 * 12.;
        for (from, to) in &row.edges {
            let start = egui::pos2(lane_x(*from), y);
            painter.line_segment(
                [start, egui::pos2(lane_x(*to), y + 12.)],
                egui::Stroke::new(1., color),
            );
            if *from != row.lane {
                painter.line_segment(
                    [egui::pos2(lane_x(*from), y - 12.), start],
                    egui::Stroke::new(1., color),
                );
            }
        }
        painter.line_segment(
            [egui::pos2(x, y - 12.), egui::pos2(x, y)],
            egui::Stroke::new(1., color),
        );
        if dashed {
            for i in 0..6 {
                let angle = i as f32 * std::f32::consts::TAU / 6.;
                let point = |a: f32| egui::pos2(x, y) + egui::vec2(a.cos(), a.sin()) * 4.;
                painter.line_segment(
                    [point(angle), point(angle + 0.6)],
                    egui::Stroke::new(1.5, color),
                );
            }
        } else {
            painter.circle_filled(egui::pos2(x, y), 3.5, color);
        }
        let mut job = egui::text::LayoutJob::simple(
            commit.subject.clone(),
            egui::FontId::proportional(12.),
            INK(),
            (rect.width() - 38. - row.width as f32 * 12.).max(30.),
        );
        job.wrap.max_rows = 1;
        job.wrap.overflow_character = Some('…');
        painter.galley(
            egui::pos2(rect.left() + 26. + row.width as f32 * 12., y - 7.),
            ui.fonts_mut(|f| f.layout_job(job)),
            INK(),
        );
    }
    if commits.is_empty() {
        if dashed {
            let center = rect.min + egui::vec2(9., 16.);
            for i in 0..6 {
                let a = i as f32 * std::f32::consts::TAU / 6.;
                let p = |v: f32| center + egui::vec2(v.cos(), v.sin()) * 5.;
                painter.line_segment([p(a), p(a + 0.6)], egui::Stroke::new(1.5, color));
            }
        }
        painter.text(
            rect.min + egui::vec2(if dashed { 24. } else { 10. }, 10.),
            egui::Align2::LEFT_TOP,
            if dashed {
                empty_notice
            } else {
                "No commits yet"
            },
            egui::FontId::proportional(12.),
            color,
        );
    }
}
impl Branchroom {
    pub(super) fn cancel_graph_action(&mut self, ctx: &egui::Context) {
        self.manipulation.cancel();
        self.manipulation.advanced = false;
        egui::DragAndDrop::clear_payload(ctx);
    }
    pub(super) fn confirm_graph_action(&mut self, ctx: &egui::Context) {
        if self.receiver.is_some()
            || self.manipulation.advanced
            || self.manipulation.error.is_some()
            || egui::DragAndDrop::has_any_payload(ctx)
        {
            return;
        }
        if self
            .manipulation
            .review
            .as_ref()
            .is_some_and(|r| r.blocked.is_none())
        {
            let review = self.manipulation.review.take().unwrap();
            self.manipulation.cancel();
            self.execute(Task::GraphApply(Box::new(review)), ctx);
        }
    }
    pub(super) fn reviewed_graph_kind(&self, task: &Task) -> Option<Kind> {
        match task {
            Task::Merge { target, reference } => Some(Kind::Merge {
                source: reference.clone(),
                target: format!("refs/heads/{target}"),
            }),
            Task::Commit(plan) => {
                if plan.amend {
                    plan.head.as_ref().map(|id| Kind::Amend {
                        commit: id.clone(),
                        message: plan.message.clone(),
                    })
                } else {
                    Some(Kind::Commit {
                        message: plan.message.clone(),
                    })
                }
            }
            Task::History(action) => match action {
                branchroom::history::Action::Undo => Some(Kind::Undo),
                branchroom::history::Action::Reset(id) => Some(Kind::Reset { commit: id.clone() }),
                branchroom::history::Action::Squash { count, message } => Some(Kind::SquashRange {
                    count: *count,
                    message: message.clone(),
                }),
                branchroom::history::Action::Reorder(order) => Some(Kind::Reorder {
                    order: order.clone(),
                }),
            },
            Task::Command { program, args, .. }
                if program == "git"
                    && !args.iter().any(|a| {
                        [
                            "--abort",
                            "--continue",
                            "--quit",
                            "--skip",
                            "--mainline",
                            "-m",
                        ]
                        .contains(&a.as_str())
                    }) =>
            {
                let source = args.last().filter(|s| !s.starts_with('-'))?;
                let branch = self.repo.as_ref()?.branch.clone();
                match args.first()?.as_str() {
                    "merge" if args.len() == 2 || (args.len() == 3 && args[1] == "--no-edit") => {
                        Some(Kind::Merge {
                            source: source.clone(),
                            target: format!("refs/heads/{branch}"),
                        })
                    }
                    "cherry-pick" if args.len() == 2 => Some(Kind::CherryPick {
                        commit: source.clone(),
                        target: format!("refs/heads/{branch}"),
                    }),
                    "reset" if args.len() == 3 && args[1] == "--mixed" => Some(Kind::Reset {
                        commit: source.clone(),
                    }),
                    _ => None,
                }
            }
            _ => None,
        }
    }
    pub(super) fn accept_graph_review(&mut self, review: Preview) {
        self.begin_graph_action(review.kind.clone());
        self.manipulation.review = Some(review);
    }
    pub(super) fn begin_graph_action(&mut self, kind: Kind) {
        if let Kind::Commit { message }
        | Kind::Amend { message, .. }
        | Kind::Squash { message, .. }
        | Kind::SquashRange { message, .. } = &kind
        {
            self.manipulation.message = message.clone();
        }
        self.manipulation.cancel();
        self.manipulation.root = self.path();
        self.manipulation.draft = Some(kind);
        self.manipulation.released = true;
        self.manipulation.requested_at = Some(Instant::now());
    }
    fn drop_kind(&self, payload: &Payload, target: Target<'_>) -> Result<Kind> {
        let repo = self.repo.as_ref().ok_or("Open a repository first")?;
        if branchroom::safety::is_protected(&repo.root) {
            return Err("This repository is protected and cannot be changed".into());
        }
        if let Target::Branch(name) = &target {
            if name.starts_with("refs/remotes/") {
                return Err("Choose a local branch as the destination. Switch to the remote branch to create a local copy first".into());
            }
            if !repo.changes.is_empty() {
                return Err("Commit or shelve working changes before combining branches".into());
            }
        }
        let message = if self.commit_message.trim().is_empty() {
            self.manipulation.message.clone()
        } else {
            self.commit_text()?
        };
        match (payload, target) {
            (
                Payload::Branch {
                    name, reference, ..
                },
                Target::Branch(target),
            ) if name != target.strip_prefix("refs/heads/").unwrap_or(target) => Ok(Kind::Merge {
                source: reference.clone(),
                target: target.into(),
            }),
            (Payload::Commit(id), Target::Branch(target)) => Ok(Kind::CherryPick {
                commit: id.clone(),
                target: target.into(),
            }),
            (Payload::Branch { current: true, .. }, Target::Commit { id, .. }) => {
                Ok(Kind::Reset { commit: id.into() })
            }
            (Payload::Branch { current: false, .. }, Target::Commit { .. }) => {
                Err("Switch to this branch before moving its tip".into())
            }
            (Payload::Working, Target::Working) => Ok(Kind::Commit { message }),
            (Payload::Working, Target::Commit { id, .. }) => Ok(Kind::Amend {
                commit: id.into(),
                message: if message.is_empty() {
                    repo.commits
                        .iter()
                        .find(|c| c.id == id)
                        .map(|c| c.subject.clone())
                        .unwrap_or_default()
                } else {
                    message
                },
            }),
            (Payload::Commit(source), Target::Commit { id, edge }) if source != id => {
                if edge == 0 {
                    return Ok(Kind::Squash {
                        source: source.clone(),
                        target: id.into(),
                        message: repo
                            .commits
                            .iter()
                            .find(|c| c.id == id)
                            .map(|c| c.subject.clone())
                            .unwrap_or_default(),
                    });
                }
                let a = repo
                    .branch_commits
                    .iter()
                    .position(|c| c.id == *source)
                    .ok_or("Only reorder commits on the current branch")?;
                let b = repo
                    .branch_commits
                    .iter()
                    .position(|c| c.id == id)
                    .ok_or("Only reorder commits on the current branch")?;
                let range = &repo.branch_commits[..=a.max(b)];
                if range.iter().any(|c| c.parents.len() > 1) {
                    return Err("Choose a linear range without a merge commit".into());
                }
                let mut order: Vec<_> = range
                    .iter()
                    .rev()
                    .map(|c| c.id.clone())
                    .filter(|c| c != source)
                    .collect();
                let pos = order.iter().position(|c| c == id).unwrap();
                order.insert(pos + usize::from(edge < 0), source.clone());
                Ok(Kind::Reorder { order })
            }
            _ => Err("Choose a different branch or commit for this action".into()),
        }
    }
    pub(super) fn graph_drop(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        target: Target<'_>,
    ) {
        let Some(payload) = response.dnd_hover_payload::<Payload>() else {
            return;
        };
        let insertion_edge = match &target {
            Target::Commit { edge, .. } if matches!(&*payload, Payload::Commit(_)) => *edge,
            _ => 0,
        };
        let kind = self.drop_kind(&payload, target);
        let released = response.dnd_release_payload::<Payload>().is_some();
        match kind {
            Ok(kind) => {
                if self.manipulation.hover_invalid {
                    self.manipulation.error = None;
                    self.manipulation.hover_invalid = false;
                }
                if self.manipulation.draft.as_ref() != Some(&kind) {
                    self.begin_graph_action(kind);
                    self.manipulation.released = false;
                }
                let invalid = self.manipulation.error.is_some()
                    || self
                        .manipulation
                        .review
                        .as_ref()
                        .is_some_and(|p| p.blocked.is_some());
                if invalid {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::NotAllowed);
                }
                ui.painter().rect_stroke(
                    response.rect.shrink(2.),
                    4.,
                    egui::Stroke::new(2., if invalid { DANGER() } else { ACCENT() }),
                    egui::StrokeKind::Inside,
                );
                if insertion_edge != 0 && !invalid {
                    let y = if insertion_edge > 0 {
                        response.rect.top()
                    } else {
                        response.rect.bottom()
                    };
                    ui.painter().line_segment(
                        [
                            egui::pos2(response.rect.left(), y),
                            egui::pos2(response.rect.right(), y),
                        ],
                        egui::Stroke::new(3., ACCENT()),
                    );
                }
                if self.manipulation.review.is_some() {
                    let center = response.rect.right_bottom() - egui::vec2(12., 10.);
                    for i in 0..6 {
                        let angle = i as f32 * std::f32::consts::TAU / 6.;
                        let p = |v: f32| center + egui::vec2(v.cos(), v.sin()) * 5.;
                        ui.painter().line_segment(
                            [p(angle), p(angle + 0.6)],
                            egui::Stroke::new(1.5, ACCENT()),
                        );
                    }
                }
                if released {
                    self.manipulation.released = true;
                }
            }
            Err(error) => {
                self.manipulation.cancel();
                self.manipulation.root = self.path();
                self.manipulation.hover_invalid = true;
                self.manipulation.error = Some(error);
                ui.ctx().set_cursor_icon(egui::CursorIcon::NotAllowed);
                ui.painter().rect_stroke(
                    response.rect.shrink(2.),
                    4.,
                    egui::Stroke::new(2., DANGER()),
                    egui::StrokeKind::Inside,
                );
            }
        }
    }
    fn risk_key(&self, source: &str, target: &str) -> String {
        let id = |name: &str| {
            let remote = name.starts_with("refs/remotes/");
            let name = name
                .strip_prefix("refs/heads/")
                .or_else(|| name.strip_prefix("refs/remotes/"))
                .unwrap_or(name);
            self.repo
                .as_ref()
                .and_then(|r| {
                    r.branches
                        .iter()
                        .find(|b| b.name == name && b.remote == remote)
                })
                .map(|b| b.id.as_str())
                .unwrap_or(name)
                .to_owned()
        };
        format!("{}:{}:{}", self.path().display(), id(source), id(target))
    }
    pub(super) fn conflict_risk(&self, source: &str, target: &str) -> Option<usize> {
        self.manipulation
            .risks
            .get(&self.risk_key(source, target))
            .copied()
    }
    pub(super) fn graph_preview_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let dragging = egui::DragAndDrop::has_payload_of_type::<Payload>(ctx);
        if (self.manipulation.active() || self.manipulation.error.is_some())
            && self.manipulation.root != self.path()
        {
            self.manipulation.cancel();
        }
        if self.manipulation.was_dragging && !dragging && !self.manipulation.released {
            self.manipulation.cancel();
        }
        self.manipulation.was_dragging = dragging;
        if let Some(rx) = &self.manipulation.receiver
            && let Ok((generation, result)) = rx.try_recv()
        {
            self.manipulation.receiver = None;
            self.manipulation.cancellation = None;
            if generation == self.manipulation.generation {
                match result {
                    Ok(review) => {
                        if let Kind::Merge { source, target } = &review.kind {
                            self.manipulation
                                .risks
                                .insert(self.risk_key(source, target), review.conflicts.len());
                        }
                        self.manipulation.review = Some(review);
                    }
                    Err(error) => self.manipulation.error = Some(error),
                }
            }
        }
        if self.manipulation.draft.is_some()
            && self.manipulation.review.is_none()
            && self.manipulation.receiver.is_none()
            && self.manipulation.error.is_none()
            && self
                .manipulation
                .requested_at
                .is_some_and(|t| t.elapsed() > Duration::from_millis(350))
        {
            let kind = self.manipulation.draft.clone().unwrap();
            let needs_message = empty_message(&kind);
            if !needs_message {
                if self.preview_mode {
                    self.manipulation.error = Some(
                        "Forecasts require a live repository. UI fixtures do not execute Git"
                            .into(),
                    );
                } else {
                    let (root, ctx, generation) =
                        (self.path(), ctx.clone(), self.manipulation.generation);
                    let (tx, rx) = mpsc::channel();
                    self.manipulation.receiver = Some(rx);
                    let token = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                    self.manipulation.cancellation = Some(token.clone());
                    std::thread::spawn(move || {
                        process::set_cancellation(token);
                        let result = branchroom::operation::preview(&root, kind);
                        process::reset_cancellation();
                        let _ = tx.send((generation, result));
                        ctx.request_repaint();
                    });
                }
            }
        }
        if !self.manipulation.active() && !dragging && self.manipulation.error.is_none() {
            self.graph_advanced(ctx);
            return;
        }
        egui::Panel::bottom(if dragging {
            "graph_drag_preview"
        } else {
            "graph_action_preview"
        })
        .resizable(!dragging)
        .default_size(if dragging { 150. } else { 280. })
        .size_range(if dragging {
            130.0..=180.0
        } else {
            130.0..=430.0
        })
        .frame(egui::Frame::new().fill(SURFACE()).inner_margin(12))
        .show(ui, |ui| {
            let mut confirmed = false;
            let mut cancel = false;
            let mut move_to = None;
            egui::Panel::bottom("graph_review_controls")
                .exact_size(if ui.available_width() < 450. {
                    80.
                } else {
                    44.
                })
                .frame(egui::Frame::new().fill(SURFACE()))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        let ready = self
                            .manipulation
                            .review
                            .as_ref()
                            .is_some_and(|p| p.blocked.is_none())
                            && !dragging
                            && self.receiver.is_none()
                            && self.manipulation.error.is_none()
                    && !self.manipulation.advanced;
                        confirmed = ui
                            .add_enabled(ready, egui::Button::new("Confirm action").fill(PRIMARY()))
                            .clicked();
                        cancel = ui.button("Cancel preview").clicked();
                        if ui.button("Advanced form").clicked() {
                            self.open_graph_form();
                        }
                    });
                });
            egui::ScrollArea::vertical()
                .id_salt("graph_action_body")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(
                                self.manipulation
                                    .draft
                                    .as_ref()
                                    .map(title)
                                    .unwrap_or("Drag onto a branch or commit"),
                            )
                            .strong(),
                        );
                        if dragging {
                            subtitle(ui, "Release to keep this preview");
                        }
                        if self.manipulation.receiver.is_some() {
                            ui.spinner();
                        }
                    });
                    let mut changed = false;
                    if let Some(
                        Kind::Commit { message }
                        | Kind::Amend { message, .. }
                        | Kind::Squash { message, .. }
                        | Kind::SquashRange { message, .. },
                    ) = &mut self.manipulation.draft
                    {
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Commit message");
                            changed = ui
                                .add(
                                    egui::TextEdit::multiline(message)
                                        .desired_rows(2)
                                        .hint_text("Describe the change")
                                        .desired_width(ui.available_width().min(500.)),
                                )
                                .changed();
                        });
                    }
                    if changed {
                        self.manipulation.review = None;
                        self.manipulation.error = None;
                        self.manipulation.generation += 1;
                        if let Some(token) = self.manipulation.cancellation.take() {
                            token.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                        self.manipulation.receiver = None;
                        self.manipulation.requested_at = Some(Instant::now());
                    }
                    ui.scope(|ui| {
                    if let Some(error) = &self.manipulation.error {
                        ui.colored_label(DANGER(), error);
                        subtitle(ui, "Adjust the action, or cancel and use Advanced");
                    }
                    if let Some(review) = &self.manipulation.review {
                        ui.add(egui::Label::new(&review.summary).wrap());
                        ui.horizontal_wrapped(|ui| {
                            badge(
                                ui,
                                &format!("{} predicted {}", review.conflicts.len(),if review.conflicts.len()==1{"conflict"}else{"conflicts"}),
                                if review.conflicts.is_empty() {
                                    SUCCESS()
                                } else {
                                    DANGER()
                                },
                            );
                            ui.label(format!("{} commits introduced", review.introduced));
                            subtitle(ui, "A safety point is saved before applying");
                        });
                        for file in &review.conflicts {
                            ui.colored_label(DANGER(), file);
                        }
                        if let Some(reason) = &review.blocked {
                            ui.colored_label(DANGER(), reason);
                        }
                        if matches!(review.kind, Kind::Commit { .. } | Kind::Amend { .. })
                            && review.default_branch.as_deref() == Some(review.current_branch())
                        {
                            ui.colored_label(
                                WARNING(),
                                format!(
                                    "You are committing directly to {}",
                                    review.current_branch()
                                ),
                            );
                            ui.horizontal_wrapped(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.manipulation.new_branch)
                                        .hint_text("New branch name")
                                        .desired_width(180.),
                                );
                                if ui
                                    .add_enabled(
                                        !self.manipulation.new_branch.trim().is_empty(),
                                        egui::Button::new("Move these commits to a new branch"),
                                    )
                                    .clicked()
                                {
                                    move_to = Some(self.manipulation.new_branch.clone());
                                }
                            });
                        }
                        if !dragging && ui.available_width() >= 600. {
                            ui.columns(2, |columns| {
                                ghost(&mut columns[0], "Before", &review.before, MUTED(), false,"No commits yet");
                                ghost(
                                    &mut columns[1],
                                    "After confirmation",
                                    &review.after,
                                    ACCENT(),
                                    true,if review.conflicts.is_empty(){"No commits remain; files stay as working changes"}else{"Resolve the listed conflicts to finish the result"},
                                );
                            });
                        } else if !dragging {
                            ghost(ui, "After confirmation", &review.after, ACCENT(), true,if review.conflicts.is_empty(){"No commits remain; files stay as working changes"}else{"Resolve the listed conflicts to finish the result"});
                            egui::CollapsingHeader::new("Before this action").show(ui, |ui| {
                                ghost(ui, "Current graph", &review.before, MUTED(), false,"No commits yet")
                            });
                        }
                        egui::CollapsingHeader::new("Show Git command").show(ui, |ui| {
                            ui.add(
                                egui::Label::new(RichText::new(&review.command).monospace()).wrap(),
                            );
                        });
                    } else if self.manipulation.error.is_none() {
                        subtitle(
                            ui,
                            if self.manipulation.draft.is_none(){"Hover over a branch or commit to preview this drop"}else if self.manipulation.draft.as_ref().is_some_and(empty_message){"Enter a commit message to prepare the preview"}else{"Preparing the forecast. No checkout files are changed by this preview"},
                        );
                    }
                });
                });
            if let Some(name) = move_to {
                self.begin_graph_action(Kind::MoveToBranch { name });
            } else if confirmed {
                self.confirm_graph_action(ctx);
            } else if cancel {
                self.manipulation.cancel();
                egui::DragAndDrop::clear_payload(ctx);
            }
        });
        self.graph_advanced(ctx);
    }
    pub(super) fn open_graph_form(&mut self) {
        match self.manipulation.draft.clone() {
            Some(Kind::Merge { source, target }) => {
                self.manipulation.mode = 0;
                self.manipulation.source = source;
                self.manipulation.target = target;
            }
            Some(Kind::CherryPick { commit, target }) => {
                self.manipulation.mode = 1;
                self.manipulation.source = commit;
                self.manipulation.target = target;
            }
            Some(Kind::Reorder { order }) => {
                self.manipulation.mode = 2;
                self.manipulation.order = order.join("\n");
            }
            Some(Kind::Squash {
                source,
                target,
                message,
            }) => {
                self.manipulation.mode = 3;
                self.manipulation.source = source;
                self.manipulation.target = target;
                self.manipulation.message = message;
            }
            Some(Kind::Reset { commit }) => {
                self.manipulation.mode = 4;
                self.manipulation.target = commit;
            }
            Some(Kind::Commit { message }) => {
                self.manipulation.mode = 5;
                self.manipulation.message = message;
            }
            Some(Kind::Amend { commit, message }) => {
                self.manipulation.mode = 6;
                self.manipulation.target = commit;
                self.manipulation.message = message;
            }
            Some(Kind::Undo) => self.manipulation.mode = 7,
            Some(Kind::SquashRange { count, message }) => {
                self.manipulation.mode = 8;
                self.history_steps = count;
                self.manipulation.message = message;
            }
            Some(Kind::MoveToBranch { name }) => {
                self.manipulation.mode = 9;
                self.manipulation.new_branch = name;
            }
            None => {}
        }
        self.manipulation.advanced = true;
    }
    fn raw_suggestion(&self) -> Vec<String> {
        let kind = self.manipulation.draft.as_ref();
        let branch = self.repo.as_ref().map(|r| r.branch.as_str()).unwrap_or("");
        match kind {
            Some(Kind::Merge { source, target }) => {
                let target = target.strip_prefix("refs/heads/").unwrap_or(target);
                if target != branch {
                    process::strings(&["switch", target])
                } else {
                    process::strings(&["merge", "--no-edit", source])
                }
            }
            Some(Kind::CherryPick { commit, target }) => {
                let target = target.strip_prefix("refs/heads/").unwrap_or(target);
                if target != branch {
                    process::strings(&["switch", target])
                } else {
                    process::strings(&["cherry-pick", commit])
                }
            }
            Some(Kind::Reset { commit }) => process::strings(&["reset", "--mixed", commit]),
            Some(Kind::Undo) => process::strings(&["reset", "--mixed", "HEAD^"]),
            Some(Kind::Commit { message }) => process::strings(&["commit", "-m", message]),
            Some(Kind::Amend { commit, message }) => {
                if self.repo.as_ref().is_some_and(|r| r.head == *commit) {
                    process::strings(&["commit", "--amend", "-m", message])
                } else {
                    process::strings(&["commit", "--fixup", commit])
                }
            }
            Some(Kind::MoveToBranch { name }) => process::strings(&["switch", "-c", name]),
            Some(Kind::Squash { target, .. }) => {
                process::strings(&["rebase", "--interactive", &format!("{target}^")])
            }
            Some(Kind::SquashRange { count, .. }) => {
                process::strings(&["rebase", "--interactive", &format!("HEAD~{count}")])
            }
            Some(Kind::Reorder { order }) => {
                process::strings(&["rebase", "--interactive", &format!("HEAD~{}", order.len())])
            }
            None => process::strings(&["status", "--short"]),
        }
    }
    fn graph_advanced(&mut self, ctx: &egui::Context) {
        if !self.manipulation.advanced {
            return;
        }
        const LABELS: [&str; 10] = [
            "Merge",
            "Cherry-pick",
            "Reorder",
            "Squash two commits",
            "Move branch tip",
            "Commit",
            "Amend or fixup",
            "Undo latest commit",
            "Squash recent range",
            "Move commits to a new branch",
        ];
        let mut open = true;
        let mut action = None;
        let mut raw = None;
        egui::Window::new("Advanced graph action").open(&mut open).default_width(470.).max_height((ctx.content_rect().height()-60.).max(180.)).vscroll(true).show(ctx,|ui|{
            let previous=self.manipulation.mode;
            egui::ComboBox::from_label("Action").selected_text(LABELS[self.manipulation.mode]).show_ui(ui,|ui|{for(index,label)in LABELS.iter().enumerate(){ui.selectable_value(&mut self.manipulation.mode,index,*label);}});
            if previous!=self.manipulation.mode {self.manipulation.target=if matches!(self.manipulation.mode,3|4|6){self.selected_commit.clone()}else{self.repo.as_ref().map(|r|format!("refs/heads/{}",r.branch)).unwrap_or_default()};}
            if self.manipulation.target.is_empty(){self.manipulation.target=self.repo.as_ref().map(|r|format!("refs/heads/{}",r.branch)).unwrap_or_default();}
            if self.manipulation.source.is_empty()&&self.manipulation.mode==0{self.manipulation.source=self.repo.as_ref().map(|r|r.base.clone()).unwrap_or_default();}
            if matches!(self.manipulation.mode,0|1|3){
                ui.label("Source branch or commit");ui.add(egui::TextEdit::singleline(&mut self.manipulation.source).hint_text("Branch name or commit ID").desired_width(f32::INFINITY));
                if matches!(self.manipulation.mode,1|3)&&ui.button("Use selected commit as source").clicked(){self.manipulation.source=self.selected_commit.clone();}
            }
            if matches!(self.manipulation.mode,0|1|3|4|6){
                ui.label("Destination branch or commit");ui.add(egui::TextEdit::singleline(&mut self.manipulation.target).hint_text("Branch name or commit ID").desired_width(f32::INFINITY));
                if matches!(self.manipulation.mode,3|4|6)&&ui.button("Use selected commit as destination").clicked(){self.manipulation.target=self.selected_commit.clone();}
            }
            if matches!(self.manipulation.mode,2|8){ui.horizontal_wrapped(|ui|{ui.label("Recent commits");ui.add(egui::DragValue::new(&mut self.history_steps).range(1..=200));});}
            if self.manipulation.mode==2 {
                if ui.button("Load recent local commits").clicked(){self.manipulation.order=self.repo.as_ref().map(|r|r.branch_commits.iter().take(self.history_steps).rev().map(|c|c.id.as_str()).collect::<Vec<_>>().join("\n")).unwrap_or_default();}
                ui.label("Commit IDs, oldest first, one per line");ui.add(egui::TextEdit::multiline(&mut self.manipulation.order).desired_rows(5).desired_width(f32::INFINITY).hint_text("Commit IDs in the desired order"));
            }
            if matches!(self.manipulation.mode,3|5|6|8){ui.label("Commit message");ui.add(egui::TextEdit::multiline(&mut self.manipulation.message).desired_rows(3).desired_width(f32::INFINITY).hint_text("Describe the change"));}
            if self.manipulation.mode==9{ui.label("New branch name");ui.add(egui::TextEdit::singleline(&mut self.manipulation.new_branch).hint_text("feature/my-change").desired_width(f32::INFINITY));}
            subtitle(ui,"These controls use the same forecast and safety checks as dragging");
            if ui.button("Preview action").clicked(){let m=&self.manipulation;action=Some(match m.mode{
                0=>Kind::Merge{source:m.source.clone(),target:m.target.clone()},1=>Kind::CherryPick{commit:m.source.clone(),target:m.target.clone()},2=>Kind::Reorder{order:m.order.lines().map(str::trim).filter(|s|!s.is_empty()).map(str::to_owned).collect()},3=>Kind::Squash{source:m.source.clone(),target:m.target.clone(),message:m.message.clone()},4=>Kind::Reset{commit:m.target.clone()},5=>Kind::Commit{message:m.message.clone()},6=>Kind::Amend{commit:m.target.clone(),message:m.message.clone()},7=>Kind::Undo,8=>Kind::SquashRange{count:self.history_steps,message:m.message.clone()},_=>Kind::MoveToBranch{name:m.new_branch.clone()}});}
            egui::CollapsingHeader::new("Exact Git command fallback").show(ui,|ui|{subtitle(ui,"For custom tools or flags, review the exact command in Command center. Writes require typed confirmation and retain available safety points");if ui.button("Open exact Git command form").clicked(){raw=Some(self.raw_suggestion());}});
        });
        self.manipulation.advanced = open;
        if let Some(kind) = action {
            self.manipulation.advanced = false;
            self.begin_graph_action(kind);
        } else if let Some(args) = raw {
            self.cancel_graph_action(ctx);
            self.page = Page::Tools;
            self.command_program = "git".into();
            self.command_json = serde_json::to_string(&args).unwrap();
        }
    }
}
