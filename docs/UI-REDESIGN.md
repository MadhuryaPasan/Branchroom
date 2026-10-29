# Interface design and QA notes

Current implementation: 0.2.0-rc.1, 26 October 2026. This document replaces incremental notes from earlier builds. The current [audit](PRODUCTION-AUDIT.md) and [migration matrix](GITHUB-DESKTOP-PARITY.md) give the verification boundary.

## Workspace model

The top bar owns repository and branch selection, refresh/sync, and IDE access. Current branch search separates local branches from last-fetched remote refs. Collaboration actions belong to the sidebar. History includes all loaded branch tips; the checked-out branch is marked and selection is shared across graph lenses. Pull requests also remain available in their own workspace.

The four primary surfaces are Graph, Changes, Pull requests and Time Machine. Graph owns Commits, Branch map and Compare lenses. Conflicts are contextual; Repositories, Settings and Advanced tools remain available through contextual controls. Status/progress is persistent; detailed output opens in a dismissible activity drawer. Blocking mutation review shows repository, effect, and command details when applicable.

## Content and hierarchy

Each workflow uses a primary action, grouped secondary actions, and a contextual inspector. Long identifiers belong in selected details with copy/full hover, not on snapshot cards. Snapshot names/dates and source titles identify tree nodes. Source commits connect to actual captures; captures from the same parent are siblings. Branch tips connect to an actual common ancestor. Unrelated histories do not get a fabricated connection.

History and diffs use virtual rows. Filtered history does not draw invented edges across hidden commits. Changed-file list mode is virtualized and is the initial choice above 500 changes. Immutable repository state and graph/filter caches avoid cloning large histories every frame.

## Responsive behavior

Headers measure their content instead of imposing fixed heights on wrapping controls. Below the sidebar breakpoint, Workspace contains navigation. At smaller content widths, Changes has Files / Diff / Commit tabs, selecting a file opens Diff, and Diff options holds formatting/selection actions. The commit action stays below the scrolling composer.

Time Machine uses a vertical timeline at every width. The commit inspector uses bounded sibling panes, a compact dock and an explicit expanded detail view at short window heights. Its actions stay outside the metadata scroller. PR creation stacks its form and live preview when two columns do not fit. Diagrams and code retain scrolling; a zoomed viewport is not represented as a shrunken desktop mockup.

## Visual system

`src/ui/theme.rs` owns semantic color roles, typography defaults, icons, and primitives. Segoe UI is used when installed; system symbol/language fallbacks and bundled fonts handle missing glyphs. Disclosure arrows use vector geometry. No system font files are distributed.

Text uses 24px page titles, 18–20px section emphasis, 14px controls, and 12–13px metadata/code before zoom. The palette distinguishes selection, success, caution, danger, and saved captures with both color and text. Radius and density are consistent across controls and cards. Badges truncate within a bound and expose full hover text. Native controls and tree nodes have names for the accessibility tree.

The contrast script passes all 25 declared opaque pairs: body text/canvas 14.74:1, muted text/selection 4.84:1, action text/primary 5.19:1, meaningful connectors/canvas 4.30:1, and input borders/background 4.42:1. This does not certify antialiasing, translucent surfaces, or assistive-technology behavior.

## Verification

The framework harness exercises every screen, selected states, minimum-size 200% zoom (530×350 logical points representing 1060×700 pixels), keyboard branch selection, modal shortcut isolation/Escape, draft restoration, project-switch PR cleanup, cancellation before close, PR/image accessible names, toolbar non-overlap, and reachable commit/diff controls. Actual framework WGPU renders were exported and inspected. Test builds omit only the developer fixture switcher, so captures show the application layout rather than extra preview chrome.

Renders are generated with:

```powershell
cargo test --all-features --bin branchroom render_audit_screens -- --ignored --nocapture
```

Output: `target/qa/ui`. Saved screenshots are excluded from the repository for privacy.

The Windows Computer Use helper could not connect to its native pipe after retry/reset. OS input/focus, native dialogs, Narrator/NVDA reading order, installer behavior, and multiple physical-DPI displays remain **not verified**. Localization/RTL is not implemented or approved. Framework accessible names and screenshots are useful evidence, not complete accessibility certification.


## Toolbar and changed-files refinement — 27 October 2026

Scope: the repository/branch selectors, Changes header, changed-file rows and commit action in the native egui workspace. The existing theme tokens, font roles, spacing and resizable pane remain the design conventions. This is a focused review of those controls; it does not claim a new whole-application or native assistive-technology audit.

| Domain | Evidence inspected | Result |
| --- | --- | --- |
| Accessibility | Named selector, inclusion, file and menu controls; 200% shortcut and non-overlap tests in `src/ui/qa.rs` | Names and exercised keyboard paths pass. Native Narrator/NVDA and full focus traversal are not verified. |
| Layout | `src/ui/shell.rs`, `src/ui/changes.rs`; normal, long-name and 200% framework renders | Fixed unstable selector widths and file rows that expanded the pane; compact tabs share the header when space permits. |
| Writing | Selector captions, file status, empty filter state and commit label | Full names remain available through accessible labels and hover; empty results appear within the visible pane. |
| Typography | `src/ui/theme.rs`; actual renders with long Unicode names and nested files | Two-line selectors elide to their real width. Long file labels retain both their beginning and ending. |
| Colors | `scripts/Check-Contrast.py` and selected/modified/added file states | Declared opaque token pairs pass; states also have letters and accessible text. |
| UI polish | Normal, long-name and compact renders under `target/qa/ui/tuning-*.png` | Matched control sizing and aligned file actions; selected row and primary commit button are distinct. |

| Severity before fix | Domain | Location | Before | After | Why |
| --- | --- | --- | --- | --- | --- |
| HIGH | Layout | `src/ui/changes.rs` | A long file row could expand the resizable pane on successive frames. | Size the row inside the available width and elide its painted name; test that the action position stays stable. | The pane must not displace the diff when filenames grow. |
| HIGH | Accessibility | `src/ui/shell.rs` | Ctrl+G selected Changes without selecting the compact Commit tab. | The shortcut opens Commit and focuses the summary field. | A keyboard path must reach the same composer at 200% zoom. |
| MEDIUM | Layout / typography | `src/ui/shell.rs`, `src/ui/theme.rs` | Repository and branch targets changed width with their text, and lengthy values were cut off inconsistently. | Use stable-width targets, two text lines and width-based ellipsis; preserve full accessible labels and hover text. | Users can scan context without the toolbar shifting between projects. |
| MEDIUM | UI / writing | `src/ui/changes.rs` | Centered file names, narrow paths and variable commit button width made file actions difficult to scan. | Align names left, keep status letters and file actions on a consistent row, limit indentation and align commit with the composer. | The changed-file list should prioritize file identity and inclusion state. |

Verification: `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test --all-targets --all-features --locked`, `cargo test --all-features --bin branchroom render_changes_tuning --locked -- --ignored --nocapture`, and `python scripts/Check-Contrast.py`. Framework renders were inspected at 1320×820, with long names and at 1060×700 with 200% zoom. Generated fixture data was used; no user repository was operated on.

Verdict for the inspected scope: **Approve**, with native screen-reader, physical-DPI, localization and RTL behavior not verified.

## Pull request and clean-worktree follow-up — 29 October 2026

Pull requests now open directly to their list and offer Open, Mine, Review requested, and Create tabs. Creating a PR defaults the destination branch, prevents selecting the same source and destination, suggests a title and repository template, and shows commits and changed files beside the form. If the branch needs publishing, the user reviews and confirms that push together with PR creation. GitHub connection controls are kept in a compact popover. The command search accepts common recovery phrases and shows the corresponding Git command for learning; these entries open the appropriate workspace rather than executing commands.

The clean Changes view replaces the empty file/diff/commit panes with a short branch status, recent commits, and next actions. The commit summary shows its character count. Repository remotes are presented as rows with separate fetch and push URLs; editing a row fills the remote settings controls.

Verification coverage added in `src/ui/qa.rs` checks the PR creation layout and tabs, clean-worktree actions, and natural-language command search. `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, and `cargo test --all-targets --all-features --locked` passed; the final UI suite also passed after follow-up refinements. Preview-mode tests do not contact GitHub or run Git commands.

## Guided fixes and workspace graph follow-up

The command search now launches reviewed actions for moving unpublished commits, splitting the latest commit, and safely reverting history; branch rename opens its existing form, and branch recovery selects a Time Machine recovery point. Move-to-new-branch previews allow the destination name to be edited. Pull-request badges on commit rows open their details, and branch-map cards expose matching linked worktrees as IDE-launchable Desk nodes. Worktree porcelain parsing handles multiple and detached checkouts.

Simple mode is the default. Advanced mode is persisted in Settings and gates raw command tools, graph/history forms, force-push controls, and advanced recovery and branch operations. UI tests cover these routes and the new graph affordances.

## Branch-map interaction polish

The branch-map update control uses a short single-line label with the full effect available on hover, preventing it from wrapping over branch sync indicators. Branch-card update/publish tools are hidden in Simple mode and appear under Advanced when enabled. The branch-map UI test checks both the label and mode behavior; the framework screenshot export was rerun for visual review.

## Commit review and comparison clarity

The branch strip is labeled “Branches & worktrees,” explains that branch names mark their latest commits, and scrolls horizontally when its contents do not fit. Branch tips explain their selection and drag behavior on hover. The unnecessary Folder control has been removed. The commit inspector uses a fixed-width, non-resizable right dock on wide screens and a compact bottom dock on narrow screens. At extremely constrained high zoom, it anchors to the bottom so the graph can remain scrollable. The history content uses explicitly reserved sibling regions rather than a nested central panel, preventing the graph from covering the inspector or patch preview. It ignores stale changed-file results when selection changes. Commit rows explain single- and double-click behavior, keep the active selection click-driven, and visibly mark the active commit; opening a patch names both the abbreviated commit ID and its subject. Linked PR badges and inspector details identify the PR title, compare branch, base branch, state, and CI status.

The Compare page distinguishes “Base branch” from “Compare with,” puts both branch names and unique-commit counts in a summary, and labels each unique-commit list with its actual branch. The two lists sit side by side when space permits, with changed files in a separate section; selecting a file opens the comparison patch. Pull-request creation previews explicitly label Base and Compare, show draft/review state and commit/file counts, and identify commits with abbreviated SHAs. The confirmation summary repeats the exact branch roles and optional reviewers and labels. UI tests cover these labels, branch selection, and the patch title.

## Final UI and release pass — 29 October 2026

See [the final review](FINAL-UI-REVIEW-2026-10-29.md) for the current findings, resolved inspector geometry and selection issues, six-phase coverage, verification commands and release boundary. This supersedes earlier descriptions of the snapshot tree and overlay inspector.
