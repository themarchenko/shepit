#!/usr/bin/env bash
# Renders every widget style x state in headless Chrome against the Vite dev server (dev mock, see
# src/mock.ts) and writes PNGs to docs/_working/widget-shots. Never captures the screen.
# Headless Chrome on the Mac has no Segoe UI, so text metrics differ from Windows.
set -euo pipefail

APP="$(cd "$(dirname "$0")/.." && pwd)"
FIXTURE="$APP/../core/tests/fixtures/widget-layout-windows.json"
OUT="${WIDGET_SHOTS_OUT:-/Volumes/LEO_SSD/work/pet-projects/whispr/docs/_working/widget-shots}"
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
  # pnpm starts vite as a child; stop the whole group of children, then the wrapper.
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

# One line per case: name width height (name is "<style>/<state>" in the fixture).
CASES="$(python3 -I - "$FIXTURE" <<'PY'
import json, sys
for c in json.load(open(sys.argv[1]))["cases"]:
    w = c["expected"]["window"]
    print(c["name"], w["width"], w["height"])
PY
)"

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

shot() { # name query width height scale
  local file="$OUT/$1@$5x.png" url="http://localhost:$PORT/?$2"
  chrome /dev/null --user-data-dir="$WORK/shot" --hide-scrollbars --force-device-scale-factor="$5" \
    --window-size="$3,$4" --default-background-color=00000000 --virtual-time-budget=2500 \
    --screenshot="$file" "$url"
  [ -s "$file" ] || { echo "missing $file" >&2; exit 1; }
  # The mock publishes a layout verdict (shape size, centring, inside the window) on <html>.
  local dom="$WORK/dom.html" verdict
  chrome "$dom" --user-data-dir="$WORK/dom" --force-device-scale-factor="$5" \
    --window-size="$3,$4" --virtual-time-budget=2500 --dump-dom "$url" || true
  verdict="$(grep -o 'data-shape-check="[^"]*"' "$dom" | head -1 | sed 's/^data-shape-check="//; s/"$//' || true)"
  verdict="${verdict:-FAIL no verdict}"
  echo "$verdict  $(basename "$file")"
  case "$verdict" in PASS*) ;; *) FAILS=$((FAILS + 1)) ;; esac
}

while read -r name w h; do
  key="${name/\//-}"
  for scale in 1 1.5; do
    shot "$key" "mock=$key&bg=dark" "$w" "$h" "$scale"
  done
done <<<"$CASES"

# Variants: long text, light backdrop with a lower opacity. Live text on a plain recording maps to the
# fixture's own listening-live case.
for style in flowPill captionBar island; do
  read -r w h < <(awk -v n="$style/listening-live" '$1==n {print $2, $3}' <<<"$CASES")
  for scale in 1 1.5; do
    shot "$style-listening-live-long" "mock=$style-listening-live&long=1&bg=dark" "$w" "$h" "$scale"
    shot "$style-listening-live-light" "mock=$style-listening-live&bg=light&opacity=0.5" "$w" "$h" "$scale"
  done
  read -r w h < <(awk -v n="$style/done" '$1==n {print $2, $3}' <<<"$CASES")
  for scale in 1 1.5; do
    shot "$style-done-long" "mock=$style-done&long=1&bg=dark" "$w" "$h" "$scale"
  done
done
# The formatting stage: the processing case with the second step active.
for style in flowPill captionBar island; do
  read -r w h < <(awk -v n="$style/processing" '$1==n {print $2, $3}' <<<"$CASES")
  for scale in 1 1.5; do
    shot "$style-processing-formatting" "mock=$style-processing-formatting&bg=dark" "$w" "$h" "$scale"
  done
done
echo "wrote $(ls "$OUT"/*.png | wc -l | tr -d ' ') PNGs to $OUT, $FAILS failed the layout check"
[ "$FAILS" -eq 0 ]
