# Local cleanup and routing

Paths are relative to `apple/ShepitCore/Sources/ShepitCore`. Decides, per dictation, whether the polishing agent runs.

## Routing

Source: Formatting/FormattingService.swift:21-39. Checked in this order; the first match wins.

| # | Condition | Text | Reported source |
|---|---|---|---|
| 1 | mode is `raw` | `clean(transcript)`. No dictionary correction. | `passthrough` |
| 2 | mode is `clean`, the profile is not an AI target and word count <= fast-path limit | `local(transcript)` | `fastPath` |
| 3 | no agent is available, or the target field is secure (see below) | `local(transcript)` | `passthrough` |
| 4 | the agent throws (timeout, exit, bad reply, pause) | `local(transcript)` | `fallback` (with the error text as a note) |
| 5 | otherwise | the agent's result | `claude` or `codex` |

Fixture: `shared/fixtures/polish-routing.json` (`FormattingService.format` with a stub agent). The source ids are the code's `FormatterSource` raw values: `passthrough`, `fastPath`, `fallback`, `claude`, `codex`.

- Fast-path limit: setting `fastPathMaxWords`, default 8 words, negative values become 0 (FormattingService.swift:7, 17-19; Settings/AppSettings.swift:171). The comparison is `words <= limit`.
- `local(t)` = `clean(DictionaryCorrector.correct(t, dictionary))` (FormattingService.swift:41-44). The dictionary is the whole saved dictionary (Pipeline/DictationSession.swift:196).
- Mode `prompt` always goes to the agent when one exists; row 2 is for `clean` only.
- Secure field (decision of 2026-10-27): when the captured context says the field is secure (macOS `secureInput`, Windows password flag true), the agent is skipped and the local text is used, so a dictation into a password field never leaves the device. Raw mode and the fast path already stay local and keep their own source. The insert still refuses a secure field. The fixture input `secure` is false when missing.
- There is no fallback from one agent to the other (see polishing.md).
- Plan intent that the code lacks: README, Gaps.

## clean

Fixture: `shared/fixtures/passthrough-clean.json`. Source: Formatting/Formatter.swift:66-70.

`clean(text)` must: split on any run of whitespace or newlines, join with single spaces (so leading and trailing space disappear), and uppercase the first character of the result. The rest is unchanged. Empty input gives the empty string. Only the first character is uppercased, using the platform's Unicode uppercase mapping for that character.

## Word count

Fixture: `shared/fixtures/word-count.json`. Source: Formatting/Profile.swift:146-148.

`wordCount(text)` must count the pieces left after splitting on whitespace and newlines, ignoring empty pieces. Punctuation stays attached to words; "a - b" counts 3.
