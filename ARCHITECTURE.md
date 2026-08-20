# Architecture

A tour of how Auto-Transcript works and, more usefully, why it works that way. Most of the
decisions below were settled by measurement; those measurements are in
[BENCHMARK.md](BENCHMARK.md).

## What it does

Captures the audio **coming out of** the computer, transcribes it with whisper.cpp as the
meeting happens, and shows it as large text. The microphone is captured on a second track,
transcribed, and stored — but never displayed.

## The pipeline

```
┌ output device ─┐   cpal      downmix    rubato     ┌──────────────┐
│ (process tap / │──► stream ──► mono ──► →16 kHz ──►│ ring buffer  │
│  WASAPI loop)  │   f32 48k                          └──────┬───────┘
└────────────────┘                                           │
                              ┌──────────────────────────────┴─────┐
                              │ WAV writer (system.wav)            │
                              │ VAD → segmenter                    │
                              │  adaptive hangover · 6 s hard cap   │
                              └──────────────┬─────────────────────┘
                                             │ Utterance{pcm, t0, t1}
              ┌──────────────────────────────┴──────────────────────┐
              │ preview worker (small)      │  final worker (turbo)  │
              │ every 700 ms, prefix-locked │  priority queue        │
              │ also detects the language ──┼─► forced language      │
              └──────────────┬──────────────┴──────────┬────────────┘
                             │ partial text            │ Segment
                             └────────────┬────────────┘
                                          │
                         ┌────────────────┴──────────────┐
                         │ SQLite (WAL) + events to the UI │
                         └────────────────┬──────────────┘
                                          │
                         ┌────────────────┴──────────────┐
                         │ live caption · history · export │
                         └───────────────────────────────┘

┌ microphone ────┐                        ┌──────────────┐
│                │──► same front end ────►│ mic.wav      │──► transcribed at low
└────────────────┘                        └──────────────┘    priority, never shown
```

**One hard rule:** the audio callback never allocates, never locks, never does I/O. It
writes into a ring buffer and nothing else. Resampling, VAD, WAV writing and inference all
happen on worker threads.

## Decisions worth knowing

### Capturing system audio needs no driver

`cpal` builds an *input* stream on an *output* device, which it implements as a CoreAudio
process tap on macOS (14.2+) and WASAPI loopback on Windows. The user's output device is
untouched, volume keys keep working, and only audio that is actually playing is recorded.

BlackHole and Multi-Output Devices still work and are still offered, because a virtual
loopback device is a legitimate setup — but they are the fallback, not the main path.

### Two tracks, one of them invisible

The microphone is transcribed so that AI summaries can see both sides of a conversation,
but it is never shown live. This also gives speaker separation for free, without any
diarisation model — which matters on a machine with 8 GB of RAM.

Microphone work is queued at lower priority than system audio. System audio is what someone
is reading right now; the microphone track can afford to lag.

### Two models, not one

A large model produces the text that is kept; a small one produces the grey preview text
that refreshes roughly every 600 ms. The small model does double duty as the **language
detector**, because automatic detection inside the large model costs a full extra encoder
pass — around 1.75 s, about half of the total latency.

Quality levels in the UI set both models together. Letting people mix them freely produced
combinations that fail in surprising ways: a large main model paired with a `base` preview
emitted Russian on Indonesian audio.

### Latency lives in the segmenter, not the model

Inference is steady at roughly 0.7–0.9 s per chunk. What determines whether the app feels
realtime is **when a chunk is closed**:

- Adaptive hangover: 320 ms while a chunk is short, 260 ms once it passes 3.5 s.
- Hard cap of 6 s, cut at the quietest frame in the last second.
- `audio_ctx` trimmed to the chunk length and rounded to fixed sizes, so Metal reuses its
  compute buffers instead of reallocating.

### Hallucinations are filtered, not tolerated

Whisper invents text on silence — it has seen a great many subtitle files that end with
"Thanks for watching". Segments are dropped when `no_speech_prob` is high, when the text
matches a blacklist of stock phrases, when a word repeats too often, or when a segment is
identical to the one before it.

The bare phrase list includes "thank you" and "terima kasih" on their own, which came from
a real session: a quiet room produced one segment reading "Thank you."

## Module map

```
src-tauri/src/
├── audio/
│   ├── devices.rs      enumerate sources; output devices become capture sources
│   ├── capture.rs      cpal streams, ring buffer, watchdog, level metering
│   ├── resample.rs     any rate → 16 kHz (passthrough when already 16 kHz)
│   ├── vad.rs          energy VAD with an adaptive noise floor
│   ├── segmenter.rs    utterance boundaries — where realtime latency is decided
│   └── writer.rs       streaming WAV output and playback
├── stt/
│   ├── model_manager.rs  catalogue, quality profiles, resumable verified downloads
│   ├── engine.rs         whisper-rs wrapper, audio_ctx sizing
│   ├── worker.rs         priority queue, shared language, prefix lock, degradation
│   └── filter.rs         hallucination filtering
├── store/
│   ├── db.rs           SQLite schema, FTS5 search
│   └── export.rs       Markdown / TXT / SRT / JSON
├── summarize/          OpenAI-compatible client, map-reduce (disabled by default)
├── session.rs          wires capture → workers → database → UI events
└── commands.rs         every Tauri command
```

## Data model

`sessions` and `segments`, plus a `segments_fts` FTS5 index kept in sync by triggers, plus
a `summaries` table that is present but unused until AI summaries are switched on.

Segments are written one at a time rather than batched. At the rate humans speak this is a
few inserts per minute, and it means a segment is durable the moment it exists.

## Two deliberately different views

**Live** is for reading: a fixed reading line, no timestamps, no scrolling. The sentence
being spoken is bright white and always appears at the same height; earlier sentences fade
upward and out. Nothing you are reading moves.

**History** is for browsing: timestamps, scrolling, full-text search across every session,
and inline editing.

The split exists because every shifting line break and every blinking timestamp forces the
eye to find its place again. That is a poor trade when someone is using this to follow a
language they are not fluent in.

## Platform differences

| | macOS | Windows |
|---|---|---|
| System capture | CoreAudio process tap | WASAPI loopback |
| Acceleration | Metal | CPU, or Vulkan via the `gpu-vulkan` feature |
| Data directory | `~/Library/Application Support/auto-transcript` | `%APPDATA%\auto-transcript` |
| Logs | `~/Library/Logs/auto-transcript` | `<data dir>\logs` |
| Keychain | Keychain | Credential Manager |
| Window chrome | overlay title bar, traffic-light gutter | standard title bar |

## Testing

Unit tests cover the segmenter, the hallucination filter, export formats, and the summary
chunker. Beyond those there are runnable tools rather than mocks, because the interesting
failures in this program are about audio and timing:

| Tool | What it answers |
|---|---|
| `examples/sim.rs` | How late does text appear, from the user's point of view? |
| `examples/bench.rs` | What is each model's real-time factor and memory use? |
| `examples/devices.rs` | Which sources exist, and does direct capture actually work? |
| `examples/e2e.rs` | Does the whole pipeline work, with or without audio hardware? |
| `examples/freshstart.rs` | Does a machine that has never run this app still start? |

That last one exists because of a real bug: `db_path()` built a path without creating its
directory, so SQLite failed and the app aborted before any window appeared — on a clean
install only. It never showed up during development, because the directory always already
existed.
