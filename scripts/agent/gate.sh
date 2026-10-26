#!/usr/bin/env bash
# The config-driven gate. It looks at what changed, picks the platform areas
# that apply, and runs only their checks. Commands live in pipeline.config.json.
#
#   gate.sh [--commit] [--dry-run]   run the fast gates (and the commit gates with --commit)
#   gate.sh --hook-stop              Stop/SubagentStop hook: refuse to stop on a red tree
#   gate.sh --hook-commit            PreToolUse(Bash) hook: block git commit/push on a red gate
#
# Areas: apple/** -> apple, windows/** -> windows (only if windows/ exists),
# shared/** and docs/spec/** -> every platform that exists.
#
# The hooks are cheap on purpose: --hook-stop does nothing unless uncommitted
# code changed (docs-only and test-only changes are ignored), and a green result
# is remembered per diff so the same tree is never gated twice.
#
# Escape hatch for --hook-stop only: FASTGATE_SKIP=1. The commit gate has no
# bypass on purpose.
set -uo pipefail

self_root="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=scripts/agent/json.sh
. "$self_root/scripts/agent/json.sh"

mode=run
dry=0
with_commit=0
for a in "$@"; do
  case "$a" in
    --dry-run) dry=1 ;;
    --commit) with_commit=1 ;;
    --hook-stop | --hook-commit) mode="$a" ;;
    *)
      echo "usage: gate.sh [--commit] [--dry-run] | --hook-stop | --hook-commit" >&2
      exit 2
      ;;
  esac
done

root="$self_root"
payload=""
if [ "$mode" != "run" ] && [ "$dry" = 0 ]; then
  payload="$(cat)"
  # Subagents work in per-task worktrees, so the payload cwd beats this script's location.
  cwd="$(printf '%s' "$payload" | json_get cwd)"
  if [ -n "$cwd" ]; then
    root="$(git -C "$cwd" rev-parse --show-toplevel 2>/dev/null || echo "$self_root")"
  fi
fi

cfg="$root/pipeline.config.json"
[ -f "$cfg" ] || exit 0
if ! command -v jq >/dev/null 2>&1; then
  echo "gate.sh: jq is required to read pipeline.config.json" >&2
  [ "$mode" = run ] && exit 2
  exit 0
fi

g() { git -C "$root" "$@"; }

base_sha() {
  local r
  for r in main origin/main; do
    if g rev-parse --verify -q "$r" >/dev/null 2>&1; then
      g merge-base HEAD "$r" 2>/dev/null && return
    fi
  done
  g rev-parse HEAD 2>/dev/null || true
}
base="$(base_sha)"

# scope "all": branch work plus uncommitted. scope "uncommitted": working tree only.
changed_files() {
  {
    if [ "$1" = all ] && [ -n "$base" ]; then
      g diff --name-only "$base" 2>/dev/null
    else
      g diff --name-only HEAD 2>/dev/null
    fi
    g ls-files --others --exclude-standard 2>/dev/null
  } | sort -u
}

# Docs-only and test-only edits do not trigger the stop gate: the tests-first red
# state is the test-author's expected intermediate output.
code_only() {
  grep -Ev '(\.md$|(^|/)(Tests|tests|e2e)/|Tests?\.swift$|\.(test|spec)\.[jt]sx?$|_test\.rs$)' || true
}

all_files=""
files=""
areas=""
skipped_note=""
add_area() { case " $areas " in *" $1 "*) ;; *) areas="$areas $1" ;; esac; }

pick_areas() {
  local f a p sh req kept=""
  areas=""
  skipped_note=""
  while IFS= read -r f; do
    [ -z "$f" ] && continue
    for a in $(jq -r '.gates | keys[]' "$cfg"); do
      while IFS= read -r p; do
        case "$f" in "$p"*) add_area "$a" ;; esac
      done < <(jq -r --arg a "$a" '.gates[$a].paths[]?' "$cfg")
    done
    while IFS= read -r p; do
      case "$f" in
        "$p"*) for sh in $(jq -r '.shared.runs[]?' "$cfg"); do add_area "$sh"; done ;;
      esac
    done < <(jq -r '.shared.paths[]?' "$cfg")
  done <<<"$files"
  # Drop areas whose directory is not there yet (windows/ before the port lands).
  for a in $areas; do
    req="$(jq -r --arg a "$a" '.gates[$a].requiresDir // empty' "$cfg")"
    if [ -n "$req" ] && [ ! -d "$root/$req" ]; then
      skipped_note="$skipped_note $a (no $req/ directory)"
    else
      kept="$kept $a"
    fi
  done
  areas="$kept"
}

touched() { # $1: comma list of path prefixes; true when a changed file matches
  local p f
  for p in ${1//,/ }; do
    while IFS= read -r f; do
      case "$f" in "$p"*) return 0 ;; esac
    done <<<"$all_files"
  done
  return 1
}

find_cargo() {
  local f
  while IFS= read -r f; do
    if grep -q '^\[workspace\]' "$f"; then
      dirname "$f"
      return
    fi
  done < <(find "$root/windows" -maxdepth 4 -name Cargo.toml -not -path '*/node_modules/*' -not -path '*/target/*' 2>/dev/null | sort)
}

find_frontend() {
  local f first=""
  while IFS= read -r f; do
    [ -z "$first" ] && first="$f"
    if grep -q '@tauri-apps' "$f"; then
      dirname "$f"
      return
    fi
  done < <(find "$root/windows" -maxdepth 4 -name package.json -not -path '*/node_modules/*' -not -path '*/target/*' 2>/dev/null | sort)
  [ -n "$first" ] && dirname "$first"
}

US=$'\x1f'
# One line per step: name US run US script US cwd US onlyIfTouched
steps_for() { # $1 area, $2 stage
  jq -r --arg a "$1" --arg s "$2" '
    .gates[$a][$s][]? |
    [.name, (.run // ""), (.script // ""), (.cwd // ""), ((.onlyIfTouched // []) | join(","))] | join("\u001f")' "$cfg"
}

# Resolve a step into step_cwd, step_cmd and step_skip (a reason; empty when it runs).
step_cwd=""
step_cmd=""
step_skip=""
resolve_step() { # $1 run, $2 script, $3 cwd, $4 onlyIfTouched
  local pm
  step_cwd="$root"
  step_cmd="$1"
  step_skip=""
  if [ -n "$4" ] && ! touched "$4"; then
    step_skip="not touched: $4"
    return
  fi
  case "$3" in
    "") ;;
    @cargo)
      step_cwd="$(find_cargo)"
      [ -z "$step_cwd" ] && { step_skip="no Cargo workspace under windows/ yet"; return; }
      ;;
    @frontend)
      step_cwd="$(find_frontend)"
      [ -z "$step_cwd" ] && { step_skip="no package.json under windows/ yet"; return; }
      ;;
    *) step_cwd="$root/$3" ;;
  esac
  if [ -n "$2" ]; then
    if ! jq -e --arg s "$2" '.scripts[$s]' "$step_cwd/package.json" >/dev/null 2>&1; then
      step_skip="package.json has no \"$2\" script"
      return
    fi
    pm=npm
    [ -f "$step_cwd/pnpm-lock.yaml" ] && pm=pnpm
    step_cmd="CI=1 $pm run --silent $2"
    if [ ! -d "$step_cwd/node_modules" ]; then
      step_cmd="echo 'node_modules missing in $step_cwd: run $pm install first' >&2; false"
    fi
  fi
}

stamp_file() { # $1 stage
  local p
  p="$(g rev-parse --git-path "shepit-gate-$1" 2>/dev/null)" || return 1
  case "$p" in /*) printf '%s' "$p" ;; *) printf '%s/%s' "$root" "$p" ;; esac
}

stamp_value() {
  {
    echo "$areas"
    [ -n "$base" ] && g diff "$base" 2>/dev/null
    g ls-files --others --exclude-standard -z 2>/dev/null | (cd "$root" && xargs -0 shasum 2>/dev/null)
  } | shasum | cut -d' ' -f1
}

# Plan or run one stage for every picked area. Stops at the first failure and
# leaves its name and output in fail_name and fail_out for the hooks.
fail_name=""
fail_out=""
do_stage() { # $1 stage, $2 run|plan
  local a name run script cwd only out rel
  for a in $areas; do
    while IFS="$US" read -r name run script cwd only; do
      [ -z "$name" ] && continue
      resolve_step "$run" "$script" "$cwd" "$only"
      if [ -n "$step_skip" ]; then
        echo "$a/$name: SKIP ($step_skip)"
        continue
      fi
      if [ "$2" = plan ]; then
        rel="${step_cwd#"$root"}"
        echo "  would run  $a/$name  [cd .${rel}]  $step_cmd"
        continue
      fi
      if out=$(cd "$step_cwd" && eval "$step_cmd" 2>&1); then
        echo "$a/$name: PASS"
      else
        echo "$a/$name: FAIL"
        fail_name="$a/$name"
        fail_out="$out"
        return 1
      fi
    done < <(steps_for "$a" "$1")
  done
  return 0
}

block() {
  {
    echo "GATE RED - '$fail_name' failed; fix it before proceeding."
    echo
    printf '%s\n' "$fail_out" | tail -60
  } >&2
  exit 2
}

# Run one stage through the per-diff memory. Used by the hooks.
stage_cached() { # $1 stage
  local sf sv
  sf="$(stamp_file "$1")" || sf=""
  sv="$(stamp_value)"
  if [ -n "$sf" ] && [ -f "$sf" ] && [ "$(cat "$sf")" = "$sv" ]; then
    return 0
  fi
  do_stage "$1" run >/dev/null || return 1
  [ -n "$sf" ] && printf '%s' "$sv" >"$sf"
  return 0
}

case "$mode" in
  run)
    all_files="$(changed_files all)"
    files="$all_files"
    pick_areas
    if [ "$dry" = 1 ]; then
      echo "base:    ${base:-none}"
      echo "changed: $(printf '%s\n' "$files" | grep -c .) files"
      echo "areas:  ${areas:- none}"
      [ -n "$skipped_note" ] && echo "skipped:${skipped_note}"
      if [ -z "$areas" ]; then
        echo "nothing to run"
        exit 0
      fi
      echo "fast gate:"
      do_stage fast plan
      echo "commit gate (--commit, and the git commit/push hook):"
      do_stage commit plan
      exit 0
    fi
    if [ -z "$areas" ]; then
      echo "GATE_EXIT=0 (no code areas touched${skipped_note:+; skipped:$skipped_note})"
      exit 0
    fi
    echo "areas:$areas"
    rc=0
    if do_stage fast run; then
      sf="$(stamp_file fast)" && stamp_value >"$sf"
      if [ "$with_commit" = 1 ]; then
        do_stage commit run || rc=1
      fi
    else
      rc=1
    fi
    [ "$rc" = 1 ] && printf '%s\n' "$fail_out" | tail -40
    echo "GATE_EXIT=$rc"
    exit $rc
    ;;

  --hook-commit)
    command="$(printf '%s' "$payload" | json_get tool_input.command)"
    case "$command" in
      *"git commit"* | *"git push"*) ;;
      *) exit 0 ;;
    esac
    all_files="$(changed_files all)"
    files="$all_files"
    pick_areas
    [ -z "$areas" ] && exit 0
    stage_cached fast || block
    stage_cached commit || block
    exit 0
    ;;

  --hook-stop)
    [ "${FASTGATE_SKIP:-}" = "1" ] && exit 0
    g rev-parse --verify HEAD >/dev/null 2>&1 || exit 0
    all_files="$(changed_files all)"
    files="$(changed_files uncommitted | code_only)"
    [ -z "$files" ] && exit 0
    pick_areas
    [ -z "$areas" ] && exit 0
    stage_cached fast || block
    exit 0
    ;;
esac
