---
description: Triage a set of tasks and run each through the /feature pipeline. Complex or shared-core tasks run alone and in sequence; only small, independent, well-specified tasks may run in parallel worktrees. Prints a review queue at the end.
argument-hint: <tasks inline, or empty to read docs/_working/task-queue.md>
---

# /orchestrate - the one command you invoke

Take the task set, triage it, and run each task through `/feature`. Follow `project-discipline`.
You are the manager and architect (run this session on Opus). You never touch the shared main tree:
all work happens in per-task worktrees.

**Tasks:** $ARGUMENTS

If no tasks are given above, read them from `docs/_working/task-queue.md` (one per list item, each
optionally tagged `[small]`, `[independent]` or `[complex]`).

## 1. Triage

Classify each task as COMPLEX or SMALL.

- **COMPLEX** (runs alone, in sequence) if any of:
  - it touches shared core (the `sharedCore` globs in `pipeline.config.json`): the spec, shared
    fixtures, the updater, insertion, CI workflows, signing config, the installer or the Makefile;
  - it changes behaviour that both platforms implement;
  - it is under-specified and would need a parked decision;
  - you are unsure. When unsure, treat it as COMPLEX.
- **SMALL** (eligible for a parallel worktree) only if all of:
  - it is explicitly small, independent and well specified;
  - it does not touch shared core;
  - its files overlap no other eligible task.

## 2. Merge-conflict guard

Never run two tasks in parallel if they touch the same files or the same area (for example both in
`apple/macOS/Shepit` settings, or both editing `docs/spec`). Check the eligible set pairwise. If you
cannot cheaply tell, assume they overlap and run them in sequence. Only one task at a time may run
`make build`: the Mac has one build slot and 16 GB of memory, so run Swift builds serially even for
otherwise independent tasks.

## 3. Run

- Fan-out is off unless a task is explicitly tagged small and independent. `max_parallel = 2`.
  COMPLEX tasks run one at a time.
- Run each task through `/feature` (worktree, audit, plan, tests first, implement, inversion,
  platform verification, council, validate, commit, draft PR, run report).

## 4. Resume, do not restart

If the human rejects a branch and re-invokes with feedback, resume on that branch and worktree from
the step that needs rework (usually implementer, council, validator). Do not start over and do not
open a new branch.

## 5. Print the REVIEW QUEUE

End with one block per branch:

```
BRANCH  <name>   (worktree: <path>)
  did:           <one paragraph>
  gate:          <fast pass/fail>   commit: <pass/fail>
  verified:      <macOS offscreen render / cargo test / Windows CI, as applicable>
  inversion:     <control broken -> red -> restored green, clean>
  parked:        <decisions parked, or none>
  device check:  <what a person must verify on real hardware, or none>
  draft PR:      <url, or "local only">
```

List parked tasks (not run) with the reason, and tasks held back from parallel runs by the guard.

## Cost discipline

- Prefer one COMPLEX task per session; queue the rest in `docs/_working/task-queue.md`. The run
  report and plan files carry state to the next session (`.claude/prompts/lean-session.md` is the
  bootstrap).
- Relay agent results as short summaries with file pointers.
- End the REVIEW QUEUE with one cost line from `python3 scripts/agent/session-cost.py` (never
  hand-computed), compared against `costBudgetUsd` in `pipeline.config.json`.
- Shipping reviewed PRs is `scripts/agent/ship.sh <pr#>...`. Releases go through the `release` agent.
