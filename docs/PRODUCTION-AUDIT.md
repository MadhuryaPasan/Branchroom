# Branchroom production readiness audit

**Build:** 0.2.0-rc.1 · **Review date:** 26 October 2026 · **Platform exercised:** Windows x64, Git 2.55.0.windows.3, Rust 1.99.0.

**Verdict: Block an unconditional production-ready claim.** The defects below were repaired and the current automated suite passes. The result is a substantially hardened release candidate. Live authenticated GitHub workflows, native Windows/assistive-technology acceptance, signed distribution, and non-Windows execution still require acceptance. The [migration matrix](GITHUB-DESKTOP-PARITY.md) also records dedicated workflows still missing; a generic command runner is not counted as GUI parity.

## Scope and method

This review covers the whole native application: navigation and status; Changes/diffs/commit composition; current-branch History; snapshot and branch trees; Merge; PR browsing/creation/review; conflicts; stashes; repository/remotes/worktrees; recovery; commands; settings; subprocess handling; persistence; packaging and CI. It combines source review, current Git/GitHub primary documentation, real disposable Git repositories, accessibility-tree interaction tests, actual framework renders, dependency advisory checks, and a repeatable refresh benchmark.

Existing README, UI design notes, parity notes and source conventions were inspected. No applicable AGENTS.md was found. The established Rust/eframe design system and installed better-interface, accessibility, layout, typography, colors, UI and writing skills informed the review. The implementation preserves a native UI rather than adding a web frontend. Unrelated document artifacts were excluded.

No write/test operation targeted personal repositories. Integration tests create generated temp repositories with isolated app state and Git global/system configuration. No GitHub PR/comment/review was published during this audit. No source changes were committed.

## Ranked findings and disposition

Severity describes the original user impact, not the amount of code changed. Locations refer to the repaired source. There is no committed pre-change baseline; tests, captured failures and before/after measurements provide the evidence.

| Rank / severity | Location | Finding and user impact | Repair / evidence | Status |
| --- | --- | --- | --- | --- |
| 1 · HIGH | `src/commit.rs:15`, `src/ui/jobs.rs:4` | A commit could execute after its staged content or branch changed, undermining the review. | Prepare a typed HEAD/tree/file/message plan off-thread; show included files; recheck exact HEAD/index tree at execution. Tests cover unborn, stale HEAD and externally changed index. Generic writes also check the reviewed HEAD. | Fixed |
| 2 · HIGH | `src/conflict.rs:6`, `src/storage.rs:21` | Saving a conflict result could overwrite a newer edit or retain unresolved markers. In-place writes also modify other hard links. | Compare original content, reject merge/diff3 markers and locks, validate paths, atomically replace then stage. Regression tests cover stale edits, markers and hard-link isolation. | Fixed |
| 3 · HIGH | `src/recovery.rs:123` | Restoring a discard could destroy newer working content. Tampered recovery identifiers could escape storage paths. | Save a recovery copy of current content before restore; atomic copy; validate entry IDs and relative paths; preserve staging area. Real-file tests verify both versions remain recoverable. | Fixed |
| 4 · HIGH | `src/snapshot.rs:11`, `src/snapshot.rs:32` | Timestamp-only IDs collided at Windows clock resolution; parallel tests demonstrated shared fixture paths. A ref update could overwrite an existing capture. | Timestamp + process + monotonic sequence IDs; create-only checked ref updates. Parallel uniqueness and snapshot isolation/ref-validation tests pass. | Fixed |
| 5 · HIGH | `src/ui/mod.rs:564`, `src/github.rs:147` | A previous project's PR remained actionable after switching repositories; merges could target a changed PR head. | Clear project-specific PR/detail/review state on project changes; typed actions; pin merge to inspected head; clear stale details if post-write reload fails. State-transition and argument tests pass. Authenticated behavior is still a release gate. | Fixed locally |
| 6 · HIGH | `src/process.rs:261`, `src/ui/mod.rs:772` | Piped commands could block cancellation; closing during a worker left the outcome unclear. Output was not consistently bounded. | Shared concurrent stdin/stdout/stderr collection; bounded output; cancel owned process tree/group; guarded close waits for completion and saves drafts. Blocked-stdin cancellation and UI close-confirmation tests pass. Cancellation does not undo an already completed remote action. | Fixed |
| 7 · HIGH | `src/history.rs:32`, `src/collaboration.rs` | Clean-looking sequencer states could allow history edits during another Git operation; baseline fetch could invalidate a selected destination. | Require idle + clean; distinguish cherry-pick/revert sequencer; refuse non-linear/unpublished violations; recheck branch/HEAD/cleanliness after fetch. Revert/mainline/conflict/abort and branch-update tests pass. | Fixed |
| 8 · HIGH | `src/ui/shell.rs:77`, `src/ui/changes.rs:81`, `src/ui/experiments.rs:4` | Actual 200% renders showed overlapping toolbar controls, clipped wrapping headers, an unusable stacked Changes layout, and almost no snapshot content. | Content-sized headers; compact global toolbar; Files/Diff/Commit with a stationary commit action; compact diff options; vertical snapshot hierarchy; collapsible comparison tray. Framework bounds and keyboard checks plus new renders verify the exercised viewport. | Fixed in inspected states |
| 9 · HIGH | `src/git.rs:369`, `src/ui/history.rs:3`, `src/ui/mod.rs` | Refresh launched two Git processes per branch and took about 16 seconds for 129 branches. Per-frame clones/filter work compounded the cost. | Batch ahead/behind and bounded DAG ancestor traversal; ID-keyed bounded cache; immutable Arc state; cached graph/filter; virtualized rows; >500 changes default to flat list. Measured refresh is about 1.1–1.3 seconds with 19–20 Git processes. | Fixed; measured |
| 10 · HIGH | `src/ui/theme.rs:23`, `src/ui/qa.rs:18` | Unsupported dropdown glyphs caused framework render failure; several controls had no accessible name. Long badges and local upstream labels were misleading. | System fallback fonts + safe missing-glyph policy; vector disclosures; named controls/helpers; bounded badges/full hover; local vs remote tracking. All screen/state name checks and glyph renders pass; native screen-reader output is not certified. | Fixed in framework |
| 11 · MEDIUM | `src/git.rs:292`, `src/git.rs:309` | Ad-hoc control-character metadata delimiters could be present in real commit subjects and corrupt history/snapshot fields. | NUL-framed fixed-field records; tests with tabs and control characters in commit/snapshot metadata. Empty/unborn logs are handled without hiding ordinary log failures. | Fixed |
| 12 · MEDIUM | `src/ui/drafts.rs:10`, `src/settings.rs:169` | Commit/PR text was lost across branch changes or restarts; damaged preferences risked being overwritten. | Per-project/per-branch drafts, periodic/switch/exit saves, atomic persistence; preserve corrupt source and keep failed writes retryable. Round-trip/corruption and branch restoration tests pass. | Fixed |
| 13 · MEDIUM | `src/github.rs`, `src/ui/pull_request.rs:4` | PR browsing/creation lacked a coherent review/lifecycle surface and conventional templates. | Structured detail view, summary review types, lifecycle and merge choices, templates, reviewers/labels, explicit base/head and stdin body. Tests verify commands and named controls. Inline threads and richer account/fork selection remain in the matrix. | Expanded; live acceptance open |
| 14 · MEDIUM | `src/image_diff.rs:23`, `src/ui/images.rs` | Binary image changes had no visual review. | Side-by-side/overlay/swipe, exact Git blob/index/worktree bytes, additions/deletions, bounded decode. Tests cover staging separation and invalid images. Supported formats and limits are explicit; SVG/PSD remain external. | Implemented |
| 15 · MEDIUM | `scripts/Build-Windows.ps1`, `.github/workflows/build.yml` | Distribution had no repeatable artifact identity or exact pointer to the newest package. | Added versioned EXE/ZIP, SHA-256 manifest, exact latest pointer, all-feature checks and documented gaps. Native/live-network/signing and non-Windows acceptance are separate, unverified release gates below. | Packaging fixed; acceptance separate |

## Component critique and coverage

| Component / feature | UX and correctness assessment after repair | Remaining practical limit |
| --- | --- | --- |
| Global navigation, repository and branch picker | Repository/branch context is top-level. Search groups on-PC and last-fetched remote branches; switching reuses tracking branches. Compact toolbar targets do not overlap in the exercised 530×350 logical viewport. | Native menu/dialog focus and multiple physical monitors need OS acceptance. |
| Changes and commit | File inclusion, lock state and staged review are separate. Diff tools are contextual; narrow view gives each task room. Final commit shows exact included files and rechecks content. | File trees may still be expensive when explicitly selected for enormous change sets; flat list is the automatic large-set default. |
| Unified/split/image diff | Equal panes, stable gutters and synchronized scroll retain code alignment. Image comparisons distinguish staged and working bytes. | Image limits: 16 MiB source, 8 megapixels, first GIF frame; no SVG/PSD or external diff-tool wizard. |
| History | HEAD-only graph/list, independent pagination, search cache, selected inspector and IDE worktree actions. Revert and reset remain distinct. | Not a complete interactive rebase or bisect editor. |
| Snapshots | Parent-correct tree, named cards, metadata on demand, vertical compact hierarchy, A/B comparison, independent IDE worktrees, remove/restore and branch promotion. | Local refs are not backup; ignored files/nested repository objects are not captured. No retention/purge UI. |
| Branch map and Merge | Shared-ancestor relationships have text divergence states. Source/destination and baseline effects are explicit. Fetch/update revalidates state and leaves conflicts recoverable. | Counts observe fetched refs; no continuously live server-state guarantee. |
| Pull requests | Browse/create/review are separate tasks. Details, checks, summary reviews and pinned-head merge choices share one context. Creation has direction, template, reviewers/labels and final review. | Inline review threads, rich author/reviewer picker, advanced fork routing and complete ruleset guidance are absent. |
| Conflicts | Base/current/incoming/result workflow; stale-save and unresolved-marker refusal; operation continue/abort/skip. | Complex binary/submodule/rename conflicts may require external tools; hooks/filters need environment-specific testing. |
| Compare & recovery | Destructive history edits are constrained and checkpointed. Working-file recovery preserves the newer version before replacement. | Remote publication checks rely on fetched refs; advanced command overrides are intentionally outside normal guards. |
| Stashes | Apply-and-keep is prominent; destructive pop/drop is reviewed. | Standard Git stash semantics and conflict behavior still apply. |
| Repositories/worktrees/IDE | URL cloning, remotes, clean worktree removal, project/default/custom IDE configuration are discoverable. | Account clone picker, Desktop URL protocol and OS integration parity remain incomplete; installed IDE launch acceptance is manual. |
| Command center | Exact argument vectors with command review; noninteractive examples; activity log remains available. | A command runner is not a GUI for every Git option. It is not a shell or security sandbox. |
| Settings/setup/errors | Missing gh has an install/recheck path, About identifies tools/build, drafts persist, corrupt settings are preserved, progress is actual stderr. | No signed auto-update/install flow; translations/RTL not implemented. |

## Interface domain coverage

| Domain | Checked | Result / boundary |
| --- | --- | --- |
| Accessibility | Named controls on every screen and selected states; keyboard branch picker; modal Escape/shortcut isolation; compact target bounds; cancellation dialog | Framework checks pass. NVDA/Narrator, native dialogs and full OS focus order **not verified**. |
| Layout | Wide renders, narrow 200% renders, headers, columns, trees, composer, inspector, activity/navigation overflow | Concrete overlap/clipping defects repaired. Other physical DPI/monitor combinations and localization **not verified**. |
| Writing | Direction labels, local/remote observations, empty/missing-tool states, revert/reset effects, actual mutation labels, recovery consequences | Updated helper text/confirmations; no invented transfer percentage or universal parity claim. |
| Typography | System/bundled fallback, missing glyphs, bounded badges, title/body/code hierarchy, full metadata access | Glyph failure repaired; unsupported scripts can still show tofu; comprehensive multilingual shaping **not verified**. |
| Color | 25 declared opaque text/action/graphic pairs via contrast script; text alternatives for status | All declared pairs pass; transparency, antialiasing and color-vision simulation are outside automated ratios. |
| UI polish | Disclosure geometry, consistent tokens, primary/secondary grouping, scroll cues, contextual options, compact transitions | Inspected framework renders pass for covered states. Full OS-level interaction remains a gate. |

## QA evidence

The final Windows build script executes:

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --all-features --locked
cargo build --release --locked
```

**Result: 66 tests passed: 57 real-Git integration tests and 9 framework UI tests.** The ordinary suite excludes one graphics-dependent render exporter, which was also executed explicitly and passed:

```powershell
cargo test --all-features --bin branchroom render_audit_screens --locked -- --ignored --nocapture
python scripts/Check-Contrast.py
```

Framework renders cover all twelve navigation screens, selected patch/snapshot states, the branch menu, structured PR details, and compact Changes/History/Snapshots/GitHub/Settings. The compact test uses 530×350 logical points at 200%, producing 1060×700 pixels. It asserts toolbar target separation and reachable diff/commit controls; it does not pretend that every row in a scrollable area must be on screen simultaneously. The developer preview bar is omitted in test builds so it does not distort production layout.

Saved screenshots were removed for privacy. UI renders can be regenerated from synthetic fixtures under `target/qa/ui`.

Tests include real merge/revert conflicts and abort, undo/squash/reorder failures with unchanged content, snapshot/index/HEAD/worktree isolation, NUL metadata, locks and line staging, stale commit plans, discard restoration, tracking branch reuse, unrelated branch histories, moving baseline statistics, and fetch/merge without pushing. UI tests cover project-switch action cleanup and per-branch draft restoration in addition to basic rendering.

**Not verified:** native Windows input/capture and full screen-reader behavior. The Computer Use native pipe was unavailable after retry/reset. The `gh` executable was not installed in this environment, so authenticated PR publication/reviews/merges, Enterprise permissions, fork routing and rate-limit/network failure handling were not accepted end to end. No external GitHub writes were attempted. macOS/Linux CI is configured but was not executed locally. Framework WGPU renders are not a substitute for those checks.

## Refresh performance

The benchmark creates its own temporary repository: 200 main commits, 64 ancestor topic branches, 64 diverged branches, 129 total branches. It calls the same repository load path three times and counts actual subprocesses. It cleans only its own generated directory.

| Measurement | Before | After |
| --- | --- | --- |
| First recorded refresh | 16,292 ms / 275 processes | 1,313 ms / 20 processes |
| Median of three | 15,617 ms | 1,116 ms |
| Subsequent subprocess count | 275 | 19 |

Data: [before](qa/refresh-before.json), [after](qa/refresh-after.json). Reproduce with `cargo run --release --locked --example measure_refresh`. These are local wall-clock observations, not a universal speed guarantee or a statistically controlled multi-machine benchmark. The fixture is branch-heavy; huge monorepos, networked filesystems and expensive filters need their own profiling.

The principal change is structural: `for-each-ref` batches ahead/behind; one bounded parent DAG supplies most merge-base answers; fallback handles truncated or unrelated histories. The bounded cache uses repository/base/tip object IDs, so a moving baseline cannot reuse old answers. UI state uses Arc; graph/search results are cached. This is not achieved by reducing the visible history scope incorrectly.

## Dependency and release review

Current versions were checked against primary package documentation; Cargo.lock is included. `cargo-audit 0.22.2` against the RustSec database reported **zero known vulnerable advisories** for the current lockfile, with three maintenance warnings through the Linux file-dialog portal dependency chain:

- `zbus_names`: RUSTSEC-2026-0371
- `zvariant`: RUSTSEC-2026-0370
- `zvariant_derive`: RUSTSEC-2026-0372

Machine-readable summary: [security-audit.json](qa/security-audit.json). These are upstream maintenance advisories, not evidence of an exploitable defect in this Windows build. They still need tracking before a Linux release; forcing an incompatible major version is not a verified fix. An advisory scan is not a security certification. Repository hooks, filters, IDEs and custom commands run with the user's permissions.

The packaging script now produces an identified Windows x64 package with SHA-256 file manifest and ZIP checksum. It does not kill an existing app to overwrite a locked EXE. A versioned output remains available, and `dist/latest.json` points to it. The build is unsigned. Release signing, installer/update design and external acceptance require actual credentials/infrastructure and are not fabricated here.

Before general production release, complete:

1. A disposable GitHub repository acceptance run: login/logout, clone, publish, PR create/template/reviewers/labels, PR reviews/lifecycle/merge queue, stale-head refusal, required checks, fork/Enterprise access and offline/rate-limit errors.
2. Native Windows keyboard/focus/Narrator or NVDA and file-dialog checks, custom IDE launches, high-DPI/multiple displays, long paths and hooks/filters.
3. Signed distribution and upgrade/data-retention acceptance, plus explicit support policy for Git/GitHub CLI versions.
4. Actual Linux/macOS CI and runtime checks before advertising those platforms; resolve/track Linux portal maintenance warnings.
5. Decide which remaining migration gaps in the matrix are release requirements. Do not label those workflows complete because a raw command can invoke them.

## Primary references

- [Git branch field batching](https://git-scm.com/docs/git-for-each-ref) and [Git 2.41 introduction](https://github.blog/open-source/git/highlights-from-git-2-41/)
- [GitHub Desktop commit management](https://docs.github.com/en/desktop/managing-commits/options-for-managing-commits-in-github-desktop)
- [GitHub review model](https://docs.github.com/en/pull-requests/reference/pull-request-reviews)
- [GitHub CLI PR creation](https://cli.github.com/manual/gh_pr_create), [review](https://cli.github.com/manual/gh_pr_review), [head-pinned merging](https://cli.github.com/manual/gh_pr_merge)
- [Git revert](https://git-scm.com/docs/git-revert), [reference transactions](https://git-scm.com/docs/git-update-ref.html), [index flags](https://git-scm.com/docs/git-update-index)
- [eframe 0.37.0](https://docs.rs/crate/eframe/latest), [image 0.25.10](https://docs.rs/crate/image/latest), [official framework test harness](https://docs.rs/egui_kittest/latest/egui_kittest/)
