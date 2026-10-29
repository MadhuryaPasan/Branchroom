# Phase 2: Graph workspace

The graph workspace has three lenses: Commits, Branch map and Compare. They retain the same selected commit, selected file and inspector. Advanced merge, history rewrite and recovery forms remain reachable from Advanced.

## Changes

- Commits uses repository-wide history, including other branch tips, rather than only ancestors of the checked-out branch. Its rows remain virtualized. View contains graph zoom, list fallback, inspector, column widths and density. Mouse dragging and scroll bars pan the graph. Lane-colored branch labels select their tips. Author initials, relative dates, branch/tag decorations and available PR/CI information accompany commit messages. Hover updates the inspector without an overlapping commit tooltip.
- The working-copy node shows the number of changed files and opens the commit composer. The inspector automatically requests changed files and their added/removed counts; selecting a file updates the shared file selection. Patch, IDE, cherry-pick, revert and existing history actions remain available.
- Author names are canonicalized by email and Git mailmap. GitHub metadata loads in background workers, refreshes periodically, and looks up related PRs after a short selection debounce. Identity differences are reported when GitHub exposes an account email. Missing CI data is reported as unavailable rather than success.
- Branch map groups branches at their actual starting commits and pairs explicitly tracked local/remote branches. Colored connections, comparison bars, tracking status, PR state, stale labels and merged-branch highlighting expose branch state. Ambiguous/unlinked remote names remain separate rather than implying a tracking relationship that Git has not configured.
- Clean up branches offers merged candidates. The backend rechecks the reviewed tip, refuses the current branch and unmerged commits, creates a recovery checkpoint, and lets Git refuse branches checked out in another parallel workspace. Checkpoints are available in Time Machine, where a recovery branch can be created.
- Compare loads its actual starting point, unique commits on each side and file statistics asynchronously. Unrelated histories remain disconnected. Lists are virtualized; commit and file selections feed the shared inspector and file selection. Advanced preserves the original raw comparison and rewrite controls.
- Plain-text search covers messages, author names, branch/tag labels, IDs and relative dates. Structured filters support `file:`, `author:`, `branch:`, `after:` and `before:`. Quote values containing spaces, for example `file:"docs/Release notes.md" after:2026-01-01`. File/date queries run outside the render thread and cached ID membership uses a hash set. Inspector and Compare share a NUL-delimited file-statistics parser so spaces, Unicode, tabs and newlines in paths are preserved.

## Files

Core graph work: `src/ui/history.rs`, `src/ui/workspace.rs`, `src/ui/compare.rs`, `src/ui/comparison_canvas.rs`, `src/ui/graph_context.rs`, `src/ui/shell.rs`, `src/ui/mod.rs`, `src/ui/jobs.rs`, `src/ui/preview.rs`.

Safety and recovery: `src/collaboration.rs`, `src/ui/experiments.rs`.

Author canonicalization: `src/git.rs`.

Verification: `src/ui/qa.rs`, `tests/repository.rs`.

## Verification

Run:

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --all-features --locked
cargo test --all-features --bin branchroom render_phase_two_lenses --locked -- --ignored --nocapture
```

Verification outcome: 1 library test, 41 application tests and 61 repository tests pass (103 total). Eight screenshot exporters are ignored in the ordinary suite; the Phase 2 exporter was run explicitly and produced nine renders. Formatting and Clippy pass.

The framework suite includes shared selection, accessible controls, minimum-size/200% zoom, column resizing and a 12,000-commit virtualization fixture. Repository tests cover cleanup checkpoint retention, stale-tip refusal, current-branch refusal and preservation of unmerged work. Framework screenshots cover all three lenses in dark, light and narrow layouts; exports are under `target/qa/phase-two`.

Live GitHub account responses and native screen-reader behavior are not certified by the fixture tests. PR/CI information requires GitHub CLI access and remains explicitly unavailable when the service cannot be read. The graph displays loaded history; use Load more to extend it.

## Manual test

1. Run `cargo run`, open a repository with multiple branches and select a branch or commit. Switch between all three lenses and check that its selection remains.
2. Open View, change zoom, pan, and resize the window. Inspect a long commit message and branch name; full metadata remains in the inspector.
3. Search by message or author, then try `branch:main`, `file:README.md`, and a date range. Select a changed file and open its patch.
4. Choose two diverged branches in Compare. Confirm unique-commit lists and file counts against `git log branchA..branchB` and `git diff --numstat branchA branchB`.
5. In a disposable repository, clean up a merged branch. Open Time Machine, expand History checkpoints and create a recovery branch. Verify its commit matches the deleted tip. An unmerged branch must remain intact.
6. With GitHub CLI connected, inspect a PR branch and a commit contained in a PR. Review the PR link and reported CI state. Disconnecting must not manufacture a successful CI result.
