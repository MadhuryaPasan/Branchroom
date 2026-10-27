use super::*;
impl Branchroom {
    fn draft_key(repo: &RepoState) -> String {
        if repo.branch == "Detached HEAD" {
            format!("detached/{}", repo.head)
        } else {
            repo.branch.clone()
        }
    }
    pub(super) fn persist_draft(&mut self) {
        if self.preview_mode {
            return;
        }
        let Some(repo) = &self.repo else {
            return;
        };
        let key = Self::draft_key(repo);
        let draft = branchroom::settings::Draft {
            summary: self.commit_message.clone(),
            description: self.commit_description.clone(),
            coauthors: self.coauthor.clone(),
            amend: self.amend,
            pr_title: self.pr_title.clone(),
            pr_body: self.pr_body.clone(),
            pr_reviewers: self.pr_reviewers.clone(),
            pr_labels: self.pr_labels.clone(),
            pr_base: self.pr_base.clone(),
            pr_head: self.pr_head.clone(),
            pr_draft: self.pr_draft,
        };
        let drafts = &mut self
            .settings
            .projects
            .entry(repo.root.clone())
            .or_default()
            .drafts;
        if drafts.get(&key) == Some(&draft) {
            return;
        }
        let root = repo.root.clone();
        let previous = drafts.insert(key.clone(), draft);
        if let Err(error) = self.settings.save() {
            let drafts = &mut self.settings.projects.entry(root).or_default().drafts;
            if let Some(previous) = previous {
                drafts.insert(key, previous);
            } else {
                drafts.remove(&key);
            }
            self.report(Err(error));
        }
    }
    pub(super) fn load_draft(&mut self, repo: &RepoState) {
        let draft = self
            .settings
            .projects
            .get(&repo.root)
            .and_then(|p| p.drafts.get(&Self::draft_key(repo)))
            .cloned();
        let draft = draft.unwrap_or(branchroom::settings::Draft {
            pr_draft: true,
            ..Default::default()
        });
        self.commit_message = draft.summary;
        self.commit_description = draft.description;
        self.coauthor = draft.coauthors;
        self.amend = draft.amend;
        self.pr_title = draft.pr_title;
        self.pr_body = draft.pr_body;
        self.pr_reviewers = draft.pr_reviewers;
        self.pr_labels = draft.pr_labels;
        self.pr_base = draft.pr_base;
        self.pr_head = draft.pr_head;
        self.pr_draft = draft.pr_draft;
    }
}
