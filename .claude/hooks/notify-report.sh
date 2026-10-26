#!/usr/bin/env bash
# Notification hook: append every agent notification (finished, needs input) to a
# dated run report, so there is a review queue to skim.
#
# The report lives in docs/_working/ (gitignored). It is an operational log, not
# a committed file. One file per day: run-report-<YYYY-MM-DD>.md.

set -uo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/agent/json.sh
. "$here/../../scripts/agent/json.sh"

input="$(cat)"
msg="$(printf '%s' "$input" | json_get message)"
[ -z "$msg" ] && msg="$(printf '%s' "$input" | json_get hook_event_name)"
[ -z "$msg" ] && msg="notification"

root="${CLAUDE_PROJECT_DIR:-.}"
dir="$root/docs/_working"
mkdir -p "$dir"

day="$(date +%F)"
file="$dir/run-report-$day.md"

if [ ! -f "$file" ]; then
  printf '# Run report - %s\n\nAppended by the Notification hook. Review queue for the day.\n\n' "$day" >"$file"
fi

printf -- '- %s - %s\n' "$(date +%H:%M:%S)" "$msg" >>"$file"

exit 0
