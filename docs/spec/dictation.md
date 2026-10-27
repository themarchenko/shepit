# Dictation session

Paths are relative to `apple/ShepitCore/Sources/ShepitCore`. One dictation: record, check for speech, transcribe, format, insert, log.

## States

| State | Meaning | Source |
|---|---|---|
| `idle` | Nothing happening. | Pipeline/DictationSession.swift:7 |
| `recording(hands_free)` | Microphone open. | :8 |
| `loading_model` | The selected recognizer is not the loaded one. | :9, 163-170 |
| `transcribing` | | :10 |
| `formatting` | Mode and profile resolved, polishing running or local cleanup. | :11, 188 |
| `inserting` | | :12, 208 |
| `done(preview)` | Result inserted. | :13, 220 |
| `failed(message)` | | :14, 233 |

- "Recording" means `recording`. "Busy" means loading_model, transcribing, formatting or inserting (:16-26).
- Start is accepted only from idle, done and failed (:67-70). Start cancels a pending reset (:71).
- `toggleHandsFree` (widget click): hands-free recording stops; non-hands-free recording becomes hands-free; idle, done and failed start a hands-free recording; busy states ignore it (:129-136).

## Timings and limits

| Rule | Value | Source |
|---|---|---|
| Sample rate of recordings | 16 000 Hz mono | :109, 141 |
| Minimum recording | 0.4 s. Shorter: discard silently, back to idle, no error. Compared as `seconds >= 0.4`. | :49, 110-116 |
| Minimum speech | 0.3 s of speech in the recording. Less: fail with "Nothing recognized", recognition is skipped. Compared as `>= 0.3`. | :155-160 |
| Done resets to idle | 2.5 s | :221 |
| Failed resets to idle | 4 s | :234 |
| Done preview length | first 60 characters of the result, newlines turned into spaces | :218 |

The reset only fires if the state is still done or failed (:242-245).

Windows only: a recording that is digital silence (every sample zero, which is what a blocked microphone gives) fails with
"Microphone: no sound, check Windows microphone privacy settings" before the speech check runs. Quiet but non-zero audio still
fails with "Nothing recognized". macOS has no such case, because a denied permission is reported when the recorder starts.

## Speech presence

Fixture: none (float input). The Rust port is `speech_seconds` in `windows/core/src/speech.rs:65`; its tests are a port of `SpeechPresenceTests` with a seeded generator in place of `Float.random`. Reference algorithm, Audio/SpeechPresence.swift:7-24:

1. Split samples into frames of 20 ms (`sampleRate / 50` samples; 320 at 16 kHz). Fewer samples than one frame: 0 s of speech. A trailing partial frame is dropped.
2. Frame energy = RMS of the frame.
3. Noise floor = the energy at index `count / 5` (integer division) of the sorted energies, that is the 20th percentile.
4. Threshold = max(3 x noise floor, 0.006).
5. Speech seconds = number of frames with energy strictly above the threshold, times 20 ms.

## Pipeline order

Source: DictationSession.swift:140-227.

1. Save the audio to the recordings store before anything can fail (:142-151). Save failures are logged and ignored.
2. Speech gate (above). Fail "Nothing recognized".
3. Get the selected engine; state `transcribing`, or `loading_model` first if another engine is loaded (:162-170).
4. Transcribe. If the progressive transcriber already has a finished result, use it; else transcribe the whole recording with the hints (:174-179). Hints are built when recording starts (see dictionary.md and language.md).
5. Drop the result if the transcript is empty or `HallucinationFilter.isJunk` (below). Fail "Nothing recognized" (:182-185).
6. Resolve profile and mode (profiles-modes.md) and format (local-cleanup.md, polishing.md).
7. Learn new dictionary terms from the formatter's report (dictionary.md) (:202-206).
8. Insert the result at the cursor of the app that was focused when recording started (:208-209). Insertion is refused when secure input is active, either recorded in the captured context or reported now by `IsSecureEventInputEnabled()` (Insertion/PasteInserter.swift:84); the dictation then fails. The paste path puts the text on the clipboard, also declaring the types `org.nspasteboard.TransientType` and `org.nspasteboard.ConcealedType` (empty data) so clipboard managers do not store it; the restore of the previous contents is not marked. It then sends the paste shortcut, waits 350 ms, and restores the previous clipboard contents unless something else changed the clipboard meanwhile. When the dictation is cancelled during the wait, the restore still happens, after the same 350 ms: the paste shortcut is already sent, and an earlier restore could paste the previous contents into the target (PasteInserter.swift, `insert` and `writeDictation`). Platform-specific: clipboard paste and the Accessibility fallback. When the insert method is "clipboard only" the status is `clipboard` and the done preview is prefixed with a hint to grant Accessibility (:215, 220).

   Windows (windows/app/src-tauri/src/insert.rs, insert_win.rs; decisions in windows/core/src/insert.rs): the state is `inserting` from accepted recognition until the restore finishes; it counts as busy, so a press is ignored and Esc returns to idle. The worker saves every restorable clipboard format in enumeration order, skipping GDI handles and private ranges (formats 2, 3, 9, 14, 0x80, 0x82, 0x83, 0x8E, 0x200-0x3FF; `is_restorable_format`, insert.rs:62). It puts the text as `CF_UNICODETEXT` under a message-only owner window, reads `GetClipboardSequenceNumber`, and sends Ctrl+V in one `SendInput` call (Ctrl down, V down, V up, Ctrl up; no extended flag, which would be Right Ctrl, the hotkey; `paste_keys`, insert.rs:94). After 350 ms (`RESTORE_DELAY_MS`, insert.rs:5) it restores the saved formats only if the sequence number is unchanged, the same rule as the macOS change count. Password rule: refuse with "A password field is active; nothing was inserted" when the field was a password at recording start (captured) or is one at insert time (now); an unreadable flag (UIA error, elevated target) pastes anyway (`decide`, insert.rs:48). If `OpenClipboard` still fails after 5 tries 20 ms apart: "The clipboard is in use by another app; nothing was inserted". If `SendInput` accepts fewer than 4 events the worker sends the key-ups, leaves the text on the clipboard and shows done "Copied (press Ctrl+V to paste): <preview>" with status `clipboard`. Ctrl+V is used for consoles too; Shift+Insert is the fallback if a device check fails. Esc while inserting returns to idle but the worker still finishes the restore (macOS does the same since 2026-10-27).
9. Save a history record, set `done`, reset after 2.5 s (:210-221).

- Cancellation at any await returns to idle without a message (:222-223). Any other error fails with the error text (:224-225).
- The target app, window title, URL, caret text and selection are captured when recording starts, not when it ends (:72).
- Failing also clears the live preview and cancels the progressive transcriber (:229-234).

## Hallucination filter

Fixture: `shared/fixtures/hallucination-filter.json`. Source: Audio/SpeechPresence.swift:27-43.

`isJunk(text)` must:

1. Lowercase the text.
2. Return true if it contains any of the "fragments" (substring match). The list is at SpeechPresence.swift:33-35.
3. Otherwise keep only Unicode letters and ASCII spaces, collapse runs of spaces and trim, and return true if the result equals one of the "whole text" phrases (exact match). The list is at SpeechPresence.swift:29-32.

The fixture holds every entry of both lists, one case each (9 whole-text phrases and 6 fragments, each expected true), and is the canonical list. Add a phrase to the code and to the fixture together. A port must not shorten the lists; the fixture fails if it does.
