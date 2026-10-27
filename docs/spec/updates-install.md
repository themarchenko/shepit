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
- Platform-specific, Windows: the updater must verify the update's signature against a key pinned in the installed app before it installs anything, and refuse with no fallback when the check fails.

## install.sh (macOS)

Source: install.sh:12-80.

- Needs macOS 26 or later (:19-22). Downloads `Shepit.dmg` from the latest release; `SHEPIT_URL` replaces the URL, `SHEPIT_DIR` replaces `/Applications` (:15-16).
- Verifies the code signature, quits only the copy being replaced, installs (with sudo if needed), strips the quarantine flag, opens the app (:32-57, 62-68).
