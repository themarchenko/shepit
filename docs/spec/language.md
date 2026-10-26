# Language

Paths are relative to `apple/ShepitCore/Sources/ShepitCore`. The speaker uses Ukrainian and English, often in one sentence.

## Setting

- `languageLock` is `auto` (default) or a language code (Settings/AppSettings.swift:173). `auto` means no lock.
- At the start of a recording the recognizer gets `language = nil` for `auto`, else the lock (Pipeline/DictationSession.swift:85). The live preview gets the same language (:93).
- The formatter's language hint is the lock if set, else the language the recognizer reported (DictationSession.swift:194). A missing language is sent to the formatter as `auto` (Formatting/PromptBuilder.swift:58).
- The history record stores the formatter's language if it gave one, else the recognizer's (DictationSession.swift:213).

## Whisper retry

Fixture: `shared/fixtures/retry-language.json`. Source: Speech/WhisperKitEngine.swift:37-61. Applies to the Whisper engine only, and only when no language is locked.

`retryLanguage(detected, text)` must return:

- `null` if `detected` is null, or is `uk` or `en`;
- otherwise `uk` if the text contains any character in U+0400 to U+04FF (inclusive), else `en`.

When it returns a language, the engine must decode the same audio again with that language fixed and report that language (WhisperKitEngine.swift:40-44). Why: Whisper's language id picks from about 100 languages and sometimes hears accented Ukrainian as Russian (:54-56).

- Whisper joins its segments by trimming each, dropping empty ones and joining with one space (:63-66).
- Decoding uses the transcribe task, timestamps off, prefill prompt on, and language detection on only when no language is given (:68-73).

## Live preview language

Source: Speech/LivePreview.swift:45-59, 74-92. With a lock, every pass uses it. Without one, each pass before 3.5 s of audio decodes as `uk` and as `en`; the pass is shown only if the better confidence is at least 0.6 and the two differ by at least 0.2. From 3.5 s of audio on, the more confident language (`uk` on a tie) is kept for the rest of the recording. See widget.md for the pass timing.
