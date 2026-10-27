#!/usr/bin/env bash
# PreToolUse safety guard for the autonomous pipeline.
#
# A concurrent agent shares the main working tree and the maintainer's Mac. This
# hook is the hard stop against the few actions that can corrupt shared history,
# release tags or the signing setup. It reads the tool payload as JSON on stdin
# and exits 2 (which blocks the call and tells the agent why) on a denied pattern.
#
# Denied:
#   - `git add .` / `-A` / `--all` (stage exact paths only)
#   - commit or push while on main/master (a push of one vX.Y.Z tag is allowed); push to a
#     main/master ref
#   - force push
#   - deleting or moving tags: `git tag -d/-f`, `git push --delete` of a v* tag or refs/tags
#   - `rm -rf` outside the project and temp dirs, --dangerously-skip-permissions
#   - `gh secret set|delete|remove`: release secrets are set once, by a person or a script
#     the user approved
#   - `security list-keychains -s`, `delete-keychain`, `add-trusted-cert`, unless the
#     command runs a script under scripts/agent/ or .github/ (an earlier mistake
#     rewrote the maintainer's keychain search list)
#   - any command, Read, Grep or Glob that touches ~/.shepit-signing
#
# It is wired to Bash and to Read|Grep|Glob (only the signing-path check applies there).
# A shell glob can still dodge a text match; this is a tripwire for honest mistakes,
# not a sandbox.
#
# Escape hatch: BASH_GUARD_SKIP=1 (emergencies only).

set -euo pipefail

[ "${BASH_GUARD_SKIP:-}" = "1" ] && exit 0

here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/agent/json.sh
. "$here/../../scripts/agent/json.sh"

input="$(cat)"

command="$(printf '%s' "$input" | json_get tool_input.command)"
target_path="$(printf '%s' "$input" | json_get tool_input.file_path)"
[ -z "$target_path" ] && target_path="$(printf '%s' "$input" | json_get tool_input.path)"

deny() {
  echo "bash-guard: $1" >&2
  echo "   Command: ${command:-$target_path}" >&2
  echo "   Blocked by the pipeline safety guard. Bypass with BASH_GUARD_SKIP=1 (emergencies only)." >&2
  exit 2
}

# --- signing material: never read or print it -------------------------------
if printf '%s\n%s' "$command" "$target_path" | grep -Fq '.shepit-signing'; then
  deny "the .shepit-signing folder holds the release certificate; agents never read, print or copy it."
fi

[ -z "$command" ] && exit 0

has() { printf '%s' "$command" | grep -Eq -- "$1"; }

# `git` then any flags/args, then the subcommand, so `git -C <path> commit` matches too.
git_subcmd() { has "git([[:space:]]+[^[:space:]]+)*[[:space:]]+$1([[:space:]]|\$)"; }

# --- stage-everything: the concurrent-agent footgun -------------------------
if git_subcmd add; then
  if has '(^|[[:space:]])(\.|-A|--all)([[:space:]]|$)'; then
    deny "'git add .' / '-A' / '--all' is forbidden; stage exact paths only."
  fi
fi

# --- release tags are permanent ---------------------------------------------
# Users' updaters and the release workflow key off vX.Y.Z tags.
if git_subcmd tag; then
  if has '(^|[[:space:]])(-[A-Za-z]*[df][A-Za-z]*|--delete|--force)([[:space:]]|$)'; then
    deny "deleting, moving or force-writing a tag is forbidden; release tags are permanent."
  fi
fi
if git_subcmd push; then
  if has '(--delete|(^|[[:space:]])-d([[:space:]]|$))' && has '(refs/tags/|[[:space:]:]v[0-9]|--tags)'; then
    deny "deleting a release tag on the remote is forbidden."
  fi
  if has '[[:space:]]:(refs/tags/)?v[0-9]'; then
    deny "pushing an empty ref over a release tag deletes it; forbidden."
  fi
fi

# --- keychain: one bad search-list edit broke the maintainer's login keychain --
if has 'security[[:space:]]+(list-keychains.*[[:space:]]-s|delete-keychain|add-trusted-cert)'; then
  if ! has '^[[:space:]]*((bash|sh|zsh)[[:space:]]+)?(\./)?(scripts/agent|\.github)/'; then
    deny "keychain list/delete/trust changes are only allowed inside scripts/agent/ or .github/ scripts."
  fi
fi

# --- destructive filesystem -------------------------------------------------
# Recursive-force deletes are routine here (stale SwiftPM .build folders, scratch dirs), so they are
# allowed for paths inside the project or a temp dir and denied for anything that could reach
# home, the root, the parent folder, .git, or an empty variable.
rm_target_ok() {
  local t="${1//\"/}"
  t="${t//\'/}"
  # Literal text from the command line, not expansions.
  # shellcheck disable=SC2088,SC2016
  case "$t" in
    "" | "/" | "~" | "~/" | "." | "./" | ".." | "../" | "*" | ".git" | ".git/" | '$HOME' | '$HOME/') return 1 ;;
    ../* | */.. | */../* | *.git | *.git/*) return 1 ;;
  esac
  # A variable with nothing after it may be empty, and then "$x/" means /.
  if [[ "$t" == \$* ]]; then
    [[ "$t" =~ ^\$\{?[A-Za-z_][A-Za-z0-9_]*\}?/[^*/] ]]
    return
  fi
  case "$t" in
    /tmp/?* | /private/tmp/?* | /private/var/folders/?* | /var/folders/?*) return 0 ;;
    "${CLAUDE_PROJECT_DIR:-/nonexistent}"/?*) return 0 ;;
    /* | ~*) return 1 ;;
  esac
  return 0
}
if has 'rm[[:space:]]+(-[A-Za-z]*[rR][A-Za-z]*[fF]|-[A-Za-z]*[fF][A-Za-z]*[rR]|-[rR][[:space:]]+-[fF]|-[fF][[:space:]]+-[rR])'; then
  while IFS= read -r segment; do
    [[ "$segment" =~ (^|[[:space:]])rm[[:space:]] ]] || continue
    read -r -a words <<< "${segment#*rm }"
    for w in "${words[@]}"; do
      [[ "$w" == -* ]] && continue
      rm_target_ok "$w" || deny "'rm -rf $w' could delete outside the project or a temp dir."
    done
  done < <(printf '%s\n' "$command" | sed -E 's/(&&|\|\||;|\|)/\n/g')
fi

# --- repo secrets ------------------------------------------------------------
# Replacing a signing secret would publish releases that installed copies refuse. Flags may sit
# between `gh` and `secret` (`gh -R owner/repo secret set`), and `gh api` reaches the same endpoint.
if has '(^|[[:space:];&|(])gh[[:space:]]+([^;&|]*[[:space:]])?secret[[:space:]]+(set|delete|remove)([[:space:]]|$)' \
  || has '(^|[[:space:];&|(])gh[[:space:]]+api[^;&|]*actions/secrets'; then
  deny "changing repo secrets is forbidden for agents; the signing secrets are never replaced."
fi
if has 'scripts/release/new-updater-key\.sh'; then
  deny "the updater key is created once, by a person; agents never run that script."
fi

# --- permission bypass ------------------------------------------------------
if has 'dangerously-skip-permissions'; then
  deny "'--dangerously-skip-permissions' is forbidden."
fi

# --- force push, push to main -----------------------------------------------
if git_subcmd push; then
  if has '(--force([[:space:]]|=|$)|--force-with-lease|(^|[[:space:]])-f([[:space:]]|$))'; then
    deny "force push is forbidden; the pipeline only pushes fresh branches."
  fi
  if has 'push[^&|;]*(^|[[:space:]:/])(main|master)([[:space:]]|$)'; then
    deny "pushing to main/master is forbidden."
  fi
fi

# --- commit/push while sitting on the protected branch ----------------------
# The tree the command targets: `git -C <path>`, else a leading `cd <path>`, else
# the session cwd from the payload (CLAUDE_PROJECT_DIR points at the main checkout
# even inside a worktree, so it is only the last fallback).
if git_subcmd commit || git_subcmd push; then
  session_cwd="$(printf '%s' "$input" | json_get cwd)"
  target="$(git -C "${session_cwd:-.}" rev-parse --show-toplevel 2>/dev/null || true)"
  [ -z "$target" ] && target="$(git rev-parse --show-toplevel 2>/dev/null || true)"
  [ -z "$target" ] && target="${CLAUDE_PROJECT_DIR:-.}"
  if [[ "$command" =~ git[[:space:]]+-C[[:space:]]+([^[:space:]]+) ]]; then
    target="${BASH_REMATCH[1]}"
  elif [[ "$command" =~ (^|[[:space:]\&\;])cd[[:space:]]+([^[:space:]\&\;\|]+) ]]; then
    target="${BASH_REMATCH[2]}"
  fi
  branch="$(git -C "$target" rev-parse --abbrev-ref HEAD 2>/dev/null || echo '')"
  # Pushing exactly one release tag and nothing else is allowed from any branch, main included:
  # releases are tagged on main. The delete and force checks above have already run.
  tag_only_push='^[[:space:]]*git([[:space:]]+-C[[:space:]]+[A-Za-z0-9_./-]+)?[[:space:]]+push[[:space:]]+origin[[:space:]]+(refs/tags/)?v[0-9]+\.[0-9]+\.[0-9]+[[:space:]]*$'
  if [[ "$command" =~ $tag_only_push ]]; then
    exit 0
  fi
  if [ "$branch" = "main" ] || [ "$branch" = "master" ]; then
    deny "commit/push on the '$branch' branch is forbidden; work on a feature branch or worktree."
  fi
fi

exit 0
