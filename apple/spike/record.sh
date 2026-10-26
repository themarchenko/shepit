#!/usr/bin/env bash
# Record the Phase 0 test set: one WAV per phrase file in spike/phrases.
# Usage: apple/spike/record.sh [--redo] [--mic "Device Name"]
# Press Enter to start each recording, press q to stop it (ffmpeg reads the key).
set -euo pipefail

cd "$(dirname "$0")"
REDO=0
MIC="MacBook Pro Microphone"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --redo) REDO=1 ;;
    --mic) MIC="$2"; shift ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
  shift
done

command -v ffmpeg >/dev/null || { echo "ffmpeg not found (brew install ffmpeg)" >&2; exit 1; }

# Resolve the avfoundation audio device index by name.
IDX=$(ffmpeg -hide_banner -f avfoundation -list_devices true -i "" 2>&1 \
  | sed -n '/audio devices/,$p' | grep -F "] $MIC" | sed -E 's/.*\[([0-9]+)\].*/\1/' | head -1)
if [[ -z "$IDX" ]]; then
  echo "microphone \"$MIC\" not found. Available audio devices:" >&2
  ffmpeg -hide_banner -f avfoundation -list_devices true -i "" 2>&1 | sed -n '/audio devices/,$p' >&2
  exit 1
fi
echo "Recording from [$IDX] $MIC at 16 kHz mono. Press q to stop each take."
echo

mkdir -p recordings
for f in phrases/*.txt; do
  name=$(basename "$f" .txt)
  out="recordings/$name.wav"
  if [[ -f "$out" && $REDO -eq 0 ]]; then
    echo "skip $name (exists)"; continue
  fi
  echo "================================================================"
  echo "[$name]"
  echo
  cat "$f"
  echo
  read -r -p "Enter = start recording, s = skip: " answer
  [[ "$answer" == "s" ]] && continue
  ffmpeg -hide_banner -loglevel error -f avfoundation -i ":$IDX" \
    -ac 1 -ar 16000 -sample_fmt s16 -t 120 -y "$out"
  dur=$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$out")
  echo "saved $out (${dur%.*} s)"
  echo
done
echo "Done. Run the benchmark: apple/spike/bench.sh"
