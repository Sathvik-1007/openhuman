//! RPC handlers for operator_inbox domain.
use super::{engine, types::*};
use serde_json::{json, Map, Value};
use tracing::debug;

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

    // Try LLM-powered draft generation first.
    let rec = engine::get_triage(id)?;
    let llm_content = try_llm_draft(&rec, &tone).await;

    match llm_content {
        Some(content) => {
            // LLM succeeded — store the draft.
            let draft_id = format!("dr-{}", id.get(3..).unwrap_or(id));
            engine::set_draft_content(id, &content);
            Ok(json!({"ok":true,"draft_id":draft_id,"content":content,"tone":tone,"source":"llm"}))
        }
        None => {
            // Fallback to template-based draft.
            match engine::generate_draft(id, tone) {
                Ok(d) => Ok(
                    json!({"ok":true,"draft_id":d.id,"content":d.content,"tone":d.tone,"source":"template"}),
                ),
                Err(e) => Ok(json!({"ok":false,"error":e})),
            }
        }
    }
}

/// Attempt LLM-powered draft generation. Returns None if LLM unavailable.
async fn try_llm_draft(rec: &TriageRecord, tone: &ReplyTone) -> Option<String> {
    use crate::openhuman::config::ops::load_config_with_timeout;
    use crate::openhuman::inference::provider::create_chat_provider;

    let config = load_config_with_timeout().await.ok()?;
    let (provider, model) = create_chat_provider("agentic", &config).ok()?;

    let tone_str = match tone {
        ReplyTone::Professional => "professional",
        ReplyTone::Casual => "casual",
        ReplyTone::Formal => "formal",
    };

    let prompt = format!(
        "Write a {} reply to this email. Be concise. Do not include subject line or headers.\n\nFrom: {}\nSubject: {}\nBody: {}\n\nReply:",
        tone_str, rec.sender, rec.subject, rec.body_preview
    );

    let system = "You are a professional email assistant. Write concise, contextual replies.";

    let text = provider
        .chat_with_system(Some(system), &prompt, &model, 0.6)
        .await
        .ok()?;

    debug!(triage_id = %rec.id, "[operator_inbox] LLM draft generated");
    Some(text)
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
