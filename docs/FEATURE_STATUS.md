# Branchroom feature status

Phase 0 audit — 31 October 2026

## Audit boundary

This began as a source-and-build audit of the current checkout. It now also
records the focused UI increments completed after the audit. Existing phase
documents and tests were treated as evidence, not as proof that an
authenticated, native, or manual workflow works. Git/engine modules were not
rewritten.

The current checkout is `e527326` (`change the version to 0.1.5`). Formatting,
strict Clippy, the full test suite, and the optimized release build pass. The
release executable was launched successfully for a bounded Windows process
check and then closed. The test command reported 5 library tests, 70 passing
UI tests with 12 ignored render tests, and 97 repository tests.

Status meanings:

- **Works** — implementation is present and supported by source/tests or a
  documented manual verification.
- **Partial** — the main path exists, but important modes, integrations, or
  acceptance evidence are incomplete.
- **Missing** — no dedicated implementation was found.
- **Unverified** — implementation appears to exist, but the available
  evidence does not establish that the real/native/external workflow works.

## Feature matrix

| Feature | Status | Evidence | Current UI location |
|---|---|---|---|
| Open an existing repository | Works | `ui::Branchroom::open_repo`, repository menu, `git::load` | Repository selector in the top bar |
| Initialize a repository | Works | `Task::Init`, `ui::workspace::remotes` | Repositories |
| Clone from a URL | Works | `Task::Clone`, clone form in `ui::workspace::remotes` | Repositories |
| Recent repositories and project switching | Partial | Settings/storage and repository menu exist; no source evidence of a searchable account-style picker | Repository selector |
| Git tool discovery and diagnostics | Works | README documents PATH, standard Windows locations, and adjacent `tools`; Settings/About UI | Settings / About |
| Branch selection and creation | Works | `ui::shell::branch_picker`, `git::validate_branch`, branch operations | Top bar and Graph |
| Branch switching with dirty changes | Works | `Task::SwitchBranch`, `collaboration::switch_branch` | Branch picker |
| Local/remote tracking status | Works | `git::Branch`, `git::load`, branch map and sync status | Top bar, Graph, Branch map |
| Push, fetch, pull, and publish | Partial | Git tasks and `smart_sync`/remote forms exist; live network/authentication is not verified | Top bar Sync and Repositories |
| Remote fetch/push address editing | Works | Remote form in `ui::workspace::remotes` | Repositories |
| Changed-file status | Works | `git::parse_status`, `ui::changes::change_list` and `change_tree` | Changes |
| Whole-file stage/unstage | Works | `Task::CommitFiles`, `ui::changes::run_git` | Changes |
| Hunk staging | Works | `diff::Patch::selection`, `apply_to_index`, patch actions | Changes > Diff |
| Line staging | Works | `ui::changes::set_line_selection`, `diff::Patch::selection` | Changes > Diff |
| Partial commits and index isolation | Works | `commit::prepare`, private-index snapshot paths, repository tests documented in parity matrix | Changes > Commit |
| Commit summary/body | Works | `commit::Plan`, `ui::changes::commit_composer` | Changes > Commit |
| Amend and co-authors | Partial | Commit composer and commit plan support amend; no account autocomplete for co-authors | Changes > Commit |
| Commit drafts | Works | `ui::drafts::persist_draft` and `load_draft` | Changes > Commit |
| Commit locks/exclusions | Works | `commit_lock::{read,set,guard_commit,guard_add}` and Actions UI | Changes > Actions > Commit locks |
| Text unified diff | Works | `ui::changes::render_patch` | Changes > Diff |
| Text split diff | Works | `ui::changes::render_split_patch` | Changes > Diff |
| Whitespace/full-context diff options | Works | `Task::FilePatch` and patch controls | Changes > Diff options |
| Large diff virtualization and synchronized scrolling | Works | Existing phase review and virtual patch rows in `ui::changes` | Changes > Diff |
| Raster image diff | Works | `image_diff::{supported,decode,compare}`, `ui::images::image_comparison` | Changes > Diff |
| SVG/PSD diff | Missing | Parity matrix explicitly lists no dedicated workflow | External editor |
| Animated image playback | Partial | First-frame image comparison is implemented; full animation playback is not | Changes > Diff |
| Commit graph | Works | `graph::layout`, `ui::history::history`, graph rendering and virtualization | Graph |
| Commit inspection | Works | `ui::inspector::commit_inspector`, asynchronous details/cache | Graph inspector |
| Automatic changed-file inspection | Works | `ui::inspector::inspector_data`, `CommitFile::parse` | Graph inspector |
| Commit/author/file/date search | Works | `ui::history::filter_terms` and structured filters | Graph search |
| Author canonicalization and mismatch warning | Partial | Phase 2 documents email/mailmap canonicalization and GitHub identity warning; live GitHub response is unverified | Graph inspector |
| Branch map | Works | `ui::history`, `graph::snapshot_groups`, branch status and tracking data | Graph > Branch map |
| Branch cleanup | Works | `Task::DeleteMerged`, `collaboration::delete_merged_branch`, checkpoint/recheck behavior | Graph > Branch map |
| Branch/commit/checkpoint comparison | Works | `ui::compare`, `ui::comparison_canvas`, `compare` backend | Graph > Compare |
| Visual zoom/pan and list fallback | Works | History view controls and phase verification | Graph |
| Merge preview and execution | Works | `operation::Kind::Merge`, `operation::{preview,apply}`, `ui::manipulation`, `ui::merge` | Graph actions / Merge |
| Cherry-pick preview and execution | Works | `operation::Kind::CherryPick`, manipulation UI | Graph actions / Advanced |
| Revert | Works | `collaboration::revert`, `Task::Revert`, reviewed action flow | Graph inspector / Advanced |
| Reset/undo branch tip | Partial | Reviewed reset/undo exists with clean/linear/published-history restrictions | Graph actions / Advanced |
| Squash/fixup | Works | `operation::Kind`, manipulation preview and history actions | Graph actions / Advanced |
| Reorder commits | Partial | Reviewed linear unpublished reorder exists, limited to documented constraints | Graph actions / Advanced |
| Move commits to a new branch | Works | `Kind::MoveToBranch`/`MoveCommits`, manipulation UI | Graph actions / fix-it flow |
| Dry-run conflict forecast | Partial | Preview workers and conflict paths exist; forecasts stop at the first replay conflict | Graph preview bar |
| Safety checkpoint before risky operations | Works | `timeline`, `snapshot`, `safety`, and operation integration | Time Machine |
| Stale-preview refusal | Works | Operation validation and Phase 3 tests/documentation | Preview confirmation |
| Conflict resolution editor | Works | `ui::views::conflicts`, `conflict::save`, side loading and marker/stale checks | Conflicts |
| Conflict continue/abort/skip | Works | Conflict operation controls and `Task::Command` | Conflicts |
| Binary/submodule/complex rename conflict UI | Partial | External editor/file actions are provided; no complete dedicated resolver | Conflicts / IDE |
| Stash/shelved work create/apply/pop/delete | Works | `Task::Shelved`, timeline page and Git stash forms | Time Machine > Shelved work |
| Discard changed file | Works | `Task::Discard`, `recovery::discard` | Changes file actions |
| Recover discarded file | Works | `recovery::restore`, `Task::RestoreDiscard` | Time Machine |
| Persistent action timeline | Works | `timeline::Item`, `timeline::candidate`, timeline page | Time Machine |
| Local Undo/Redo | Works | `timeline_undo`, Ctrl+Z/Ctrl+Shift+Z shortcut handling | Time Machine |
| Full-state restore preview | Works | `timeline::RestorePlan`, `Task::TimelinePlan/Restore`, stale/external-edit checks | Time Machine |
| Snapshot/checkpoint capture | Works | `Task::Snapshot`, `snapshot` module, checkpoint capture dialog | Checkpoints |
| Snapshot comparison | Works | Snapshot selection, comparison canvas, source grouping | Snapshots / Compare |
| Snapshot independent workspace | Works | `Task::Worktree`, IDE settings and worktree creation | Snapshot inspector |
| Promote snapshot to branch | Works | `Task::Promote`, `ui::experiments` | Snapshot inspector |
| Recover removed snapshots | Works | `Task::RestoreSnapshot`, removed snapshot storage | Time Machine |
| Local-only/off-device backup boundary | Works | README, Time Machine copy, and checkpoint capture copy explicitly state checkpoints are not backups | Checkpoints |
| Pull Request list and filters | Works | `collaboration::pull_requests`, `Task::PullRequests`, collaboration UI | Pull requests |
| Pull Request details/diff/checks | Partial | `github::details`, `gh pr diff/checks` commands and details UI; live `gh` access unverified | Pull requests |
| Pull Request independent review workspace | Partial | `Task::OpenPr`, `collaboration::open_pull_request`; native IDE launch unverified | Pull Request details |
| Create draft/ready Pull Request | Works | `github::CreateRequest`, `Task::CreatePullRequest`, creation form | Pull requests > Create |
| PR templates, title/body, reviewers, labels | Partial | Template/title/body and text reviewer/label inputs exist; no rich account search | Pull requests > Create |
| PR comment/approve/request changes | Works | `github::Review`, `Task::GithubAction`, review controls | Pull Request details |
| PR ready/close/reopen/merge | Works | `github::Action`, pinned reviewed-head checks, details UI | Pull Request details |
| Inline review threads | Missing | Parity matrix explicitly directs users to GitHub web | GitHub web |
| Issues, Projects, Actions editor, rulesets | Partial | Some command/read-only coverage exists; no complete dedicated UI | Tools / GitHub web |
| Command Center | Works | `Page::Tools`, raw/advanced Git and `gh` task forms | Command center |
| Tags, reflog, blame, submodules, maintenance | Partial | Commands/forms are available; no complete dedicated visual workflows for all | Command center |
| Interactive rebase editor | Missing | Parity matrix lists it as a missing dedicated workflow | Terminal/Advanced |
| Bisect orchestration | Missing | Parity matrix lists it as missing | Terminal/Advanced |
| IDE selection and launch | Partial | Settings support default/project/custom executable and `{path}`; native launch acceptance is unverified | Settings, repository/snapshot/PR actions |
| Keyboard shortcuts and accessibility names | Partial | `ui::shell::shortcuts`, named controls, framework tests; native screen-reader/focus traversal is unverified | Global |
| Responsive narrow/high-zoom layouts | Partial | Phase UI tests cover minimum-size/200% zoom; physical DPI and all manual layouts are unverified | Global |
| Light/dark themes and contrast | Works | `ui::theme`, `Check-Contrast.py`, phase review reports declared pair checks pass | Settings / Global |
| Activity/progress/error reporting | Works | Job system, activity drawer, stderr/details, explicit error reporting | Status bar/activity drawer |
| Onboarding and first-run guided tour | Missing | No dedicated onboarding flow found in current page/task architecture | Not present |
| Simple/Advanced mode | Partial | `settings.advanced_mode` gates some controls and Advanced forms exist; no complete first-run mode model | Settings / contextual Advanced |
| Plain-language command intents | Partial | Command bar exists; source shows command search and some count parsing, but no complete intent catalog | Ctrl+K command bar |
| Automatic checkpoints before actions/timer/branch switch | Partial | Automatic safety points exist for reviewed actions; configurable timer and all requested trigger choices are not established | Time Machine/settings |
| One unified checkpoint concept | Partial | Primary navigation, capture, restore, delete, recover, and command copy now use Checkpoints; internal snapshot/safety storage names and some secondary labels remain | Checkpoints / Advanced |

## Current UI problems

1. **Navigation is fragmented.** `Page` still contains Changes, History,
   Branches, Snapshots, Stashes, Conflicts, Remotes, Github, Tools, Settings,
   Compare, and Merge. This conflicts with the proposed five-item rail and
   makes related work appear in separate destinations.
2. **Checkpoint terminology is inconsistent.** The product uses “Time
   Machine”, “safety point”, “snapshot”, “saved version”, “recovery head”,
   “removed snapshot”, and “shelved work”. The storage is increasingly
   unified, but the mental model is not.
3. **The main graph is not yet the universal action surface.** Graph actions
   exist in `ui::manipulation`, but Changes, Merge, Conflicts, Snapshots, and
   Pull Requests retain separate form-oriented paths.
4. **The top bar is dense.** Repository, branch, sync, command search, IDE,
   mode navigation, and status controls compete for width. Narrow mode moves
   pages into a menu rather than providing the requested stable left rail.
5. **The same operation has multiple entry points.** Merge/update, branch
   switching, recovery, stash/shelved work, snapshot promotion, and GitHub
   actions appear in different pages and context menus. The paths do not
   consistently share one visual preview language.
6. **Changes is still a form/list workspace.** It has strong staging and diff
   support, but the empty state and commit workflow are not centered on a
   graph/checkpoint explanation.
7. **Pull Requests are not graph-native.** PR details and actions are in a
   dedicated page; the source evidence does not establish PR/CI nodes as a
   single, always-visible graph vocabulary.
8. **Advanced mode is partly feature-gated rather than a complete information
   architecture.** Some controls are hidden or moved based on
   `advanced_mode`, but there is no evidence of a complete Simple/Advanced
   onboarding model.
9. **Jargon remains in the implementation/UI.** “Time Machine”, “safety
   point”, “reflog”, “worktree”, “HEAD”, “baseline”, “stash”, and Git command
   terms remain visible in internal labels or advanced explanations, although
   primary checkpoint copy has been updated.
10. **Manual/native acceptance remains incomplete.** Existing framework tests
    do not establish native screen-reader behavior, OS focus traversal,
    physical-DPI behavior, live GitHub authentication, or native IDE launch.
11. **Some major Git workflows are command-oriented.** Bisect, interactive
    rebase, inline PR threads, and complete tag/blame/file-history views are
    not dedicated visual experiences.
12. **The graph implementation and requested graph architecture are not the
    same thing.** The current graph has tested layout and virtualization, but
    Phase 0 does not establish that the complete future canvas can support
    all lanes, nodes, drag previews, and 10k+ commit rendering without further
    design work.

## Keep / merge / move / cut plan

| Current screen or subsystem | Keep | Merge | Move | Cut |
|---|---|---|---|---|
| Graph / History | Commit graph, inspector, search, lenses, branch map, compare | Merge Compare into the graph canvas | Make it Project home and the primary action surface | Remove duplicate standalone Graph/Compare navigation |
| Changes | File tree, staging, line/hunk selection, diff viewers, commit composer | Merge commit preview and working-copy node context | Keep as the Changes rail item; link to graph/checkpoint context | Remove duplicated raw action controls that lack preview |
| Snapshots / Experiments | Capture, compare, independent workspace, promote, recovery | Merge with Time Machine and discarded/shelved entries as Checkpoints | Rename and move to Checkpoints | Remove separate “Snapshots” and “Safety point” concepts from primary copy |
| Time Machine | Timeline, restore plans, Undo/Redo, safety checks | Merge with Snapshots, recovery, and shelved work | Make Checkpoints have an Activity lane | Remove duplicate recovery lists and the old Time Machine/Snapshot split |
| Stashes / Shelved work | Apply, keep, pop, delete, inline diff | Merge into Checkpoints as a checkpoint-type entry | Put under Checkpoints with explicit “Shelved work” filter | Remove standalone Shelved work page |
| Branches | Branch switching, tracking state, cleanup, create/rename/delete | Merge with Graph branch map | Put branch actions on graph lanes and top-bar picker | Remove standalone Branches page |
| Merge / manipulation | Preview, stale checks, safety checkpoints, Advanced fallback | Merge all reviewed operation previews into one graph preview bar | Put direct manipulation on Project; keep Advanced in Tools | Cut duplicate per-operation confirmation layouts, not the safety rules |
| Conflicts | Safe editor, base/current/incoming/result, continue/abort/skip | Merge conflict status into graph and Changes banners | Show only when conflicts exist; retain dedicated editor surface | Cut jargon from the primary path, not the resolver |
| Pull requests / collaboration | List, details, checks, review, lifecycle, creation | Merge PR/CI indicators into graph vocabulary | Keep a dedicated Pull requests rail item with graph-linked details | Cut any duplicate PR actions that bypass reviewed-head checks |
| Repositories / remotes | Clone/init, remote editing, independent workspaces | Merge repository switching into menus | Put project setup in Project empty state and menus | Remove a primary navigation slot for Repositories |
| Command center / Advanced | Raw Git/gh, tags, reflog, blame, submodules, maintenance | Merge all expert-only controls into Tools | Make Tools the single expert surface | Do not cut functionality; cut duplicate simple-path controls |
| Settings | Theme, density, IDE, project preferences, advanced mode | Merge repository switching and setup menus where appropriate | Keep behind menu, not primary rail | Cut settings from the primary navigation |
| Activity/progress | Jobs, cancellation, stderr, details | Merge status and timeline activity | Put compact status in footer and detail in bottom drawer | Cut silent/background failure states |

## Missing or incomplete features for a complete Git GUI

Ranked by likely user value:

1. **Interactive rebase editor** — visual pick/reword/edit/squash/fixup/drop
   workflow with reorder preview and conflict continuation.
2. **Conflict-aware dry-run previews for all replay operations** — current
   forecasts stop at the first replay conflict and cannot fully predict hook,
   driver, or network effects.
3. **File history and blame visualization** — trace a line/file through
   commits, authors, renames, and related Pull Requests.
4. **Bisect wizard** — guided good/bad selection, automated test command,
   progress, and recovery.
5. **Dedicated tags/releases UI** — create, move, annotate, sign, compare, and
   publish tags without relying on command forms.
6. **Inline Pull Request review threads** — show, create, resolve, and reply to
   file/line conversations.
7. **Complete GitHub issue, Projects, Actions, ruleset, and fork workflows** —
   current command/read-only coverage is not a web-equivalent UI.
8. **Partial clone and repository performance controls** — shallow/partial
   clone, fetch-depth management, sparse checkout, and large-repository
   guidance.
9. **Submodule visual management** — status, update, initialize, nested
   changes, and commit navigation.
10. **LFS and Git signing setup/troubleshooting** — explicit diagnostics and
    guided configuration while honoring existing Git tooling.
11. **First-run onboarding and guided recovery** — detect Git/`gh`, explain the
    graph/checkpoint model, and offer open/clone/init.
12. **Unified checkpoint experience** — one user-facing concept spanning
    snapshots, safety points, shelved work, action history, and recovery.
13. **Native accessibility and localization certification** — screen-reader
    acceptance, full OS focus traversal, RTL, and multilingual shaping.
14. **Signed installer, updater, and OS integration** — signed distribution,
    protocol/file associations, and safe update flow.

## Manual verification still required

The next phase must not assume these claims from source inspection alone:

- Launch the Windows executable and inspect every current page at normal,
  narrow, and 200% zoom sizes.
- Use a disposable repository to test branch switching with dirty files,
  partial staging, merge/cherry-pick/reorder previews, conflict resolution,
  stash/shelved work, snapshots, restore, Undo, and Redo.
- Use a disposable remote and authenticated `gh` account to verify PR listing,
  creation, review, lifecycle, checks, and merge behavior.
- Verify native file dialogs and IDE launches.
- Verify screen-reader names, keyboard focus order, Escape behavior, and
  high-DPI rendering on Windows.
- Re-run the full checks after each later implementation phase and update the
  evidence if behavior or test coverage changes.

## Completed UI increments after the audit

These focused changes were implemented and validated without replacing the
existing Git engine:

1. **Primary navigation and design vocabulary**
   - Primary destinations are Project, Changes, Checkpoints, Pull requests,
     and Tools.
   - `Ctrl+S` opens checkpoint capture.
   - Semantic colors cover working copy, checkpoint, Pull Request, remote,
     added, removed, modified, conflict, merged, and safe states.
2. **Project graph**
   - The existing custom-painted graph keeps its inspector, lenses,
     virtualization, and drag actions.
   - A compact legend identifies working copy, checkpoint, Pull Request, and
     remote nodes.
3. **Direct-manipulation previews**
   - Preview copy states the predicted conflict count, resulting commit count,
     checkpoint-before-apply behavior, and unchanged-files-until-confirmation
     rule.
4. **Checkpoint experience**
   - Time Machine's primary copy now uses Checkpoints, including save, delete,
     recover, restore, and local-only guidance.
5. **Changes and commit**
   - The commit composer shows staged, left-out, and locked file counts and
     explains exactly which changes will be included.
6. **Onboarding and polish**
   - The empty welcome screen explains Project, Changes, and Checkpoints and
     offers Open, Clone, and Create repository actions.

Validation for the completed increments:

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features --locked -- -D warnings`
- `cargo test --all-targets --all-features --locked`
- 5 library tests, 70 passing UI tests, 12 ignored UI render tests, and 97
  repository tests
