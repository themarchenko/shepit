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

- Windows only: the hovered idle shapes are 300 x 28 (flowPill), 360 x 30 (captionBar) and 280 x 30 (island), and the idle island is 128 x 26. The hint names "Right Ctrl" in Segoe UI, wider than the macOS key label, and 262, 330 and 252 clip it; the idle island has to fit the mic icon and the keycap inside the same 14 pt padding as macOS, which 96 does not. The widths are estimates until a device check.
- flowPill shows a caption above the pill while listening with text, and in processing, done and failed (WidgetModel.swift:96-102). Caption maximum width 440 (:85). With a caption the flowPill panel grows by 42 pt in height (8 pt gap plus 34 pt caption) and its width is at least 464 (440 + 24) (:137-140).
- The panel is sized to the shape plus a shadow margin of 28 x 26 on each side (:84, 132-147).
- Default position: centred horizontally on the primary display's work area, the idle pill's bottom edge 34 pt above the work area bottom (8 pt panel clamp margin plus the 26 pt shadow margin), and the panel clamped inside the work area with an 8 pt margin (WidgetController.swift:27, 196-213). Windows scales these points by the monitor scale factor.

## Platform-specific captions

| Platform | Phase | Caption | Source |
|---|---|---|---|
| Windows only | processing, model files downloading | `Downloading model N%` (N is a whole percent) | windows/app/src/Widget.tsx:7 |

macOS does not show this caption. Both platforms word the other stages the same way (`Loading model`, `Transcribing`; WidgetModel.swift:21-22).

## Windows notes

Windows only, where it differs from the macOS behaviour above.

- Anchor units: `widgetAnchor` is the bottom centre of the window in physical pixels of the virtual screen, y down. Logical points have no single meaning on a desktop with mixed scaling. The default is the centre of the primary work area, 8 px times the scale above its bottom edge. The anchor is clamped to the work area of the monitor that holds it, with an 8 pt margin. If no monitor holds it (a display was unplugged), the widget goes back to the default.
- No blur: the glass is `rgba(0, 0, 0, glassOpacity)` with the hairline and shadow, but no backdrop blur. A WebView2 `backdrop-filter` cannot sample the desktop behind the window, and Acrylic or Mica would also fill the shadow margin of the window. Confirmed by the user on 2026-10-27, reversible.
- Captions are one line. Live and processing words keep the newest end and fade the clipped start. Done and failed text keep the start and end in an ellipsis, so "Copied (press Ctrl+V to paste)" stays readable. The island body keeps two lines.
- The right-click menu (modes, language, history, settings) is not there yet. Right click does nothing. Known gap: the items need the settings pane and history, which come with tasks 8 and 10. The tray has "Show widget" and "Reset widget position".
- Formatting stage (task 9): `Stage::Formatting` runs from the accepted transcript until the final text is ready, for every route, including raw and fast path where it is short. It counts as busy (windows/core/src/dictation.rs:95, 378). The widget payload is `processing` with stage `formatting`, caption "Formatting", step 1 of the three steps (windows/app/src/captionContent.ts:19, 31; widgetState.ts:4). The island step bar "Transcribe, Format, Insert" now fills the second step in this stage. The demo shows formatting for 1.15 s between transcribing (0.75 s) and inserting (0.4 s) (app wiring: task 9 chunk C).
- Known gap: there is no screens-changed handling. A monitor that is unplugged while the widget sits on it is noticed at the next size change or drag, not at once.
- Live text since task 12 (see Live preview, Windows). The layout reads the flag.

## Appearance settings

Source: Settings/AppSettings.swift:101-103, 163-164, 170, 183-194.

| Key | Default | Meaning |
|---|---|---|
| `glassOpacity` | 0.7 | Opacity of the dark glass, range 0.5 to 0.9 |
| `fadeWhenIdle` | true | Fade the idle widget |
| `widgetVisible` | true | Show the widget |
| `widgetAnchor` | unset | `{x, y}`: middle of the bottom edge, global screen coordinates. One anchor for all displays. Windows: physical pixels, see Windows notes. |

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
- Stopping recording keeps the last text on screen; it is cleared when the result is inserted, when the recording is discarded for being shorter than the minimum (DictationSession.swift:110-114), or when the dictation is cancelled or fails (DictationSession.swift:107, 219, 230; the discard path clears it at :112).
- With the setting off the preview is cleared and not started (DictationSession.swift:91-96).
- The preview runs on the fast recognizer (Parakeet), whatever engine is selected (Speech/SpeechEngine.swift:140-146).

Fixtures: `shared/fixtures/live-preview-join.json` for the join rule, `shared/fixtures/live-preview-split.json` for the stable/unstable split (the last two words of the pass text are unstable, the rest joins the frozen text).

Windows (windows/core/src/live.rs, windows/app/src-tauri/src/recognizer.rs), where it differs from the table above. Parakeet runs on the CPU there, so the window is smaller and the load is capped:

- Freeze and tail are separate jobs. A freeze job decodes the tail up to the cut; its result is shown only together with the result of the tail job that follows it. After a freeze the tail job is sent at once, without the 1 s minimum.
- Freeze window: the tail is frozen when it is longer than 6 s (96 000 samples). The cut is the quietest 100 ms frame between 4.0 and 5.5 s of the tail, the macOS ratio. No pass covers more than about 7 s of audio.
- Cadence: the first pass at 1 s of audio. After a tail pass that took p seconds the next one waits `max(0.15, 0.5 - p, p)`, which keeps the duty at or below 50%. At most one pass is in flight, and each pass reads the newest audio when it starts.
- Slow-pass stop: if any pass takes more than 1.0 s, the preview stops for the rest of that dictation and keeps its text.
- One model and one recognizer thread: previews share the thread with the final pass. The model cannot be interrupted, so at release the final waits for at most the pass already running (up to about 0.7 s on CI-class hardware). That pass's result is dropped, and preview text never reaches the inserted text.
- The keyboard hook thread runs at `THREAD_PRIORITY_HIGHEST`, so the sustained load does not delay keystrokes or the release stamp.
- The setting is read when a recording starts. Changing it affects the next dictation.
- The text clears on done, on fail, on Esc, when a short recording is discarded and on a stream error. It stays through processing, until one of those.
- Language: no gate, see language.md.
