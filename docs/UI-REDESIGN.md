# Interface design and QA notes

Current implementation: 0.2.0-rc.1, 26 October 2026. This document replaces incremental notes from earlier builds. The current [audit](PRODUCTION-AUDIT.md) and [migration matrix](GITHUB-DESKTOP-PARITY.md) give the verification boundary.

## Workspace model

The top bar owns repository and branch selection, refresh/sync, and IDE access. Current branch search separates local branches from last-fetched remote refs. Collaboration actions belong to the sidebar. History always follows the checked-out branch; pull requests are a separate workspace.

Primary surfaces are Changes, History, Snapshots, Pull requests, Merge branches, Branch map, Compare & recovery, Stashes, Conflicts, Repositories, Command center, and Settings. Status/progress is persistent; detailed output opens in a dismissible activity drawer. Blocking mutation review shows repository, effect, and command details when applicable.

## Content and hierarchy

Each workflow uses a primary action, grouped secondary actions, and a contextual inspector. Long identifiers belong in selected details with copy/full hover, not on snapshot cards. Snapshot names/dates and source titles identify tree nodes. Source commits connect to actual captures; captures from the same parent are siblings. Branch tips connect to an actual common ancestor. Unrelated histories do not get a fabricated connection.

History and diffs use virtual rows. Filtered history does not draw invented edges across hidden commits. Changed-file list mode is virtualized and is the initial choice above 500 changes. Immutable repository state and graph/filter caches avoid cloning large histories every frame.

## Responsive behavior

Headers measure their content instead of imposing fixed heights on wrapping controls. Below the sidebar breakpoint, Workspace contains navigation. At smaller content widths, Changes has Files / Diff / Commit tabs, selecting a file opens Diff, and Diff options holds formatting/selection actions. The commit action stays below the scrolling composer.

Snapshot trees become a vertical indented source/capture hierarchy at narrow widths. Compare versions is collapsed initially in compact mode, with its disclosure visible. Inspectors move into separate windows when they no longer fit beside the content. Diagrams and code retain scrolling; a zoomed viewport is not represented as a shrunken desktop mockup.

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
