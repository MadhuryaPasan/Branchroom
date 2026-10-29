use super::*;
#[derive(Clone)]
struct Account {
    login: String,
    host: String,
}
#[derive(Default)]
pub(super) struct Connection {
    key: String,
    receiver: Option<Receiver<(String, Result<Option<Account>>)>>,
    value: Option<Result<Option<Account>>>,
    checked: Option<Instant>,
}
fn account_from_json(text: &str, host: &str) -> Result<Option<Account>> {
    let json:serde_json::Value=serde_json::from_str(text).map_err(|_|"GitHub CLI returned an unreadable account status. Update the CLI and check the account again".to_owned())?;
    let accounts = json["hosts"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(name, _)| host.is_empty() || name.as_str() == host);
    for (hostname, values) in accounts {
        for account in values.as_array().into_iter().flatten() {
            if account["active"].as_bool().unwrap_or(true)
                && account["state"].as_str() == Some("success")
                && let Some(login) = account["login"].as_str()
            {
                return Ok(Some(Account {
                    login: login.into(),
                    host: hostname.into(),
                }));
            }
        }
    }
    Ok(None)
}
fn remote_host(remotes: &str) -> String {
    let url = remotes
        .lines()
        .find(|l| l.starts_with("origin\t"))
        .or_else(|| remotes.lines().next())
        .and_then(|l| l.split_whitespace().nth(1))
        .unwrap_or("");
    if let Some((_, tail)) = url.split_once("://") {
        tail.split('/')
            .next()
            .unwrap_or("")
            .rsplit('@')
            .next()
            .unwrap_or("")
            .split(':')
            .next()
            .unwrap_or("")
            .into()
    } else if let Some((_, tail)) = url.split_once('@') {
        tail.split(':').next().unwrap_or("").into()
    } else {
        String::new()
    }
}
impl Branchroom {
    pub(super) fn github_connection(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let key = self.path().display().to_string();
        if self.connection.key != key {
            self.connection = Default::default();
            self.connection.key = key.clone();
        }
        if let Some(rx) = &self.connection.receiver
            && let Ok((root, value)) = rx.try_recv()
        {
            self.connection.receiver = None;
            if root == key {
                self.connection.value = Some(value);
            }
        }
        if !self.preview_mode
            && self.connection.receiver.is_none()
            && self
                .connection
                .checked
                .is_none_or(|at| at.elapsed() > Duration::from_secs(120))
        {
            self.connection.checked = Some(Instant::now());
            let (tx, rx) = mpsc::channel();
            self.connection.receiver = Some(rx);
            let host = self
                .repo
                .as_ref()
                .map(|r| remote_host(&r.remotes))
                .unwrap_or_default();
            let (root, ctx) = (self.path(), ctx.clone());
            std::thread::spawn(move || {
                let mut args = process::strings(&[
                    "auth",
                    "status",
                    "--active",
                    "--json",
                    "hosts",
                    "--no-refresh",
                ]);
                if !host.is_empty() {
                    args.extend(["--hostname".into(), host.clone()]);
                }
                let result = process::stdout("gh", &root, &args)
                    .and_then(|text| account_from_json(&text, &host));
                let _ = tx.send((key, result));
                ctx.request_repaint();
            });
        }
        let account = self
            .connection
            .value
            .as_ref()
            .and_then(|v| v.as_ref().ok())
            .and_then(|v| v.clone());
        let label = account
            .as_ref()
            .map(|a| format!("● {}", a.login))
            .unwrap_or_else(|| {
                if self.preview_mode || self.prs_loaded {
                    "GitHub · Connected".into()
                } else {
                    "GitHub account".into()
                }
            });
        ui.menu_button(label, |ui| {
            if let Some(account) = &account {
                ui.label(format!(
                    "Signed in as {} on {}",
                    account.login, account.host
                ));
            } else {
                subtitle(ui, "Branchroom uses your GitHub CLI account");
            }
            if ui.button("Check account").clicked() {
                self.connection.checked = None;
                ui.close();
            }
            if ui.button("Sign in to GitHub…").clicked() {
                if !self.preview_mode {
                    self.report(process::github_login());
                    self.connection.checked = None;
                }
                ui.close();
            }
        });
        if matches!(self.connection.value, Some(Ok(None))) {
            surface().show(ui, |ui| {
                ui.label(RichText::new("Connect GitHub").strong());
                subtitle(
                    ui,
                    "Sign in with GitHub CLI to load pull requests for this repository",
                );
                if ui.button("Sign in to GitHub…").clicked() {
                    self.report(process::github_login());
                    self.connection.checked = None;
                }
            });
        }
        if matches!(self.connection.value, Some(Err(_))) {
            subtitle(
                ui,
                "The account check could not finish. Check your connection and retry from GitHub account",
            );
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn active_account_is_scoped_to_the_repository_host() {
        let text = r#"{"hosts":{"github.com":[{"active":true,"state":"success","login":"alice"}],"git.example.com":[{"active":true,"state":"error","login":"bob"}]}}"#;
        assert_eq!(
            account_from_json(text, "github.com")
                .unwrap()
                .unwrap()
                .login,
            "alice"
        );
        assert!(
            account_from_json(text, "git.example.com")
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn repository_hosts_support_ssh_https_and_local_paths() {
        assert_eq!(
            remote_host("origin\tgit@git.example.com:team/repo.git (fetch)"),
            "git.example.com"
        );
        assert_eq!(
            remote_host("origin\thttps://github.com/team/repo.git (fetch)"),
            "github.com"
        );
        assert_eq!(remote_host("origin\tC:/repos/local.git (fetch)"), "");
    }
}
