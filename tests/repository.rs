use branchroom::{
    git::{self, git},
    graph, snapshot,
};
use std::{fs, path::PathBuf};

struct Fixture(PathBuf, branchroom::settings::StateDirectory);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("branchroom-test-{}", snapshot::unique_id()));
        fs::create_dir_all(&path).unwrap();
        let state = branchroom::settings::StateDirectory::enter_isolated(
            std::env::temp_dir().join(format!("branchroom-state-{}", snapshot::unique_id())),
        );
        git(&path, &["init", "-b", "main"]).unwrap();
        git(&path, &["config", "user.name", "Branchroom tests"]).unwrap();
        git(&path, &["config", "user.email", "tests@example.invalid"]).unwrap();
        git(&path, &["config", "commit.gpgsign", "false"]).unwrap();
        git(&path, &["config", "core.autocrlf", "false"]).unwrap();
        Self(path, state)
    }
    fn write(&self, name: &str, text: &str) {
        fs::write(self.0.join(name), text).unwrap();
    }
    fn commit(&self, message: &str) {
        git(&self.0, &["add", "-A"]).unwrap();
        git(&self.0, &["commit", "-m", message]).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let state = self.1.path().to_path_buf();
        self.1.restore();
        if state.parent() == Some(std::env::temp_dir().as_path())
            && state
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("branchroom-state-"))
        {
            let _ = fs::remove_dir_all(state);
        }
        // Every fixture lives directly below the temp directory with a generated prefix.
        if self.0.parent() == Some(std::env::temp_dir().as_path())
            && self
                .0
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("branchroom-test-")
        {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

#[test]
fn snapshot_preserves_index_and_head_and_captures_untracked() {
    let f = Fixture::new();
    f.write("file.txt", "original\n");
    f.write(".gitignore", "ignored.txt\n");
    f.commit("Initial");
    f.write("file.txt", "staged\n");
    git(&f.0, &["add", "file.txt"]).unwrap();
    f.write("file.txt", "working\n");
    f.write("new file.txt", "new\n");
    f.write("ignored.txt", "secret\n");
    let index = git(&f.0, &["write-tree"]).unwrap();
    let head = git::resolve(&f.0, "HEAD").unwrap();
    let status = git(&f.0, &["status", "--porcelain=v1", "-z"]).unwrap();
    snapshot::create(&f.0, "Experiment one").unwrap();
    assert_eq!(git(&f.0, &["write-tree"]).unwrap(), index);
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), head);
    assert_eq!(
        git(&f.0, &["status", "--porcelain=v1", "-z"]).unwrap(),
        status
    );
    let repo = git::load(&f.0, "main", 100).unwrap();
    assert_eq!(repo.snapshots.len(), 1);
    let id = &repo.snapshots[0].id;
    assert_eq!(
        git(&f.0, &["show", &format!("{id}:file.txt")]).unwrap(),
        "working\n"
    );
    assert_eq!(
        git(&f.0, &["show", &format!("{id}:new file.txt")]).unwrap(),
        "new\n"
    );
    assert!(git(&f.0, &["show", &format!("{id}:ignored.txt")]).is_err());
}

#[test]
fn snapshot_can_capture_unborn_repository() {
    let f = Fixture::new();
    f.write("first.txt", "hello");
    snapshot::create(&f.0, "Before first commit").unwrap();
    assert!(git::resolve(&f.0, "HEAD").is_err());
    assert_eq!(git::load(&f.0, "main", 100).unwrap().snapshots.len(), 1);
}

#[test]
fn snapshot_opens_independent_worktrees_and_promotes() {
    let f = Fixture::new();
    f.write("file.txt", "first");
    f.commit("Initial");
    snapshot::create(&f.0, "First").unwrap();
    f.write("file.txt", "second");
    snapshot::create(&f.0, "Second").unwrap();
    let repo = git::load(&f.0, "main", 100).unwrap();
    let a = &repo.snapshots[0];
    let b = &repo.snapshots[1];
    let work_a = f.0.join("work-a");
    let work_b = f.0.join("work-b");
    snapshot::materialize(&f.0, &a.reference, &work_a).unwrap();
    snapshot::materialize(&f.0, &b.reference, &work_b).unwrap();
    assert_ne!(
        fs::read_to_string(work_a.join("file.txt")).unwrap(),
        fs::read_to_string(work_b.join("file.txt")).unwrap()
    );
    snapshot::promote(&f.0, &a.reference, "final-choice").unwrap();
    assert_eq!(git::resolve(&f.0, "final-choice").unwrap(), a.id);
    assert_eq!(
        git(&f.0, &["branch", "--show-current"]).unwrap().trim(),
        "main"
    );
    assert!(snapshot::materialize(&f.0, &a.reference, &work_a).is_err());
    git(&f.0, &["worktree", "remove", &work_a.to_string_lossy()]).unwrap();
    git(&f.0, &["worktree", "remove", &work_b.to_string_lossy()]).unwrap();
}

#[test]
fn divergence_and_real_conflict_are_detected() {
    let f = Fixture::new();
    f.write("file.txt", "base\n");
    f.commit("Base");
    git(&f.0, &["switch", "-c", "feature"]).unwrap();
    f.write("file.txt", "feature\n");
    f.commit("Feature");
    git(&f.0, &["switch", "main"]).unwrap();
    f.write("file.txt", "main\n");
    f.commit("Main");
    git(&f.0, &["switch", "feature"]).unwrap();
    let state = git::load(&f.0, "main", 100).unwrap();
    let feature = state.branches.iter().find(|b| b.name == "feature").unwrap();
    assert_eq!((feature.ahead, feature.behind), (1, 1));
    assert!(git(&f.0, &["merge", "--no-edit", "main"]).is_err());
    let state = git::load(&f.0, "main", 100).unwrap();
    assert_eq!(state.operation, "merge");
    assert!(state.changes[0].conflict());
    assert!(snapshot::create(&f.0, "Unresolved").is_err());
    git(&f.0, &["merge", "--abort"]).unwrap();
    assert!(git::load(&f.0, "main", 100).unwrap().operation.is_empty());
}

#[test]
fn nul_status_handles_rename_spaces_and_unicode() {
    let changes = git::parse_status(
        "R  renamed file.txt\0original file.txt\0?? café\nfile.rs\0 M changed.txt\0",
    );
    assert_eq!(changes.len(), 3);
    assert_eq!(changes[0].original.as_deref(), Some("original file.txt"));
    assert_eq!(changes[1].path, "café\nfile.rs");
    assert_eq!(changes[2].worktree, 'M');
}

#[test]
fn graph_draws_merge_edges_and_keeps_shared_parent() {
    use git::Commit;
    let commits = vec![
        Commit {
            id: "merge".into(),
            parents: vec!["a".into(), "b".into()],
            ..Default::default()
        },
        Commit {
            id: "a".into(),
            parents: vec!["root".into()],
            ..Default::default()
        },
        Commit {
            id: "b".into(),
            parents: vec!["root".into()],
            ..Default::default()
        },
        Commit {
            id: "root".into(),
            ..Default::default()
        },
    ];
    let rows = graph::layout(&commits);
    assert_eq!(rows[0].edges.len(), 2);
    assert_eq!(rows[0].width, 2);
    assert_eq!(rows[2].edges.len(), 2);
    assert!(rows[3].edges.is_empty());
}

#[test]
fn snapshot_tree_uses_git_parents_instead_of_capture_order() {
    use git::Snapshot;
    let snapshots = vec![
        Snapshot {
            reference: "latest".into(),
            parents: vec!["base-a".into()],
            ..Default::default()
        },
        Snapshot {
            reference: "middle".into(),
            parents: vec!["base-b".into()],
            ..Default::default()
        },
        Snapshot {
            reference: "oldest".into(),
            parents: vec!["base-a".into()],
            ..Default::default()
        },
        Snapshot {
            reference: "unborn".into(),
            ..Default::default()
        },
    ];
    let groups = graph::snapshot_groups(&snapshots);
    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0].parent.as_deref(), Some("base-a"));
    assert_eq!(
        groups[0]
            .snapshots
            .iter()
            .map(|s| s.reference.as_str())
            .collect::<Vec<_>>(),
        vec!["latest", "oldest"]
    );
    assert!(groups[2].parent.is_none());
}

#[test]
fn history_branch_filter_keeps_both_merge_parents_and_excludes_other_tips() {
    use git::Commit;
    let commits = vec![
        Commit {
            id: "other".into(),
            parents: vec!["root".into()],
            ..Default::default()
        },
        Commit {
            id: "merge".into(),
            parents: vec!["a".into(), "b".into()],
            ..Default::default()
        },
        Commit {
            id: "a".into(),
            parents: vec!["root".into()],
            ..Default::default()
        },
        Commit {
            id: "b".into(),
            parents: vec!["root".into()],
            ..Default::default()
        },
        Commit {
            id: "root".into(),
            ..Default::default()
        },
    ];
    let visible = graph::ancestors(&commits, "merge");
    assert_eq!(
        visible.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec!["merge", "a", "b", "root"]
    );
}

#[test]
fn older_preferences_receive_interface_defaults() {
    let settings: branchroom::settings::Settings =
        serde_json::from_str("{\"recent\":[],\"projects\":{}}").unwrap();
    assert_eq!(settings.ui_density, 1);
    assert_eq!(settings.ui_zoom, 1.);
    assert!(settings.auto_refresh);
    assert_eq!(settings.history_columns.graph, 0.);
    assert_eq!(settings.snapshot_source_width, 194.);
    assert_eq!(settings.branch_source_width, 205.);
}

fn changed_lines(
    patch: &branchroom::diff::Patch,
    text: &str,
) -> std::collections::BTreeSet<(usize, usize)> {
    patch
        .hunks
        .iter()
        .enumerate()
        .flat_map(|(hi, h)| {
            h.lines
                .iter()
                .enumerate()
                .filter(move |(_, l)| l.text == text)
                .map(move |(li, _)| (hi, li))
        })
        .collect()
}

#[test]
fn partial_staging_and_unstaging_preserve_other_lines() {
    let f = Fixture::new();
    f.write("file.txt", "one\ntwo\nthree\n");
    f.commit("Initial");
    f.write("file.txt", "ONE\ntwo\nTHREE\n");
    let patch = branchroom::diff::file_patch(&f.0, "file.txt", false, false, false).unwrap();
    let mut selected = changed_lines(&patch, "-one");
    selected.extend(changed_lines(&patch, "+ONE"));
    let selection = patch.selection(&Default::default(), &selected).unwrap();
    branchroom::diff::apply_to_index(&f.0, &selection, false).unwrap();
    assert_eq!(
        git(&f.0, &["show", ":file.txt"]).unwrap(),
        "ONE\ntwo\nthree\n"
    );
    git(&f.0, &["add", "file.txt"]).unwrap();
    let staged = branchroom::diff::file_patch(&f.0, "file.txt", true, false, false).unwrap();
    let mut selected = changed_lines(&staged, "-one");
    selected.extend(changed_lines(&staged, "+ONE"));
    let selection = staged
        .unstaging_selection(&Default::default(), &selected)
        .unwrap();
    branchroom::diff::apply_to_index(&f.0, &selection, false).unwrap();
    assert_eq!(
        git(&f.0, &["show", ":file.txt"]).unwrap(),
        "one\ntwo\nTHREE\n"
    );
    assert!(
        fs::read_to_string(f.0.join("file.txt"))
            .unwrap()
            .contains("ONE")
    );
}

#[test]
fn partial_new_and_deleted_files_work_without_rewriting_the_worktree() {
    let f = Fixture::new();
    f.write("old.txt", "keep\nremove\n");
    f.commit("Initial");
    f.write("new.txt", "first\nsecond\n");
    let patch = branchroom::diff::file_patch(&f.0, "new.txt", false, false, false).unwrap();
    let selection = patch
        .selection(&Default::default(), &changed_lines(&patch, "+first"))
        .unwrap();
    branchroom::diff::apply_to_index(&f.0, &selection, false).unwrap();
    assert_eq!(git(&f.0, &["show", ":new.txt"]).unwrap(), "first\n");
    let staged = branchroom::diff::file_patch(&f.0, "new.txt", true, false, false).unwrap();
    let selection = staged
        .unstaging_selection(&[0].into_iter().collect(), &Default::default())
        .unwrap();
    branchroom::diff::apply_to_index(&f.0, &selection, false).unwrap();
    assert!(git(&f.0, &["show", ":new.txt"]).is_err());
    fs::remove_file(f.0.join("old.txt")).unwrap();
    let patch = branchroom::diff::file_patch(&f.0, "old.txt", false, false, false).unwrap();
    let selection = patch
        .selection(&Default::default(), &changed_lines(&patch, "-remove"))
        .unwrap();
    branchroom::diff::apply_to_index(&f.0, &selection, false).unwrap();
    assert_eq!(git(&f.0, &["show", ":old.txt"]).unwrap(), "keep\n");
    assert!(!f.0.join("old.txt").exists());
}

#[test]
fn whole_hunk_deletion_has_valid_zero_line_ranges() {
    let f = Fixture::new();
    f.write("file.txt", "only\n");
    f.commit("Initial");
    f.write("file.txt", "");
    let patch = branchroom::diff::file_patch(&f.0, "file.txt", false, false, false).unwrap();
    let selection = patch
        .selection(&[0].into_iter().collect(), &Default::default())
        .unwrap();
    branchroom::diff::apply_to_index(&f.0, &selection, false).unwrap();
    assert_eq!(git(&f.0, &["show", ":file.txt"]).unwrap(), "");
}

#[test]
fn squash_retains_tree_and_recovery_ref_and_refuses_published_commits() {
    let f = Fixture::new();
    f.write("file", "base");
    f.commit("Base");
    f.write("a", "alpha");
    f.commit("A");
    f.write("b", "beta");
    f.commit("B");
    let tree = git(&f.0, &["rev-parse", "HEAD^{tree}"]).unwrap();
    let old = git::resolve(&f.0, "HEAD").unwrap();
    branchroom::history::run(
        &f.0,
        &branchroom::history::Action::Squash {
            count: 2,
            message: "Combined change".into(),
        },
    )
    .unwrap();
    assert_eq!(git(&f.0, &["rev-parse", "HEAD^{tree}"]).unwrap(), tree);
    assert_eq!(
        git(&f.0, &["rev-list", "--count", "HEAD"]).unwrap().trim(),
        "2"
    );
    assert!(
        git(
            &f.0,
            &[
                "for-each-ref",
                "--format=%(objectname)",
                "refs/branchroom/recovery"
            ]
        )
        .unwrap()
        .contains(&old)
    );
    let head = git::resolve(&f.0, "HEAD").unwrap();
    git(&f.0, &["update-ref", "refs/remotes/origin/main", &head]).unwrap();
    assert!(branchroom::history::run(&f.0, &branchroom::history::Action::Undo).is_err());
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), head);
}

#[test]
fn reorder_independent_commits_is_atomic_and_keeps_final_content() {
    let f = Fixture::new();
    f.write("base", "base");
    f.commit("Base");
    f.write("a", "alpha");
    f.commit("A");
    f.write("b", "beta");
    f.commit("B");
    let tree = git(&f.0, &["rev-parse", "HEAD^{tree}"]).unwrap();
    let mut ids = branchroom::history::linear_tip(&f.0, 2).unwrap();
    ids.reverse();
    branchroom::history::run(&f.0, &branchroom::history::Action::Reorder(ids)).unwrap();
    assert_eq!(git(&f.0, &["rev-parse", "HEAD^{tree}"]).unwrap(), tree);
    assert!(
        git(&f.0, &["log", "--format=%s", "-2"])
            .unwrap()
            .starts_with("A\nB")
    );
}

#[test]
fn reorder_conflict_leaves_head_and_index_unchanged() {
    let f = Fixture::new();
    f.write("file", "base\n");
    f.commit("Base");
    f.write("file", "middle\n");
    f.commit("Middle");
    f.write("file", "last\n");
    f.commit("Last");
    let head = git::resolve(&f.0, "HEAD").unwrap();
    let index = git(&f.0, &["write-tree"]).unwrap();
    let mut ids = branchroom::history::linear_tip(&f.0, 2).unwrap();
    ids.reverse();
    assert!(branchroom::history::run(&f.0, &branchroom::history::Action::Reorder(ids)).is_err());
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), head);
    assert_eq!(git(&f.0, &["write-tree"]).unwrap(), index);
    assert!(git(&f.0, &["status", "--porcelain"]).unwrap().is_empty());
}

#[test]
fn undo_root_commit_retains_working_files() {
    let f = Fixture::new();
    f.write("file", "data");
    f.commit("Root");
    branchroom::history::run(&f.0, &branchroom::history::Action::Undo).unwrap();
    assert!(git::resolve(&f.0, "HEAD").is_err());
    assert_eq!(fs::read_to_string(f.0.join("file")).unwrap(), "data");
    assert!(
        git(&f.0, &["status", "--porcelain"])
            .unwrap()
            .contains("?? file")
    );
}

#[test]
fn discarded_content_can_be_recovered_and_ignore_patterns_are_literal() {
    let f = Fixture::new();
    f.write("file.txt", "base\n");
    f.commit("Base");
    f.write("file.txt", "valuable\n");
    branchroom::recovery::discard(&f.0, "file.txt").unwrap();
    assert!(
        fs::read_to_string(f.0.join("file.txt"))
            .unwrap()
            .starts_with("base")
    );
    let entries = branchroom::recovery::entries(&f.0);
    let entry = entries.first().unwrap();
    branchroom::recovery::restore(&f.0, &entry.id).unwrap();
    assert_eq!(
        fs::read_to_string(f.0.join("file.txt")).unwrap(),
        "valuable\n"
    );
    f.write("literal[1].txt", "untracked");
    branchroom::recovery::ignore(&f.0, "literal[1].txt").unwrap();
    let ignored = git(&f.0, &["check-ignore", "literal[1].txt"]);
    assert!(
        ignored.is_ok(),
        "{ignored:?} pattern={:?}",
        fs::read_to_string(f.0.join(".gitignore"))
    );
    assert!(git(&f.0, &["check-ignore", "literal1.txt"]).is_err());
    let base = fs::canonicalize(branchroom::recovery::directory()).unwrap();
    let owned = fs::canonicalize(base.join(&entry.id)).unwrap();
    assert_eq!(owned.parent(), Some(base.as_path()));
    assert!(entry.id.bytes().all(|b| b.is_ascii_hexdigit()));
    fs::remove_dir_all(owned).unwrap();
}

#[test]
fn protected_read_policy_rejects_write_capable_flags() {
    use branchroom::process::strings;
    use branchroom::safety::{read_only_gh, read_only_git};
    assert!(read_only_git(&strings(&["status", "--porcelain=v1", "-z"])));
    assert!(read_only_git(&strings(&["show", "HEAD"])));
    for args in [
        vec!["diff", "--output=changed.txt"],
        vec!["config", "--get", "user.name", "--unset"],
        vec!["branch", "--list", "-D", "main"],
        vec!["fetch"],
        vec!["stash", "push"],
        vec!["diff", "--ext-diff"],
    ] {
        assert!(!read_only_git(&strings(&args)), "{args:?}");
    }
    assert!(read_only_gh(&strings(&["pr", "list"])));
    assert!(!read_only_gh(&strings(&[
        "pr", "create", "--title", "Example"
    ])));
}

#[test]
fn invalid_refs_cannot_be_promoted() {
    let f = Fixture::new();
    f.write("file", "data");
    f.commit("Initial");
    assert!(snapshot::promote(&f.0, "HEAD", "--force").is_err());
    assert!(git::resolve(&f.0, "--help").is_err());
}

#[test]
fn stash_keeps_untracked_and_can_be_applied() {
    let f = Fixture::new();
    f.write("file", "base");
    f.commit("Initial");
    f.write("file", "edited");
    f.write("untracked file", "new");
    git(&f.0, &["stash", "push", "-u", "-m", "Test stash"]).unwrap();
    let state = git::load(&f.0, "main", 100).unwrap();
    assert_eq!(state.stashes.len(), 1);
    assert!(
        state.changes.is_empty(),
        "Remaining changes: {:?}",
        state.changes
    );
    git(&f.0, &["stash", "apply", "stash@{0}"]).unwrap();
    assert_eq!(
        fs::read_to_string(f.0.join("untracked file")).unwrap(),
        "new"
    );
}

#[test]
fn history_hides_internal_checkpoints_but_keeps_named_snapshots() {
    let f = Fixture::new();
    f.write("file", "base\n");
    f.commit("Base");
    let head = git::resolve(&f.0, "HEAD").unwrap();
    let tree = git(&f.0, &["rev-parse", "HEAD^{tree}"]).unwrap();
    let checkpoint = git(
        &f.0,
        &[
            "commit-tree",
            tree.trim(),
            "-p",
            &head,
            "-m",
            "Internal checkpoint",
        ],
    )
    .unwrap();
    git(
        &f.0,
        &[
            "update-ref",
            "refs/agents/checkpoints/test",
            checkpoint.trim(),
        ],
    )
    .unwrap();
    snapshot::create(&f.0, "Named experiment").unwrap();
    let repo = git::load(&f.0, "main", 100).unwrap();
    assert!(
        repo.commits
            .iter()
            .all(|c| c.subject != "Internal checkpoint")
    );
    assert!(
        repo.commits
            .iter()
            .any(|c| c.subject.contains("Named experiment"))
    );
}

#[test]
fn partial_patch_handles_separated_hunks_and_missing_final_newline() {
    use std::collections::BTreeSet;
    let f = Fixture::new();
    git(&f.0, &["config", "core.autocrlf", "false"]).unwrap();
    let original = (0..40)
        .map(|n| format!("line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    f.write("file", &original);
    f.commit("Base");
    let edited = original
        .replace("line 2\n", "line two\nextra\n")
        .replace("line 35\n", "line thirty-five\n");
    f.write("file", &edited);
    let patch = branchroom::diff::file_patch(&f.0, "file", false, false, false).unwrap();
    assert_eq!(patch.hunks.len(), 2);
    let selected = patch
        .selection(&BTreeSet::from([1]), &BTreeSet::new())
        .unwrap();
    branchroom::diff::apply_to_index(&f.0, &selected, false).unwrap();
    assert_eq!(
        git(&f.0, &["show", ":file"]).unwrap(),
        original.replace("line 35\n", "line thirty-five\n")
    );
    assert_eq!(fs::read_to_string(f.0.join("file")).unwrap(), edited);
}

#[test]
fn selecting_one_pair_in_a_full_replacement_keeps_its_original_position() {
    use std::collections::BTreeSet;
    let f = Fixture::new();
    f.write("file", "one\ntwo\nthree\n");
    f.commit("Base");
    f.write("file", "ONE\nTWO\nTHREE\n");
    let patch = branchroom::diff::file_patch(&f.0, "file", false, false, false).unwrap();
    let lines = BTreeSet::from([(0, 0), (0, 3)]);
    let selected = patch.selection(&BTreeSet::new(), &lines).unwrap();
    branchroom::diff::apply_to_index(&f.0, &selected, false).unwrap();
    assert_eq!(git(&f.0, &["show", ":file"]).unwrap(), "ONE\ntwo\nthree\n");
    let staged = branchroom::diff::file_patch(&f.0, "file", true, false, false).unwrap();
    let reverse = staged
        .unstaging_selection(&BTreeSet::from([0]), &BTreeSet::new())
        .unwrap();
    branchroom::diff::apply_to_index(&f.0, &reverse, false).unwrap();
    assert_eq!(git(&f.0, &["show", ":file"]).unwrap(), "one\ntwo\nthree\n");
    assert_eq!(
        fs::read_to_string(f.0.join("file")).unwrap(),
        "ONE\nTWO\nTHREE\n"
    );
}

#[test]
fn revert_creates_inverse_commit_and_preserves_later_work() {
    let f = Fixture::new();
    f.write("base.txt", "base\n");
    f.commit("Base");
    f.write("feature.txt", "feature\n");
    f.commit("Feature");
    let feature = git::resolve(&f.0, "HEAD").unwrap();
    f.write("later.txt", "later\n");
    f.commit("Later");
    let before = git::resolve(&f.0, "HEAD").unwrap();
    branchroom::collaboration::revert(&f.0, &feature, None).unwrap();
    assert_eq!(git::resolve(&f.0, "HEAD^").unwrap(), before);
    assert!(!f.0.join("feature.txt").exists());
    assert_eq!(
        fs::read_to_string(f.0.join("later.txt")).unwrap(),
        "later\n"
    );
}

#[test]
fn revert_refuses_dirty_checkout_and_unrelated_commit() {
    let f = Fixture::new();
    f.write("file.txt", "base\n");
    f.commit("Base");
    git(&f.0, &["switch", "-c", "feature"]).unwrap();
    f.write("feature.txt", "feature\n");
    f.commit("Feature");
    let feature = git::resolve(&f.0, "HEAD").unwrap();
    git(&f.0, &["switch", "main"]).unwrap();
    let head = git::resolve(&f.0, "HEAD").unwrap();
    assert!(branchroom::collaboration::revert(&f.0, &feature, None).is_err());
    f.write("dirty.txt", "keep me\n");
    assert!(branchroom::collaboration::revert(&f.0, &head, None).is_err());
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), head);
    assert_eq!(
        fs::read_to_string(f.0.join("dirty.txt")).unwrap(),
        "keep me\n"
    );
}

#[test]
fn merge_revert_requires_valid_parent_and_keeps_mainline() {
    let f = Fixture::new();
    f.write("base.txt", "base\n");
    f.commit("Base");
    git(&f.0, &["switch", "-c", "feature"]).unwrap();
    f.write("feature.txt", "feature\n");
    f.commit("Feature");
    git(&f.0, &["switch", "main"]).unwrap();
    f.write("main.txt", "main\n");
    f.commit("Main");
    git(&f.0, &["merge", "--no-ff", "--no-edit", "feature"]).unwrap();
    let merge = git::resolve(&f.0, "HEAD").unwrap();
    assert!(branchroom::collaboration::revert(&f.0, &merge, None).is_err());
    assert!(branchroom::collaboration::revert(&f.0, &merge, Some(3)).is_err());
    branchroom::collaboration::revert(&f.0, &merge, Some(1)).unwrap();
    assert!(!f.0.join("feature.txt").exists());
    assert!(f.0.join("main.txt").exists());
    assert_eq!(git::resolve(&f.0, "HEAD^").unwrap(), merge);
}

#[test]
fn selected_branch_update_fetches_remote_then_merges_without_pushing() {
    let origin = Fixture::new();
    origin.write("base.txt", "base\n");
    origin.commit("Base");
    let local = Fixture::new();
    git(
        &local.0,
        &["remote", "add", "origin", origin.0.to_str().unwrap()],
    )
    .unwrap();
    git(&local.0, &["fetch", "origin"]).unwrap();
    git(&local.0, &["switch", "-c", "feature", "origin/main"]).unwrap();
    local.write("feature.txt", "local\n");
    local.commit("Feature");
    let tip = git::resolve(&local.0, "HEAD").unwrap();
    git(&local.0, &["switch", "-c", "other"]).unwrap();
    origin.write("new.txt", "remote\n");
    origin.commit("New baseline");
    let origin_head = git::resolve(&origin.0, "HEAD").unwrap();
    branchroom::collaboration::update_branch(&local.0, "feature", "origin/main").unwrap();
    assert_eq!(
        git(&local.0, &["branch", "--show-current"]).unwrap().trim(),
        "feature"
    );
    assert!(local.0.join("feature.txt").exists() && local.0.join("new.txt").exists());
    git(&local.0, &["merge-base", "--is-ancestor", &tip, "HEAD"]).unwrap();
    assert_eq!(git::resolve(&origin.0, "HEAD").unwrap(), origin_head);
    local.write("dirty.txt", "keep\n");
    assert!(branchroom::collaboration::update_branch(&local.0, "other", "origin/main").is_err());
    assert_eq!(
        git(&local.0, &["branch", "--show-current"]).unwrap().trim(),
        "feature"
    );
}

#[test]
fn remote_status_distinguishes_tracking_from_local_only() {
    use branchroom::{collaboration::branch_status, git::Branch};
    let local = Branch {
        name: "feature".into(),
        id: "abc".into(),
        ..Default::default()
    };
    assert!(branch_status(&local, &[]).starts_with("Local · no upstream"));
    let remote = Branch {
        name: "origin/feature".into(),
        id: "abc".into(),
        remote: true,
        ..Default::default()
    };
    assert!(branch_status(&local, std::slice::from_ref(&remote)).contains("no upstream"));
    assert!(
        branch_status(&remote, std::slice::from_ref(&local)).contains("no local branch tracks it")
    );
    let published = Branch {
        upstream: remote.name.clone(),
        tracking: "[ahead 2]".into(),
        ..local
    };
    assert!(branch_status(&published, std::slice::from_ref(&remote)).contains("ahead 2"));
    assert!(branch_status(&remote, &[published]).contains("tracked on this PC"));
}

#[test]
fn remote_switch_creates_tracking_branch_then_reuses_it() {
    let origin = Fixture::new();
    origin.write("base.txt", "base\n");
    origin.commit("Base");
    git(&origin.0, &["branch", "feature"]).unwrap();
    let local = Fixture::new();
    git(
        &local.0,
        &["remote", "add", "origin", origin.0.to_str().unwrap()],
    )
    .unwrap();
    git(&local.0, &["fetch", "origin"]).unwrap();
    branchroom::collaboration::switch_branch(&local.0, "origin/main", false).unwrap();
    branchroom::collaboration::switch_branch(&local.0, "origin/feature", false).unwrap();
    assert_eq!(
        git(&local.0, &["branch", "--show-current"]).unwrap().trim(),
        "feature"
    );
    branchroom::collaboration::switch_branch(&local.0, "origin/main", false).unwrap();
    branchroom::collaboration::switch_branch(&local.0, "origin/feature", false).unwrap();
    assert_eq!(
        git(&local.0, &["branch", "--show-current"]).unwrap().trim(),
        "feature"
    );
}

#[test]
fn revert_conflict_can_be_aborted_without_losing_commits() {
    let f = Fixture::new();
    f.write("file.txt", "base\n");
    f.commit("Base");
    f.write("file.txt", "feature\n");
    f.commit("Feature");
    let feature = git::resolve(&f.0, "HEAD").unwrap();
    f.write("file.txt", "later\n");
    f.commit("Later");
    let before = git::resolve(&f.0, "HEAD").unwrap();
    assert!(branchroom::collaboration::revert(&f.0, &feature, None).is_err());
    assert!(
        !git(&f.0, &["diff", "--name-only", "--diff-filter=U"])
            .unwrap()
            .is_empty()
    );
    git(&f.0, &["revert", "--abort"]).unwrap();
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), before);
    assert_eq!(fs::read_to_string(f.0.join("file.txt")).unwrap(), "later\n");
}

#[test]
fn current_branch_history_has_its_own_limit_and_excludes_unmerged_refs() {
    let f = Fixture::new();
    f.write("main.txt", "main\n");
    f.commit("Main baseline");
    let main = git::resolve(&f.0, "HEAD").unwrap();
    git(&f.0, &["switch", "-c", "unmerged-pr"]).unwrap();
    for i in 0..4 {
        f.write("other.txt", &format!("{i}\n"));
        f.commit(&format!("Unmerged PR {i}"));
    }
    let unmerged = git::resolve(&f.0, "HEAD").unwrap();
    git(&f.0, &["switch", "main"]).unwrap();
    snapshot::create(&f.0, "Separate experiment").unwrap();
    let repo = git::load(&f.0, "main", 1).unwrap();
    assert_eq!(repo.branch_commits.len(), 1);
    assert_eq!(repo.branch_commits[0].id, main);
    assert!(
        repo.branch_commits
            .iter()
            .all(|c| c.id != unmerged && c.subject != "Separate experiment")
    );
    assert_ne!(repo.commits[0].id, main);
    git(&f.0, &["switch", "unmerged-pr"]).unwrap();
    let repo = git::load(&f.0, "main", 2).unwrap();
    assert_eq!(repo.branch_commits.len(), 2);
    assert_eq!(repo.branch_commits[0].id, unmerged);
}

#[test]
fn current_branch_history_includes_merged_commits_and_handles_detached_or_unborn_head() {
    let f = Fixture::new();
    assert!(
        git::load(&f.0, "main", 20)
            .unwrap()
            .branch_commits
            .is_empty()
    );
    f.write("base.txt", "base\n");
    f.commit("Base");
    git(&f.0, &["switch", "-c", "feature"]).unwrap();
    f.write("feature.txt", "feature\n");
    f.commit("Feature");
    let feature = git::resolve(&f.0, "HEAD").unwrap();
    git(&f.0, &["switch", "main"]).unwrap();
    git(&f.0, &["merge", "--no-ff", "--no-edit", "feature"]).unwrap();
    let repo = git::load(&f.0, "main", 20).unwrap();
    assert_eq!(repo.branch_commits.len(), 3);
    assert!(repo.branch_commits.iter().any(|c| c.id == feature));
    git(&f.0, &["switch", "--detach", &feature]).unwrap();
    let repo = git::load(&f.0, "main", 20).unwrap();
    assert_eq!(repo.branch_commits.len(), 2);
    assert_eq!(repo.branch_commits[0].id, feature);
}

#[test]
fn removing_and_restoring_snapshot_preserves_branches_index_and_open_worktree() {
    let f = Fixture::new();
    f.write("file.txt", "base\n");
    f.commit("Base");
    f.write("file.txt", "snapshot\n");
    snapshot::create(&f.0, "Experiment").unwrap();
    let saved = git::load(&f.0, "main", 20).unwrap().snapshots.remove(0);
    let directory = f.0.join("review-copy");
    snapshot::materialize(&f.0, &saved.reference, &directory).unwrap();
    f.write("file.txt", "working\n");
    git(&f.0, &["add", "file.txt"]).unwrap();
    f.write("file.txt", "unstaged\n");
    let head = git::resolve(&f.0, "HEAD").unwrap();
    let index = git(&f.0, &["write-tree"]).unwrap();
    let before = git(&f.0, &["status", "--porcelain=v1", "-z"]).unwrap();
    let recovery = snapshot::remove(&f.0, &saved.reference).unwrap();
    let repo = git::load(&f.0, "main", 20).unwrap();
    assert!(repo.snapshots.is_empty());
    assert_eq!(repo.removed_snapshots.len(), 1);
    assert_eq!(repo.removed_snapshots[0].id, saved.id);
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), head);
    assert_eq!(git(&f.0, &["write-tree"]).unwrap(), index);
    assert_eq!(
        git(&f.0, &["status", "--porcelain=v1", "-z"]).unwrap(),
        before
    );
    assert_eq!(
        fs::read_to_string(directory.join("file.txt")).unwrap(),
        "snapshot\n"
    );
    let restored = snapshot::restore(&f.0, &recovery).unwrap();
    let repo = git::load(&f.0, "main", 20).unwrap();
    assert_eq!(repo.snapshots[0].reference, restored);
    assert_eq!(repo.snapshots[0].id, saved.id);
    assert_eq!(repo.snapshots[0].label, "Experiment");
    assert!(repo.removed_snapshots.is_empty());
    assert_eq!(
        git(&f.0, &["status", "--porcelain=v1", "-z"]).unwrap(),
        before
    );
    git(&f.0, &["worktree", "remove", directory.to_str().unwrap()]).unwrap();
}

#[test]
fn snapshot_removal_rejects_other_refs_symbolic_refs_and_stale_selection() {
    let f = Fixture::new();
    f.write("file.txt", "base\n");
    f.commit("Base");
    let before = git::resolve(&f.0, "HEAD").unwrap();
    for reference in [
        "HEAD",
        "refs/heads/main",
        "refs/branchroom/snapshots/",
        "refs/branchroom/snapshots/a\ndelete refs/heads/main",
    ] {
        assert!(snapshot::remove(&f.0, reference).is_err());
    }
    git(
        &f.0,
        &[
            "symbolic-ref",
            "refs/branchroom/snapshots/link",
            "refs/heads/main",
        ],
    )
    .unwrap();
    assert!(snapshot::remove(&f.0, "refs/branchroom/snapshots/link").is_err());
    git(
        &f.0,
        &["symbolic-ref", "--delete", "refs/branchroom/snapshots/link"],
    )
    .unwrap();
    snapshot::create(&f.0, "Experiment").unwrap();
    let saved = git::load(&f.0, "main", 20).unwrap().snapshots.remove(0);
    let recovery = snapshot::remove(&f.0, &saved.reference).unwrap();
    assert!(snapshot::remove(&f.0, &saved.reference).is_err());
    assert!(snapshot::restore(&f.0, "refs/heads/main").is_err());
    snapshot::restore(&f.0, &recovery).unwrap();
    assert!(snapshot::restore(&f.0, &recovery).is_err());
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), before);
}

#[test]
fn merge_requires_reviewed_destination_and_clean_worktree() {
    let f = Fixture::new();
    f.write("base.txt", "base\n");
    f.commit("Base");
    git(&f.0, &["switch", "-c", "feature"]).unwrap();
    f.write("new.txt", "feature\n");
    f.commit("Feature");
    let feature = git::resolve(&f.0, "HEAD").unwrap();
    git(&f.0, &["switch", "main"]).unwrap();
    let before = git::resolve(&f.0, "HEAD").unwrap();
    assert!(branchroom::collaboration::merge(&f.0, "feature", "feature").is_err());
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), before);
    f.write("dirty.txt", "keep\n");
    assert!(branchroom::collaboration::merge(&f.0, "main", "feature").is_err());
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), before);
    fs::remove_file(f.0.join("dirty.txt")).unwrap();
    branchroom::collaboration::merge(&f.0, "main", "feature").unwrap();
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), feature);
    assert!(f.0.join("new.txt").exists());
}

#[test]
fn commit_lock_unstages_file_keeps_edits_and_is_repository_local() {
    let f = Fixture::new();
    let other = Fixture::new();
    f.write("config.txt", "base\n");
    f.write("code.txt", "base\n");
    f.commit("Base");
    f.write("config.txt", "staged local setting\n");
    git(&f.0, &["add", "--", "config.txt"]).unwrap();
    f.write("config.txt", "working local setting\n");
    f.write("code.txt", "changed code\n");
    branchroom::commit_lock::set(&f.0, "config.txt", true).unwrap();
    assert_eq!(
        fs::read_to_string(f.0.join("config.txt")).unwrap(),
        "working local setting\n"
    );
    assert_eq!(git(&f.0, &["show", ":config.txt"]).unwrap(), "base\n");
    assert_eq!(
        branchroom::commit_lock::read(&f.0).unwrap(),
        vec!["config.txt"]
    );
    assert!(branchroom::commit_lock::read(&other.0).unwrap().is_empty());
    branchroom::commit_lock::stage_unlocked(&f.0).unwrap();
    assert!(
        git(&f.0, &["diff", "--cached", "--name-only"])
            .unwrap()
            .contains("code.txt")
    );
    git(&f.0, &["commit", "-m", "Code only"]).unwrap();
    assert_eq!(git(&f.0, &["show", "HEAD:config.txt"]).unwrap(), "base\n");
    git(&f.0, &["switch", "-c", "another"]).unwrap();
    assert_eq!(
        git::load(&f.0, "main", 10).unwrap().commit_locks,
        vec!["config.txt"]
    );
    branchroom::commit_lock::set(&f.0, "config.txt", false).unwrap();
    git(&f.0, &["add", "--", "config.txt"]).unwrap();
    git(&f.0, &["commit", "-m", "Include unlocked setting"]).unwrap();
    assert_eq!(
        git(&f.0, &["show", "HEAD:config.txt"]).unwrap(),
        "working local setting\n"
    );
}

#[test]
fn commit_guard_blocks_external_staging_and_auto_commit_options() {
    let f = Fixture::new();
    f.write("config.txt", "base\n");
    f.commit("Base");
    branchroom::commit_lock::set(&f.0, "config.txt", true).unwrap();
    f.write("config.txt", "local setting\n");
    let before = git::resolve(&f.0, "HEAD").unwrap();
    assert!(git(&f.0, &["add", "--", "config.txt"]).is_err());
    assert!(git(&f.0, &["commit", "-am", "Automatic commit"]).is_err());
    assert!(
        git(
            &f.0,
            &["commit", "--only", "config.txt", "-m", "Path commit"]
        )
        .is_err()
    );
    // Emulate staging in another Git client, which does not know Branchroom's locks.
    assert!(
        branchroom::process::command("git", &f.0)
            .args(["add", "--", "config.txt"])
            .status()
            .unwrap()
            .success()
    );
    assert!(git(&f.0, &["commit", "-m", "Blocked locked file"]).is_err());
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), before);
    branchroom::commit_lock::set(&f.0, "config.txt", true).unwrap();
    assert!(
        git(&f.0, &["diff", "--cached", "--name-only"])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn commit_locks_handle_unborn_files_nested_untracked_paths_and_literal_names() {
    let f = Fixture::new();
    fs::create_dir(f.0.join("config")).unwrap();
    f.write("config/local.txt", "local staged\n");
    git(&f.0, &["add", "--", "config/local.txt"]).unwrap();
    f.write("config/local.txt", "local unstaged\n");
    branchroom::commit_lock::set(&f.0, "config/local.txt", true).unwrap();
    f.write("config/shared.txt", "shared\n");
    f.write("-file [literal].txt", "literal\n");
    assert!(git(&f.0, &["add", "--", "config/"]).is_err());
    branchroom::commit_lock::stage_unlocked(&f.0).unwrap();
    let staged = git(&f.0, &["diff", "--cached", "--name-only", "-z"]).unwrap();
    assert!(!staged.contains("local.txt"));
    assert!(staged.contains("config/shared.txt"));
    assert!(staged.contains("-file [literal].txt"));
    git(&f.0, &["commit", "-m", "Shared files only"]).unwrap();
    assert_eq!(
        fs::read_to_string(f.0.join("config/local.txt")).unwrap(),
        "local unstaged\n"
    );
    assert!(
        git(
            &f.0,
            &["ls-tree", "--name-only", "HEAD", "--", "config/local.txt"]
        )
        .unwrap()
        .is_empty()
    );
    for path in ["../outside", ".git/config", "/absolute"] {
        assert!(branchroom::commit_lock::set(&f.0, path, true).is_err());
    }
}

#[test]
fn identifiers_remain_unique_under_parallel_generation() {
    let values = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                let ids: Vec<_> = (0..1000).map(|_| snapshot::unique_id()).collect();
                values.lock().unwrap().extend(ids);
            });
        }
    });
    let values = values.into_inner().unwrap();
    let unique: std::collections::HashSet<_> = values.iter().collect();
    assert_eq!(unique.len(), 8000);
}

#[test]
fn commit_plan_refuses_index_changes_after_review() {
    let f = Fixture::new();
    f.write("file.txt", "base\n");
    f.commit("Base");
    f.write("file.txt", "reviewed\n");
    git(&f.0, &["add", "--", "file.txt"]).unwrap();
    let plan = branchroom::commit::prepare(&f.0, "Reviewed commit", false).unwrap();
    let before = git::resolve(&f.0, "HEAD").unwrap();
    f.write("file.txt", "not reviewed\n");
    git(&f.0, &["add", "--", "file.txt"]).unwrap();
    assert!(branchroom::commit::execute(&f.0, &plan).is_err());
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), before);
    assert_eq!(git(&f.0, &["show", ":file.txt"]).unwrap(), "not reviewed\n");
}

#[test]
fn commit_plan_handles_unborn_head_and_refuses_stale_head() {
    let f = Fixture::new();
    f.write("file.txt", "initial\n");
    git(&f.0, &["add", "--", "file.txt"]).unwrap();
    let plan = branchroom::commit::prepare(&f.0, "Initial\n\nUnicode: café", false).unwrap();
    branchroom::commit::execute(&f.0, &plan).unwrap();
    assert!(
        git(&f.0, &["show", "-s", "--format=%B", "HEAD"])
            .unwrap()
            .contains("Unicode: café")
    );
    f.write("file.txt", "reviewed\n");
    git(&f.0, &["add", "--", "file.txt"]).unwrap();
    let plan = branchroom::commit::prepare(&f.0, "Next", false).unwrap();
    git(&f.0, &["commit", "-m", "External client"]).unwrap();
    let before = git::resolve(&f.0, "HEAD").unwrap();
    assert!(branchroom::commit::execute(&f.0, &plan).is_err());
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), before);
}

#[test]
fn snapshot_capture_obeys_commit_locks_without_changing_the_index() {
    let f = Fixture::new();
    f.write("file.txt", "base\n");
    f.commit("Base");
    branchroom::commit_lock::set(&f.0, "file.txt", true).unwrap();
    f.write("file.txt", "local experiment\n");
    snapshot::create(&f.0, "Independent capture").unwrap();
    let repo = git::load(&f.0, "main", 10).unwrap();
    assert_eq!(
        git(
            &f.0,
            &["show", &format!("{}:file.txt", repo.snapshots[0].id)]
        )
        .unwrap(),
        "local experiment\n"
    );
    assert!(
        git(&f.0, &["diff", "--cached", "--name-only"])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn preferences_roundtrip_drafts_and_preserve_corrupt_source() {
    let f = Fixture::new();
    let path = f.0.join("preferences.json");
    let mut settings = branchroom::settings::Settings::default();
    settings
        .projects
        .entry(f.0.clone())
        .or_default()
        .drafts
        .insert(
            "feature".into(),
            branchroom::settings::Draft {
                summary: "Keep this draft".into(),
                pr_body: "Unsent description".into(),
                pr_draft: true,
                ..Default::default()
            },
        );
    settings.history_columns.commit = 620.;
    settings.history_columns.graph = 240.;
    settings.snapshot_source_width = 275.;
    settings.branch_source_width = 310.;
    settings.save_to(&path).unwrap();
    settings.save_to(&path).unwrap();
    let loaded = branchroom::settings::Settings::load_from(&path).unwrap();
    assert_eq!(loaded.history_columns.commit, 620.);
    assert_eq!(loaded.history_columns.graph, 240.);
    assert_eq!(loaded.snapshot_source_width, 275.);
    assert_eq!(loaded.branch_source_width, 310.);
    assert_eq!(
        loaded.projects[&f.0].drafts["feature"].summary,
        "Keep this draft"
    );
    fs::write(&path, b"{broken preferences").unwrap();
    assert!(settings.save_to(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"{broken preferences");
}

#[test]
fn conflict_resolution_refuses_stale_content_and_diff3_markers() {
    let f = Fixture::new();
    f.write("file.txt", "base\n");
    f.commit("Base");
    f.write("file.txt", "external edit\n");
    assert!(branchroom::conflict::save(&f.0, "file.txt", "resolved\n", "base\n").is_err());
    assert!(
        branchroom::conflict::save(&f.0, "file.txt", "||||||| ancestor\n", "external edit\n")
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(f.0.join("file.txt")).unwrap(),
        "external edit\n"
    );
    branchroom::conflict::save(&f.0, "file.txt", "resolved\n", "external edit\n").unwrap();
    assert_eq!(git(&f.0, &["show", ":file.txt"]).unwrap(), "resolved\n");
}

#[test]
fn atomic_resolution_does_not_modify_other_hard_links() {
    let f = Fixture::new();
    let outside = Fixture::new();
    outside.write("original.txt", "original\n");
    fs::hard_link(outside.0.join("original.txt"), f.0.join("linked.txt")).unwrap();
    branchroom::conflict::save(&f.0, "linked.txt", "resolved\n", "original\n").unwrap();
    assert_eq!(
        fs::read_to_string(outside.0.join("original.txt")).unwrap(),
        "original\n"
    );
    assert_eq!(
        fs::read_to_string(f.0.join("linked.txt")).unwrap(),
        "resolved\n"
    );
}

#[test]
fn restoring_a_discard_keeps_a_recovery_copy_of_newer_edits() {
    let f = Fixture::new();
    f.write("file.txt", "base\n");
    f.commit("Base");
    f.write("file.txt", "discarded\n");
    branchroom::recovery::discard(&f.0, "file.txt").unwrap();
    let first = branchroom::recovery::entries(&f.0).remove(0);
    f.write("file.txt", "newer local edit\n");
    branchroom::recovery::restore(&f.0, &first.id).unwrap();
    assert_eq!(
        fs::read_to_string(f.0.join("file.txt")).unwrap(),
        "discarded\n"
    );
    let newer = branchroom::recovery::entries(&f.0)
        .into_iter()
        .find(|e| e.id != first.id)
        .unwrap();
    branchroom::recovery::restore(&f.0, &newer.id).unwrap();
    assert_eq!(
        fs::read_to_string(f.0.join("file.txt")).unwrap(),
        "newer local edit\n"
    );
}

#[test]
fn branch_statistics_match_git_and_recompute_when_baseline_moves() {
    let f = Fixture::new();
    f.write("file.txt", "base\n");
    f.commit("Base");
    git(&f.0, &["switch", "-c", "feature"]).unwrap();
    f.write("feature.txt", "feature\n");
    f.commit("Feature");
    git(&f.0, &["switch", "main"]).unwrap();
    f.write("main.txt", "main\n");
    f.commit("Main");
    for step in 0..2 {
        let repo = git::load(&f.0, "main", 30).unwrap();
        for branch in &repo.branches {
            let actual = git(
                &f.0,
                &[
                    "rev-list",
                    "--left-right",
                    "--count",
                    &format!("main...{}", branch.name),
                ],
            )
            .unwrap();
            let counts: Vec<usize> = actual
                .split_whitespace()
                .map(|n| n.parse().unwrap())
                .collect();
            assert_eq!((branch.behind, branch.ahead), (counts[0], counts[1]));
            let bases = git(&f.0, &["merge-base", "--all", "main", &branch.name]).unwrap();
            assert!(bases.lines().any(|id| id == branch.fork_point));
        }
        f.write("next.txt", &format!("next {step}\n"));
        f.commit("Move baseline");
    }
}

#[test]
fn local_upstreams_are_not_reported_as_published() {
    let branch = git::Branch {
        name: "feature".into(),
        upstream: "main".into(),
        ..Default::default()
    };
    let status = branchroom::collaboration::branch_status(&branch, &[]);
    assert!(status.contains("on this PC"));
    assert!(!status.contains("Published"));
}

#[test]
fn rewrite_refuses_pending_operations_even_when_files_are_clean() {
    let f = Fixture::new();
    f.write("file.txt", "base\n");
    f.commit("Base");
    let head = git::resolve(&f.0, "HEAD").unwrap();
    fs::write(f.0.join(".git/MERGE_HEAD"), format!("{head}\n")).unwrap();
    assert!(branchroom::history::run(&f.0, &branchroom::history::Action::Undo).is_err());
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), head);
}

#[test]
fn github_action_arguments_pin_merge_head_and_validate_review() {
    use branchroom::github::{Action, MergeMethod, Review};
    let head = "a".repeat(40);
    let action = Action::Merge {
        number: 42,
        head: head.clone(),
        method: MergeMethod::Squash,
    };
    let args = action.arguments().unwrap();
    assert!(args.windows(2).any(|w| w == ["--match-head-commit", &head]));
    assert!(
        !args
            .iter()
            .any(|a| a == "--admin" || a == "--delete-branch")
    );
    assert!(
        Action::Review {
            number: 42,
            kind: Review::RequestChanges,
            body: String::new()
        }
        .arguments()
        .is_err()
    );
    assert!(
        Action::Review {
            number: 42,
            kind: Review::Approve,
            body: String::new()
        }
        .arguments()
        .is_ok()
    );
}

#[test]
fn pull_request_composer_uses_stdin_and_explicit_branches() {
    let request = branchroom::github::CreateRequest {
        base: "main".into(),
        head: "feature".into(),
        head_oid: "a".repeat(40),
        title: "Literal $(text)".into(),
        body: "Large description\n".repeat(4000),
        draft: true,
        publish_if_needed: false,
        publish_remote: "origin".into(),
        reviewers: vec!["reviewer".into()],
        labels: vec!["bug".into()],
    };
    let args = request.arguments().unwrap();
    assert!(args.windows(2).any(|w| w == ["--body-file", "-"]));
    assert!(!args.iter().any(|a| a.contains("Large description")));
    assert!(args.windows(2).any(|w| w == ["--head", "feature"]));
}

#[test]
fn image_comparison_preserves_binary_content_and_separates_index_from_worktree() {
    let f = Fixture::new();
    let save = |color: [u8; 4]| {
        image::RgbaImage::from_pixel(3, 2, image::Rgba(color))
            .save(f.0.join("image.png"))
            .unwrap();
    };
    save([255, 0, 0, 255]);
    f.commit("Red");
    save([0, 255, 0, 255]);
    git(&f.0, &["add", "--", "image.png"]).unwrap();
    save([0, 0, 255, 255]);
    let working = branchroom::image_diff::compare(&f.0, "image.png", false).unwrap();
    assert_eq!(&working.before.unwrap().rgba[..4], &[0, 255, 0, 255]);
    assert_eq!(&working.after.unwrap().rgba[..4], &[0, 0, 255, 255]);
    let staged = branchroom::image_diff::compare(&f.0, "image.png", true).unwrap();
    assert_eq!(&staged.before.unwrap().rgba[..4], &[255, 0, 0, 255]);
    assert_eq!(&staged.after.unwrap().rgba[..4], &[0, 255, 0, 255]);
}

#[test]
fn image_preview_handles_added_deleted_and_invalid_files() {
    let f = Fixture::new();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([255u8, 0, 0, 255]))
        .save(f.0.join("new.png"))
        .unwrap();
    let added = branchroom::image_diff::compare(&f.0, "new.png", false).unwrap();
    assert!(added.before.is_none());
    assert!(added.after.is_some());
    f.commit("Image");
    fs::remove_file(f.0.join("new.png")).unwrap();
    let deleted = branchroom::image_diff::compare(&f.0, "new.png", false).unwrap();
    assert!(deleted.before.is_some());
    assert!(deleted.after.is_none());
    assert!(branchroom::image_diff::decode(b"not an image".to_vec()).is_err());
    assert!(branchroom::image_diff::decode(vec![0; 16 * 1024 * 1024 + 1]).is_err());
}

#[test]
fn piped_commands_can_be_canceled_while_stdin_is_blocked() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let f = Fixture::new();
    git(&f.0, &["config", "alias.branchroom-wait", "!sleep 30"]).unwrap();
    let token = Arc::new(AtomicBool::new(false));
    branchroom::process::set_cancellation(token.clone());
    let cancel = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(500));
        token.store(true, Ordering::Relaxed);
    });
    let started = std::time::Instant::now();
    let result = branchroom::process::input(
        "git",
        &f.0,
        &branchroom::process::strings(&["branchroom-wait"]),
        &vec![b'x'; 1024 * 1024],
        &[],
    );
    cancel.join().unwrap();
    branchroom::process::reset_cancellation();
    assert!(result.unwrap_err().contains("canceled"));
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
}

#[test]
fn unrelated_branch_histories_have_no_fabricated_merge_base() {
    let f = Fixture::new();
    f.write("file.txt", "base\n");
    f.commit("Base");
    git(&f.0, &["switch", "--orphan", "independent"]).unwrap();
    f.write("other.txt", "independent\n");
    f.commit("Independent");
    let repo = git::load(&f.0, "main", 20).unwrap();
    let independent = repo
        .branches
        .iter()
        .find(|b| b.name == "independent")
        .unwrap();
    assert!(independent.fork_point.is_empty());
    assert!(independent.ahead > 0 && independent.behind > 0);
}

#[test]
fn commit_and_snapshot_metadata_survive_control_characters() {
    let f = Fixture::new();
    f.write("file.txt", "base\n");
    let message = format!("Subject {} separators {} preserved", '\u{001e}', '\u{001f}');
    f.commit(&message);
    snapshot::create(&f.0, "Snapshot\twith a tab").unwrap();
    let repo = git::load(&f.0, "main", 20).unwrap();
    assert_eq!(repo.branch_commits[0].subject, message);
    assert_eq!(repo.snapshots[0].label, "Snapshot\twith a tab");
}

#[test]
fn line_staging_respects_repository_file_locks() {
    let f = Fixture::new();
    f.write("file.txt", "base\n");
    f.commit("Base");
    branchroom::commit_lock::set(&f.0, "file.txt", true).unwrap();
    f.write("file.txt", "local\n");
    let patch = branchroom::diff::file_patch(&f.0, "file.txt", false, false, false).unwrap();
    let selection = patch
        .selection(&[0].into_iter().collect(), &Default::default())
        .unwrap();
    assert!(branchroom::diff::apply_to_index(&f.0, &selection, false).is_err());
    assert!(
        git(&f.0, &["diff", "--cached", "--name-only"])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn remote_head_symbolic_reference_is_not_shown_as_a_branch() {
    let f = Fixture::new();
    f.write("readme.txt", "example\n");
    f.commit("Initial commit");
    let head = git::resolve(&f.0, "HEAD").unwrap();
    git(&f.0, &["update-ref", "refs/remotes/origin/trunk", &head]).unwrap();
    git(
        &f.0,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/trunk",
        ],
    )
    .unwrap();
    let state = git::load(&f.0, "main", 50).unwrap();
    assert_eq!(state.default_branch.as_deref(), Some("trunk"));
    assert!(
        state
            .branches
            .iter()
            .any(|branch| branch.name == "origin/trunk")
    );
    assert!(
        !state.branches.iter().any(
            |branch| branch.remote && (branch.name == "origin" || branch.name == "origin/HEAD")
        )
    );
}

#[test]
fn sync_counts_use_the_tracked_remote_not_the_comparison_branch() {
    let f = Fixture::new();
    f.write("base.txt", "base");
    f.commit("Base");
    git(&f.0, &["switch", "-c", "feature"]).unwrap();
    f.write("feature.txt", "feature");
    f.commit("Feature");
    let feature = git::resolve(&f.0, "HEAD").unwrap();
    git(&f.0, &["remote", "add", "origin", "."]).unwrap();
    git(
        &f.0,
        &["update-ref", "refs/remotes/origin/feature", &feature],
    )
    .unwrap();
    git(&f.0, &["config", "branch.feature.remote", "origin"]).unwrap();
    git(
        &f.0,
        &["config", "branch.feature.merge", "refs/heads/feature"],
    )
    .unwrap();
    let repo = git::load(&f.0, "main", 30).unwrap();
    assert_eq!(repo.branches.iter().find(|b| b.current).unwrap().ahead, 1);
    assert_eq!(repo.sync_counts, Some((0, 0)));
    f.write("another.txt", "ahead");
    f.commit("Ahead of remote");
    assert_eq!(
        git::load(&f.0, "main", 30).unwrap().sync_counts,
        Some((1, 0))
    );
    git(&f.0, &["switch", "--detach", &feature]).unwrap();
    f.write("remote.txt", "remote");
    f.commit("Remote change");
    let remote_tip = git::resolve(&f.0, "HEAD").unwrap();
    git(
        &f.0,
        &["update-ref", "refs/remotes/origin/feature", &remote_tip],
    )
    .unwrap();
    git(&f.0, &["switch", "feature"]).unwrap();
    assert_eq!(
        git::load(&f.0, "main", 30).unwrap().sync_counts,
        Some((1, 1))
    );
}

#[test]
fn deleting_a_merged_branch_keeps_a_recovery_checkpoint_and_refuses_stale_tips() {
    let f = Fixture::new();
    f.write("notes.txt", "first");
    f.commit("First");
    git(&f.0, &["branch", "finished"]).unwrap();
    let tip = git::resolve(&f.0, "finished").unwrap();
    assert!(
        branchroom::collaboration::delete_merged_branch(&f.0, "finished", &"0".repeat(40)).is_err()
    );
    assert!(git::resolve(&f.0, "finished").is_ok());
    branchroom::collaboration::delete_merged_branch(&f.0, "finished", &tip).unwrap();
    assert!(git::resolve(&f.0, "finished").is_err());
    let state = git::load(&f.0, "main", 100).unwrap();
    assert!(state.recovery_heads.iter().any(|saved| saved.id == tip));
    assert!(branchroom::collaboration::delete_merged_branch(&f.0, "main", &tip).is_err());
}

#[test]
fn branch_cleanup_refuses_unmerged_commits_without_creating_a_checkpoint() {
    let f = Fixture::new();
    f.write("notes.txt", "base");
    f.commit("Base");
    git(&f.0, &["switch", "-c", "unfinished"]).unwrap();
    f.write("notes.txt", "unfinished");
    f.commit("Unfinished");
    let tip = git::resolve(&f.0, "HEAD").unwrap();
    git(&f.0, &["switch", "main"]).unwrap();
    assert!(branchroom::collaboration::delete_merged_branch(&f.0, "unfinished", &tip).is_err());
    assert_eq!(git::resolve(&f.0, "unfinished").unwrap(), tip);
    assert!(
        git::load(&f.0, "main", 100)
            .unwrap()
            .recovery_heads
            .is_empty()
    );
}

#[test]
fn graph_merge_forecast_lists_conflicts_without_changing_checkout_or_index() {
    use branchroom::operation::{self, Kind};
    let f = Fixture::new();
    f.write("same.txt", "base\n");
    f.commit("Base");
    git(&f.0, &["switch", "-c", "incoming"]).unwrap();
    f.write("same.txt", "incoming\n");
    f.commit("Incoming");
    git(&f.0, &["switch", "main"]).unwrap();
    f.write("same.txt", "current\n");
    f.commit("Current");
    let head = git::resolve(&f.0, "HEAD").unwrap();
    let index = git(&f.0, &["write-tree"]).unwrap();
    let review = operation::preview(
        &f.0,
        Kind::Merge {
            source: "incoming".into(),
            target: "main".into(),
        },
    )
    .unwrap();
    assert_eq!(review.conflicts, vec!["same.txt"]);
    assert!(review.blocked.is_none());
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), head);
    assert_eq!(git(&f.0, &["write-tree"]).unwrap(), index);
    assert_eq!(
        fs::read_to_string(f.0.join("same.txt")).unwrap(),
        "current\n"
    );
    assert!(git::load(&f.0, "main", 100).unwrap().snapshots.is_empty());
}
#[test]
fn reviewed_graph_merge_snapshots_first_and_refuses_a_changed_source() {
    use branchroom::operation::{self, Kind};
    let f = Fixture::new();
    f.write("base.txt", "base");
    f.commit("Base");
    git(&f.0, &["switch", "-c", "incoming"]).unwrap();
    f.write("incoming.txt", "new");
    f.commit("Incoming");
    git(&f.0, &["switch", "main"]).unwrap();
    let review = operation::preview(
        &f.0,
        Kind::Merge {
            source: "incoming".into(),
            target: "main".into(),
        },
    )
    .unwrap();
    assert!(review.conflicts.is_empty());
    assert!(review.introduced > 0);
    let old = git::resolve(&f.0, "HEAD").unwrap();
    operation::apply(&f.0, &review).unwrap();
    assert_eq!(fs::read_to_string(f.0.join("incoming.txt")).unwrap(), "new");
    let state = git::load(&f.0, "main", 100).unwrap();
    assert!(!state.snapshots.is_empty());
    assert!(state.recovery_heads.iter().any(|s| s.id == old));
    assert!(operation::apply(&f.0, &review).is_err());
}
#[test]
fn graph_commit_and_fixup_keep_unstaged_work_and_checkpoint_the_index() {
    use branchroom::operation::{self, Kind};
    let f = Fixture::new();
    f.write("base.txt", "base");
    f.commit("Base");
    f.write("notes.txt", "included");
    git(&f.0, &["add", "notes.txt"]).unwrap();
    f.write("notes.txt", "included plus unstaged");
    let review = operation::preview(
        &f.0,
        Kind::Commit {
            message: "Included change".into(),
        },
    )
    .unwrap();
    assert!(review.conflicts.is_empty());
    operation::apply(&f.0, &review).unwrap();
    assert_eq!(git(&f.0, &["show", "HEAD:notes.txt"]).unwrap(), "included");
    assert_eq!(
        fs::read_to_string(f.0.join("notes.txt")).unwrap(),
        "included plus unstaged"
    );
    git(&f.0, &["add", "notes.txt"]).unwrap();
    let head = git::resolve(&f.0, "HEAD").unwrap();
    let amend = operation::preview(
        &f.0,
        Kind::Amend {
            commit: head,
            message: "Updated change".into(),
        },
    )
    .unwrap();
    operation::apply(&f.0, &amend).unwrap();
    assert_eq!(
        git(&f.0, &["show", "HEAD:notes.txt"]).unwrap(),
        "included plus unstaged"
    );
    assert!(git::load(&f.0, "main", 100).unwrap().recovery_heads.len() >= 4);
}
#[test]
fn graph_rewrite_forecast_preserves_final_tree_and_refuses_published_commits() {
    use branchroom::operation::{self, Kind};
    let f = Fixture::new();
    f.write("root.txt", "root");
    f.commit("Root");
    f.write("a.txt", "a");
    f.commit("A");
    let a = git::resolve(&f.0, "HEAD").unwrap();
    f.write("b.txt", "b");
    f.commit("B");
    let b = git::resolve(&f.0, "HEAD").unwrap();
    let tree = git(&f.0, &["rev-parse", "HEAD^{tree}"]).unwrap();
    let review = operation::preview(
        &f.0,
        Kind::Squash {
            source: b,
            target: a.clone(),
            message: "A and B".into(),
        },
    )
    .unwrap();
    assert!(review.blocked.is_none());
    operation::apply(&f.0, &review).unwrap();
    assert_eq!(git(&f.0, &["rev-parse", "HEAD^{tree}"]).unwrap(), tree);
    git(&f.0, &["update-ref", "refs/remotes/origin/main", "HEAD"]).unwrap();
    let head = git::resolve(&f.0, "HEAD").unwrap();
    assert!(
        operation::preview(
            &f.0,
            Kind::Amend {
                commit: head,
                message: "Rewrite shared".into()
            }
        )
        .is_err()
    );
}

#[test]
fn graph_fixup_of_an_earlier_commit_replays_later_commits_without_touching_working_files() {
    use branchroom::operation::{self, Kind};
    let f = Fixture::new();
    f.write("root.txt", "root");
    f.commit("Root");
    f.write("a.txt", "a");
    f.commit("A");
    let a = git::resolve(&f.0, "HEAD").unwrap();
    f.write("b.txt", "b");
    f.commit("B");
    f.write("a.txt", "a fixed");
    git(&f.0, &["add", "a.txt"]).unwrap();
    f.write("b.txt", "b plus unstaged");
    let staged = git(&f.0, &["write-tree"]).unwrap();
    let review = operation::preview(
        &f.0,
        Kind::Amend {
            commit: a,
            message: "Fix A".into(),
        },
    )
    .unwrap();
    assert!(review.blocked.is_none());
    assert!(review.conflicts.is_empty());
    operation::apply(&f.0, &review).unwrap();
    assert_eq!(git(&f.0, &["rev-parse", "HEAD^{tree}"]).unwrap(), staged);
    assert_eq!(
        fs::read_to_string(f.0.join("b.txt")).unwrap(),
        "b plus unstaged"
    );
    assert_eq!(
        git(&f.0, &["log", "-1", "--format=%s"]).unwrap().trim(),
        "B"
    );
}
#[test]
fn graph_preview_refuses_a_changed_source_or_newly_published_amend() {
    use branchroom::operation::{self, Kind};
    let f = Fixture::new();
    f.write("root.txt", "root");
    f.commit("Root");
    git(&f.0, &["switch", "-c", "incoming"]).unwrap();
    f.write("a.txt", "a");
    f.commit("A");
    git(&f.0, &["switch", "main"]).unwrap();
    let review = operation::preview(
        &f.0,
        Kind::Merge {
            source: "incoming".into(),
            target: "main".into(),
        },
    )
    .unwrap();
    git(&f.0, &["update-ref", "refs/heads/incoming", "HEAD"]).unwrap();
    assert!(operation::apply(&f.0, &review).is_err());
    assert!(git::load(&f.0, "main", 100).unwrap().snapshots.is_empty());
    let head = git::resolve(&f.0, "HEAD").unwrap();
    let amend = operation::preview(
        &f.0,
        Kind::Amend {
            commit: head,
            message: "Changed root".into(),
        },
    )
    .unwrap();
    git(&f.0, &["update-ref", "refs/remotes/origin/main", "HEAD"]).unwrap();
    assert!(operation::apply(&f.0, &amend).is_err());
}
#[test]
fn graph_move_to_new_branch_preserves_dirty_work_and_keeps_main_at_remote_tip() {
    use branchroom::operation::{self, Kind};
    let f = Fixture::new();
    f.write("root.txt", "root");
    f.commit("Root");
    let base = git::resolve(&f.0, "HEAD").unwrap();
    git(&f.0, &["update-ref", "refs/remotes/origin/main", &base]).unwrap();
    git(&f.0, &["config", "branch.main.remote", "origin"]).unwrap();
    git(&f.0, &["config", "branch.main.merge", "refs/heads/main"]).unwrap();
    git(
        &f.0,
        &[
            "config",
            "remote.origin.url",
            "https://example.invalid/repo",
        ],
    )
    .unwrap();
    git(
        &f.0,
        &[
            "config",
            "remote.origin.fetch",
            "+refs/heads/*:refs/remotes/origin/*",
        ],
    )
    .unwrap();
    f.write("a.txt", "a");
    f.commit("A");
    let old = git::resolve(&f.0, "HEAD").unwrap();
    f.write("a.txt", "working edit");
    let review = operation::preview(
        &f.0,
        Kind::MoveToBranch {
            name: "feature/right-place".into(),
        },
    )
    .unwrap();
    operation::apply(&f.0, &review).unwrap();
    assert_eq!(git::resolve(&f.0, "main").unwrap(), base);
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), old);
    assert_eq!(
        git(&f.0, &["symbolic-ref", "--short", "HEAD"])
            .unwrap()
            .trim(),
        "feature/right-place"
    );
    assert_eq!(
        fs::read_to_string(f.0.join("a.txt")).unwrap(),
        "working edit"
    );
}

#[test]
fn graph_root_undo_keeps_files_and_saves_the_original_commit() {
    use branchroom::operation::{self, Kind};
    let f = Fixture::new();
    f.write("first.txt", "first");
    f.commit("First");
    let head = git::resolve(&f.0, "HEAD").unwrap();
    let review = operation::preview(&f.0, Kind::Undo).unwrap();
    assert!(review.after.is_empty());
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), head);
    operation::apply(&f.0, &review).unwrap();
    assert!(git::resolve(&f.0, "HEAD").is_err());
    assert_eq!(fs::read_to_string(f.0.join("first.txt")).unwrap(), "first");
    assert!(git(&f.0, &["ls-files"]).unwrap().is_empty());
    assert!(
        git::load(&f.0, "HEAD", 100)
            .unwrap()
            .recovery_heads
            .iter()
            .any(|saved| saved.id == head)
    );
}
#[test]
fn graph_reset_returns_removed_changes_to_the_working_directory() {
    use branchroom::operation::{self, Kind};
    let f = Fixture::new();
    f.write("root.txt", "root");
    f.commit("Root");
    let root = git::resolve(&f.0, "HEAD").unwrap();
    f.write("next.txt", "next");
    f.commit("Next");
    let review = operation::preview(
        &f.0,
        Kind::Reset {
            commit: root.clone(),
        },
    )
    .unwrap();
    operation::apply(&f.0, &review).unwrap();
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), root);
    assert_eq!(fs::read_to_string(f.0.join("next.txt")).unwrap(), "next");
    assert!(
        git(&f.0, &["status", "--porcelain"])
            .unwrap()
            .contains("next.txt")
    );
}
#[test]
fn graph_cherry_pick_forecast_and_apply_preserve_the_source_branch() {
    use branchroom::operation::{self, Kind};
    let f = Fixture::new();
    f.write("root.txt", "root");
    f.commit("Root");
    git(&f.0, &["switch", "-c", "incoming"]).unwrap();
    f.write("new.txt", "incoming");
    f.commit("Incoming");
    let id = git::resolve(&f.0, "HEAD").unwrap();
    git(&f.0, &["switch", "main"]).unwrap();
    let review = operation::preview(
        &f.0,
        Kind::CherryPick {
            commit: id.clone(),
            target: "main".into(),
        },
    )
    .unwrap();
    assert_eq!(review.introduced, 1);
    assert!(!f.0.join("new.txt").exists());
    operation::apply(&f.0, &review).unwrap();
    assert_eq!(git::resolve(&f.0, "incoming").unwrap(), id);
    assert_eq!(fs::read_to_string(f.0.join("new.txt")).unwrap(), "incoming");
}
#[test]
fn graph_reorder_and_range_squash_keep_every_file() {
    use branchroom::operation::{self, Kind};
    let f = Fixture::new();
    f.write("root.txt", "root");
    f.commit("Root");
    f.write("a.txt", "a");
    f.commit("A");
    let a = git::resolve(&f.0, "HEAD").unwrap();
    f.write("b.txt", "b");
    f.commit("B");
    let b = git::resolve(&f.0, "HEAD").unwrap();
    let tree = git(&f.0, &["rev-parse", "HEAD^{tree}"]).unwrap();
    let review = operation::preview(&f.0, Kind::Reorder { order: vec![b, a] }).unwrap();
    assert!(review.blocked.is_none());
    operation::apply(&f.0, &review).unwrap();
    assert_eq!(git(&f.0, &["rev-parse", "HEAD^{tree}"]).unwrap(), tree);
    assert_eq!(
        git(&f.0, &["log", "-1", "--format=%s"]).unwrap().trim(),
        "A"
    );
    f.write("c.txt", "c");
    f.commit("C");
    let tree = git(&f.0, &["rev-parse", "HEAD^{tree}"]).unwrap();
    let review = operation::preview(
        &f.0,
        Kind::SquashRange {
            count: 3,
            message: "Combined local work".into(),
        },
    )
    .unwrap();
    operation::apply(&f.0, &review).unwrap();
    assert_eq!(git(&f.0, &["rev-parse", "HEAD^{tree}"]).unwrap(), tree);
    assert_eq!(
        git(&f.0, &["rev-list", "--count", "HEAD"]).unwrap().trim(),
        "2"
    );
}
#[test]
fn graph_forecast_refuses_custom_merge_drivers_instead_of_running_them() {
    use branchroom::operation::{self, Kind};
    let f = Fixture::new();
    f.write("root.txt", "root");
    f.commit("Root");
    git(&f.0, &["branch", "incoming"]).unwrap();
    git(&f.0, &["config", "merge.custom.driver", "exit 99"]).unwrap();
    let error = operation::preview(
        &f.0,
        Kind::Merge {
            source: "incoming".into(),
            target: "main".into(),
        },
    )
    .unwrap_err();
    assert!(error.contains("custom merge driver"));
    assert!(git::load(&f.0, "main", 100).unwrap().snapshots.is_empty());
}
#[test]
fn saving_conflict_resolution_keeps_the_previous_marker_file_in_recovery() {
    let f = Fixture::new();
    f.write("shared.txt", "base\n");
    f.commit("Base");
    git(&f.0, &["switch", "-c", "incoming"]).unwrap();
    f.write("shared.txt", "incoming\n");
    f.commit("Incoming");
    git(&f.0, &["switch", "main"]).unwrap();
    f.write("shared.txt", "current\n");
    f.commit("Current");
    assert!(git(&f.0, &["merge", "--no-edit", "incoming"]).is_err());
    let original = fs::read_to_string(f.0.join("shared.txt")).unwrap();
    branchroom::conflict::save(&f.0, "shared.txt", "resolved\n", &original).unwrap();
    let entries = branchroom::recovery::entries(&f.0);
    assert_eq!(entries.len(), 1);
    let backup = branchroom::recovery::directory()
        .join(&entries[0].id)
        .join("content");
    assert_eq!(fs::read_to_string(backup).unwrap(), original);
    assert_eq!(git(&f.0, &["show", ":shared.txt"]).unwrap(), "resolved\n");
}
#[test]
fn prepared_commit_refuses_a_branch_switch_even_when_the_tip_is_the_same() {
    let f = Fixture::new();
    f.write("root.txt", "root");
    f.commit("Root");
    f.write("next.txt", "next");
    git(&f.0, &["add", "next.txt"]).unwrap();
    let plan = branchroom::commit::prepare(&f.0, "Next", false).unwrap();
    git(&f.0, &["switch", "-c", "other"]).unwrap();
    assert!(branchroom::commit::execute(&f.0, &plan).is_err());
    assert_eq!(
        git(&f.0, &["log", "-1", "--format=%s"]).unwrap().trim(),
        "Root"
    );
}

#[test]
fn graph_actions_bind_literal_branch_names_even_when_tags_and_prefixes_collide() {
    use branchroom::operation::{self, Kind};
    let f = Fixture::new();
    f.write("root.txt", "root");
    f.commit("Root");
    let base = git::resolve(&f.0, "HEAD").unwrap();
    git(&f.0, &["tag", "main"]).unwrap();
    git(&f.0, &["branch", "heads/main", "HEAD"]).unwrap();
    f.write("a.txt", "a");
    f.commit("A");
    let a = git::resolve(&f.0, "HEAD").unwrap();
    f.write("b.txt", "b");
    f.commit("B");
    let b = git::resolve(&f.0, "HEAD").unwrap();
    assert_eq!(git::current_branch(&f.0).unwrap(), "main");
    assert_eq!(git::load(&f.0, "main", 100).unwrap().branch, "main");
    let review = operation::preview(
        &f.0,
        Kind::Squash {
            source: b,
            target: a,
            message: "A and B".into(),
        },
    )
    .unwrap();
    operation::apply(&f.0, &review).unwrap();
    assert_eq!(git::resolve(&f.0, "refs/heads/heads/main").unwrap(), base);
    let main = git::resolve(&f.0, "refs/heads/main").unwrap();
    assert_ne!(main, base);
    assert_eq!(
        git(&f.0, &["log", "-1", "--format=%ce"]).unwrap().trim(),
        "tests@example.invalid"
    );
    let review = operation::preview(
        &f.0,
        Kind::Merge {
            source: "refs/heads/main".into(),
            target: "refs/heads/heads/main".into(),
        },
    )
    .unwrap();
    operation::apply(&f.0, &review).unwrap();
    assert_eq!(git::current_branch(&f.0).unwrap(), "heads/main");
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), main);
    assert_eq!(git::resolve(&f.0, "refs/tags/main").unwrap(), base);
}
#[test]
fn advanced_resolution_delete_preserves_conflicted_file_before_removing_it() {
    let f = Fixture::new();
    f.write("shared.txt", "base\n");
    f.commit("Base");
    git(&f.0, &["switch", "-c", "incoming"]).unwrap();
    f.write("shared.txt", "incoming\n");
    f.commit("Incoming");
    git(&f.0, &["switch", "main"]).unwrap();
    f.write("shared.txt", "current\n");
    f.commit("Current");
    assert!(git(&f.0, &["merge", "--no-edit", "incoming"]).is_err());
    let original = fs::read_to_string(f.0.join("shared.txt")).unwrap();
    branchroom::operation::advanced_safety(
        &f.0,
        &branchroom::process::strings(&["rm", "--", "shared.txt"]),
    )
    .unwrap();
    git(&f.0, &["rm", "--", "shared.txt"]).unwrap();
    assert!(!f.0.join("shared.txt").exists());
    let entries = branchroom::recovery::entries(&f.0);
    assert!(entries.iter().any(|e| {
        fs::read_to_string(
            branchroom::recovery::directory()
                .join(&e.id)
                .join("content"),
        )
        .ok()
        .as_deref()
            == Some(original.as_str())
    }));
}
#[test]
fn signing_policy_requires_exact_advanced_rewrite_instead_of_importing_unsigned_commits() {
    use branchroom::operation::{self, Kind};
    let f = Fixture::new();
    f.write("root.txt", "root");
    f.commit("Root");
    f.write("a.txt", "a");
    f.commit("A");
    let a = git::resolve(&f.0, "HEAD").unwrap();
    f.write("b.txt", "b");
    f.commit("B");
    let b = git::resolve(&f.0, "HEAD").unwrap();
    git(&f.0, &["config", "commit.gpgsign", "true"]).unwrap();
    assert!(
        operation::preview(
            &f.0,
            Kind::Squash {
                source: b,
                target: a,
                message: "Combined".into()
            }
        )
        .unwrap_err()
        .contains("signing")
    );
}

#[test]
fn timeline_undo_redo_restores_head_index_working_and_untracked_files() {
    use branchroom::timeline;
    let f = Fixture::new();
    f.write("a.txt", "original\n");
    f.commit("Root");
    let original = git::resolve(&f.0, "HEAD").unwrap();
    f.write("a.txt", "included\n");
    git(&f.0, &["add", "a.txt"]).unwrap();
    f.write("a.txt", "working\n");
    f.write("new.txt", "untracked\n");
    let action = timeline::begin(&f.0, "Commit included changes").unwrap();
    let id = action.id.clone();
    git(&f.0, &["commit", "-m", "Included"]).unwrap();
    let committed = git::resolve(&f.0, "HEAD").unwrap();
    timeline::finish(&f.0, action, true).unwrap();
    timeline::undo_redo(&f.0, false, &id).unwrap();
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), original);
    assert_eq!(git(&f.0, &["show", ":a.txt"]).unwrap(), "included\n");
    assert_eq!(fs::read_to_string(f.0.join("a.txt")).unwrap(), "working\n");
    assert_eq!(
        fs::read_to_string(f.0.join("new.txt")).unwrap(),
        "untracked\n"
    );
    timeline::undo_redo(&f.0, true, &id).unwrap();
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), committed);
    assert_eq!(git(&f.0, &["show", ":a.txt"]).unwrap(), "included\n");
    assert_eq!(fs::read_to_string(f.0.join("a.txt")).unwrap(), "working\n");
}
#[test]
fn timeline_refuses_external_edits_and_retires_redo_after_a_new_action() {
    use branchroom::timeline;
    let f = Fixture::new();
    f.write("a.txt", "one");
    f.commit("Root");
    let action = timeline::begin(&f.0, "Edit file").unwrap();
    let id = action.id.clone();
    f.write("a.txt", "two");
    timeline::finish(&f.0, action, true).unwrap();
    f.write("a.txt", "external");
    assert!(
        timeline::undo_redo(&f.0, false, &id)
            .unwrap_err()
            .contains("changed after")
    );
    assert_eq!(fs::read_to_string(f.0.join("a.txt")).unwrap(), "external");
    f.write("a.txt", "two");
    timeline::undo_redo(&f.0, false, &id).unwrap();
    let next = timeline::begin(&f.0, "Another edit").unwrap();
    f.write("a.txt", "three");
    timeline::finish(&f.0, next, true).unwrap();
    assert!(timeline::candidate(&f.0, true).is_err());
}
#[test]
fn timeline_recovers_deleted_branch_and_non_top_shelved_work() {
    use branchroom::timeline;
    let f = Fixture::new();
    f.write("a.txt", "root");
    f.commit("Root");
    git(&f.0, &["branch", "saved"]).unwrap();
    for label in ["first", "second", "third"] {
        f.write("a.txt", label);
        git(&f.0, &["stash", "push", "-m", label]).unwrap();
    }
    let shelves = git(&f.0, &["stash", "list", "--format=%H%x00%gs"]).unwrap();
    let action = timeline::begin(&f.0, "Delete shelved work").unwrap();
    let id = action.id.clone();
    git(&f.0, &["stash", "drop", "stash@{1}"]).unwrap();
    git(&f.0, &["branch", "-D", "saved"]).unwrap();
    timeline::finish(&f.0, action, true).unwrap();
    timeline::undo_redo(&f.0, false, &id).unwrap();
    assert_eq!(
        git(&f.0, &["stash", "list", "--format=%H%x00%gs"]).unwrap(),
        shelves
    );
    assert!(git::resolve(&f.0, "refs/heads/saved").is_ok());
    timeline::undo_redo(&f.0, true, &id).unwrap();
    assert_eq!(
        git(&f.0, &["stash", "list", "--format=%H"])
            .unwrap()
            .lines()
            .count(),
        2
    );
    assert!(git::resolve(&f.0, "refs/heads/saved").is_err());
}
#[test]
fn timeline_restore_preview_is_stale_safe_and_restore_itself_can_be_undone() {
    use branchroom::timeline;
    let f = Fixture::new();
    f.write("a.txt", "one");
    f.commit("First");
    let first = git::resolve(&f.0, "HEAD").unwrap();
    f.write("a.txt", "two");
    f.commit("Second");
    let second = git::resolve(&f.0, "HEAD").unwrap();
    let item = timeline::items(&f.0, 100)
        .unwrap()
        .into_iter()
        .find(|e| e.kind == "Commit" && e.target == first)
        .unwrap();
    let plan = timeline::plan_restore(&f.0, &item, false).unwrap();
    f.write("new.txt", "newer");
    assert!(timeline::apply_restore(&f.0, &plan).is_err());
    fs::remove_file(f.0.join("new.txt")).unwrap();
    let action = timeline::begin(&f.0, "Restore first version").unwrap();
    let id = action.id.clone();
    timeline::apply_restore(&f.0, &plan).unwrap();
    timeline::finish(&f.0, action, true).unwrap();
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), first);
    assert_eq!(fs::read_to_string(f.0.join("a.txt")).unwrap(), "one");
    timeline::undo_redo(&f.0, false, &id).unwrap();
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), second);
}
#[test]
fn timeline_unborn_commit_undo_and_redo_keeps_files() {
    use branchroom::timeline;
    let f = Fixture::new();
    f.write("a.txt", "first");
    git(&f.0, &["add", "a.txt"]).unwrap();
    let action = timeline::begin(&f.0, "First commit").unwrap();
    let id = action.id.clone();
    git(&f.0, &["commit", "-m", "First"]).unwrap();
    let head = git::resolve(&f.0, "HEAD").unwrap();
    timeline::finish(&f.0, action, true).unwrap();
    timeline::undo_redo(&f.0, false, &id).unwrap();
    assert!(git::resolve(&f.0, "HEAD").is_err());
    assert_eq!(git(&f.0, &["show", ":a.txt"]).unwrap(), "first");
    timeline::undo_redo(&f.0, true, &id).unwrap();
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), head);
}
#[test]
fn timeline_merges_old_safety_points_discards_shelves_and_commits_without_losing_deleted_data() {
    use branchroom::timeline;
    let f = Fixture::new();
    f.write("a.txt", "first");
    f.commit("First");
    let saved = snapshot::create(&f.0, "Manual version").unwrap();
    let reference = saved.lines().find(|l| l.starts_with("refs/")).unwrap();
    snapshot::remove(&f.0, reference).unwrap();
    f.write("a.txt", "discard");
    branchroom::recovery::discard(&f.0, "a.txt").unwrap();
    f.write("a.txt", "shelved");
    git(&f.0, &["stash", "push", "-m", "Later"]).unwrap();
    let items = timeline::items(&f.0, 100).unwrap();
    for kind in [
        "Commit",
        "Deleted safety point",
        "Discarded file",
        "Shelved work",
    ] {
        assert!(items.iter().any(|i| i.kind == kind), "Missing {kind}");
    }
}

#[test]
fn timeline_restore_refuses_ignored_file_collisions_and_keeps_the_original_index() {
    use branchroom::timeline;
    let f = Fixture::new();
    f.write("secret.txt", "tracked version");
    f.commit("First");
    let first = git::resolve(&f.0, "HEAD").unwrap();
    git(&f.0, &["rm", "secret.txt"]).unwrap();
    f.write(".gitignore", "secret.txt\n");
    f.commit("Ignore local secret");
    f.write("secret.txt", "private local value");
    let item = timeline::Item {
        target: first,
        kind: "Commit".into(),
        label: "First".into(),
        ..Default::default()
    };
    let plan = timeline::plan_restore(&f.0, &item, false).unwrap();
    let head = git::resolve(&f.0, "HEAD").unwrap();
    let index = git(&f.0, &["write-tree"]).unwrap();
    assert!(timeline::apply_restore(&f.0, &plan).is_err());
    assert_eq!(
        fs::read_to_string(f.0.join("secret.txt")).unwrap(),
        "private local value"
    );
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), head);
    assert_eq!(git(&f.0, &["write-tree"]).unwrap(), index);
}
#[test]
fn timeline_can_undo_and_redo_multiple_actions_and_preserve_new_file_removal() {
    use branchroom::timeline;
    let f = Fixture::new();
    f.write("a.txt", "root");
    f.commit("Root");
    let mut ids = vec![];
    for (name, text) in [("first.txt", "one"), ("second.txt", "two")] {
        let entry = timeline::begin(&f.0, name).unwrap();
        ids.push(entry.id.clone());
        f.write(name, text);
        timeline::finish(&f.0, entry, true).unwrap();
    }
    timeline::undo_redo(&f.0, false, &ids[1]).unwrap();
    assert!(!f.0.join("second.txt").exists());
    timeline::undo_redo(&f.0, false, &ids[0]).unwrap();
    assert!(!f.0.join("first.txt").exists());
    timeline::undo_redo(&f.0, true, &ids[0]).unwrap();
    timeline::undo_redo(&f.0, true, &ids[1]).unwrap();
    assert_eq!(fs::read_to_string(f.0.join("second.txt")).unwrap(), "two");
}
#[test]
fn timeline_visibility_deletion_keeps_recovery_data_and_can_be_reversed() {
    use branchroom::timeline;
    let f = Fixture::new();
    f.write("a.txt", "root");
    f.commit("Root");
    let item = timeline::items(&f.0, 50)
        .unwrap()
        .into_iter()
        .find(|i| i.kind == "Commit")
        .unwrap();
    timeline::set_visible(&f.0, &item.key, false).unwrap();
    assert!(
        timeline::items(&f.0, 50)
            .unwrap()
            .iter()
            .find(|i| i.key == item.key)
            .unwrap()
            .hidden
    );
    assert!(git::resolve(&f.0, &item.target).is_ok());
    timeline::set_visible(&f.0, &item.key, true).unwrap();
    assert!(
        !timeline::items(&f.0, 50)
            .unwrap()
            .iter()
            .find(|i| i.key == item.key)
            .unwrap()
            .hidden
    );
}
#[test]
fn timeline_completed_conflict_continuation_retains_the_original_before_state() {
    use branchroom::timeline;
    let f = Fixture::new();
    f.write("a.txt", "base\n");
    f.commit("Base");
    git(&f.0, &["switch", "-c", "other"]).unwrap();
    f.write("a.txt", "other\n");
    f.commit("Other");
    git(&f.0, &["switch", "main"]).unwrap();
    f.write("a.txt", "main\n");
    f.commit("Main");
    let original = git::resolve(&f.0, "HEAD").unwrap();
    let action = timeline::begin(&f.0, "Merge branches").unwrap();
    assert!(git(&f.0, &["merge", "--no-edit", "other"]).is_err());
    timeline::finish(&f.0, action, false).unwrap();
    f.write("a.txt", "combined\n");
    git(&f.0, &["add", "a.txt"]).unwrap();
    let continuation = timeline::unavailable(&f.0, "Continue merge", "Active merge").unwrap();
    git(&f.0, &["commit", "--no-edit"]).unwrap();
    timeline::finish(&f.0, continuation, true).unwrap();
    let entry = timeline::candidate(&f.0, false).unwrap();
    timeline::undo_redo(&f.0, false, &entry.id).unwrap();
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), original);
    assert_eq!(fs::read_to_string(f.0.join("a.txt")).unwrap(), "main\n");
}

#[test]
fn timeline_pins_deleted_branch_objects_through_git_garbage_collection() {
    use branchroom::timeline;
    let f = Fixture::new();
    f.write("a.txt", "root");
    f.commit("Root");
    git(&f.0, &["switch", "-c", "temporary"]).unwrap();
    f.write("unique.txt", "unique");
    f.commit("Unique branch commit");
    let tip = git::resolve(&f.0, "HEAD").unwrap();
    git(&f.0, &["switch", "main"]).unwrap();
    let entry = timeline::begin(&f.0, "Delete temporary branch").unwrap();
    let id = entry.id.clone();
    git(&f.0, &["branch", "-D", "temporary"]).unwrap();
    timeline::finish(&f.0, entry, true).unwrap();
    git(&f.0, &["reflog", "expire", "--expire=now", "--all"]).unwrap();
    git(&f.0, &["gc", "--prune=now"]).unwrap();
    timeline::undo_redo(&f.0, false, &id).unwrap();
    assert_eq!(git::resolve(&f.0, "refs/heads/temporary").unwrap(), tip);
}

#[test]
fn timeline_failed_action_does_not_claim_external_edits_as_a_completed_undo_step() {
    use branchroom::timeline;
    let f = Fixture::new();
    f.write("a.txt", "root");
    f.commit("Root");
    let entry = timeline::begin(&f.0, "A refused stale action").unwrap();
    f.write("a.txt", "newer external work");
    timeline::finish(&f.0, entry, false).unwrap();
    assert!(timeline::candidate(&f.0, false).is_err());
    assert_eq!(
        fs::read_to_string(f.0.join("a.txt")).unwrap(),
        "newer external work"
    );
    assert!(
        timeline::items(&f.0, 50)
            .unwrap()
            .iter()
            .any(|e| e.kind == "Action")
    );
}

#[test]
fn final_review_revert_forecasts_conflicts_and_keeps_published_history() {
    use branchroom::operation::{self, Kind};
    let f = Fixture::new();
    f.write("a.txt", "one\n");
    f.commit("First");
    let first = git::resolve(&f.0, "HEAD").unwrap();
    f.write("a.txt", "two\n");
    f.commit("Second");
    let second = git::resolve(&f.0, "HEAD").unwrap();
    git(&f.0, &["update-ref", "refs/remotes/origin/main", &second]).unwrap();
    let review = operation::preview(
        &f.0,
        Kind::Revert {
            commit: second.clone(),
            mainline: None,
        },
    )
    .unwrap();
    assert!(review.conflicts.is_empty());
    assert!(review.command.contains("revert"));
    operation::apply(&f.0, &review).unwrap();
    assert_eq!(fs::read_to_string(f.0.join("a.txt")).unwrap(), "one\n");
    assert_eq!(git::resolve(&f.0, "HEAD^").unwrap(), second);
    assert_eq!(
        git::resolve(&f.0, "refs/remotes/origin/main").unwrap(),
        second
    );
    f.write("a.txt", "Later content that must be resolved\n");
    f.commit("Later local change");
    let head = git::resolve(&f.0, "HEAD").unwrap();
    let conflict = operation::preview(
        &f.0,
        Kind::Revert {
            commit: first,
            mainline: None,
        },
    )
    .unwrap();
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), head);
    assert_eq!(conflict.conflicts, vec!["a.txt"]);
    assert!(conflict.blocked.is_none());
}
#[test]
fn final_review_move_exact_count_works_without_remote_and_keeps_dirty_files() {
    use branchroom::operation::{self, Kind};
    let f = Fixture::new();
    f.write("root.txt", "root");
    f.commit("Root");
    let root = git::resolve(&f.0, "HEAD").unwrap();
    for name in ["a.txt", "b.txt"] {
        f.write(name, name);
        f.commit(name);
    }
    let tip = git::resolve(&f.0, "HEAD").unwrap();
    f.write("scratch.txt", "still working");
    let review = operation::preview(
        &f.0,
        Kind::MoveCommits {
            name: "feature/moved".into(),
            count: 2,
        },
    )
    .unwrap();
    operation::apply(&f.0, &review).unwrap();
    assert_eq!(git::current_branch(&f.0).unwrap(), "feature/moved");
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), tip);
    assert_eq!(git::resolve(&f.0, "refs/heads/main").unwrap(), root);
    assert_eq!(
        fs::read_to_string(f.0.join("scratch.txt")).unwrap(),
        "still working"
    );
}
#[test]
fn final_review_branch_rename_and_recovery_use_checked_previews() {
    use branchroom::operation::{self, Kind};
    let f = Fixture::new();
    f.write("a.txt", "root");
    f.commit("Root");
    let root = git::resolve(&f.0, "HEAD").unwrap();
    let rename = operation::preview(
        &f.0,
        Kind::RenameBranch {
            name: "feature/renamed".into(),
        },
    )
    .unwrap();
    operation::apply(&f.0, &rename).unwrap();
    assert_eq!(git::current_branch(&f.0).unwrap(), "feature/renamed");
    let recovered = operation::preview(
        &f.0,
        Kind::BranchFrom {
            name: "main".into(),
            commit: root.clone(),
        },
    )
    .unwrap();
    operation::apply(&f.0, &recovered).unwrap();
    assert_eq!(git::resolve(&f.0, "refs/heads/main").unwrap(), root);
    assert!(operation::apply(&f.0, &recovered).is_err());
}
#[test]
fn final_review_commit_details_load_history_outside_the_graph_page() {
    let f = Fixture::new();
    f.write("a.txt", "root");
    f.commit("Initial commit");
    let root = git::resolve(&f.0, "HEAD").unwrap();
    f.write("a.txt", "later");
    f.commit("Later commit");
    assert_eq!(git::load(&f.0, "main", 1).unwrap().commits.len(), 1);
    let details = git::commit_details(&f.0, &root).unwrap();
    assert_eq!(details.subject, "Initial commit");
    assert!(details.parents.is_empty());
}

#[test]
fn final_review_undo_branch_rename_restores_tracking_configuration() {
    use branchroom::{
        operation::{self, Kind},
        timeline,
    };
    let f = Fixture::new();
    f.write("a.txt", "root");
    f.commit("Root");
    git(&f.0, &["config", "branch.main.remote", "origin"]).unwrap();
    git(&f.0, &["config", "branch.main.merge", "refs/heads/main"]).unwrap();
    let review = operation::preview(
        &f.0,
        Kind::RenameBranch {
            name: "renamed".into(),
        },
    )
    .unwrap();
    let entry = timeline::begin(&f.0, "Rename main").unwrap();
    let id = entry.id.clone();
    operation::apply(&f.0, &review).unwrap();
    timeline::finish(&f.0, entry, true).unwrap();
    assert_eq!(
        git(&f.0, &["config", "branch.renamed.remote"])
            .unwrap()
            .trim(),
        "origin"
    );
    timeline::undo_redo(&f.0, false, &id).unwrap();
    assert_eq!(git::current_branch(&f.0).unwrap(), "main");
    assert_eq!(
        git(&f.0, &["config", "branch.main.remote"]).unwrap().trim(),
        "origin"
    );
    assert!(git(&f.0, &["config", "branch.renamed.remote"]).is_err());
}

#[test]
fn final_review_can_move_the_initial_commit_and_undo_the_move() {
    use branchroom::{
        operation::{self, Kind},
        timeline,
    };
    let f = Fixture::new();
    f.write("a.txt", "root");
    f.commit("Root");
    let head = git::resolve(&f.0, "HEAD").unwrap();
    let plan = operation::preview(
        &f.0,
        Kind::MoveCommits {
            name: "feature/initial".into(),
            count: 1,
        },
    )
    .unwrap();
    let entry = timeline::begin(&f.0, "Move initial commit").unwrap();
    let id = entry.id.clone();
    operation::apply(&f.0, &plan).unwrap();
    timeline::finish(&f.0, entry, true).unwrap();
    assert_eq!(git::current_branch(&f.0).unwrap(), "feature/initial");
    assert!(git::resolve(&f.0, "refs/heads/main").is_err());
    timeline::undo_redo(&f.0, false, &id).unwrap();
    assert_eq!(git::current_branch(&f.0).unwrap(), "main");
    assert_eq!(git::resolve(&f.0, "HEAD").unwrap(), head);
}
