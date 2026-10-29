# Phase 3: Canvas actions and previews

Branch and commit gestures open a review before execution. The review draws the current and proposed history, explains the action, lists forecast conflicts, and exposes the Git command. Confirm stays disabled until the forecast is ready. Escape cancels; Ctrl+Enter confirms. Preview workers use disposable repositories and do not edit the checkout.

## Gestures

- Drag a branch onto another branch to merge; drag a commit onto a branch to cherry-pick
- Drop a commit near another row edge to reorder, or in its center to squash
- Drag the current branch tip onto an earlier commit to reset while keeping removed changes in the working directory
- Drag included working changes onto the working node to commit, or onto a commit to amend or fix up
- Use Update from main on a branch lane to review an update
- Open Advanced > Preview graph action for the keyboard form for every supported operation

The preview bar provides before/after graphs, conflict paths, commit count, message editing where needed, and Show Git command. Invalid gestures explain their reason. Branch map connections reuse cached conflict forecasts. Default-branch commits warn and offer moving commits onto a new branch. Published history rewrites are refused; the existing revert path remains available.

Every confirmed action saves automatic working, index and history safety points where applicable. Application checks that the branch, source, staged files, working files, identity and relevant merge/signing policies still match the preview. Literal branch references prevent tags with the same name from redirecting operations. Custom merge drivers and signing-sensitive imported rewrites require the exact Advanced command fallback. Exact commands preserve their flags and require typed confirmation for writes.

The conflict resolver shows current, incoming and result side by side, stacks at narrow widths, loads sides automatically, and retains the previous file in recovery before saving. Existing base/side inspection remains in Advanced. Unresolved-file removal and abort commands preserve available file backups and history points.

## Files

Planner and safety: `src/operation.rs`, `src/history.rs`, `src/commit.rs`, `src/conflict.rs`, `src/recovery.rs`, `src/git.rs`, `src/collaboration.rs`, `src/lib.rs`.

Canvas and forms: `src/ui/manipulation.rs`, `src/ui/history.rs`, `src/ui/workspace.rs`, `src/ui/comparison_canvas.rs`, `src/ui/changes.rs`, `src/ui/mod.rs`, `src/ui/jobs.rs`, `src/ui/dialogs.rs`, `src/ui/views.rs`, `src/ui/shell.rs`.

Verification: `src/ui/qa.rs`, `tests/repository.rs`.

## Verification and manual checks

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --all-features --locked
cargo test --all-features --bin branchroom render_phase_three_previews --locked -- --ignored --nocapture
```

In a disposable repository, make two branches with separate commits. Drag one branch onto the other, inspect the dashed proposed graph and command, cancel, then repeat and confirm. Repeat with overlapping edits to see conflict paths. Try commit-to-branch, commit-to-commit center and edge drops, and current-tip reset. Include a file before testing working-copy commit/amend. Change the repository after a preview and verify confirmation refuses the stale plan. Inspect automatic safety points in Time Machine. Test Advanced forms and exact custom flags. Resize the window to 530 pixels and test Escape, Ctrl+Enter and keyboard focus.

Forecasts stop at the first conflict encountered during a replay; later conflicts become knowable after that conflict is resolved. Linear history edits are limited to 200 commits per review. Git hooks and network services can introduce execution-time failures beyond a local forecast; safety points remain available.

Verification result: the full suite passed (1 library, 45 UI and 78 repository tests; 9 visual exports excluded from the normal run). The Phase 3 visual export passed separately and produced eight dark/light, wide/narrow forecast and resolver images in `target/qa/phase-three`. Strict Clippy, formatting and whitespace checks passed. The final default-branch and insertion-marker refinements were followed by the UI suite and the remote-default regression test.
