# Contributing

## Getting it building

You need Rust, Node 20+, and CMake. CMake is not optional — whisper.cpp is compiled from
source as part of the build.

```bash
brew install cmake        # macOS
npm install
npm run tauri dev
```

On Windows you additionally need Visual Studio Build Tools with the "Desktop development
with C++" workload.

## Before opening a pull request

```bash
cd src-tauri
cargo clippy --all-targets -- -D warnings    # must be clean, warnings included
cargo test
cargo run --release --example freshstart     # must pass: proves a clean install still starts
cd .. && npm run build                        # type-check plus bundle
```

CI runs exactly these on macOS, and builds on Windows too.

## Measure before you tune

This project has a habit of measuring things rather than reasoning about them, and several
of its most important decisions came out of that. Please keep it up — there are tools for it:

```bash
cargo run --release --example bench -- a.wav b.wav    # real-time factor and memory per model
cargo run --release --example sim -- a.wav            # latency as the user actually feels it
cargo run --release --example devices                 # audio sources, and a live capture test
cargo run --release --example e2e -- --offline a.wav  # full pipeline without audio hardware
```

`BENCHMARK.md` records what was measured and what was decided as a result. If you change
anything that affects latency or accuracy, please add your numbers there. A pull request
that says "this feels faster" is much harder to act on than one with a table.

Test audio is easy to generate on macOS without any recording:

```bash
say -v Samantha -o sample-en.wav --data-format=LEI16@16000 --file-format=WAVE "your text here"
say -v Damayanti -o sample-id.wav --data-format=LEI16@16000 --file-format=WAVE "teks bahasa Indonesia"
```

Do note that synthesised speech is much cleaner than real meeting audio, so treat the
absolute numbers as optimistic and the comparisons between them as meaningful.

## Things worth knowing before you touch the audio path

- The audio callback must never allocate, lock, or do I/O. It writes into a ring buffer and
  nothing else. Everything expensive happens on worker threads.
- Latency is dominated by **when a chunk is closed**, not by how fast the model is. Before
  reaching for a faster model, look at `segmenter.rs`.
- Reducing `audio_ctx` speeds Whisper up dramatically, but combining it with automatic
  language detection corrupts mixed-language speech. The two must not be used together.
  `BENCHMARK.md` has the measurements.

## Code style

`cargo clippy -D warnings` and `tsc --strict` are the arbiters. Beyond that: comments
should explain *why*, not *what*. Several comments in this codebase point at a specific
measurement or a specific bug that motivated the code — that is the standard to aim for.
