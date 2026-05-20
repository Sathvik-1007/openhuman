//! RPC handlers for live_captions domain.

use super::{store, types::*};
use serde_json::{json, Map, Value};
use tracing::debug;

pub async fn handle_start_transcript(p: Map<String, Value>) -> Result<Value, String> {
    let source = match p
        .get("source")
        .and_then(|v| v.as_str())
        .unwrap_or("microphone")
    {
        "system_audio" => CaptionSource::SystemAudio,
        "meet_call" => CaptionSource::MeetCall,
        _ => CaptionSource::Microphone,
    };
    let id = p
        .get("transcript_id")
        .and_then(|v| v.as_str())
        .map(String::from);
    let title = p.get("title").and_then(|v| v.as_str()).map(String::from);
    let t = store::start_transcript(id, source, title);
    Ok(json!({ "ok": true, "transcript_id": t.id, "state": t.state }))
}

pub async fn handle_append_segment(p: Map<String, Value>) -> Result<Value, String> {
    let tid = p
        .get("transcript_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let seg = CaptionSegment {
        text: p
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        start_ms: p.get("start_ms").and_then(|v| v.as_u64()).unwrap_or(0),
        end_ms: p.get("end_ms").and_then(|v| v.as_u64()).unwrap_or(0),
        speaker: p.get("speaker").and_then(|v| v.as_str()).map(String::from),
        confidence: p.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0),
        is_final: p.get("is_final").and_then(|v| v.as_bool()).unwrap_or(true),
    };
    match store::append_segment(tid, seg) {
        Ok(t) => Ok(json!({ "ok": true, "segment_count": t.segments.len() })),
        Err(e) => Ok(json!({ "ok": false, "error": e })),
    }
}

pub async fn handle_complete_transcript(p: Map<String, Value>) -> Result<Value, String> {
    let tid = p
        .get("transcript_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    match store::complete_transcript(tid) {
        Ok(t) => Ok(
            json!({ "ok": true, "transcript_id": t.id, "state": t.state, "segments": t.segments.len() }),
        ),
        Err(e) => Ok(json!({ "ok": false, "error": e })),
    }
}

pub async fn handle_summarize_transcript(p: Map<String, Value>) -> Result<Value, String> {
    let tid = p
        .get("transcript_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Get the full text for LLM summarization.
    let transcript = store::get_transcript(tid)?;
    if transcript.state != TranscriptState::Completed {
        return Err("transcript must be completed before summarizing".into());
    }

    let full_text = transcript.full_text();

    // Try LLM summarization first.
    if let Some(summary) = try_llm_summarize(&full_text, transcript.segments.len()).await {
        store::set_summary(tid, &summary);
        return Ok(
            json!({ "ok": true, "transcript_id": tid, "summary": summary, "source": "llm" }),
        );
    }

    // Fallback to extractive summary.
    match store::summarize_transcript(tid) {
        Ok(t) => Ok(
            json!({ "ok": true, "transcript_id": t.id, "summary": t.summary, "source": "extractive" }),
        ),
        Err(e) => Ok(json!({ "ok": false, "error": e })),
    }
}

/// Attempt LLM-powered transcript summarization.
async fn try_llm_summarize(full_text: &str, segment_count: usize) -> Option<String> {
    use crate::openhuman::config::ops::load_config_with_timeout;
    use crate::openhuman::inference::provider::create_chat_provider;

    if full_text.is_empty() {
        return None;
    }

    let config = load_config_with_timeout().await.ok()?;
    let (provider, model) = create_chat_provider("agentic", &config).ok()?;

    // Truncate to ~4000 chars to fit context window.
    let text_for_llm = if full_text.len() > 4000 {
        &full_text[..full_text.floor_char_boundary(4000)]
    } else {
        full_text
    };

    let prompt = format!(
        "Summarize this transcript ({} segments) into concise meeting notes. Include key points, decisions, and action items if any.\n\nTranscript:\n{}",
        segment_count, text_for_llm
    );

    let system = "You are a meeting notes assistant. Produce concise, structured summaries.";

    let text = provider
        .chat_with_system(Some(system), &prompt, &model, 0.3)
        .await
        .ok()?;

    debug!(
        text_len = text.len(),
        "[live_captions] LLM summary generated"
    );
    Some(text)
}

pub async fn handle_get_transcript(p: Map<String, Value>) -> Result<Value, String> {
    let tid = p
        .get("transcript_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    match store::get_transcript(tid) {
        Ok(t) => Ok(json!({
            "ok": true, "transcript_id": t.id, "source": t.source,
            "state": t.state, "title": t.title, "segments": t.segments.len(),
            "summary": t.summary, "duration_ms": t.duration_ms(),
        })),
        Err(e) => Ok(json!({ "ok": false, "error": e })),
    }
}

pub async fn handle_list_transcripts(_p: Map<String, Value>) -> Result<Value, String> {
    let all = store::list_transcripts();
    let list: Vec<Value> = all
        .iter()
        .map(|t| {
            json!({
                "id": t.id, "source": t.source, "state": t.state,
                "title": t.title, "segments": t.segments.len(),
                "duration_ms": t.duration_ms(),
            })
        })
        .collect();
    Ok(json!({ "ok": true, "transcripts": list }))
}

pub async fn handle_search_transcripts(p: Map<String, Value>) -> Result<Value, String> {
    let query = p.get("query").and_then(|v| v.as_str()).unwrap_or("");
    if query.is_empty() {
        return Ok(json!({ "ok": false, "error": "query is required" }));
    }
    let results = store::search_transcripts(query);
    let list: Vec<Value> = results
        .iter()
        .map(|t| {
            json!({
                "id": t.id, "source": t.source, "state": t.state,
                "title": t.title, "segments": t.segments.len(),
                "duration_ms": t.duration_ms(),
            })
        })
        .collect();
    Ok(json!({ "ok": true, "results": list, "count": list.len() }))
}

/// Transcribe PCM audio bytes and auto-append as a caption segment.
///
/// Accepts base64-encoded PCM audio, transcribes via the voice STT factory,
/// and appends the result to the active transcript.
pub async fn handle_transcribe_audio(p: Map<String, Value>) -> Result<Value, String> {
    let transcript_id = p
        .get("transcript_id")
        .and_then(|v| v.as_str())
        .ok_or("missing transcript_id")?;
    let audio_b64 = p
        .get("audio_base64")
        .and_then(|v| v.as_str())
        .ok_or("missing audio_base64")?;
    let start_ms = p.get("start_ms").and_then(|v| v.as_u64()).unwrap_or(0);
    let end_ms = p.get("end_ms").and_then(|v| v.as_u64()).unwrap_or(0);

    // Attempt STT transcription via voice factory.
    let text = transcribe_via_stt(audio_b64).await.unwrap_or_else(|e| {
        debug!(error = %e, "[live_captions] STT fallback to empty");
        String::new()
    });

    if text.is_empty() {
        return Ok(json!({ "ok": false, "error": "transcription produced empty result" }));
    }

    let seg = CaptionSegment {
        text: text.clone(),
        start_ms,
        end_ms,
        speaker: None,
        confidence: 0.8,
        is_final: true,
    };

    match store::append_segment(transcript_id, seg) {
        Ok(t) => Ok(json!({
            "ok": true, "text": text,
            "segment_count": t.segments.len(), "transcript_id": transcript_id,
        })),
        Err(e) => Ok(json!({ "ok": false, "error": e })),
    }
}

/// Attempt transcription using the voice STT factory.
async fn transcribe_via_stt(audio_b64: &str) -> Result<String, String> {
    use crate::openhuman::config::ops::load_config_with_timeout;
    use crate::openhuman::voice::factory::create_stt_provider;

    let config = load_config_with_timeout()
        .await
        .map_err(|e| format!("config load failed: {e}"))?;

    let provider = create_stt_provider("whisper", "", &config)
        .map_err(|e| format!("STT provider unavailable: {e}"))?;

    let outcome = provider
        .transcribe(&config, audio_b64, Some("audio/pcm"), None, None)
        .await
        .map_err(|e| format!("STT error: {e}"))?;

    Ok(outcome.value.text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn start_transcript_rpc() {
        let mut p = Map::new();
        p.insert("transcript_id".into(), Value::String("rpc-lc-1".into()));
        p.insert("source".into(), Value::String("microphone".into()));
        let r = handle_start_transcript(p).await.unwrap();
        assert_eq!(r["ok"], true);
        assert_eq!(r["transcript_id"], "rpc-lc-1");
    }

    #[tokio::test]
    async fn append_segment_rpc() {
        let mut p = Map::new();
        p.insert("transcript_id".into(), Value::String("rpc-lc-2".into()));
        handle_start_transcript(p).await.unwrap();

        let mut p = Map::new();
        p.insert("transcript_id".into(), Value::String("rpc-lc-2".into()));
        p.insert("text".into(), Value::String("Hello".into()));
        p.insert("start_ms".into(), json!(0));
        p.insert("end_ms".into(), json!(500));
        p.insert("confidence".into(), json!(0.9));
        let r = handle_append_segment(p).await.unwrap();
        assert_eq!(r["ok"], true);
        assert_eq!(r["segment_count"], 1);
    }

    #[tokio::test]
    async fn complete_and_summarize_rpc() {
        let mut p = Map::new();
        p.insert("transcript_id".into(), Value::String("rpc-lc-3".into()));
        handle_start_transcript(p).await.unwrap();

        let mut p = Map::new();
        p.insert("transcript_id".into(), Value::String("rpc-lc-3".into()));
        p.insert("text".into(), Value::String("Test segment".into()));
        p.insert("start_ms".into(), json!(0));
        p.insert("end_ms".into(), json!(1000));
        handle_append_segment(p).await.unwrap();

        let mut p = Map::new();
        p.insert("transcript_id".into(), Value::String("rpc-lc-3".into()));
        let r = handle_complete_transcript(p).await.unwrap();
        assert_eq!(r["ok"], true);

        let mut p = Map::new();
        p.insert("transcript_id".into(), Value::String("rpc-lc-3".into()));
        let r = handle_summarize_transcript(p).await.unwrap();
        assert_eq!(r["ok"], true);
        // Summary comes from either LLM or extractive fallback.
        assert!(r["summary"].as_str().unwrap().len() > 5);
    }
}
