#!/usr/bin/env bash
# One-time setup on a fresh machine: XcodeGen, project generation, first build.
set -euo pipefail
cd "$(dirname "$0")/.."
command -v brew >/dev/null || { echo "Homebrew is required"; exit 1; }
command -v xcodegen >/dev/null || brew install xcodegen
xcodebuild -version | head -1
make generate
echo "Next: make install && make run"
