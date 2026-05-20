//! RPC handlers for guided_flows domain.

use serde_json::{json, Map, Value};
use tracing::debug;

use super::engine;

pub async fn handle_list_flows(_p: Map<String, Value>) -> Result<Value, String> {
    let flows = engine::list_flows();
    let list: Vec<Value> = flows
        .iter()
        .map(|f| {
            json!({
                "id": f.id,
                "name": f.name,
                "description": f.description,
                "version": f.version,
                "step_count": f.steps.len(),
            })
        })
        .collect();
    Ok(json!({ "ok": true, "flows": list }))
}

pub async fn handle_start_flow(p: Map<String, Value>) -> Result<Value, String> {
    let flow_id = p.get("flow_id").and_then(|v| v.as_str()).unwrap_or("");
    let session_id = p
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(String::from);

    match engine::start_flow(flow_id, session_id) {
        Ok(s) => Ok(json!({
            "ok": true,
            "session_id": s.session_id,
            "flow_id": s.flow_id,
            "current_step": s.current_step,
            "state": s.state,
        })),
        Err(e) => Ok(json!({ "ok": false, "error": e })),
    }
}

pub async fn handle_submit_answer(p: Map<String, Value>) -> Result<Value, String> {
    let session_id = p.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
    let step_id = p.get("step_id").and_then(|v| v.as_str()).unwrap_or("");
    let value = p.get("value").cloned().unwrap_or(Value::Null);

    match engine::submit_answer(session_id, step_id, value) {
        Ok(s) => {
            let mut resp = json!({
                "ok": true,
                "session_id": s.session_id,
                "state": s.state,
                "current_step": s.current_step,
            });
            if let Some(rec) = &s.recommendation {
                // Enhance recommendation with LLM-generated personalized summary.
                let personalized = try_llm_personalize(rec, &s.answers).await;
                resp["recommendation"] = json!({
                    "title": rec.title,
                    "summary": personalized.as_deref().unwrap_or(&rec.summary),
                    "confidence": rec.confidence,
                    "next_actions": rec.next_actions,
                });
            }
            Ok(resp)
        }
        Err(e) => Ok(json!({ "ok": false, "error": e })),
    }
}

/// LLM-powered personalization of flow recommendations based on user answers.
async fn try_llm_personalize(
    rec: &super::types::Recommendation,
    answers: &[super::types::StepAnswer],
) -> Option<String> {
    use crate::openhuman::config::ops::load_config_with_timeout;
    use crate::openhuman::inference::provider::create_chat_provider;

    let config = load_config_with_timeout().await.ok()?;
    let (provider, model) = create_chat_provider("agentic", &config).ok()?;

    let answers_text: String = answers
        .iter()
        .map(|a| format!("- {}: {}", a.step_id, serde_json::to_string(&a.value).unwrap_or_default()))
        .collect::<Vec<_>>()
        .join("\n");

    let prompt = format!(
        "Based on these user preferences, write a personalized 2-3 sentence recommendation summary.\n\nRecommendation: {}\nUser answers:\n{}\nNext actions: {}\n\nPersonalized summary:",
        rec.title, answers_text, rec.next_actions.join(", ")
    );

    let system = "You are a setup assistant. Write warm, personalized recommendations that reference the user's specific choices. Be concise and actionable.";

    let text = provider
        .chat_with_system(Some(system), &prompt, &model, 0.6)
        .await
        .ok()?;

    debug!("[guided_flows] LLM personalization generated");
    Some(text.trim().to_string())
}

pub async fn handle_get_session(p: Map<String, Value>) -> Result<Value, String> {
    let session_id = p.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
    match engine::get_session(session_id) {
        Ok(s) => {
            let mut resp = json!({
                "ok": true,
                "session_id": s.session_id,
                "flow_id": s.flow_id,
                "state": s.state,
                "current_step": s.current_step,
                "answers_count": s.answers.len(),
            });
            if let Some(rec) = &s.recommendation {
                resp["recommendation"] = json!({
                    "title": rec.title,
                    "summary": rec.summary,
                    "confidence": rec.confidence,
                    "next_actions": rec.next_actions,
                });
            }
            Ok(resp)
        }
        Err(e) => Ok(json!({ "ok": false, "error": e })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn list_flows_rpc_returns_ok() {
        let resp = handle_list_flows(Map::new()).await.unwrap();
        assert_eq!(resp["ok"], true);
        assert!(resp["flows"].as_array().unwrap().len() > 0);
    }

    #[tokio::test]
    async fn start_flow_rpc_returns_session() {
        let mut p = Map::new();
        p.insert("flow_id".into(), Value::String("onboarding_setup".into()));
        p.insert("session_id".into(), Value::String("rpc-t1".into()));
        let resp = handle_start_flow(p).await.unwrap();
        assert_eq!(resp["ok"], true);
        assert_eq!(resp["session_id"], "rpc-t1");
    }

    #[tokio::test]
    async fn start_flow_rpc_bad_id() {
        let mut p = Map::new();
        p.insert("flow_id".into(), Value::String("nope".into()));
        let resp = handle_start_flow(p).await.unwrap();
        assert_eq!(resp["ok"], false);
    }

    #[tokio::test]
    async fn submit_answer_rpc_advances() {
        let mut p = Map::new();
        p.insert("flow_id".into(), Value::String("onboarding_setup".into()));
        p.insert("session_id".into(), Value::String("rpc-t2".into()));
        handle_start_flow(p).await.unwrap();

        let mut p = Map::new();
        p.insert("session_id".into(), Value::String("rpc-t2".into()));
        p.insert("step_id".into(), Value::String("use_case".into()));
        p.insert(
            "value".into(),
            Value::String("Personal productivity".into()),
        );
        let resp = handle_submit_answer(p).await.unwrap();
        assert_eq!(resp["ok"], true);
        assert_eq!(resp["current_step"], "privacy_pref");
    }

    #[tokio::test]
    async fn get_session_rpc_works() {
        let mut p = Map::new();
        p.insert("flow_id".into(), Value::String("onboarding_setup".into()));
        p.insert("session_id".into(), Value::String("rpc-t3".into()));
        handle_start_flow(p).await.unwrap();

        let mut p = Map::new();
        p.insert("session_id".into(), Value::String("rpc-t3".into()));
        let resp = handle_get_session(p).await.unwrap();
        assert_eq!(resp["ok"], true);
        assert_eq!(resp["state"], "active");
    }
}
