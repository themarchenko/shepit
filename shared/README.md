# Shared fixtures

Language-neutral test data for behaviour that must match on macOS and Windows. The Swift runner is
`apple/ShepitCore/Tests/ShepitCoreTests/SharedFixtureTests.swift`; the Rust runner is
`windows/core/tests/shared_fixtures.rs`; both read the same files.

Every file in `fixtures/` has this envelope (keys are snake_case, `cases` is never empty):
`{ "version": 1, "description": string, "cases": [ { "name": string, "input": {...}, "expected": ... } ] }`
`name` is a group and a number, such as `Name/3` or `Name/5_Suffix`, usually the old Swift test name without `test`. Optional inputs are omitted when absent.

| File | input | expected |
|---|---|---|
| dictionary-correction.json | `{text, dictionary:[string]}` | corrected string |
| dictionary-learning.json | `{reported:[string], existing:[string]}` | `[string]` of new entries |
| hotkey-gestures.json | `{events:[{key:"down"\|"up", at}]}`, `at` in seconds from the first event | one `{output, duration?}` per event; `output` is `none`, `press_began`, `press_ended`, `short_tap` or `double_tap`; `duration` (seconds, within 0.001) only on `press_ended` |
| passthrough-clean.json | `{text}` | string |
| word-count.json | `{text}` | integer |
| profile-resolution.json | `{bundle_id?, window_title?, url?}` | profile id string |
| mode-resolution.json | `{requested, profile, word_count, prompt_mode_min_words}`; modes are `raw`, `clean`, `prompt`; `profile` is a profile id | mode string |
| hallucination-filter.json | `{text}` | bool |
| retry-language.json | `{detected: string or null, text}` | language string or null |
| live-preview-join.json | `{a, b}` | string |
| live-preview-split.json | `{frozen, current}` | `{stable, unstable}`; the last two words of `current` are unstable, the rest is joined to `frozen` |
| app-version.json | `{candidate, installed}` | bool |
| prompt-message.json | `{transcript, language_hint?, mode, profile, context{app?, bundle_id?, window_title?, url?, text_before_caret?, selected_text?}, dictionary?:[string], examples?:[{raw, final}]}` | the user message string (Swift output, Foundation JSON escaping) |
| reply-parse.json | `{reply}` | `{text, language: string or null, new_terms:[string]}`, or null when the reply has no usable object |
| reply-finish.json | `{text, profile}` | cleaned string |
| polish-routing.json | `{mode, profile, transcript, fast_path_max_words, dictionary, agent:"none"\|"reply"\|"error", agent_text?, secure?}`; `secure` is false when missing; `agent_text` is the final text the agent layer returns | `{source, text}`; `source` is `passthrough`, `fastPath`, `fallback` or `claude` |

## Audio

`audio/` holds short speech clips for recognizer tests (WAV, 16 kHz, mono, 16-bit) and
`audio/manifest.json` with `{file, language, text, must_contain, source, license}` per clip. Only the
Rust runner reads it (`windows/engine/tests/transcribe_fixture.rs`); the Swift side has no runner yet.
It is English only for now; a Ukrainian clip follows. See `audio/README.md`.

Non-ASCII text is stored as UTF-8; the em dash in a window title is the JSON escape text `\u2014`.
