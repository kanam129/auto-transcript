use super::vad::{rms_db, EnergyVad};
use super::{Track, Utterance, FRAME_SAMPLES, TARGET_RATE};
use std::collections::VecDeque;

const PREROLL_FRAMES: usize = 10; // 200 ms before the onset is included
const ONSET_FRAMES: usize = 3; // 60 ms of speech opens an utterance
const HANGOVER_FRAMES: usize = 16; // 320 ms of silence closes it
const TAIL_KEEP_FRAMES: usize = 5; // 100 ms of silence kept so it does not sound clipped
/// Once a chunk reaches this length, even a short pause is enough to close it.
const SOFT_CUT_AFTER_SAMPLES: usize = (TARGET_RATE as usize * 7) / 2; // 3.5 s
const SOFT_HANGOVER_FRAMES: usize = 13; // 260 ms
/// Counted from frames that actually contain speech — not the buffer length, which already
/// includes 200 ms of pre-roll and some trailing silence.
const MIN_SPEECH_FRAMES: usize = 15; // 300 ms of speech
/// Hard cut-off. This — not model speed — is what decides whether the app feels realtime:
/// final text cannot appear before its chunk is closed. Measured in `examples/sim.rs`,
/// lowering this from 20 s to 6 s cut worst-case latency by far more than any model
/// speedup did.
const MAX_UTT_SAMPLES: usize = TARGET_RATE as usize * 6;
/// Search window for the forced cut point, deliberately narrow (1 second).
///
/// A forced cut only happens when somebody talks non-stop with no pause at all — natural
/// pauses are handled by the adaptive hangover above. In that situation there is no good
/// sentence boundary to find, so what matters is minimising the distance from the cut to
/// "now": every second we skip back is a second of text held from the screen.
const CUT_SEARCH_FRAMES: usize = 50;
const CUT_GUARD_FRAMES: usize = 5; // never cut right at the end of the buffer

/// Splits the 16 kHz stream into utterances based on pauses in speech.
///
/// A forced cut is not made exactly at the time limit but at the quietest frame within the
/// search window, so the boundary lands between words rather than through one.
pub struct Segmenter {
    track: Track,
    vad: EnergyVad,
    partial_frame: Vec<f32>,
    preroll: VecDeque<(Vec<f32>, f32)>,
    speech: Vec<f32>,
    /// (energy, is this frame speech) aligned with `speech`, one entry per frame.
    frames: Vec<(f32, bool)>,
    in_speech: bool,
    onset_run: usize,
    silence_run: usize,
    utt_start_sample: u64,
    total_samples: u64,
}

impl Segmenter {
    pub fn new(track: Track) -> Self {
        Self {
            track,
            vad: EnergyVad::default(),
            partial_frame: Vec::with_capacity(FRAME_SAMPLES),
            preroll: VecDeque::with_capacity(PREROLL_FRAMES + 1),
            speech: Vec::with_capacity(TARGET_RATE as usize * 25),
            frames: Vec::with_capacity(1200),
            in_speech: false,
            onset_run: 0,
            silence_run: 0,
            utt_start_sample: 0,
            total_samples: 0,
        }
    }

    fn samples_to_ms(s: u64) -> i64 {
        (s as i64 * 1000) / TARGET_RATE as i64
    }

    pub fn push(&mut self, samples: &[f32], out: &mut Vec<Utterance>) {
        let mut idx = 0;
        while idx < samples.len() {
            let need = FRAME_SAMPLES - self.partial_frame.len();
            let take = need.min(samples.len() - idx);
            self.partial_frame.extend_from_slice(&samples[idx..idx + take]);
            idx += take;
            if self.partial_frame.len() == FRAME_SAMPLES {
                let frame = std::mem::replace(
                    &mut self.partial_frame,
                    Vec::with_capacity(FRAME_SAMPLES),
                );
                self.process_frame(frame, out);
            }
        }
    }

    fn process_frame(&mut self, frame: Vec<f32>, out: &mut Vec<Utterance>) {
        let energy = rms_db(&frame);
        let is_speech = self.vad.is_speech(&frame);
        let frame_end = self.total_samples + FRAME_SAMPLES as u64;

        if !self.in_speech {
            self.preroll.push_back((frame, energy));
            if self.preroll.len() > PREROLL_FRAMES {
                self.preroll.pop_front();
            }
            self.onset_run = if is_speech { self.onset_run + 1 } else { 0 };

            if self.onset_run >= ONSET_FRAMES {
                self.speech.clear();
                self.frames.clear();
                let n = self.preroll.len();
                for (i, (f, e)) in self.preroll.drain(..).enumerate() {
                    self.speech.extend_from_slice(&f);
                    // The last three pre-roll frames are the onset frames that opened this.
                    self.frames.push((e, i + ONSET_FRAMES >= n));
                }
                self.utt_start_sample = frame_end - self.speech.len() as u64;
                self.in_speech = true;
                self.silence_run = 0;
                self.onset_run = 0;
            }
        } else {
            self.speech.extend_from_slice(&frame);
            self.frames.push((energy, is_speech));
            self.silence_run = if is_speech { 0 } else { self.silence_run + 1 };

            // Adaptive hangover. Early in a chunk we are patient, so a natural pause in
            // the middle of a sentence does not split it in two. Once the chunk is long
            // enough that patience gets expensive: waiting for a long pause means a
            // finished sentence sits in the buffer. Measured in `examples/sim.rs`, this
            // change took worst-case latency from 3.3 s down to about 1 s.
            let hangover_needed = if self.speech.len() >= SOFT_CUT_AFTER_SAMPLES {
                SOFT_HANGOVER_FRAMES
            } else {
                HANGOVER_FRAMES
            };

            if self.silence_run >= hangover_needed {
                self.close_utterance(out);
            } else if self.speech.len() >= MAX_UTT_SAMPLES {
                self.force_cut(out);
            }
        }

        self.total_samples = frame_end;
    }

    fn close_utterance(&mut self, out: &mut Vec<Utterance>) {
        let trim_frames = self.silence_run.saturating_sub(TAIL_KEEP_FRAMES);
        let keep_frames = self.frames.len().saturating_sub(trim_frames);
        let keep_samples = (keep_frames * FRAME_SAMPLES).min(self.speech.len());
        let speech_frames = self.frames[..keep_frames.min(self.frames.len())]
            .iter()
            .filter(|(_, sp)| *sp)
            .count();

        if speech_frames >= MIN_SPEECH_FRAMES {
            let pcm = self.speech[..keep_samples].to_vec();
            out.push(Utterance {
                track: self.track,
                start_ms: Self::samples_to_ms(self.utt_start_sample),
                end_ms: Self::samples_to_ms(self.utt_start_sample + keep_samples as u64),
                pcm,
            });
        }

        self.in_speech = false;
        self.silence_run = 0;
        self.onset_run = 0;
        self.speech.clear();
        self.frames.clear();
        self.preroll.clear();
    }

    fn force_cut(&mut self, out: &mut Vec<Utterance>) {
        let n = self.frames.len();
        let hi = n.saturating_sub(CUT_GUARD_FRAMES);
        let lo = hi.saturating_sub(CUT_SEARCH_FRAMES);
        let mut cut_frame = hi;
        let mut best = f32::MAX;
        for i in lo..hi {
            if self.frames[i].0 < best {
                best = self.frames[i].0;
                cut_frame = i;
            }
        }
        let cut = (cut_frame * FRAME_SAMPLES).min(self.speech.len());
        let speech_frames = self.frames[..cut_frame.min(n)]
            .iter()
            .filter(|(_, sp)| *sp)
            .count();
        if speech_frames >= MIN_SPEECH_FRAMES {
            out.push(Utterance {
                track: self.track,
                start_ms: Self::samples_to_ms(self.utt_start_sample),
                end_ms: Self::samples_to_ms(self.utt_start_sample + cut as u64),
                pcm: self.speech[..cut].to_vec(),
            });
        }
        self.speech.drain(..cut);
        self.frames.drain(..cut_frame);
        self.utt_start_sample += cut as u64;
        self.silence_run = 0;
    }

    /// A copy of the speech in progress, for the preview path (the grey text).
    pub fn partial_snapshot(&self, min_samples: usize) -> Option<(i64, Vec<f32>)> {
        if self.in_speech && self.speech.len() >= min_samples {
            Some((
                Self::samples_to_ms(self.utt_start_sample),
                self.speech.clone(),
            ))
        } else {
            None
        }
    }

    /// Called when a session stops, so the final sentence is not lost.
    pub fn flush(&mut self, out: &mut Vec<Utterance>) {
        if !self.partial_frame.is_empty() {
            let mut frame = std::mem::take(&mut self.partial_frame);
            frame.resize(FRAME_SAMPLES, 0.0);
            self.process_frame(frame, out);
        }
        if self.in_speech {
            self.silence_run = TAIL_KEEP_FRAMES;
            self.close_utterance(out);
        }
    }

    pub fn noise_floor_db(&self) -> f32 {
        self.vad.noise_floor_db()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(n: usize, amp: f32) -> Vec<f32> {
        (0..n)
            .map(|i| amp * (i as f32 * 0.05).sin())
            .collect()
    }

    #[test]
    fn splits_two_utterances_on_a_pause() {
        let mut seg = Segmenter::new(Track::System);
        let mut out = Vec::new();
        let silence = vec![0.0f32; TARGET_RATE as usize]; // 1 s
        seg.push(&silence, &mut out);
        seg.push(&tone(TARGET_RATE as usize, 0.3), &mut out); // 1 s bicara
        seg.push(&vec![0.0f32; TARGET_RATE as usize], &mut out); // a 1 s pause
        seg.push(&tone(TARGET_RATE as usize, 0.3), &mut out);
        seg.flush(&mut out);
        assert_eq!(out.len(), 2, "expected two utterances, got {}", out.len());
        assert!(out[0].end_ms <= out[1].start_ms);
    }

    #[test]
    fn drops_utterances_that_are_too_short() {
        let mut seg = Segmenter::new(Track::System);
        let mut out = Vec::new();
        seg.push(&vec![0.0f32; TARGET_RATE as usize], &mut out);
        seg.push(&tone(FRAME_SAMPLES * 4, 0.3), &mut out); // 80 ms
        seg.push(&vec![0.0f32; TARGET_RATE as usize], &mut out);
        seg.flush(&mut out);
        assert!(out.is_empty(), "an 80 ms utterance should be dropped");
    }

    #[test]
    fn force_cuts_during_non_stop_speech() {
        let mut seg = Segmenter::new(Track::System);
        let mut out = Vec::new();
        seg.push(&vec![0.0f32; TARGET_RATE as usize / 2], &mut out);
        seg.push(&tone(TARGET_RATE as usize * 45, 0.3), &mut out); // 45 s non-stop
        assert!(out.len() >= 2, "45 seconds should be cut at least twice");
        for u in &out {
            assert!(u.end_ms - u.start_ms <= 21_000);
        }
    }
}
