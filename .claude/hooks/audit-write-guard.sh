#!/usr/bin/env bash
# PreToolUse(Write|Edit) guard: keep audit, analysis and scratch files under
# docs/_working/ (gitignored) instead of leaking them into docs/ or the repo root.
#
# It only fires on file names that look like a working artifact (audit, analysis,
# investigation, findings, scratch, notes, run-report, learning, task-queue).
# Code and real docs are untouched. Exit 2 blocks the write and says where the
# file belongs.
#
# Escape hatch: AUDIT_GUARD_SKIP=1.

set -euo pipefail

[ "${AUDIT_GUARD_SKIP:-}" = "1" ] && exit 0

here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/agent/json.sh
. "$here/../../scripts/agent/json.sh"

path="$(cat | json_get tool_input.file_path)"
[ -z "$path" ] && exit 0

base="$(basename "$path")"

# The scratch dir itself and .claude/ config (agents/auditor.md and friends) are fine.
case "$path" in
  */docs/_working/* | docs/_working/*) exit 0 ;;
  */.claude/* | .claude/*) exit 0 ;;
esac

# Match whole words so 'auditor' and 'analyser' are not caught by 'audit' and 'analysis'.
if printf '%s' "$base" | grep -Eqi '(^|[^a-z])(audit|analysis|investigat[a-z]*|findings|scratch|notes|run-report|learning|task-queue)([^a-z]|$)'; then
  echo "audit-write-guard: '$base' looks like an audit or analysis file." >&2
  echo "   Write working analysis under docs/_working/ (gitignored), not '$path'." >&2
  echo "   Only settled docs belong in docs/. Bypass with AUDIT_GUARD_SKIP=1." >&2
  exit 2
fi

exit 0
