# Roadmap

An honest account of what works, what is thin, and what has not been done. This replaces an
internal task list that tracked 57 items through to completion; what follows is what is
still open.

## Works and is exercised regularly

Realtime capture and transcription · direct system-audio capture with no driver on both
platforms · separate microphone track · language handling for mixed English/Indonesian ·
live caption view · history with full-text search and inline editing · export to
Markdown/TXT/SRT/JSON · automatic model downgrade under load · crash recovery · quality
profiles · settings migrations.

## Thin, or verified less than it should be

**Windows has never been used by a human.** CI compiles, lints, unit-tests and bundles it
on every push, so the platform-specific code is genuinely compiled rather than merely
written. Nobody has run it in a meeting. A report either way would be valuable.

**No long soak test.** The longest continuous session so far is a few minutes. Memory
stability, thermal behaviour and timestamp drift over a two-hour meeting are unknown.
`examples/sim.rs` can replay long audio if someone wants to check.

**Preview text is not perfect.** It comes from a smaller model and gets corrected when the
final text arrives. That is by design, but it means the grey text is sometimes wrong in
ways the white text is not.

**Accuracy is measured on synthesised speech.** Real meeting audio — noise, VoIP
compression, accents, people talking over each other — is harder, and no systematic
measurement of it exists.

## Not built

**Streaming final text.** Today final text cannot appear until its chunk is closed, which
puts a floor of roughly 2.5 s on it. Running the main model over the growing buffer and
locking text in once two consecutive hypotheses agree (LocalAgreement-2) would remove that
wait entirely — an estimated 1.2–1.5 s, with no burstiness. This is the single largest
remaining improvement and it is architectural, not a matter of tuning.

**AI summaries are disabled.** The client, map-reduce chunking, prompts, keychain storage
and database table all exist behind a switch that defaults to off. What is missing is the
UI and, more importantly, a decision about how a local-first app should present a feature
that sends transcripts to a third party.

**Per-application capture.** The process tap currently captures a whole output device.
CoreAudio can tap a single process, which would let someone record only their video-call
app and not their music.

**Signing and notarisation.** Builds are ad-hoc signed, so macOS users must clear the
quarantine attribute by hand. Fixing this properly needs a paid Apple Developer account;
the release workflow already reads the relevant secrets if they are ever configured.

**Speaker diarisation.** Everything from the system output is currently one speaker. Real
diarisation would need another model and, on 8 GB, probably more memory than is available.

**Translation.** Whisper can only translate *into* English, so showing an Indonesian
translation of English speech would need an LLM. The OpenAI-compatible client built for
summaries would be a reasonable foundation.

## Would be genuinely welcome

- Measurements from real meeting audio, especially in languages other than English and
  Indonesian
- A Windows report of any kind
- Linux support — PipeWire loopback would be the equivalent of the existing mechanism
- Better model choices, backed by numbers in `BENCHMARK.md`
