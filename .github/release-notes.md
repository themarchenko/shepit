## Install

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

Then allow Microphone and Accessibility when Shepit asks. After an update macOS may ask for
Accessibility again, because the app is signed ad hoc.

Requires macOS 26 on Apple silicon.
