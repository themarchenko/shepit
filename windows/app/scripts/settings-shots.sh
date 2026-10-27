#!/usr/bin/env bash
# Renders every Settings, History and tour state in headless Chrome against the Vite dev server (dev mock, see
# src/shared/mockBackend.ts) and writes PNGs to docs/_working/settings-shots. Never captures the screen.
# Settings is 620x600, History 760x520, the tour 620x540 and the update card 420x320, the sizes of the real windows (the card
# window is 420 wide and sized to its content), at 1x and 1.5x.
# Headless Chrome on the Mac has no Segoe UI, so text metrics differ from Windows.
set -euo pipefail

APP="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${SETTINGS_SHOTS_OUT:-/Volumes/LEO_SSD/work/pet-projects/whispr/docs/_working/settings-shots}"
CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
PORT=1420
NODE_BIN="$HOME/.nvm/versions/node/v24.22.1/bin"
export PATH="$NODE_BIN:$PATH" COREPACK_ENABLE_DOWNLOAD_PROMPT=0

mkdir -p "$OUT"
FAILS=0
WORK="$(mktemp -d)"
cd "$APP"
corepack pnpm dev >"$WORK/vite.log" 2>&1 &
VITE=$!
cleanup() {
  # pnpm starts vite as a child; stop the children first, then the wrapper.
  pkill -P "$VITE" 2>/dev/null || true
  kill "$VITE" 2>/dev/null || true
  wait "$VITE" 2>/dev/null || true
  rm -r -- "$WORK"
}
trap cleanup EXIT

for _ in $(seq 1 60); do
  curl -fs "http://localhost:$PORT/" >/dev/null && break
  sleep 0.5
done
curl -fs "http://localhost:$PORT/" >/dev/null || { cat "$WORK/vite.log"; exit 1; }

# Chrome has no built-in timeout and macOS has no timeout(1): stop a run that hangs after 40 s.
chrome() { # output-file args...
  local out="$1"
  shift
  "$CHROME" --headless=new --disable-gpu --no-first-run "$@" >"$out" 2>/dev/null &
  local pid=$! n=0
  while kill -0 "$pid" 2>/dev/null; do
    n=$((n + 1))
    [ "$n" -gt 400 ] && { kill "$pid"; echo "chrome timed out" >&2; return 1; }
    sleep 0.1
  done
  wait "$pid" || true
}

# SHOTS_ONLY="tour-1" renders just the matching states (a regex on the mock name).
shot() { # page mock width height scale
  [[ -z "${SHOTS_ONLY:-}" || "$2" =~ $SHOTS_ONLY ]] || return 0
  local file="$OUT/$2@$5x.png" url="http://localhost:$PORT/$1.html?mock=$2&still=1"
  chrome /dev/null --user-data-dir="$WORK/shot" --hide-scrollbars --force-device-scale-factor="$5" \
    --window-size="$3,$4" --virtual-time-budget=3600 --screenshot="$file" "$url"
  [ -s "$file" ] || { echo "missing $file" >&2; exit 1; }
  # The mock publishes a layout verdict on <html>.
  local dom="$WORK/dom.html" verdict
  chrome "$dom" --user-data-dir="$WORK/dom" --force-device-scale-factor="$5" \
    --window-size="$3,$4" --virtual-time-budget=3600 --dump-dom "$url" || true
  verdict="$(grep -o 'data-layout-check="[^"]*"' "$dom" | head -1 | sed 's/^data-layout-check="//; s/"$//' || true)"
  verdict="${verdict:-FAIL no verdict}"
  echo "$verdict  $(basename "$file")"
  case "$verdict" in PASS*) ;; *) FAILS=$((FAILS + 1)) ;; esac
}

SETTINGS="settings-general settings-general-downloading settings-general-picking settings-general-rightalt
settings-general-mic-silent settings-appearance-flowPill settings-appearance-captionBar
settings-appearance-island settings-polishing settings-polishing-none settings-polishing-codex
settings-dictionary settings-dictionary-empty
settings-about settings-about-available settings-about-checking settings-about-failed"
UPDATE="update-checking update-upToDate update-available update-downloading update-downloadingNoSize
update-installing update-checkFailed update-installFailed"
HISTORY="history history-empty history-many history-error history-clear-confirm"
TOUR="tour-1 tour-2 tour-2-denied tour-3 tour-4 tour-5 tour-5-downloading"

for scale in 1 1.5; do
  for name in $SETTINGS; do shot settings "$name" 620 600 "$scale"; done
  for name in $HISTORY; do shot history "$name" 760 520 "$scale"; done
  for name in $TOUR; do shot tour "$name" 620 540 "$scale"; done
  for name in $UPDATE; do shot update "$name" 420 320 "$scale"; done
done
echo "$(ls "$OUT"/*.png | wc -l | tr -d ' ') PNGs to $OUT, $FAILS failed the layout check"
[ "$FAILS" -eq 0 ]
