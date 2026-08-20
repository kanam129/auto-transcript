# Third-party licenses

Auto-Transcript itself is MIT licensed. It links against the libraries below, all of
which are permissively licensed. Two of them are Apache-2.0, which requires their license
text to travel with any **binary** distribution — so this file ships alongside the DMG and
the Windows installer, not just in the repository.

## Speech recognition

| Component | License | Notes |
|---|---|---|
| [whisper.cpp](https://github.com/ggerganov/whisper.cpp) | MIT | Compiled into the binary via `whisper-rs`. |
| [whisper-rs](https://github.com/tazz4843/whisper-rs) | Unlicense | Rust bindings. |
| OpenAI Whisper model weights | MIT | **Not redistributed.** See below. |

### About the models

The `ggml-*.bin` model files are **not** part of this repository and are **not** included
in any release artifact. The app downloads them on first run, directly from
[ggerganov/whisper.cpp on Hugging Face](https://huggingface.co/ggerganov/whisper.cpp),
and verifies each file against a SHA-256 hash pinned in `src-tauri/src/stt/model_manager.rs`.

The weights originate from [OpenAI Whisper](https://github.com/openai/whisper) and are MIT
licensed. Nothing about the download is proxied or modified by this project.

## Application framework

| Component | License |
|---|---|
| [Tauri](https://tauri.app) | MIT OR Apache-2.0 |
| [React](https://react.dev) | MIT |
| [Zustand](https://github.com/pmndrs/zustand) | MIT |
| [TanStack Virtual](https://tanstack.com/virtual) | MIT |

## Audio and data

| Component | License |
|---|---|
| [cpal](https://github.com/RustAudio/cpal) | **Apache-2.0** |
| [hound](https://github.com/ruuda/hound) | **Apache-2.0** |
| [rubato](https://github.com/HEnquist/rubato) | MIT OR Apache-2.0 |
| [ringbuf](https://github.com/agerasev/ringbuf) | MIT OR Apache-2.0 |
| [rusqlite](https://github.com/rusqlite/rusqlite) + SQLite | MIT / public domain |
| [keyring](https://github.com/hwchen/keyring-rs) | MIT OR Apache-2.0 |
| [reqwest](https://github.com/seanmonstar/reqwest) | MIT OR Apache-2.0 |

The full text of the Apache License 2.0 is at
<https://www.apache.org/licenses/LICENSE-2.0>. A complete, machine-generated dependency
list can be produced with `cargo tree` and `npm ls`.
