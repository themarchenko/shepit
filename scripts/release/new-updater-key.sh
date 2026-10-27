#!/usr/bin/env bash
# Creates the minisign key pair that the Windows updater checks every update against, keeps the
# private half in ~/.shepit-signing and stores it as repo secrets for release.yml. Run once, by a
# person. It refuses to replace an existing key: installed copies would stop updating.
set -euo pipefail
umask 077

repo="themarchenko/shepit"
dir="$HOME/.shepit-signing"
key="$dir/tauri-updater.key"
pass_file="$dir/tauri-updater.password"
root="$(cd "$(dirname "$0")/../.." && pwd)"

for f in "$key" "$key.pub" "$pass_file"; do
  if [ -e "$f" ]; then
    echo "error: $f exists; the updater key is never replaced" >&2
    exit 1
  fi
done
existing="$(gh secret list -R "$repo" --json name -q '.[].name')"
for name in TAURI_SIGNING_PRIVATE_KEY TAURI_SIGNING_PRIVATE_KEY_PASSWORD; do
  if grep -qx "$name" <<<"$existing"; then
    echo "error: secret $name already exists; the updater key is never replaced" >&2
    exit 1
  fi
done

# The pinned pnpm runs through corepack, which needs the nvm Node (see CLAUDE.md, Windows).
node_bin="$(find "$HOME/.nvm/versions/node" -maxdepth 2 -name bin -type d 2>/dev/null | sort -V | tail -1)"
if [ -n "$node_bin" ]; then PATH="$node_bin:$PATH"; fi
export COREPACK_ENABLE_DOWNLOAD_PROMPT=0

mkdir -p "$dir"
chmod 700 "$dir"
openssl rand -base64 32 | tr -d '\n' >"$pass_file"
cd "$root/windows/app"
corepack pnpm install --frozen-lockfile >/dev/null
# The CLI prints both keys to stdout, so stdout is dropped. It takes the password only as an
# argument, visible to `ps` for the second the command runs. `pnpm exec`, not `pnpm tauri`: a
# package script echoes its whole command line, password included.
corepack pnpm exec tauri signer generate --ci -w "$key" -p "$(cat "$pass_file")" >/dev/null
if [ ! -s "$key" ] || [ ! -s "$key.pub" ]; then
  echo "error: the key was not written" >&2
  exit 1
fi

gh secret set TAURI_SIGNING_PRIVATE_KEY -R "$repo" <"$key"
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD -R "$repo" <"$pass_file"

echo "Public key, for tauri.conf.json and the release check:"
cat "$key.pub"
echo
