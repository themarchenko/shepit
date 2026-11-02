# Doctor (macOS only)

A window named "Shepit Doctor" that lists what keeps dictation from working, says why in one line and offers a fix where one exists. macOS only; Windows lags (see Windows below). Code: `ShepitCore/Sources/ShepitCore/Doctor/` (pure evaluation, the `tccutil` runner, thin probes) and `macOS/Shepit/Doctor/DoctorView.swift` (window and polling). Paths are relative to `apple/`.

## Checks

Rows appear in this order. Status is ok, info, warning, problem or checking. Info is a gray, optional note: it is never counted in the summary.

| Check | Problem or warning when | Fix button |
|---|---|---|
| Accessibility (required) | Not trusted. Problem. The text says macOS may be holding the permission of an older build. After a reset it lists steps, and while the reset runs the row is "checking". | "Reset and ask again" |
| Microphone (required) | Denied or restricted: problem. The text says macOS may be holding the permission of an older build, and one step says to reset and allow the microphone. While a reset runs the row is "checking". Not asked yet: problem. | "Reset and ask again", or "Allow" (system prompt) |
| Dictation key | Only for Fn. Keyboard setting "Press Fn key to" (`AppleFnUsageType`) other than 0: problem, with steps. Unreadable: warning. Any other key: ok, the Fn setting is ignored. | "Open Keyboard settings" |
| Speech model | Loading: checking. Failed load: problem with the first line of the error (max 120 characters). Not on disk and not loaded: warning. | "Download now" (starts the load again) |
| Polishing agent | Row hidden when polishing is off. No executable found: problem. Found: ok, with a Test button. A failed test is a problem with a plain-text reason. | "Open Polishing settings", "Test" |
| Secure input | Secure event input is on: warning, naming the app holding it when macOS says which. | none |
| Input Monitoring | Not allowed: info ("Shepit usually works without it"), with a step: if Shepit is not in the list, click + and add it from Applications. There is no signal for a dead dictation key, so the row is not escalated. | "Open Input Monitoring" (asks first, then opens the list) |

Summary line: "Everything Shepit needs is working.", or "N things need attention." for every row that is a problem, warning or checking. Info rows are not counted.

Required problem: Accessibility not trusted, or Microphone not granted. Only these two count for the menu bar marker and the auto-open rule.

## Polling

While the window is open the checks are read again every second, so a row turns green by itself after the user changes a setting in System Settings. Polling stops when the window closes. The model state comes from the engine manager (`loaded`, `loading`, `failed`, `idle`); a failed load stays failed until the user presses "Download now" or changes the recognizer.

## Permission reset (Accessibility and Microphone)

The fix runs `/usr/bin/tccutil reset Accessibility <bundle id>` with the app's own bundle id, with no shell. The arguments are exactly `["reset", "Accessibility", id]`. If the id is missing, empty, starts with "-" or does not match `^[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)+$`, nothing runs. The id is never dropped, because `tccutil reset Accessibility` without an id resets every app. After the command (success or failure) Shepit asks for Accessibility again and opens the Accessibility list. On failure the row shows manual steps (remove Shepit with the minus button, reset again, switch Shepit on).

The Microphone fix runs the same command with the service `Microphone`: `["reset", "Microphone", id]`, under the same id rules. When it succeeds Shepit asks for the microphone again, so the system prompt appears (the status is "not determined" after a reset). When it fails Shepit opens the Microphone list. The service names `Accessibility` and `Microphone` were checked against `tccutil` on macOS (`ListenEvent` also works; an unknown name exits 70).

## Window

On every open the list scrolls to the first row. The window height follows the content: header plus rows, at least 420 pt and at most the visible screen height minus 80 pt, 600 pt wide. It is recomputed when the rows change, keeping the top-left corner fixed.

Quoted UI copy in this file is the English source; the Ukrainian text lives in the String Catalogs (localization.md).

## Entry points

- Menu bar menu: "Doctor…" between History… and Settings…. When a required check is red, the menu bar icon is the orange warning triangle instead of the brand glyph.
- Widget right-click menu: "Doctor…" after History….
- Settings > General > Permissions: "Open Doctor" next to "Check again", and a caption when Accessibility is not trusted.
- The welcome tour, permissions step: "Open Doctor" after the user opened the Accessibility list and it is still not allowed.
- Launch: after onboarding is complete, and not while the tour is open, Doctor opens instead of the system Accessibility prompt when a required check is red. At most once per launch (`Doctor.shouldAutoOpen`).
- The done caption of a clipboard-only insert starts with "Copied, not pasted (open Doctor): " (dictation.md, step 8).

## Agent test

"Test" sends the fixed sample `This is a short test from Shepit Doctor.` straight to the polishing agent in clean mode with the generic profile and no user context (no app name, window title, caret text or selection, no dictionary, no examples). It skips the fast path. It is disabled while a dictation is recording, busy or in review. A failed test counts toward the agent's pause after repeated failures (polishing.md); a paused Claude shows as an agent error.

## Windows

Not implemented. Windows lags: its only counterpart is the microphone test in Settings > General (`windows/app/src-tauri/src/commands.rs`).
