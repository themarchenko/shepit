# Shepit

macOS dictation app: hold a key, speak Ukrainian or English, release, and the text lands in the
focused app. Recognition runs on device (WhisperKit, FluidAudio). Optional polishing goes through the
Claude Code CLI or Codex.

## Layout

- `Packages/ShepitCore`: all logic that does not need AppKit windows. Audio, hotkey gestures,
  recognizers, live preview, formatting agents, insertion, history, settings. Has `swift test`.
- `Packages/ShepitCore/Sources/shepit-smoke`: headless runner for recognizers and formatting.
- `Shepit/`: the app. Widget, Settings, onboarding tour, What's new, status item.
- `docs/DECISIONS.md`: dated log of decisions and measurements. Add an entry for anything a future
  reader would otherwise have to rediscover.
- `docs/PLAN.md`: the original plan and architecture.
- `spike/`: phase 0 benchmarks. Recordings are local only.

## Commands

```
make build      # XcodeGen + Release build into .build/DerivedData
make install    # build and copy to /Applications
make test       # ShepitCore unit tests
make package    # disk image for a GitHub release, in .build/dist
make logs       # live logs of the running app
```

`project.yml` is the source of truth; `Shepit.xcodeproj` is generated and not committed.
Signing is ad hoc unless `Config/Local.xcconfig` names a team (see `Config/Signing.xcconfig`).

## Conventions

- English only in code, comments, docs, commits and UI strings.
- No em dashes in any text. Plain words, no filler.
- Comments explain why, not what. Match the density of the file you are in.
- `@MainActor` for UI and the session; actors for engines and agent workers.
- A user-visible feature gets an item in `WhatsNew.releases` for its version. Bug fixes do not.
- UI changes: check them by rendering the view offscreen (`NSView.cacheDisplay`), never by capturing
  the screen. The screen may show private windows.

## Git

- `main` is protected: linear history, changes land through pull requests, squash merge only.
  Branches are deleted after merge.
- Branch names: `feat/...`, `fix/...`, `chore/...`, `docs/...`.
- Commit messages: imperative subject under 72 characters, body when the reason is not obvious.
  No assistant attribution lines.
- Releases: bump `CFBundleShortVersionString` and `CFBundleVersion` in `project.yml` in a PR, merge,
  then tag `vX.Y.Z` on `main` and push the tag. `.github/workflows/release.yml` builds, signs with the
  "Shepit Release" certificate (repo secrets `SIGNING_P12_BASE64`, `SIGNING_P12_PASSWORD`), packs a disk
  image and publishes the GitHub release. The `release` agent does all of this.
- The in-app updater installs only builds signed with that certificate. Never publish a release
  signed otherwise, and never replace the certificate: installed copies would stop updating.
