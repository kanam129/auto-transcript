# Security and privacy

## What this app does with your data

Auto-Transcript records meeting audio and transcribes it. That is sensitive material, so
here is exactly what happens to it.

**Everything stays on your machine.** Transcription runs locally through whisper.cpp. There
is no telemetry, no crash reporting, no analytics, and no account.

The app opens exactly two kinds of network connection, both of which you trigger yourself:

1. **Downloading a model** from Hugging Face, on first run or when you pick a different
   quality level. Each file is verified against a SHA-256 hash pinned in the source.
2. **AI summaries**, if — and only if — you switch them on in Settings and provide an API
   key. That switch is off by default. While it is off, the app makes no other network
   calls at all.

## Where your data is stored

| Platform | Location |
|---|---|
| macOS | `~/Library/Application Support/auto-transcript/` |
| Windows | `%APPDATA%\auto-transcript\` |

That folder holds the transcript database, your settings, downloaded models, and the raw
audio recordings. Logs live in `~/Library/Logs/auto-transcript/` on macOS and in a `logs/`
subfolder on Windows.

**Uninstalling the app does not delete any of it.** Remove the folder yourself if you want
it gone. Settings → *Where your data lives* shows the exact paths and can open the folder
for you.

API keys are stored in the operating system keychain (macOS Keychain, Windows Credential
Manager), never in a config file and never in a log.

## Recording other people

Laws about recording conversations differ by country, and in many places every participant
has to consent. This app does not — and cannot — check that for you. Whether you are
allowed to record a given meeting is your responsibility.

## Reporting a vulnerability

Please open a
[GitHub Security Advisory](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability)
rather than a public issue, so the problem can be fixed before it is widely known.

Please include the app version, your OS version, and the relevant part of `app.log`.
**Read the log before pasting it** — it contains device names and, at debug level, timing
information about your sessions. It does not contain transcript text.
