//! RPC handlers for live_captions domain.

use super::{store, types::*};
use serde_json::{json, Map, Value};

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
    match store::summarize_transcript(tid) {
        Ok(t) => Ok(json!({ "ok": true, "transcript_id": t.id, "summary": t.summary })),
        Err(e) => Ok(json!({ "ok": false, "error": e })),
    }
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
        assert!(r["summary"].as_str().unwrap().contains("1 segments"));
    }
}
