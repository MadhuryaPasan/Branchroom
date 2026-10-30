use super::*;
use branchroom::diff::{LineKind, Patch};
use std::collections::BTreeMap;

struct SplitCell {
    position: (usize, Option<usize>),
    old: bool,
    rect: egui::Rect,
    offset: f32,
    selectable: bool,
}

impl Branchroom {
    fn commit_lock_for(&self, path: &str) -> Option<String> {
        let repo = self.repo.as_ref()?;
        let original = repo
            .changes
            .iter()
            .find(|c| c.path == path)
            .and_then(|c| c.original.as_ref());
        repo.commit_locks
            .iter()
            .find(|p| {
                branchroom::commit_lock::matches(p, path)
                    || original.is_some_and(|old| branchroom::commit_lock::matches(p, old))
            })
            .cloned()
    }
    pub(super) fn run_git(&mut self, args: &[&str], ctx: &egui::Context) {
        self.execute(
            Task::Command {
                program: "git".into(),
                args: process::strings(args),
                refresh: true,
            },
            ctx,
        );
    }
    pub(super) fn select_file(&mut self, path: &str) {
        self.selected_path = path.into();
        self.compact_changes_tab = 1;
        self.image_data = None;
        self.image_view = None;
        self.file_patch = None;
        self.diff_refresh = true;
        self.selected_hunks.clear();
        self.selected_lines.clear();
    }
    pub(super) fn commit_text(&self) -> Result<String> {
        if self.commit_message.trim().is_empty() {
            return Err("Enter a commit summary before committing".into());
        }
        let mut message = self.commit_message.trim().to_owned();
        if !self.commit_description.trim().is_empty() {
            message.push_str("\n\n");
            message.push_str(self.commit_description.trim());
        }
        if !self.coauthor.trim().is_empty() {
            for author in self.coauthor.lines().filter(|s| !s.trim().is_empty()) {
                let author = author.trim().trim_start_matches("Co-authored-by: ");
                if !author.contains('<') || !author.contains('@') || !author.ends_with('>') {
                    return Err("Enter each co-author as Name <email@example.com>.".into());
                }
                message.push_str("\n\nCo-authored-by: ");
                message.push_str(author);
            }
        }
        Ok(message)
    }
    pub(super) fn queue_commit(&mut self, ctx: &egui::Context) {
        match self.commit_text() {
            Ok(message) => self.execute(
                Task::PrepareCommit {
                    message,
                    amend: self.amend,
                },
                ctx,
            ),
            Err(error) => self.report(Err(error)),
        }
    }

    pub(super) fn changes(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let repo = self.repo.as_ref().unwrap().clone();
        let writable = !branchroom::safety::is_protected(&repo.root) && self.receiver.is_none();
        let compact = ui.available_width() < 760.;
        egui::Panel::top("changes_header").frame(egui::Frame::new().fill(PAPER()).inner_margin(if compact{8}else{16})).show(ui,|ui|{
            ui.horizontal_wrapped(|ui|{ui.label(RichText::new("Changes").size(TEXT_HEADER).strong());badge(ui,&format!("{} {}",repo.changes.len(),if repo.changes.len()==1{"file"}else{"files"}),WARNING());if ui.available_width()>700.{badge(ui,&repo.branch,ACCENT());}
                ui.menu_button("Actions",|ui|{
                if ui.button("Stash changes…").clicked(){self.form("Save stash","Save tracked and non-ignored untracked files, then clean the working tree.","git",&["stash","push","--include-untracked","-m"],&[("Stash message","")],&[]);}
                if ui.button("Recover discarded files").clicked(){self.page=Page::Compare;}
                ui.menu_button(format!("Commit locks ({})",repo.commit_locks.len()),|ui|{
                    ui.set_width(340.);
                    subtitle(ui,"Locked files stay editable and visible, but are excluded from Branchroom commits. Other Git clients and snapshot captures are independent.");
                    if ui.add_enabled(writable,egui::Button::new("Choose a file to exclude…")).clicked()&&let Some(file)=rfd::FileDialog::new().set_directory(&repo.root).pick_file(){
                        match file.strip_prefix(&repo.root){Ok(relative)=>{let path=relative.to_string_lossy().replace('\\',"/");self.execute(Task::SetCommitLock{path,locked:true},ctx);},Err(_)=>self.report(Err("Choose a file inside the current repository.".into()))}ui.close();
                    }
                    ui.separator();
                    egui::ScrollArea::vertical().max_height(240.).show(ui,|ui|{
                        for path in &repo.commit_locks {
                            if ui.add_enabled(writable,egui::Button::new(format!("Unlock {path}")).truncate()).on_hover_text(path).clicked(){self.execute(Task::SetCommitLock{path:path.clone(),locked:false},ctx);ui.close();}
                        }
                        if repo.commit_locks.is_empty(){subtitle(ui,"No commit locks in this project.");}
                    });
                });
                });
                if compact && !repo.changes.is_empty() {
                    ui.selectable_value(&mut self.compact_changes_tab,0,"Files");
                    ui.selectable_value(&mut self.compact_changes_tab,1,"Diff");
                    ui.selectable_value(&mut self.compact_changes_tab,2,"Commit");
                }
            });
            if !compact {subtitle(ui,"Choose files, hunks, or individual lines to include in your commit.");}

        });
        if repo.changes.is_empty() {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(PAPER()).inner_margin(16))
                .show(ui, |ui| {
                    ui.label(RichText::new("Working tree is clean").size(TEXT_HEADER).strong());
                    subtitle(
                        ui,
                        "There are no file changes to stage or commit. Keep moving with your branch.",
                    );
                    ui.horizontal_wrapped(|ui| {
                        if primary(ui, "Create pull request").clicked() {
                            self.pr_head = repo.branch.clone();
                            self.pr_tab = 3;
                            self.page = Page::Github;
                        }
                        if ui.button("Open pull requests").clicked() {
                            self.pr_tab = 0;
                            self.page = Page::Github;
                        }
                        if ui.button("View commit graph").clicked() {
                            self.page = Page::History;
                        }
                    });
                    ui.add_space(16.);
                    surface().show(ui, |ui| {
                        ui.label(RichText::new(format!("Recent commits on {}", repo.branch)).strong());
                        if repo.branch_commits.is_empty() {
                            subtitle(ui, "No commits on this branch yet.");
                        }
                        for commit in repo.branch_commits.iter().take(3) {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("●").color(ACCENT()));
                                ui.label(&commit.subject);
                            });
                        }
                    });
                });
            return;
        }
        if compact {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(PAPER()).inner_margin(16))
                .show(ui, |ui| match self.compact_changes_tab {
                    0 => self.change_list(ui, ctx, &repo, writable),
                    1 => self.patch_view(ui, ctx, writable),
                    _ => {
                        egui::Panel::bottom("compact_commit_footer")
                            .frame(egui::Frame::new().fill(PAPER()))
                            .show(ui, |ui| self.commit_footer(ui, writable));
                        egui::ScrollArea::vertical()
                            .id_salt("compact_commit")
                            .show(ui, |ui| self.commit_composer(ui));
                    }
                });
            return;
        }
        egui::Panel::left("changes_files")
            .resizable(true)
            .default_size(370.)
            .size_range(310.0..=520.0)
            .frame(egui::Frame::new().fill(SURFACE()).inner_margin(14))
            .show(ui, |ui| {
                egui::Panel::bottom("commit_form")
                    .resizable(true)
                    .default_size(285.)
                    .size_range(230.0..=440.0)
                    .frame(egui::Frame::new().fill(SURFACE()))
                    .show(ui, |ui| {
                        egui::Panel::bottom("commit_footer")
                            .exact_size(66.)
                            .frame(egui::Frame::new().fill(SURFACE()))
                            .show(ui, |ui| {
                                self.commit_footer(ui, writable);
                            });
                        egui::ScrollArea::vertical()
                            .id_salt("commit_form_scroll")
                            .show(ui, |ui| {
                                self.commit_composer(ui);
                            });
                    });
                self.change_list(ui, ctx, &repo, writable);
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(PAPER()).inner_margin(16))
            .show(ui, |ui| self.patch_view(ui, ctx, writable));
    }

    fn commit_composer(&mut self, ui: &mut egui::Ui) {
        section(ui, "COMMIT");
        ui.add_space(4.);
        let count = self.commit_message.chars().count();
        ui.horizontal(|ui| {
            ui.label(RichText::new("Summary (required)").strong());
            if count > 0 {
                let color = if count > 72 { WARNING() } else { MUTED() };
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{count}/72"))
                            .color(color)
                            .size(TEXT_CAPTION),
                    );
                });
            }
        });
        ui.add(
            egui::TextEdit::singleline(&mut self.commit_message)
                .id(egui::Id::unique("commit_summary"))
                .desired_width(f32::INFINITY)
                .hint_text("Describe the change"),
        );
        ui.add_space(4.);
        ui.label(RichText::new("Description").strong());
        ui.add(
            egui::TextEdit::multiline(&mut self.commit_description)
                .desired_rows(3)
                .desired_width(f32::INFINITY)
                .hint_text("Why is this change needed? (optional)"),
        );
        ui.add_space(4.);
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.amend, "Amend latest commit");
            egui::CollapsingHeader::new("Co-authors").show(ui, |ui| {
                ui.label("One author per line (Name <email@example.com>)");
                ui.add(
                    egui::TextEdit::multiline(&mut self.coauthor)
                        .desired_rows(2)
                        .hint_text("Name <email@example.com>"),
                );
            });
        });
    }
    fn commit_footer(&mut self, ui: &mut egui::Ui, writable: bool) {
        ui.add_space(8.);
        let branch = self
            .repo
            .as_ref()
            .map(|r| r.branch.as_str())
            .unwrap_or("branch");
        let label = format!("Commit to {branch}");
        let visible_label = format!("Commit to {}", shorten_label(branch, 24));
        let response = ui
            .add_enabled_ui(writable, |ui| {
                ui.add_sized(
                    [ui.available_width(), 34.],
                    egui::Button::new(RichText::new(visible_label).color(INK()).strong())
                        .fill(PRIMARY())
                        .stroke(egui::Stroke::NONE),
                )
            })
            .inner;
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, writable, &label));
        if response.on_hover_text(&label).clicked() {
            self.queue_commit(&ui.ctx().clone());
        }
        let count = self
            .repo
            .as_ref()
            .map(|r| {
                r.changes
                    .iter()
                    .filter(|c| c.index != ' ' && c.index != '?')
                    .count()
            })
            .unwrap_or(0);
        subtitle(
            ui,
            &format!(
                "{count} staged {} · Ctrl+Enter to commit",
                if count == 1 { "file" } else { "files" }
            ),
        );
    }

    fn change_list(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        repo: &RepoState,
        writable: bool,
    ) {
        ui.horizontal(|ui| {
            section(ui, "CHANGED FILES");
            ui.selectable_value(&mut self.files_tree, true, "Tree");
            ui.selectable_value(&mut self.files_tree, false, "List");
        });
        ui.add(
            egui::TextEdit::singleline(&mut self.file_filter)
                .desired_width(f32::INFINITY)
                .hint_text("Filter files"),
        );
        ui.horizontal_wrapped(|ui| {
            let eligible: Vec<_> = repo
                .changes
                .iter()
                .filter(|c| self.commit_lock_for(&c.path).is_none())
                .collect();
            let mut all = !eligible.is_empty()
                && eligible
                    .iter()
                    .all(|c| c.index != ' ' && c.index != '?' && c.worktree == ' ');
            let partial = !all && eligible.iter().any(|c| c.index != ' ' && c.index != '?');
            if ui
                .add_enabled(
                    writable
                        && !eligible.is_empty()
                        && !repo.changes.iter().any(git::Change::conflict),
                    egui::Checkbox::new(
                        &mut all,
                        if repo.commit_locks.is_empty() {
                            "Include all files"
                        } else {
                            "Include unlocked files"
                        },
                    )
                    .indeterminate(partial),
                )
                .on_disabled_hover_text("Wait for the current operation, resolve conflicts, or unlock files before including changes.")
                .changed()
            {
                if all {
                    self.execute(Task::StageUnlocked, ctx);
                } else {
                    self.run_git(
                        if repo.head.is_empty() {
                            &["read-tree", "--empty"]
                        } else {
                            &["reset"]
                        },
                        ctx,
                    );
                }
            }
            if ui.button("View staged").clicked() {
                self.diff_staged = true;
                self.diff_refresh = true;
                if !self.selected_path.is_empty() { self.compact_changes_tab = 1; }
            }
        });
        ui.add_space(8.);
        let files: Vec<_> = repo
            .changes
            .iter()
            .filter(|c| {
                self.file_filter.is_empty()
                    || c.path
                        .to_lowercase()
                        .contains(&self.file_filter.to_lowercase())
            })
            .collect();
        if files.is_empty() {
            subtitle(
                ui,
                if repo.changes.is_empty() {
                    "No changed files"
                } else {
                    "No files match this filter"
                },
            );
            return;
        }
        if self.files_tree {
            egui::ScrollArea::vertical()
                .id_salt("changed_files_tree")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().interact_size.y = 24.;
                    ui.spacing_mut().item_spacing.y = 3.;
                    self.change_tree(ui, ctx, &files, "", writable);
                });
        } else {
            ui.spacing_mut().item_spacing.y = 0.;
            egui::ScrollArea::vertical()
                .id_salt("changed_files_list")
                .auto_shrink([false, false])
                .show_rows(ui, 32., files.len(), |ui, range| {
                    ui.spacing_mut().item_spacing.y = 0.;
                    for index in range {
                        let change = files[index];
                        self.change_row(ui, ctx, change, &change.path, writable);
                    }
                });
        }
    }

    fn change_tree(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        files: &[&git::Change],
        prefix: &str,
        writable: bool,
    ) {
        // Keep deeply nested paths readable instead of consuming the entire pane with indentation.
        if prefix.matches('/').count() >= 3 || ui.available_width() < 160. {
            for file in files {
                let relative = file.path.strip_prefix(prefix).unwrap_or(&file.path);
                let name = relative.rsplit('/').next().unwrap_or(relative);
                self.change_row(ui, ctx, file, name, writable);
            }
            return;
        }
        let mut folders: BTreeMap<String, Vec<&git::Change>> = BTreeMap::new();
        for file in files {
            let relative = file.path.strip_prefix(prefix).unwrap_or(&file.path);
            if let Some((folder, _)) = relative.split_once('/') {
                folders.entry(folder.into()).or_default().push(*file);
            } else {
                self.change_row(ui, ctx, file, relative, writable);
            }
        }
        for (folder, files) in folders {
            egui::CollapsingHeader::new(format!("{folder}/"))
                .id_salt(format!("filetree/{prefix}{folder}"))
                .default_open(true)
                .show(ui, |ui| {
                    self.change_tree(ui, ctx, &files, &format!("{prefix}{folder}/"), writable);
                });
        }
    }
    fn change_row(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        change: &git::Change,
        label: &str,
        writable: bool,
    ) {
        let lock = self.commit_lock_for(&change.path);
        ui.horizontal(|ui|{
            ui.spacing_mut().item_spacing.x=4.;
            let mut staged=change.index!=' '&&change.index!='?';
            let partial=staged&&change.worktree!=' ';
            let checkbox=ui.add_enabled(writable&&!change.conflict()&&lock.is_none(),egui::Checkbox::new(&mut staged,"").indeterminate(partial));
            checkbox.widget_info(||{
                let enabled=writable&&!change.conflict()&&lock.is_none();
                let label=format!("Include {}{}",change.path,if partial{" · partially staged"}else{""});
                if partial{egui::WidgetInfo::labeled(egui::Role::CheckBox,enabled,label)}else{egui::WidgetInfo::selected(egui::Role::CheckBox,enabled,staged,label)}
            });
            if checkbox.on_hover_text(format!("Include {}",change.path)).changed(){if staged{self.run_git(&["add","--",&change.path],ctx);}else if self.repo.as_ref().is_some_and(|r|r.head.is_empty()){self.run_git(&["rm","--cached","--",&change.path],ctx);}else{self.run_git(&["restore","--staged","--",&change.path],ctx);}}
            let (status,status_text,color)=if lock.is_some(){("L","Locked",MUTED())}else if change.conflict(){("!","Conflict",DANGER())}else if change.index=='?' {("U","Untracked",SUCCESS())}else if change.index=='A'||change.worktree=='A'{("A","Added",SUCCESS())}else if change.index=='D'||change.worktree=='D'{("D","Deleted",DANGER())}else if change.index=='R'||change.worktree=='R'{("R","Renamed",ACCENT())}else{("M","Modified",WARNING())};
            theme::status_badge(ui, status, status_text, color);
            let file_width=(ui.available_width()-44.).max(20.);
            let selected=self.selected_path==change.path;
            let response=ui.add_sized([file_width,32.],egui::Button::new("").selected(selected).frame_when_inactive(selected));
            theme::paint_file_label(ui,response.rect,label);
            response.widget_info(||egui::WidgetInfo::selected(egui::Role::Button,ui.is_enabled(),selected,format!("{} · {}{}",change.path,status_text,if partial{" · partially staged"}else{""})));
            if response.on_hover_text(format!("{}\n{}",change.path,status_text)).clicked(){self.select_file(&change.path);}
            let menu=ui.menu_button("…",|ui|{
                if let Some(path)=&lock {
                    if ui.add_enabled(writable,egui::Button::new("Unlock for commits")).clicked(){self.execute(Task::SetCommitLock{path:path.clone(),locked:false},ctx);ui.close();}
                    if staged && ui.add_enabled(writable,egui::Button::new("Unstage locked file")).clicked(){self.execute(Task::SetCommitLock{path:path.clone(),locked:true},ctx);ui.close();}
                }else if ui.add_enabled(writable,egui::Button::new("Exclude from commits (lock)")).on_hover_text("Unstage this file and keep its local edits out of future Branchroom commits until you unlock it.").clicked(){self.execute(Task::SetCommitLock{path:change.path.clone(),locked:true},ctx);ui.close();}
                ui.separator();
                if ui.button("View working changes").clicked(){self.diff_staged=false;self.select_file(&change.path);ui.close();}
if ui.button("View staged changes").clicked(){self.diff_staged=true;self.select_file(&change.path);ui.close();}
                ui.add_enabled_ui(writable,|ui|{if ui.button("Ignore file…").clicked(){self.pending=Some(Pending{title:"Ignore file".into(),explanation:format!("Add an exact path pattern for {} to .gitignore. Tracked files remain tracked.",change.path),task:Task::Ignore(change.path.clone())});ui.close();}
if ui.button(RichText::new("Discard working changes…").color(DANGER())).clicked(){self.pending=Some(Pending{title:"Discard file changes".into(),explanation:format!("Restore {} to the staged version. A recovery copy will be saved outside the repository; recover it from Compare & recovery.",change.path),task:Task::Discard(change.path.clone())});ui.close();}});
                if ui.button("Show in Explorer").clicked(){self.report(process::reveal(&self.path().join(&change.path)));ui.close();}
            });
            menu.response.widget_info(||egui::WidgetInfo::labeled(egui::Role::Button,ui.is_enabled(),format!("File actions for {}",change.path)));
        });
    }

    fn patch_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, writable: bool) {
        if self.selected_path.is_empty() {
            ui.label(
                RichText::new("Select a changed file")
                    .size(TEXT_HEADER)
                    .strong(),
            );
            subtitle(
                ui,
                "Review its patch, then include the whole file or selected changes.",
            );
            return;
        }
        ui.add(
            egui::Label::new(RichText::new(&self.selected_path).strong())
                .wrap()
                .selectable(true),
        );
        let lock = self.commit_lock_for(&self.selected_path);
        if let Some(path) = &lock {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Locked · excluded from commits").color(MUTED()));
                if self.diff_staged
                    && ui
                        .add_enabled(writable, egui::Button::new("Unstage locked file"))
                        .clicked()
                {
                    self.execute(
                        Task::SetCommitLock {
                            path: path.clone(),
                            locked: true,
                        },
                        ctx,
                    );
                }
                if ui
                    .add_enabled(writable, egui::Button::new("Unlock"))
                    .clicked()
                {
                    self.execute(
                        Task::SetCommitLock {
                            path: path.clone(),
                            locked: false,
                        },
                        ctx,
                    );
                }
            });
        }
        let compact = ui.available_width() < 760.;
        let before = (self.diff_staged, self.diff_whitespace, self.diff_full);
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.diff_staged, false, "Working changes");
            ui.selectable_value(&mut self.diff_staged, true, "Staged changes");
            if compact {
                ui.menu_button("Diff options", |ui| {
                    ui.separator();
                    ui.selectable_value(&mut self.diff_split, false, "Unified");
                    ui.selectable_value(&mut self.diff_split, true, "Split");
                    ui.checkbox(&mut self.diff_whitespace, "Hide whitespace");
                    ui.checkbox(&mut self.diff_full, "Full context");

                    if let Some(patch) = self.file_patch.clone() {
                        let selectable = writable
                            && lock.is_none()
                            && !self.diff_whitespace
                            && !patch.binary
                            && !patch.prefix.contains("rename from ");
                        self.patch_actions(ui, ctx, writable, &patch, selectable);
                    }
                });
            } else {
                ui.separator();
                ui.selectable_value(&mut self.diff_split, false, "Unified");
                ui.selectable_value(&mut self.diff_split, true, "Split");
                ui.checkbox(&mut self.diff_whitespace, "Hide whitespace");
                ui.checkbox(&mut self.diff_full, "Full context");
            }
        });
        if before != (self.diff_staged, self.diff_whitespace, self.diff_full) {
            self.image_data = None;
            self.image_view = None;
            self.diff_refresh = true;
            self.file_patch = None;
        }
        let Some(patch) = self.file_patch.clone() else {
            subtitle(ui, "Loading file patch…");
            return;
        };
        let selectable = writable
            && lock.is_none()
            && !self.diff_whitespace
            && !patch.binary
            && !patch.prefix.contains("rename from ");
        if self.diff_whitespace {
            subtitle(
                ui,
                "Turn off whitespace hiding before staging individual lines.",
            );
        }
        if !compact {
            self.patch_actions(ui, ctx, writable, &patch, selectable);
        }
        ui.add_space(10.);
        if patch.binary && branchroom::image_diff::supported(&self.selected_path) {
            self.image_comparison(ui, ctx);
            return;
        }
        if patch.binary {
            ui.label(RichText::new("Binary file").size(TEXT_HEADER).strong());
            subtitle(
                ui,
                "This file has no text patch. Include or discard it using the file controls.",
            );
            return;
        }
        if patch.hunks.is_empty() {
            subtitle(
                ui,
                if self.diff_staged {
                    "This file has no staged changes."
                } else {
                    "No working changes in this file. Select Staged changes to review what is included."
                },
            );
            return;
        }
        self.render_patch(ui, &patch, selectable);
    }

    fn patch_actions(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        _writable: bool,
        patch: &Patch,
        selectable: bool,
    ) {
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    selectable,
                    egui::Button::new(if self.diff_staged {
                        "Unstage selected changes"
                    } else {
                        "Stage selected changes"
                    }),
                )
                .clicked()
            {
                let result = if self.diff_staged {
                    patch.unstaging_selection(&self.selected_hunks, &self.selected_lines)
                } else {
                    patch.selection(&self.selected_hunks, &self.selected_lines)
                };
                match result {
                    Ok(patch) => self.execute(Task::ApplyPatch(patch), ctx),
                    Err(e) => self.report(Err(e)),
                }
            }
            if ui.button("Select all hunks").clicked() {
                self.selected_hunks = (0..patch.hunks.len()).collect();
            }
            if ui.button("Clear selection").clicked() {
                self.selected_hunks.clear();
                self.selected_lines.clear();
            }
            if ui.button("Copy patch").clicked() {
                ctx.copy_text(patch.raw.clone());
            }
        });
    }

    fn render_patch(&mut self, ui: &mut egui::Ui, patch: &Patch, selectable: bool) {
        if self.diff_split {
            self.render_split_patch(ui, patch, selectable);
            return;
        }
        enum Row {
            Header(usize),
            Line(usize, usize),
        }
        let mut rows = vec![];
        for (hi, hunk) in patch.hunks.iter().enumerate() {
            rows.push(Row::Header(hi));
            rows.extend((0..hunk.lines.len()).map(|li| Row::Line(hi, li)));
        }
        ui.spacing_mut().item_spacing.y = 0.;
        egui::ScrollArea::both()
            .id_salt("structured_patch")
            .auto_shrink([false, false])
            .show_rows(ui, 27., rows.len(), |ui, range| {
                ui.spacing_mut().interact_size.y = 24.;
                ui.spacing_mut().item_spacing.y = 0.;
                for row in &rows[range] {
                    match *row {
                        Row::Header(hi) => {
                            ui.horizontal(|ui| {
                                let mut selected = self.selected_hunks.contains(&hi);
                                if ui
                                    .add_enabled(
                                        selectable,
                                        egui::Checkbox::new(&mut selected, &patch.hunks[hi].header),
                                    )
                                    .changed()
                                {
                                    self.set_hunk_selection(hi, selected);
                                }
                            });
                        }
                        Row::Line(hi, li) => {
                            let line = &patch.hunks[hi].lines[li];
                            let color = match line.kind {
                                LineKind::Added => SUCCESS(),
                                LineKind::Removed => DANGER(),
                                LineKind::Meta => MUTED(),
                                _ => INK(),
                            };
                            let changed = matches!(line.kind, LineKind::Added | LineKind::Removed);
                            let fill = if changed {
                                color.gamma_multiply(0.08)
                            } else {
                                Color32::TRANSPARENT
                            };
                            egui::Frame::new().fill(fill).show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let mut selected = self.selected_hunks.contains(&hi)
                                        || self.selected_lines.contains(&(hi, li));
                                    let response = ui.add_enabled(
                                        selectable && changed,
                                        egui::Checkbox::new(&mut selected, ""),
                                    );
                                    response.widget_info(|| {
                                        egui::WidgetInfo::labeled(
                                            egui::Role::CheckBox,
                                            ui.is_enabled(),
                                            format!(
                                                "Include line {}: {}",
                                                line.new.or(line.old).unwrap_or(0),
                                                line.text
                                            ),
                                        )
                                    });
                                    if response.changed() {
                                        self.set_line_selection(patch, hi, li, selected);
                                    }
                                    ui.add_sized(
                                        [40., 24.],
                                        egui::Label::new(
                                            RichText::new(
                                                line.old.map(|n| n.to_string()).unwrap_or_default(),
                                            )
                                            .monospace()
                                            .color(MUTED()),
                                        ),
                                    );
                                    ui.add_sized(
                                        [40., 24.],
                                        egui::Label::new(
                                            RichText::new(
                                                line.new.map(|n| n.to_string()).unwrap_or_default(),
                                            )
                                            .monospace()
                                            .color(MUTED()),
                                        ),
                                    );

                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&line.text).monospace().color(color),
                                        )
                                        .selectable(true)
                                        .extend(),
                                    );
                                });
                            });
                        }
                    }
                }
            });
    }
    fn render_split_patch(&mut self, ui: &mut egui::Ui, patch: &Patch, selectable: bool) {
        enum Row {
            Header(usize),
            Pair(usize, Option<usize>, Option<usize>),
        }
        let mut rows = Vec::new();
        for (hi, hunk) in patch.hunks.iter().enumerate() {
            rows.push(Row::Header(hi));
            let mut li = 0;
            while li < hunk.lines.len() {
                if hunk.lines[li].kind == LineKind::Removed
                    || hunk.lines[li].kind == LineKind::Added
                {
                    let mut removed = Vec::new();
                    let mut added = Vec::new();
                    while li < hunk.lines.len() && hunk.lines[li].kind == LineKind::Removed {
                        removed.push(li);
                        li += 1;
                    }
                    while li < hunk.lines.len() && hunk.lines[li].kind == LineKind::Added {
                        added.push(li);
                        li += 1;
                    }
                    for n in 0..removed.len().max(added.len()) {
                        rows.push(Row::Pair(
                            hi,
                            removed.get(n).copied(),
                            added.get(n).copied(),
                        ));
                    }
                } else {
                    rows.push(Row::Pair(hi, Some(li), Some(li)));
                    li += 1;
                }
            }
        }
        let longest = patch
            .hunks
            .iter()
            .flat_map(|h| &h.lines)
            .max_by_key(|line| line.text.chars().count())
            .map(|line| line.text.replace('\t', "    "))
            .unwrap_or_default();
        let text_width = ui
            .painter()
            .layout_no_wrap(longest, egui::FontId::monospace(13.), INK())
            .size()
            .x
            + 20.;
        let view_width = ui.available_width();
        let overflow = (text_width - (view_width / 2. - 72.)).max(0.);
        let mut offset = 0.;
        egui::Panel::bottom("split_horizontal_scroll")
            .exact_size(20.)
            .frame(egui::Frame::new().fill(PAPER()))
            .show(ui, |ui| {
                let scroll = egui::ScrollArea::horizontal()
                    .id_salt(("split_x", &self.selected_path))
                    .auto_shrink([false, false])
                    .min_scrolled_height(1.)
                    .show(ui, |ui| {
                        ui.allocate_space(egui::vec2(view_width + overflow, 1.));
                    });
                offset = scroll.state.clamped_offset().x;
            });
        let (header, header_response) =
            ui.allocate_exact_size(egui::vec2(view_width, 32.), egui::Sense::hover());
        header_response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::Role::Label,
                true,
                if self.diff_staged {
                    "Left: before, HEAD. Right: after, index."
                } else {
                    "Left: before, index. Right: after, working tree."
                },
            )
        });
        ui.spacing_mut().item_spacing.y = 0.;
        let body = egui::ScrollArea::vertical()
            .id_salt(("split_patch_y", &self.selected_path))
            .auto_shrink([false, false])
            .show_rows(ui, 27., rows.len(), |ui, range| {
                for row in &rows[range] {
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 27.),
                        egui::Sense::hover(),
                    );
                    match *row {
                        Row::Header(hi) => {
                            ui.painter().rect_filled(rect, 0., RAISED());
                            let mut cell = ui.new_child(
                                egui::UiBuilder::new()
                                    .id_salt(("hunk", hi))
                                    .max_rect(rect)
                                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                            );
                            cell.set_clip_rect(rect.intersect(ui.clip_rect()));
                            let mut checked = self.selected_hunks.contains(&hi);
                            if cell
                                .add_enabled(
                                    selectable,
                                    egui::Checkbox::new(&mut checked, &patch.hunks[hi].header),
                                )
                                .changed()
                            {
                                self.set_hunk_selection(hi, checked);
                            }
                        }
                        Row::Pair(hi, left, right) => {
                            let mid = rect.center().x;
                            let left_rect =
                                egui::Rect::from_min_max(rect.min, egui::pos2(mid, rect.bottom()));
                            let right_rect =
                                egui::Rect::from_min_max(egui::pos2(mid, rect.top()), rect.max);
                            self.split_cell(
                                ui,
                                patch,
                                SplitCell {
                                    position: (hi, left),
                                    old: true,
                                    rect: left_rect,
                                    offset,
                                    selectable,
                                },
                            );
                            self.split_cell(
                                ui,
                                patch,
                                SplitCell {
                                    position: (hi, right),
                                    old: false,
                                    rect: right_rect,
                                    offset,
                                    selectable,
                                },
                            );
                            ui.painter().line_segment(
                                [egui::pos2(mid, rect.top()), egui::pos2(mid, rect.bottom())],
                                egui::Stroke::new(1., LINE()),
                            );
                        }
                    }
                }
            });
        let header = egui::Rect::from_min_max(
            header.min,
            egui::pos2(body.inner_rect.right(), header.bottom()),
        );
        ui.painter().rect_filled(header, 0., SURFACE());
        let before = if self.diff_staged {
            "Before · HEAD"
        } else {
            "Before · index"
        };
        let after = if self.diff_staged {
            "After · index"
        } else {
            "After · working tree"
        };
        for (x, label) in [
            (header.left() + 12., before),
            (header.center().x + 12., after),
        ] {
            ui.painter().text(
                egui::pos2(x, header.center().y),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(13.),
                MUTED(),
            );
        }
        ui.painter().line_segment(
            [header.center_top(), header.center_bottom()],
            egui::Stroke::new(1., LINE()),
        );
    }

    fn split_cell(&mut self, ui: &mut egui::Ui, patch: &Patch, cell: SplitCell) {
        let SplitCell {
            position: (hi, li),
            old,
            rect,
            offset,
            selectable,
        } = cell;
        let Some(li) = li else {
            ui.painter().rect_filled(rect, 0., SIDEBAR());
            return;
        };
        let line = &patch.hunks[hi].lines[li];
        let changed = matches!(line.kind, LineKind::Added | LineKind::Removed);
        let color = match line.kind {
            LineKind::Added => SUCCESS(),
            LineKind::Removed => DANGER(),
            _ => INK(),
        };
        if changed {
            ui.painter()
                .rect_filled(rect, 0., color.gamma_multiply(0.08));
        }
        let gutter = egui::Rect::from_min_size(rect.min, egui::vec2(66., 27.));
        let mut controls = ui.new_child(
            egui::UiBuilder::new()
                .id_salt(("split_gutter", hi, li, old))
                .max_rect(gutter)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        controls.set_clip_rect(gutter.intersect(ui.clip_rect()));
        let mut checked =
            self.selected_hunks.contains(&hi) || self.selected_lines.contains(&(hi, li));
        let response =
            controls.add_enabled(selectable && changed, egui::Checkbox::new(&mut checked, ""));
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::Role::CheckBox,
                selectable && changed,
                format!(
                    "Include {} line {}: {}",
                    if old { "old" } else { "new" },
                    if old { line.old } else { line.new }.unwrap_or(0),
                    line.text
                ),
            )
        });
        if response.changed() {
            self.set_line_selection(patch, hi, li, checked);
        }
        let number = if old { line.old } else { line.new }
            .map(|n| n.to_string())
            .unwrap_or_default();
        controls.label(RichText::new(number).monospace().size(12.).color(MUTED()));
        let code_rect =
            egui::Rect::from_min_max(egui::pos2(rect.left() + 72., rect.top()), rect.max);
        let text_rect = egui::Rect::from_min_max(
            egui::pos2(code_rect.left() - offset, rect.top()),
            egui::pos2(rect.right() + 10000., rect.bottom()),
        );
        let mut code = ui.new_child(
            egui::UiBuilder::new()
                .id_salt(("split_code", hi, li, old))
                .max_rect(text_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        code.set_clip_rect(code_rect.intersect(ui.clip_rect()));
        let content = line
            .text
            .get(1..)
            .unwrap_or(&line.text)
            .replace('\t', "    ");
        code.add(
            egui::Label::new(RichText::new(&content).monospace().size(13.).color(color))
                .extend()
                .selectable(true),
        )
        .on_hover_text(content);
    }
    fn set_hunk_selection(&mut self, hi: usize, selected: bool) {
        self.selected_lines.retain(|(h, _)| *h != hi);
        if selected {
            self.selected_hunks.insert(hi);
        } else {
            self.selected_hunks.remove(&hi);
        }
    }
    fn set_line_selection(&mut self, patch: &Patch, hi: usize, li: usize, selected: bool) {
        if self.selected_hunks.remove(&hi) {
            for (index, line) in patch.hunks[hi].lines.iter().enumerate() {
                if matches!(line.kind, LineKind::Added | LineKind::Removed) {
                    self.selected_lines.insert((hi, index));
                }
            }
        }
        if selected {
            self.selected_lines.insert((hi, li));
        } else {
            self.selected_lines.remove(&(hi, li));
        }
    }
}
