# Final UI and release review — 29 October 2026

## Scope and coverage

Scope: the complete Branchroom workspace, the existing six-phase implementation and its local Git operations. Stack: Rust with egui/eframe 0.37, shared native theme tokens and the existing Git/GitHub CLI APIs. Existing uncommitted feature work was retained. No repository reset, source commit, tag or GitHub publication was performed.

Project conventions read: README, UI-REDESIGN, the phase reports, PRODUCTION-AUDIT, GITHUB-DESKTOP-PARITY and RELEASING. No AGENTS.md was found. The better-interface review was applied with better-accessibility, better-layout, better-writing, better-typography, better-colors and better-ui. Browser-only CSS rules were translated to the native framework's layout, font, focus and accessibility APIs.

| Domain | Evidence inspected | Result |
| --- | --- | --- |
| Accessibility | Framework names, keyboard shortcuts, modal Escape, text-editor Undo isolation, inspector expansion and numeric resize fallback; ui/qa.rs and inspector.rs | Exercised framework paths pass; native screen-reader and OS focus traversal are not verified |
| Layout | Rendered core modes, Compare, PR creation, Time Machine, conflict resolution, repository forms and Settings; bounded panes and compact inspector | Fixed sibling overlap, inconsistent action positions, narrow PR columns and short-height inspector clipping |
| Writing | Inspector loading/retry states, operation summaries, GitHub account state, error recovery text, remote-address controls | Explicit branch roles, requested move counts and recovery actions; raw output remains available in details |
| Typography | Long commit messages, identifiers, file labels, bundled/system font roles and the active-commit marker | Metadata wraps inside a scroller; actions remain fixed; active marker raised from 9px to 12px |
| Colors | Check-Contrast.py, theme contrast tests and the actual active-marker pair in both themes | All 50 declared opaque pairs pass; active marker corrected from 1.73:1 to 7.83:1 dark / 8.14:1 light |
| UI polish | Theme primitives, page insets, graph toolbar, inspector docks, live PR preview and error/progress surfaces | Shared content alignment, one stable inspector structure, reduced header clutter and explicit compact expansion |

## Resolved findings

| Severity before fix | Domain | Location | Before | After | Why |
| --- | --- | --- | --- | --- | --- |
| HIGH | Layout | src/ui/history.rs:806; src/ui/theme.rs:584 | Several allocation/overlay paths let the inspector cover graph controls and siblings | Bounded graph, inspector and patch regions; compact affordance and explicit expanded details | Content cannot resize or paint over its neighbour |
| HIGH | Accessibility / layout | src/ui/history.rs:806 | At 200% zoom, the inspector affordance was partly behind the status bar | Compact graph header, shorter optional chrome, a properly sized dock and geometry regression checks | The complete control must remain reachable and visible |
| HIGH | Colors / typography | src/ui/history.rs:649 | Active label used INK on ACCENT at 1.73:1 and 9px | Accent text on a neutral surface at 12px, with selected-row and graph-node cues retained | Readable status text and redundant selection cues |
| HIGH | Data / UX | src/ui/inspector.rs:79 | Selection reused scroll state and a shared operation worker; merge file statistics were inconsistent | Per-selection scroll revision, independent bounded cache, stale-result protection and explicit first-parent merge statistics | Each commit receives the same inspection behavior and the correct files |
| HIGH | Safety | src/timeline.rs:396 | Undoing branch rename restored branch tips but left renamed tracking configuration | Checkpoints restore saved branch and remote-address configuration; old checkpoint schemas remain readable | A restored branch must keep its tracking behavior |
| MEDIUM | Layout | src/ui/collaboration.rs:305 | PR form/preview always used two columns | Two columns only when content width supports them; otherwise one stacked column | Form fields and live preview keep usable widths |
| MEDIUM | Writing / UX | src/ui/shell.rs:1249 | Intent search ignored requested counts and matched unrelated words across a whole sentence | Word-level fuzzy matching and exact-count/name parsing; requested moves use the shared reviewed backend | “Move last 2 commits” now means two commits |
| MEDIUM | UX | src/ui/connection.rs:61 | Connection display depended on whether a PR list had loaded | Repository-host account check with an account chip and a setup state | Account state is independent of the currently selected PR tab |
| MEDIUM | Layout / polish | src/ui/history.rs:806; src/ui/shell.rs | Repeated instructions, separate Columns/Density buttons and varying content insets crowded the graph | View menu owns layout controls; shared insets and compact header | More canvas space and consistent alignment |

The original overlay is visible in `target/qa/redesign/History-zoom.png`. Final captures live in `target/qa/release`; the final inspector export replaces the graph captures after the short-height correction. Captures are ignored by Git because real UI screenshots can contain private repository information.

## Six-phase coverage

1. **Shell and design system:** shared semantic theme/spacing, four-item navigation, repository/branch selectors, command search, state-driven sync, a separate dangerous section, activity drawer, long-operation progress and human-readable error summaries
2. **Graph workspace:** Commits / Branch map / Compare, shared selection and one inspector, automatic file counts, PR/CI context, working-copy node, branch cleanup, author handling, search and virtualized 12,000-commit verification
3. **Reviewed manipulation:** merge, cherry-pick, reset, reorder, squash, commit/amend and update previews; isolated conflict forecasts; safety checkpoints; keyboard forms and exact-command fallback. This pass also routes revert, current-branch rename and branch-from-commit through the same preview path
4. **Time Machine:** unified action/safety/file/shelved-work timeline, before/after restore, local Undo/Redo, exact index/working content and durable object pins. Tracking/address configuration is now included where available
5. **Pull requests and fixes:** list-first browsing, account chip, Open/Mine/Review requested/Create tabs, default base, distinct branch validation, automatic title/template/commit/file preview and reviewed publication; fixes for wrong branch, local undo, pushed revert, splitting the latest unpublished commit, rename and recovery. Specific-count moves work without an upstream, including the initial commit
6. **Changes and extras:** file/hunk/line staging, diff, clean status view, summary count/amend/co-authors, clone/init, remote table with separate fetch/push address editing, workspace/Desk access, contextual conflicts and Simple/Advanced mode

Existing lower-level Git/gh operations remain available. Some complex operations retain their explicit Advanced forms rather than pretending that the app implements every Git subcommand as a dedicated wizard. The parity matrix lists functionality outside the requested phases.

## Verification

Passed:

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features --locked -- -D warnings`
- `cargo test --all-targets --all-features --locked`: 2 library, 70 UI and 96 repository cases, with graphical exports excluded from the normal run
- The final 70-case UI suite was rerun after the compact-header correction
- `python scripts/Check-Contrast.py`: 50 declared opaque pairs in both themes
- `cargo test --all-features --bin branchroom render_release_candidate --locked -- --ignored --nocapture`
- `cargo test --all-features --bin branchroom render_final_inspector_fix --locked -- --ignored --nocapture`
- `./scripts/Build-Windows.ps1 -SkipChecks`: optimized Windows x64 build, versioned ZIP and SHA-256 manifest; final artifact details are in `dist/latest.json`

Tests use disposable repositories and isolated test preferences. They cover stale previews, partial staging, working-file preservation, published-history restrictions, conflict forecasts, root-commit moves, rename/recovery, Undo/Redo and Git garbage collection. No test submits hosted GitHub changes.

Not verified: live authenticated GitHub publication/review/merge, native Narrator/NVDA reading order, every installed IDE, physical multi-monitor DPI, Linux/macOS execution, localization/RTL, signed installation and automatic updates. The package is an unsigned Windows release candidate, not a claim of certification in these areas.

## Changed files

New shared UI modules: `src/ui/inspector.rs`, `src/ui/connection.rs`.

UI integration: `src/ui/history.rs`, `src/ui/theme.rs`, `src/ui/shell.rs`, `src/ui/mod.rs`, `src/ui/jobs.rs`, `src/ui/manipulation.rs`, `src/ui/collaboration.rs`, `src/ui/comparison_canvas.rs`, `src/ui/views.rs`, `src/ui/workspace.rs`, `src/ui/changes.rs`, `src/ui/time_machine.rs`.

Backend support and safety: `src/git.rs`, `src/operation.rs`, `src/timeline.rs`.

Verification and handoff: `src/ui/qa.rs`, `tests/repository.rs`, README and the updated design/audit documents. Earlier uncommitted GitHub, settings and workspace feature changes remain intact and must be included in the release commit as well.

## Publish handoff

1. Use the versioned package referenced by `dist/latest.json`; do not accidentally launch an older executable
2. Smoke-test the package against a disposable repository: inspect several commits, open a merge commit's files, resize/expand details, stage a hunk and commit, then Undo/Redo
3. On a signed-in account, check the live PR list and draft creation against a test repository before announcing a production release
4. Review all changed and new files, choose an unused release version/tag and follow [RELEASING.md](RELEASING.md)

## Verdict

Framework UI verdict: **Approve for the inspected Windows release-candidate scope**, after the resolved findings above. Hosted/native acceptance and distribution limitations remain explicitly not verified. No GitHub release was published by this task.
