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
- Version = tag with leading "v" removed. The release must have an asset named `Shepit.dmg`, else the release is rejected.

## Installing an update (macOS)

Source: UpdateInstaller.swift:44-106. Swift-only.

- Refuse to run from App Translocation (:44).
- Download the disk image with the expected size, mount read-only, copy the app, detach, remove the quarantine flag (:55-63).
- Accept only if the bundle id matches, the version equals the release's and is newer than the installed one, and the new app satisfies the installed app's designated code requirement (:85-100).
- Swap in place, or ask for an administrator password when the folder is not writable; then relaunch (:66-80, 107-137).

## install.sh (macOS)

Source: install.sh:12-80.

- Needs macOS 26 or later (:19-22). Downloads `Shepit.dmg` from the latest release; `SHEPIT_URL` replaces the URL, `SHEPIT_DIR` replaces `/Applications` (:15-16).
- Verifies the code signature, quits only the copy being replaced, installs (with sudo if needed), strips the quarantine flag, opens the app (:32-57, 62-68).
