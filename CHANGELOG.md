# Changelog

All notable changes to this project are documented here. This project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] — unreleased

First public release.

### Features

- Realtime captioning of system audio using whisper.cpp, running fully offline.
- **Direct system-audio capture with no driver**: a CoreAudio process tap on macOS and
  WASAPI loopback on Windows. Your output device is not changed and volume keys keep
  working. BlackHole plus a Multi-Output Device is still supported as a fallback.
- Microphone is recorded and transcribed separately as material for summaries, and never
  shown in the live window.
- Two deliberately different views: a live view with a fixed reading line, no timestamps
  and no scrolling; and a history view with timestamps, scrolling, full-text search across
  every session, and inline editing.
- Automatic language handling for meetings that mix English and Indonesian.
- Export to Markdown, plain text, SRT, and JSON.
- Quality levels that configure both the main and the preview model together.
- AI summaries are implemented behind an OpenAI-compatible client but ship disabled.

### Performance

Measured with `examples/sim.rs`, which replays audio through the real pipeline at real
speed. Final text went from 10.5–11.7 s behind the speaker to 2.4–2.7 s; preview text from
every 2000 ms to roughly every 600 ms. `BENCHMARK.md` records how.

### Known limitations

- macOS builds are ad-hoc signed, so Gatekeeper blocks them until the quarantine attribute
  is removed. See the README.
- The Windows build path is verified by CI but has not been run by a human.
- No sustained multi-hour soak test has been performed yet.
