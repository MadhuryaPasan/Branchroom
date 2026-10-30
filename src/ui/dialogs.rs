use super::*;

impl Branchroom {
    pub(super) fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(kind) = self
            .pending
            .as_ref()
            .and_then(|p| self.reviewed_graph_kind(&p.task))
        {
            self.pending = None;
            self.confirmation_input.clear();
            self.begin_graph_action(kind);
        }
        if let Some(pending) = self.pending.clone() {
            let response =
                egui::Modal::new(egui::Id::unique("review_operation_modal")).show(ctx, |ui| {
                    ui.set_width(520.0_f32.min(ctx.content_rect().width() - 48.));
                    ui.heading(&pending.title);
                    ui.add_space(12.);
                    egui::ScrollArea::vertical()
                        .id_salt("operation_consequences")
                        .max_height((ctx.content_rect().height() * 0.35).max(80.))
                        .show(ui, |ui| {
                            ui.add(egui::Label::new(&pending.explanation).wrap());
                        });
                    if let Task::Commit(plan) = &pending.task {
                        egui::CollapsingHeader::new(format!(
                            "Included files ({})",
                            plan.files.len()
                        ))
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 0.;
                            egui::ScrollArea::vertical()
                                .id_salt("commit_review_files")
                                .max_height(110.)
                                .show_rows(ui, 20., plan.files.len(), |ui, range| {
                                    for index in range {
                                        ui.add(egui::Label::new(&plan.files[index]).truncate())
                                            .on_hover_text(&plan.files[index]);
                                    }
                                });
                        });
                    }
                    ui.add_space(16.);
                    section(ui, "REPOSITORY");
                    ui.add(
                        egui::Label::new(self.path().display().to_string())
                            .wrap()
                            .selectable(true),
                    );
                    ui.add_space(12.);
                    if let Task::Command { program, args, .. } = &pending.task {
                        egui::CollapsingHeader::new("Command details").show(ui, |ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(format!(
                                        "{program} {}",
                                        args.iter()
                                            .map(|s| format!("{s:?}"))
                                            .collect::<Vec<_>>()
                                            .join(" ")
                                    ))
                                    .monospace(),
                                )
                                .wrap()
                                .selectable(true),
                            );
                        });
                    }
                    if let Task::RawGit(args) = &pending.task {
                        ui.label("This exact Advanced command is not changed by the graph planner");
                        egui::CollapsingHeader::new("Show Git command").show(ui, |ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(branchroom::operation::display_command(args))
                                        .monospace(),
                                )
                                .wrap()
                                .selectable(true),
                            );
                        });
                    }
                    let destructive = requires_typed_confirmation(&pending);
                    if destructive {
                        ui.label("Type CONFIRM to continue.");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.confirmation_input)
                                .hint_text("CONFIRM"),
                        );
                    }
                    ui.add_space(20.);
                    ui.horizontal(|ui| {
                        let label = match &pending.task {
                            Task::UpdateBranch { .. } => "Update branch",
                            Task::Revert { .. } => "Create revert commit",
                            _ => pending.title.as_str(),
                        };
                        let response = ui.add_enabled(
                            self.receiver.is_none()
                                && (!destructive || self.confirmation_input == "CONFIRM"),
                            egui::Button::new(
                                RichText::new(label)
                                    .color(if destructive { SIDEBAR() } else { INK() })
                                    .strong(),
                            )
                            .min_size(egui::vec2(100., 32.))
                            .fill(if destructive {
                                DANGER()
                            } else {
                                PRIMARY()
                            }),
                        );
                        if response.clicked() {
                            self.pending = None;
                            self.confirmation_input.clear();
                            self.execute(pending.task.clone(), ctx);
                        }
                        if ui
                            .add(egui::Button::new("Cancel").min_size(egui::vec2(80., 32.)))
                            .clicked()
                        {
                            self.pending = None;
                            self.confirmation_input.clear();
                        }
                    });
                });
            if response.should_close() {
                self.pending = None;
                self.confirmation_input.clear();
            }
        }
        if let Some(mut form) = self.form.take() {
            let mut keep = true;
            let response =
                egui::Modal::new(egui::Id::unique("operation_form_modal")).show(ctx, |ui| {
                    ui.set_width(500.0_f32.min((ctx.content_rect().width() - 48.).max(200.)));
                    ui.heading(&form.title);
                    ui.add_space(12.);
                    ui.label(&form.description);
                    ui.add_space(16.);
                    for field in &mut form.fields {
                        ui.label(&field.label);
                        ui.add(
                            egui::TextEdit::singleline(&mut field.value)
                                .hint_text(&field.label)
                                .desired_width(f32::INFINITY),
                        );
                        ui.add_space(8.);
                    }
                    if !form.error.is_empty() {
                        ui.label(RichText::new(&form.error).color(DANGER()));
                    }
                    ui.add_space(12.);
                    ui.horizontal_wrapped(|ui| {
                        if primary(ui, "Review operation").clicked() {
                            if form
                                .fields
                                .iter()
                                .any(|f| f.value.trim().is_empty() || f.value.starts_with('-'))
                            {
                                form.error =
                                    "Fill in every field. Values cannot start with a hyphen."
                                        .into();
                                return;
                            }
                            let mut args = form.prefix.clone();
                            args.extend(form.fields.iter().map(|f| f.value.clone()));
                            args.extend(form.suffix.clone());
                            self.pending = Some(Pending {
                                title: form.title.clone(),
                                explanation: form.description.clone(),
                                task: Task::Command {
                                    program: form.program.clone(),
                                    args,
                                    refresh: true,
                                },
                            });
                            keep = false;
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            if response.should_close() {
                keep = false;
            }
            if keep {
                self.form = Some(form);
            }
        }
    }
}

// Gate operations that can remove or rewrite data. Existing recovery features remain available,
// but are not assumed to cover every case (especially remote and workspace changes).
fn requires_typed_confirmation(pending: &Pending) -> bool {
    let title = pending.title.to_lowercase();
    if title == "run command" {
        return true;
    }
    if [
        "delete",
        "discard",
        "drop",
        "abort",
        "remove",
        "reset",
        "squash",
        "reorder",
        "force push",
    ]
    .iter()
    .any(|word| title.contains(word))
    {
        return true;
    }
    match &pending.task {
        Task::RawGit(args) => !branchroom::safety::read_only_git(args),
        Task::History(_)
        | Task::Discard(_)
        | Task::RestoreDiscard(_)
        | Task::RemoveSnapshot(_)
        | Task::Merge { .. }
        | Task::UpdateBranch { .. } => true,
        Task::Command { program, args, .. } if program == "git" => {
            args.iter().any(|arg| {
                matches!(
                    arg.as_str(),
                    "reset"
                        | "--amend"
                        | "rebase"
                        | "clean"
                        | "rm"
                        | "--force"
                        | "-D"
                        | "--hard"
                        | "--abort"
                        | "--force-with-lease"
                ) || arg.starts_with("--force-with-lease=")
            }) || (args
                .first()
                .is_some_and(|a| ["merge", "cherry-pick"].contains(&a.as_str()))
                && !args.iter().any(|a| a == "--continue"))
                || (args.first().is_some_and(|arg| arg == "stash")
                    && args
                        .iter()
                        .any(|arg| matches!(arg.as_str(), "drop" | "clear" | "pop")))
                || (args.first().is_some_and(|arg| arg == "worktree")
                    && args.iter().any(|arg| arg == "remove"))
        }
        _ => false,
    }
}

#[cfg(test)]
mod safety_tests {
    use super::*;
    #[test]
    fn gates_rewrites_and_force_push_but_not_fetch() {
        let pending = |task| Pending {
            title: "Review".into(),
            explanation: String::new(),
            task,
        };
        assert!(requires_typed_confirmation(&pending(Task::History(
            branchroom::history::Action::Undo
        ))));
        assert!(requires_typed_confirmation(&pending(Task::Command {
            program: "git".into(),
            args: vec![
                "push".into(),
                "--force-with-lease=refs/heads/main:abc".into()
            ],
            refresh: true
        })));
        assert!(!requires_typed_confirmation(&pending(Task::Command {
            program: "git".into(),
            args: vec!["fetch".into()],
            refresh: true
        })));
    }
}
