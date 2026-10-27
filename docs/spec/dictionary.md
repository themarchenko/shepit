# Dictionary

Paths are relative to `apple/ShepitCore/Sources/ShepitCore`. The dictionary is a list of strings in the settings, in the order they were added (Settings/AppSettings.swift:116, 175). Duplicates are not removed by the settings itself.

## Where the dictionary is used

| Use | Which entries | Source |
|---|---|---|
| Recognizer vocabulary | The last 30 entries (the newest). Why: Whisper decodes its prompt token by token and a long list slows every dictation. | Pipeline/DictationSession.swift:81-86 |
| Local correction (fast path, no agent, agent error) | All entries | Formatting/FormattingService.swift:41-44 |
| Agent prompt | The last 120 entries (the newest) as "Dictionary (preferred spellings): a, b, c" | Formatting/PromptBuilder.swift:70-74 |
| Learning | Entries the agent reports (below) | Learning/DictionaryCorrector.swift:226-243 |

Windows: there is no recognizer vocabulary. Parakeet takes none on either platform (Speech/ParakeetEngine.swift:36; windows/engine/src/recognizer.rs:22) and Windows has no Whisper yet, so entries drive local correction (all) and the agent prompt (last 120) only. The Dictionary pane footer leaves out the Whisper sentence.

Both slices take the newest entries (last 120 for the prompt, last 30 for the recognizer). Learned terms are appended, so a prompt that took the first 120 would never show the agent a term learned past that point (docs/DECISIONS.md, 2026-10-27).

## Learning

Fixture: `shared/fixtures/dictionary-learning.json`. Source: Learning/DictionaryCorrector.swift:226-243.

`newEntries(reported, existing)` must go through `reported` in order and keep a term only if all of these hold:

1. After trimming whitespace its length is 2 to 40 characters (inclusive).
2. Its lowercase form is not already in `existing` or earlier in this call (case-insensitive duplicate check).
3. Every character is ASCII, and it contains a letter.
4. It has at most 3 space-separated words.
5. It contains an uppercase letter, a digit, a "." or an "_".

Then return the kept terms (trimmed, original case), limited to `300 - existing.count` entries (never negative). The dictionary maximum is 300 (:227). Why the strictness: the agent's list is noisy (:223-225).

After each dictation the session appends the new entries to the dictionary (DictationSession.swift:202-206).

Windows: the agent's `new_terms` are the only source, as on macOS; nothing reads the target field after the paste. `new_entries` (windows/core/src/dictionary.rs:399-425) runs against the current list inside the settings store's lock and appends the result, so `settings-changed` reaches an open Dictionary pane (windows/app/src-tauri/src/settings_store.rs, `learned` and `learn`). The pane replaces its text only when the terms in it differ from the incoming list, so a learned term shows up without undoing typing. The 300 cap applies to learning only: `normalize_terms` (:431) does not cap hand-entered terms (settings.md). The learning cases in the fixture also run in the Rust test `windows/core/tests/shared_fixtures.rs`.

## Correction without a language model

Fixture: `shared/fixtures/dictionary-correction.json`. Source: Learning/DictionaryCorrector.swift:14-220.

The fixture pins `correct(text, dictionary)`. The sound key and the spoken parts are not public in Swift and have no fixtures of their own; the algorithm below is the reference, and a port must give the same `correct` output for every fixture case.

**Terms.** A dictionary entry is usable only if its sound key has at least 5 characters (:75). The sound key of a term is the concatenation of the keys of its spoken parts (:74).

**Spoken parts** (:143-162). Split on every character that is not a letter or digit, and also between a lowercase letter and a following uppercase letter. "handleSubmit" has 2 parts, "UserService.swift" has 3.

**Tokens** (:122-141). A token is a run of letters and digits. An apostrophe (', U+2019, U+02BC) between a token character and a following letter stays inside the token.

**Sound key of one word** (:169-197):

1. Lowercase, then transliterate each Cyrillic letter using the table at :206-212; apostrophes and the soft and hard signs map to nothing.
2. If the original word is all ASCII, drop a final "e" when the word has more than 3 characters and the character before it is not a vowel (a e i o u y) (:199-204).
3. Replace these sequences, in this order: shch, sch, tch, dzh, sh, ch, zh to J; ph to F; th to T; kh to K; ck to K; qu to KF; ts to S; x to KS; ce to Se; ci to Si; cy to Sy (:172-177).
4. Map each remaining character: a e i o u y to A; b p to P; f v w to F; g h k c q K to K; d t T to T; s z S to S; j J to J; F to F; l m n r to their uppercase; digits stay; anything else is dropped (:181-192).
5. Skip a character equal to the last key character, so repeats collapse (:193).

**Similarity of two keys** (:97-102). Both keys must have at least 5 characters. Equal keys score 1. Otherwise both must have at least 7 characters and an edit distance (Levenshtein) of at most 1; the score is 1 - 1/(the longer length). Anything else: no match.

**Matching** (:14-60):

1. For each term, slide windows of 1 to `min(token count, term parts)` consecutive tokens over the text.
2. A window is protected if its tokens joined and reduced to lowercase letters and digits equal the term reduced the same way. Protected tokens never take part in a correction (:30-34). This keeps "handleSubmit" as written.
3. For unprotected windows: a term with fewer than 2 parts is skipped when every token of the window is ASCII (so a single English word is never "corrected") (:39).
4. A window whose key (the window's token keys joined) matches the term's key by the similarity above is a candidate.
5. Sort candidates by score, then by window length, both descending. Pick greedily, skipping windows that overlap a picked one (:45-52).
6. Replace each picked window, from the end of the text to the start, with the term as written in the dictionary. Text between and around windows, including whitespace and punctuation between tokens inside a window, is replaced along with the window (:54-58).

No match, an empty text or an empty dictionary: return the text unchanged.
