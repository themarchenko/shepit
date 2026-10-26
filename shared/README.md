# Shared fixtures

Language-neutral test data for behaviour that must match on macOS and Windows. The Swift runner is
`apple/ShepitCore/Tests/ShepitCoreTests/SharedFixtureTests.swift`; the Rust port reads the same files.

Every file in `fixtures/` has this envelope (keys are snake_case, `cases` is never empty):
`{ "version": 1, "description": string, "cases": [ { "name": string, "input": {...}, "expected": ... } ] }`
`name` is the old Swift test name without `test`, plus a number. Optional inputs are omitted when absent.

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
| app-version.json | `{candidate, installed}` | bool |

Non-ASCII text is stored as UTF-8; the em dash in a window title is the JSON escape `—`.
