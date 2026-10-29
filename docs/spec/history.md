# History and recordings

Paths are relative to `apple/ShepitCore/Sources/ShepitCore` unless they start with `windows/` or `apple/macOS`. History is one SQLite row per inserted dictation. Recordings are the audio of the last 200 dictations. No fixtures: the Rust tests (`windows/core/src/history.rs`, `windows/app/src-tauri/src/history_store.rs`, `recordings.rs`, `history_worker.rs`) cite this file.

## When a row is written

A row is written only after the text reached the target: the insert returned Pasted or ClipboardOnly (Pipeline/DictationSession.swift:209-217). Failures, Esc, an empty or junk transcript, the speech gate and a refused insert write no row. Nothing is logged from a row and no row is sent to an agent.

## Schema

Table `dictations`, columns in this order (Learning/HistoryStore.swift:69-95; Windows windows/app/src-tauri/src/history_store.rs:62-81, `audioFile` added by version 2 at :85-90):

| Column | Type | Value |
|---|---|---|
| `id` | INTEGER PRIMARY KEY AUTOINCREMENT | |
| `createdAt` | DATETIME NOT NULL | GRDB text form `YYYY-MM-DD HH:MM:SS.SSS`, in UTC |
| `bundleID` | TEXT | Target app id. macOS: bundle id. Windows: the exe file name (polishing.md, Windows app identifiers) |
| `appName` | TEXT | App display name (Windows: the exe FileDescription) |
| `windowTitle` | TEXT | Title when recording started |
| `profile`, `mode` | TEXT NOT NULL | Resolved profile id and mode id (profiles-modes.md) |
| `engine` | TEXT NOT NULL | Recognizer id. Windows: `parakeet-v3` (windows/core/src/dictation.rs:263) |
| `rawText`, `formattedText` | TEXT NOT NULL | Transcript and the inserted text |
| `finalText` | TEXT | Always NULL. macOS has `updateFinalText` but no caller (Learning/HistoryStore.swift:112), so nothing reads the field after the paste |
| `language` | TEXT | The format result's language, else NULL |
| `audioSeconds` | DOUBLE NOT NULL | Samples / 16 000 |
| `sttMs` | INTEGER NOT NULL | Time around `transcribe` only (DictationSession.swift:171-180) |
| `llmMs` | INTEGER NOT NULL | Formatting latency; 0 when no agent ran |
| `formatterSource` | TEXT NOT NULL | `claude`, `codex`, `passthrough`, `fastPath` or `fallback` (Formatting/Formatter.swift:25-31) |
| `insertMethod` | TEXT NOT NULL | `paste` or `clipboardOnly` (Insertion/PasteInserter.swift:6-10) |
| `status` | TEXT NOT NULL | `inserted` for paste, `clipboard` for clipboard only (DictationSession.swift:215) |
| `audioFile` | TEXT | Name of the recording file, NULL when none was kept |

Text context (text before the caret, selection) is never stored: there is no column for it. Windows ids: `insert_method_id` and `status_id` (windows/core/src/history.rs:108-122). `PRAGMA user_version` is 2 on Windows. Opening a file made by the other platform is not a goal.

## Retention

- Rows are deleted only by Clear history below, on both platforms. The window loads the newest 200 by `createdAt` (HistoryView.swift:96; Windows `RECENT_LIMIT`, history_store.rs:15, ordered at :150).
- Recordings keep the newest 200 files by name, deleting the oldest after each save (Learning/RecordingStore.swift:7, 28-36; Windows `KEEP_RECORDINGS`, windows/core/src/history.rs:8, `to_prune` :100, only `.wav` files; the Windows prune also sweeps a crash-left `*.wav.tmp`). The count is by file, not by row.

## Recordings file

16 kHz mono 16-bit PCM WAV with a 44-byte header (RecordingStore.swift:46-74; Windows `wav_16k_mono`, windows/core/src/audio.rs:133). Samples are clamped to -1 to 1, multiplied by 32767 and rounded half away from zero. Name `yyyyMMdd-HHmmss-SSS.wav` (:10-15).

## The History window

Mirrors apple/macOS/Shepit/History/HistoryView.swift. The bar shows the row count on the left and, on the right, Clear History… and Refresh:

- Title "Shepit History", 760 x 520, minimum 620 x 420 (:50, 110-114; Windows windows/app/src-tauri/src/ui.rs:21-24).
- Opened from the status item or tray, "History…".
- Each row: time, app (name, else app id, else "?"), `profile · mode · source`, `stt N ms · llm N ms`, a copy button for `formattedText`, the raw text in at most 3 lines, and the formatted text, selectable (:30-47).
- A count line "N dictations" (:19).

## Windows

- **Location.** `app_local_data_dir()`, which is `%LOCALAPPDATA%\com.themarchenko.shepit`: `shepit.sqlite` and `recordings\` (windows/app/src-tauri/src/main.rs:131; `dir_in`, recordings.rs:11). Not the roaming `%APPDATA%`, so dictated text and audio stay out of profile sync, and not the install folder `%LOCALAPPDATA%\Shepit`, which the installer manages.
- **UTC names and times.** `createdAt` and the recording names use UTC (`grdb_timestamp`, `recording_file_name`, windows/core/src/history.rs:55, 93). macOS names use local time. UTC keeps name order equal to time order through a daylight-saving change, and prune depends on that. The window shows the time in the local zone, and the date too when the row is not from today.
- **Password exclusion.** A dictation whose captured password flag is true, or whose insert was refused with "A password field is active", keeps no audio and no row (`finish`, windows/core/src/dictation.rs:397-402). After a successful insert an unreadable flag (None) keeps both, as insertion pastes in that case. On every end without a successful insert (digital silence, speech gate, recognizer error, insert error, Esc) the audio is kept only when the captured flag is false: None (not read yet or unreadable) and true keep nothing, since the field may be a password. To know the flag Windows saves the audio when the dictation ends, not when recording stops, so the dictation steps differ (dictation.md, steps 1 and 9). macOS saves first and so also keeps audio spoken into a password field: a gap, macOS lags.
- **What else is kept.** Quiet, digital-silence and Esc-while-busy dictations that passed the 0.4 s check keep their audio and no row (`Finished`, dictation.rs:175; `take_finished`, :564). Esc while recording and a recording under 0.4 s keep nothing.
- **Clear history.** A button at the window bar, behind a confirmation "Delete all N dictations and their recordings? This cannot be undone." It deletes every row, then every recording (`*.wav`, any `*.wav.tmp` a crash left), even if the rows failed (windows/app/src-tauri/src/commands.rs:183-198). `secure_delete` is on for the connection and `VACUUM` runs after the delete, so the text does not stay in free pages (history_store.rs:58, 188-193).
  - macOS: "Clear History…" sits next to Refresh and is disabled when the store holds no rows. The confirmation is a SwiftUI alert, title "Clear history?", with Cancel and a destructive Delete. N is `HistoryStore.count()`, not the 200 rows on screen. Delete runs `HistoryStore.clear()` (DELETE, then `VACUUM` outside the write transaction), then `RecordingStore.clear()` (every regular file in the recordings folder, subdirectories stay; a failed removal does not stop the rest), even if the rows failed. The first error shows in the window as "Could not clear history: <description>". `secure_delete` is set on every connection in `prepareDatabase`, before migrations. The work runs off the main thread and the button is disabled meanwhile; N is read when the button is pressed. When the store failed to open the button is disabled and recordings are not cleared. The window then reloads and `DictationSession.forgetLastRecord()` empties the in-memory last result, so "Copy last result" no longer offers deleted text.
  - Accepted race, both platforms: a dictation that finishes while the clear runs can leave a row or a WAV behind. Clear is not blocked during a dictation.
- **Live refresh.** After a row is written or history is cleared, the app sends `history-changed` to an open History window, which reloads (ui.rs:104-108). The window is not a Settings tab and has its own capability with only `history_recent` and `history_clear`.
- **Unavailable store.** If the database cannot be opened, dictation still works and WAVs are still saved. The window shows "History is unavailable" (commands.rs:136-170). Errors are logged by kind only.
- **Open folder.** The About pane has a Recordings row, "Last 200 dictations, kept for comparing recognizers", with an Open folder button. The path is computed in the app, never sent from the page (commands.rs:398-404).
- **No import or export**, on either platform.
