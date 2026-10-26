# Lean orchestrator session - bootstrap prompt

Reference this when starting a pipeline session in the repo root. One task per session; state
lives in files, not in the conversation. Run the main session on Opus: it is the architect and
manager and owns the decisions.

## Hard layer

The permission classifier is the soft layer. These stay on regardless:

- `.claude/settings.json` hooks: the commit gate (blocks red commits and pushes), `bash-guard`
  (no commits on main, no `git add .`, no force push, no tag deletion, no keychain edits, no
  reads of `~/.shepit-signing`), `audit-write-guard`, and the stop gate.
- A blocked action means stop: report the block and the exact edit you wanted, then wait. Do not
  route around it with shell tricks or re-encoding.
- Worktrees isolate each task. Secrets live outside the repo. `main` takes changes only through
  pull requests.

## Session lifecycle

1. Read `docs/_working/task-queue.md` and the newest `docs/_working/run-report-*.md`. Pick one
   task (top of the queue unless the human named one).
2. Run it through `/feature` end to end.
3. Print the REVIEW QUEUE block, update the queue, end the session.

## Cost contract

- **Models**: Opus is the main session and the planner. Sonnet writes code and reviews
  (test-author, implementer, reviewer-domain, reviewer-security). Haiku does read-only legwork
  (auditor, validator). Do not swap a cheaper model into a role that carries a decision.
- Agents return verdicts, not essays. Auditor at most 120 lines, planner 150, reviewers 80,
  validator 40. The implementer writes its full report to `docs/_working/impl-report-<branch>.md`
  and returns at most 25 lines.
- Chunk the implementer: a plan that spans more than two layers (ShepitCore, macOS app, Windows)
  becomes sequential implementer runs with fresh contexts, one brief each.
- Reviewers do not re-run the gate. The hooks enforce it.
- Risk-tiered council: a diff that touches `sharedCore` globs in `pipeline.config.json` gets
  reviewer-domain and reviewer-security. Anything else gets reviewer-domain alone, plus security
  when secrets, permissions, processes or the network are anywhere near the diff.
- Scripts over tool loops: `scripts/agent/gate.sh` runs the gate in one call. Batch independent
  tool calls and do not poll a background task that will notify you.

## Unchanged rules

Everything in the `project-discipline` skill applies: evidence before code, restate and
out-of-scope, park real ambiguity, tests first, inversion evidence, named staging, commit per
chunk, verify per platform and list device checks, keep the shared contract true, and no
self-edited rules (propose changes in the run report as SUGGESTED RULE CHANGE).

## Task-done summary

End every task with exactly this block:

```
DONE <task> -> <PR url>
- did: <1-2 plain lines>
- gates: fast PASS/FAIL | commit PASS/FAIL | council <n> findings, all fixed
- needs device check: <list, or none>
- watch out: <0-2 bullets, or none>
cost: <one line from: python3 scripts/agent/session-cost.py>
```

The cost line comes from the script, never from an estimate. If it is above `costBudgetUsd` in
`pipeline.config.json`, say so on the next line.

Shipping reviewed PRs (merge, sync main, remove the worktree) is `scripts/agent/ship.sh <pr#>...`.
Releases go through the `release` agent.
