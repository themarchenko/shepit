# Prompt golden

`system-prompt.md` is the system prompt the macOS app sends to the polisher, built with the default
speaker notes (`SpeakerNotes.defaultText`) plus one final newline. It is a golden, not a resource: no
code loads it at runtime. `PromptGoldenTests` fails when `PromptBuilder.systemPrompt` drifts from it.
Source: `apple/ShepitCore/Sources/ShepitCore/Formatting/PromptBuilder.swift` lines 5-51.
