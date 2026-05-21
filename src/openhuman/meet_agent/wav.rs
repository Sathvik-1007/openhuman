//! Shared audio utilities — PCM encoding, WAV container, text-for-speech
//! cleanup.
//!
//! Used by both `meet_agent` and `voice_assistant` domains.

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};

const WAV_HEADER_LEN: usize = 44;

/// Decode a base64 string of PCM16LE bytes into samples. Empty input is
/// a "heartbeat" push (no audio this tick) and yields an empty Vec.
pub fn decode_pcm16le_b64(b64: &str) -> Result<Vec<i16>, String> {
    if b64.is_empty() {
        return Ok(Vec::new());
    }
    let bytes = B64
        .decode(b64.as_bytes())
        .map_err(|e| format!("base64: {e}"))?;
    if bytes.len() % 2 != 0 {
        return Err(format!("odd byte length {}", bytes.len()));
    }
    Ok(bytes
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect())
}

/// Strip characters that sound bad when read aloud by TTS.
/// Removes markdown fences, bullet markers, and inline formatting.
pub fn strip_for_speech(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_code = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        let cleaned: String = trimmed
            .trim_start_matches(|c: char| c == '-' || c == '*' || c == '#' || c == '>')
            .trim()
            .chars()
            .filter(|c| !matches!(c, '*' | '`' | '_' | '#'))
            .collect();
        if cleaned.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&cleaned);
    }
    out.trim().to_string()
}

/// Produce a complete WAV file (header + interleaved PCM16LE samples).
/// Caller passes the raw `i16` slice and the sample rate; mono is
/// hard-coded because that's what the meet-agent loop uses end-to-end.
pub fn pack_pcm16le_mono_wav(samples: &[i16], sample_rate_hz: u32) -> Vec<u8> {
    let data_bytes = samples.len() * 2;
    let mut out = Vec::with_capacity(WAV_HEADER_LEN + data_bytes);

    // RIFF chunk descriptor
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + data_bytes) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVE");

    // fmt sub-chunk
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // PCM header size
    out.extend_from_slice(&1u16.to_le_bytes()); // audio format = PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // num channels = 1
    out.extend_from_slice(&sample_rate_hz.to_le_bytes());
    out.extend_from_slice(&(sample_rate_hz * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample

    // data sub-chunk
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data_bytes as u32).to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_bytes_match_riff_wave_layout() {
        let bytes = pack_pcm16le_mono_wav(&[0; 8000], 16_000);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[12..16], b"fmt ");
        assert_eq!(&bytes[36..40], b"data");
        // RIFF size = 36 + data_bytes (8000 samples * 2 bytes = 16000).
        let riff_size = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        assert_eq!(riff_size, 36 + 16_000);
        // Sample rate field at offset 24.
        let rate = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
        assert_eq!(rate, 16_000);
    }

    #[test]
    fn empty_input_still_produces_valid_header() {
        let bytes = pack_pcm16le_mono_wav(&[], 16_000);
        assert_eq!(bytes.len(), WAV_HEADER_LEN);
        assert_eq!(&bytes[0..4], b"RIFF");
    }

    #[test]
    fn samples_are_appended_little_endian() {
        let bytes = pack_pcm16le_mono_wav(&[0x1234, -1], 16_000);
        // First sample 0x1234 → LE bytes 0x34, 0x12 starting at offset 44.
        assert_eq!(bytes[44], 0x34);
        assert_eq!(bytes[45], 0x12);
        // -1 in i16 LE → 0xFF, 0xFF.
        assert_eq!(bytes[46], 0xFF);
        assert_eq!(bytes[47], 0xFF);
    }

    #[test]
    fn decode_pcm16le_b64_roundtrip() {
        let samples: Vec<i16> = vec![100, -200, 32767, -32768];
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let encoded = B64.encode(bytes);
        assert_eq!(decode_pcm16le_b64(&encoded).unwrap(), samples);
    }

    #[test]
    fn decode_pcm16le_b64_empty_is_heartbeat() {
        assert_eq!(decode_pcm16le_b64("").unwrap(), Vec::<i16>::new());
    }

    #[test]
    fn decode_pcm16le_b64_odd_bytes_rejected() {
        let encoded = B64.encode([0x01, 0x02, 0x03]);
        assert!(decode_pcm16le_b64(&encoded).is_err());
    }

    #[test]
    fn strip_for_speech_removes_markdown() {
        let input = "## Hello\n- **world**\n```rust\ncode\n```\n> quote";
        let out = strip_for_speech(input);
        assert!(!out.contains('#'));
        assert!(!out.contains('*'));
        assert!(!out.contains("code"));
        assert!(out.contains("Hello"));
        assert!(out.contains("world"));
    }

    #[test]
    fn strip_for_speech_empty_input() {
        assert_eq!(strip_for_speech(""), "");
    }
}
