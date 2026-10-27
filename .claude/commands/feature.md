---
description: Run one task through the full pipeline in an isolated worktree (audit, plan, tests first, implement, inversion evidence, platform verification, risk-tiered review, validate, commit, draft PR, run report).
argument-hint: <task description, or a task from docs/_working/task-queue.md>
---

# /feature - single-task pipeline

Run the task below end to end in its own worktree. Follow the `project-discipline` skill
throughout. You (the main session, on Opus) own every decision; the subagents do bounded work.
Never touch the shared main tree: every code change happens in the task worktree, staged by exact
path.

**Task:** $ARGUMENTS

Work the steps in order. If a step hard-fails or a decision must be parked, stop and record it in
the run report (step 10) instead of guessing.

## 1. Worktree and branch

- Branch name from the task: `feat/<slug>`, `fix/<slug>`, `chore/<slug>` or `docs/<slug>`.
- Sibling worktree, so parallel tasks never share a tree:
  ```
  repo="$(git rev-parse --show-toplevel)"; name="$(basename "$repo")"
  wt="$(dirname "$repo")/${name}-worktrees/<branch>"
  git worktree add -b <branch> "$wt" main
  ```
- If the task touches `windows/` and a `package.json` exists there, install frontend dependencies in
  the worktree. Swift packages resolve on first build.
- Every later command runs inside `$wt`.

## 2. Audit, then plan

- Spawn **auditor** (haiku) on the task. Save its audit to `docs/_working/audit-<branch>.md`.
- Spawn **planner** (opus) with the task and the audit. It returns a scoped plan (chunks, tests
  first, cross-platform call, riskiest control, verification plan, out-of-scope) or a **PARKED**
  block.
- If PARKED: write the decision to the run report and stop. Do not guess.
- Read the plan yourself and challenge it before any code is written. You are the manager.

## 3. Tests first, then implement (chunked)

- Spawn **test-author** (sonnet) with the plan. It adds the failing tests and shows them red.
- Spawn **implementer** (sonnet) once per plan chunk, each with a fresh context, in order (for
  example ShepitCore, then the macOS app, then Windows). Each loops on `scripts/agent/gate.sh` until
  green. The Stop hook runs the same gate.

## 4. Inversion evidence

On the control the planner named: record the original lines, break the control, run only the
relevant test and show it red, restore the lines verbatim, show it green, and confirm `git diff` on
the file is empty after the restore. This runs once per branch, in the implementer report.

## 5. Platform verification (once per branch)

- Run `scripts/agent/gate.sh --commit` in the worktree. It runs `make build` when `apple/macOS/**`
  changed.
- **macOS UI**: render the changed view offscreen with `NSView.cacheDisplay` and inspect the image.
  Never capture the screen; it may show private windows. This rule has no exceptions.
- **Windows**: `cargo test` locally (the gate does this) plus the Windows CI job on the draft PR.
  Wait for the job result before calling it verified.
- **Needs device check**: anything only a person on real hardware can verify (microphone, global
  hotkey, Accessibility and other permissions, pasting into other apps, the installer, the updater
  against a real release, signing) goes in the run report under "needs device check". Never claim it
  as done.
- On failure, hand the output back to the implementer (step 3) and re-verify.

## 6. Council (risk-tiered)

- Match the diff against `sharedCore` in `pipeline.config.json`. A match gets the full council:
  **reviewer-domain** and **reviewer-security** in parallel. Anything else gets reviewer-domain; add
  security when secrets, permissions, spawned processes, downloads or the network are near the diff.
- Merge the findings into one severity-ordered punch list. Triage it yourself: drop findings that
  are wrong, decide disputed ones.
- Hand the list to the implementer to fix and re-run the gate. Repeat until the reviewers pass.
  Reviewers never edit code; they only name. Each saves its punch list under `docs/_working/` (a
  hook blocks any other path) and returns the verdict and the path.

## 7. Validate

Spawn **validator** (haiku). It checks the branch against the ORIGINAL task text and confirms the
inversion evidence is honest and that no device-only item is claimed as done. On REPROMPT, route it
to the implementer (step 3), then re-run the council and the validator. Only a PASS continues.

## 8. Commit

Commit in coherent chunks, staging exact paths (never `git add .`). Imperative subject under 72
characters, body when the reason is not obvious, multi-line messages through `git commit -F <file>`.
No assistant attribution lines. If the change altered documented behaviour, update `docs/spec/`, the
fixtures in `shared/`, and add a `docs/DECISIONS.md` entry when a future reader would otherwise have
to rediscover it. A user-visible feature gets a `WhatsNew.releases` item.

## 9. Push and draft PR

If `gh` is authed and a remote exists: `git push -u origin <branch>`, then
`gh pr create --draft --title "..." --body-file <notes>`. Otherwise leave the branch local and list
it. `main` is protected: squash merge only, through the PR.

## 10. Run report

Append one entry to `docs/_working/run-report-<date>.md`: branch and worktree path, what the task
did, gate results, platform verification results, inversion evidence, PARKED decisions, **needs
device check** items, and the draft PR link. This is what the human reads to accept or reject the
branch.

## 11. Learning journal

If something failed and was fixed (a red gate, a flaky step, a reviewer catch), append a short entry
to `docs/_working/learning.md`. If a rule should change, do not edit any skill, `CLAUDE.md`, hook or
setting: write it to the run report under "SUGGESTED RULE CHANGE" for a human.

## 12. Teardown

After the PR is merged, the human runs `scripts/agent/ship.sh <pr#>`, which merges, syncs main and
removes the worktree and local branch. Do not re-implement that by hand.

## Cost discipline

- Full reports go to `docs/_working/`; returned text stays short. Never paste a whole agent report
  into the conversation: summarize and point at the file.
- Implementer briefs are chunked, one layer each, with fresh contexts.
- Reviewers do not re-run the gate.
- The gate is one call: `scripts/agent/gate.sh`.
