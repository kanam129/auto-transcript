use crate::error::{AppError, Result};
use rubato::{FftFixedInOut, Resampler};

/// Converts any sample rate to 16 kHz. If the device is already at 16 kHz this becomes a
/// passthrough, costing nothing and adding no distortion.
pub struct ToTarget {
    inner: Option<FftFixedInOut<f32>>,
    pending: Vec<f32>,
    scratch_in: Vec<Vec<f32>>,
    scratch_out: Vec<Vec<f32>>,
}

impl ToTarget {
    pub fn new(input_rate: u32, target_rate: u32) -> Result<Self> {
        if input_rate == target_rate {
            return Ok(Self {
                inner: None,
                pending: Vec::new(),
                scratch_in: Vec::new(),
                scratch_out: Vec::new(),
            });
        }
        // 20 ms chunks; rubato rounds to a size that is valid for this ratio.
        let chunk = (input_rate as usize / 50).max(64);
        let r = FftFixedInOut::<f32>::new(input_rate as usize, target_rate as usize, chunk, 1)
            .map_err(|e| AppError::Audio(format!("resampler {input_rate}->{target_rate}: {e}")))?;
        let in_len = r.input_frames_next();
        let out_len = r.output_frames_max();
        Ok(Self {
            inner: Some(r),
            pending: Vec::with_capacity(in_len * 4),
            scratch_in: vec![vec![0.0; in_len]],
            scratch_out: vec![vec![0.0; out_len]],
        })
    }

    /// Feeds in native-rate samples and returns 16 kHz samples ready for use.
    pub fn push(&mut self, input: &[f32], out: &mut Vec<f32>) -> Result<()> {
        let Some(r) = self.inner.as_mut() else {
            out.extend_from_slice(input);
            return Ok(());
        };
        self.pending.extend_from_slice(input);
        loop {
            let need = r.input_frames_next();
            if self.pending.len() < need {
                break;
            }
            self.scratch_in[0][..need].copy_from_slice(&self.pending[..need]);
            let (_, written) = r
                .process_into_buffer(&self.scratch_in, &mut self.scratch_out, None)
                .map_err(|e| AppError::Audio(format!("resampling failed: {e}")))?;
            out.extend_from_slice(&self.scratch_out[0][..written]);
            self.pending.drain(..need);
        }
        Ok(())
    }
}
