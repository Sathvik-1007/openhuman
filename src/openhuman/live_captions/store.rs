//! In-memory transcript store with caption streaming.

use std::collections::HashMap;
use std::sync::Mutex;
use tracing::{debug, info, warn};

use super::types::*;
use crate::openhuman::util::now_epoch;

/// Maximum transcripts before LRU eviction.
const MAX_TRANSCRIPTS: usize = 100;

static TRANSCRIPTS: std::sync::LazyLock<Mutex<HashMap<String, Transcript>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn start_transcript(
    id: Option<String>,
    source: CaptionSource,
    title: Option<String>,
) -> Result<Transcript, String> {
    let tid = id.unwrap_or_else(uuid_v4);
    let now = now_epoch();
    let t = Transcript {
        id: tid.clone(),
        source,
        state: TranscriptState::Recording,
        title,
        segments: Vec::new(),
        summary: None,
        created_at: now,
        updated_at: now,
    };
    let mut store = TRANSCRIPTS.lock().unwrap_or_else(|e| e.into_inner());
    if store.contains_key(&tid) {
        return Err(format!("transcript already exists: {tid}"));
    }
    // Evict oldest completed transcripts if at capacity.
    if store.len() >= MAX_TRANSCRIPTS {
        let oldest = store
            .iter()
            .filter(|(_, t)| t.state == TranscriptState::Completed)
            .min_by_key(|(_, t)| t.updated_at)
            .map(|(id, _)| id.clone());
        if let Some(old_id) = oldest {
            warn!(evicted = %old_id, "[live_captions] evicting oldest transcript (at capacity)");
            store.remove(&old_id);
        }
    }
    if store.len() >= MAX_TRANSCRIPTS {
        return Err("transcript store at capacity".into());
    }
    store.insert(tid, t.clone());
    info!(transcript_id = %t.id, "[live_captions] transcript started");
    Ok(t)
}

pub fn append_segment(transcript_id: &str, segment: CaptionSegment) -> Result<Transcript, String> {
    debug!(transcript_id = %transcript_id, text_len = segment.text.len(), "[live_captions] segment appended");
    let mut store = TRANSCRIPTS
        .lock()
        .map_err(|e| format!("lock poisoned: {e}"))?;
    let t = store
        .get_mut(transcript_id)
        .ok_or_else(|| format!("transcript not found: {transcript_id}"))?;
    if t.state != TranscriptState::Recording {
        return Err("transcript is not recording".into());
    }
    t.segments.push(segment);
    t.updated_at = now_epoch();
    Ok(t.clone())
}

pub fn pause_transcript(transcript_id: &str) -> Result<Transcript, String> {
    let mut store = TRANSCRIPTS
        .lock()
        .map_err(|e| format!("lock poisoned: {e}"))?;
    let t = store
        .get_mut(transcript_id)
        .ok_or_else(|| format!("transcript not found: {transcript_id}"))?;
    if t.state != TranscriptState::Recording {
        return Err("transcript is not recording".into());
    }
    t.state = TranscriptState::Paused;
    t.updated_at = now_epoch();
    Ok(t.clone())
}

pub fn resume_transcript(transcript_id: &str) -> Result<Transcript, String> {
    let mut store = TRANSCRIPTS
        .lock()
        .map_err(|e| format!("lock poisoned: {e}"))?;
    let t = store
        .get_mut(transcript_id)
        .ok_or_else(|| format!("transcript not found: {transcript_id}"))?;
    if t.state != TranscriptState::Paused {
        return Err("transcript is not paused".into());
    }
    t.state = TranscriptState::Recording;
    t.updated_at = now_epoch();
    Ok(t.clone())
}

pub fn complete_transcript(transcript_id: &str) -> Result<Transcript, String> {
    let mut store = TRANSCRIPTS
        .lock()
        .map_err(|e| format!("lock poisoned: {e}"))?;
    let t = store
        .get_mut(transcript_id)
        .ok_or_else(|| format!("transcript not found: {transcript_id}"))?;
    t.state = TranscriptState::Completed;
    t.updated_at = now_epoch();
    info!(transcript_id = %transcript_id, "[live_captions] transcript completed");
    Ok(t.clone())
}

pub fn summarize_transcript(transcript_id: &str) -> Result<Transcript, String> {
    info!(transcript_id = %transcript_id, "[live_captions] summarizing");
    let mut store = TRANSCRIPTS
        .lock()
        .map_err(|e| format!("lock poisoned: {e}"))?;
    let t = store
        .get_mut(transcript_id)
        .ok_or_else(|| format!("transcript not found: {transcript_id}"))?;
    if t.state != TranscriptState::Completed {
        return Err("transcript must be completed before summarizing".into());
    }
    // Simple extractive summary: first and last segments + word count
    let full = t.full_text();
    let word_count = full.split_whitespace().count();
    let duration_s = t.duration_ms() / 1000;
    let summary = format!(
        "Transcript ({} words, {}s). {} segments from {:?} source.",
        word_count,
        duration_s,
        t.segments.len(),
        t.source
    );
    t.summary = Some(summary);
    t.updated_at = now_epoch();
    Ok(t.clone())
}

pub fn get_transcript(transcript_id: &str) -> Result<Transcript, String> {
    TRANSCRIPTS
        .lock()
        .map_err(|e| format!("lock poisoned: {e}"))?
        .get(transcript_id)
        .cloned()
        .ok_or_else(|| format!("transcript not found: {transcript_id}"))
}

pub fn list_transcripts() -> Vec<Transcript> {
    TRANSCRIPTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .values()
        .cloned()
        .collect()
}

/// Search transcripts by text content. Returns transcripts containing the query.
pub fn search_transcripts(query: &str) -> Vec<Transcript> {
    let lower_query = query.to_lowercase();
    TRANSCRIPTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .values()
        .filter(|t| {
            t.segments
                .iter()
                .any(|s| s.text.to_lowercase().contains(&lower_query))
                || t.title
                    .as_ref()
                    .map_or(false, |title| title.to_lowercase().contains(&lower_query))
        })
        .cloned()
        .collect()
}

/// Set summary directly (used when LLM generates the summary).
pub fn set_summary(transcript_id: &str, summary: &str) {
    if let Some(t) = TRANSCRIPTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_mut(transcript_id)
    {
        t.summary = Some(summary.to_string());
        t.updated_at = now_epoch();
    }
}

fn uuid_v4() -> String {
    format!("lc-{}", crate::openhuman::util::uuid_v4())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_creates_transcript() {
        let t = start_transcript(Some("st-1".into()), CaptionSource::Microphone, None).unwrap();
        assert_eq!(t.id, "st-1");
        assert_eq!(t.state, TranscriptState::Recording);
        assert!(t.segments.is_empty());
    }

    #[test]
    fn append_segment_works() {
        start_transcript(Some("st-2".into()), CaptionSource::Microphone, None).unwrap();
        let seg = CaptionSegment {
            text: "Hello".into(),
            start_ms: 0,
            end_ms: 500,
            speaker: None,
            confidence: 0.9,
            is_final: true,
        };
        let t = append_segment("st-2", seg).unwrap();
        assert_eq!(t.segments.len(), 1);
        assert_eq!(t.segments[0].text, "Hello");
    }

    #[test]
    fn append_to_nonexistent_errors() {
        assert!(append_segment(
            "nope",
            CaptionSegment {
                text: "x".into(),
                start_ms: 0,
                end_ms: 0,
                speaker: None,
                confidence: 0.0,
                is_final: true,
            }
        )
        .is_err());
    }

    #[test]
    fn pause_and_resume() {
        start_transcript(Some("st-3".into()), CaptionSource::Microphone, None).unwrap();
        let t = pause_transcript("st-3").unwrap();
        assert_eq!(t.state, TranscriptState::Paused);
        // Can't append while paused
        assert!(append_segment(
            "st-3",
            CaptionSegment {
                text: "x".into(),
                start_ms: 0,
                end_ms: 0,
                speaker: None,
                confidence: 0.0,
                is_final: true,
            }
        )
        .is_err());
        let t = resume_transcript("st-3").unwrap();
        assert_eq!(t.state, TranscriptState::Recording);
    }

    #[test]
    fn complete_and_summarize() {
        start_transcript(
            Some("st-4".into()),
            CaptionSource::MeetCall,
            Some("Meeting".into()),
        )
        .unwrap();
        append_segment(
            "st-4",
            CaptionSegment {
                text: "First point".into(),
                start_ms: 0,
                end_ms: 2000,
                speaker: Some("Alice".into()),
                confidence: 0.95,
                is_final: true,
            },
        )
        .unwrap();
        append_segment(
            "st-4",
            CaptionSegment {
                text: "Second point".into(),
                start_ms: 2000,
                end_ms: 4000,
                speaker: Some("Bob".into()),
                confidence: 0.9,
                is_final: true,
            },
        )
        .unwrap();
        let t = complete_transcript("st-4").unwrap();
        assert_eq!(t.state, TranscriptState::Completed);
        let t = summarize_transcript("st-4").unwrap();
        assert!(t.summary.is_some());
        assert!(t.summary.unwrap().contains("2 segments"));
    }

    #[test]
    fn summarize_requires_completed() {
        start_transcript(Some("st-5".into()), CaptionSource::Microphone, None).unwrap();
        assert!(summarize_transcript("st-5").is_err());
    }

    #[test]
    fn get_transcript_works() {
        start_transcript(Some("st-6".into()), CaptionSource::SystemAudio, None).unwrap();
        let t = get_transcript("st-6").unwrap();
        assert_eq!(t.source, CaptionSource::SystemAudio);
    }

    #[test]
    fn get_transcript_not_found() {
        assert!(get_transcript("nope").is_err());
    }

    #[test]
    fn list_transcripts_returns_all() {
        start_transcript(Some("st-7".into()), CaptionSource::Microphone, None).unwrap();
        let all = list_transcripts();
        assert!(all.iter().any(|t| t.id == "st-7"));
    }
}
