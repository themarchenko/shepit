# Widget

Paths are relative to `apple/` (`macOS/Shepit/Widget` for the widget, `ShepitCore/Sources/ShepitCore` for Core). The widget is a small always-on-top panel that shows the dictation state. Platform-specific: the panel, the glass look and the springs are AppKit and SwiftUI. What follows is the behaviour a port keeps.

## Phases

Source: macOS/Shepit/Widget/WidgetModel.swift:8-30; macOS/Shepit/Widget/WidgetController.swift:109-131.

| Session state | Widget phase |
|---|---|
| idle | `idle` |
| recording(hands_free) | `listening(hands_free)`, elapsed timer starts |
| loading_model | `processing(loading)` |
| transcribing | `processing(transcribing)` |
| formatting | `processing(formatting)` |
| inserting | `processing(inserting)` |
| done(preview) | `done(preview)` |
| failed(message) | `failed(message)` |

- The user sees three steps while processing: transcribing, formatting, inserting. Loading the model counts as the first step (WidgetModel.swift:28-29).
- Elapsed time shows as `m:ss` (:62-65).
- The level meter keeps the last 32 microphone levels (:34, 67-70).
- The caption chip names the app the text goes to (WidgetModel.swift:37-38; Pipeline/DictationSession.swift:62).

## Pointer input

Source: macOS/Shepit/Widget/WidgetController.swift:51-55.

- Click: toggle hands-free (DictationSession.swift:129-136).
- Press and hold: start recording on press, stop on release (not hands-free).
- Drag ends: save the new position.
- Right click: menu.
- Hover: the idle shape grows (sizes below).

## Styles

Source: Settings/AppSettings.swift:67-86. Setting `widgetStyle`, default `flowPill`.

| Id | Description |
|---|---|
| `flowPill` | A dash that springs into a pill. Words float above it. |
| `captionBar` | A wide bar that reads like subtitles. |
| `island` | One shape that grows into a card with the text and steps. |

Shape sizes in points, from macOS/Shepit/Widget/WidgetModel.swift:78-129 (width x height). These are the current macOS look; a port may scale them, but the relative growth and the phases they show are the contract.

| Phase | flowPill | captionBar | island |
|---|---|---|---|
| idle | 40 x 8 | 178 x 30 | 96 x 26 |
| idle, hovered | 262 x 28 | 330 x 30 | 252 x 30 |
| listening, push-to-talk | 132 x 32 | 560 x 42 | 270 x 36 (410 x 94 with text) |
| listening, hands-free | 166 x 32 | 560 x 42 | same as push-to-talk |
| processing | 112 x 32 | 560 x 42 | 410 x 122 |
| done | 44 x 32 | 560 x 42 | 410 x 94 |
| failed | 44 x 32 | 270 x 42 | 254 x 36 |

- flowPill shows a caption above the pill while listening with text, and in processing, done and failed (WidgetModel.swift:96-102). Caption maximum width 440 (:85).
- The panel is sized to the shape plus a shadow margin of 28 x 26 on each side (:84, 132-147).

## Appearance settings

Source: Settings/AppSettings.swift:101-103, 163-164, 170, 183-194.

| Key | Default | Meaning |
|---|---|---|
| `glassOpacity` | 0.7 | Opacity of the dark glass, range 0.5 to 0.9 |
| `fadeWhenIdle` | true | Fade the idle widget |
| `widgetVisible` | true | Show the widget |
| `widgetAnchor` | unset | `{x, y}`: middle of the bottom edge, global screen coordinates. One anchor for all displays. |

## Live preview

Source: Speech/LivePreview.swift. Setting `livePreview`, default on (AppSettings.swift:174). It shows words while the user speaks; it never decides the inserted text.

| Rule | Value | Source |
|---|---|---|
| Pass interval | 0.5 s, and at least 0.15 s of rest | :41, 96 |
| First pass needs | 16 000 samples (1 s) | :43, 66 |
| Freeze: tail longer than | 12 s | :69 |
| Freeze cut | quietest 100 ms frame between 8 s and 11 s of the tail | :70, 131-148 |
| Unstable words | the last 2 words of the current pass | :121 |
| Join of two parts | non-empty parts joined with one space | :127-129 |

- A pass decodes only the audio after the last freeze point. Frozen text is decoded once more and kept.
- The preview text has a stable part and an unstable part; the unstable part may be rewritten by the next pass (:21-25).
- Stopping recording keeps the last text on screen; it is cleared when the result is inserted or the dictation is cancelled or fails (DictationSession.swift:107, 112, 219, 230).
- With the setting off the preview is cleared and not started (DictationSession.swift:91-96).
- The preview runs on the fast recognizer (Parakeet), whatever engine is selected (Speech/SpeechEngine.swift:140-146).

Fixture for the join rule: `shared/fixtures/live-preview-join.json`.
