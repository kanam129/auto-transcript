/// Energy-based voice activity detection with an adaptive noise floor.
///
/// Chosen over WebRTC or Silero because it adds neither a C dependency nor a second model,
/// and for meeting audio — high signal-to-noise, a digital source coming from loopback —
/// the quality difference is small. Its decisions are filtered again downstream by
/// Whisper's own `no_speech_prob`, so a false positive here does not become junk text.
pub struct EnergyVad {
    noise_db: f32,
    initialized: bool,
    /// Absolute floor, so digital silence (all zeroes) can never be mistaken for speech.
    floor_db: f32,
    /// How far above the noise floor counts as speech.
    margin_db: f32,
}

impl Default for EnergyVad {
    fn default() -> Self {
        Self {
            noise_db: -70.0,
            initialized: false,
            floor_db: -58.0,
            margin_db: 9.0,
        }
    }
}

pub fn rms_db(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return -100.0;
    }
    let sum: f32 = frame.iter().map(|s| s * s).sum();
    let rms = (sum / frame.len() as f32).sqrt();
    20.0 * (rms + 1e-10).log10()
}

impl EnergyVad {
    /// `true` if this frame is considered speech.
    pub fn is_speech(&mut self, frame: &[f32]) -> bool {
        let db = rms_db(frame);
        if !self.initialized {
            self.noise_db = db;
            self.initialized = true;
        }
        let speech = db > self.floor_db && db > self.noise_db + self.margin_db;
        if speech {
            // Raise the noise floor very slowly during speech so it does not chase a voice.
            self.noise_db += 0.001 * (db - self.noise_db).min(0.0);
        } else {
            // Falls fast, rises slowly: adapts to a room that suddenly gets noisy.
            let rate = if db < self.noise_db { 0.20 } else { 0.02 };
            self.noise_db += rate * (db - self.noise_db);
        }
        self.noise_db = self.noise_db.clamp(-90.0, -20.0);
        speech
    }

    pub fn noise_floor_db(&self) -> f32 {
        self.noise_db
    }
}
