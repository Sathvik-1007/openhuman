//! Voice profiles for speaker identification via audio embeddings.

use std::collections::HashMap;
use std::sync::Mutex;
use tracing::{debug, info};

const MAX_PROFILES: usize = 50;
const EMBEDDING_DIM: usize = 13;

static PROFILES: std::sync::LazyLock<Mutex<HashMap<String, VoiceProfile>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone)]
pub struct VoiceProfile {
    pub id: String,
    pub name: String,
    pub embedding: Vec<f32>,
    pub sample_count: u32,
}

/// Register a voice profile from >= 1s of 16kHz audio.
pub fn register_profile(name: &str, samples: &[i16]) -> Result<String, String> {
    if samples.len() < 16_000 {
        return Err("need >= 1s audio at 16kHz".into());
    }
    let id = format!("vp-{}", crate::openhuman::util::uuid_v4());
    let profile = VoiceProfile {
        id: id.clone(),
        name: name.into(),
        embedding: extract_embedding(samples),
        sample_count: 1,
    };
    let mut store = PROFILES.lock().map_err(|e| format!("lock: {e}"))?;
    if store.len() >= MAX_PROFILES {
        return Err("max profiles reached".into());
    }
    store.insert(id.clone(), profile);
    info!("[voice-profiles] registered '{name}' id={id}");
    Ok(id)
}

/// Update profile with additional audio (running average).
pub fn update_profile(profile_id: &str, samples: &[i16]) -> Result<(), String> {
    if samples.len() < 16_000 {
        return Err("need >= 1s audio".into());
    }
    let new_emb = extract_embedding(samples);
    let mut store = PROFILES.lock().map_err(|e| format!("lock: {e}"))?;
    let p = store.get_mut(profile_id).ok_or("profile not found")?;
    let n = p.sample_count as f32;
    for (i, val) in p.embedding.iter_mut().enumerate() {
        *val = (*val * n + new_emb[i]) / (n + 1.0);
    }
    p.sample_count += 1;
    debug!(
        "[voice-profiles] updated {} samples={}",
        profile_id, p.sample_count
    );
    Ok(())
}

/// Identify speaker from audio. Returns (id, name, similarity) if above threshold.
pub fn identify_speaker(samples: &[i16], threshold: f32) -> Option<(String, String, f32)> {
    if samples.len() < 8_000 {
        return None;
    }
    let emb = extract_embedding(samples);
    let store = PROFILES.lock().ok()?;
    store
        .values()
        .map(|p| (p, cosine_sim(&emb, &p.embedding)))
        .filter(|(_, sim)| *sim > threshold)
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(p, sim)| (p.id.clone(), p.name.clone(), sim))
}

pub fn list_profiles() -> Vec<(String, String, u32)> {
    PROFILES
        .lock()
        .map(|s| {
            s.values()
                .map(|p| (p.id.clone(), p.name.clone(), p.sample_count))
                .collect()
        })
        .unwrap_or_default()
}

pub fn delete_profile(id: &str) -> Result<(), String> {
    PROFILES
        .lock()
        .map_err(|e| format!("{e}"))?
        .remove(id)
        .map(|_| ())
        .ok_or("not found".into())
}

fn extract_embedding(samples: &[i16]) -> Vec<f32> {
    let frame_size = 512;
    let mut features = vec![0.0f32; EMBEDDING_DIM];
    let mut count = 0u32;
    for frame in samples.chunks(frame_size) {
        if frame.len() < frame_size {
            break;
        }
        count += 1;
        let band_size = frame_size / EMBEDDING_DIM;
        for (bi, band) in frame.chunks(band_size).enumerate().take(EMBEDDING_DIM) {
            let energy: f32 =
                band.iter().map(|&s| (s as f32).powi(2)).sum::<f32>() / band.len() as f32;
            features[bi] += energy.sqrt();
        }
    }
    if count > 0 {
        for f in features.iter_mut() {
            *f /= count as f32;
        }
    }
    let norm: f32 = features.iter().map(|f| f * f).sum::<f32>().sqrt();
    if norm > 1e-6 {
        for f in features.iter_mut() {
            *f /= norm;
        }
    }
    features
}

fn cosine_sim(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na < 1e-6 || nb < 1e-6 {
        0.0
    } else {
        dot / (na * nb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_needs_min_audio() {
        assert!(register_profile("x", &[0; 100]).is_err());
    }

    #[test]
    fn same_signal_high_sim() {
        let s: Vec<i16> = (0..16_000)
            .map(|i| ((i as f32 * 0.1).sin() * 5000.0) as i16)
            .collect();
        let e1 = extract_embedding(&s);
        let e2 = extract_embedding(&s);
        assert!(cosine_sim(&e1, &e2) > 0.99);
    }
}
