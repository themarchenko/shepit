# Behaviour spec

What Shepit does, written from the macOS code as it is today. The Windows app (and later ports) must match it. A rule here is a "must". Every rule cites the `apple/...:line` it was read from; where a line number and the code disagree, the code wins and the spec is wrong.

Platform mechanisms (key monitors, panels, installers) appear only where a rule needs them and are marked platform-specific.

## Files

| File | Covers |
|---|---|
| [hotkey.md](hotkey.md) | Key gesture timings, key ids, what each gesture does |
| [dictation.md](dictation.md) | Session states, limits, speech gate, pipeline order, hallucination filter |
| [language.md](language.md) | Language lock, Whisper retry, live-preview language |
| [local-cleanup.md](local-cleanup.md) | Routing between raw, fast path and agent; text cleanup; word count |
| [dictionary.md](dictionary.md) | Where the dictionary is used, learning, correction algorithm |
| [profiles-modes.md](profiles-modes.md) | Modes, built-in profiles, profile and mode resolution |
| [polishing.md](polishing.md) | Providers, system prompt, per-dictation message, agent limits, Windows agent runtime |
| [widget.md](widget.md) | Phases, styles, sizes, live preview |
| [settings.md](settings.md) | Every setting: key, default, values |
| [updates-install.md](updates-install.md) | Version order, update schedule, installer |

## Fixtures and golden

Test data lives in `shared/` (format in `shared/README.md`). The Swift runners are in `apple/ShepitCore/Tests/ShepitCoreTests`. The Rust port reads the same files.

| Fixture | Spec section | Swift runner |
|---|---|---|
| `shared/fixtures/dictionary-correction.json` | dictionary.md, Correction without a language model | `SharedFixtureTests` |
| `shared/fixtures/dictionary-learning.json` | dictionary.md, Learning | `SharedFixtureTests` |
| `shared/fixtures/hotkey-gestures.json` | hotkey.md, Gesture recognizer | `SharedFixtureTests` |
| `shared/fixtures/passthrough-clean.json` | local-cleanup.md, clean | `SharedFixtureTests` |
| `shared/fixtures/word-count.json` | local-cleanup.md, Word count | `SharedFixtureTests` |
| `shared/fixtures/profile-resolution.json` | profiles-modes.md, Profile resolution | `SharedFixtureTests` |
| `shared/fixtures/mode-resolution.json` | profiles-modes.md, Mode resolution | `SharedFixtureTests` |
| `shared/fixtures/hallucination-filter.json` | dictation.md, Hallucination filter | `SharedFixtureTests` |
| `shared/fixtures/retry-language.json` | language.md, Whisper retry | `SharedFixtureTests` |
| `shared/fixtures/live-preview-join.json` | widget.md, Live preview | `SharedFixtureTests` |
| `shared/fixtures/app-version.json` | updates-install.md, Version order | `SharedFixtureTests` |
| `shared/fixtures/prompt-message.json` | polishing.md, Message per dictation | `SharedFixtureTests` |
| `shared/fixtures/reply-parse.json` | polishing.md, Reply handling | `SharedFixtureTests` |
| `shared/fixtures/reply-finish.json` | polishing.md, Reply handling | `SharedFixtureTests` |
| `shared/fixtures/polish-routing.json` | local-cleanup.md, Routing | `SharedFixtureTests` (secure cases red until the macOS privacy change merges) |
| `shared/audio/manifest.json` | dictation.md, Pipeline (recognition) | none (gap: Swift does not read it; Rust only, `windows/engine/tests/transcribe_fixture.rs`, ignored by default) |
| `shared/prompts/system-prompt.md` (golden) | polishing.md, System prompt | `PromptGoldenTests` |

Rules with no fixture (speech presence, agent limits, widget sizes, settings, update schedule) are described here and tested only in Swift or by hand. The update schedule is also encoded in Rust tests (`windows/core/src/update.rs`) that cite updates-install.md; there is no shared data file, because every fixture would need a Swift runner and so a refactor of the macOS updater.

## How to change behaviour

1. Change the spec and the fixtures (or the golden) in the same pull request.
2. Change both implementations in that pull request, or list the difference under Gaps or Platform-specific below, naming the platform that lags.
3. Run `scripts/agent/gate.sh`; a Swift fixture runner or the prompt golden goes red when the code and the data disagree.
4. A prompt change updates `shared/prompts/system-prompt.md` by hand from the test's `system-prompt.actual.md` output. Nothing rewrites it automatically.
5. A decision or measurement a later reader would have to rediscover goes into `docs/DECISIONS.md`.

## Gaps

Things `docs/PLAN.md` describes that the macOS code does not do. The spec follows the code. Source: PLAN.md sections 5 to 8 against the code.

| Plan intent | Code today |
|---|---|
| Fast path only when there are "no structure cues" | Word count only (Formatting/FormattingService.swift:26) |
| A modifier key forces prompt mode for one dictation | Absent. `modeOverride` exists (Pipeline/DictationSession.swift:31, 190) but no key sets it |
| Recent accepted examples sent to the agent | The request field exists and the prompt builder uses it, but the session never passes any (DictationSession.swift:195-196; PromptBuilder.swift:73-78) |
| Snippets | Absent |
| "About 50 most relevant" dictionary entries in the prompt | All entries to the local corrector; last 120 to the prompt (PromptBuilder.swift:73); last 30 to the recognizer (DictationSession.swift:86) |
| Widget as a 40 pt circle, position per display | Flow pill idle shape is 40 x 8; one global anchor (macOS/Shepit/Widget/WidgetModel.swift:89; Settings/AppSettings.swift:183-194) |
| Narrowing what context is shared, in Settings | Absent. Window title, URL, caret text and selection are always sent when known (PromptBuilder.swift:60-65) |

## Platform-specific and open

Paths below are relative to `apple/` where they point at code.

| Item | State |
|---|---|
| The system prompt says "on macOS" (ShepitCore/Sources/ShepitCore/Formatting/PromptBuilder.swift:7) | Resolved for Windows: the builder takes the platform word and the app passes "Windows"; with "macOS" it equals the golden (polishing.md, System prompt). |
| Profiles are keyed by macOS bundle ids (Formatting/Profile.swift:98-112) | Resolved for Windows by exe file name (profiles-modes.md, Windows app identifiers). The shared fixture holds the macOS ids. |
| Default dictation key | macOS: Right Option. Windows: Right Ctrl, per `docs/PLAN-windows.md`. Key ids and codes are per platform (hotkey.md). |
| Update and install | macOS: GitHub release, `Shepit.dmg`, code signature check, `install.sh`. Windows: `Shepit-setup.exe` and `latest.json` on the same release, Tauri updater with a minisign key pinned in the app, `install.ps1` (updates-install.md). Only the version order and the check schedule are shared. |
| Recognizer | macOS: FluidAudio Parakeet v3 and WhisperKit, models fetched by the libraries. Windows: Parakeet v3 int8 through `transcribe-rs`, files pinned by commit and SHA-256 (`windows/engine/src/pins.rs`), stored under `%LOCALAPPDATA%\Shepit\models`. |
| Parakeet script filter | macOS passes a language hint that filters decoder candidates by script, only for a hard language lock (Speech/ParakeetEngine.swift:34-37). Windows has no such filter. |
| Insertion | Done on Windows (PLAN-windows task 6, dictation.md step 8). Needs device checks. |
| Recordings store, history, dictionary learning, live preview | Windows lags: PLAN-windows tasks 10 and 12. Formatting is done (task 9, polishing.md, Windows); `new_terms` from the agent is parsed and dropped. |
| Settings panes and the tour | macOS: General, Appearance, Polishing, Dictionary, About, and a 5-step tour with permissions and polishing. Windows (tasks 8 and 9): General, Appearance, Polishing, About and a 5-step tour (welcome, microphone, look, polish the text, try it). No language lock, dictionary, history, live preview, What's new, update card or launch at login yet. The microphone step explains Windows privacy settings and notes that apps running as administrator get the text copied, not pasted. |
| Dictation key picker | Windows only: records the next key through the keyboard hook, from a fixed candidate list (hotkey.md, Windows). macOS offers three fixed choices. |
| Tray | Windows: a left click opens Settings and a right click shows the menu. |
| Text insertion, permissions, executable search paths, widget rendering | Per platform. |
| Agent transport | Claude Code is driven through its stream-json mode, Codex through `app-server`. A port matches timings and output cleanup (polishing.md), not the transport. Windows uses the same transports; discovery, spawning and the Job object are in polishing.md, Windows. |
| Reply cleanup is written twice (Formatting/ClaudeCodeWorker.swift:218-225 and CodexFormatter.swift:117-127) | Kept as is; the spec describes one rule. |
