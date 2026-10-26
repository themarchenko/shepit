#!/bin/bash
# Installs or updates Shepit from the latest GitHub release:
#
#   curl -fsSL https://raw.githubusercontent.com/themarchenko/shepit/main/install.sh | bash
#
# Shepit is not notarized, so a copy downloaded in a browser is quarantined and macOS refuses to open
# it until the user finds "Open Anyway". curl does not quarantine what it downloads, so the app this
# script installs opens straight away.
#
# SHEPIT_DIR installs somewhere other than /Applications. SHEPIT_URL takes another disk image, for
# example a local build: SHEPIT_URL=file://$PWD/.build/dist/Shepit.dmg
set -euo pipefail

main() {
  local dir="${SHEPIT_DIR:-/Applications}"
  local url="${SHEPIT_URL:-https://github.com/themarchenko/shepit/releases/latest/download/Shepit.dmg}"
  local app="$dir/Shepit.app"

  [[ "$(uname -s)" == Darwin ]] || fail "Shepit runs on macOS only."
  local os
  os="$(sw_vers -productVersion)"
  (( ${os%%.*} >= 26 )) || fail "Shepit needs macOS 26 or later. This Mac has macOS $os."

  tmp="$(mktemp -d)"
  trap cleanup EXIT

  echo "Downloading Shepit..."
  curl -fL --progress-bar -o "$tmp/Shepit.dmg" "$url" || fail "Download failed: $url"
  hdiutil attach -quiet -nobrowse -readonly -mountpoint "$tmp/volume" "$tmp/Shepit.dmg"
  ditto "$tmp/volume/Shepit.app" "$tmp/Shepit.app"
  hdiutil detach -quiet "$tmp/volume"
  codesign --verify --deep --strict "$tmp/Shepit.app" 2>/dev/null || fail "The downloaded app is damaged. Try again."

  # Quit only the copy being replaced. A build running from somewhere else keeps running.
  if pgrep -f "^$app/Contents/MacOS/Shepit" >/dev/null; then
    echo "Quitting the running Shepit..."
    pkill -f "^$app/Contents/MacOS/Shepit" || true
    for _ in 1 2 3 4 5 6 7 8 9 10; do
      pgrep -f "^$app/Contents/MacOS/Shepit" >/dev/null || break
      sleep 0.5
    done
  fi

  mkdir -p "$dir" 2>/dev/null || true
  if [[ -w "$dir" ]]; then
    install_app "$app"
  else
    echo "Writing to $dir needs your password."
    sudo -v
    install_app "$app" sudo
  fi

  local version
  version="$(defaults read "$app/Contents/Info" CFBundleShortVersionString)"
  echo "Installed Shepit $version in $dir."
  open "$app"
  echo "Shepit is in the menu bar. Allow Microphone and Accessibility when it asks."
}

# Swaps the new app in. A copy downloaded with curl carries no quarantine flag; removing it anyway
# covers SHEPIT_URL pointing at a file that came from a browser.
install_app() {
  local app="$1" sudo="${2:-}"
  $sudo mkdir -p "$(dirname "$app")"
  $sudo rm -rf "$app"
  $sudo ditto "$tmp/Shepit.app" "$app"
  $sudo xattr -dr com.apple.quarantine "$app" 2>/dev/null || true
}

cleanup() {
  hdiutil detach -quiet "$tmp/volume" 2>/dev/null || true
  rm -rf "$tmp"
}

fail() {
  echo "Error: $1" >&2
  exit 1
}

main "$@"
