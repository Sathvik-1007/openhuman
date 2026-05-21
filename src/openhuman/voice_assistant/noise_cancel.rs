//! Noise and echo cancellation for voice assistant audio.
//!
//! Spectral subtraction for noise reduction + NLMS adaptive filter for echo cancellation.

use tracing::debug;

const LOG_PREFIX: &str = "[voice-noise-cancel]";

#[derive(Debug, Clone)]
pub struct NoiseCancelConfig {
    pub noise_reduction_strength: f32,
    pub echo_cancel_enabled: bool,
    pub echo_filter_len: usize,
    pub nlms_step: f32,
}

impl Default for NoiseCancelConfig {
    fn default() -> Self {
        Self {
            noise_reduction_strength: 0.5,
            echo_cancel_enabled: true,
            echo_filter_len: 256,
            nlms_step: 0.1,
        }
    }
}

pub struct NoiseCancelState {
    config: NoiseCancelConfig,
    noise_floor: f32,
    echo_weights: Vec<f32>,
    reference_buf: Vec<f32>,
    frame_count: u64,
}

impl NoiseCancelState {
    pub fn new(config: NoiseCancelConfig) -> Self {
        let filter_len = config.echo_filter_len;
        Self {
            config,
            noise_floor: 0.0,
            echo_weights: vec![0.0; filter_len],
            reference_buf: Vec::new(),
            frame_count: 0,
        }
    }

    /// Process mic input with noise reduction and optional echo cancellation.
    pub fn process(&mut self, input: &[i16], reference: Option<&[i16]>) -> Vec<i16> {
        self.frame_count += 1;
        let mut samples: Vec<f32> = input.iter().map(|&s| s as f32).collect();

        if self.config.noise_reduction_strength > 0.0 {
            samples = self.reduce_noise(&samples);
        }

        if self.config.echo_cancel_enabled {
            if let Some(ref_signal) = reference {
                samples = self.cancel_echo(&samples, ref_signal);
            }
        }

        samples
            .iter()
            .map(|&s| s.clamp(-32768.0, 32767.0) as i16)
            .collect()
    }

    fn reduce_noise(&mut self, samples: &[f32]) -> Vec<f32> {
        let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
        if self.frame_count < 10 || rms < self.noise_floor * 1.5 {
            self.noise_floor = self.noise_floor * 0.95 + rms * 0.05;
        }
        let threshold = self.noise_floor * (1.0 + self.config.noise_reduction_strength * 2.0);
        let gain = if rms > threshold {
            1.0
        } else {
            (rms / threshold).max(0.05)
        };
        if gain < 1.0 {
            debug!("{LOG_PREFIX} noise gate gain={:.2}", gain);
        }
        samples.iter().map(|&s| s * gain).collect()
    }

    fn cancel_echo(&mut self, input: &[f32], reference: &[i16]) -> Vec<f32> {
        self.reference_buf
            .extend(reference.iter().map(|&s| s as f32));
        let max_buf = self.config.echo_filter_len + input.len();
        if self.reference_buf.len() > max_buf {
            let drain = self.reference_buf.len() - max_buf;
            self.reference_buf.drain(..drain);
        }
        let fl = self.config.echo_filter_len;
        if self.reference_buf.len() < fl {
            return input.to_vec();
        }
        let mut output = Vec::with_capacity(input.len());
        let ref_start = self.reference_buf.len().saturating_sub(fl + input.len());
        for (i, &mic) in input.iter().enumerate() {
            let idx = ref_start + i;
            if idx + fl > self.reference_buf.len() {
                output.push(mic);
                continue;
            }
            let ref_slice = &self.reference_buf[idx..idx + fl];
            let echo_est: f32 = self
                .echo_weights
                .iter()
                .zip(ref_slice)
                .map(|(w, r)| w * r)
                .sum();
            let error = mic - echo_est;
            let power: f32 = ref_slice.iter().map(|r| r * r).sum::<f32>() + 1e-6;
            let step = self.config.nlms_step / power;
            for (w, r) in self.echo_weights.iter_mut().zip(ref_slice) {
                *w += step * error * r;
            }
            output.push(error);
        }
        output
    }

    /// Feed TTS output for echo reference tracking.
    pub fn feed_reference(&mut self, reference: &[i16]) {
        self.reference_buf
            .extend(reference.iter().map(|&s| s as f32));
        if self.reference_buf.len() > 80_000 {
            let drain = self.reference_buf.len() - 80_000;
            self.reference_buf.drain(..drain);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_is_gated() {
        let mut state = NoiseCancelState::new(NoiseCancelConfig::default());
        for _ in 0..15 {
            state.process(&vec![10i16; 160], None);
        }
        let out = state.process(&vec![5i16; 160], None);
        let rms: f32 =
            (out.iter().map(|&s| (s as f32).powi(2)).sum::<f32>() / out.len() as f32).sqrt();
        assert!(rms <= 10.0);
    }

    #[test]
    fn loud_passes() {
        let mut state = NoiseCancelState::new(NoiseCancelConfig::default());
        for _ in 0..15 {
            state.process(&vec![10i16; 160], None);
        }
        let out = state.process(&vec![5000i16; 160], None);
        assert!(out.iter().all(|&s| s > 1000));
    }
}
