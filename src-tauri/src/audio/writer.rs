use crate::error::Result;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

const FLUSH_EVERY_SAMPLES: usize = super::TARGET_RATE as usize * 5; // 5 s

/// Writes 16 kHz mono i16 WAV — exactly the format handed to Whisper, so a recording can be
/// re-transcribed later without any conversion.
pub struct WavTrackWriter {
    writer: Option<hound::WavWriter<BufWriter<File>>>,
    since_flush: usize,
}

impl WavTrackWriter {
    pub fn create(path: &Path) -> Result<Self> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: super::TARGET_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        Ok(Self {
            writer: Some(hound::WavWriter::create(path, spec)?),
            since_flush: 0,
        })
    }

    pub fn write(&mut self, samples: &[f32]) -> Result<()> {
        let Some(w) = self.writer.as_mut() else {
            return Ok(());
        };
        for &s in samples {
            let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            w.write_sample(v)?;
        }
        self.since_flush += samples.len();
        if self.since_flush >= FLUSH_EVERY_SAMPLES {
            w.flush()?;
            self.since_flush = 0;
        }
        Ok(())
    }

    pub fn finalize(&mut self) -> Result<()> {
        if let Some(w) = self.writer.take() {
            w.finalize()?;
        }
        Ok(())
    }
}

impl Drop for WavTrackWriter {
    fn drop(&mut self) {
        if let Err(e) = self.finalize() {
            tracing::warn!("could not close wav file: {e}");
        }
    }
}

/// Reads a 16 kHz mono WAV back as f32 — used for re-transcription.
pub fn read_wav_mono_f32(path: &Path) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => reader
            .samples::<i16>()
            .filter_map(|s| s.ok())
            .map(|s| s as f32 / i16::MAX as f32)
            .collect(),
        hound::SampleFormat::Float => reader.samples::<f32>().filter_map(|s| s.ok()).collect(),
    };
    if spec.channels <= 1 {
        return Ok(samples);
    }
    let ch = spec.channels as usize;
    Ok(samples
        .chunks(ch)
        .map(|c| c.iter().sum::<f32>() / ch as f32)
        .collect())
}
