use super::*;
impl Branchroom {
    pub(super) fn conflicts(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        heading(
            ui,
            "Conflicts",
            "Inspect each side, edit the result, and stage the resolution.",
        );
        let repo = self.repo.as_ref().unwrap().clone();
        if !repo.operation.is_empty() {
            surface().show(ui,|ui|{
            ui.horizontal_wrapped(|ui|{
                badge(ui,&format!("{} in progress",repo.operation),WARNING());
                if primary(ui,"Continue operation").clicked(){self.propose("Continue operation","Continue after every conflicted file has been resolved and staged.",&[&repo.operation,"--continue"]);}
                ui.menu_button("Operation actions",|ui|{
                    if repo.operation=="rebase"&&ui.button("Skip current commit").clicked(){self.propose("Skip current commit","Drop the current replayed commit and continue rebasing.",&["rebase","--skip"]);ui.close();}
                    if ui.button(RichText::new("Abort operation").color(DANGER())).clicked(){self.propose("Abort operation","Return to the state before this operation. Resolution edits can be lost.",&[&repo.operation,"--abort"]);ui.close();}
                });
            });
        });
            ui.add_space(18.);
        }
        let conflicts: Vec<_> = repo.changes.iter().filter(|c| c.conflict()).collect();
        if conflicts.is_empty() {
            surface().show(ui, |ui| {
                ui.heading("No unmerged files");
                subtitle(
                    ui,
                    if repo.operation.is_empty() {
                        "Your working tree has no conflicts to resolve."
                    } else {
                        "All conflicts are staged. Continue the operation above when ready."
                    },
                );
            });
            return;
        }
        section(ui, "UNMERGED FILES");
        for change in conflicts {
            ui.horizontal_wrapped(|ui|{
                badge(ui,"Unmerged",DANGER());
                if ui.selectable_label(self.conflict_path==change.path,&change.path).clicked(){
                    match std::fs::read_to_string(repo.root.join(&change.path)){
                        Ok(content)=>{self.conflict_path=change.path.clone();self.conflict_original=content.clone();self.conflict_content=content;self.conflict_side=3;},
                        Err(e)=>self.report(Err(format!("Cannot read this as text: {e}. Resolve binary or deleted files using your IDE or the file actions."))),
                    }
                }
                ui.menu_button("File actions",|ui|{
                    if ui.button("Stage external resolution").clicked(){self.propose("Stage resolved file","Stage the file as it is. Verify that your external editor removed every conflict marker.",&["add","--", &change.path]);ui.close();}
                    if ui.button(RichText::new("Resolve as deleted").color(DANGER())).clicked(){self.propose("Resolve file as deleted","Remove this conflicted file from the index and working directory.",&["rm","--",&change.path]);ui.close();}
                    if ui.button("Open project in IDE").clicked(){let result=self.settings.open(&repo.root,self.settings.ide_for(&repo.root));self.report(result);ui.close();}
                });
            });
        }
        ui.add_space(20.);
        if self.conflict_path.is_empty() {
            subtitle(
                ui,
                "Select a file to inspect its versions and edit the result.",
            );
            return;
        }
        surface().show(ui,|ui|{
            ui.label(RichText::new(&self.conflict_path).strong());ui.add_space(8.);
            ui.horizontal_wrapped(|ui|{
                for(index,label)in["Base","Current (ours)","Incoming (theirs)","Result editor"].iter().enumerate(){
                    if ui.selectable_value(&mut self.conflict_side,index,*label).clicked()&&index<3{
                        self.detail_title=label.to_string();self.read("git",process::strings(&["show",&format!(":{}:{}",index+1,self.conflict_path)]),ctx);
                    }
                }
            });ui.add_space(12.);
            if self.conflict_side==3{
                egui::ScrollArea::both().id_salt("resolution_editor").max_height(360.).show(ui,|ui|{ui.add(egui::TextEdit::multiline(&mut self.conflict_content).hint_text("Resolved file contents").code_editor().desired_rows(14).desired_width(f32::INFINITY));});
                ui.add_space(12.);
                if ui.button("Save result & stage").clicked(){self.pending=Some(Pending{title:"Save conflict resolution".into(),explanation:format!("Write your edited result to {} and stage it. Conflict markers and externally changed files will be rejected.",self.conflict_path),task:Task::Resolve{path:self.conflict_path.clone(),content:self.conflict_content.clone(),original:self.conflict_original.clone()}});}
            }else{egui::ScrollArea::both().id_salt("conflict_side_preview").max_height(360.).show(ui,|ui|{ui.add(egui::Label::new(RichText::new(&self.detail).monospace()).selectable(true));});}
        });
        ui.add_space(10.);
        subtitle(
            ui,
            "During rebase, ours and theirs describe Git's replay process. Review the content rather than relying on the side name.",
        );
    }
    pub(super) fn remotes(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        heading(
            ui,
            "Repositories",
            "Clone a project, manage remotes, and open independent parallel workspaces.",
        );
        surface().show(ui, |ui| {
            ui.label(
                RichText::new("Clone or create a repository")
                    .size(18.)
                    .strong(),
            );
            ui.add_space(12.);
            ui.label("Repository URL");
            ui.add(
                egui::TextEdit::singleline(&mut self.clone_url)
                    .desired_width(f32::INFINITY)
                    .hint_text("https://github.com/owner/project.git"),
            );
            ui.label("Destination directory");
            ui.horizontal_wrapped(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.clone_directory)
                        .desired_width(420.)
                        .hint_text("C:\\Projects\\project"),
                );
                if ui.button("Choose parent folder…").clicked()
                    && let Some(path) = rfd::FileDialog::new().pick_folder()
                {
                    self.clone_directory = path.join("new-project").display().to_string();
                }
            });
            ui.add_space(12.);
            ui.horizontal_wrapped(|ui| {
                if primary(ui, "Clone repository").clicked() {
                    if self.clone_url.is_empty()
                        || self.clone_directory.is_empty()
                        || self.clone_url.starts_with('-')
                    {
                        self.report(Err(
                            "Enter a repository URL and destination directory, then clone again."
                                .into(),
                        ));
                    } else {
                        self.pending = Some(Pending {
                            title: "Clone repository".into(),
                            explanation: format!(
                                "Clone {} into {} and open the project.",
                                self.clone_url, self.clone_directory
                            ),
                            task: Task::Clone {
                                url: self.clone_url.clone(),
                                destination: PathBuf::from(&self.clone_directory),
                            },
                        });
                    }
                }
                if ui.button("Initialize empty repository").clicked() {
                    if self.clone_directory.is_empty() {
                        self.report(Err(
                            "Choose a destination directory before initializing a repository."
                                .into(),
                        ));
                    } else {
                        self.pending = Some(Pending {
                            title: "Initialize repository".into(),
                            explanation: format!(
                                "Create a Git repository at {} with main as its initial branch.",
                                self.clone_directory
                            ),
                            task: Task::Init(PathBuf::from(&self.clone_directory)),
                        });
                    }
                }
                if ui.button("Open destination").clicked() {
                    self.open_repo(PathBuf::from(&self.clone_directory), ctx);
                }
            });
        });
        ui.add_space(20.);
        if let Some(repo) = self.repo.clone() {
            surface().show(ui, |ui| {
                ui.label(RichText::new("Remotes").size(18.).strong());
                ui.add_space(12.);
                if repo.remotes.is_empty() {
                    subtitle(
                        ui,
                        "No remotes configured. Add one to fetch and publish this project.",
                    );
                } else {
                    ui.add(
                        egui::Label::new(RichText::new(&repo.remotes).monospace())
                            .wrap()
                            .selectable(true),
                    );
                }
                ui.add_space(12.);
                ui.horizontal_wrapped(|ui| {
                    ui.label("Name");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.remote_name)
                            .hint_text("Remote name")
                            .desired_width(120.),
                    );
                    ui.label("URL");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.remote_url)
                            .hint_text("Remote URL")
                            .desired_width(360.),
                    );
                });
                ui.horizontal_wrapped(|ui| {
                    let name = self.remote_name.clone();
                    let url = self.remote_url.clone();
                    if ui.button("Add remote").clicked() {
                        if !name.is_empty() && !name.starts_with('-') && !url.is_empty() {
                            self.propose(
                                "Add remote",
                                "Register this name and URL.",
                                &["remote", "add", &name, &url],
                            );
                        } else {
                            self.report(
                                Err("Enter a remote name and URL before adding it.".into()),
                            );
                        }
                    }
                    if ui.button("Change URL").clicked()
                        && !name.starts_with('-')
                        && !name.is_empty()
                        && !url.is_empty()
                    {
                        self.propose(
                            "Change remote URL",
                            "Change the URL used for fetching and pushing this remote.",
                            &["remote", "set-url", &name, &url],
                        );
                    }
                    ui.menu_button("More remote actions", |ui| {
                        if ui.button("Set upstream & publish branch").clicked() {
                            self.form(
                                "Publish current branch",
                                "Push this branch and remember its upstream.",
                                "git",
                                &["push", "--set-upstream"],
                                &[("Remote", "origin"), ("Branch", &repo.branch)],
                                &[],
                            );
                            ui.close();
                        }
                        if ui
                            .button(RichText::new("Remove remote").color(DANGER()))
                            .clicked()
                            && !name.starts_with('-')
                            && !name.is_empty()
                        {
                            self.propose(
                                "Remove remote",
                                "Remove this remote and its remote-tracking references.",
                                &["remote", "remove", &name],
                            );
                            ui.close();
                        }
                    });
                });
            });
            ui.add_space(20.);
            surface().show(ui,|ui|{
                ui.label(RichText::new("Independent parallel workspaces").size(18.).strong());ui.add_space(12.);
                ui.add(egui::Label::new(RichText::new(&repo.worktrees).monospace()).wrap().selectable(true));ui.add_space(12.);
                ui.horizontal_wrapped(|ui|{
                    if ui.button("Add branch parallel workspace").clicked(){self.form("Add branch parallel workspace","Check out an existing branch in a new directory.","git",&["worktree","add"],&[("New directory",""),("Branch","")],&[]);}
                    if ui.button("Open parallel workspace in IDE…").clicked()&&let Some(path)=rfd::FileDialog::new().pick_folder(){let result=self.settings.open(&path,self.settings.ide_for(&repo.root));self.report(result);}
                    ui.menu_button("Manage parallel workspaces",|ui|{if ui.button(RichText::new("Remove clean parallel workspace…").color(DANGER())).clicked(){self.form("Remove clean parallel workspace","Git refuses dirty, locked, and main worktrees. The selected directory will be removed.","git",&["worktree","remove"],&[("Parallel workspace directory","")],&[]);ui.close();}});
                });
            });
        }
    }
    pub(super) fn github(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        heading(
            ui,
            "Pull requests",
            "Review proposed changes or create a pull request for your branch.",
        );
        if !self.github_available {
            surface().show(ui,|ui|{ui.heading("Set up GitHub access");ui.label("Install GitHub CLI, then sign in to browse and manage pull requests. Git-only features are available without it.");ui.hyperlink_to("Open GitHub CLI installation instructions","https://cli.github.com/");if ui.button("Check installation again").clicked(){self.github_available=process::find_executable("gh").is_some();if self.github_available{self.report(Ok("GitHub CLI found. Select Sign in to GitHub to continue.".into()));}else{self.report(Err("GitHub CLI was not found. Install it, then check again. If it was installed outside the standard location, restart Branchroom after adding it to PATH.".into()));}}});
            return;
        }
        egui::CollapsingHeader::new("GitHub connection").default_open(true).show(ui,|ui|{
            ui.horizontal_wrapped(|ui|{
                ui.label(RichText::new("Account connection").size(18.).strong());
                if primary(ui,"Sign in to GitHub…").clicked(){if self.preview_mode{self.report(Err("Sign-in is unavailable in UI preview. Restart without --ui-preview to connect GitHub.".into()));}else{self.report(process::github_login());}}
                if ui.button("Check account").clicked(){self.detail_title="GitHub account".into();self.read("gh",process::strings(&["auth","status"]),ctx);}
            });ui.add_space(8.);subtitle(ui,"Requires GitHub CLI (gh). Complete browser sign-in in the opened terminal; credentials stay with GitHub CLI.");
        });
        ui.add_space(20.);
        self.pr_workspace(ui, ctx);
        ui.add_space(20.);
        egui::CollapsingHeader::new("Repository tools & PR lookup").show(ui,|ui|{
            ui.horizontal_wrapped(|ui|{
                for(label,args)in[("Load pull requests",vec!["pr","list","--limit","50"]),("Load issues",vec!["issue","list","--limit","50"]),("Load Actions runs",vec!["run","list","--limit","30"]),("Browse repository",vec!["browse"])]{if ui.button(label).clicked(){self.detail_title=label.into();self.read("gh",process::strings(&args),ctx);}}
            });ui.add_space(16.);
            ui.horizontal_wrapped(|ui|{
                ui.label("Pull request number");ui.add(egui::TextEdit::singleline(&mut self.github_number).desired_width(90.).hint_text("42"));
                if let Ok(number)=self.github_number.parse::<u64>(){let number=number.to_string();for(label,args)in[("View details",vec!["pr","view",&number]),("View diff",vec!["pr","diff",&number]),("View checks",vec!["pr","checks",&number])]{if ui.button(label).clicked(){self.detail_title=format!("PR #{number} · {label}");self.read("gh",process::strings(&args),ctx);}}
                    if ui.button("Check out branch").clicked(){self.pending=Some(Pending{title:"Check out pull request".into(),explanation:"Fetch this pull request and switch the working tree to its branch.".into(),task:Task::Command{program:"gh".into(),args:process::strings(&["pr","checkout",&number]),refresh:true}});}
                }
            });ui.add_space(16.);
            egui::CollapsingHeader::new("More GitHub actions").show(ui,|ui|{ui.horizontal_wrapped(|ui|{
                if ui.button("Fork this repository…").clicked(){self.pending=Some(Pending{title:"Fork repository on GitHub".into(),explanation:"Create a fork in your account, add it as a remote, and keep this checkout on its current branch.".into(),task:Task::Command{program:"gh".into(),args:process::strings(&["repo","fork","--clone=false","--remote=true"]),refresh:true}});}
                if ui.button("Create issue…").clicked(){self.form("Create issue","Publish an issue in the connected GitHub repository.","gh",&["issue","create","--title"],&[("Title","")],&["--body",""]);}
                if ui.button("List my repositories").clicked(){self.detail_title="Your GitHub repositories".into();self.read("gh",process::strings(&["repo","list","--limit","100"]),ctx);}
            });});
            egui::CollapsingHeader::new("Create a private GitHub repository").show(ui,|ui|{if ui.button("Create repository from this project…").clicked(){self.form("Create private GitHub repository","Create a private repository and add origin. Files are not pushed automatically.","gh",&["repo","create"],&[("Repository name","")],&["--private","--source",".","--remote","origin"]);}});
        });
        if !self.detail.is_empty() {
            ui.add_space(20.);
            section(ui, &self.detail_title);
            egui::ScrollArea::both()
                .id_salt("github_output")
                .max_height(400.)
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(&self.detail).monospace()).selectable(true),
                    );
                });
        }
    }
    pub(super) fn tools(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        heading(
            ui,
            "Command center",
            "Inspect, maintain, or recover a repository with Git's complete command surface.",
        );
        surface().show(ui, |ui| {
            section(ui, "INSPECT REPOSITORY");
            ui.add_space(8.);
            ui.horizontal_wrapped(|ui| {
                for (label, args) in [
                    ("Reflog", vec!["reflog", "--date=relative", "-100"]),
                    ("Tags", vec!["tag", "-n"]),
                    ("Submodules", vec!["submodule", "status"]),
                    ("Object storage", vec!["count-objects", "-vH"]),
                    ("Verify objects", vec!["fsck", "--full"]),
                    ("Configuration", vec!["config", "--list", "--show-origin"]),
                    ("All Git commands", vec!["help", "-a"]),
                ] {
                    if ui.button(label).clicked() {
                        self.detail_title = label.into();
                        self.read("git", process::strings(&args), ctx);
                    }
                }
            });
            ui.add_space(14.);
            egui::CollapsingHeader::new("Maintenance actions").show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Rename branch…").clicked() {
                        self.form(
                            "Rename branch",
                            "Rename an existing local branch.",
                            "git",
                            &["branch", "-m"],
                            &[("Old name", ""), ("New name", "")],
                            &[],
                        );
                    }
                    if ui.button("Set author name…").clicked() {
                        self.form(
                            "Set author name",
                            "Set the author name for this repository.",
                            "git",
                            &["config", "user.name"],
                            &[("Author name", "")],
                            &[],
                        );
                    }
                    if ui.button("Set author email…").clicked() {
                        self.form(
                            "Set author email",
                            "Set the author email for this repository.",
                            "git",
                            &["config", "user.email"],
                            &[("Email address", "")],
                            &[],
                        );
                    }
                    if ui.button("Blame file…").clicked() {
                        self.form(
                            "Inspect line authorship",
                            "Read authorship for a repository-relative file path.",
                            "git",
                            &["blame", "--"],
                            &[("File path", "")],
                            &[],
                        );
                    }
                    if ui.button("Update submodules").clicked() {
                        self.propose(
                            "Update submodules",
                            "Initialize and check out recorded submodule commits recursively.",
                            &["submodule", "update", "--init", "--recursive"],
                        );
                    }
                    if ui.button("Remember conflict resolutions").clicked() {
                        self.propose(
                            "Enable rerere",
                            "Store conflict resolutions locally for recurring conflicts.",
                            &["config", "rerere.enabled", "true"],
                        );
                    }
                });
            });
        });
        ui.add_space(20.);
        surface().show(ui,|ui|{
            ui.label(RichText::new("Run a command").size(18.).strong());ui.add_space(12.);
            ui.horizontal(|ui|{ui.label("Executable");egui::ComboBox::from_id_salt("command_program").selected_text(&self.command_program).show_ui(ui,|ui|{ui.selectable_value(&mut self.command_program,"git".into(),"git");ui.selectable_value(&mut self.command_program,"gh".into(),"gh");}).response.on_hover_text("Command executable");});
            ui.label("Arguments as a JSON array");ui.add(egui::TextEdit::multiline(&mut self.command_json).hint_text("Command arguments as a JSON array").code_editor().desired_rows(3).desired_width(f32::INFINITY));subtitle(ui,"Each string is one argument. Shell expressions are not expanded. Interactive commands are unsupported.");ui.add_space(14.);
            if primary(ui,"Review command").clicked(){match serde_json::from_str::<Vec<String>>(&self.command_json){Ok(args)if !args.is_empty()=>self.pending=Some(Pending{title:"Run command".into(),explanation:"This command runs with your permissions and can change local files, configuration, or remote services. Review every argument.".into(),task:Task::Command{program:self.command_program.clone(),args,refresh:true}}),_=>self.report(Err("Enter a nonempty JSON array of argument strings, for example [\"status\", \"--short\"].".into()))}}
            ui.add_space(8.);egui::CollapsingHeader::new("Argument examples").show(ui,|ui|{ui.monospace("[\"log\", \"--oneline\", \"--graph\", \"--all\"]\n[\"bundle\", \"create\", \"backup.bundle\", \"--all\"]\n[\"config\", \"user.email\", \"you@example.com\"]\n[\"lfs\", \"status\"]");});
        });
        if !self.detail.is_empty() {
            ui.add_space(20.);
            section(ui, &self.detail_title);
            code(ui, &self.detail);
        }
    }
    pub(super) fn preferences(&mut self, ui: &mut egui::Ui) {
        heading(
            ui,
            "Settings",
            "Make the workspace and project launchers fit the way you work.",
        );
        surface().show(ui, |ui| {
            ui.label(RichText::new("Workspace appearance").size(18.).strong());
            ui.horizontal(|ui| {
                ui.label("Theme");
                let changed = ui
                    .selectable_value(&mut self.settings.light_theme, false, "Dark")
                    .changed()
                    | ui.selectable_value(&mut self.settings.light_theme, true, "Light")
                        .changed();
                if changed {
                    theme::install(ui.ctx(), self.settings.light_theme);
                    self.save();
                }
            });
            ui.add_space(12.);
            ui.horizontal_wrapped(|ui| {
                ui.label("History density");
                for (index, label) in ["Compact", "Comfortable", "Spacious"].iter().enumerate() {
                    ui.selectable_value(&mut self.density, index, *label);
                }
            });
            ui.checkbox(
                &mut self.auto_refresh,
                "Refresh the repository automatically while idle",
            );
            subtitle(
                ui,
                "Fetch remains explicit. Automatic refresh only reads local files, branches and tags.",
            );
            if ui.button("Reset interface scale to 100%").clicked() {
                ui.ctx().set_zoom_factor(1.);
                self.settings.ui_zoom = 1.;
            }
        });
        ui.add_space(12.);
        egui::CollapsingHeader::new("About Branchroom").show(ui, |ui| {
            ui.label(format!(
                "Branchroom {} · {} / {}",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS,
                std::env::consts::ARCH
            ));
            for (program, label) in [("git", "Git"), ("gh", "GitHub CLI")] {
                ui.label(format!(
                    "{label}: {}",
                    process::find_executable(program)
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "Not installed".into())
                ));
            }
            ui.hyperlink_to("Git installation", "https://git-scm.com/downloads");
            ui.hyperlink_to("GitHub CLI installation", "https://cli.github.com/");
        });
        ui.add_space(20.);
        surface().show(ui, |ui| {
            ui.label(RichText::new("Project IDE").size(18.).strong());
            ui.add_space(12.);
            ui.horizontal_wrapped(|ui| {
                ui.label("Default IDE");
                ide_combo(
                    ui,
                    "default_ide",
                    &self.settings.clone(),
                    &mut self.settings.default_ide,
                );
            });
            if let Some(repo) = self.repo.clone() {
                let mut selected = self
                    .settings
                    .projects
                    .get(&repo.root)
                    .and_then(|p| p.ide)
                    .map(|i| i + 1)
                    .unwrap_or(0);
                ui.horizontal_wrapped(|ui| {
                    ui.label("This project");
                    egui::ComboBox::from_id_salt("project_ide")
                        .selected_text(if selected == 0 {
                            "Use default".into()
                        } else {
                            self.settings
                                .ides
                                .get(selected - 1)
                                .map(|i| i.name.clone())
                                .unwrap_or_default()
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut selected, 0, "Use default");
                            for (i, ide) in self.settings.ides.iter().enumerate() {
                                ui.selectable_value(&mut selected, i + 1, &ide.name);
                            }
                        })
                        .response
                        .on_hover_text("Project IDE");
                });
                self.settings
                    .projects
                    .entry(repo.root.clone())
                    .or_default()
                    .ide = selected.checked_sub(1);
            }
        });
        ui.add_space(20.);
        surface().show(ui,|ui|{
            ui.label(RichText::new("IDE launchers").size(18.).strong());ui.add_space(12.);
            for(index,ide)in self.settings.ides.iter_mut().enumerate(){egui::CollapsingHeader::new(&ide.name).id_salt(("ide",index)).show(ui,|ui|{
                ui.label("Display name");ui.add(egui::TextEdit::singleline(&mut ide.name).desired_width(300.));ui.label("Executable path");ui.horizontal_wrapped(|ui|{ui.add(egui::TextEdit::singleline(&mut ide.executable).desired_width(420.));if ui.button("Browse executable…").clicked()&&let Some(path)=rfd::FileDialog::new().pick_file(){ide.executable=path.display().to_string();}});
                subtitle(ui,&format!("Arguments: {}",serde_json::to_string(&ide.arguments).unwrap_or_default()));
            });}
            ui.add_space(12.);egui::CollapsingHeader::new("Add a custom IDE").show(ui,|ui|{
                ui.label("IDE name");ui.text_edit_singleline(&mut self.ide_name);ui.label("Executable path");ui.horizontal_wrapped(|ui|{ui.add(egui::TextEdit::singleline(&mut self.ide_executable).desired_width(420.));if ui.button("Browse…").clicked()&&let Some(path)=rfd::FileDialog::new().pick_file(){self.ide_executable=path.display().to_string();}});
                ui.label("Arguments as JSON · {path} becomes the project directory");ui.text_edit_singleline(&mut self.ide_arguments);
                if ui.button("Add launcher").clicked(){match serde_json::from_str::<Vec<String>>(&self.ide_arguments){Ok(arguments)if !self.ide_name.trim().is_empty()&&!self.ide_executable.trim().is_empty()=>{self.settings.ides.push(branchroom::settings::Ide{name:self.ide_name.clone(),executable:self.ide_executable.clone(),arguments});},_=>self.report(Err("Enter an IDE name, executable path, and JSON argument array before adding the launcher.".into()))}}
            });
        });
        ui.add_space(20.);
        if primary(ui, "Save preferences").clicked() {
            self.error = false;
            if self.preview_mode {
                self.report(Ok("Preferences are not saved in UI preview.".into()));
            } else {
                self.save();
                if !self.error {
                    self.report(Ok("Preferences saved".into()));
                }
            }
        }
        ui.add_space(12.);
        ui.add(
            egui::Label::new(
                RichText::new(format!(
                    "Preferences file: {}",
                    branchroom::settings::settings_path().display()
                ))
                .size(12.)
                .color(MUTED()),
            )
            .wrap()
            .selectable(true),
        );
    }
}
