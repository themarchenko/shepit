# Voice calibration (macOS only)

A wizard that records the user reading fixed words and phrases, compares them with what the recognizer heard, and asks the polishing agent to write speaker notes and suggest dictionary terms. macOS only; Windows lags (see Windows below). Code: `ShepitCore/Sources/ShepitCore/Calibration/` (script, flow, messages, offer, agents), the capture path in `Pipeline/DictationSession.swift`, and `macOS/Shepit/Calibration/CalibrationView.swift` (window). Paths are relative to `apple/`.

## Flow

Stages, in order: intro, words, phrases, picking sentences (agent round 1), a few more sentences, writing notes (agent round 2), review. A failed round 2 shows a failed stage with Try again.

- Intro: the user picks the languages (the same set as the Language setting; Start writes it) and presses Start.
- Words, phrases and the extra sentences: one prompt at a time. The user holds the dictation key or an on-screen button and reads it. Next moves on, Skip (no take yet) moves on without one, Again drops the take, Back steps to the previous prompt or the intro. Back is not offered from the first extra sentence, because those were picked from the earlier takes.
- Picking sentences: round 1 runs after the last phrase. It always ends in 2 or 3 sentences: the agent's, or the fixed fallback when the agent fails, returns a bad reply, or there are no takes to look at. The window says which one was used.
- Writing notes: round 2 sends every recorded take, extra sentences included. Fewer than 3 takes: no agent call, the failed stage says to read at least three items.
- Review: the notes are an editable text; the suggested terms are checkboxes, checked by default. Save writes the trimmed notes to `speakerNotes` and merges the checked terms into `dictionary`. Cancel or Close changes nothing.

Takes are kept per prompt, so going back and forth keeps what was read. A take belongs to the prompt that was on screen when the key went down; one that finishes after the user moved on is dropped. Back, Again and Next/Skip are disabled while a take is being recorded or checked. Nothing is written to history, the recordings folder or the dictionary until Save.

While a reading step is open every dictation is captured as a take, wherever the focus was. The window therefore comes to the front each time a reading step starts. The review tip does not show while the window is open.

## Reading lists

By language policy (`LanguagePolicy`, language.md). Prompts have stable ids; no list holds Russian-only or Polish letters.

| Policy | Words (8) | Phrases (5) |
|---|---|---|
| Ukrainian + English | 6 Ukrainian, then 2 English | 2 Ukrainian technical, 2 Ukrainian plain, 1 English |
| Ukrainian | 8 Ukrainian | 2 technical, 3 plain |
| English | 8 English | 5 English |

Fallback sentences (3): 2 Ukrainian and 1 English under Ukrainian + English, 3 of the one language otherwise. The lists are in `Calibration/CalibrationScript.swift`. They mix technical terms, plain Ukrainian with r and l, the apostrophe, and English words with sounds Ukrainian speakers often miss.

## Agent rounds

Both rounds go to the configured polishing agent (Claude Code or Codex), one fresh process per round. The system prompt is `CalibrationMessages.systemPrompt`; it is separate from the polishing prompt and has no golden.

Request (one JSON object, keys sorted, no escaped slashes):

| Key | Value |
|---|---|
| `round` | `targets` or `profile` |
| `languages` | the policy's codes, `["uk","en"]`, `["uk"]` or `["en"]` |
| `takes` | array of `{reference, heard}` plus `raw` only when it differs from `heard` |

`reference` is the prompt text. `heard` is the recognizer text after the Russian-letter map and the junk filter (empty when nothing was recognized or the filter dropped it). `raw` is the engine output before that cleanup. Round 1 sends the words and phrases; round 2 sends those and the extra sentences. Only recorded prompts are sent, in the order the user saw them.

Reply, round `targets`: `{"sentences": [string]}`. Round `profile`: `{"notes": string, "terms": [string]}`. The first JSON object in the text is used (a fence or surrounding prose is tolerated, as in polishing.md, Reply handling). Codex gets the matching strict JSON schema (all keys required, no extra keys).

### Reply filters

Sentences: each is trimmed; dropped when empty, longer than 200 characters, containing a Russian-only letter (ы э ъ ё) or a Polish letter, or containing Cyrillic under an English-only policy. At most 3 are kept. A reply that is not an object, whose `sentences` is not an array, or has a non-string item, is bad. No sentence left: bad. A bad reply means the fixed fallback.

Notes: required, trimmed, not empty and at most 2000 characters; else the reply is bad (the failed stage, Try again).

Terms: optional; non-strings skipped. A term is kept when its trimmed length is 2 to 40, it holds a letter, has no control character (a newline would reach the polishing prompt), has at most 3 words, has no Russian-only or Polish letters, and is not a case-insensitive duplicate of an earlier term or of an entry already in the dictionary. At most 12 are kept.

### Dictionary merge

Checked terms are appended in order to the existing dictionary, skipping case-insensitive duplicates, and stop at `DictionaryLearning.maxTerms` (300, dictionary.md). Existing entries are never reordered or removed.

## Auto-open

The wizard opens by itself once, then never again on its own (`calibrationOffered` is set when it opens). It opens only when all of these hold:

- `calibrationOffered` is false;
- onboarding is completed and the tour window is not open;
- Doctor did not open itself in this launch and has no required problem (doctor.md);
- the speaker notes are unset: empty or whitespace only, or equal (trimmed) to `SpeakerNotes.legacyDefault`;
- an agent is available (polishing is not off and the chosen executable was found).

Call sites: the returning-user launch path and the end of the tour. When What's new is shown there, the offer waits until its card closes, so the two never stack. Settings, Polishing, "Build from my voice" opens it at any time; that button is disabled while polishing is off. Existing notes are not changed by the offer; Save replaces them with the reviewed text.

## Privacy

- Audio never leaves the device and is never sent or stored by calibration.
- The agent receives the reference phrases, what the recognizer heard (and `raw` when it differs) and the language codes. It never receives history, the dictionary, context (app, window, URL, caret text, selection) or the current speaker notes.
- Logs carry the error kind only (`CalibrationCoach.failureKind`), never reply or take text.
- The polishing privacy rules (polishing.md) otherwise apply.

## Timeouts and process lifecycle

| Rule | Value |
|---|---|
| Request timeout, each round, both agents | 90 s |
| Effort | `medium` |
| Processes | one new process per round, ended when the call returns (also on error or timeout) |
| Work directory | a new empty directory under the system temporary directory, `shepit-calibration-<uuid>`, removed after the call; never the formatter worker directories |
| Claude arguments | the same arguments and safety flags as the formatter worker (no tools, settings, MCP or session file; no shell) |
| Codex | `app-server` with a new thread and the round's output schema; an empty model, which means the user's own Codex default |

The calibration agent never touches the formatter workers, their conversation count or their failure pause.

## Capture path

While the wizard is on a reading stage it sets `DictationSession.captureSink`. Rules:

- The route is fixed when the recording starts (`captureTarget`). A press that began as a calibration reading ends as one, even if the window closes meanwhile; a press that began as a dictation never reaches the sink.
- A reading never touches the pipeline: no recording file, context probe, formatting, learning, paste, review or history row. The state returns to idle.
- Recordings under 0.4 s are discarded (no take). Audio without 0.3 s of speech gives an empty take and no engine call.
- Recognition uses the language policy's hint and the last 30 dictionary terms, like a dictation. The take holds `raw` (engine text), `cleaned` (after the Russian-letter map; empty when the hallucination filter flags it as junk) and the language.
- The sink is cleared when the wizard leaves the reading stages or closes; closing also stops an agent round and an in-progress recording.

## Windows

Not built. The Windows app has no wizard and keeps the one-sentence speaker-notes default (settings.md). It can adopt this spec later: the request, reply and filter rules above are platform-neutral, and the reading lists are plain text.
