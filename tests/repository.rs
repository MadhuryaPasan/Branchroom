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
    git(&f.0, &["update-ref", "refs/remotes/origin/main", &head]).unwrap();
    git(
        &f.0,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
    )
    .unwrap();
    let state = git::load(&f.0, "main", 50).unwrap();
    assert!(
        state
            .branches
            .iter()
            .any(|branch| branch.name == "origin/main")
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
