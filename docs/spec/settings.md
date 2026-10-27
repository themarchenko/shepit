# Settings

Source: `apple/ShepitCore/Sources/ShepitCore/Settings/AppSettings.swift`. Keys are the persisted names (macOS stores them in UserDefaults; a port keeps the key names and ids so a settings file reads the same). Invalid or missing values fall back to the default (:156-179).

| Key | Type | Default | Values | Line |
|---|---|---|---|---|
| `engine` | id | `parakeet-v3` | `parakeet-v3`, `parakeet-ultra`, `whisper-turbo` | :156; Speech/SpeechEngine.swift:3-6 |
| `hotkey` | id | `rightOption` | `rightOption`, `rightCommand`, `fn`. Windows: default `rightCtrl`; `rightCtrl`, `rightAlt`, `f13` to `f24` (hotkey.md, Windows) | :3-6, 157 |
| `polishing` | id | `claude` (see polishing.md for the legacy switch) | `claude`, `codex`, `off` | :34-37, 160 |
| `claudeModel` | id | `haiku` | `haiku`, `sonnet` | :26-29, 167 |
| `claudeExecutable` | string | empty (search) | path (Windows: plain absolute drive path, see polishing.md) | :168 |
| `codexExecutable` | string | empty (search) | path (Windows: plain absolute drive path, see polishing.md) | :161 |
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
- Windows storage of the update keys: `checkForUpdates`, `skippedUpdate` and `lastUpdateCheck` are fields of the one `settings.json` file (windows/core/src/settings.rs, 19 known keys), with the same key names. Every write, including the updater's own `lastUpdateCheck`, goes through the settings store under one lock, so the About switch and the updater cannot overwrite each other. `lastUpdateCheck` is Unix seconds there (a date on macOS). A missing file, invalid JSON or a wrong type falls back to the default for that key; keys the app does not know are kept when it writes the file. Platform-specific.
- Windows panes: General (dictation key, default mode, prompt-mode word limit, recognizer status, show widget, microphone test),
  Appearance (style, glass, fade when idle, reset position), Polishing (provider, Claude model, executable paths, speaker notes),
  Dictionary (one term per line in a monospace box, sent on every input) and About (version, Recordings with an Open folder button (history.md), the Parakeet CC-BY-4.0 credit, the Updates group, show the welcome tour again). The Updates group has the `checkForUpdates` switch, a status line and Check now or Install... (updates-install.md, Installing an update (Windows)). The Polishing tab sits between Appearance
  and Dictionary and scrolls inside the 620x600 window. Language lock, live preview, recognizer choice,
  What's new and launch at login are not shown until their tasks land (README, Platform-specific). Settings opens on General.
- Windows `hotkey` is stored as the choice id above. A bad id falls back to `rightCtrl`. `onboardingCompleted` defaults
  to false and is set by every way of closing the tour (finish, Skip, Esc, Alt+F4).
- Windows edits from the UI go through one strict patch: an object with any of `widgetStyle`, `glassOpacity`
  (clamped to 0.5-0.9), `fadeWhenIdle`, `widgetVisible`, `hotkey` and `checkForUpdates`. An unknown key, a wrong type or a bad id rejects the
  whole patch and changes nothing. `widgetAnchor`, `onboardingCompleted`, `skippedUpdate` and `lastUpdateCheck` cannot be patched (`skippedUpdate` and `lastUpdateCheck` belong to the updater); they have their own commands.
  Loading the file stays lenient, key by key.
- Windows polishing keys (windows/core/src/settings.rs): `polishing`, `claudeModel`, `claudeExecutable`, `codexExecutable`,
  `defaultMode`, `fastPathMaxWords`, `promptModeMinWords`, `speakerNotes` and `dictionary`, with the names and defaults in the
  table above. Each is read on its own and falls back to its default alone; stored integers are not clamped on read, as on
  macOS. The legacy `formatterEnabled` is read only when `polishing` is missing or not a valid id (false gives `off`, anything
  else `claude`), as in AppSettings.swift:158-160, and stays in the file untouched. `dictionary` is read and written back as a
  list of strings. Read rule (windows/core/src/settings.rs:273-281): a missing key, a value that is not an array, or an array
  holding any non-string element gives an empty list; order and duplicates are kept. Patch rule (:520-526): `dictionary` must be
  an array of strings, else the whole patch is rejected; every item is split on `\r\n`, `\r` or `\n`, trimmed, blanks dropped
  (`normalize_terms`, windows/core/src/dictionary.rs:431), order and duplicates kept, no cap. A change sets both the
  `formatting` and `dictionary` flags (:391-424) and applies from the next dictation. Learning appends through the same store
  and lock, so a learned term and a concurrent edit both survive (dictionary.md). The patch also accepts the
  other eight keys and clamps `fastPathMaxWords` to 0-30 and `promptModeMinWords` to 10-200, the macOS stepper ranges
  (SettingsView.swift:39, 160); a wrong type, a bad id or an unknown key rejects the whole patch. The Polishing tab and the
  tour step call `detect_agents`, which applies the fallback in polishing.md and saves it (app wiring: task 9 chunk C).
  Changes to the provider, model, notes or either path replace the agent 1.5 s later; mode, word limits and dictionary apply
  from the next dictation (settings.rs `changes`, :340-370).
- Windows tour: if the app quits while the tour is open, the flag is not set and the tour shows again on the next
  launch. Accepted.
