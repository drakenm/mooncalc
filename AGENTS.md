# Agent working instructions

## Documentation navigation

Use [the documentation map and conventions](docs/README.md) when adding or moving documentation. Keep current guidance, proposed work, and historical evidence distinct. After adding, moving, or renaming a document, update the map and check that every relative link you touched resolves.

## Scope and default workflow

These instructions apply throughout this repository. Follow explicit user instructions for the current task when they override this workflow.

Develop work in dedicated branches and worktrees. Merge ready work through pull requests into its parent: a feature parent or the current release branch. The release branch `release/vX.Y.Z` collects the work for one release, is validated and fixed as a whole, and is the only branch that normally merges into `master`. Each release is tagged on `master`. Continuing feature parents receive released changes from `master` and remain available for further development.

Adding or changing these instructions does not itself authorize executing a release, tagging master, publishing branches, opening or merging pull requests, or reorganizing existing worktrees. Perform those actions when they are part of the requested work. Within an assigned development task, complete local implementation, verification, commits, and ready parent promotion without repeatedly asking for approval when the parent and scope are already established.

## Branch roles and direction

- `master`: released, validated history. MoonCalc is a desktop app, so nothing is deployed; each release is published as GitHub release downloads. Only release branches merge into it, plus the occasional hotfix (see [Hotfixes](#hotfixes)). Tag releases here.
- `release/vX.Y.Z`: the integration branch for one release. It starts from `master` and combines the selected feature and bugfix work for checks and integration fixes. CI runs on pushes and pull requests to `release/v*`.
- `feature/<name>`: the parent for an ongoing feature, collecting coherent improvements. A simple feature can use one implementation branch without an extra parent layer.
- Child or section branches: focused implementation work descending from the assigned parent or the previous completed section.
- `hotfix/<name>`: an urgent production fix that cannot wait for the next release branch.
- Temporary integration or compatibility branches can combine work for testing. They never merge into `master` or back into source parents.

Normal flow:

```text
child/section work -> feature parent --+
                                       +-> release/vX.Y.Z -> master -> vX.Y.Z-rcN -> vX.Y.Z
bugfix, chore, docs, ... work ---------+

master -> continuing feature parents
master -> next release/vX.Y.Z
```

Arrows show promotion or merge direction, not permission to merge every branch automatically. Every merge into a pushed branch goes through a GitHub pull request. Do not merge release branches back into source parents: that spreads unrelated, unreleased work into them. Merging a branch does not close it; later commits can be merged in a later pull request.

### Branch naming

Name branches by the organization's [version control guide](https://github.com/labor-law-compliance-center/documentation/tree/master/application-development-guide/version-control-guide):

| Prefix | Purpose |
| --- | --- |
| `feature/` or `feat/` | New feature |
| `bugfix/` or `fix/` | Bug fix |
| `hotfix/` | Urgent production fix |
| `release/` | A release, as `release/vX.Y.Z` |
| `chore/` | Maintenance, tooling, config |
| `docs/` | Documentation only |
| `refactor/` | Code restructuring |
| `test/` | Adding or improving tests |
| `ci/` | CI/CD pipeline changes |
| `experiment/` or `spike/` | Temporary or exploratory work |
| `dependabot/` | Automated dependency updates |
| `update/` | General code change |

Every branch pushed to GitHub must follow this scheme. Local worktree branches created by tooling, such as `t3/<id>`, stay local: rename the branch, or create a correctly named one from it, before pushing or opening a pull request. Do not use `codex/` as a prefix.

## Inspect and plan before changing files

1. Read applicable agent instructions and inspect the repository, relevant code, scripts, and conventions.
2. Run `git status --short --branch`, identify the current branch and commit, and inspect `git worktree list` before switching branches or making changes.
3. If the current worktree is dirty, stop and explain what is present. Do not stash, overwrite, reset, revert, or commit existing user changes to make the workflow proceed. Also check the destination worktree before any promotion or refresh; leave a dirty destination untouched and report the pending operation.
4. If no Git repository exists, initialize it with `master` as the default branch.
5. Organize the requested work into meaningful implementation sections before editing. Identify the starting branch, the parent to receive completed work, and the release branch it targets if one is part of the task.
6. For a new independent task, start from the current release branch unless the user identifies another base. For a child task under an established parent, start from that parent's clean tip. Do not guess a parent from similar names when several candidates exist; inspect the task context and history, and clarify only if still ambiguous.
7. Name branches by [Branch naming](#branch-naming).

## Worktrees and concurrent work

Use separate worktrees for independent work. Before creating a worktree, inspect existing worktrees and reuse an appropriate clean one when practical. Never force the same branch to be checked out in multiple worktrees, remove another worker's worktree, or manipulate a checked-out branch behind that worker's back.

Perform local parent promotions in the parent's worktree if it is already checked out. Verify its status and current tip immediately before promotion. Coordinate with active work there; do not switch or merge underneath another agent's work. If a destination is occupied or unsafe to update, keep the completed child commits intact and report the promotion as pending.

Parallel child branches can start from the same parent, but only one may remain eligible for a fast-forward after another is promoted. Do not assume independent work will retain a linear relationship.

## Stack implementation sections linearly

For meaningful sections of one task:

1. Create section 1 from the chosen clean base.
2. Implement, verify, and commit section 1 in focused commits.
3. Create section 2 from the completed section 1 tip, not from the original base.
4. Continue that pattern for later sections.

```text
parent -> section-1 -> section-2 -> section-3
```

Prefer fewer meaningful branches over tiny branches that add no review value. Keep tightly coupled, very small work on its parent section branch. If an unnecessary section branch has already been created, fold it into its parent, preferably by fast-forwarding, and report why it was consolidated. Never squash or rewrite commits that have already been pushed or merged into a release branch.

A merge includes the selected commit's entire ancestry. Do not offer a later stacked section as an independent change without explaining its earlier dependencies. If releases need independently selectable improvements, develop them on independent branches from a suitable common base rather than treating a combined parent as a source of arbitrary subsets.

## Commits, dependencies, and verification

- Use focused conventional commits: `type(scope): imperative description`, with scope optional. Types follow the version control guide: `feat`, `fix`, `docs`, `style`, `content`, `refactor`, `perf`, `test`, `chore`, `ci`, `build`, `revert`.
- Keep the code, tests, documentation, and fixtures for one behavior together. Do not mix unrelated work.
- Before every commit, check every touched text file for CRLF and store it with LF endings, following `.gitattributes`. Check actual file bytes, not just Git's normalized diff. Do not normalize unrelated files or inspect binary assets as text.
- Review staged changes and run `git diff --cached --check` before committing. Stage specific intended paths.
- Run checks appropriate to the change. This is a Rust workspace; the required checks are:
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - `cargo build --target wasm32-unknown-unknown -p mooncalc-core -p mooncalc-visibility -p mooncalc-maps -p mooncalc-hijri -p mooncalc-atlas -p mooncalc-app` when touching those portable crates (see the portability rule in [architecture](docs/architecture.md)).
  - `npm run build` / `npm run check` in `frontend/tauri/` when touching the TypeScript frontend. npm packages follow the same 14-day pinning rule, with exact versions and a committed lockfile.
  - `cargo build -p mooncalc-gui --features de440` when touching the SPK/DE440 backend or GUI feature wiring.
  - On this machine the toolchain lives in `%USERPROFILE%\.cargo\bin`, which is not on `PATH`: prefix commands with `export PATH="$USERPROFILE/.cargo/bin:$PATH";` in Bash.
- Astronomy code must cite its source (book chapter, paper, or dataset) in doc comments, and carry tests against published reference values. Validation fixtures live in `tests/fixtures/` and are described in [validation](docs/validation.md).
- Documentation-only changes need content review, whitespace checks, and line-ending verification, not test runs.
- Prefer no new dependencies. When needed, select and pin a suitable version released at least 14 days ago after checking known vulnerabilities. Prefer the newest eligible safe version. If a serious vulnerability requires a newer security fix, document the reason for the age-policy exception. Update the lockfile with dependency changes.
- For performance work, record a reproducible baseline and after-change measurements using comparable data, workload, environment, and methodology. Verify correctness as well as speed. Report measurement limits and unavailable checks rather than claiming unmeasured gains.

## Promote ready work into its parent

A pause alone is not a promotion trigger. Commit unfinished work on its child branch and describe what remains. Promote at a pause only if the checkpoint is coherent, verified, and safe for other branches to consume. A completed, verified section can be promoted even when the larger feature continues.

When the parent has been pushed, promote through a pull request from the child into the parent. Merge it with a merge commit; do not squash-merge or rebase-merge, because later merges depend on the original ancestry staying recognizable.

When the parent is local only, promote a ready checkpoint with a fast-forward-only merge in the clean worktree containing the parent:

```sh
git status --short --branch
git merge --ff-only feature/search-filters
```

Substitute actual branch names. A recorded commit SHA can be used instead of a moving branch tip when the parent should consume only a specific checkpoint. Verify that the selected commit is reachable from the child and descends from the parent's current tip. Record both the previous parent tip and the promoted commit.

A successful fast-forward moves the parent reference to the ready child commit without creating a merge commit. Verify the resulting parent tip and worktree status. The child branch can remain available for further development; merging it does not delete or finish it automatically.

### When fast-forward is impossible

`--ff-only` failure is a signal to inspect history, not to force the parent pointer.

- If the child is private, unpushed, and has no downstream consumers, rebasing its unpromoted commits onto the new parent may be appropriate. Re-run affected checks before promotion.
- If the child has been pushed, has dependent branches, or has been merged into a release branch, preserve history. Use a normal merge, or a pull request when the parent is pushed, when combining that work is within the task scope, resolving conflicts and rechecking the result.
- If that merge would import unrelated work or make the parent unsuitable for consumption, retain the child and explain the specific integration decision needed.

Never reset the parent backwards, force-push, or rewrite pushed history merely to obtain a fast-forward. Linear section stacks are the default for sequential work; legitimate concurrent development can require merge commits.

## Assemble and stabilize a release branch

1. Start a new `release/vX.Y.Z` branch from the intended clean `master` commit, or use the assigned release branch.
2. Open pull requests from verified feature, bugfix, and other work branches into the release branch. Record the source branch and exact head SHA for each one so a moving source branch cannot silently change the release contents.
3. Merge those pull requests with merge commits. Do not squash-merge, rebase-merge, or routinely cherry-pick work into the release branch.
4. Run the combined checks required for the release: `ci-required` on the release branch, supplemented by relevant end-to-end, database, and performance checks. Resolve and verify conflicts in the receiving branch.
5. Repeat checks affected by integration fixes. Record unresolved failures or blocked checks; do not call a release branch release-ready while required checks are outstanding.

A release branch couples the selected changes for release. If one change is not ready, fix it, exclude it before merging, use an appropriate feature flag, or deliberately revert it. Do not assume a later merge automatically restores reverted changes: Git still remembers the original merge, so reintroduction requires an explicit fix or revert-of-revert strategy.

### Put fixes at the correct source

- A defect intrinsic to a feature belongs on its source branch. Verify and promote the fix there, then merge the new checkpoint into the release branch.
- A conflict resolution or behavior change specific to the assembled combination belongs on the release branch. Continuing parents receive it through `master` after release.
- If an urgent fix was made directly on the release branch and a source parent needs it before release, a focused cherry-pick back can be justified. Inspect its dependencies, preserve traceability with `git cherry-pick -x`, and verify it on the parent. This is an exception for a fix, not the normal promotion path. Note the duplicate patch when later merging `master` back.

To test ongoing source work against unreleased release-branch changes, use a temporary compatibility branch or worktree that combines them. Keep that combined history out of the permanent source parent.

## Release to master and tag

When releasing is part of the requested task:

1. Inspect the current `master` tip, release branch tip, existing version tags, and release conventions. Fetch relevant remote refs when needed and available; do not assume stale local refs represent the published state.
2. If `master` advanced outside the release branch, merge it into the release branch and verify the combined result before release.
3. Open a pull request from `release/vX.Y.Z` into `master` and merge it with a merge commit once `ci-required` passes on the pull request. Do not squash or rebase the promotion. The pull request's CI already tested the merged result, so CI does not run again on `master`: after the merge, check that the `master` tree matches the tested release branch head (`git diff <release head> origin/master` is empty) and tag straight away. If the trees differ, validate the actual release result before tagging.
4. Version tags are `vX.Y.Z`, always with three components, plus an optional suffix: `-rcN` for a release candidate, or `-hf<issue>` for a hotfix, where `<issue>` is the GitHub issue number the hotfix resolves. Determine the version from the release branch name and existing tags. Increment the appropriate semantic version component for the change; do not blindly increment an unrelated package version. If release intent leaves the version materially ambiguous, clarify it before creating the tag.
5. Immediately after the merge, create an annotated release-candidate tag `vX.Y.Z-rcN` on the `master` merge commit and push it. The tag triggers the Release workflow (see [CI](docs/ci.md)), which builds every platform and variant and publishes them as a GitHub pre-release. That pre-release takes the place of staging: its downloads are what gets tested. Each later fix merged to `master` gets the next candidate number. Never tag a pull request branch tip: the release must be built from the `master` merge commit, which has a different SHA.
6. When the candidate's downloads are confirmed, create the final annotated `vX.Y.Z` tag on the same commit as the confirmed candidate and push it. Don't add a new commit for it. The Release workflow builds that commit again and publishes the GitHub release, marked latest. What was tested and what is released are then the same commit.
7. Check that a tag does not already exist. Never move, delete, or overwrite a release tag.
8. Push branches and tags only when publishing is within the requested scope. Report local-only releases accurately. Deployment remains a separate action governed by the requested scope and repository release process.

No version tag is needed for an ordinary implementation commit or parent promotion.

### Hotfixes

Every hotfix references a GitHub issue. A production defect that cannot wait for the next release branch first gets an issue (and its `GHI-` entry in the [local GHI tracker](docs/local-ghi-tracker.md)), then a `hotfix/<issue>-<name>` branch from `master`, for example `hotfix/301-login-redirect`. Open its pull request into `master` with `Closes #<issue>`; this is the exception to the release-branch-only rule. Tag the merge commit `vX.Y.Z-hf<issue>` as soon as it lands, where `X.Y.Z` is the next patch version, for example `v1.5.1-hf301`; the tag publishes the release like any other. Then merge `master` into the current release branch so the fix is not lost.

## Refresh and continue after release

In each clean, continuing feature-parent worktree, explicitly merge `master` into the parent:

```sh
git status --short --branch
git merge master
```

Use the verified release-containing `master` ref. If ongoing commits already extend beyond the released checkpoint, this can create a merge commit; preserve them and resolve conflicts without dropping that work. Re-run affected checks. Never reset a continuing parent to `master` or rebase its pushed history. When the parent is pushed, do the refresh through a pull request or push the merge commit, as the task scope allows.

Child branches can incorporate the refreshed parent by merge, or by rebase only when private and safe to rewrite. Do not modify another active worker's branch without coordination.

Keep a released `release/vX.Y.Z` branch as a historical reference. Start the next release branch from updated `master` when the next release task starts.

Use explicit fetch and merge steps when remote synchronization is needed rather than relying on `git pull` configuration to choose between merge and rebase. Do not delete branches or worktrees automatically after release.

## Record follow-up tasks

The [local GHI tracker](docs/local-ghi-tracker.md) is an offline mirror of the GitHub issues, covering bugs, features, and follow-up work. Read it instead of querying GitHub when you need the backlog.

When work turns up something worth doing later (a deferred item, a bug noticed in passing, a failed or unmeasured check, an operational step, or a design limit), file a GitHub issue with `gh issue create`, labelled `bug` or `enhancement` as appropriate. Then add a `GHI-<issue number>` entry to the file in the same branch as the work that found it. When work resolves an issue, reference it in the pull request (for example, `Closes #285`) and move its entry to **Closed** in the same branch. Never invent IDs; the issue number is the ID. Items under **Untracked** were noted before they had an issue: when you file one, move it to **Open** under its `GHI-` ID. Present new and updated entries to the user as they occur.

## Required handoff report

At task completion or a pause, report:

- The implementation sections chosen.
- Worktree path(s), starting base branch and commit, and designated parent branch(es).
- Branches created and their ancestry.
- Commits made on each branch, using short SHAs and descriptive subjects.
- Pull requests opened or merged, with links.
- Parent promotions performed, including the promoted checkpoints, or the concrete reason promotion remains pending.
- Branches consolidated, merged, or squashed back into their parent and why; explicitly state when none were consolidated.
- Tests, benchmarks, whitespace and LF verification performed, with outcomes and any unavailable required checks.
- Release branch, `master`, tag, and publishing actions only if performed, including the exact release commit and version when applicable.
- Remaining work or known risks needed for the next agent to continue safely.
- Issues filed or updated, and the matching entries in `docs/local-ghi-tracker.md`, by `GHI-` ID.
