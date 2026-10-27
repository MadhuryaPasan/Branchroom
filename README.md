# Branchroom

A native Rust Git workspace for Windows, with branch history, pull requests, and independent project snapshots. Version **0.2.0-rc.1** is a release candidate. The [production audit](docs/PRODUCTION-AUDIT.md) records what was tested, what changed, and the remaining release gates. The [migration matrix](docs/GITHUB-DESKTOP-PARITY.md) distinguishes dedicated workflows from command-runner coverage and missing features.

## Run on Windows

For a packaged build, extract the ZIP and run `branchroom.exe`. Keep the included documentation and manifest. Git is an external requirement; GitHub CLI is required only for GitHub features. No Node runtime or web server is needed.

- [Git for Windows](https://git-scm.com/downloads/win), **2.41 or later**. Branch comparisons use Git's batched ahead/behind information.
- Optional [GitHub CLI](https://cli.github.com/) (`gh`) and an authenticated GitHub or Enterprise account. Sign in through the user-controlled browser/device flow. Branchroom does not store a second copy of your access token.
- Your chosen IDE and project runtimes, installed separately.

The app searches PATH, standard Windows Git/GitHub CLI locations, and an optional `tools` directory beside its executable. Settings → About shows the discovered tools and version. GitHub screens offer installation/recheck guidance when `gh` is missing. Use Git/GCM/SSH configuration for network authentication.

To run from source, install [Rust MSVC](https://rustup.rs/) **1.95+** and Visual Studio Build Tools with Desktop development with C++ and a Windows SDK:

```powershell
cargo run --locked
cargo run --locked -- "C:\Projects\my-project"
```

## Build and verify

```powershell
./scripts/Build-Windows.ps1
```

The script checks formatting, strict Clippy, and all-feature tests, then builds the standard release executable. It creates a unique versioned directory and ZIP under `dist`, a SHA-256 manifest, and `dist/latest.json` pointing to the exact build. It also updates `dist/branchroom.exe` when that file is not in use. It never terminates an older running app. `-SkipChecks` is intended only after the same checks have already passed for the current source.

For individual steps:

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --all-features --locked
cargo build --release --locked
.\target\release\branchroom.exe
```

The executable is unsigned. A signed installer, auto-updater, native screen-reader sign-off, and live authenticated GitHub acceptance are release gates. CI is configured for Windows, Linux, and macOS; only Windows was exercised locally. Linux needs the native X11/Wayland/OpenGL development libraries listed in the workflow.

## Everyday workflows

| Area | What you can do |
| --- | --- |
| Current branch, in the top bar | Search branches on this PC and last-fetched remote branches; reuse tracking branches or create one; carry or stash changes before switching |
| Changes | Filter/tree/list files; stage files, hunks, or lines; aligned split/unified diffs; whitespace/full context; image comparison; commit, amend, co-authors; local commit exclusions |
| History | Current-branch-only parent graph/list; search and pagination; inspect changed files; open an exact commit in a separate IDE worktree; revert, cherry-pick, branch, tag |
| Snapshots | Named captures in a source tree; compare A/B; independent worktrees and IDEs; promote to a branch; remove and restore captures |
| Pull requests | Browse and inspect, checks/diff, independent IDE review; create draft/ready PRs with templates, reviewer logins, labels; comment/approve/request changes; ready/close/reopen; merge/squash/rebase after review |
| Merge and branch map | Source/destination preview; fetch and merge baseline updates; ahead/behind/diverged state and shared ancestors; create/rename/delete branches |
| Compare & recovery | Branch/commit comparisons; undo/reset/squash/reorder restricted unpublished history; saved checkpoints; recover discarded working files |
| Conflicts and stashes | Base/current/incoming/result editor; stage a checked resolution; continue/abort/skip operations; stash inspect/apply/pop/drop |
| Repositories | Clone/init; remotes; publish/push/pull/fetch; clean worktree creation/removal; default or project-specific IDE |
| Command center | Reviewed noninteractive Git/gh arguments, reflog, blame, tags, submodule and maintenance actions |
| Settings | Density, zoom, auto-refresh, project baseline, default/project/custom IDEs, tool discovery, version diagnostics |

At narrow widths or high zoom, navigation moves to **Workspace**, inspectors become windows, and **Changes** has Files / Diff / Commit tabs. Selecting a file opens Diff. In a wide window, files, composer, and diff remain visible together. Secondary Changes actions, including commit locks, live in **Actions**.

Keyboard: **Ctrl+O** open, **Ctrl+R** refresh, **Ctrl+B** branches, **Ctrl+1/2/3** Changes/History/Snapshots, **Ctrl+G** commit summary, **Ctrl+Enter** review commit, **Ctrl+K** commands, **Escape** dismiss. Tab reaches native controls. The status bar provides zoom and the activity log.

Remote status is a last-fetched observation. A remote-tracking ref is not proof of a live server connection. Network operations are explicit; real Git progress is shown when available, with cancellation and an activity log. Local auto-refresh runs at idle and can be disabled.

## Snapshots and IDEs

1. Capture working files with a descriptive name. The capture uses a private index, includes non-ignored untracked files, and preserves the real index and HEAD. Commit locks do not exclude a file from snapshots.
2. Select captures as A and B and compare. Source nodes show ancestry; captures of the same source are siblings, not a fabricated timeline.
3. Open each capture in its own new directory, choosing the default or a different configured IDE. These are independent Git worktrees. Dependencies, run commands, and environment setup remain project-specific; Branchroom does not automatically execute project code.
4. Keep an experiment by promoting it to a new branch. This does not overwrite the active checkout.
5. Use the inspector or card menu to **Remove snapshot**. It moves to **Removed snapshots** and can be restored. Existing branches, files, and open worktrees are preserved. Internal references and commit IDs are under Technical details.

Snapshots and checkpoints are local Git refs and are not pushed by normal branch publication. A snapshot is not an off-device backup, does not capture ignored files, and does not recursively capture another repository's objects. Use a separate backup for those needs.

IDE settings accept an executable and argument array; `{path}` is replaced with the project/worktree directory. Configure a global default or project override, including cloned projects. Review worktrees are detached: create a branch there before keeping new commits.

## Commit exclusions and recovery

**Changes → Actions → Commit locks** manages tracked or untracked file exclusions, even when a file is unchanged. A file menu also offers Exclude from commits (lock). Locking unstages it without restoring working content. Inclusion skips locked paths, partial staging is guarded, and normal commits refuse externally staged locked paths. Unlock to include changes again.

Locks persist in local `.git/config` under `branchroom.commitLock`; linked worktrees share them. They are not shared with collaborators. Other Git clients, custom command overrides, hooks, snapshots/stashes, and merge/revert operations remain independent. These are normal Branchroom commit exclusions, not filesystem locks. Branchroom does not misuse assume-unchanged/skip-worktree as a commit lock.

A commit is prepared in the background, then reviewed with its branch, message, and included file list. Execution checks HEAD and the index tree again. Draft commit/PR text is saved per project and branch, including before switching and on exit. Corrupt settings are preserved and saving reports an actionable error.

**Revert** creates an inverse commit and preserves later work. Merge reverts require a mainline parent and explain the consequences. **Reset/undo/squash/reorder** are separate, restricted to clean, idle, linear unpublished history (up to 200 commits), checked against local remote-tracking refs. Fetch first if publication status may be stale. Successful rewrites keep recovery refs. Reorder uses a private index and refuses conflicts or changed final content.

Discard recovery stores a working-file copy in app data. Restoring it leaves the index unchanged and first saves any newer working content it would replace. Copies have no automatic expiration. Conflict saves reject changed-on-disk content, unresolved conflict markers, symlink paths, and commit-locked files before atomically replacing and staging a result.

## GitHub

Install `gh`, sign in, then reload PRs. Creation requires an explicitly chosen base and local compare branch with a matching remote upstream; publish/sync/fetch first. Templates are loaded from conventional repository locations. Reviewer logins and label names are entered as comma-separated values, not an account search picker.

The review screen supports summary reviews, comments, checks, and merge actions. Merge requests pin the inspected PR head with `--match-head-commit`; GitHub still applies permissions, checks, rulesets, and merge-queue behavior. No administrator bypass is offered. Inline review threads, full GitHub issue/project editors, account-based clone selection, and detailed ruleset guidance remain outside this release. See the migration matrix.

## Adjusting the trees

In History, drag a column boundary in the header to resize the graph, commit, author, or date column. Open **Columns** to enter widths with the keyboard or reset them. The branch map and snapshot tree also have draggable source-column boundaries and a **Tree layout** menu. Widths are saved in your preferences. Wide history tables can be scrolled horizontally; at high zoom the author is available in the commit details and accessible row label.

## Architecture

- `git.rs`, `graph.rs`: repository snapshots, NUL-delimited machine parsing, batched branch statistics and bounded ancestry cache, parent relationships.
- `process.rs`, `safety.rs`: argument-based subprocesses, bounded output, progress, cancellation, tool discovery, protected project policy.
- `commit.rs`, `history.rs`, `diff.rs`, `commit_lock.rs`: reviewed commits, restricted rewrites, tested partial patches, exclusions.
- `snapshot.rs`, `collaboration.rs`, `github.rs`: private captures/worktrees, branch integration/reverts, typed GitHub actions.
- `storage.rs`, `recovery.rs`, `conflict.rs`, `settings.rs`: atomic replacement, recovery copies, checked conflict saves, preferences and drafts.
- `ui/`: shell/theme, separate workflow screens, image comparison, draft handling, worker orchestration in `jobs.rs`, framework QA in `qa.rs`.

The UI holds immutable shared repository state and cached history geometry/filter results. History, diffs, and flat changed-file lists are virtualized; very large change sets default to the list view. Git work runs off the UI thread. Commands go through argument vectors, never a shell string. Git still runs the repository's configured hooks and filters; the app is not a sandbox for untrusted projects.

Preferences live under `%APPDATA%\Branchroom` on Windows (`$XDG_CONFIG_HOME/Branchroom` or `~/.config/Branchroom` elsewhere). This includes local draft text and discard-recovery copies; treat that directory as private project data. A `protected-repositories.json` array of absolute paths makes repositories read-only in Branchroom; malformed policy blocks writes until repaired. The guard blocks mutation and IDE/terminal launch and suppresses optional Git index writes on inspection. It is an app policy, not an OS security boundary. Tests use disposable repositories.

## QA and dependency evidence

The integration suite creates disposable repositories with isolated app state and Git configuration. It covers snapshot/index isolation, worktrees, merge and revert conflicts, partial staging, local exclusions, branch statistics, stale commit plans, NUL metadata, recovery, and typed GitHub arguments. It does not publish anything to GitHub.

```powershell
# Fixture UI: commands and preference writes are disabled.
cargo run --features ui-preview -- --ui-preview
# Actual framework renders for visual inspection, using fixture data:
cargo test --all-features --bin branchroom render_audit_screens -- --ignored --nocapture
# Reproducible disposable-repository refresh measurement:
cargo run --release --locked --example measure_refresh
python scripts/Check-Contrast.py
```

Package versions were checked against current primary sources on 26 October 2026: [eframe 0.37.0](https://docs.rs/crate/eframe/latest), [image 0.25.10](https://docs.rs/crate/image/latest), [egui_kittest 0.37.0](https://docs.rs/egui_kittest/latest/egui_kittest/), [serde](https://docs.rs/serde/latest/serde/), [serde_json](https://docs.rs/serde_json/latest/serde_json/), and [rfd](https://docs.rs/rfd/latest/rfd/). `Cargo.lock` pins the graph. The audit records zero known vulnerable advisories and three upstream Linux portal maintenance advisories; see the production audit for IDs and limits.

Source behavior follows [Git ref transactions](https://git-scm.com/docs/git-update-ref.html), [worktrees](https://git-scm.com/docs/git-worktree), [revert](https://git-scm.com/docs/git-revert), [index flags](https://git-scm.com/docs/git-update-index), [batched branch comparisons](https://git-scm.com/docs/git-for-each-ref), and the [GitHub CLI PR interface](https://cli.github.com/manual/gh_pr_create).
