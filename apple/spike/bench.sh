#!/usr/bin/env bash
# Build and run the recognizer benchmark on apple/spike/recordings. Extra args go to shepit-bench
# (for example: --engines parakeet-v3,whisper-turbo). First run downloads the models (~3 GB).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PKG="$ROOT/spike/shepit-bench"
swift build -c release --package-path "$PKG" 2>&1 | grep -E 'error|warning: unre|Compiling|Build complete' | tail -5
exec "$PKG/.build/release/shepit-bench" \
  --recordings "$ROOT/spike/recordings" \
  --phrases "$ROOT/spike/phrases" \
  --out "$ROOT/spike/results" "$@"
