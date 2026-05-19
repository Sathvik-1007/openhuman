//! Voice assistant brain — STT → LLM → TTS orchestration.
//!
//! Runs a single conversational turn: drains inbound PCM from the session,
//! transcribes via the configured STT provider, sends the transcript to the
//! LLM, synthesizes the reply via the configured TTS provider, and enqueues
//! the resulting PCM on the session's outbound buffer.
//!
//! ## Log prefix
//!
//! `[voice-assistant-brain]` — grep-friendly for end-to-end traces.

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use log::{debug, info, warn};
use serde_json::{json, Value};

use crate::openhuman::config::Config;
use crate::openhuman::meet_agent::wav::pack_pcm16le_mono_wav;
use crate::openhuman::voice::factory::{create_stt_provider, create_tts_provider};

use super::session::SessionRegistry;
use super::types::SessionState;

const LOG_PREFIX: &str = "[voice-assistant-brain]";

/// Run a single voice assistant turn for the given session.
///
/// Called when VAD detects end-of-utterance. The session must exist and
/// have inbound PCM buffered.
pub async fn run_turn(session_id: &str) -> Result<(), String> {
    info!("{LOG_PREFIX} turn started session={session_id}");

    // 1. Mark session as processing and drain inbound PCM.
    let (pcm, stt_provider_name, tts_provider_name, language, history) =
        SessionRegistry::with_session(session_id, |s| {
            s.state = SessionState::Processing;
            let pcm = s.drain_inbound_pcm();
            let history: Vec<(String, String)> = s
                .history
                .iter()
                .map(|t| (t.user_text.clone(), t.assistant_text.clone()))
                .collect();
            (
                pcm,
                s.stt_provider.clone(),
                s.tts_provider.clone(),
                s.language.clone(),
                history,
            )
        })?;

    if pcm.is_empty() {
        debug!("{LOG_PREFIX} no PCM buffered, skipping turn session={session_id}");
        SessionRegistry::with_session(session_id, |s| {
            s.state = SessionState::Listening;
        })?;
        return Ok(());
    }

    debug!(
        "{LOG_PREFIX} draining {} samples ({:.2}s) session={session_id}",
        pcm.len(),
        pcm.len() as f64 / 16_000.0
    );

    // 2. STT: PCM → text.
    let config = crate::openhuman::config::ops::load_config_with_timeout()
        .await
        .map_err(|e| format!("{LOG_PREFIX} config load failed: {e}"))?;
    let transcript = run_stt(&config, &pcm, &stt_provider_name, language.as_deref()).await?;

    if transcript.trim().is_empty() {
        debug!("{LOG_PREFIX} empty transcript, skipping LLM session={session_id}");
        SessionRegistry::with_session(session_id, |s| {
            s.state = SessionState::Listening;
        })?;
        return Ok(());
    }

    info!(
        "{LOG_PREFIX} STT result: \"{}\" session={session_id}",
        truncate(&transcript, 80)
    );

    // 3. LLM: transcript + history → reply.
    let reply = run_llm(&config, &transcript, &history).await?;

    info!(
        "{LOG_PREFIX} LLM reply: \"{}\" session={session_id}",
        truncate(&reply, 80)
    );

    // 4. TTS: reply → PCM.
    let tts_pcm = run_tts(&config, &reply, &tts_provider_name).await?;

    debug!(
        "{LOG_PREFIX} TTS produced {} samples ({:.2}s) session={session_id}",
        tts_pcm.len(),
        tts_pcm.len() as f64 / 16_000.0
    );

    // 5. Enqueue outbound and record turn.
    SessionRegistry::with_session(session_id, |s| {
        s.state = SessionState::Speaking;
        s.enqueue_outbound_pcm(&tts_pcm);
        s.record_turn(&transcript, &reply);
    })?;

    // 6. After enqueue, transition back to listening.
    SessionRegistry::with_session(session_id, |s| {
        s.state = SessionState::Listening;
    })?;

    info!("{LOG_PREFIX} turn completed session={session_id}");
    Ok(())
}

// ---------------------------------------------------------------------------
// STT
// ---------------------------------------------------------------------------

async fn run_stt(
    config: &Config,
    pcm: &[i16],
    provider_name: &str,
    language: Option<&str>,
) -> Result<String, String> {
    let provider = create_stt_provider(provider_name, "", config)
        .map_err(|e| format!("{LOG_PREFIX} STT provider creation failed: {e}"))?;

    // Pack PCM into WAV and base64-encode for the provider interface.
    let wav_bytes = pack_pcm16le_mono_wav(pcm, 16_000);
    let audio_b64 = B64.encode(&wav_bytes);

    debug!(
        "{LOG_PREFIX} STT dispatch provider={} wav_bytes={} b64_len={}",
        provider.name(),
        wav_bytes.len(),
        audio_b64.len()
    );

    let outcome = provider
        .transcribe(config, &audio_b64, Some("audio/wav"), None, language)
        .await
        .map_err(|e| format!("{LOG_PREFIX} STT failed: {e}"))?;

    Ok(outcome.value.text)
}

// ---------------------------------------------------------------------------
// LLM
// ---------------------------------------------------------------------------

async fn run_llm(
    config: &Config,
    transcript: &str,
    history: &[(String, String)],
) -> Result<String, String> {
    use crate::api::config::effective_backend_api_url;
    use crate::api::jwt::get_session_token;
    use crate::api::BackendOAuthClient;
    use reqwest::Method;

    let token = get_session_token(config)
        .map_err(|e| e.to_string())?
        .filter(|t| !t.trim().is_empty())
        .ok_or_else(|| format!("{LOG_PREFIX} no backend session token"))?;

    let api_url = effective_backend_api_url(&config.api_url);
    let client = BackendOAuthClient::new(&api_url).map_err(|e| e.to_string())?;

    // Build messages array for the LLM.
    let system_msg = json!({
        "role": "system",
        "content": "You are a helpful voice assistant. Keep responses concise and conversational — \
                    the user is speaking to you and will hear your reply read aloud. \
                    Avoid markdown, code blocks, or long lists unless explicitly asked."
    });

    let mut messages = vec![system_msg];

    // Add conversation history (last 10 turns max for context window).
    for (user, assistant) in history.iter().rev().take(10).rev() {
        messages.push(json!({"role": "user", "content": user}));
        messages.push(json!({"role": "assistant", "content": assistant}));
    }

    messages.push(json!({"role": "user", "content": transcript}));

    debug!(
        "{LOG_PREFIX} LLM request messages={} transcript_len={}",
        messages.len(),
        transcript.len()
    );

    let body = json!({
        "model": "agentic-v1",
        "temperature": 0.5,
        "max_tokens": 300,
        "messages": messages,
    });

    let raw = client
        .authed_json(
            &token,
            Method::POST,
            "/openai/v1/chat/completions",
            Some(body),
        )
        .await
        .map_err(|e| format!("{LOG_PREFIX} LLM request failed: {e}"))?;

    let text = extract_chat_completion_text(&raw)
        .ok_or_else(|| format!("{LOG_PREFIX} unexpected LLM response: {raw}"))?;

    Ok(strip_for_speech(&text))
}

// ---------------------------------------------------------------------------
// TTS
// ---------------------------------------------------------------------------

async fn run_tts(config: &Config, text: &str, provider_name: &str) -> Result<Vec<i16>, String> {
    let provider = create_tts_provider(provider_name, "", config)
        .map_err(|e| format!("{LOG_PREFIX} TTS provider creation failed: {e}"))?;

    debug!(
        "{LOG_PREFIX} TTS dispatch provider={} text_len={}",
        provider.name(),
        text.len()
    );

    let outcome = provider
        .synthesize(config, text, None)
        .await
        .map_err(|e| format!("{LOG_PREFIX} TTS failed: {e}"))?;

    let result = outcome.value;

    // Decode the base64 audio into PCM16LE samples.
    let audio_bytes = B64
        .decode(&result.audio_base64)
        .map_err(|e| format!("{LOG_PREFIX} TTS audio decode failed: {e}"))?;

    // The audio may be WAV-wrapped or raw PCM depending on provider.
    // Try to strip WAV header if present (44 bytes for standard RIFF/WAVE).
    let pcm_bytes = if audio_bytes.len() > 44 && &audio_bytes[0..4] == b"RIFF" {
        &audio_bytes[44..]
    } else {
        &audio_bytes
    };

    if pcm_bytes.len() % 2 != 0 {
        warn!(
            "{LOG_PREFIX} TTS returned odd byte count {}, truncating last byte",
            pcm_bytes.len()
        );
    }

    let samples: Vec<i16> = pcm_bytes
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect();

    Ok(samples)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        // Find the last char boundary at or before `max` to avoid panicking on multi-byte UTF-8.
        let end = s.floor_char_boundary(max);
        &s[..end]
    }
}

/// Extract the assistant message text from a chat completions response.
fn extract_chat_completion_text(raw: &Value) -> Option<String> {
    raw.get("choices")?
        .get(0)?
        .get("message")?
        .get("content")?
        .as_str()
        .map(|s| s.trim().to_string())
}

/// Strip characters that sound bad when read aloud by TTS.
fn strip_for_speech(text: &str) -> String {
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
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_string() {
        let long = "a".repeat(100);
        assert_eq!(truncate(&long, 10).len(), 10);
    }

    #[test]
    fn truncate_multibyte_utf8_no_panic() {
        // Each emoji is 4 bytes. Slicing at byte 5 would split a char and panic without floor_char_boundary.
        let s = "😀😀😀";
        let result = truncate(s, 5);
        assert_eq!(result, "😀"); // 4 bytes fits, 8 doesn't
    }

    #[test]
    fn extract_chat_completion_text_parses_openai_format() {
        let raw = json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Hello there!"
                }
            }]
        });
        assert_eq!(
            extract_chat_completion_text(&raw),
            Some("Hello there!".to_string())
        );
    }

    #[test]
    fn extract_chat_completion_text_returns_none_on_bad_shape() {
        assert_eq!(extract_chat_completion_text(&json!({})), None);
        assert_eq!(extract_chat_completion_text(&json!({"choices": []})), None);
    }

    #[test]
    fn strip_for_speech_removes_markdown() {
        assert_eq!(strip_for_speech("**bold** text"), "bold text");
        assert_eq!(
            strip_for_speech("- item one\n- item two"),
            "item one item two"
        );
        assert_eq!(strip_for_speech("```\ncode\n```\nafter"), "after");
    }
}
