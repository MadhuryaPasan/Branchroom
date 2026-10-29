use super::*;
#[derive(Default)]
pub(super) struct GraphContext {
    key: String,
    selected_key: String,
    selected_at: Option<Instant>,
    links_receiver: Option<Receiver<(String, Vec<branchroom::collaboration::PullRequest>)>>,
    links: std::collections::HashMap<String, Vec<u64>>,
    checked_at: Option<Instant>,
    receiver: Option<Receiver<Result<ContextData>>>,
    pub(super) data: Option<Result<ContextData>>,
}
pub(super) struct ContextData {
    pub reviews: Vec<ReviewBadge>,
    pub identity_warning: Option<String>,
}
pub(super) struct ReviewBadge {
    pub number: u64,
    pub title: String,
    pub branch: String,
    pub base_branch: String,
    pub commit: String,
    pub url: String,
    pub state: String,
    pub ci: String,
}
impl GraphContext {
    fn finish_links(
        &mut self,
        root: &std::path::Path,
        key: String,
        prs: Vec<branchroom::collaboration::PullRequest>,
    ) {
        let belongs_to_current = key.starts_with(&format!("{}:", root.display()));
        self.links
            .insert(key, prs.iter().map(|p| p.number).collect());
        if belongs_to_current && let Some(Ok(data)) = &mut self.data {
            for pr in prs {
                if !data.reviews.iter().any(|p| p.number == pr.number) {
                    data.reviews.push(ReviewBadge {
                        number: pr.number,
                        title: pr.title,
                        branch: pr.head_ref_name,
                        base_branch: pr.base_ref_name,
                        commit: String::new(),
                        url: pr.url,
                        state: pr.state.to_lowercase(),
                        ci: "CI not reported".into(),
                    });
                }
            }
        }
    }

    pub(super) fn linked_to(&self, root: &std::path::Path, commit: &str, number: u64) -> bool {
        self.links
            .get(&format!("{}:{commit}", root.display()))
            .is_some_and(|ids| ids.contains(&number))
    }
}
impl Branchroom {
    pub(super) fn load_graph_context(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.graph_context.receiver
            && let Ok(data) = rx.try_recv()
        {
            self.graph_context.data = Some(data);
            self.graph_context.receiver = None;
        }
        let Some(repo) = &self.repo else { return };
        if let Some(rx) = &self.graph_context.links_receiver
            && let Ok((key, prs)) = rx.try_recv()
        {
            self.graph_context.finish_links(&repo.root, key, prs);
            self.graph_context.links_receiver = None;
        }
        let selected_key = format!("{}:{}", repo.root.display(), self.selected_commit);
        if self.graph_context.selected_key != selected_key {
            self.graph_context.selected_key = selected_key.clone();
            self.graph_context.selected_at = Some(Instant::now());
        }
        if !self.preview_mode
            && self.github_available
            && !repo.remotes.is_empty()
            && !self.selected_commit.is_empty()
            && self.graph_context.links_receiver.is_none()
            && !self.graph_context.links.contains_key(&selected_key)
            && self
                .graph_context
                .selected_at
                .is_some_and(|t| t.elapsed() > Duration::from_millis(500))
        {
            let (tx, rx) = mpsc::channel();
            self.graph_context.links_receiver = Some(rx);
            let (root, commit, ctx) =
                (repo.root.clone(), self.selected_commit.clone(), ctx.clone());
            std::thread::spawn(move || {
                let prs = branchroom::collaboration::commit_pull_requests(&root, &commit)
                    .unwrap_or_default();
                let _ = tx.send((selected_key, prs));
                ctx.request_repaint();
            });
        }
        let key = format!("{}:{}", repo.root.display(), repo.head);
        if self.preview_mode
            || !self.github_available
            || repo.remotes.is_empty()
            || (self.graph_context.key == key
                && self
                    .graph_context
                    .checked_at
                    .is_some_and(|t| t.elapsed() < Duration::from_secs(60)))
            || self.graph_context.receiver.is_some()
        {
            return;
        }
        self.graph_context.key = key;
        self.graph_context.checked_at = Some(Instant::now());
        self.graph_context.data = None;
        let (root, ctx) = (repo.root.clone(), ctx.clone());
        let (tx, rx) = mpsc::channel();
        self.graph_context.receiver = Some(rx);
        std::thread::spawn(move || {
            let result = (|| {
                let json = process::stdout(
                    "gh",
                    &root,
                    &process::strings(&[
                        "pr",
                        "list",
                        "--state",
                        "all",
                        "--limit",
                        "100",
                        "--json",
                        "number,title,headRefName,headRefOid,baseRefName,url,state,statusCheckRollup",
                    ]),
                )?;
                let prs: serde_json::Value =
                    serde_json::from_str(&json).map_err(|e| e.to_string())?;
                let reviews = prs
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|p| {
                        let checks = p["statusCheckRollup"].as_array();
                        let ci = if checks.is_none_or(|c| c.is_empty()) {
                            "CI not reported"
                        } else if checks.unwrap().iter().any(|c| {
                            matches!(
                                c["conclusion"].as_str().unwrap_or(""),
                                "FAILURE" | "CANCELLED" | "TIMED_OUT"
                            ) || c["state"] == "FAILURE"
                                || c["state"] == "ERROR"
                        }) {
                            "CI failed"
                        } else if checks.unwrap().iter().any(|c| {
                            c["status"] == "IN_PROGRESS"
                                || c["status"] == "QUEUED"
                                || c["state"] == "PENDING"
                        }) {
                            "CI running"
                        } else {
                            "CI complete"
                        };
                        let text = |key: &str| p[key].as_str().unwrap_or("").to_owned();
                        ReviewBadge {
                            number: p["number"].as_u64().unwrap_or(0),
                            title: text("title"),
                            branch: text("headRefName"),
                            base_branch: text("baseRefName"),
                            commit: text("headRefOid"),
                            url: text("url"),
                            state: text("state").to_lowercase(),
                            ci: ci.into(),
                        }
                    })
                    .collect();
                let identity_warning = (|| {
                    let email = git::git(&root, &["config", "user.email"]).ok()?;
                    let user =
                        process::stdout("gh", &root, &process::strings(&["api", "user"])).ok()?;
                    let user: serde_json::Value = serde_json::from_str(&user).ok()?;
                    let account_email = user["email"].as_str()?;
                    if account_email.is_empty() || account_email.eq_ignore_ascii_case(email.trim())
                    {
                        None
                    } else {
                        Some(format!(
                            "Commits use {}. Your GitHub account reports {}. Check your Git identity in Settings.",
                            email.trim(),
                            account_email
                        ))
                    }
                })();
                Ok(ContextData {
                    reviews,
                    identity_warning,
                })
            })();
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delayed_review_from_another_repository_does_not_enter_current_inspector() {
        let mut context = GraphContext {
            data: Some(Ok(ContextData {
                reviews: vec![],
                identity_warning: None,
            })),
            ..Default::default()
        };
        let pr = branchroom::collaboration::PullRequest {
            number: 7,
            title: "Other repo".into(),
            head_ref_name: "feature".into(),
            base_ref_name: "main".into(),
            url: "https://example.invalid/other/7".into(),
            is_draft: false,
            state: "OPEN".into(),
        };
        context.finish_links(
            std::path::Path::new("current"),
            "other:abc".into(),
            vec![pr],
        );
        assert!(
            context
                .data
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap()
                .reviews
                .is_empty()
        );
        assert!(!context.linked_to(std::path::Path::new("current"), "abc", 7));
        assert!(context.linked_to(std::path::Path::new("other"), "abc", 7));
    }
}
