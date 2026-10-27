# Shepit for Windows: plan

Status: proposed 2026-10-26. Tasks below run through `/orchestrate`, one at a time unless marked
independent. Each task names what "done" means and what only a person on a Windows machine can
confirm.

## What stays the same across platforms

The behaviour, not the code. `docs/spec/` describes it once: the hotkey gestures, the dictation
states, language handling, local cleanup and dictionary correction, what goes to a polishing agent
and the prompt it gets, what the widget shows when, settings and their defaults, updates and
install. `shared/fixtures/` holds JSON cases (input, expected output) that the Swift tests and the
Rust tests both run, so the two implementations cannot drift on the deterministic parts without a
red test. `shared/prompts/` holds the polishing prompts both read.

## Stack

| Concern | macOS (exists) | Windows (new) |
|---|---|---|
| Language, UI | Swift, SwiftUI + AppKit | Rust core, Tauri 2 shell, React + TypeScript UI |
| Recognition | WhisperKit (CoreML), FluidAudio Parakeet (ANE) | `transcribe-rs`: Parakeet v3 through ONNX Runtime (CPU, DirectML), Whisper through whisper.cpp (Vulkan) |
| Audio | AVAudioEngine | `cpal`, resampled to 16 kHz mono |
| Hotkey | CGEventTap | low-level keyboard hook (`WH_KEYBOARD_LL`) |
| Insertion | pasteboard + CGEvent Cmd+V | clipboard + `SendInput` Ctrl+V, clipboard restored |
| Context | Accessibility API | UI Automation (focused element, selection, password flag) |
| Storage | GRDB SQLite | `rusqlite` |
| Polishing | Claude Code CLI, Codex app-server | same CLIs, same protocol, same prompts |
| Updates | own updater, release certificate | Tauri updater plugin, its own signing key |
| Install | DMG, `install.sh` | NSIS installer, `install.ps1` (PowerShell download carries no Mark of the Web) |

Why Rust and Tauri: one codebase covers Windows now and Linux later, the core crate is testable on
any OS (including the Mac this is developed on), and Handy (MIT, same problem) shows the stack holds
up for dictation. Why not port the Swift core: CoreML and the Neural Engine are the reason the Mac
version is fast, and none of it runs on Windows.

Layout:

```
windows/
  Cargo.toml          workspace
  core/               crate shepit-core: gestures, cleanup, dictionary, settings model, pipeline
                      state machine. No OS calls; runs the shared fixtures.
  app/                Tauri app: src-tauri (OS integration: audio, hook, insertion, UIA, engines,
                      agents, updater) and src (React UI: widget, settings, onboarding)
```

## Tasks

1. **Spec and fixtures** [complex]. Write `docs/spec/` from the Swift implementation. Extract the
   deterministic cases from the Swift tests (dictionary correction, local cleanup, hotkey gestures,
   language detection) into `shared/fixtures/*.json`. Make the Swift tests read them, keeping every
   current assertion. Done: Swift tests green and reading the fixtures; spec reviewed.
2. **Scaffold and CI** [complex]. Cargo workspace, `shepit-core` with the fixture tests, Tauri app
   that starts to a tray icon and an empty transparent widget window, CI job on `windows-latest`
   (fmt, clippy, cargo test, frontend typecheck, `tauri build`). Done: CI green, installer artifact
   uploaded. Device check: installer runs, tray icon appears.
3. **Core logic** [independent]. Port gestures, cleanup and dictionary correction into
   `shepit-core` until every shared fixture passes. Done: fixtures green on macOS and Windows CI.
4. **Audio and hotkey**. Capture, level meter, hold and double tap through the core gesture machine,
   Esc cancels. Device check: hold Right Ctrl, see the level move, release.
5. **Recognition**. Model download with progress and checksum, Parakeet v3 through `transcribe-rs`,
   Whisper turbo optional. Done: a fixture WAV transcribes in CI. Device check: speed on real
   hardware, with and without DirectML.
   Status: Parakeet v3 int8 on CPU, pinned download, English fixture only (a Ukrainian clip is still
   to come). DirectML is deferred to the DirectML build task. See DECISIONS 2026-10-27.
6. **Insertion and context**. Paste with clipboard restore, password fields skipped, app name,
   window title and selection captured. Device check: Notepad, a browser, Windows Terminal, a
   password field.
   Status: code done (clipboard paste and restore, password refusal, context capture, exe profiles);
   device checks still open. See DECISIONS 2026-10-27: Windows insertion and context.
7. **Widget**. The three styles in the black glass, live text, position kept. Device check: looks
   and animations at 100% and 150% scaling.
   Status: code done (three styles, window grows before the shape springs, drag and saved position,
   tray toggle and reset, idle fade, click and hold). No blur, no right-click menu, no live text yet.
   Device checks open: looks and spring at 100% and 150%, hover growth, focus stays in the target app,
   drag across monitors. See DECISIONS 2026-10-27: Windows widget.
8. **Settings and onboarding**. Same panes and defaults as the spec; microphone permission guidance
   for Windows privacy settings.
   Status 2026-10-27: code done (General, Appearance and About panes, key picker with live change, microphone
   test with Windows guidance, 4-step tour, tray left click). Device checks open. See DECISIONS 2026-10-27:
   Windows settings and tour.
9. **Polishing**. Claude Code and Codex workers, same prompts and protocol. Done: worker tests with a
   fake CLI. Device check: with a signed-in Claude Code.
10. **History and dictionary learning**.
11. **Release and install** [complex]. Tag builds the NSIS installer on Windows CI, signs the update
    payload, publishes it to the same GitHub release next to `Shepit.dmg`, `install.ps1`, updater.
    The macOS release flow must keep working unchanged. README gets the Windows install section
    described under Decisions. Device check: install, update to a test version.
    Status 2026-10-27: workflows, `install.ps1`, release check and docs written; updater wiring and
    the update card pending; a dry run (Release workflow, Run workflow) and the device checks are open.
    See DECISIONS 2026-10-27: Windows release and updater.
12. **Live preview**.
13. **Whisper turbo** [optional]. Second recognizer for Windows through `transcribe-rs`, chosen in
    Settings, for the Whisper-only cases (language retry). Pinned download like Parakeet. Done: the
    fixture clips transcribe with it. Device check: speed and Ukrainian quality against Parakeet.

Tasks 1 and 2 come first and in that order; 3 can run beside 4. Everything that touches
`shared/`, `docs/spec/`, release workflows or insertion gets the full review council.

## Decisions (2026-10-26, with the user)

- Default dictation key: Right Ctrl, changeable in Settings. Right Alt is AltGr on many layouts,
  including Ukrainian Enhanced (AltGr+г types ґ), so it is not offered as the default.
- CPU builds first. DirectML and Vulkan builds come after task 5 has speed numbers.
- No paid Windows code signing for now. The unsigned installer triggers SmartScreen; `install.ps1`
  avoids it for the first install because PowerShell downloads carry no Mark of the Web. The README
  says this plainly: that the installer is unsigned, what SmartScreen shows, that the PowerShell
  command skips it and why, and the "More info, Run anyway" route for the manual download.

## What this machine cannot verify

The Mac runs the core crate's tests and builds nothing for Windows locally. Windows builds and tests
run on the CI runner. Anything that needs a real Windows desktop (hotkey feel, paste into real
apps, widget rendering, GPU speed, SmartScreen behaviour) is listed per task as a device check and
stays unconfirmed until someone runs it.
