# Auto-Transcript

[![CI](https://github.com/OWNER/auto-transcript/actions/workflows/ci.yml/badge.svg)](https://github.com/OWNER/auto-transcript/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Live captions for meetings on macOS. It captures the audio **coming out** of your computer
(the people you are talking to), transcribes it in realtime with Whisper, and shows it as
large text in a dark, resizable window you can pin next to Zoom.

Your microphone is recorded and transcribed quietly as material for AI summaries, but it
**never appears in the transcript window**.

Everything is transcribed locally. The app makes no network connection at all, except to
download a model — or if you explicitly enable AI summaries yourself.

![The live view: dimmed history above a fixed reading line, the current sentence in white below it](docs/screenshots/live.png)

## Install (macOS)

Download the DMG from [Releases](../../releases), open it, and drag the app to
Applications.

### macOS will say the app is damaged. It is not.

These builds are ad-hoc signed rather than signed with a paid Apple Developer ID, so
Gatekeeper refuses them with a misleading message. Clear the quarantine attribute once:

```bash
xattr -dr com.apple.quarantine /Applications/Auto-Transcript.app
```

Then open it normally. There is no way around this short of a $99/year Apple Developer
account — if you would rather not run that command, build from source instead
(see [Development](#development)); locally built apps are never quarantined.

On first launch the app walks you through three steps: choose a transcription quality
level and download it, pick a system audio source, and grant permissions.

![The first-run screen, choosing a transcription quality level](docs/screenshots/onboarding.png)

**No driver required.** The app taps your output device directly through a CoreAudio
process tap (macOS 14.2+). Your output device is not changed, your volume keys keep
working, and only audio that is actually playing gets recorded.

BlackHole plus a Multi-Output Device still works as a fallback if direct capture is
unavailable, but it is no longer the main path.

## Using it

| Action | How |
|---|---|
| Start / stop recording | The ● button, or **⌘⇧R** (works while the app is in the background) |
| Text size | **⌘+** / **⌘−** / **⌘0** |
| Keep the window on top | The pin icon, top right |
| History and search | The clock icon |
| Fix a wrong word | Double-click the line (in history view) |
| Copy one line | Right-click it |
| Export | Open a session from history → Markdown or SRT |

### Two deliberately different views

**Live** is a reading surface: a fixed reading line, no timestamps, no scrolling. The
sentence being spoken is bright white and always appears at the same height; earlier
sentences fade upward and out. Nothing you are reading moves.

**History** is a browsing surface: timestamps, scrolling, full-text search across every
session, and inline editing.

![The history view, with full-text search across every session](docs/screenshots/history.png)

## Models

| Model | Size | Role |
|---|---|---|
| `large-v3-turbo-q5_0` | 574 MB | Default. The only one that handles mixed-language sentences correctly. |
| `small-q5_1` | 190 MB | Realtime preview text. Reliable on mixed speech. |
| `base-q5_1` | 60 MB | Emergency fallback when the queue falls behind. |

Models are downloaded on first run from
[ggerganov/whisper.cpp on Hugging Face](https://huggingface.co/ggerganov/whisper.cpp) and
verified against a pinned SHA-256. They are not redistributed by this project.

The reasoning behind these choices, with measurements, is in [BENCHMARK.md](BENCHMARK.md).

## Where files live

```
~/Library/Application Support/auto-transcript/
├── data.db                  transcripts, history, search index
├── settings.json
├── models/
└── recordings/<session-id>/{system,mic}.wav      ~58 MB per hour per track
~/Library/Logs/auto-transcript/app.log.YYYY-MM-DD
```

Raw audio is kept so a session can be re-transcribed with a better model later. Automatic
cleanup is configurable under Settings → Storage.

## Windows

Windows installers are produced by CI and attached to each
[release](../../releases). If you want to build one yourself, note that a Windows binary
**cannot be built from macOS**: whisper.cpp has to be compiled with the MSVC toolchain and
Tauri needs the WebView2 SDK, both of which only exist on Windows.

On a Windows PC, install first:

- [Rust](https://rustup.rs) (the default `x86_64-pc-windows-msvc` toolchain)
- [Node.js](https://nodejs.org) 20 or newer
- **Visual Studio Build Tools** with the "Desktop development with C++" workload
- [CMake](https://cmake.org/download/) — make sure it is on PATH
- WebView2 Runtime (already bundled with Windows 11)

Then:

```powershell
npm install
npm run tauri build
```

Output lands in `src-tauri\target\release\bundle\nsis\` (an .exe installer) and
`...\msi\`.

### What differs on Windows

| Area | Behaviour |
|---|---|
| Capturing system audio | WASAPI loopback — **no driver, no BlackHole**. Just pick your output device. |
| Acceleration | CPU by default. For GPU: install the Vulkan SDK and build with `npm run tauri build -- --features gpu-vulkan`. |
| Recommended model | Without a GPU, `large-v3-turbo` is likely too heavy. Start with `small-q5_1` and measure with `cargo run --release --example sim`. |
| Window chrome | A normal Windows title bar; the macOS traffic-light gutter is not reserved. |

Being precise about how much this is tested: CI compiles, lints, unit-tests and bundles
the Windows build on every push, so the platform-specific code paths are genuinely
compiled and not merely written. But **no human has run it in a real meeting.** If you do,
a report either way would be genuinely useful.

## Development

```bash
npm install
npm run tauri dev                 # needs cmake: brew install cmake
cd src-tauri && cargo test        # 15 unit tests
cargo clippy --all-targets -- -D warnings

# tools
cargo run --release --example bench -- a.wav b.wav       # RTF and memory per model
cargo run --release --example e2e -- --offline a.wav     # pipeline without audio devices
cargo run --release --example devices                    # list sources, test direct capture
cargo run --release --example sim -- a.wav               # measure user-perceived latency
cargo run --release --example freshstart                 # verify a clean first run works
```

How it works and why: [ARCHITECTURE.md](ARCHITECTURE.md) · measurements:
[BENCHMARK.md](BENCHMARK.md) · what is left to do: [ROADMAP.md](ROADMAP.md).

## Recording other people

Rules about recording conversations differ by country, and in many places every
participant has to consent. This app cannot check that for you, and does not try to.
Whether you are allowed to record a given meeting is your call.

## Contributing

Bug reports, measurements, and pull requests are welcome — see
[CONTRIBUTING.md](CONTRIBUTING.md). The short version: `cargo clippy -D warnings` must be
clean, and if you change anything that affects latency or accuracy, please put numbers in
[BENCHMARK.md](BENCHMARK.md) rather than describing how it feels.

## Licence

[MIT](LICENSE). Third-party components and their licences are listed in
[THIRD-PARTY-LICENSES.md](THIRD-PARTY-LICENSES.md); model weights are not redistributed by
this project. Privacy and security details are in [SECURITY.md](SECURITY.md).

## AI summaries

Not enabled yet. The whole machinery is in place — an OpenAI-compatible client, map-reduce
for long meetings, key storage in the OS keychain — behind a **Settings → AI summaries**
switch that defaults to off. While that switch is off, the app never touches the network.
