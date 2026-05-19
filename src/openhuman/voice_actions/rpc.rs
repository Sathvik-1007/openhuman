//! RPC handlers for voice_actions domain.

use super::engine;
use serde_json::{json, Map, Value};

pub async fn handle_recognize(p: Map<String, Value>) -> Result<Value, String> {
    let utterance = p.get("utterance").and_then(|v| v.as_str()).unwrap_or("");
    match engine::recognize_intent(utterance) {
        Ok(i) => Ok(json!({
            "ok": true, "intent_id": i.id, "action": i.action,
            "namespace": i.namespace, "function": i.function,
            "confidence": i.confidence, "safety": i.safety, "status": i.status,
        })),
        Err(e) => Ok(json!({ "ok": false, "error": e })),
    }
}

pub async fn handle_confirm(p: Map<String, Value>) -> Result<Value, String> {
    let id = p.get("intent_id").and_then(|v| v.as_str()).unwrap_or("");
    match engine::confirm_intent(id) {
        Ok(i) => Ok(json!({ "ok": true, "intent_id": i.id, "status": i.status })),
        Err(e) => Ok(json!({ "ok": false, "error": e })),
    }
}

pub async fn handle_reject(p: Map<String, Value>) -> Result<Value, String> {
    let id = p.get("intent_id").and_then(|v| v.as_str()).unwrap_or("");
    match engine::reject_intent(id) {
        Ok(i) => Ok(json!({ "ok": true, "intent_id": i.id, "status": i.status })),
        Err(e) => Ok(json!({ "ok": false, "error": e })),
    }
}

pub async fn handle_get_intent(p: Map<String, Value>) -> Result<Value, String> {
    let id = p.get("intent_id").and_then(|v| v.as_str()).unwrap_or("");
    match engine::get_intent(id) {
        Ok(i) => Ok(json!({
            "ok": true, "intent_id": i.id, "utterance": i.utterance,
            "action": i.action, "status": i.status, "safety": i.safety,
            "result": i.result, "error": i.error,
        })),
        Err(e) => Ok(json!({ "ok": false, "error": e })),
    }
}

pub async fn handle_list_mappings(_p: Map<String, Value>) -> Result<Value, String> {
    let mappings: Vec<Value> = engine::list_mappings()
        .iter()
        .map(|m| {
            json!({
                "pattern": m.pattern, "namespace": m.namespace,
                "function": m.function, "safety": m.safety, "description": m.description,
            })
        })
        .collect();
    Ok(json!({ "ok": true, "mappings": mappings }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn recognize_rpc() {
        let mut p = Map::new();
        p.insert("utterance".into(), Value::String("open settings".into()));
        let r = handle_recognize(p).await.unwrap();
        assert_eq!(r["ok"], true);
        assert_eq!(r["namespace"], "config");
    }

    #[tokio::test]
    async fn recognize_unknown_rpc() {
        let mut p = Map::new();
        p.insert("utterance".into(), Value::String("xyz abc".into()));
        let r = handle_recognize(p).await.unwrap();
        assert_eq!(r["ok"], false);
    }

    #[tokio::test]
    async fn list_mappings_rpc() {
        let r = handle_list_mappings(Map::new()).await.unwrap();
        assert_eq!(r["ok"], true);
        assert!(r["mappings"].as_array().unwrap().len() >= 8);
    }
}
