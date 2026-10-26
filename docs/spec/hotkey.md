# Hotkey

Paths are relative to `apple/`. `Core` = `ShepitCore/Sources/ShepitCore`, `App` = `macOS/Shepit/App`.

## Gesture recognizer

One key, two events (down, up). The recognizer is pure: it takes timestamps and returns one output per event.
Fixture: `shared/fixtures/hotkey-gestures.json`.

| Constant | Value | Source |
|---|---|---|
| Short tap maximum | 0.25 s | Core/Hotkey/HotkeyGesture.swift:14 |
| Double tap window | 0.45 s | Core/Hotkey/HotkeyGesture.swift:15 |

Rules:

- Key down while the key is already down must return `none` and change nothing (HotkeyGesture.swift:26).
- Key down while up must return `press_began` and remember the time (:27-28).
- Key up while up must return `none` (:32).
- Key up after a press of at most 0.25 s (inclusive) is a short tap (:35). If the previous short tap ended at most 0.45 s (inclusive) before this key up, the output is `double_tap` and the stored tap time is cleared; otherwise `short_tap` and this key up time is stored (:36-41). The window is measured from key up to key up.
- Key up after a press longer than 0.25 s must return `press_ended` with the press duration and clear the stored tap time (:43-44).
- A third quick tap after a double tap starts a new pair: `double_tap` clears the stored time, so the next tap is a `short_tap`.

## Keys

The user picks one key. Default Right Option (AppSettings.swift:157).

| Choice id | macOS key code | Label | Source |
|---|---|---|---|
| `rightOption` | 61 | Right Option | App/HotkeyMonitor.swift:60 |
| `rightCommand` | 54 | Right Command | App/HotkeyMonitor.swift:61 |
| `fn` | 63 | Fn / Globe | App/HotkeyMonitor.swift:62 |

- Only the chosen modifier key counts. Other keys in the same event stream are ignored (HotkeyMonitor.swift:68).
- Esc (key code 53) is a separate cancel key and is not part of the gesture (HotkeyMonitor.swift:83-85).
- Changing the key replaces the recognizer, so a half-finished gesture is dropped (HotkeyMonitor.swift:26-29).
- Platform-specific: the key codes, modifier flags and the global monitors (NSEvent, needs Accessibility) are macOS. A port maps the same three roles (push-to-talk key, Esc) to its own key ids; its default key is a Windows decision (see README, Platform-specific).

## What each gesture does

The app maps gesture outputs to the session (App/AppDelegate.swift:50-55, 103-123).

| Output | Action |
|---|---|
| `press_began` | Session hands-free recording: stop it (:105-106). Session recording not hands-free: nothing (:107-108). Any other state: start recording, not hands-free (:109-110). |
| `press_ended` | Stop recording (:52). |
| `short_tap` | If recording and not hands-free: cancel the recording (:116-118). Otherwise nothing. A hands-free recording stopped by `press_began` is already stopped by then. |
| `double_tap` | Make the current recording hands-free (:54; DictationSession.swift:101-103). |
| Esc | If recording or busy (loading model, transcribing, formatting, inserting): cancel (:121-123). |

Consequences the port must keep:

- Hold the key longer than 0.25 s: push-to-talk. Recording starts on key down, so a short tap first records and is then cancelled.
- Tap twice within 0.45 s: hands-free. The first tap starts and cancels a recording, the second starts a new one that `double_tap` makes hands-free. A tap while hands-free recording stops it.
- `startRecording` is ignored unless the state is idle, done or failed (DictationSession.swift:67-70).
- Cancel stops the recorder, clears the live preview, cancels processing and returns to idle without inserting anything (DictationSession.swift:120-127).
