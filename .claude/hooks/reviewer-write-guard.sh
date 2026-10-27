#!/usr/bin/env bash
# PreToolUse(Write|Edit) guard for the reviewers, wired in their agent frontmatter.
#
# Reviewers name problems and never patch them. They get the Write tool only to save
# their punch list: writing it through a Bash heredoc failed whenever the report
# quoted a command bash-guard watches. This hook keeps that one tool inside
# docs/_working/ of the repo or one of its sibling worktrees. Exit 2 blocks the write.
#
# It fails closed: no path, a relative path or a '..' segment is refused.

set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/agent/json.sh
. "$here/../../scripts/agent/json.sh"

input="$(cat)"
path="$(printf '%s' "$input" | json_get tool_input.file_path)"
[ -z "$path" ] && path="$(printf '%s' "$input" | json_get tool_input.notebook_path)"

root="$(cd "$here/../.." && pwd)"
worktrees="$(dirname "$root")/$(basename "$root")-worktrees"

deny() {
  echo "reviewer-write-guard: $1" >&2
  echo "   Path: ${path:-<none>}" >&2
  echo "   Reviewers write only their report, under docs/_working/. Name the fix in the report instead." >&2
  exit 2
}

[ -z "$path" ] && deny "no file path in the call."
case "$path" in
  /*) ;;
  *) deny "the path is not absolute." ;;
esac
case "/$path/" in
  */../*) deny "the path has a '..' segment." ;;
esac

case "$path" in
  "$root"/docs/_working/*) exit 0 ;;
  "$worktrees"/*/docs/_working/*) exit 0 ;;
esac

deny "the path is outside docs/_working/."
