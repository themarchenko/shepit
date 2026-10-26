#!/usr/bin/env bash
# Ship reviewed PRs in one command: mark ready, squash-merge (the branch is deleted
# on the remote), sync local main, and tear down the matching sibling worktree and
# local branch. Releases are a separate step: the `release` agent tags main.
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

  # Remove the worktree before the merge: `gh pr merge --delete-branch` also
  # deletes the local branch, and that fails while a worktree has it checked out.
  wt="$(dirname "$root")/$(basename "$root")-worktrees/$branch"
  if [ -d "$wt" ]; then
    git worktree remove "$wt" 2>/dev/null && echo "worktree removed: $branch" \
      || echo "worktree NOT removed (dirty?): $wt - clean it up by hand"
  fi

  gh pr merge "$pr" --squash --delete-branch
  git branch -D "$branch" 2>/dev/null || true
  echo "merged #$pr ($branch)"
done

git pull --ff-only origin main
git log --oneline -"$#"
echo "SHIP OK"
