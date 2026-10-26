#!/usr/bin/env bash
# Sourced by the hooks and the gate. json_get reads JSON on stdin and prints the
# value at a dotted path ("tool_input.command"), or nothing when it is missing.
# jq ships with current macOS; python3 is the fallback. Node is not required.

json_get() {
  if command -v jq >/dev/null 2>&1; then
    jq -r --arg p "$1" 'try (getpath($p | split(".")) // empty | if type == "string" then . else tojson end) catch empty' 2>/dev/null
  elif command -v python3 >/dev/null 2>&1; then
    python3 -c '
import json, sys
try:
    v = json.load(sys.stdin)
    for k in sys.argv[1].split("."):
        v = v.get(k) if isinstance(v, dict) else None
    sys.stdout.write("" if v is None else (v if isinstance(v, str) else json.dumps(v)))
except Exception:
    pass
' "$1"
  else
    echo "json_get: need jq or python3" >&2
    return 1
  fi
}
