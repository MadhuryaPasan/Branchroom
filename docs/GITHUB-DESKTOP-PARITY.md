# Git / GitHub Desktop / GitHub web workflow review

Reviewed 26 October 2026 for Branchroom 0.2.0-rc.1. “Implemented” describes a dedicated workflow, not a guarantee of every option or completed live acceptance. A generic command runner does not establish GUI parity.

| Workflow | Branchroom | Verification / remaining gap |
| --- | --- | --- |
| Open, initialize, clone URL, recent repositories | Implemented | Disposable repository operations; directory paths and per-project IDE; no searchable account clone picker |
| Top-bar branch selection | Implemented | Search local / last-fetched remote groups, tracking reuse, dirty checkout carry/stash; shortcut and accessibility tests |
| File inclusion and partial commits | Implemented | Real Git tests for add/delete, replacement pairs, separated hunks, missing newline, stage/unstage, index isolation |
| Text differences | Implemented | Aligned split/unified, whitespace/full context, virtual rows, fixed gutters and synchronized scrolling; compact options menu |
| Raster image differences | Implemented in this pass | Side-by-side, overlay and swipe; PNG/JPEG/GIF/WebP/BMP/ICO; binary blob/index/worktree tests; bounded decode |
| SVG, PSD and external diff tool integration | Missing dedicated workflow | Use an external editor; first-frame GIF previews do not play animations |
| Commit summary, body, amend, co-authors | Implemented | Reviewed HEAD/index plan, stdin message, per-branch persisted drafts; no account autocomplete |
| Discard and recovery | Implemented differently | Recoverable local copies; restore backs up newer content and leaves index intact; no automatic retention cleanup |
| History | Implemented | Separately paginated HEAD graph, merged ancestry included, unmerged branches/PRs/snapshots excluded; source/commit IDE worktrees |
| Revert / cherry-pick | Implemented | Inverse commit preserves later work; merge parent selection; conflict/abort tests; cherry-pick uses Git's own operation state |
| Undo / reset / squash / reorder | Implemented with restrictions | Clean idle checkout, linear unpublished range up to 200 commits, remote-tracking reachability, recovery refs; no unrestricted interactive rebase editor |
| Merge / update from main | Implemented | Explicit direction; fetch exact baseline then switch/merge; rechecks state after fetch; never pushes automatically; conflict resolution/abort |
| Text conflict resolution | Implemented | Base/current/incoming/result, stale-write refusal, conflict-marker refusal, atomic replacement, stage, continue/abort/skip; binary/submodule/complex rename conflicts may need external tools |
| Stash | Implemented | Include untracked, inspect, apply/keep, pop/drop; normal Git stash semantics |
| Publish / fetch / fast-forward pull / push | Implemented | Tracking-aware status, exact target force-with-lease review, actual transfer stderr, cancellation; publication is observed from last-fetched refs |
| Local vs remote status | Implemented | No-upstream, missing upstream, remote-only, local tracking and remote tracking distinguished; not a live connectivity indicator |
| GitHub and Enterprise credentials | Delegated to gh / Git credential tooling | Missing-tool setup guidance and discovery; user-controlled sign-in; no second token store; live accounts unverified here |
| PR list / preview / checks / independent checkout | Implemented | Structured cards/details, diff/checks via gh, related commit PR lookup, isolated IDE worktree; no local checkout change for inspection |
| PR creation | Expanded in this pass | Explicit base/head, stale-head check, title/body/draft, conventional templates, reviewers/labels by text entry; stdin body; no rich account search or advanced fork routing |
| PR summary reviews and lifecycle | Added in this pass | Comment/approve/request changes, ready/close/reopen; merge/squash/rebase pinned to reviewed head; argument and accessibility tests; authenticated writes unverified |
| Inline review threads, requested-change conversations | Missing dedicated workflow | Open GitHub web; summary reviews do not replace inline discussions |
| Fork / create private repo / issues / Actions | Partial via gh controls | Reviewed command forms and read-only lists; no complete web-style issue editor, Projects board, Actions editor or ruleset UI |
| Author identity | Repository-local forms | Existing Git identity applies; no full email attribution/account mismatch guidance |
| Git signing, LFS setup, SSH setup | Existing tooling honored | No dedicated setup/troubleshooting wizard; custom Git hooks/filters need environment-specific acceptance |
| Submodules, tags, blame, reflog, object maintenance | Commands/forms available | Not a complete dedicated UI for every subcommand; interactive terminal commands unsupported |
| Bisect and interactive rebase orchestration | Missing dedicated workflow | Use Git terminal; cancellation is not an automatic operation rollback |
| IDE selection / Explorer / terminal | Implemented | Global/project/custom IDE, clone project settings, independent review/snapshot worktrees; native installed IDE launches still require manual acceptance |
| Snapshot trees / parallel experiments | Additional workflow | Private index/HEAD isolation; actual source parent grouping; remove/restore; two worktrees; promote; not an off-device backup |
| Per-project commit exclusions | Additional workflow | Local config, unstages safely, locks file and line staging, checks externally staged paths; advanced/external Git and integration operations remain independent |
| Distribution / OS integration | Release candidate | Versioned Windows x64 ZIP/EXE, checksum manifest; unsigned; no installer/updater/protocol handler; macOS/Linux CI configured but not run locally |

## Acceptance boundary

66 automated tests (57 real-Git integration and 9 framework UI tests) pass, plus an explicitly executed framework render export. Tests use generated repositories and isolated preferences/Git configuration. They do not touch personal repositories or submit external GitHub writes. All-feature lint and formatting checks pass. Details, screenshots, benchmark data, and release gates are in [PRODUCTION-AUDIT.md](PRODUCTION-AUDIT.md).

**Migration verdict:** core local workflows are present and substantially hardened. Complete GitHub Desktop replacement and complete GitHub web parity are not established. Missing dedicated workflows and unverified authenticated/native behavior remain visible above.

## Primary sources

- [GitHub Desktop commit management](https://docs.github.com/en/desktop/managing-commits/options-for-managing-commits-in-github-desktop)
- [Reviewing changes in GitHub Desktop](https://docs.github.com/en/desktop/making-changes-in-a-branch/committing-and-reviewing-changes-to-your-project-in-github-desktop)
- [GitHub pull request review types](https://docs.github.com/en/pull-requests/reference/pull-request-reviews)
- [GitHub image comparison modes](https://docs.github.com/en/enterprise-server%403.18/repositories/working-with-files/using-files/working-with-non-code-files)
- [GitHub CLI create](https://cli.github.com/manual/gh_pr_create), [review](https://cli.github.com/manual/gh_pr_review), [merge and match-head-commit](https://cli.github.com/manual/gh_pr_merge)
- [Git revert](https://git-scm.com/docs/git-revert), [worktrees](https://git-scm.com/docs/git-worktree), [index flags](https://git-scm.com/docs/git-update-index), [reference transactions](https://git-scm.com/docs/git-update-ref.html)
