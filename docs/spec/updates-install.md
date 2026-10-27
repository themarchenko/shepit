# Updates and install

Paths are relative to `apple/ShepitCore/Sources/ShepitCore/Update` unless they start with `install.sh`. This area is mostly platform-specific; the Windows app uses its own updater and installer (README, Platform-specific). What a port keeps is the version rule and the schedule.

## Version order

Fixture: `shared/fixtures/app-version.json`. Source: ReleaseFeed.swift:9-23.

`isNewer(candidate, installed)` must: remove leading and trailing "v" and "V" characters, split on ".", read each piece as an integer (a piece that is not an integer counts as 0), pad the shorter list with 0, and compare piece by piece from the left. The first difference decides; all equal returns false. "0.3" and "0.3.0" are equal.

## Check schedule

Source: UpdateChecker.swift:22-65.

| Rule | Value |
|---|---|
| First look after launch | 20 s |
| Look again every | 1 hour (so a machine that sleeps overnight still checks) |
| A look asks GitHub only if the last check is at least | 24 hours old, and `checkForUpdates` is on |
| Nothing is checked while a check, download or install runs | |

- A successful check stores the time (`lastUpdateCheck`), also when no newer version exists (:58-62).
- A newer release is shown to the user if they asked for the check, or if the version is not `skippedUpdate` (:65). Skip stores the version (:73-75).
- A failed background check is silent and retried at the next hourly look; a failed check the user asked for shows the error (:66-70).

## Release feed (macOS)

Source: ReleaseFeed.swift:40-86. Swift-only, no fixture.

- Latest release: GitHub API `releases/latest` of `themarchenko/shepit`. Drafts and pre-releases do not appear there. Timeout 20 s. Accept `application/vnd.github+json`; User-Agent `Shepit/<version>`.
- Status other than 200 is an error. 403 and 429 read as rate limiting.
- Version = tag with leading "v" removed. The release must have an asset named `Shepit.dmg`, else the release is rejected. The asset's `browser_download_url` must use https, else the release is rejected (`insecureDiskImage`; defence in depth, the signature check stays the boundary).

## Installing an update (macOS)

Source: UpdateInstaller.swift:44-106. Swift-only.

- Refuse to run from App Translocation (:44).
- Download the disk image (the size from the release only drives the progress bar, nothing checks it), mount read-only, copy the app, detach, remove the quarantine flag (:55-63).
- Accept only if the bundle id matches, the version equals the release's and is newer than the installed one, and the new app satisfies the installed app's designated code requirement (:85-100).
- Swap in place, or ask for an administrator password when the folder is not writable; then relaunch (:66-80, 107-137). The password prompt is the operating system's (`osascript` with administrator privileges); the app never sees, reads or stores the password.

## Trust boundary

- No downloaded build is installed or launched unless it passes the signature check. There is no fallback and no override: any failure throws and the old app stays (UpdateInstaller.swift:65, 98, 102-115).
- The requirement is the designated requirement of the running app (`target` defaults to `Bundle.main.bundleURL`, :43; :103-107). Every release therefore must be signed with the same release certificate as the one installed, or the update is refused.
- The check is strict and covers nested code and all architectures: `kSecCSCheckAllArchitectures | kSecCSStrictValidate | kSecCSCheckNestedCode` (:110-112).
- The download is HTTPS from GitHub (the asset URL from the release feed) into a fresh temp directory named with a UUID, removed afterwards (:47-52); the asset URL is the release's `browser_download_url` on GitHub, and the feed parser rejects a non-https scheme.
- The quarantine flag is removed on the staged copy only (:63). The signature and identity check follows (:65) and nothing from the download is executed in between: it is only mounted read-only and copied.
- There is no size or checksum check. `expectedSize` only drives progress (:52).
- Platform-specific, Windows: the updater verifies the update's minisign signature against a key pinned in the installed app before it runs the installer, and refuses with no fallback when the check fails (see Trust boundary, Windows).

## install.sh (macOS)

Source: install.sh:12-80.

- Needs macOS 26 or later (:19-22). Downloads `Shepit.dmg` from the latest release; `SHEPIT_URL` replaces the URL, `SHEPIT_DIR` replaces `/Applications` (:15-16).
- Verifies the code signature, quits only the copy being replaced, installs (with sudo if needed), strips the quarantine flag, opens the app (:32-57, 62-68).

## Release feed (Windows)

Sources: `.github/workflows/release.yml`, `windows/release-check`, `windows/app/src-tauri/tauri.conf.json`. Rust tests, no shared fixture.

- One version per tag for both platforms. `version` in `windows/app/src-tauri/Cargo.toml` must equal `CFBundleShortVersionString` in `apple/macOS/project.yml`; `ci.yml` fails a pull request where they differ, and the release workflow fails a tag that does not match both.
- A release carries four assets: `Shepit.dmg` (macOS, unchanged), `Shepit-setup.exe`, `Shepit-setup.exe.sha256` (`<hex>  Shepit-setup.exe`) and `latest.json`. macOS code ignores the three Windows assets.
- The app reads `https://github.com/themarchenko/shepit/releases/latest/download/latest.json`. Only https endpoints are accepted. Drafts and pre-releases never serve it.
- `latest.json` has `version` (the tag without "v"), `notes`, `pub_date` and exactly one platform, `windows-x86_64`, with `signature` and `url`. The url is versioned (`releases/download/<tag>/Shepit-setup.exe`), so the feed and the installer always match.
- The macOS job creates the release; a separate job adds the Windows files afterwards, `latest.json` last. For about a minute after the macOS publish, or until a re-run if the Windows build failed, the latest release has no Windows feed. The background check fails silently and retries at the next hourly look (Check schedule above).
- A published Windows build is never replaced: a re-run that finds `latest.json` on the release changes nothing.
- A newer version is decided by the version order above (`isNewer`, fixture `app-version.json`), not by the updater plugin's own comparison.

## Installing an update (Windows)

Sources: `windows/app/src-tauri/tauri.conf.json`, `tauri-plugin-updater` 2.14.0. NOT WIRED YET: the updater plugin, the tray items and the update card arrive in the next change (task 11, C3 and C5). Until then the Windows app does not check for updates and users reinstall with `install.ps1`. What follows is the target behaviour; it needs device checks.

- The app checks on the schedule above, offers the update in a card, supports skip, and has Check for Updates and the daily-check switch in the tray menu.
- Installing downloads `Shepit-setup.exe`, verifies the signature, runs the installer in passive mode (progress window, no questions, per-user, no administrator rights) and relaunches the app.
- Models under the install folder (`%LOCALAPPDATA%\Shepit\models`) must survive an update.
- Windows does not compare the version inside the installer with the feed. The release workflow guarantees they match: `release-check verify` fails the build otherwise.

## Trust boundary (Windows)

Sources: `windows/release-check/src/lib.rs`, `tauri.conf.json`, `tauri.runtime.conf.json`. Not enforced by the app until the updater is wired (see above); the release checks already run.

- Every update must carry a minisign signature that verifies against the public key in `plugins.updater.pubkey` of the installed app. There is no fallback and no override; the plugin refuses before it runs anything, and no `dangerous*` key is set.
- `requireSignedVersion` is on: the signed comment must carry a version equal to the version the feed announces. A tampered feed cannot offer an older signed installer as an update.
- The same key is pinned a second time as `PINNED_UPDATER_PUBKEY` in `windows/release-check`. A unit test requires the two to be equal. The release workflow verifies the built installer against the constant, before anything is uploaded.
- The private key and its password live only in the repository secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` and in a local folder of the maintainer. The key is never replaced: installed copies would stop updating, as with the macOS certificate.
- The installer itself is not code-signed (no Authenticode certificate). The signature check above is the update boundary; the first install is covered below.
- Signing happens only in the release workflow, in its own step after the build: `tauri signer sign --app-version <version>` on the staged installer, with the key and password in the environment of that step only. The build itself (the same config as pull requests, `tauri.runtime.conf.json`) never sees the key and creates no updater artifacts. Pull request builds have no secrets and no signature.

## install.ps1 (Windows)

Source: `install.ps1`, run by `windows.yml` against the CI build. Needs device checks.

- Needs 64-bit Windows 10 or later. Sets TLS 1.2. `SHEPIT_URL` replaces the installer URL and may be a local path.
- The checksum is always read from `<installer url>.sha256` and always compared; a mismatch stops with "damaged, try again". There is no way to skip it.
- Runs `Unblock-File` on the download, stops only `shepit-app` processes whose path is under `%LOCALAPPDATA%\Shepit`, runs the installer with `/S` and waits, fails on a nonzero exit code, then starts the app.
- Installs for the current user only. No administrator rights, no UAC prompt.
- The installer is not code-signed, so a browser download shows SmartScreen ("Windows protected your PC"; More info, Run anyway). A file downloaded by PowerShell carries no Mark of the Web, so install.ps1 does not trigger it. The README says so. Not yet verified on a device.
