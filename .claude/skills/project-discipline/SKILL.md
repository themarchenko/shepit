---
name: project-discipline
description: The standing rules every pipeline agent follows - evidence before code, stop on ambiguity, tests first, inversion evidence, named staging, per-chunk commits, platform verification, no self-edited rules. Load this before any task work.
---

# Project discipline

These rules apply to every agent in the Shepit pipeline. If a rule conflicts with
convenience, the rule wins. When in doubt, stop and report; do not guess.

The hooks in `.claude/hooks/` and the gate are a backstop, not a substitute. If a hook
blocks you, you skipped a rule. Fix the work; do not route around the hook.

## 1. Evidence before code

Before changing anything, read the real files, symbols and call sites. Quote them as
`path:line`. Do not assume a function, a Make target, a setting key or a flag exists.
A plan or diff with no evidence trail is not acceptable.

## 2. Restate before you write

Say what the task asks (the behaviour, not the title), the concrete change and where,
and an explicit out-of-scope list. Editing outside that list is a defect: stop and report.

## 3. Stop on ambiguity

On any product or architecture ambiguity (two plausible behaviours, a security tradeoff,
a question about what the other platform should do), do not pick one. Park it in
`docs/_working/run-report-<date>.md` under "PARKED - needs decision" with the options and
your recommendation, then stop that task. A parked task is a good outcome.

## 4. Tests first

Write the test before the code, and show it red. Swift: `apple/ShepitCore/Tests`. Rust:
the crate's own tests. Behaviour that both platforms share gets a fixture under `shared/`
and a line in `docs/spec/`, so each platform's tests read the same data.

## 5. Inversion evidence

A green suite does not show the test checks anything. For the riskiest control in the diff:
record the original lines, break the control, run only the relevant test (for example
`swift test --package-path apple/ShepitCore --filter <Name>` or `cargo test <name>`), show
it red, restore the lines verbatim, show it green, and confirm `git diff` on the file is
empty after the restore. No evidence, not done.

## 6. Named staging

Never `git add .`, `-A` or `--all`. Stage exact paths. Never commit or push on `main`.

## 7. Commit per logical chunk

One coherent change per commit. Imperative subject under 72 characters, a body when the
reason is not obvious. No assistant attribution lines and no `Co-Authored-By`. For multi-line
messages use `git commit -F <file>`.

## 8. Verify per platform, and say what you could not

- macOS UI: render the view offscreen with `NSView.cacheDisplay` and look at the image. Never
  capture the screen: it may show private windows. This rule has no exceptions.
- Windows: `cargo test` locally plus the Windows CI job.
- Anything only a person on real hardware can check (microphone, global hotkey, Accessibility
  or other permissions, pasting into other apps, signing, the installer, the updater against
  a real release) goes in the run report as "needs device check". Never claim it as done.

## 9. Keep the shared contract true

A behaviour change on one platform updates `docs/spec/` and the fixtures in `shared/` in the
same change, or the run report lists it as a known gap with the platform that lags. Add a
`docs/DECISIONS.md` entry for any decision or measurement a future reader would have to
rediscover. A user-visible feature gets a `WhatsNew.releases` item for its version.

## 10. Text rules

English only in code, comments, docs, commits and UI strings. No em dashes (use "-"). Plain
words, no filler. Comments say why, not what.

## 11. No self-mutation of the rules

Never edit skills, `CLAUDE.md`, `.claude/settings.json`, hooks or agent definitions to change
the rules you work under. Write a proposed change to the run report under "SUGGESTED RULE
CHANGE" for a human. The rules change only when a human changes them.
