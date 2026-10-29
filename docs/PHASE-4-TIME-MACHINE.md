# Phase 4: Time Machine

Time Machine is a single vertical timeline. It combines Branchroom actions, commits, manual and automatic safety points, saved history tips, deleted safety points, discarded files and shelved work. The previous snapshot Tree/List/Inspector controls and duplicate recovery sections are removed. Existing data stays in its original storage and is mapped into timeline entries.

## Interaction

Select an entry to load its saved graph and changes automatically. Action entries let you switch between before and after. Restore opens a before/after graph review and a file-change summary, with Confirm and Cancel fixed above the status bar. Compare opens the shared graph comparison lens; Branch from here creates a branch. Deleting an ordinary timeline entry hides it while retaining recovery data; Advanced can recover hidden entries. Deleting a safety point moves it into a recoverable deleted entry. Shelved work has an inline diff and visible Apply, Apply and keep, and Delete actions.

Advanced retains saved-version patch inspection, comparison with working files, creating a branch, copying IDs and opening an independent copy in an IDE. Existing history forms remain accessible from Graph > Advanced; their duplicate recovery lists now live exclusively in Time Machine.

## Undo and safety

Ctrl+Z and Ctrl+Shift+Z undo and redo completed local Branchroom actions. The shortcuts leave text-editor Undo alone. Each recorded action pins the working tree, included changes, branch tips, tags and the complete shelved-work stack. This includes non-top shelved entries. The journal is persisted under the current Git directory in `branchroom-timeline`; private Git references keep saved objects reachable. A new action retires the redo chain. No-op writes do not consume an Undo step.

Restores validate their repository and reviewed state. Newer working edits, included changes, branch changes or shelved-work changes cause refusal. Ignored-file collisions are checked explicitly because Git can overwrite ignored files during a two-tree checkout. Branches open in another parallel workspace are protected. Safety data remains available if an action or journal update fails. Failed actions are kept for explicit recovery rather than treating possible external edits as a completed global Undo step. A completed conflict continuation connects back to its original action safety point.

Global Undo covers local Git state. It does not reverse server changes, application settings, external programs or create/remove parallel workspace directories. Finish or cancel an active Git operation before restoring; its safety points and discarded-file recovery remain visible. Full-state restore refuses nested Git repositories, sparse/assumed-unchanged file flags and unresolved index entries so those can be handled explicitly through Advanced commands. Previously discarded entries without recorded history still recover their saved file; newly saved entries also remember the commit at that point. Ignored files are preserved and are not included in safety points.

## Files changed in this phase

- `src/timeline.rs`: persistent action journal, checkpoint capture, restoration, Undo/Redo, legacy timeline mapping and visibility
- `src/recovery.rs`: backward-compatible history identity and saved-file previews
- `src/git.rs`: timeline data on repository refresh
- `src/lib.rs`: timeline module export
- `src/ui/time_machine.rs`: timeline, entry actions, inline shelved diffs, restore review and keyboard action routing
- `src/ui/experiments.rs`: retained capture and independent-copy forms; removed old snapshot tree UI
- `src/ui/jobs.rs`, `src/ui/mod.rs`: journal integration, background previews and restore tasks
- `src/ui/shell.rs`: global shortcuts and removal of duplicate Time Machine tabs
- `src/ui/compare.rs`, `src/ui/workspace.rs`: move recovery and shelved-work views into Time Machine
- `src/ui/preview.rs`, `src/ui/qa.rs`, `tests/repository.rs`: preview compatibility and regression coverage

## Verify

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --all-features --locked
cargo test --all-features --bin branchroom render_phase_four_timeline --locked -- --ignored --nocapture
```

The render export produces nine images under `target/qa/phase-four`: timeline, restore review and shelved work, each in dark, light and narrow layouts. UI tests also exercise 530×350 at 200% zoom, accessible controls, removal of the old snapshot modes and text-editor Undo.

For a manual check, use a disposable repository:

1. Include only part of a changed file, leave additional edits and an untracked file, then commit through Branchroom
2. Open Time Machine and inspect the action; press Ctrl+Z and verify the original included and working content returns; press Ctrl+Shift+Z to redo
3. Edit the file externally after an action and verify Undo refuses to replace the newer work
4. Select an older commit or safety point, review Restore, confirm, then undo that restore
5. Shelve three versions, delete the middle one, and verify Undo restores all three in order
6. Recover a discarded file; delete and recover a safety point; hide and recover a timeline entry
7. Resize to a narrow window, zoom to 200%, and verify Confirm and Cancel remain above the status bar

Verification result: 140 automated cases passed across the full suite and the final targeted regression (1 library, 49 UI, 90 repository cases). The full run passed before the final failed-action regression was added; that case passed separately. The final UI suite passed after the Windows executable-lock retry. Strict Clippy, formatting, whitespace checks and the native debug build passed. All nine final screenshots exported successfully; dark/light, wide/narrow timeline, restore and shelved-work renders were inspected. Deleted branch recovery was also verified after expiring reflogs and running Git garbage collection.
