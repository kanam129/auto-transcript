# Measurements

Every number here was measured on the same machine: a MacBook Pro (M2, 8 GB RAM,
macOS 26.5), whisper.cpp on the Metal backend, 4 threads, release builds.

The samples are synthesised with the macOS `say` command: one meeting sentence in English,
one in Indonesian, and one file that splices an English sentence straight into an
Indonesian one **with no pause** — the worst case for language detection.

> Synthesised speech is far cleaner than a real meeting: no noise, no VoIP compression, no
> accents, nobody talking over anybody. Treat the absolute numbers as optimistic and the
> comparisons between them as meaningful.

---

## Part 1 · Choosing a model

| Model | Sample | Audio (s) | Load (s) | Inference (s) | RTF | RSS (MB) | Detected |
|---|---|---:|---:|---:|---:|---:|---|
| base-q5_1 | en | 14.6 | 7.9 | 0.88 | **0.06** | 132 | en |
| base-q5_1 | id | 18.2 | — | 0.40 | **0.02** | 145 | id |
| base-q5_1 | mix | 9.1 | — | 0.40 | **0.04** | 146 | id |
| small-q5_1 | en | 14.6 | 0.2 | 1.11 | **0.08** | 276 | en |
| small-q5_1 | id | 18.2 | — | 1.08 | **0.06** | 259 | id |
| small-q5_1 | mix | 9.1 | — | 0.96 | **0.11** | 248 | id |
| large-v3-turbo-q5_0 | en | 14.6 | 0.5 | 4.14 | **0.28** | 576 | en |
| large-v3-turbo-q5_0 | id | 18.2 | — | 3.78 | **0.21** | 567 | id |
| large-v3-turbo-q5_0 | mix | 9.1 | — | 3.76 | **0.41** | 566 | id |

### Speed was not the deciding factor

All three sit far below the 0.4 real-time-factor target on clean audio. Even turbo spends
only 0.21–0.41 seconds per second of audio, and 576 MB on an 8 GB machine still leaves room
for a browser and a video call.

### Indonesian accuracy separated them

- `base` misheard repeatedly: "konvirmasi", "harirabu", "keperlataan owner".
- `small` was nearly right, but turned "product owner" into "Project Owner".
- `turbo` was correct throughout.

### Mixed-language handling decided it

On the spliced sample:

- `base` mangled the English half: *"Jadi, di teknik kita finatkan kembali ke seluruhnya."*
- `small` **dropped the entire English sentence without a trace.** Its output contained only
  the Indonesian half. This is the dangerous kind of failure: the transcript looks perfectly
  reasonable while half the content is missing.
- `turbo` transcribed both halves correctly despite the language changing mid-file.

Since the whole point of this app is following a meeting whose language keeps switching,
that behaviour disqualified `small` as the main model.

### Decision

- **Main model: `large-v3-turbo-q5_0`.**
- **Preview model: `small-q5_1`** — see Part 3 for why it is not `base`.
- **Fallback: `base-q5_1`**, used only when the queue falls behind.

---

## Part 2 · Making it feel realtime

The first feedback from real use was blunt: *"it still doesn't feel realtime."* It was
correct, and measurable.

`examples/sim.rs` replays a WAV file into the real pipeline at real speed and records the
gap between a sentence being heard and its text appearing. That is the number a user feels
— not RTF.

| | Preview | Final text (mean) | Final text (worst) |
|---|---:|---:|---:|
| Before | every ~2000 ms | 10.5 – 11.7 s | 17.9 s |
| After | every ~600 ms | 2.4 – 2.5 s | 4.1 s |

### The model was not the bottleneck

Inference held steady at 770–935 ms per chunk. Four other things mattered more.

**1. Whisper always encodes a full 30-second window**, however short the audio is. A 4-second
chunk pays the same fixed cost as a 30-second one. Trimming `audio_ctx` to match the real
duration took an 8-second chunk from 1.87 s to 0.79 s.

| audio_ctx | 8 s chunk, correct language forced | Quality |
|---|---:|---|
| full (1500) | 1.91 s | intact |
| 1100 | 1.28 s | intact |
| 900 | 1.02 s | intact |
| 768 | ~0.85 s | intact |
| 528 | 0.58 s | **starts repeating words** |

The floor sits at 768 after the simulator caught the model emitting one identical sentence
three times in a row at 512.

**2. Automatic language detection in the large model costs a whole extra encoder pass** —
about 1.75 s, roughly half of the total latency. The fix: let the **small preview model**,
which is already running, detect the language in ~0.2 s, and have the large model use that
as a forced language.

These two must not be combined naively. Automatic detection *plus* a trimmed window turns
English sentences in mixed audio into Indonesian.

**3. Final text cannot appear before its chunk is closed.** A 20-second cap meant a sentence
finished at second 3 still waited until second 20. This was the single largest contributor,
and no amount of model speed can compensate for it. The cap is now 6 seconds.

**4. Waiting for a long pause is expensive at the end of a chunk but necessary at the start.**
The hangover is therefore adaptive: 320 ms while the chunk is still short, so natural pauses
mid-sentence do not split a sentence in two, dropping to 260 ms once the chunk passes
3.5 seconds so a finished sentence is not held in the buffer.

### Two bugs the measurement exposed

The simulator caught Whisper **repeating a single sentence three times** when the encoder
window was trimmed too far. Hence the 768 floor, plus a guard that drops a segment identical
to the one before it — humans essentially never repeat a whole sentence verbatim twice in
a row.

---

## Part 3 · "Can it be faster?"

Short answer: raw latency is close to its floor. What could still be improved was the
*usefulness* of the fast text.

| Experiment | Result | Verdict |
|---|---|---|
| Flash attention on Metal | 8 s chunk 1.86 → 1.44 s (full window), 0.76 → 0.69 s (ctx 700). Identical text. | **Adopted** |
| Preview cadence 700 → 450 ms | **Worse on both counts**: preview 750 → 839 ms, final 2.45 → 2.89 s. The model was not finished when the next tick arrived, so work queued up. | Rejected |
| Prefix locking (LocalAgreement-2) | No latency change, but the start of a sentence stops rewriting itself. | **Adopted** |
| Main model for previews | Previews collapsed from 15 events to 6; final text stretched to 3.8 s (worst 6.0 s). GPU contention. | Rejected |
| `small` for previews | +80 ms, but preview text went from wrong to right. | **Adopted** |

### Why the preview model changed

Once the latency was down, the preview text itself became the problem. Same audio, same
chunk:

| Model | Preview text |
|---|---|
| `base` | "escalate this to the bottom" → "This is the product owner of their product owner" |
| `small` | "escalate this to the product owner if there is no answer by Wednesday" |

On mixed-language audio the gap was worse: `base` got stuck on the English sentence, mangled
the Indonesian into "Iyabatul", and at one point emitted `всю` — Russian. `small` tracked
both languages correctly.

Text that arrives 150 ms sooner but says the wrong thing does not help anyone follow a
meeting.

### Final numbers

| | Preview | Final (mean) | Final (worst) |
|---|---:|---:|---:|
| Original | every ~2000 ms | 10.5 – 11.7 s | 17.9 s |
| Round 1 | ~550 – 650 ms, **often wrong** | 2.4 – 2.5 s | 4.1 s |
| Now | ~600 – 790 ms, **correct** | 2.45 – 2.74 s | 3.6 – 4.3 s |

Peak memory with `large-v3-turbo` and `small` loaded together: **928 MB**.

### Where the floor is

Final-text latency is now three parts, all near their limits: waiting for the chunk to close
(260–320 ms after someone stops, at most 6 s if they never do), inference (~0.7 s), and the
sub-segment effect — a sentence that ends mid-chunk waits for that chunk to close.

Pushing further through tuning means cutting more often, and that measurably damages
Indonesian: sentences split mid-phrase and artefacts like "kredensial dari TAP" and
"4. Selesai" appear. That is not a good trade for one second.

The next meaningful speedup is architectural rather than a matter of tuning: run the main
model over the growing buffer and lock text in as soon as two consecutive hypotheses agree,
so final-quality text flows continuously without ever waiting for a chunk boundary. Expected
result is roughly 1.2–1.5 s instead of 2.5 s, with no burstiness. It has not been built.

---

## Reproducing this

```bash
say -v Samantha -o sample-en.wav --data-format=LEI16@16000 --file-format=WAVE "..."
say -v Damayanti -o sample-id.wav --data-format=LEI16@16000 --file-format=WAVE "..."

cd src-tauri
cargo run --release --example bench -- ../sample-en.wav ../sample-id.wav
cargo run --release --example sim -- ../sample-en.wav
```
