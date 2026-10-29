use crate::process::{Result, command};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ide {
    pub name: String,
    pub executable: String,
    pub arguments: Vec<String>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub ide: Option<usize>,
    pub base: String,
    pub drafts: BTreeMap<String, Draft>,
}
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Draft {
    pub summary: String,
    pub description: String,
    pub coauthors: String,
    pub amend: bool,
    pub pr_title: String,
    pub pr_body: String,
    pub pr_reviewers: String,
    pub pr_labels: String,
    pub pr_base: String,
    pub pr_head: String,
    pub pr_draft: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct HistoryColumns {
    // Zero uses the available width or the number of graph lanes.
    pub graph: f32,
    pub commit: f32,
    pub author: f32,
    pub date: f32,
}
impl Default for HistoryColumns {
    fn default() -> Self {
        Self {
            graph: 0.,
            commit: 0.,
            author: 160.,
            date: 112.,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub recent: Vec<PathBuf>,
    pub ides: Vec<Ide>,
    pub default_ide: usize,
    pub projects: BTreeMap<PathBuf, Project>,
    pub ui_density: usize,
    pub ui_zoom: f32,
    pub light_theme: bool,
    pub advanced_mode: bool,
    pub auto_refresh: bool,
    pub history_columns: HistoryColumns,
    pub snapshot_source_width: f32,
    pub branch_source_width: f32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            recent: vec![],
            default_ide: 0,
            projects: BTreeMap::new(),
            ui_density: 1,
            ui_zoom: 1.,
            light_theme: false,
            advanced_mode: false,
            auto_refresh: true,
            history_columns: HistoryColumns::default(),
            snapshot_source_width: 194.,
            branch_source_width: 205.,
            ides: vec![
                Ide {
                    name: "Visual Studio Code".into(),
                    executable: "code".into(),
                    arguments: vec!["--new-window".into(), "{path}".into()],
                },
                Ide {
                    name: "Cursor".into(),
                    executable: "cursor".into(),
                    arguments: vec!["--new-window".into(), "{path}".into()],
                },
                Ide {
                    name: "Zed".into(),
                    executable: "zed".into(),
                    arguments: vec!["{path}".into()],
                },
            ],
        }
    }
}
thread_local! {static ISOLATED_GIT:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
thread_local! {static STATE_DIRECTORY:std::cell::RefCell<Option<PathBuf>>=const{std::cell::RefCell::new(None)};}
/// Scoped state location for isolated clients and tests; never changes process environment.
pub struct StateDirectory {
    previous: Option<PathBuf>,
    path: PathBuf,
    active: bool,
    previous_isolation: bool,
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl StateDirectory {
    pub fn enter(path: PathBuf) -> Self {
        Self::enter_with_isolation(path, false)
    }
    pub fn enter_isolated(path: PathBuf) -> Self {
        Self::enter_with_isolation(path, true)
    }
    fn enter_with_isolation(path: PathBuf, isolated: bool) -> Self {
        let previous_isolation = ISOLATED_GIT.with(|flag| flag.replace(isolated));
        let previous = STATE_DIRECTORY.with(|slot| slot.replace(Some(path.clone())));
        Self {
            previous,
            path,
            active: true,
            previous_isolation,
            _thread: std::marker::PhantomData,
        }
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn restore(&mut self) {
        if self.active {
            ISOLATED_GIT.with(|flag| flag.set(self.previous_isolation));
            STATE_DIRECTORY.with(|slot| {
                slot.replace(self.previous.take());
            });
            self.active = false;
        }
    }
}
impl Drop for StateDirectory {
    fn drop(&mut self) {
        self.restore();
    }
}
pub fn isolated_git_config() -> Option<PathBuf> {
    if !ISOLATED_GIT.with(std::cell::Cell::get) {
        return None;
    }
    STATE_DIRECTORY.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|path| path.join("empty-global.gitconfig"))
    })
}
pub fn settings_path() -> PathBuf {
    if let Some(path) = STATE_DIRECTORY.with(|slot| slot.borrow().clone()) {
        return path.join("settings.json");
    }
    std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir)
                .join(".config")
        })
        .join("Branchroom")
        .join("settings.json")
}
impl Settings {
    pub fn load() -> Result<Self> {
        Self::load_from(&settings_path())
    }
    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?).map_err(|e| {
            format!(
                "Cannot read preferences: {e}. Repair {} before saving.",
                path.display()
            )
        })
    }
    pub fn save(&self) -> Result<()> {
        self.save_to(&settings_path())
    }
    pub fn save_to(&self, path: &Path) -> Result<()> {
        if path.exists() && Self::load_from(path).is_err() {
            return Err(format!(
                "Preferences at {} are unreadable. The original file is preserved; repair or move it before saving.",
                path.display()
            ));
        }
        crate::storage::write_atomic(
            path,
            &serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?,
        )
    }
    pub fn remember(&mut self, path: &Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.into());
        self.recent.truncate(20);
    }
    pub fn ide_for(&self, path: &Path) -> usize {
        self.projects
            .get(path)
            .and_then(|p| p.ide)
            .unwrap_or(self.default_ide)
            .min(self.ides.len().saturating_sub(1))
    }
    pub fn open(&self, path: &Path, index: usize) -> Result<String> {
        crate::safety::ensure_writable(path)?;
        let ide = self.ides.get(index).ok_or("Select an IDE in Settings.")?;
        let arguments: Vec<_> = ide
            .arguments
            .iter()
            .map(|s| s.replace("{path}", &path.to_string_lossy()))
            .collect();
        // Invoke executables directly. Custom launchers should point to .exe, not shell scripts.
        let executable = ide.executable.clone();
        #[cfg(windows)]
        let executable = if executable == "code" || executable == "cursor" {
            let local = std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_default();
            let candidate = if executable == "code" {
                local.join("Programs/Microsoft VS Code/Code.exe")
            } else {
                local.join("Programs/cursor/Cursor.exe")
            };
            if candidate.is_file() {
                candidate.to_string_lossy().into_owned()
            } else {
                executable
            }
        } else {
            executable
        };
        command(&executable, path)
            .args(arguments)
            .spawn()
            .map_err(|e| {
                format!(
                    "Cannot launch {}: {e}. Set its executable path in Settings.",
                    ide.name
                )
            })?;
        Ok(format!("Opened {} in {}", path.display(), ide.name))
    }
}
