# Language

Paths are relative to `apple/ShepitCore/Sources/ShepitCore`. The speaker uses Ukrainian and English, often in one sentence, and never Russian or Polish. Windows lags on all of this (end of file).

## Setting

- The setting is an allowed set, `LanguagePolicy` (Speech/LanguagePolicy.swift:6-9): `uk+en` (default, "Ukrainian + English"), `uk` ("Ukrainian"), `en` ("English"). It is stored under `languageLock` as that raw value (Settings/AppSettings.swift:114, 179).
- Migration: a missing, empty, `auto` or unknown stored value reads as `uk+en`. Nothing is written back on read, so a downgrade still sees the old value.
- `allowed` lists the codes, `uk` first (LanguagePolicy.swift:17-22). Russian and Polish are never allowed under any policy.
- Hint to the recognizers: `uk` or `en` for a single-language policy, nil for `uk+en` (LanguagePolicy.swift:26; Pipeline/DictationSession.swift:110 for the recording, :118 for the live preview). A nil hint means the recognizer chooses, and the rules below keep the result inside the set.
- The formatter's language hint is the single language of the policy, else the language the recognizer reported (LanguagePolicy.swift:47; DictationSession.swift:223). A missing language is sent to the formatter as `auto` (Formatting/PromptBuilder.swift:59).
- The history record stores the formatter's language if it gave one, else the recognizer's (DictationSession.swift:243).
- The agent is also told that Russian or Polish words are recognition errors (polishing.md, System prompt).

## Choosing between allowed languages

`pick(fromProbabilities:)` (LanguagePolicy.swift:36): a single-language policy returns its language. Otherwise it takes the allowed code with the highest probability, `uk` winning a tie, and returns nil if no allowed code is in the map.

## Whisper

Fixture: `shared/fixtures/retry-language.json`, read by the two-argument `retryLanguage(detected, text)` (Speech/WhisperKitEngine.swift:69-72). It must return:

- `null` if `detected` is null, or is `uk` or `en`;
- otherwise `uk` if the text contains any character in U+0400 to U+04FF (inclusive), else `en` (`scriptLanguage`, :74).

The macOS engine runs detection when the hint is nil (:39-60). When the detected language is outside the policy:

1. If the first pass is Cyrillic it retries as Ukrainian without measuring (`needsProbe`). Otherwise it measures real Ukrainian and English probabilities (:98-111). WhisperKit's own result keeps only the winner's probability, so this repeats its detection step with a sampler that reads the `<|uk|>` and `<|en|>` logits and takes a softmax over the two (:89-95). It costs one extra encoder pass and runs only on this path. A failure means no probabilities.
2. `retryLanguage(detected, text, probabilities, policy)` (:81-86) picks the language: Ukrainian for a Cyrillic first pass; otherwise from the probabilities if they hold an allowed code, else by the script rule. If the result is still outside the policy it is the first allowed code. It returns nil when detection is nil or already allowed.
3. The engine decodes the same audio again with that language fixed and reports it.

Why: Whisper's language id picks from about 100 languages and sometimes hears accented Ukrainian as Russian or as Polish. Script alone cannot help when the first pass was Latin Polish, so the probabilities decide there. They are not used for a Cyrillic first pass: Whisper's uk/en logits lean English, and on a real Ukrainian recording heard as Russian they picked English, so the retry translated the speech into English.

- Whisper joins its segments by trimming each, dropping empty ones and joining with one space (:114).
- Decoding uses the transcribe task, timestamps off, prefill prompt on, and language detection on only when no language is given (:119).

## Parakeet

Source: Speech/ParakeetEngine.swift:29-43. The hint is passed only for a single-language policy, because it filters decoder candidates by script and would force identifiers into Cyrillic in mixed speech. Under `uk+en` the first pass has no hint. If its text has Polish-only letters (ą ć ę ł ń ś ź ż and capitals; ó is excluded, it appears in loanwords; Speech/ScriptCleanup.swift:14), the engine decodes again as `uk` and as `en`, each from a fresh decoder state, and keeps the most confident of the first pass and the two re-decodes (a missing confidence counts as 0; a tie keeps the first pass, then `uk`). If every re-decode fails the first pass stays (ScriptCleanup.swift:28-36). Single-language policies never re-decode. The reported language is nil.

## Final text letter map

Russian-only letters ы, э, ъ, ё map to и, е, the modifier apostrophe U+02BC (the Ukrainian apostrophe letter, kept over U+0027 on purpose) and е, capitals likewise (ScriptCleanup.swift:7). The session applies the map to the final transcript of every engine before the empty and junk check (DictationSession.swift:208); only the text changes. History `rawText` stores the mapped text.

## Progressive Whisper

Pieces of a long dictation adopt the language Whisper reports for the first piece only if the policy allows it (Speech/ProgressiveTranscriber.swift:106). An outside language is never adopted, so later pieces keep detecting.

## Live preview language

Source: Speech/LivePreview.swift:70-78, 121-138. With a single-language policy, every pass uses it. With `uk+en`, each pass before 3.5 s of audio decodes as `uk` and as `en`; the pass is shown only if the better confidence is at least 0.6 and the two differ by at least 0.2. From 3.5 s of audio on, the more confident language (`uk` on a tie) is kept for the rest of the recording. See widget.md for the pass timing.

Preview text always gets the letter map above (LivePreview.swift:157); Russian is never allowed, so there is no language gate. A Whisper head uses its own language for its text.

## Windows (known gaps)

Windows has no language setting, no allowed-set rule, no Polish re-decode and no letter map; its formatter hint is always `auto`. The Windows prompt carries the Russian and Polish line (polishing.md). Windows has no live-preview language gate: Parakeet there takes no language hint and returns no confidence, so each pass decodes once with the model's own detection. Ukrainian may show in the wrong script for the first seconds. macOS keeps the gate.
