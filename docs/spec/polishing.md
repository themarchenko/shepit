# Polishing

Paths are relative to `apple/ShepitCore/Sources/ShepitCore` unless marked `App/` (`macOS/Shepit/App`). A polishing agent is an external CLI that rewrites the transcript. Two exist, Claude Code and Codex.

## Provider setting

Source: Settings/AppSettings.swift:34-64, 158-160.

| Id | Meaning |
|---|---|
| `claude` (default) | Claude Code CLI, executable name `claude` |
| `codex` | Codex CLI, executable name `codex` |
| `off` | No agent. Text is inserted as heard, with local cleanup and the dictionary (local-cleanup.md). |

- Old setting `formatterEnabled` (before 0.2): used only when `polishing` is unset; true or missing gives `claude`, false gives `off` (AppSettings.swift:158-160).
- The agent is found from the configured path (must be an executable file, else not found), or else the first existing file among: `~/.local/bin/<name>`, `/opt/homebrew/bin/<name>`, `/usr/local/bin/<name>`, `~/.npm-global/bin/<name>`, `~/.bun/bin/<name>` (AppSettings.swift:57-64). Platform-specific: the search locations.
- If the chosen agent's executable is not found, polishing is off. The other agent is never tried (App/AppDelegate.swift:131-146).
- Claude model: `haiku` (default) or `sonnet` (AppSettings.swift:26-31, 167). Codex gets an empty model, which means the user's own Codex default (AppDelegate.swift:147-148; the `gpt-6-luna` default at CodexFormatter.swift:6 is not used by the app).
- Both agents get the same system prompt, built once from the speaker notes (AppDelegate.swift:130). The agent is replaced 1.5 s after the last change to provider, Claude model, speaker notes or either executable path: the old one is shut down, the new one is warmed up (App/AppDelegate.swift:170-188).
- Both run in an empty working directory under the app support folder so they find no project files (AppSettings.swift:221-232).

## System prompt

Golden: `shared/prompts/system-prompt.md`, the exact output of `systemPrompt(speakerNotes: defaultText)` plus a final newline. Source: Formatting/PromptBuilder.swift:5-51.

- Order: fixed instructions, then one section per built-in profile in `all` order with its Markdown line and style rules (:36-40), then the speaker section only if the notes are non-empty after trimming (:41-43), then the output section (:44-49).
- Default speaker notes: Settings/AppSettings.swift:247-251.
- The prompt names macOS (PromptBuilder.swift:7). That is open for Windows (README, open points).
- The agent must return only a JSON object with `text`, `language` (`uk`, `en` or `mixed`) and `new_terms` (PromptBuilder.swift:48).

## Message per dictation

Source: Formatting/PromptBuilder.swift:54-81. Not covered by a fixture yet.

```
Context:
<JSON object, keys sorted, no pretty printing>

Dictionary (preferred spellings): a, b, c      (only when the dictionary is not empty)

Recent accepted examples for this profile:      (only when examples exist)
Transcript: <raw>
Final: <final>

Transcript:
<transcript>
```

Context keys, in the JSON object:

| Key | When | Limit |
|---|---|---|
| `profile` | always | profile id |
| `mode` | always | mode id |
| `language_hint` | always | the hint, or `auto` |
| `app` | app name known | |
| `bundle_id` | known | |
| `window_title` | known | first 200 characters |
| `url` | known | |
| `text_before_caret` | known and not empty | last 500 characters |
| `selected_text` | known and not empty | first 2000 characters |

The dictionary part takes at most the last 120 entries (the newest). At most 5 examples; the code never passes any (Pipeline/DictationSession.swift:195-196, README, Gaps).

The target context fields are bundle id, app name, window title, URL, text before the caret, selected text, editable focus and secure input (Context/TargetContext.swift:4-12). When the focused field is secure (secure event input is on, or the field has the secure text subrole) no text is read: text before the caret and selected text stay empty, while the app name and window title are still captured (ContextProbe.mayReadText).

Windows field mapping (windows/core/src/context.rs, windows/app/src-tauri/src/insert_win.rs; formatting itself is Windows task 9):

| Field | Windows source |
|---|---|
| `bundle_id` | exe file name of the foreground process (`QueryFullProcessImageNameW`); `ApplicationFrameHost.exe` resolves to its `Windows.UI.Core.CoreWindow` child process |
| `app` | `FileDescription` from the exe version info, else the exe name without `.exe` |
| `window_title` | `GetWindowTextW`, up to 1024 UTF-16 units |
| `text_before_caret`, `selected_text` | UI Automation on the focused element: `TextPattern2` caret, else the start of the `TextPattern` selection, else `ValuePattern`; 1 s connection and transaction timeouts |
| `url` | not captured on either platform (macOS never sets it, ContextProbe.swift:18-48) |

Limits are in graphemes: the last 500 before the caret; the field value is read only under 50000 (`MAX_VALUE_GRAPHEMES`, context.rs:8); the selection is dropped when empty or at 10000 or more (`MAX_SELECTION_GRAPHEMES`, :9), as in ContextProbe.swift:35-44. The 200 and 2000 character cuts apply when the prompt is built. Text is read only when the password flag was read and is false (`may_read_text`, context.rs:44); a password field or an unreadable flag yields no text. The `TextPattern` path gives at most 1000 characters before the caret, so the 50000 limit only bites on the `ValuePattern` path.

## Reply handling

- The reply is trimmed; if it then starts with a code fence, every ```` ```json ```` and ```` ``` ```` in it is removed. The candidate is the text from the first `{` to the last `}`, and the first `{` must come before the last `}` (Formatting/Formatter.swift:75-79). The JSON must have a string `text`. `language` and `new_terms` are optional (PromptBuilder.swift:83-88; Formatting/Formatter.swift:73-80). If the first parse fails, the same object text is tried again with every raw newline and tab replaced by `\n` and `\t`, because replies sometimes hold raw line breaks inside strings (Formatting/Formatter.swift:75-93; the retry is at :86-92). No JSON object or no `text` after that: the dictation counts as a failure and falls back to local text (local-cleanup.md).
- Cleanup of the agent text, the same for both agents: trim newlines at both ends; if the profile strips trailing newlines, remove trailing newlines again; if the first character is a lowercase letter, uppercase it (CodexFormatter.swift:117-127; the Claude copy is Formatting/ClaudeCodeWorker.swift:218-225).

## Privacy rules

- Audio never leaves the device. Recognition is local; only text goes to an agent.
- An agent receives the transcript, the dictionary part, the optional examples and the context keys listed above, and nothing else.
- Dictated text is never logged at public privacy level. The one log of transcript text is marked private and cut to 80 characters (Pipeline/DictationSession.swift:186). A port must keep transcripts, results and context text out of public logs.
- A secure captured field (password field or secure event input) skips the agent: the result is the local text with source `passthrough`, and the transcript is not sent anywhere. Error logs from the agents carry the error kind and the reply byte length only, never reply or server text.

## Process lifetime (both agents)

| Rule | Value | Source |
|---|---|---|
| Request timeout | 25 s | ClaudeCodeWorker.swift:12; CodexFormatter.swift:19 |
| Warm-up timeout (start, handshake, first turn) | 40 s | ClaudeCodeWorker.swift:13; CodexFormatter.swift:21 |
| Turns per conversation before it is replaced | 25 | ClaudeCodeWorker.swift:10; CodexFormatter.swift:17 |
| A standby conversation is prepared after | 20 turns | ClaudeCodeWorker.swift:11; CodexFormatter.swift:18 |
| Consecutive start failures before a pause | 2 | ClaudeCodeWorker.swift:259; CodexFormatter.swift:174 |
| Pause length | 300 s (5 min) | ClaudeCodeWorker.swift:260; CodexFormatter.swift:175 |
| Effort | `low` | ClaudeCodeWorker.swift:7; CodexFormatter.swift:11, 23 |

- A successful start resets the failure count and the pause (ClaudeCodeWorker.swift:265-266; CodexFormatter.swift:165-166).
- While paused, a format call fails at once and the dictation uses local text (ClaudeCodeWorker.swift:250-252; CodexFormatter.swift:141).
- Error handling differs. Claude: any send error (timeout, exit) terminates the process (ClaudeCodeWorker.swift:208-214). Codex: a failure reported by Codex itself (`turnFailed`, for example a usage limit) keeps the process; any other error drops it (CodexFormatter.swift:97-105).
- Codex requests run one at a time (CodexFormatter.swift:85-87).
- Platform-specific: how the CLIs are driven (Claude stream-json process, Codex app-server JSON lines) is covered by Swift tests only. A port must match the timings and the cleanup above, not the transport.
