<p align="center">
  <img src="apple/macOS/Shepit/Assets.xcassets/AppIcon.appiconset/icon_256.png" width="128" height="128" alt="">
</p>

# Shepit

Shepit (шепіт, Ukrainian for "a whisper") is a dictation app for macOS. Hold a key, speak Ukrainian or
English, let go, and the text appears where your cursor is: in Terminal, a chat, an editor, a browser.

Speech recognition runs on your Mac. If you already use Claude Code or Codex, Shepit can hand the
recognized text to it to fix punctuation, names and code terms before inserting it.

There is also a Windows version. It is newer: it records, recognizes on the PC and pastes the text.
Polishing, history, live preview and Settings are macOS only for now.

## Features

- On-device recognition: Whisper large-v3-turbo through WhisperKit, or Parakeet v3 through
  FluidAudio. The audio never leaves the Mac.
- Ukrainian, English and both in one sentence: English terms said inside Ukrainian speech come
  out in Latin script. "зроби рефакторинг хендл сабміт" becomes "Зроби рефакторинг handleSubmit".
- Words while you speak: the widget shows what it hears as you talk.
- Long dictations finish sooner: Whisper transcribes in pieces while you are still speaking. A
  46-second prompt is ready about 4 seconds after you let go.
- Optional polishing: pick Claude Code, Codex or nothing. With an agent, Shepit fixes
  punctuation, self-corrections ("на п'яту, ні, на шосту"), filler words and identifiers. Without
  one, the text goes in as heard, with spacing, capitals and dictionary terms fixed locally.
- A dictionary that grows: names and terms the recognizer gets wrong. Terms the agent corrects
  are added on their own.
- Three widget styles: flow pill, caption bar or island, on black glass. Drag the widget
  anywhere and it stays there.

## Requirements

- macOS 26. Built and tested on an M1 with 16 GB of memory.
- About 1.6 GB of disk for the speech models, downloaded on first use.
- Windows: 64-bit Windows 10 or later. About 670 MB of disk for the speech model, downloaded on first
  use.
- For polishing (optional): [Claude Code](https://claude.ai/code) or
  [Codex](https://github.com/openai/codex), installed and signed in.

## Install on macOS

Paste this into Terminal:

```
curl -fsSL https://raw.githubusercontent.com/themarchenko/shepit/main/install.sh | bash
```

The script downloads the latest release, puts `Shepit.app` in `/Applications` and opens it. Files
downloaded with curl are not quarantined, so macOS opens the app without the "could not verify"
warning. [install.sh](install.sh) is about 80 lines if you want to read it first.

After that Shepit updates itself: it checks GitHub once a day and offers the new version, which it
downloads, checks and installs in place. The switch and a Check now button are in Settings → About.

### Without Terminal (macOS)

1. Download [Shepit.dmg](https://github.com/themarchenko/shepit/releases/latest/download/Shepit.dmg),
   open it and drag Shepit into Applications.
2. Open Shepit. It is not notarized, so macOS says it could not verify it. Click **Done**, not Move
   to Trash.
3. Open System Settings → Privacy & Security, scroll down to Security and click **Open Anyway** next
   to the line about Shepit. Confirm with your password or Touch ID. The button is there for about an
   hour after step 2.

If Shepit is already in Applications, this does the same as steps 2 and 3:

```
xattr -dr com.apple.quarantine /Applications/Shepit.app
```

### Permissions (macOS)

On first launch Shepit asks for the microphone and for Accessibility. Accessibility lets it notice the
key in any app and paste the text at your cursor; the welcome tour opens that list in System
Settings, where you turn Shepit on. Releases from 0.3.0 on are all signed with the same certificate,
so the permissions stay granted across updates. Coming from 0.2.x, grant Accessibility once more. If
Shepit is on in the list but nothing gets pasted, remove it with the minus button, click Open Privacy
settings in Shepit's Settings and turn it on again.

## Install on Windows

Paste this into PowerShell (no administrator rights needed):

```
irm https://raw.githubusercontent.com/themarchenko/shepit/main/install.ps1 | iex
```

The script downloads `Shepit-setup.exe` from the latest release, checks its checksum, installs Shepit
for your user and starts it. Shepit appears in the notification area next to the clock.
[install.ps1](install.ps1) is short if you want to read it first.

To update, run the same command again. The app does not update itself on Windows yet.

The installer is not code-signed, because a code-signing certificate costs money. If you download
`Shepit-setup.exe` from the [releases page](https://github.com/themarchenko/shepit/releases/latest)
in a browser, Windows SmartScreen shows "Windows protected your PC". Click **More info**, then
**Run anyway**. The PowerShell command above avoids this: files that PowerShell downloads carry no
Mark of the Web, the flag a browser adds and SmartScreen looks for.

## Use

| Do this | To |
|---|---|
| Hold Right Option, speak, release | Dictate |
| Double tap Right Option | Dictate hands-free; tap once to stop |
| Esc | Cancel |

The key, language, recognizer, widget style and polishing agent are in Settings, from the menu bar
icon. A short tour runs on first launch.

## Privacy

- Audio is recorded and recognized on your Mac. The last 200 recordings are kept in
  `~/Library/Application Support/Shepit/recordings/` so you can compare recognizers on your own
  voice. Delete the folder whenever you like.
- With polishing on, the agent you picked receives the recognized text and some context about where
  it goes: the app name, window title, page URL in browsers, up to 500 characters before the cursor
  and any selected text. The agent then sends that to Anthropic or OpenAI under your own account.
  Turn polishing off and nothing leaves the Mac.
- Once a day Shepit asks the GitHub API for the latest release. The request carries the app version
  and nothing else. Turn it off in Settings → About.
- Password fields are detected and nothing is inserted into them.
- Windows: audio is recognized on your PC and no recordings are kept. Apart from the model download
  on first use and the installer download, nothing leaves the PC. The Windows app does not check for
  updates yet; when it does, it will make one request a day to github.com and have a switch for it.

## Build from source

You need Xcode 26 and [XcodeGen](https://github.com/yonaskolb/XcodeGen).

```
brew install xcodegen
make install     # Release build, copied to /Applications
make test        # unit tests of ShepitCore
make logs        # live logs from the running app
```

Builds are signed ad hoc, which means macOS forgets the Accessibility permission on every rebuild.
To keep it, sign with your own Apple Development identity: see `apple/macOS/Config/Signing.xcconfig`.

`shepit-smoke` runs the recognizers and the polishing step without the app, for example to
compare engines on your own recordings:

```
cd apple/ShepitCore
swift run -c release shepit-smoke --audio "$HOME/Library/Application Support/Shepit/recordings" \
    --engine parakeet-v3 --engine whisper-turbo --no-claude
```

## Project layout

- `apple/ShepitCore`: audio, hotkey gestures, recognizers, live preview, polishing agents,
  insertion, history, settings and updates. Everything that can be tested with `swift test`.
- `apple/macOS`: the app itself (widget, Settings, welcome tour, menu bar item), its XcodeGen spec
  and the layout of the release disk image.
- `windows`: the Windows version, a Rust workspace (`core`, `engine`, `release-check`) and the Tauri
  app in `app`.
- `install.sh` and `install.ps1`: the one-line installers for macOS and Windows.
- `docs/PLAN.md` and `docs/DECISIONS.md`: the plan and a dated log of decisions and measurements.
- `CLAUDE.md` and `.claude/agents/`: notes and subagents for working on the code with Claude Code.

## Contributing

Issues and pull requests are welcome. `main` takes changes through pull requests only, merged with
squash, so the history stays linear. Releases are tags `vX.Y.Z`; CI builds and publishes them.

## Thanks

[WhisperKit](https://github.com/argmaxinc/argmax-oss-swift) by Argmax,
[FluidAudio](https://github.com/FluidInference/FluidAudio) by Fluid Inference,
[GRDB](https://github.com/groue/GRDB.swift), OpenAI's Whisper and NVIDIA's Parakeet models.

## License

MIT. See [LICENSE](LICENSE).
