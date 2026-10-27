# Decisions

Short log of choices that are not obvious from the code. Newest first.

## 2026-10-24: scope and defaults (from the plan review)

- Hotkey is configurable; default Right Option hold, double tap for hands-free. Reason: the
  Fn key on this Mac switches input sources.
- Formatting runs through the local Claude Code CLI on the subscription. No API key.
- Speech recognition is local only. Cloud recognizers are out, including as a fallback.
- App name Shepit, bundle id `com.themarchenko.shepit`.

## 2026-10-24: measurements behind the plan

- Apple `SpeechTranscriber` on macOS 26.5: 30 locales, no Ukrainian. `DictationTranscriber`
  lists `uk-UA`. `SFSpeechRecognizer(uk-UA)` has no on-device recognition.
- Headless Claude Code (Haiku), persistent stream-json worker with `--setting-sources ""
  --strict-mcp-config --tools ""`: 1.1-1.3 s per trivial turn, 624 system-prompt tokens.
  One-shot `claude -p`: 2.3-6.0 s. `--bare` skips the subscription login and is not usable.
- Queued stdin messages are merged into one turn, so the app sends one message and waits
  for its `result` before the next.

## 2026-10-24: first launches of the built app (headless checks, no user present)

- Release build signed with the Apple Development identity, installed to `/Applications`,
  launched through Finder (`open -a`) and from a terminal. Launch to ready: recognizer
  (Parakeet v3) 0.2-0.5 s after the first run (30 s the very first time, CoreML compilation),
  Claude worker warm in 1.6-1.9 s.
- Bug found and fixed: in a Finder-launched process, reading the worker's stdout through
  `FileHandle.bytes.lines` delivered nothing, although Claude Code's own debug log showed the
  turn finishing in 2 s. The same code worked from a terminal and in the smoke CLI. The reader
  is now a dedicated thread doing blocking reads on the pipe. Lesson: do not rely on
  `AsyncBytes` for subprocess pipes in a GUI app.
- Second bug fixed on the way: after a timeout the code read `Process.terminationStatus` on a
  running process, which raises an Objective-C exception and crashes the app. The worker now
  distinguishes cancellation from exit and never reads the status of a live process.
- Repeated worker failures pause Claude formatting for 5 minutes; dictation falls back to the
  recognizer text so the app stays usable.
- `--effort low` on the worker: prompt-mode formatting dropped from ~6 s to 1.7-2.5 s on Haiku
  with no visible quality change on the test transcripts.
- Diagnostics: worker stderr in `~/Library/Application Support/Shepit/claude-work/worker-stderr.log`,
  Claude Code's own debug log in `worker-debug.log` next to it (rotated per spawn), app logs via
  `make logs`. In an interactive zsh, `log` is a shell builtin: call `/usr/bin/log`.
- Headless pipeline check: `swift run shepit-smoke --text "…"` or `--audio file.wav` in
  `Packages/ShepitCore` runs recognizer, worker and formatting without the UI.

## 2026-10-24: first interactive launch (user present)

- Microphone and Accessibility granted; the app reports both at launch
  ("microphone granted: true, accessibility trusted: true").
- Bug found and fixed: the first click on the widget did nothing. `AVAudioEngine` raised
  "required condition is false: format.sampleRate == inputHWFormat.sampleRate" inside
  `installTap`, an Objective-C exception that AppKit swallows at the event loop, so the state
  never changed and no error reached the widget. The input node had cached a 24 kHz format (the
  microphone of a pair of AirPods) while the current device, the built-in microphone, runs at
  48 kHz. The recorder now rebuilds the engine after `AVAudioEngineConfigurationChange` and
  whenever the cached format differs from `inputFormat(forBus: 0)`, and throws an `AudioError`
  instead of installing a tap with a mismatched rate. Lesson: never cache audio device formats
  on a Mac that pairs with Bluetooth headsets.
- Verification without the user: a synthetic Right Option tap (CGEvent with keyCode 61, posted
  from a Swift script) made the hotkey fire, capture started at 48 kHz and the 0.19 s recording
  was discarded as too short, so nothing was inserted anywhere. Scripted taps are a usable
  end-to-end check as long as the recording stays under `minimumSeconds`.
- Hotkey gesture outputs and capture start/stop are logged at notice level so acceptance runs
  can be checked afterwards with `/usr/bin/log show`.

## 2026-10-24: accuracy work after the first real dictations

- First two real dictations (Ukrainian, built-in mic, Parakeet v3): "Привіт, привіт, перевірка, як
  працює нова штука" came out as "Привека, привека, перевека, як працює нова штука"; Claude
  repaired two of the three words by guessing. Recognition, not formatting, is the weak link.
- The speaker described their speech: anglicisms inside Ukrainian sentences and two sounds the
  recognizer keeps confusing. That became a "Speaker" section of the formatter's system prompt, editable in
  Settings (`AppSettings.speakerNotes`, default in `SpeakerNotes.defaultText`). Editing it
  respawns the worker (debounced 1.5 s) because the system prompt is cached per process. With the
  notes, Haiku turned "перевев логи" into "перевір логи" and "перевека" into "перевірка".
- Anglicism policy in the prompt: technical terms the recognizer wrote in Cyrillic become Latin
  with standard English spelling (pull request, commit, deploy); Ukrainian-inflected forms used as
  ordinary words stay as spoken (задеплоїти, фіча). Checked on "зроби пул реквест і задеплой на
  стейджинг після комміта": pull request and commit were converted, задеплой kept.
- Every dictation's audio is kept as 16 kHz WAV in `~/Library/Application Support/Shepit/recordings/`
  (last 200, `audioFile` column in the history table, schema v2). This replaces the "record 20
  phrases" step: the recognizer comparison runs on real dictations with
  `shepit-smoke --audio <dir> --engine ... --engine ...`.
- Recognizers now receive hints: a language lock ("uk"/"en") and the dictionary. Whisper gets the
  dictionary plus the last three accepted dictations as its text prompt (style and spelling bias);
  Parakeet applies the language only as a script filter, so it is set only under a hard lock,
  never for mixed speech.

## 2026-10-24: recognizer comparison on the English samples, Whisper made the default

- `shepit-smoke --engine parakeet-v3 --engine parakeet-ultra --engine whisper-turbo` over the two
  English TTS clips (8.6 s and 9.5 s), M1: Parakeet v3 and Ultra 0.12-0.20 s per clip, both wrote
  "cloed code" for "Claude Code"; Whisper turbo 1.25-1.49 s per clip, got "Claude Code" (the term
  was in the vocabulary prompt) and everything else right. Ultra differed from v3 only in
  punctuation and digits.
- Whisper turbo's first load in a new process took 609 s with the model already on disk: CoreML
  compiles the encoder for the Neural Engine and caches the result per process identity in
  `~/Library/Caches/<bundle id or process name>/com.apple.e5rt.e5bundlecache` (200 MB each for
  `shepit-smoke` and `shepit-bench`; the app had only Parakeet's 36 MB). Copying the CLI's
  compiled cache into the app's directory did not help: the app still compiled for 524 s on its
  first Whisper load. The second launch loaded Whisper in 5.9 s, so the cost is one-time per
  process identity. At launch Parakeet loads first (0.5 s) and serves until Whisper is ready.
- `EngineManager` now keeps the previous recognizer serving while a newly selected one loads, so a
  switch never stalls a dictation; the history row records which engine actually ran.
- Default recognizer switched to Whisper turbo before the user's own recordings existed, on three
  signals: Parakeet's Ukrainian on the user's voice (3 of 7 words wrong on a plain phrase), the
  "cloed code" miss above, and the vocabulary prompt that only Whisper supports. The comparison
  on real recordings still runs once they exist; Parakeet stays one menu click away.
- Whisper guards: recognition is skipped when energy-based detection finds under 0.3 s of speech,
  and known Whisper fillers on silence ("Дякую за перегляд", "Субтитри створив DimaTorzok",
  "Thank you.") are treated as nothing recognized (`HallucinationFilter`).

## 2026-10-24: implementation choices (Phase 1 code)

- Swift 5 language mode (`swiftLanguageMode(.v5)`, `SWIFT_VERSION 5`) under the Swift 6.3 compiler.
  Strict concurrency would cost days of AppKit, CoreML and Accessibility API plumbing with no
  payoff for a single-user app. Isolation is still explicit: `@MainActor` UI, actors for engines,
  the Claude worker and the formatting service.
- XcodeGen (`project.yml`) generates `Shepit.xcodeproj`; the project file is not committed. Core
  logic lives in the SwiftPM package `Packages/ShepitCore` so it has `swift test` and no Xcode
  dependency.
- Signing: an Apple Development identity, manual style (moved to a git-ignored
  `Config/Local.xcconfig` on 2026-10-26; the default is ad hoc), no hardened runtime, no sandbox (Accessibility, CGEvent posting and spawning `claude` rule the
  sandbox out). Same identity on every build so TCC grants persist.
- One Claude worker for all profiles: the system prompt lists every profile's rules and the
  per-dictation message names the profile. One process, one prompt cache.
- Claude's reply is JSON inside text, parsed tolerantly (`JSONExtraction`); `--json-schema` was
  measured slower (see the worker probe below).
- Fast path: dictations up to 8 words outside AI targets skip Claude entirely.
- Prompt mode switches on automatically in AI targets from 40 words; both thresholds are settings.

## 2026-10-24: worker probe with a realistic prompt (`spike/worker/probe.py`)

System prompt of ~1.5K tokens (rules, terminal profile, 80-term dictionary, 3 examples),
Haiku, four dictations sent one at a time:

- JSON asked for in the prompt: first turn 3.7 s (prompt cache being created), then
  1.5-1.7 s per turn. Output was valid JSON every time.
- `--json-schema` with stream-json works (`structured_output` is present) but costs a tool
  round trip: 2.5-4.2 s per turn and 2-5x more output tokens. Decision: no `--json-schema`;
  ask for JSON in the prompt and parse tolerantly.
- Quality on the four cases: transliterated identifiers restored (`хендл сабміт` to
  `handleSubmit`, `юзер сервіс свіфт` to `UserService.swift`), self-correction applied,
  prompt mode produced a goal line plus a short list without inventing content.
- `duration_api_ms` in the result event is cumulative across the session; use wall time.
- `new_terms` is noisy (it listed `main` and `git rebase`); the dictionary logic must filter
  candidates against the existing dictionary and keep proper-noun-like tokens only.

## 2026-10-24: recognizer smoke test (English TTS only, M1 16 GB)

Two synthetic English clips (macOS "Samantha" voice, 8.6 s and 9.5 s) through `spike/bench.sh`,
to prove the tool and prime the model downloads. Not a quality verdict: the Ukrainian and mixed
clips need the user's voice.

| Engine | First load (with download) | Latency per ~9 s clip | WER on the two clips |
| --- | ---: | ---: | ---: |
| parakeet-v3 | 56 s | 0.12-0.13 s | 0% / 3.8% |
| parakeet-ultra | 50 s | 0.12-0.13 s | 0% / 15.4% (digits where the reference has words) |
| whisper-turbo | 620 s (~1.5 GB) | 1.0-2.1 s | 0% / 15.4% ("Claude Code" came out "code code") |

Parakeet is 10-15x faster than Whisper on this machine; both heard the TTS "Claude" badly.
Models are cached under `~/Library/Application Support/Shepit/models` (WhisperKit) and
FluidAudio's default cache. WER counts "25" vs "twenty five" as an error; read the per-clip text
in the report before trusting a number.

## 2026-10-26: Whisper language id, prompt cost, rebuild cost

- First Whisper dictations in the app (Sublime Text, 5.5 s and 6.1 s): one was detected as Russian
  and came out "Достаю введення в файла" (said: "Тестую введення в файл"); the 4 words went
  through the fast path, so Claude never saw it. The other was detected as Ukrainian and was right.
- Whisper's language filter in WhisperKit is private, so `WhisperKitEngine` retries instead: a
  detected language other than uk/en is decoded again with the language forced, Ukrainian when the
  first pass is Cyrillic, English otherwise. Free when detection is right; the retry cost 0.8 s on
  the 5.5 s clip. Forcing Ukrainian gave "Достаємо введення в файла": Ukrainian words, but
  "тестую" is still misheard, which is an acoustic error and not a language one.
- The text prompt roughly doubles Whisper's decode time: 0.92 s without and 1.71 s with three
  short sentences of prompt on the 6.1 s clip (the app measured 1.81 s). It also helped there
  ("в файл", the comma). The style sample feeds earlier output back into the recognizer, mistakes
  included. Prompt length is to be tuned on the user's batch of recordings.
- The CoreML compile cache does not survive a rebuild of the app: after `make install` Parakeet
  took 30.9 s and Whisper 526.6 s to load again. Every rebuild costs ~9 minutes on Parakeet before
  Whisper takes over.

## 2026-10-26: ideas taken from Handy, widget placement

Compared with Handy (handy.computer, MIT, Tauri + transcribe-cpp, Whisper and Parakeet, live
overlay text, LLM post-processing through API keys with one global prompt). Taken over:

- Dictionary correction without a language model (`DictionaryCorrector`), on the paths that skip
  Claude (fast path, no worker, worker failure). Terms are matched by a coarse sound key after
  transliterating Cyrillic, so "клод код", "Cloud Code" and "Claude Code" agree. Tuned on false
  hits found by the tests ("колода карт" first matched "Claude Code"): keys of at least 5 sounds,
  exact below 7, one sound of slack above; inflected forms ("після коміту") are left for Claude;
  Latin text only toward multi-word terms, so "Whisper" stays "Whisper" with "Shepit" in the
  dictionary. It does not help misheard ordinary words such as "тестую" heard as "достаємо".
- The dictionary was empty, so it now learns from Claude's `new_terms`, filtered to
  proper-noun-like Latin tokens (capital, dot, underscore or digit; "main" and "git rebase" are
  dropped), up to 300 terms. Whisper gets only the 30 newest terms because its prompt costs time.
- Live preview while recording (`LivePreview`): Parakeet v3 recognizes the recording so far every
  0.5 s; past 12 s the head is frozen at a quiet point so a pass stays short. Measured with
  `shepit-smoke --live` on the two real recordings: 7-8 updates per 5-6 s clip, 0.07-0.30 s per
  pass. Parakeet's language hint is a script filter: without it the first seconds flip between
  Polish, English and Cyrillic, so the preview uses the language lock or the last dictation's
  language. FluidAudio's `SlidingWindowAsrManager` was not used: it emits nothing before 13 s of
  audio. Parakeet now stays loaded next to Whisper.
- Not taken: unloading the model after idle time (Whisper reloads in ~7 s, memory is not short).

Long dictations with Whisper (`ProgressiveTranscriber`): a 46 s prompt dictated into Claude Code
waited 10.8 s for Whisper and 2.9 s for Claude after release. Whisper now transcribes long
recordings in 7-12 s pieces while they are still being recorded (cut at the quietest 100 ms once
14 s are waiting); later pieces get the text so far as prompt and the first piece's language. After
release only the rest is left. Measured on that recording with `shepit-smoke --progressive`
(played in real time): 3.8 s after release instead of 9.8 s, text of the same quality (both runs
garbled the same two places). Parakeet is fast enough to skip this, and a failed piece falls back
to one transcription of the whole recording. The history row now records the engine that actually
ran, not the selected one.

The same dictation's inserted text began with a sentence that is not in the audio (both engines
agree it starts later); most likely it was already in the input line and Claude repeated
`text_before_caret`. Not reproduced in three runs with that context, but the system prompt now says
the context is never copied into the output.

Widget placement: the per-display position was saved under the widget's current display number
but looked up under the main display's, so a position on a second monitor was never restored, and
the widget always snapped to a side edge. It now keeps one anchor (middle of the bottom edge, global
coordinates) that survives relaunches, grows around it, and falls back to the bottom middle of the
menu-bar display when the anchor is on no connected display.

## 2026-10-26: widget styles, settings, welcome tour, polishing choice (0.2.0)

- Six widget concepts were mocked in `docs/design/widget-concepts.html`; the user picked 1 (Flow
  pill), 2 (Caption bar) and 4 (Island) and wants all three selectable in Settings → Appearance.
  The look is black translucent glass (opacity adjustable 0.5-0.9), white text, red only for the
  recording dot, amber for errors, green for success. Primary buttons are off-white: the user
  rejected a blue one in the mock (`docs/design/settings-onboarding.html`).
- The widget panel grows at once around its anchor and shrinks 0.7 s after the shape settles, so
  SwiftUI's springs are never clipped by the window and the panel does not cover more of the screen
  than it needs between dictations. The window shadow is off; the glass draws its own, which
  follows the animated shape.
- Settings moved to a toolbar-tab window: General, Appearance, Polishing, Dictionary, About.
- Polishing is a choice of Claude Code, Codex or Off. The app looks for both CLIs in the usual
  install folders; a missing agent cannot be picked, and when the selected one is gone the choice
  falls back to the other agent or to Off. Off inserts the text as heard, with spacing, capitals and
  dictionary terms fixed locally.
- First launch shows a five-step welcome tour (dictation demo, the two permissions, style, polishing,
  a box to try it in). Until the tour is done the app does not show the system permission prompts
  at launch; the tour's buttons do. After an update with an entry in `WhatsNew.releases`, a card
  rises from the widget once; bug-fix releases have no entry and stay quiet. Someone who dictated
  before the tour existed gets the card right after the tour.

## 2026-10-26: no recent-dictation prompt for Whisper

English dictations came out in Ukrainian: "Okay, let's try to test this application" was inserted
as "Окей, давайте тестимемо цю апп." Whisper detected English correctly, but its prompt held the
last three dictations, all Ukrainian, and it followed the prompt's language. Without that prompt
the same two recordings came out right in English, in 0.7 s instead of 2.5-6.5 s. Whisper now gets
only the dictionary terms. Inside one long dictation, later pieces still get the text so far,
which is in the same language.

The widget's shadow ended in a visible rectangle: the panel margin (16 x 12 pt) was smaller than
the shadow's blur and offset. The margin is now 28 x 26 pt with a radius-10 shadow.

## 2026-10-26: live-preview language, brand

- The live preview wrote Ukrainian as Polish-looking Latin after two English dictations: its
  Parakeet script filter came from the last dictation's language. Asking Whisper for the language
  during recording was right but slow (first words after 4-5 s). Now each early pass decodes twice,
  with the Ukrainian and the English filter, and Parakeet's own confidence picks the script. On five
  of the user's recordings the more confident script was wrong for 2 of 3 Ukrainian clips at 1.5 s
  and right for every clip from 2.5 s, so an early pass is shown only when the winner has at least
  0.6 and leads by 0.2, and after 3.5 s of audio the language sticks. First words now appear at
  1.2-2.7 s, never in the wrong script on those recordings.
- Brand: of four directions (`docs/design/branding.html`) the user picked "voice to cursor", three
  rising bars flowing into a text caret, white on the widget's black glass with the teal and amber
  glows. `scripts/make-icons.swift` renders the app icon and an 18 pt template menu bar glyph into
  `Shepit/Assets.xcassets`; the in-app mark is `ShepitMark` with the same geometry. The icon is drawn
  edge to edge: rendering the built app with `NSWorkspace.icon(forFile:)` showed macOS 26 applying its
  own rounded mask and rim, so a drawn squircle with margins would have been shrunk inside it.

## 2026-10-26: name, open source, Settings look

- Renamed from Whispr to Shepit (Ukrainian "шепіт", a whisper) before the first public commit.
  "Whispr" was too close to Wispr Flow, the app the widget took cues from. Earlier entries use the
  new name throughout. Bundle id `com.themarchenko.shepit`; the one local install was moved by hand
  (preferences exported and imported, Application Support folder and database renamed).
- Published as open source under MIT. Signing moved out of `project.yml`: ad hoc by default so
  anyone can build, a personal team in the git-ignored `Config/Local.xcconfig`. Release builds from
  CI are ad hoc too, so macOS asks for Accessibility again after each update; notarization needs a
  Developer ID account.
- Releases: a `vX.Y.Z` tag on `main` runs `.github/workflows/release.yml`, which checks the tag
  against `project.yml`, runs the tests, zips the app and publishes a GitHub release.
- Settings moved off the system window look onto the widget's black glass: a borderless-looking
  titled window over a behind-window blur, its own tab bar, grouped forms with the system
  background hidden. The status item menu stays a system `NSMenu`.
- Widget style previews: three side-by-side cards at a fixed 0.46 scale overflowed once the caption
  bar (560 pt) or the island card appeared. Replaced by a segmented choice over one preview scaled
  from the style's largest shape (`WidgetLayout.largest`), checked offscreen in every phase.

## 2026-10-26: install without the quarantine hunt

- A colleague installing 0.2.0 from the zip could not find how to get past Gatekeeper: macOS 15 and
  later dropped the right-click Open route, and "Open Anyway" sits at the bottom of Privacy & Security
  for about an hour only. Two changes:
- `install.sh` at the repository root, run as `curl ... | bash`. curl does not set
  `com.apple.quarantine`, so the app it installs opens with no prompt. It downloads
  `releases/latest/download/Shepit.dmg`, verifies the signature, quits only the copy it replaces and
  opens the new one. Tested into a scratch folder from a disk image carrying a Safari quarantine flag:
  the installed app had none, a Finder-style copy of the same image kept it.
- The release asset is now `Shepit.dmg` (same name every release, so the latest link is stable). Its
  window shows the drag into Applications and the Open Anyway steps. Built by `dmgbuild` in a venv
  under `.build/venv`, which writes the Finder layout without scripting Finder, so it runs on CI. CI
  builds the image on every pull request, because a release tag cannot be moved once pushed.
- The same colleague found the welcome tour stuck above System Settings: the tour window was
  `.floating` and could not be dragged (`isMovableByWindowBackground` does nothing over SwiftUI
  content). It is now a normal window with `WindowDragGesture`, its Accessibility button asks for
  access (which adds Shepit to the list) and opens the list, and it comes back to the front when
  access is granted. The Settings permission button now asks first too; before, it opened a list
  without Shepit in it.

## 2026-10-26: updates from GitHub, one signing certificate

- Shepit asks `api.github.com/repos/themarchenko/shepit/releases/latest` 20 seconds after launch and
  then whenever the last check is a day old (an hourly timer looks, so a Mac that sleeps through the
  night still checks). A newer version shows a card in the top right corner: install and reopen,
  later, or skip this version. Menu bar and Settings → About can check by hand. Sparkle was the
  alternative; it needs an appcast and its own EdDSA keys, and its windows do not match the black
  glass. The GitHub release is the feed, so there is nothing else to publish.
- Install: download `Shepit.dmg` with URLSession, mount it, copy the app out, remove the quarantine
  flag (URLSession sets none because Shepit does not opt in, but clearing it costs nothing), check
  bundle id, version and signature, copy it next to the running app and swap with
  `FileManager.replaceItemAt`, then quit and reopen through a shell that waits for the old process.
  A folder the user cannot write to gets an administrator prompt through `osascript`. A translocated
  copy refuses and asks to be moved to Applications.
- The signature check is the security boundary: the new app must satisfy the running app's
  designated requirement. Releases up to 0.2.1 were ad hoc, whose requirement is a single cdhash, so
  no update could ever pass, and macOS dropped Accessibility on every update. From 0.3.0 CI re-signs
  the app with a self-signed "Shepit Release" certificate (10 years, in the repo secrets, private copy
  in `~/.shepit-signing` on the maintainer's Mac). The requirement becomes
  `identifier "com.themarchenko.shepit" and certificate leaf = H"96a6c4..."`, stable across builds,
  so TCC keeps the permissions and the updater accepts only builds from CI. The release workflow fails
  if the signature does not carry that leaf. Hardened runtime stays off, so library validation does
  not object to a certificate without a team.
- Tested with `SHEPIT_UPDATE_E2E` (UpdateTests.swift) on disk images built locally with a test
  certificate: a signed 9.0.0 installed over a scratch 0.2.1 from an image with a Safari quarantine
  flag and came out without it; an ad hoc 9.0.0 and a same-version build were refused and left the
  old app untouched. The CI signing steps were replayed locally in a throwaway keychain.

## 2026-10-26: one repository, one folder per platform

- A Windows version is next, and Linux and phones may follow. Each platform gets its own
  implementation on the stack that suits it; what they share is behaviour, decided once here and in
  the spec. Rewriting the working Swift core into a shared cross-platform core would trade CoreML and
  the Neural Engine on the Mac for portability, and risk a stable app for code nobody has yet.
- The Swift code moved under `apple/`: `apple/ShepitCore` (package, ready for an iOS target later),
  `apple/macOS` (XcodeGen spec, app sources, signing config, disk image, generators) and
  `apple/spike`. `install.sh`, `docs/`, `.github/` and `.claude/` stay at the root. Paths in entries
  above this one are the old ones.

## 2026-10-26: agent pipeline, Windows plan

- Ported the pipeline from ShipKit and Whitebox: `/orchestrate` triages tasks, `/feature` runs one
  in its own worktree (audit, plan, tests first, implement, inversion evidence, platform check,
  review council, validate, draft PR). Opus runs the main session and the planner, because the
  architecture calls are where a weaker model costs the most to undo. Sonnet writes tests and code
  and reviews. Haiku audits and validates, which is read-only legwork. Reports go to
  `docs/_working/` and only a summary returns to the main session, to keep Opus context small.
- `scripts/agent/gate.sh` runs only the checks for the platform a diff touches (`swift test` for
  `apple/`, cargo and the frontend checks for `windows/`, both for `shared/` and `docs/spec/`), so a
  Windows change does not pay for a Swift build. The Stop hook skips docs-only diffs and trees it
  already passed.
- `bash-guard` blocks what has bitten this repo or would be hard to undo: staging everything,
  commits on `main`, force pushes, touching release tags, changing the keychain search list (a test
  command did that once and hid the login keychain from `gh` for 25 minutes), reading the signing
  folder, and recursive deletes outside the project or a temp dir.
- Windows: Rust core plus a Tauri 2 shell, recognition through `transcribe-rs` (Parakeet v3 on ONNX
  Runtime, Whisper on whisper.cpp). Full reasoning, layout and the task list in
  `docs/PLAN-windows.md`.

## 2026-10-26: shared spec and fixtures

- `docs/spec/` describes what the macOS code does today, with a source line for every rule. The Windows
  app must match it. What `docs/PLAN.md` intended but the code lacks is listed under Gaps in
  `docs/spec/README.md`, not promised in the contract.
- Deterministic behaviour (gestures, dictionary correction and learning, profile and mode resolution,
  the hallucination filter, the Whisper retry, version order) is data in `shared/fixtures/*.json`. The
  Swift tests read it; the Rust port reads the same files. 55 assertions moved out of the Swift tests.
- The sound key and the spoken-parts splitter stay Swift-only: they are not public, and they are pinned
  through the `correct` cases. The spec documents them as the reference algorithm.
- The system prompt is not moved into a shared resource. The package declares no `resources:`, and
  bundling the file would change the app. A golden copy in `shared/prompts/system-prompt.md`, checked
  byte for byte against `PromptBuilder` output with the default speaker notes, catches drift.
- A behaviour change updates the spec and the fixtures in the same pull request; a platform that lags is
  named under Gaps or Platform-specific until it catches up.

## 2026-10-26: Windows scaffold

- Tauri is pinned to 2.12: `@tauri-apps/api` and `@tauri-apps/cli` at 2.12.4, the `tauri` crate at 2.12.5 (same minor, which `tauri build` requires). The 3.0 alpha is out,
  but an alpha under a shipping app is a cost with no feature we need yet.
- The Rust toolchain is pinned in `windows/rust-toolchain.toml` (1.97.1 with rustfmt and clippy), so
  local builds and the CI job use the same compiler.
- The identifier is `com.themarchenko.shepit`, the same as the macOS bundle id. One app identity across
  platforms; nothing shares an install location, so there is no clash.
- The widget window is 96x60 and shows the idle pill. The default position is bottom centre of the
  primary display.
- TypeScript is pinned to 6.0.3: typescript-eslint refuses 7. Raise it when the lint stack allows.
- The agent gate runs per platform. Finding node through nvm landed separately (#8).
- The Windows job (`windows.yml`) is not a required check yet. Make it required once it has passed on a
  few pull requests, and the installer artifact has been installed on a real machine.

## 2026-10-27: Windows recognition

- Crate `windows/engine` (`shepit-engine`): `src/pins.rs` (pin table), `src/download.rs`,
  `src/recognizer.rs`, test `tests/transcribe_fixture.rs`. It depends on `transcribe-rs` =0.3.12 with
  default features off and only `onnx` (its Cargo.toml features; `default = []`). GPU features
  (`ort-directml`, `ort-cuda`) stay off.
- Files Parakeet int8 opens (transcribe-rs src/onnx/parakeet/mod.rs:96-105): `encoder-model`,
  `decoder_joint-model`, `nemo128.onnx`, `vocab.txt`. The loader resolves `{name}.int8.onnx` and falls
  back to the fp32 `{name}.onnx` when the int8 file is missing (src/onnx/session.rs:216-242). So the
  pins list exactly `encoder-model.int8.onnx`, `decoder_joint-model.int8.onnx`, `nemo128.onnx` and
  `vocab.txt`; the fp32 encoder (2.4 GB extra) is never fetched. About 670 MB in total.
- `ort` is pinned to 2.0.0-rc.12 in `Cargo.lock` and in `windows/engine/Cargo.toml`. transcribe-rs
  0.3.12 asks for `2.0.0-rc.12`, but Cargo resolves rc.15, which it does not compile against
  (`Session::builder()` needs an `Environment` there, session.rs:154). Never run `cargo update` on
  ort; CI uses `--locked`.
- ONNX Runtime is downloaded at build time by ort-sys (feature `download-binaries`) from pyke's CDN
  (ort.pyke.io / cdn.pyke.io). The archive hash is baked into the crate and compared after download:
  a mismatch is a build error (ort-sys build/main.rs:125-130). The crate itself is pinned by the
  `Cargo.lock` checksum, so the expected hash cannot drift. It is linked statically on
  x86_64-pc-windows-msvc (`rustc-link-lib=static=onnxruntime`, main.rs:160; dist `ms@1.24.2`,
  build/download/dist.txt:8).
- Finding: the pyke build is always compiled with DirectML, so ort-sys links dxguid, DXCORE, DXGI,
  D3D12 and DirectML import libs even for CPU (static_link/mod.rs:54-62, comment at :56). The exe
  therefore imports DirectML.dll at start-up. `copy-dylibs` (ort default) leaves
  `windows/target/release/DirectML.dll` as a symlink into ort's download cache, and the Tauri NSIS
  installer did not carry it. Fix: CI copies the resolved file to `windows/app/src-tauri/runtime/`
  (git-ignored) and builds with `--config src-tauri/tauri.runtime.conf.json`, whose
  `bundle.resources` maps it to the install folder root next to the exe. A CI step lists the
  installer with `7z l` and fails when DirectML.dll is missing; another prints the exe's imports
  (`dumpbin /dependents`). The config is passed on the command line, not kept in `tauri.conf.json`,
  because tauri-build fails every plain `cargo build` when a listed resource is missing. A release
  workflow for Windows must pass the same config and stage the file first. Needs a clean-machine check.
- Pins: repo `istupakov/parakeet-tdt-0.6b-v3-onnx` at commit
  `8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce`, each file with size and SHA-256 (sizes and hashes from
  the `/tree/<commit>` API, re-checked with `shasum -a 256`). Files are downloaded to `<name>.part`
  and renamed only after a full SHA-256 match. At launch a file counts as installed when it exists
  with the pinned size; it is not re-hashed at launch. `verify_installed` re-hashes every pinned file
  and deletes mismatches: the app runs it once when a load fails (then downloads again and retries),
  and the CI fixture test runs it so a poisoned cache cannot pass. The `.part` file is `sync_all`ed
  before the rename. No auth header is sent and `HF_TOKEN` is never read.
- Downloads are https only (also after redirects; other hosts are allowed, the SHA-256 guards the
  content). ureq 3.5 has no per-read timeout (`timeout_recv_body` is a budget for the whole body),
  so a helper thread reads the body and the download fails after 60 s without data; the `.part`
  file is kept and resumes. A stalled socket leaves that thread blocked until the OS closes it.
- The recognizer sends `Ready` once. The controller loop keeps the latest model status outside the
  loop and gives it to a controller restarted after a panic, so dictation does not stick in
  "Loading model".
- Models live in `%LOCALAPPDATA%\Shepit\models\parakeet-tdt-0.6b-v3-int8` (`dirs::data_local_dir`),
  not in the roaming profile: 670 MB does not belong in roaming sync.
- Download retry differs from macOS. macOS rethrows the cached load failure
  (Speech/SpeechEngine.swift:120-121 loads once per engine id). A network download can fail for a
  transient reason, so on Windows a failed load is retried on the next dictation, and an interrupted
  `.part` resumes with a `Range` request (a 200 reply restarts from zero).
- Measured on the M1 Mac, release build, CPU: the 7.5 s fixture clip took 0.39 s, real-time factor
  0.052; 30 s of the full intro took 1.61 s (0.054). This is not a Windows number. Windows CI prints its
  own factor; DirectML speed is for the DirectML build task.
- Model license: CC-BY-4.0 (the NVIDIA Parakeet v3 weights, as repackaged by istupakov). Attribution
  belongs in the app's About text before release.
- The audio fixture `shared/audio/en-librivox-preamble.wav` is English only. A Ukrainian clip follows
  once one is provided or approved.

## 2026-10-27: Windows insertion and context

- Crate: only `windows` 0.62.2 with extra features for COM, clipboard, UI Automation and SendInput. No
  `arboard`: it handles text and images only, while macOS saves every pasteboard type
  (PasteInserter.swift:26-41). We save every format in enumeration order and skip GDI handles and
  private ranges (`is_restorable_format`, windows/core/src/insert.rs:62).
- The clipboard owner is a message-only window on the worker thread. A NULL owner makes
  `SetClipboardData` fail after `EmptyClipboard`. The worker pumps messages every 20 ms, also during the
  wait. `OpenClipboard` is retried 5 times, 20 ms apart.
- Restore rule: after 350 ms, only if `GetClipboardSequenceNumber` still equals the number read after our
  write (macOS change-count check, PasteInserter.swift:52, 64, 72). Esc while inserting returns to idle
  and the worker still restores.
- Consoles: Ctrl+V everywhere (Windows Terminal default; conhost has Ctrl shortcuts on by default since
  Windows 10). Fallback if a device check fails: Shift+Insert for the terminal set.
- The hotkey hook ignores our paste keys twice: `is_accepted` rejects Ctrl without the extended flag and
  V, and `KeyFilter` drops `LLKHF_INJECTED` (windows/core/src/keys.rs:35-39, 54-57). A test pins the paste
  keys against both. They never use `KEYEVENTF_EXTENDEDKEY`, which would be Right Ctrl.
- Password rule: refuse if the field was a password at recording start or is one at insert time; an
  unreadable flag pastes anyway (PasteInserter.swift:58; ContextProbe.swift:24). Context text is read only
  when the flag was read and is false.
- Context: exe file name, `FileDescription`, window title, caret and selection through UI Automation
  (TextPattern2, TextPattern, ValuePattern). Limits as in ContextProbe.swift:35-44; the `TextPattern` path
  returns at most 1000 characters before the caret. Elevated targets are not detected: paste to an
  elevated window fails silently (known gap).
- URL is not captured on either platform (macOS never sets it, ContextProbe.swift:18-48). Deferred.
- Profiles use the exe file name; the table and a Windows-only fixture are in
  docs/spec/profiles-modes.md. A `shared/` fixture would need a Swift runner.
- Privacy: no clipboard data, dictated text or context in logs; only outcome kinds and a line of
  field-presence flags.
- PARKED for the user: Windows clipboard history and cloud sync would keep the dictated text. Writes are
  unmarked, like macOS today. Marking the data (exclude-from-history formats) is a one-function change in
  `put_text`. Decide whether to do it on both platforms.
- macOS gaps found: context capture is not gated on secure input (spec README.md:64), and macOS skips the
  clipboard restore after Esc (PasteInserter.swift:71).
- Not verified: all runtime behaviour. Windows CI is the first compile of `insert_win.rs`. Device checks:
  Notepad, Chrome or Edge, Windows Terminal, cmd, password field, held Shift or Alt, elevated terminal,
  Store-app exe names.

## Pending (Phase 0)

- Recognizer verdict for Ukrainian and mixed speech: record `spike/record.sh`, run
  `spike/bench.sh`, read `spike/results/bench-*.md`. Default stays Parakeet v3 until then.
- First interactive launch: grant Microphone and Accessibility, then the Phase 1 acceptance
  checks from the plan (dictate into Claude Code in Terminal, Claude.app, claude.ai in Chrome;
  release-to-insert under 3 s; grants survive a rebuild).
