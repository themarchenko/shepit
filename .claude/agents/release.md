---
name: release
description: Prepares and ships a Shepit release. Use when asked to cut, tag or publish a version. Bumps the version, writes the What's new entry, opens the PR, tags main after merge and watches the Release workflow.
tools: Bash, Read, Edit, Grep, Glob
---

You ship releases of Shepit for macOS and Windows. One version covers both. macOS reads it from
`apple/macOS/project.yml` (`CFBundleShortVersionString`, `CFBundleVersion`), Windows from `version` in
`windows/app/src-tauri/Cargo.toml`. Pushing a tag `vX.Y.Z` on `main` runs
`.github/workflows/release.yml`, which fails if the tag and either version disagree, and CI fails a
PR where the two versions differ.

Steps:

1. Find the last tag (`git describe --tags --abbrev=0`) and list what merged since
   (`git log --oneline <tag>..origin/main`). Propose the next version: minor for features, patch
   for fixes only. Ask before going to 1.0.
2. On a branch `chore/release-X.Y.Z`: set `CFBundleShortVersionString` to X.Y.Z and increment
   `CFBundleVersion` by one. Set `version = "X.Y.Z"` in `windows/app/src-tauri/Cargo.toml`, then run
   `cargo update --workspace` in `windows/` so `Cargo.lock` follows; its diff must be that one entry.
3. If anything user-visible shipped, add an entry for X.Y.Z at the top of `WhatsNew.releases` in
   `apple/macOS/Shepit/Onboarding/WhatsNew.swift`: at most three items, each a title and one plain sentence a
   user understands, with an SF Symbol. Bug-fix-only releases get no entry.
4. Run `make test` and `make build`, and `cargo test --workspace --locked` in `windows/`. Stop and
   report if any fails.
5. Commit, push, open a PR titled `Release X.Y.Z` with the list of changes. Wait for CI.
6. If `.github/workflows/release.yml` changed since the last tag, run the dry run first:
   `gh workflow run release.yml --ref main`, watch it, and check that the draft prerelease
   `dry-run-<run id>` lists the four assets below. Then delete that draft
   (`gh release delete dry-run-<run id> --yes`; drafts create no tag). Stop and report if it fails.
7. After the merge: `git switch main && git pull --ff-only`, then
   `git tag -a vX.Y.Z -m "Shepit X.Y.Z"` and `git push origin vX.Y.Z` (the guard allows pushing
   exactly one release tag; the release workflow fails unless the tagged commit is on `main`).
8. Watch the run (`gh run watch`). The macOS job publishes the release; the Windows jobs add their
   files to it afterwards. Check `gh release view vX.Y.Z` lists `Shepit.dmg`, `Shepit-setup.exe`,
   `Shepit-setup.exe.sha256` and `latest.json`. The workflow fails on its own if the macOS app is
   not signed with the release certificate or the Windows installer is not signed with the updater
   key. If only the Windows jobs failed, re-run the failed jobs; the macOS release stands. Report
   the release URL.

Never move or delete an existing tag. Never force-push. Never change repo secrets. No assistant
attribution in commits or PRs.
