//! RPC handlers for operator_inbox domain.
use super::{engine, types::*};
use serde_json::{json, Map, Value};

pub async fn handle_triage_message(p: Map<String, Value>) -> Result<Value, String> {
    let source = match p.get("source").and_then(|v| v.as_str()).unwrap_or("email") {
        "chat" => MessageSource::Chat,
        "social" => MessageSource::Social,
        "webhook" => MessageSource::Webhook,
        _ => MessageSource::Email,
    };
    let sender = p.get("sender").and_then(|v| v.as_str()).unwrap_or("");
    let subject = p.get("subject").and_then(|v| v.as_str()).unwrap_or("");
    let body = p.get("body").and_then(|v| v.as_str()).unwrap_or("");
    let r = engine::triage_message(source, sender, subject, body);
    Ok(
        json!({"ok":true,"triage_id":r.id,"priority":r.priority,"reason":r.reason,"status":r.status}),
    )
}

pub async fn handle_generate_draft(p: Map<String, Value>) -> Result<Value, String> {
    let id = p.get("triage_id").and_then(|v| v.as_str()).unwrap_or("");
    let tone = match p
        .get("tone")
        .and_then(|v| v.as_str())
        .unwrap_or("professional")
    {
        "casual" => ReplyTone::Casual,
        "formal" => ReplyTone::Formal,
        _ => ReplyTone::Professional,
    };
    match engine::generate_draft(id, tone) {
        Ok(d) => Ok(json!({"ok":true,"draft_id":d.id,"content":d.content,"tone":d.tone})),
        Err(e) => Ok(json!({"ok":false,"error":e})),
    }
}

pub async fn handle_schedule_followup(p: Map<String, Value>) -> Result<Value, String> {
    let id = p.get("triage_id").and_then(|v| v.as_str()).unwrap_or("");
    let at = p.get("follow_up_at").and_then(|v| v.as_u64()).unwrap_or(0);
    match engine::schedule_followup(id, at) {
        Ok(r) => Ok(json!({"ok":true,"triage_id":r.id,"follow_up_at":r.follow_up_at})),
        Err(e) => Ok(json!({"ok":false,"error":e})),
    }
}

pub async fn handle_get_triage(p: Map<String, Value>) -> Result<Value, String> {
    let id = p.get("triage_id").and_then(|v| v.as_str()).unwrap_or("");
    match engine::get_triage(id) {
        Ok(r) => Ok(
            json!({"ok":true,"triage_id":r.id,"priority":r.priority,"status":r.status,"subject":r.subject}),
        ),
        Err(e) => Ok(json!({"ok":false,"error":e})),
    }
}

pub async fn handle_list_triage(_p: Map<String, Value>) -> Result<Value, String> {
    let all: Vec<Value> = engine::list_triage()
        .iter()
        .map(|r| json!({"id":r.id,"priority":r.priority,"status":r.status,"subject":r.subject}))
        .collect();
    Ok(json!({"ok":true,"records":all}))
}

pub async fn handle_archive(p: Map<String, Value>) -> Result<Value, String> {
    let id = p.get("triage_id").and_then(|v| v.as_str()).unwrap_or("");
    match engine::archive_triage(id) {
        Ok(r) => Ok(json!({"ok":true,"triage_id":r.id,"status":r.status})),
        Err(e) => Ok(json!({"ok":false,"error":e})),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn triage_rpc() {
        let mut p = Map::new();
        p.insert("sender".into(), Value::String("test@x.com".into()));
        p.insert("subject".into(), Value::String("URGENT help".into()));
        p.insert("body".into(), Value::String("Need help now".into()));
        let r = handle_triage_message(p).await.unwrap();
        assert_eq!(r["ok"], true);
        assert_eq!(r["priority"], "urgent");
    }
    #[tokio::test]
    async fn list_rpc() {
        let r = handle_list_triage(Map::new()).await.unwrap();
        assert_eq!(r["ok"], true);
    }
}
