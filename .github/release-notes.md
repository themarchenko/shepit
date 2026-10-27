## Install on macOS

Paste this into Terminal:

```
curl -fsSL https://raw.githubusercontent.com/themarchenko/shepit/main/install.sh | bash
```

It downloads `Shepit.dmg` from the latest release, puts Shepit in Applications and opens it. macOS
does not block it, because files downloaded with curl are not quarantined. Run the same command to
update.

Or download `Shepit.dmg` below, open it and drag Shepit into Applications. Shepit is not notarized,
so macOS blocks the first launch:

1. Open Shepit. When macOS says it could not verify Shepit, click **Done**, not Move to Trash.
2. Open System Settings → Privacy & Security, scroll down to Security and click **Open Anyway** next
   to the line about Shepit. Confirm with your password or Touch ID. The button is there for about an
   hour after step 1.

Then allow Microphone and Accessibility when Shepit asks. Releases are signed with the same
certificate every time, so both permissions stay granted after an update.

Requires macOS 26 on Apple silicon.

## Install on Windows

Paste this into PowerShell:

```
irm https://raw.githubusercontent.com/themarchenko/shepit/main/install.ps1 | iex
```

It downloads `Shepit-setup.exe` from the latest release, checks its checksum, installs Shepit for
your user (no administrator rights) and starts it.
Shepit updates itself: once a day it checks for a new version and shows a card. The switch and
Check now are in Settings → About.

The installer is not code-signed. If you download `Shepit-setup.exe` below in a browser, Windows
SmartScreen will most likely show "Windows protected your PC": click **More info**, then **Run
anyway**. The PowerShell command should not get that prompt, because files PowerShell downloads
carry no Mark of the Web.

Requires 64-bit Windows 10 or later.
