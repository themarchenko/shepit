#!/usr/bin/env bash
# Ship reviewed PRs in one command: mark ready, check GitHub can merge it, squash-merge,
# sync local main, and tear down the matching sibling worktree and local branch.
# Releases are a separate step: the `release` agent tags main.
#
#   scripts/agent/ship.sh 12        # merge one PR
#   scripts/agent/ship.sh 12 13     # merge several, in order
#
# Run it only for PRs you have reviewed - it does not ask twice.
set -euo pipefail

[ $# -ge 1 ] || { echo "usage: $0 <pr#> [<pr#>...]" >&2; exit 2; }

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

for pr in "$@"; do
  branch=$(gh pr view "$pr" --json headRefName --jq .headRefName)
  gh pr ready "$pr" >/dev/null 2>&1 || true

  # GitHub computes mergeability lazily: right after a push, or after the previous PR in
  # this run lands on main, UNKNOWN means "ask again", not "no".
  state=UNKNOWN
  for _ in 1 2 3 4 5 6; do
    state=$(gh pr view "$pr" --json mergeable --jq .mergeable)
    [ "$state" != UNKNOWN ] && break
    sleep 5
  done
  if [ "$state" != MERGEABLE ]; then
    echo "#$pr is $state, not merged. The worktree is kept: rebase $branch on main and run again." >&2
    exit 1
  fi

  # Merge first, tear down after, so a refused merge (a conflict that appeared since the
  # check, a failing check) leaves the worktree in place. No --delete-branch: it also
  # deletes the local branch, which fails while a worktree has it checked out. The repo
  # deletes the remote branch on merge (deleteBranchOnMerge).
  gh pr merge "$pr" --squash

  wt="$(dirname "$root")/$(basename "$root")-worktrees/$branch"
  if [ -d "$wt" ]; then
    git worktree remove "$wt" 2>/dev/null && echo "worktree removed: $branch" \
      || echo "worktree NOT removed (dirty?): $wt - clean it up by hand"
  fi
  git branch -D "$branch" 2>/dev/null || true
  echo "merged #$pr ($branch)"
done

git pull --ff-only origin main
git log --oneline -"$#"
echo "SHIP OK"
