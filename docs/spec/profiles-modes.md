# Profiles and modes

Paths are relative to `apple/ShepitCore/Sources/ShepitCore`. A profile is how text should look for one kind of target app. A mode is how much shaping the text gets.

## Modes

Source: Formatting/Profile.swift:3-15. Ids: `clean`, `prompt`, `raw`. Default `clean` (Settings/AppSettings.swift:169). A one-off override applies to the next dictation (DictationSession.swift:30, 190).

- `clean`: cleanup only.
- `prompt`: cleanup, then shape as a prompt for an AI assistant (rules in the system prompt, Formatting/PromptBuilder.swift:25-29).
- `raw`: whitespace cleanup only, no agent (local-cleanup.md).

## Mode resolution

Fixture: `shared/fixtures/mode-resolution.json`. Source: Formatting/Profile.swift:141-144.

`resolve(requested, profile, word_count, prompt_mode_min_words)` must return `prompt` if `requested` is `clean`, the profile is an AI target and `word_count >= prompt_mode_min_words`; otherwise `requested`. `raw` and `prompt` are never changed. The setting `promptModeMinWords` defaults to 40 (AppSettings.swift:172). The word count is the one in local-cleanup.md, taken from the transcript (DictationSession.swift:191-193).

## Built-in profiles

Source: Formatting/Profile.swift:37-95. The order below is the order of `BuiltInProfiles.all` and of the sections in the system prompt.

| id | name | AI target | Markdown allowed |
|---|---|---|---|
| `claude-code-terminal` | Claude Code in a terminal | yes | no |
| `terminal` | Terminal | no | no |
| `ai-chat` | AI chat app or site | yes | yes |
| `code-editor` | Code editor | yes | yes |
| `messenger` | Messenger | no | no |
| `mail` | Email | no | no |
| `notes` | Notes and documents | no | yes |
| `generic` | Other apps | no | no |

- Every built-in profile strips trailing newlines from agent output (`stripTrailingNewline` defaults to true, Profile.swift:27, 25).
- The style rule sentences are part of the system prompt and are pinned by `shared/prompts/system-prompt.md`, not repeated here.

## Profile resolution

Fixture: `shared/fixtures/profile-resolution.json`. Source: Formatting/Profile.swift:97-137. Input: bundle id, window title, URL, each optional. Checked in this order; the first match wins.

1. Bundle id is in the terminal set: `claude-code-terminal` if the lowercased window title contains "claude", else `terminal` (:117-119).
2. Bundle id is in the AI app set: `ai-chat` (:120).
3. Bundle id is in the code editor set: `code-editor` (:121).
4. Bundle id is in the messenger set: `messenger` (:122).
5. Bundle id is in the mail app set: `mail` (:123).
6. Bundle id is in the notes app set: `notes` (:124).
7. The URL has a host (lowercased, a leading "www." removed, :133-136). Then: host equals an AI host or is a subdomain of one: `ai-chat`; host equals a mail host: `mail`; host equals a document host or is a subdomain of one: `notes` (:125-129).
8. Otherwise `generic` (:130).

Missing bundle id is treated as empty, missing window title as empty (:115-116). The lists:

| Set | Entries | Source |
|---|---|---|
| terminal bundle ids | 7 | Profile.swift:98-101 |
| AI app bundle ids | 3 | :102 |
| AI hosts | 6 | :103 |
| code editor bundle ids | 4 | :104 |
| messenger bundle ids | 7 | :105-108 |
| mail app bundle ids | 3 | :109 |
| notes app bundle ids | 6 | :110 |
| mail hosts | 3 | :111 |
| document hosts | 2 | :112 |

The fixture `shared/fixtures/profile-resolution.json` holds every bundle id and every host above, one case each, and is the canonical list. Add an entry to the code and to the fixture together; a port with a truncated set fails. Platform-specific: bundle ids are macOS identifiers. A port keeps the host rules and the order, and needs its own app identifiers (README, open points).
