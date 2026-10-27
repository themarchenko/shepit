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

Sources: `windows/app/src-tauri/src/updater.rs`, `update_card.rs`, `tray.rs`, `windows/core/src/update.rs`, `tauri-plugin-updater` 2.14.0. The behaviour is wired and tested in Rust; the real install and the card on screen need a Windows PC check (list at the end).

- Schedule and rules are the core ones (`update.rs:5-9`, "Check schedule" above). A worker thread `updates` takes the first look after `FIRST_LOOK_SECS` and then one every `LOOK_EVERY_SECS` from a ticker thread (updater.rs:337-346). A look checks only when no check, download or install runs and `is_due` holds.
- Check: the plugin reads the one endpoint from `tauri.conf.json`. The timeout is 20 s (`CHECK_TIMEOUT`, updater.rs:23, 41). The download has no timeout, so a slow connection can finish.
- Newer means `isNewer` (updater.rs:26-28), set as the plugin's default version comparator (updater.rs:31-36). The same or an older version is never offered.
- Two surfaces:
  - Settings > About, group "Updates": the switch "Check for updates once a day" (`checkForUpdates`), a status label with a detail line, and one button: "Check now", a spinner while busy, or "Install..." when a release is waiting (AboutPane.tsx:46-73). "Check now" never opens the card; only a found release does (updater.rs, `check_updates_now`). "Install..." opens the card.
  - Tray: one item after "Settings...", "Check for updates..." or "Update to X..." (tray.rs:32-36, main.rs:130). It opens the card first and then checks (main.rs:175-179). The ellipsis is U+2026 in both places.
- The card is a window with label `update`, 420 logical px wide (a 380 px card plus 20 px margins), built when needed, one at a time (update_card.rs:14, 53-85). It sits in the bottom right corner of the primary work area and keeps that corner when its height changes (`card_frame`, update_card.rs:34-45; height clamped to 60-480). It is built with `focused(false)` and `focusable(false)`, so the app being dictated into keeps the keyboard (update_card.rs:78-79); the cost is no Enter shortcut. macOS does the same with a non-activating panel.
- Card texts come from Rust (`card_for`, updater.rs:130), pinned by `windows/app/src-tauri/tests/fixtures/update-cards.json`; the window renders only what it is sent. Buttons: "Skip this version", "Later" and "Install and reopen", plus a "What changed" link that opens the release page. The available text says Shepit downloads the update, checks that it is signed like this one, runs the installer and opens again. While installing it says "Checking the signature and starting the installer. Shepit reopens on its own.", because the plugin reports the download as finished before it verifies. With no known size the bar is indeterminate and the detail reads "Downloading...". A failed install shows "The update did not install" with the error (state `InstallFailed`, update.rs:22).
- Skip stores the version in `skippedUpdate` and closes the card; the state stays Available, so a check the user starts still shows it.
- Installing: the plugin downloads `Shepit-setup.exe`, verifies the signature, and only then starts the installer (`download_and_install`, updater.rs:544). The install runs on its own thread `update-install` (updater.rs:514-516), so the card and the tray still answer, and a second click cannot start a second install. On Windows the plugin starts the installer and exits the process; the NSIS installer in passive mode (progress window, no questions, per-user, no administrator rights) reopens the app with `/R`. The app does not call a restart itself. On any other system the install fails with "Updates install only on Windows." without calling the plugin (updater.rs:529-533).
- Models under the install folder (`%LOCALAPPDATA%\Shepit\models`) must survive an update.
- Windows does not compare the version inside the installer with the feed. The release workflow guarantees they match: `release-check verify` fails the build otherwise.
- Each state change is sent to the Settings window as `update-status` and to the card as `update-card`; the stored `lastUpdateCheck` and `skippedUpdate` are written through the settings store, under its one lock (settings.md).
- Needs a Windows PC check: the card appears without taking focus from Notepad and its buttons respond; placement at 100% and 150% display scale and the kept corner on resize; the tray item changes to "Update to X..."; a real install from one release to the next, the `/R` reopen at the new version, no UAC prompt, models kept; Skip holds through a background check; the daily switch stops the hourly looks; behind a corporate proxy.

## Trust boundary (Windows)

Sources: `windows/release-check/src/lib.rs`, `tauri.conf.json`, `tauri.runtime.conf.json`. The app enforces this through the plugin (below); the release checks run in CI.

- Every update must carry a minisign signature that verifies against the public key in `plugins.updater.pubkey` of the installed app. There is no fallback and no override; the plugin refuses before it runs anything, and no `dangerous*` key is set.
- `requireSignedVersion` is on: the signed comment must carry a version equal to the version the feed announces. A tampered feed cannot offer an older signed installer as an update.
- The same key is pinned a second time as `PINNED_UPDATER_PUBKEY` in `windows/release-check`. A unit test requires the two to be equal. The release workflow verifies the built installer against the constant, before anything is uploaded.
- The private key and its password live only in the repository secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` and in a local folder of the maintainer. The key is never replaced: installed copies would stop updating, as with the macOS certificate.
- The installer itself is not code-signed (no Authenticode certificate). The signature check above is the update boundary; the first install is covered below.
- Signing happens only in the release workflow, in its own step after the build: `tauri signer sign --app-version <version>` on the staged installer, with the key and password in the environment of that step only. The build itself (the same config as pull requests, `tauri.runtime.conf.json`) never sees the key and creates no updater artifacts. Pull request builds have no secrets and no signature.
- No webview can reach the plugin. The plugin registers `check`, `download` and `install` commands for JavaScript, and its `check` accepts a proxy, a target and headers. No capability grants any `updater:`, `opener:`, `shell:`, `process:`, `fs:` or `http:` permission (`capabilities/*.json`); every plugin call is Rust in `updater.rs`. The card window gets only `core:event:default` and its three own commands (capabilities/update.json). The Windows app does not use `tauri-plugin-opener`: release page URLs are built in Rust from a constant and a version of digits and dots, else `releases/latest` (shell.rs:14).
- Nothing in code overrides the pinned trust. `plugin()` sets only the version comparator and `checker()` only the timeout (updater.rs:31-41). Tests that fail the build:
  - `tests/updater_trust.rs`: no `.pubkey(`, `.endpoints(`, `dangerous`, `allow_downgrades`, `restore_update`, `installer_arg`, `configure_client` or `.install(` in the app sources; `tauri_plugin_updater::` only in `updater.rs`; the plugin registered once, as `.plugin(updater::plugin())`; `Cargo.lock` has tauri-plugin-updater 2.14.0 only and the manifest pins it with `=`.
  - `tests/command_scoping.rs`: `no_window_can_call_a_plugin_command`; the card commands appear in no other capability.
  - Functional tests in `updater.rs` against a local server: a release signed with another key is refused (`the_apps_updater_refuses_a_release_signed_with_another_key`), the same and older versions are not offered, and a harness with the real key accepts a correct release, so the refusal is not a broken setup. Test-only code overrides the endpoint; app code does not.

## install.ps1 (Windows)

Source: `install.ps1`, run by `windows.yml` against the CI build. Needs device checks.

- Needs 64-bit Windows 10 or later. Sets TLS 1.2. `SHEPIT_URL` replaces the installer URL and may be a local path.
- The checksum is always read from `<installer url>.sha256` and always compared; a mismatch stops with "damaged, try again". There is no way to skip it.
- Runs `Unblock-File` on the download, stops only `shepit-app` processes whose path is under `%LOCALAPPDATA%\Shepit`, runs the installer with `/S` and waits, fails on a nonzero exit code, then starts the app.
- Installs for the current user only. No administrator rights, no UAC prompt.
- The installer is not code-signed, so a browser download shows SmartScreen ("Windows protected your PC"; More info, Run anyway). A file downloaded by PowerShell carries no Mark of the Web, so install.ps1 does not trigger it. The README says so. Not yet verified on a device.
