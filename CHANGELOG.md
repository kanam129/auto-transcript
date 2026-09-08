# Changelog

All notable changes to this project are documented here. This project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] — 2026-09-08

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

### Fixed on Windows

The first run of this application on a real Windows machine, which found three things CI
could not:

- **The app could not start at all.** The global start/stop shortcut was registered while
  the plugin initialised, so a combination already owned by another application aborted
  the whole program before any window appeared. Windows owns Win+Shift+R (the system
  screen recorder), which made this certain rather than unlikely. The shortcut is now
  Ctrl+Shift+R off macOS, and failing to register it costs the shortcut and nothing else.
- **System audio capture kept restarting itself.** Every stream error was treated as
  fatal, and WASAPI reports an underrun whenever the output device runs dry — that is,
  during every silence. The stream was torn down and rebuilt 112 times in 6 seconds, and
  no audio was captured while that was happening. Only errors that genuinely break the
  stream now trigger a rebuild.
- **`examples/devices.rs` reported a false failure.** It played its test sound with
  `afplay`, which exists only on macOS, so on Windows it made no sound and then declared
  that capture had failed. It now plays a sound on each platform, and says "nothing was
  playing" rather than "capture is broken" when no audio arrives.

Also: the in-app text-size and record shortcuts tested `metaKey`, which is the Windows key
off macOS, so none of them worked there; `examples/bench.rs` measured memory with `ps` and
reported 0 MB on Windows; and the README omitted LLVM, without which the build cannot
finish because bindgen needs `libclang.dll`.

### Added on Windows

- **GPU acceleration is chosen at startup, not at build time.** A binary built with
  `gpu-vulkan` asks the Vulkan loader whether the machine has a device: it uses the GPU
  when there is one and stays on the CPU when there is not, so one build serves both.
  `AUTO_TRANSCRIPT_GPU=0` forces the CPU path.
  The check deliberately goes to the Vulkan loader rather than to `ggml_backend_vk_*`,
  because calling ggml on a machine whose loader finds no driver terminates the process
  with no message at all. `vulkan-1.dll` is also delay-loaded now, so a Vulkan-enabled
  build still starts on a machine that has no Vulkan runtime at all.
- `scripts/build-windows-gpu.bat` builds that configuration in one command.

### Known limitations

- macOS builds are ad-hoc signed, so Gatekeeper blocks them until the quarantine attribute
  is removed. See the README.
- Windows now starts, captures system audio and transcribes on a real machine, but has
  still not been used through an actual meeting.
- No sustained multi-hour soak test has been performed yet.
