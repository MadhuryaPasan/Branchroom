use super::*;
use egui_kittest::Harness;
fn screen(page: Page, selected: bool) -> egui_kittest::Harness<'static, Branchroom> {
    Harness::builder().with_size(egui::vec2(1320.,820.)).with_accessibility_check(false).build_eframe(move|cc|{
        let mut app=Branchroom::with_context(&cc.egui_ctx,true);
        if page==Page::Conflicts{app.preview_state=6;app.load_preview_state();}
        app.page=page;
        if selected&&page==Page::Snapshots{app.selected_snapshot=app.repo.as_ref().unwrap().snapshots[0].reference.clone();}
        if selected&&page==Page::Github{app.pr_tab=1;}
        if selected&&page==Page::Changes{
            app.selected_path="src/cache.rs".into();app.diff_refresh=false;
            app.file_patch=Some(std::sync::Arc::new(branchroom::diff::Patch::parse("diff --git a/src/cache.rs b/src/cache.rs\n--- a/src/cache.rs\n+++ b/src/cache.rs\n@@ -1 +1 @@\n-const SIZE: usize = 128;\n+const SIZE: usize = 256;\n")));
        }
        app
    })
}
#[test]
fn every_screen_has_named_controls() {
    let mut failures = vec![];
    for page in [
        Page::History,
        Page::Changes,
        Page::Snapshots,
        Page::Branches,
        Page::Compare,
        Page::Merge,
        Page::Stashes,
        Page::Conflicts,
        Page::Remotes,
        Page::Github,
        Page::Tools,
        Page::Settings,
    ] {
        for selected in [false, true] {
            let mut harness = screen(page, selected);
            harness.run_steps(3);
            let unnamed = harness.unnamed_widgets();
            if !unnamed.is_empty() {
                failures.push(format!(
                    "{page:?}, selected={selected}: {}",
                    unnamed.join("; ")
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
#[test]
#[ignore = "Exports real framework renders for manual visual QA; requires a graphics adapter."]
fn render_audit_screens() {
    std::fs::create_dir_all("target/qa/ui").unwrap();
    for page in [
        Page::History,
        Page::Changes,
        Page::Snapshots,
        Page::Branches,
        Page::Compare,
        Page::Merge,
        Page::Stashes,
        Page::Conflicts,
        Page::Remotes,
        Page::Github,
        Page::Tools,
        Page::Settings,
    ] {
        let mut harness = screen(page, true);
        harness.run_steps(3);
        harness
            .render()
            .unwrap()
            .save(format!("target/qa/ui/{page:?}.png"))
            .unwrap();
    }

    for page in [
        Page::History,
        Page::Changes,
        Page::Snapshots,
        Page::Github,
        Page::Settings,
    ] {
        let mut harness = screen(page, true);
        harness.ctx.set_zoom_factor(2.);
        harness.run_steps(2);
        harness.set_size(egui::vec2(530., 350.));
        harness.run_steps(3);
        assert!((harness.ctx.content_rect().width() - 530.).abs() < 1.);
        harness
            .render()
            .unwrap()
            .save(format!("target/qa/ui/{page:?}-zoom.png"))
            .unwrap();
    }
    for (tab, name) in [(1, "Diff"), (2, "Commit")] {
        let mut harness = screen(Page::Changes, true);
        harness.ctx.set_zoom_factor(2.);
        harness.run_steps(2);
        harness.set_size(egui::vec2(530., 350.));
        harness.state_mut().compact_changes_tab = tab;
        harness.run_steps(3);
        harness
            .render()
            .unwrap()
            .save(format!("target/qa/ui/Changes-{name}-zoom.png"))
            .unwrap();
    }
    let mut long = screen(Page::Changes, false);
    long.state_mut().preview_state = 5;
    long.state_mut().load_preview_state();
    long.state_mut().page = Page::Changes;
    long.run_steps(3);
    long.render()
        .unwrap()
        .save("target/qa/ui/Changes-long.png")
        .unwrap();
    let mut harness = screen(Page::Github, false);
    harness.state_mut().pr_details = Some(review_fixture());
    harness.state_mut().pr_tab = 2;
    harness.run_steps(3);
    harness
        .render()
        .unwrap()
        .save("target/qa/ui/Review.png")
        .unwrap();
    let mut harness = screen(Page::History, false);
    harness.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::B);
    harness.run_steps(3);
    harness
        .render()
        .unwrap()
        .save("target/qa/ui/Branch-picker.png")
        .unwrap();
}

#[test]
fn keyboard_branch_picker_and_modal_escape_work() {
    use egui_kittest::kittest::Queryable;
    let mut harness = screen(Page::History, false);
    harness.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::B);
    harness.run_steps(3);
    assert!(harness.query_by_label("New branch…").is_some());
    harness.key_press(egui::Key::Escape);
    harness.run_steps(2);
    assert!(harness.query_by_label("New branch…").is_none());
    harness.state_mut().capture_open = true;
    harness.run_steps(2);
    harness.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::Num1);
    harness.run_steps(2);
    assert_eq!(harness.state().page, Page::History);
    harness.key_press(egui::Key::Escape);
    harness.run_steps(2);
    assert!(!harness.state().capture_open);
}

#[test]
fn screens_survive_minimum_size_and_two_hundred_percent_zoom() {
    for page in [
        Page::History,
        Page::Changes,
        Page::Snapshots,
        Page::Branches,
        Page::Compare,
        Page::Merge,
        Page::Stashes,
        Page::Conflicts,
        Page::Remotes,
        Page::Github,
        Page::Tools,
        Page::Settings,
    ] {
        let mut harness = screen(page, true);
        harness.ctx.set_zoom_factor(2.);
        harness.run_steps(2);
        harness.set_size(egui::vec2(530., 350.));
        harness.run_steps(3);
        assert!((harness.ctx.content_rect().width() - 530.).abs() < 1.);
        assert!(
            harness.unnamed_widgets().is_empty(),
            "{page:?}: {:?}",
            harness.unnamed_widgets()
        );
        let rect = harness.ctx.content_rect();
        assert!(rect.width() > 0. && rect.height() > 0.);
    }
}

fn review_fixture() -> branchroom::github::PullRequestDetails {
    branchroom::github::PullRequestDetails{number:42,title:"Keep notes available offline".into(),body:"Cache recent notes so they remain available during a network outage.\n\nTested with a disconnected account.".into(),url:"https://example.invalid/pull/42".into(),state:"OPEN".into(),head_ref_name:"feature/offline".into(),base_ref_name:"main".into(),head_ref_oid:"a".repeat(40),merge_state_status:"CLEAN".into(),changed_files:3,additions:42,deletions:6,..Default::default()}
}
#[test]
fn review_and_image_controls_have_names() {
    let mut harness = screen(Page::Github, false);
    harness.state_mut().pr_details = Some(review_fixture());
    harness.state_mut().pr_tab = 2;
    harness.run_steps(3);
    harness.check_accessibility();
    let mut harness = screen(Page::Changes, true);
    harness.state_mut().selected_path = "image.png".into();
    harness.state_mut().file_patch = Some(std::sync::Arc::new(branchroom::diff::Patch::parse(
        "diff --git a/image.png b/image.png\nBinary files a/image.png and b/image.png differ\n",
    )));
    harness.state_mut().image_data = Some(branchroom::image_diff::Comparison {
        before: Some(branchroom::image_diff::Frame {
            width: 2,
            height: 2,
            rgba: vec![255; 16],
        }),
        after: Some(branchroom::image_diff::Frame {
            width: 2,
            height: 2,
            rgba: vec![128; 16],
        }),
    });
    harness.run_steps(3);
    harness.check_accessibility();
}

#[test]
fn project_switch_clears_pull_request_actions() {
    let mut harness = screen(Page::Github, false);
    harness.state_mut().pr_details = Some(review_fixture());
    harness.state_mut().pr_review = "Unsent review for the old project".into();
    let mut repo = harness.state().repo.as_ref().unwrap().as_ref().clone();
    repo.root = repo.root.join("different-project");
    let (tx, rx) = mpsc::channel();
    harness.state_mut().receiver = Some(rx);
    tx.send(Completed {
        output: Ok(String::new()),
        state: Some(Ok(repo)),
        detail: false,
        open: None,
        data: JobData::None,
    })
    .unwrap();
    harness.run_steps(2);
    assert!(harness.state().pr_details.is_none());
    assert!(harness.state().pr_review.is_empty());
    assert!(harness.state().pull_requests.is_empty());
}

#[test]
fn branch_drafts_restore_without_leaking_between_branches() {
    let mut harness = screen(Page::Changes, false);
    let first = harness.state().repo.as_ref().unwrap().as_ref().clone();
    let mut other = first.clone();
    other.branch = "another-branch".into();
    let app = harness.state_mut();
    app.settings
        .projects
        .entry(first.root.clone())
        .or_default()
        .drafts
        .insert(
            first.branch.clone(),
            branchroom::settings::Draft {
                summary: "Offline cache".into(),
                pr_body: "Review the outage behavior".into(),
                ..Default::default()
            },
        );
    app.load_draft(&first);
    assert_eq!(app.commit_message, "Offline cache");
    app.load_draft(&other);
    assert!(app.commit_message.is_empty());
    assert!(app.pr_body.is_empty());
    app.load_draft(&first);
    assert_eq!(app.pr_body, "Review the outage behavior");
}
#[test]
fn close_confirmation_cancels_worker_before_closing() {
    use egui_kittest::kittest::Queryable;
    let mut harness = screen(Page::History, false);
    let (_tx, rx) = mpsc::channel();
    let token = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    harness.state_mut().receiver = Some(rx);
    harness.state_mut().cancellation = Some(token.clone());
    harness.state_mut().close_pending = true;
    harness.run_steps(3);
    harness.check_accessibility();
    harness.get_by_label("Cancel operation & close").click();
    harness.run_steps(2);
    assert!(token.load(std::sync::atomic::Ordering::Relaxed));
    assert!(harness.state().close_after_job);
    assert!(harness.state().receiver.is_some());
}

#[test]
fn compact_changes_keeps_commit_action_reachable() {
    use egui_kittest::kittest::Queryable;
    let mut harness = screen(Page::Changes, true);
    harness.ctx.set_zoom_factor(2.);
    harness.run_steps(2);
    harness.set_size(egui::vec2(530., 350.));
    harness.run_steps(3);
    harness.get_by_label("Commit").click();
    harness.run_steps(3);
    assert_eq!(harness.state().compact_changes_tab, 2);
    let action = harness.get_by_label("Commit to feature/offline");
    let bounds = action.rect();
    assert!(bounds.max.y <= 350. && bounds.min.y > 0., "{bounds:?}");
}

#[test]
fn compact_toolbar_and_diff_keep_distinct_visible_targets() {
    use egui_kittest::kittest::Queryable;
    let mut harness = screen(Page::Changes, true);
    harness.ctx.set_zoom_factor(2.);
    harness.run_steps(2);
    harness.set_size(egui::vec2(530., 350.));
    harness.state_mut().compact_changes_tab = 1;
    harness.run_steps(3);
    let branch = harness
        .get_by_label("Current branch\nfeature/offline")
        .rect();
    let sync = harness.get_by_label("Sync").rect();
    assert!(sync.min.x >= branch.max.x, "{branch:?} overlaps {sync:?}");
    let line = harness
        .get_by_label("Include line 1: +const SIZE: usize = 256;")
        .rect();
    assert!(line.max.y <= 350., "Diff content clipped: {line:?}");
}

#[test]
fn selector_size_stays_stable_for_long_repository_and_branch_names() {
    use egui_kittest::kittest::Queryable;
    let mut harness = screen(Page::Changes, false);
    harness.run_steps(3);
    let initial_repo = format!("Current repository: {}", harness.state().path().display());
    let initial_branch = "Current branch\nfeature/offline";
    let repo_rect = harness.get_by_label(&initial_repo).rect();
    let branch_rect = harness.get_by_label(initial_branch).rect();
    let mut repo = harness.state().repo.as_ref().unwrap().as_ref().clone();
    repo.root = repo
        .root
        .with_file_name("a-very-long-project-name-with-unicode-Δ-and-more-details");
    repo.branch = "feature/a-very-long-branch-name-with-additional-context-and-unicode-Δ".into();
    let repo_label = format!("Current repository: {}", repo.root.display());
    let branch_label = format!("Current branch\n{}", repo.branch);
    harness.state_mut().repo = Some(std::sync::Arc::new(repo));
    harness.run_steps(3);
    let long_repo = harness.get_by_label(&repo_label).rect();
    let long_branch = harness.get_by_label(&branch_label).rect();
    assert!(
        (repo_rect.width() - long_repo.width()).abs() < 1.,
        "{repo_rect:?} {long_repo:?}"
    );
    assert!(
        (branch_rect.width() - long_branch.width()).abs() < 1.,
        "{branch_rect:?} {long_branch:?}"
    );
    assert!(
        long_repo.max.x <= long_branch.min.x,
        "{long_repo:?} {long_branch:?}"
    );
}
#[test]
fn changed_file_rows_expose_selection_and_file_actions() {
    use egui_kittest::kittest::Queryable;
    let mut harness = screen(Page::Changes, false);
    harness.state_mut().files_tree = false;
    harness.run_steps(3);
    let file = harness.get_by_label("src/cache.rs · Added · partially staged");
    let file_rect = file.rect();
    let actions = harness.get_by_label("File actions for src/cache.rs");
    let action_rect = actions.rect();
    assert!(
        file_rect.max.x <= action_rect.min.x,
        "{file_rect:?} {action_rect:?}"
    );
    assert!(file_rect.height() >= 30.);
    file.click();
    harness.run_steps(2);
    assert_eq!(harness.state().selected_path, "src/cache.rs");
    harness
        .get_by_label("File actions for src/cache.rs")
        .click();
    harness.run_steps(2);
    assert!(harness.query_by_label("View working changes").is_some());
}

#[test]
#[ignore = "Exports focused layout renders; requires a graphics adapter."]
fn render_changes_tuning() {
    std::fs::create_dir_all("target/qa/ui").unwrap();
    let mut normal = screen(Page::Changes, true);
    normal.run_steps(3);
    normal
        .render()
        .unwrap()
        .save("target/qa/ui/tuning-normal.png")
        .unwrap();
    let mut long = screen(Page::Changes, false);
    long.state_mut().preview_state = 5;
    long.state_mut().load_preview_state();
    long.state_mut().page = Page::Changes;
    long.run_steps(3);
    long.render()
        .unwrap()
        .save("target/qa/ui/tuning-long.png")
        .unwrap();
    let mut zoom = screen(Page::Changes, false);
    zoom.ctx.set_zoom_factor(2.);
    zoom.run_steps(2);
    zoom.set_size(egui::vec2(530., 350.));
    zoom.run_steps(3);
    zoom.render()
        .unwrap()
        .save("target/qa/ui/tuning-zoom.png")
        .unwrap();
}

#[test]
fn compact_shortcut_opens_the_commit_composer() {
    let mut harness = screen(Page::Changes, false);
    harness.ctx.set_zoom_factor(2.);
    harness.run_steps(2);
    harness.set_size(egui::vec2(530., 350.));
    harness.run_steps(3);
    harness.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::G);
    harness.run_steps(2);
    assert_eq!(harness.state().compact_changes_tab, 2);
    assert_eq!(
        harness.ctx.memory(|m| m.focused()),
        Some(egui::Id::unique("commit_summary"))
    );
}

#[test]
fn long_file_names_do_not_expand_the_resizable_changes_pane() {
    use egui_kittest::kittest::Queryable;
    let mut harness = screen(Page::Changes, false);
    harness.state_mut().preview_state = 5;
    harness.state_mut().load_preview_state();
    harness.state_mut().page = Page::Changes;
    harness.run_steps(3);
    let label = "File actions for src/synchronization/disconnected_accounts/metadata_reconciliation/incremental_snapshot_queue_000.rs";
    let before = harness.get_by_label(label).rect();
    harness.run_steps(10);
    let after = harness.get_by_label(label).rect();
    assert!(
        (before.max.x - after.max.x).abs() < 1.,
        "pane expanded: {before:?} to {after:?}"
    );
    assert!(
        after.max.x < 690.,
        "long file names expanded the pane: {after:?}"
    );
}

#[test]
#[ignore = "Exports design review screens; requires a graphics adapter."]
fn render_redesign_screens() {
    use egui_kittest::kittest::Queryable;
    std::fs::create_dir_all("target/qa/redesign").unwrap();
    for page in [
        Page::History,
        Page::Changes,
        Page::Snapshots,
        Page::Branches,
        Page::Merge,
        Page::Compare,
        Page::Stashes,
        Page::Conflicts,
        Page::Remotes,
        Page::Github,
        Page::Tools,
        Page::Settings,
    ] {
        let mut harness = screen(page, true);
        harness.run_steps(3);
        harness
            .render()
            .unwrap()
            .save(format!("target/qa/redesign/{page:?}.png"))
            .unwrap();
    }
    for page in [
        Page::History,
        Page::Changes,
        Page::Snapshots,
        Page::Branches,
        Page::Merge,
        Page::Compare,
        Page::Stashes,
        Page::Conflicts,
        Page::Remotes,
        Page::Github,
        Page::Tools,
        Page::Settings,
    ] {
        let mut compact = screen(page, true);
        compact.ctx.set_zoom_factor(2.);
        compact.run_steps(2);
        compact.set_size(egui::vec2(530., 350.));
        compact.run_steps(3);
        compact
            .render()
            .unwrap()
            .save(format!("target/qa/redesign/{page:?}-zoom.png"))
            .unwrap();
    }
    let mut menu = screen(Page::History, false);
    menu.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::B);
    menu.run_steps(3);
    menu.render()
        .unwrap()
        .save("target/qa/redesign/Branch-menu.png")
        .unwrap();
    let mut repo_menu = screen(Page::History, false);
    repo_menu.run_steps(3);
    let repo_label = format!("Current repository: {}", repo_menu.state().path().display());
    repo_menu.get_by_label(&repo_label).click();
    repo_menu.run_steps(3);
    repo_menu
        .render()
        .unwrap()
        .save("target/qa/redesign/Repository-menu.png")
        .unwrap();
    let mut wide = screen(Page::History, false);
    wide.state_mut().settings.history_columns.commit = 900.;
    wide.state_mut().inspector_open = false;
    wide.run_steps(3);
    wide.render()
        .unwrap()
        .save("target/qa/redesign/History-wide-columns.png")
        .unwrap();
    for (state, name) in [
        (1, "empty"),
        (3, "loading"),
        (4, "error"),
        (5, "long-labels"),
    ] {
        let mut sample = screen(Page::Changes, false);
        sample.state_mut().preview_state = state;
        sample.state_mut().load_preview_state();
        sample.state_mut().page = Page::Changes;
        sample.run_steps(3);
        sample
            .render()
            .unwrap()
            .save(format!("target/qa/redesign/Changes-{name}.png"))
            .unwrap();
    }
    let mut columns = screen(Page::History, false);
    columns.run_steps(3);
    columns.get_by_label("View").click();
    columns.run_steps(2);
    columns.get_by_label("Columns").click();
    columns.run_steps(3);
    columns
        .render()
        .unwrap()
        .save("target/qa/redesign/Columns-menu.png")
        .unwrap();
}

#[test]
fn history_graph_column_resizes_by_pointer_and_exposes_keyboard_width_controls() {
    use egui_kittest::kittest::Queryable;
    let mut harness = screen(Page::History, false);
    harness.run_steps(3);
    let handle = harness
        .ctx
        .read_response(egui::Id::unique(("history_column_resize", 0)))
        .expect("graph divider")
        .rect
        .center();
    let initial = harness.state().history_view.resolved_widths[0];
    harness.hover_at(handle);
    harness.run_steps(1);
    harness.drag_at(handle);
    harness.run_steps(1);
    harness.hover_at(handle + egui::vec2(40., 0.));
    harness.run_steps(2);
    harness.drop_at(handle + egui::vec2(40., 0.));
    harness.run_steps(3);
    assert!(harness.state().settings.history_columns.graph > initial + 20.);
    harness.get_by_label("View").click();
    harness.run_steps(2);
    harness.get_by_label("Columns").click();
    harness.run_steps(2);
    harness.check_accessibility();
    assert!(harness.query_by_label("Graph column width").is_some());
    harness
        .get_by_label("Graph column width")
        .set_value_accesskit(260.);
    harness.run_steps(2);
    assert_eq!(harness.state().settings.history_columns.graph, 260.);
    assert!(harness.query_by_label("Reset column widths").is_some());
    harness.get_by_label("Reset column widths").click();
    harness.run_steps(2);
    assert_eq!(harness.state().settings.history_columns.graph, 0.);
}
#[test]
fn history_graph_automatically_fits_more_than_nine_lanes() {
    let mut harness = screen(Page::History, false);
    let count = harness.state().repo.as_ref().unwrap().branch_commits.len();
    harness.state_mut().graph = std::sync::Arc::new(
        (0..count)
            .map(|_| graph::GraphRow {
                lane: 11,
                width: 12,
                edges: vec![(11, 11)],
            })
            .collect(),
    );
    harness.run_steps(3);
    assert!(harness.state().history_view.resolved_widths[0] >= 12. * 16. + 34.);
}

#[test]
fn branch_source_column_resizes_and_offers_numeric_controls() {
    use egui_kittest::kittest::Queryable;
    {
        let (page, id) = (Page::Branches, "branch_source_divider");
        let mut harness = screen(page, false);
        harness.run_steps(3);
        let handle = harness
            .ctx
            .read_response(egui::Id::unique(id))
            .expect("tree divider")
            .rect
            .center();
        let before = if page == Page::Branches {
            harness.state().settings.branch_source_width
        } else {
            harness.state().settings.snapshot_source_width
        };
        harness.hover_at(handle);
        harness.run_steps(1);
        harness.drag_at(handle);
        harness.run_steps(1);
        harness.hover_at(handle + egui::vec2(35., 0.));
        harness.run_steps(2);
        harness.drop_at(handle + egui::vec2(35., 0.));
        harness.run_steps(2);
        let after = if page == Page::Branches {
            harness.state().settings.branch_source_width
        } else {
            harness.state().settings.snapshot_source_width
        };
        assert!(after > before + 15., "{page:?}: {before} -> {after}");
        harness.get_by_label("Tree layout").click();
        harness.run_steps(2);
        harness.check_accessibility();
        assert!(
            harness
                .query_by_label(if page == Page::Branches {
                    "Ancestor column width"
                } else {
                    "Source column width"
                })
                .is_some()
        );
        harness.get_by_label("Reset tree layout").click();
        harness.run_steps(2);
        let reset = if page == Page::Branches {
            harness.state().settings.branch_source_width
        } else {
            harness.state().settings.snapshot_source_width
        };
        assert_eq!(reset, before);
    }
}
#[test]
fn commit_inspector_keeps_actions_visible_and_metadata_available() {
    use egui_kittest::kittest::Queryable;
    let mut harness = screen(Page::History, true);
    harness.run_steps(3);
    assert!(harness.query_by_label("View patch").is_some());
    assert!(harness.query_by_label("Open this commit in IDE…").is_some());
    assert!(harness.query_by_label("Copy full SHA").is_none());
    harness.get_by_label("Technical details").click();
    harness.run_steps(2);
    assert!(harness.query_by_label("Copy full SHA").is_some());
    harness.get_by_label("Commit actions…").click();
    harness.run_steps(2);
    assert!(harness.query_by_label("Revert this commit…").is_some());
}

#[test]
fn repository_and_branch_pickers_keep_navigation_actions() {
    use egui_kittest::kittest::Queryable;
    let mut harness = screen(Page::History, false);
    harness.run_steps(3);
    let name = format!("Current repository: {}", harness.state().path().display());
    harness.get_by_label(&name).click();
    harness.run_steps(2);
    assert!(harness.query_by_label("Browse repositories…").is_some());
    assert!(harness.query_by_label("Clone or initialize…").is_some());
    harness.get_by_label("Open a path").click();
    harness.run_steps(2);
    assert!(harness.query_by_label("Open directory").is_some());
    harness.key_press(egui::Key::Escape);
    harness.run_steps(2);
    harness.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::B);
    harness.run_steps(2);
    assert!(harness.query_by_label("New branch…").is_some());
    assert!(harness.query_by_label("View branch map").is_some());
    assert!(
        harness
            .query_by_label("origin/main — Remote-tracking · no local branch tracks it")
            .is_some()
    );
    harness.check_accessibility();
}

#[test]
fn compact_branch_map_keeps_tree_layout_and_sync_accessible() {
    use egui_kittest::kittest::Queryable;
    let mut harness = screen(Page::Branches, false);
    harness.ctx.set_zoom_factor(2.);
    harness.run_steps(2);
    harness.set_size(egui::vec2(530., 350.));
    harness.run_steps(3);
    let tree = harness.get_by_label("Tree layout").rect();
    assert!(
        tree.max.y < 325.,
        "Tree layout is clipped behind the status bar: {tree:?}"
    );
    harness.get_by_label("Sync & comparison").click();
    harness.run_steps(2);
    assert!(
        harness
            .query_by_label("Advanced branch operations")
            .is_some()
    );
    harness.check_accessibility();
}

#[test]
fn github_setup_exposes_next_steps_and_missing_cli_feedback() {
    use egui_kittest::kittest::Queryable;
    let mut harness = screen(Page::Github, false);
    harness.state_mut().github_available = false;
    harness.run_steps(3);
    assert!(
        harness
            .query_by_label("Open GitHub CLI installation instructions")
            .is_some()
    );
    assert!(harness.query_by_label("Check installation again").is_some());
    harness.state_mut().github_available = true;
    harness.run_steps(3);
    assert!(harness.query_by_label("Sign in to GitHub…").is_some());
    assert!(harness.query_by_label("Check account").is_some());
    harness.check_accessibility();
}

#[test]
#[ignore = "Exports focused screenshots; requires a graphics adapter."]
fn render_github_setup_and_branch_status() {
    std::fs::create_dir_all("target/qa/followup").unwrap();
    for (page, available, name) in [
        (Page::Branches, true, "branches"),
        (Page::Github, true, "github"),
        (Page::Github, false, "github-missing"),
    ] {
        let mut harness = screen(page, false);
        harness.state_mut().github_available = available;
        harness.run_steps(3);
        harness
            .render()
            .unwrap()
            .save(format!("target/qa/followup/{name}.png"))
            .unwrap();
    }
}

#[test]
fn command_bar_opens_with_shortcut_and_filters_actions() {
    use egui_kittest::kittest::Queryable;
    let mut harness = screen(Page::History, false);
    harness.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::K);
    harness.run_steps(2);
    assert!(harness.query_by_label("Open Time Machine").is_some());
    harness.state_mut().command_bar_query = "settings".into();
    harness.run_steps(2);
    assert!(harness.query_by_label("Open settings").is_some());
    assert!(harness.query_by_label("Open Time Machine").is_none());
    harness.get_by_label("Open settings").click();
    harness.run_steps(2);
    assert_eq!(harness.state().page, Page::Settings);
}

#[test]
fn conflict_banner_only_appears_for_conflicts_or_an_active_operation() {
    use egui_kittest::kittest::Queryable;
    let mut harness = screen(Page::History, false);
    harness.run_steps(2);
    assert!(harness.query_by_label("Resolve conflicts").is_none());
    let mut repo = harness.state().repo.as_ref().unwrap().as_ref().clone();
    repo.changes.push(git::Change {
        index: 'U',
        worktree: 'U',
        path: "conflict.txt".into(),
        original: None,
    });
    harness.state_mut().repo = Some(std::sync::Arc::new(repo));
    harness.run_steps(2);
    harness.get_by_label("Resolve conflicts").click();
    harness.run_steps(2);
    assert_eq!(harness.state().page, Page::Conflicts);
}

#[test]
#[ignore = "Exports theme and compact renders for visual review."]
fn render_phase_one_themes() {
    std::fs::create_dir_all("target/qa/ui").unwrap();
    let mut light = screen(Page::History, true);
    light.state_mut().settings.light_theme = true;
    theme::install(&light.ctx, true);
    light.run_steps(3);
    light
        .render()
        .unwrap()
        .save("target/qa/ui/History-light.png")
        .unwrap();
    let mut compact = screen(Page::History, true);
    compact.ctx.set_zoom_factor(2.);
    compact.set_size(egui::vec2(530., 350.));
    compact.run_steps(3);
    compact
        .render()
        .unwrap()
        .save("target/qa/ui/History-compact-current.png")
        .unwrap();
}

#[test]
#[ignore = "Exports the patch preview in both themes for visual review."]
fn render_patch_preview() {
    std::fs::create_dir_all("target/qa/ui").unwrap();
    for light in [false, true] {
        let mut harness = screen(Page::History, false);
        harness.state_mut().settings.light_theme = light;
        theme::install(&harness.ctx, light);
        harness.state_mut().detail_title = "Commit abc12345 · patch".into();
        harness.state_mut().detail = "Improve cache defaults\n\n src/cache.rs | 2 +-\n 1 file changed, 1 insertion(+), 1 deletion(-)\n\ndiff --git a/src/cache.rs b/src/cache.rs\nindex 1111111..2222222 100644\n--- a/src/cache.rs\n+++ b/src/cache.rs\n@@ -1 +1 @@\n-const CACHE_LIMIT: usize = 64;\n+const CACHE_LIMIT: usize = 128;\n".into();
        harness.state_mut().detail_page = Page::History;
        harness.state_mut().detail_open = true;
        harness.run_steps(3);
        harness
            .render()
            .unwrap()
            .save(format!(
                "target/qa/ui/patch-{}.png",
                if light { "light" } else { "dark" }
            ))
            .unwrap();
        if !light {
            harness.ctx.set_zoom_factor(2.);
            harness.set_size(egui::vec2(530., 350.));
            harness.run_steps(3);
            harness
                .render()
                .unwrap()
                .save("target/qa/ui/patch-compact.png")
                .unwrap();
        }
    }
}

#[test]
fn patch_preview_can_switch_between_formatted_and_raw() {
    use egui_kittest::kittest::Queryable;
    let mut harness = screen(Page::History, false);
    harness.state_mut().detail_title = "Commit abc · patch".into();
    harness.state_mut().detail =
        "diff --git a/a b/a\nindex abc..def 100644\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-old\n+new\n"
            .into();
    harness.state_mut().detail_page = Page::History;
    harness.state_mut().detail_open = true;
    harness.run_steps(3);
    harness.get_by_label("Show raw patch").click();
    harness.run_steps(2);
    assert!(harness.state().patch_raw);
    assert!(harness.query_by_label("Show formatted patch").is_some());
    harness.get_by_label("Show formatted patch").click();
    harness.run_steps(2);
    assert!(!harness.state().patch_raw);
    harness.get_by_label("Close preview").click();
    harness.run_steps(2);
    assert!(!harness.state().detail_open);
}

fn branch_pair_fixture() -> egui_kittest::Harness<'static, Branchroom> {
    let mut harness = screen(Page::Branches, false);
    let mut repo = harness.state().repo.as_ref().unwrap().as_ref().clone();
    repo.branch = "main".into();
    repo.head = "abc123".into();
    repo.base = "main".into();
    repo.sync_counts = Some((0, 0));
    repo.branches = vec![
        git::Branch {
            name: "main".into(),
            current: true,
            upstream: "origin/main".into(),
            id: "abc123".into(),
            fork_point: "abc123".into(),
            ..Default::default()
        },
        git::Branch {
            name: "origin/main".into(),
            remote: true,
            id: "abc123".into(),
            fork_point: "abc123".into(),
            ..Default::default()
        },
    ];
    harness.state_mut().repo = Some(std::sync::Arc::new(repo));
    harness.state_mut().branch_filter.clear();
    harness
}

#[test]
fn branch_pair_has_one_accessible_card_and_keeps_remote_actions() {
    use egui_kittest::kittest::Queryable;
    let mut harness = branch_pair_fixture();
    harness.run_steps(3);
    assert!(harness.query_by_label("main  · current, This computer and remote match, Remote: origin/main · as of last fetch").is_some());
    assert!(harness.query_by_label("origin/main, This branch is only on the remote, Switch to it to create a branch on this computer").is_none());
    harness.get_by_label("Actions").click();
    harness.run_steps(2);
    harness.get_by_label("Remote branch: origin/main").click();
    harness.run_steps(2);
    assert!(harness.query_by_label("View remote commit").is_some());
    assert!(harness.query_by_label("Switch to remote branch…").is_some());
    harness.check_accessibility();
}

#[test]
#[ignore = "Exports a branch pair for visual review."]
fn render_branch_pair() {
    std::fs::create_dir_all("target/qa/ui").unwrap();
    for light in [false, true] {
        let mut harness = branch_pair_fixture();
        harness.state_mut().settings.light_theme = light;
        theme::install(&harness.ctx, light);
        harness.run_steps(3);
        harness
            .render()
            .unwrap()
            .save(format!(
                "target/qa/ui/branch-pair-{}.png",
                if light { "light" } else { "dark" }
            ))
            .unwrap();
    }
}

#[test]
fn operation_spinner_fits_in_the_status_bar() {
    use egui_kittest::kittest::Queryable;
    let mut harness = screen(Page::History, false);
    let (_tx, rx) = mpsc::channel();
    harness.state_mut().receiver = Some(rx);
    harness.state_mut().busy = "Running operation…".into();
    harness.run_steps(3);
    let spinner = harness.get_by_label("Operation in progress").rect();
    let cancel = harness.get_by_label("Cancel operation").rect();
    assert!((spinner.width() - 14.).abs() < 1.);
    assert!((spinner.height() - 14.).abs() < 1.);
    assert!((spinner.center().y - cancel.center().y).abs() < 3.);
}

#[test]
fn repository_menu_filters_recent_paths_without_hiding_current_repository() {
    use egui_kittest::kittest::Queryable;
    let mut harness = screen(Page::History, false);
    let current = harness.state().path();
    harness.state_mut().settings.recent = vec![
        current.clone(),
        PathBuf::from("C:/projects/sample-rust-project"),
        PathBuf::from("C:/projects/sample-web-project"),
    ];
    harness.run_steps(3);
    harness
        .get_by_label(&format!("Current repository: {}", current.display()))
        .click();
    harness.run_steps(2);
    harness.state_mut().repository_filter = "rust".into();
    harness.run_steps(2);
    assert!(
        harness
            .query_by_label("sample-rust-project — C:/projects/sample-rust-project")
            .is_some()
    );
    assert!(
        harness
            .query_by_label("sample-web-project — C:/projects/sample-web-project")
            .is_none()
    );
    assert!(
        harness
            .query_by_label(&format!(
                "{} — {}",
                current.file_name().unwrap().to_string_lossy(),
                current.display()
            ))
            .is_some()
    );
}

#[test]
fn fetch_completion_updates_freshness_only_on_success() {
    let mut harness = screen(Page::History, false);
    for success in [true, false] {
        let (tx, rx) = mpsc::channel();
        let app = harness.state_mut();
        app.fetching = true;
        app.receiver = Some(rx);
        tx.send(Completed {
            output: if success {
                Ok(String::new())
            } else {
                Err("Connection unavailable".into())
            },
            state: None,
            detail: false,
            open: None,
            data: JobData::None,
        })
        .unwrap();
        let previous = app.last_fetch;
        app.poll();
        assert!(!app.fetching);
        assert_eq!(app.fetch_error, !success);
        if success {
            assert!(app.last_fetch.is_some());
        } else {
            assert_eq!(app.last_fetch, previous);
        }
    }
}

#[test]
fn preview_inspector_does_not_attempt_repository_commands() {
    let mut harness = screen(Page::History, true);
    harness.state_mut().inspector_open = true;
    harness.run_steps(3);
    assert!(!harness.state().error);
    assert!(harness.state().receiver.is_none());
}

#[test]
fn shell_actions_remain_inside_a_narrow_window() {
    use egui_kittest::kittest::Queryable;
    let mut harness = screen(Page::History, false);
    harness.set_size(egui::vec2(320., 700.));
    harness.run_steps(3);
    for name in ["Modes", "Sync", "Open in IDE", "Activity log"] {
        let rect = harness.get_by_label(name).rect();
        assert!(rect.min.x >= 0. && rect.max.x <= 320., "{name}: {rect:?}");
    }
}

#[test]
fn branch_map_selection_is_retained_in_commit_view() {
    use egui_kittest::kittest::Queryable;
    let mut harness = branch_pair_fixture();
    harness.run_steps(3);
    harness.get_by_label("main  · current, This computer and remote match, Remote: origin/main · as of last fetch").click();
    harness.run_steps(2);
    assert_eq!(harness.state().selected_commit, "abc123");
    assert!(harness.state().inspector_open);
    harness.state_mut().page = Page::History;
    harness.run_steps(2);
    assert_eq!(harness.state().selected_commit, "abc123");
}

#[test]
fn twelve_thousand_commits_render_as_virtual_rows() {
    use egui_kittest::kittest::Queryable;
    let mut harness = screen(Page::History, false);
    let mut repo = harness.state().repo.as_ref().unwrap().as_ref().clone();
    repo.commits = (0..12000)
        .map(|i| git::Commit {
            id: format!("{i:040x}"),
            parents: if i < 11999 {
                vec![format!("{:040x}", i + 1)]
            } else {
                vec![]
            },
            subject: format!("Virtual commit {i}"),
            author: "Test author".into(),
            date: "now".into(),
            ..Default::default()
        })
        .collect();
    harness.state_mut().graph = std::sync::Arc::new(graph::layout(&repo.commits));
    harness.state_mut().repo = Some(std::sync::Arc::new(repo));
    harness.run_steps(3);
    assert!(
        harness
            .query_by_label("Virtual commit 0 — Test author — now")
            .is_some()
    );
    assert!(
        harness
            .query_by_label("Virtual commit 11999 — Test author — now")
            .is_none()
    );
    assert!(!harness.state().error);
}

#[test]
#[ignore = "Exports Phase 2 graph lenses for visual verification"]
fn render_phase_two_lenses() {
    std::fs::create_dir_all("target/qa/phase-two").unwrap();
    for (page, name) in [
        (Page::History, "commits"),
        (Page::Branches, "branches"),
        (Page::Compare, "compare"),
    ] {
        for (light, width, height) in [
            (false, 1320., 820.),
            (true, 1320., 820.),
            (false, 530., 700.),
        ] {
            let mut harness = screen(page, true);
            harness.state_mut().settings.light_theme = light;
            theme::install(&harness.ctx, light);
            harness.set_size(egui::vec2(width, height));
            harness.run_steps(3);
            harness
                .render()
                .unwrap()
                .save(format!("target/qa/phase-two/{name}-{light}-{width}.png"))
                .unwrap();
        }
    }
}

#[test]
fn numstat_preserves_unicode_tabs_and_newlines_in_file_paths() {
    let files = CommitFile::parse("2\t3\tdocs/Release notes.md\0-\t-\timages/日本\t語\n.png\0");
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].path, "docs/Release notes.md");
    assert_eq!(files[1].path, "images/日本\t語\n.png");
    assert!(files[1].label().ends_with("Binary file"));
}

#[test]
fn dragging_a_commit_onto_a_branch_opens_a_review_without_executing() {
    use egui_kittest::kittest::Queryable;
    let mut harness = screen(Page::History, false);
    let mut repo = harness.state().repo.as_ref().unwrap().as_ref().clone();
    repo.changes.clear();
    harness.state_mut().repo = Some(std::sync::Arc::new(repo));
    harness.run_steps(3);
    let source = harness
        .get_by_label(
            "Keep notes available without a network connection — Sample Developer — 2 hours ago",
        )
        .rect()
        .center();
    let target = harness.get_by_label("main").rect().center();
    harness.hover_at(source);
    harness.run_steps(1);
    harness.drag_at(source);
    harness.run_steps(1);
    harness.hover_at(target);
    harness.run_steps(2);
    harness.drop_at(target);
    harness.run_steps(2);
    assert!(
        matches!(&harness.state().manipulation.draft,Some(branchroom::operation::Kind::CherryPick{target,..}) if target=="refs/heads/main")
    );
    assert!(harness.state().receiver.is_none());
    assert!(harness.query_by_label("Confirm action").is_some());
}
#[test]
fn escape_cancels_a_graph_draft_and_repository_switch_discards_it() {
    let mut harness = screen(Page::History, false);
    harness
        .state_mut()
        .begin_graph_action(branchroom::operation::Kind::Commit {
            message: String::new(),
        });
    harness.run_steps(2);
    harness.key_press(egui::Key::Escape);
    harness.run_steps(2);
    assert!(harness.state().manipulation.draft.is_none());
    harness
        .state_mut()
        .begin_graph_action(branchroom::operation::Kind::Commit {
            message: String::new(),
        });
    let mut repo = harness.state().repo.as_ref().unwrap().as_ref().clone();
    repo.root = repo.root.join("another-repository");
    harness.state_mut().repo = Some(std::sync::Arc::new(repo));
    harness.run_steps(2);
    assert!(harness.state().manipulation.draft.is_none());
}

struct GraphActionFixture {
    root: PathBuf,
    state: branchroom::settings::StateDirectory,
}
impl GraphActionFixture {
    fn new(conflict: bool) -> Self {
        let id = branchroom::snapshot::unique_id();
        let root = std::env::temp_dir().join(format!("branchroom-ui-three-{id}"));
        let state = branchroom::settings::StateDirectory::enter_isolated(
            std::env::temp_dir().join(format!("branchroom-ui-three-state-{id}")),
        );
        std::fs::create_dir(&root).unwrap();
        for args in [
            vec!["init", "-b", "main"],
            vec!["config", "user.name", "Preview tests"],
            vec!["config", "user.email", "preview@example.invalid"],
            vec!["config", "commit.gpgsign", "false"],
            vec!["config", "core.autocrlf", "false"],
        ] {
            git::git(&root, &args).unwrap();
        }
        std::fs::write(root.join("shared.txt"), "Base version\n").unwrap();
        git::git(&root, &["add", "-A"]).unwrap();
        git::git(&root, &["commit", "-m", "Base point"]).unwrap();
        git::git(&root, &["switch", "-c", "incoming"]).unwrap();
        std::fs::write(
            root.join(if conflict {
                "shared.txt"
            } else {
                "incoming.txt"
            }),
            "Incoming version\n",
        )
        .unwrap();
        git::git(&root, &["add", "-A"]).unwrap();
        git::git(&root, &["commit", "-m", "Incoming change"]).unwrap();
        git::git(&root, &["switch", "main"]).unwrap();
        std::fs::write(
            root.join(if conflict { "shared.txt" } else { "main.txt" }),
            "Current version\n",
        )
        .unwrap();
        git::git(&root, &["add", "-A"]).unwrap();
        git::git(&root, &["commit", "-m", "Current change"]).unwrap();
        Self { root, state }
    }
    fn harness(
        &self,
        review: Option<branchroom::operation::Preview>,
        resolver: bool,
    ) -> Harness<'static, Branchroom> {
        let root = self.root.clone();
        Harness::builder()
            .with_size(egui::vec2(1320., 820.))
            .with_accessibility_check(false)
            .build_eframe(move |cc| {
                let mut app = Branchroom::with_context(&cc.egui_ctx, true);
                let repo = git::load(&root, "main", 100).unwrap();
                app.graph = std::sync::Arc::new(graph::layout(&repo.commits));
                app.selected_commit = repo.head.clone();
                app.repo = Some(std::sync::Arc::new(repo));
                app.page = if resolver {
                    Page::Conflicts
                } else {
                    Page::History
                };
                if let Some(review) = review {
                    app.accept_graph_review(review);
                }
                if resolver {
                    app.conflict_path = "shared.txt".into();
                    app.conflict_original =
                        std::fs::read_to_string(root.join("shared.txt")).unwrap();
                    app.conflict_content = "Merged current and incoming versions\n".into();
                    app.conflict_versions = std::array::from_fn(|i| {
                        git::git(&root, &["show", &format!(":{}:shared.txt", i + 1)]).unwrap()
                    });
                }
                app
            })
    }
    fn review(&self) -> branchroom::operation::Preview {
        branchroom::operation::preview(
            &self.root,
            branchroom::operation::Kind::Merge {
                source: "incoming".into(),
                target: "main".into(),
            },
        )
        .unwrap()
    }
}
impl Drop for GraphActionFixture {
    fn drop(&mut self) {
        let state = self.state.path().to_path_buf();
        self.state.restore();
        for path in [&self.root, &state] {
            if path.parent() == Some(std::env::temp_dir().as_path())
                && path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("branchroom-ui-three-"))
            {
                let _ = std::fs::remove_dir_all(path);
            }
        }
    }
}
#[test]
fn graph_review_has_keyboard_controls_and_cancel_stays_visible_at_zoom() {
    use egui_kittest::kittest::Queryable;
    let fixture = GraphActionFixture::new(false);
    let mut harness = fixture.harness(Some(fixture.review()), false);
    harness.ctx.set_zoom_factor(2.);
    harness.run_steps(2);
    harness.set_size(egui::vec2(530., 350.));
    harness.run_steps(3);
    assert!((harness.ctx.content_rect().height() - 350.).abs() < 1.);
    assert!(harness.query_by_label("Confirm action").is_some());
    let cancel = harness.get_by_label("Cancel preview");
    assert!(
        cancel.rect().max.y <= 350.,
        "Cancel is clipped: {:?}",
        cancel.rect()
    );
    assert!(
        harness.unnamed_widgets().is_empty(),
        "{:?}",
        harness.unnamed_widgets()
    );
    harness.get_by_label("Cancel preview").click();
    harness.run_steps(2);
    assert!(!harness.state().manipulation.active());
}
#[test]
fn raw_advanced_flags_are_not_silently_changed_by_graph_routing() {
    let harness = screen(Page::History, false);
    for args in [
        vec!["cherry-pick", "--no-commit", "abc"],
        vec!["merge", "--no-ff", "incoming"],
        vec!["merge", "one", "two"],
        vec!["reset", "--hard", "HEAD~1"],
    ] {
        assert!(
            harness
                .state()
                .reviewed_graph_kind(&Task::Command {
                    program: "git".into(),
                    args: process::strings(&args),
                    refresh: true
                })
                .is_none()
        );
    }
}
#[test]
#[ignore = "Exports real graph forecasts and the conflict resolver for visual QA"]
fn render_phase_three_previews() {
    std::fs::create_dir_all("target/qa/phase-three").unwrap();
    for conflict in [false, true] {
        let fixture = GraphActionFixture::new(conflict);
        let review = fixture.review();
        for (light, width, height) in [
            (false, 1320., 820.),
            (true, 1320., 820.),
            (false, 530., 700.),
        ] {
            let mut harness = fixture.harness(Some(review.clone()), false);
            harness.state_mut().settings.light_theme = light;
            theme::install(&harness.ctx, light);
            harness.set_size(egui::vec2(width, height));
            harness.run_steps(3);
            harness
                .render()
                .unwrap()
                .save(format!(
                    "target/qa/phase-three/forecast-{conflict}-{light}-{width}.png"
                ))
                .unwrap();
        }
        if conflict {
            assert!(git::git(&fixture.root, &["merge", "--no-edit", "incoming"]).is_err());
            for (width, height) in [(1320., 820.), (530., 700.)] {
                let mut harness = fixture.harness(None, true);
                harness.set_size(egui::vec2(width, height));
                harness.run_steps(3);
                harness
                    .render()
                    .unwrap()
                    .save(format!("target/qa/phase-three/resolver-{width}.png"))
                    .unwrap();
            }
        }
    }
}

#[test]
fn time_machine_replaces_snapshot_modes_and_keeps_recovery_actions_named() {
    use egui_kittest::kittest::Queryable;
    let mut harness = screen(Page::Snapshots, true);
    harness.run_steps(3);
    for label in [
        "Save safety point",
        "Undo last action",
        "Redo action",
        "Shelve work",
        "Restore…",
        "Compare",
        "Branch from here…",
        "Delete entry",
    ] {
        assert!(harness.query_by_label(label).is_some(), "Missing {label}");
    }
    for old in [
        "Tree",
        "List",
        "Inspector",
        "Removed snapshots",
        "Tree layout",
        "Compare versions",
        "Saved points",
    ] {
        assert!(
            harness.query_by_label(old).is_none(),
            "Old snapshot UI remains: {old}"
        );
    }
}
#[test]
fn shelved_work_has_inline_diff_and_explicit_actions_at_narrow_width() {
    use egui_kittest::kittest::Queryable;
    let mut harness = screen(Page::Snapshots, false);
    let item = branchroom::timeline::Item {
        key: "shelf:test".into(),
        kind: "Shelved work".into(),
        label: "Finish offline cache".into(),
        reference: "stash@{0}".into(),
        target: "saved".into(),
        ..Default::default()
    };
    std::sync::Arc::make_mut(harness.state_mut().repo.as_mut().unwrap()).timeline =
        vec![item.clone()];
    harness.state_mut().time_machine.selected = item.key;
    harness.state_mut().time_machine.patch = "-old line\n+saved line".into();
    harness.set_size(egui::vec2(530., 700.));
    harness.run_steps(3);
    for label in ["Apply", "Apply and keep", "Delete", "Saved changes"] {
        assert!(harness.query_by_label(label).is_some(), "Missing {label}");
    }
    assert!(harness.query_by_label("More actions").is_none());
    harness.check_accessibility();
}
#[test]
#[ignore = "Exports Time Machine timeline, restore and shelved work in both themes and narrow windows"]
fn render_phase_four_timeline() {
    use branchroom::timeline;
    std::fs::create_dir_all("target/qa/phase-four").unwrap();
    let fixture = GraphActionFixture::new(false);
    let entry = timeline::begin(&fixture.root, "Commit offline cache").unwrap();
    std::fs::write(fixture.root.join("cache.txt"), "Offline cache\n").unwrap();
    git::git(&fixture.root, &["add", "cache.txt"]).unwrap();
    git::git(&fixture.root, &["commit", "-m", "Add offline cache"]).unwrap();
    timeline::finish(&fixture.root, entry, true).unwrap();
    std::fs::write(
        fixture.root.join("cache.txt"),
        "Saved offline improvements\n",
    )
    .unwrap();
    git::git(
        &fixture.root,
        &["stash", "push", "-m", "Finish offline cache"],
    )
    .unwrap();
    snapshot::create(&fixture.root, "Before cache experiment").unwrap();
    let items = timeline::items(&fixture.root, 100).unwrap();
    for mode in ["timeline", "restore", "shelved"] {
        for (light, width, height) in [
            (false, 1320., 820.),
            (true, 1320., 820.),
            (false, 530., 700.),
        ] {
            let mut harness = fixture.harness(None, false);
            harness.state_mut().page = Page::Snapshots;
            let item = items
                .iter()
                .find(|i| {
                    i.kind
                        == if mode == "shelved" {
                            "Shelved work"
                        } else {
                            "Action"
                        }
                })
                .unwrap()
                .clone();
            harness.state_mut().time_machine.selected = item.key.clone();
            harness.state_mut().time_machine.graph = if item.kind == "Action" {
                let state = timeline::entries(&fixture.root)
                    .unwrap()
                    .into_iter()
                    .find(|e| e.id == item.action)
                    .unwrap()
                    .after
                    .unwrap();
                timeline::state_graph(&fixture.root, &state).unwrap()
            } else {
                timeline::graph(&fixture.root, &item.target).unwrap()
            };
            if mode == "shelved" {
                harness.state_mut().time_machine.patch = git::git(
                    &fixture.root,
                    &["stash", "show", "--include-untracked", "-p", &item.target],
                )
                .unwrap();
            }
            if mode == "restore" {
                harness.state_mut().time_machine.plan = Some(Box::new(
                    timeline::plan_restore(&fixture.root, &item, true).unwrap(),
                ));
            }
            harness.state_mut().settings.light_theme = light;
            theme::install(&harness.ctx, light);
            harness.set_size(egui::vec2(width, height));
            harness.run_steps(3);
            harness
                .render()
                .unwrap()
                .save(format!("target/qa/phase-four/{mode}-{light}-{width}.png"))
                .unwrap();
        }
    }
}

#[test]
fn timeline_restore_controls_remain_visible_at_two_hundred_percent_zoom() {
    use egui_kittest::kittest::Queryable;
    let fixture = GraphActionFixture::new(false);
    let item = branchroom::timeline::items(&fixture.root, 100)
        .unwrap()
        .into_iter()
        .find(|i| i.kind == "Commit")
        .unwrap();
    let plan = branchroom::timeline::plan_restore(&fixture.root, &item, false).unwrap();
    let mut harness = fixture.harness(None, false);
    harness.state_mut().page = Page::Snapshots;
    harness.state_mut().time_machine.plan = Some(Box::new(plan));
    harness.ctx.set_zoom_factor(2.);
    harness.run_steps(2);
    harness.set_size(egui::vec2(530., 350.));
    harness.run_steps(3);
    for label in ["Confirm restore", "Cancel restore"] {
        let rect = harness.get_by_label(label).rect();
        assert!(
            rect.max.x <= 530. && rect.max.y <= 306.,
            "Clipped {label}: {rect:?}"
        );
    }
}
#[test]
fn global_undo_shortcut_does_not_consume_text_editor_undo() {
    let mut harness = screen(Page::Changes, false);
    harness.run_steps(2);
    harness.state_mut().commit_message = "A message".into();
    harness
        .ctx
        .memory_mut(|m| m.request_focus(egui::Id::unique("commit_summary")));
    harness.run_steps(2);
    let output = harness.state().output.clone();
    harness.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::Z);
    harness.run_steps(2);
    assert_eq!(harness.state().output, output);
    assert!(harness.state().receiver.is_none());
}
