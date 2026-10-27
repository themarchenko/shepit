# Settings

Source: `apple/ShepitCore/Sources/ShepitCore/Settings/AppSettings.swift`. Keys are the persisted names (macOS stores them in UserDefaults; a port keeps the key names and ids so a settings file reads the same). Invalid or missing values fall back to the default (:156-179).

| Key | Type | Default | Values | Line |
|---|---|---|---|---|
| `engine` | id | `parakeet-v3` | `parakeet-v3`, `parakeet-ultra`, `whisper-turbo` | :156; Speech/SpeechEngine.swift:3-6 |
| `hotkey` | id | `rightOption` | `rightOption`, `rightCommand`, `fn` | :3-6, 157 |
| `polishing` | id | `claude` (see polishing.md for the legacy switch) | `claude`, `codex`, `off` | :34-37, 160 |
| `claudeModel` | id | `haiku` | `haiku`, `sonnet` | :26-29, 167 |
| `claudeExecutable` | string | empty (search) | path | :168 |
| `codexExecutable` | string | empty (search) | path | :161 |
| `defaultMode` | id | `clean` | `clean`, `prompt`, `raw` | :169 |
| `fastPathMaxWords` | int | 8 | 0 and up | :171 |
| `promptModeMinWords` | int | 40 | | :172 |
| `languageLock` | string | `auto` | `auto` or a language code | :173 |
| `livePreview` | bool | true | | :174 |
| `dictionary` | [string] | empty | at most 300 learned entries | :175; Learning/DictionaryCorrector.swift:227 |
| `speakerNotes` | string | one sentence, `SpeakerNotes.defaultText` | free text | :176, 247-251 |
| `widgetStyle` | id | `flowPill` | `flowPill`, `captionBar`, `island` | :162 |
| `glassOpacity` | double | 0.7 | 0.5 to 0.9 | :101, 163 |
| `fadeWhenIdle` | bool | true | | :164 |
| `widgetVisible` | bool | true | | :170 |
| `widgetAnchor` | `{x, y}` | unset | | :183-194 |
| `onboardingCompleted` | bool | false | | :165 |
| `whatsNewSeen` | string | empty | newest version seen | :166 |
| `checkForUpdates` | bool | true | | :177 |
| `skippedUpdate` | string | empty | a version | :178 |
| `lastUpdateCheck` | date | distant past | | :179 |

Notes:

- The engine ids are `EngineID` raw values. Parakeet v3 is the default engine and also the live-preview engine (SpeechEngine.swift:140-146).
- `fastPathMaxWords` below 0 is applied as 0 by `max(0, maxWords)` (FormattingService.swift:18); the macOS Settings stepper limits it to 0...30 (macOS/Shepit/Settings/SettingsView.swift:157), and the stored value is not clamped on load (AppSettings.swift:171).
- The 300-entry dictionary cap is `DictionaryLearning.maxTerms` (Learning/DictionaryCorrector.swift:227). It is enforced when learning: new entries are cut to `maxTerms - existing.count` (:242). Entries the user adds by hand are not capped in the code.
- Hotkey display strings (AppSettings.swift:8-14): `Right Option (⌥)`, `Right Command (⌘)`, `Fn / Globe (needs "Press Fn key to: Do Nothing" in System Settings)`. Short key-cap labels (:16-23): `Right ⌥`, `Right ⌘`, `Fn`. The Fn choice needs the system's own Fn action turned off (:12). Platform-specific.
- The legacy `widgetPositions` key is removed on launch (:150, 180). Platform-specific.
- Settings changes take effect without a restart: hotkey, engine preload, agent replacement, live preview and fast-path limit (macOS/Shepit/App/AppDelegate.swift:159-215, see polishing.md).
- Windows file: `app_config_dir()/settings.json` (Tauri), a flat JSON object with the key names above. Only the
  widget keys are read so far (`widgetStyle`, `glassOpacity`, `fadeWhenIdle`, `widgetVisible`,
  `widgetAnchor`). Each key falls back to its default on its own, so one bad value does not reset the rest.
  `glassOpacity` is clamped to 0.5-0.9 on read (macOS does not clamp on load). Keys the app does not know are
  kept when it writes. Writes go to `settings.json.tmp` and are renamed over the file. If the file cannot be
  parsed, defaults are used, and the first write copies the bad file to `settings.json.bak` once before it
  replaces it.
- Speaker notes go into the system prompt (polishing.md).
