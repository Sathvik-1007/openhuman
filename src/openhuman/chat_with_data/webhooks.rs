//! Webhook event notifications for chat_with_data insights.
//!
//! Fires HTTP POST to registered webhook URLs when anomalies or insights are detected.

use std::collections::HashMap;
use std::sync::Mutex;
use tracing::{debug, info, warn};

use super::types::Insight;

const LOG_PREFIX: &str = "[cwd-webhooks]";
const MAX_HOOKS: usize = 20;

static HOOKS: std::sync::LazyLock<Mutex<HashMap<String, WebhookConfig>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone)]
pub struct WebhookConfig {
    pub id: String,
    pub url: String,
    pub events: Vec<WebhookEvent>,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WebhookEvent {
    AnomalyDetected,
    InsightGenerated,
    ThresholdBreached,
}

/// Validate that a webhook URL does not target private/loopback addresses (SSRF protection).
fn validate_webhook_url(url: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| format!("invalid URL: {e}"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        s => return Err(format!("webhook url must be http/https, got: {s}")),
    }
    let host = parsed.host_str().unwrap_or("");
    if host.is_empty() {
        return Err("URL has no host".into());
    }
    let lower = host.to_lowercase();
    if lower == "localhost" || lower == "::1" {
        return Err("loopback addresses are not allowed".into());
    }
    if let Ok(ip) = host.parse::<std::net::Ipv4Addr>() {
        if ip.is_loopback()
            || ip.octets()[0] == 10
            || (ip.octets()[0] == 172 && (ip.octets()[1] & 0xf0) == 16)
            || (ip.octets()[0] == 192 && ip.octets()[1] == 168)
            || (ip.octets()[0] == 169 && ip.octets()[1] == 254)
        {
            return Err("private/link-local addresses are not allowed".into());
        }
    }
    if let Ok(ip) = host.parse::<std::net::Ipv6Addr>() {
        if ip.is_loopback() {
            return Err("loopback addresses are not allowed".into());
        }
    }
    Ok(())
}

/// Register a webhook endpoint.
pub fn register_webhook(url: &str, events: Vec<WebhookEvent>) -> Result<String, String> {
    validate_webhook_url(url)?;
    let mut store = HOOKS.lock().map_err(|e| format!("lock: {e}"))?;
    if store.len() >= MAX_HOOKS {
        return Err("max webhooks reached".into());
    }
    let id = format!("wh-{}", crate::openhuman::util::uuid_v4());
    let host = reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_else(|| "redacted".into());
    store.insert(
        id.clone(),
        WebhookConfig {
            id: id.clone(),
            url: url.into(),
            events,
            active: true,
        },
    );
    info!("{LOG_PREFIX} registered webhook {id} -> host={host}");
    Ok(id)
}

/// Remove a webhook.
pub fn unregister_webhook(id: &str) -> Result<(), String> {
    HOOKS
        .lock()
        .map_err(|e| format!("lock: {e}"))?
        .remove(id)
        .map(|_| ())
        .ok_or("webhook not found".into())
}

/// List registered webhooks.
pub fn list_webhooks() -> Vec<WebhookConfig> {
    HOOKS
        .lock()
        .map(|s| s.values().cloned().collect())
        .unwrap_or_default()
}

/// Fire webhook notifications for an insight event.
/// Spawns async HTTP POST tasks — does not block.
pub fn notify_insight(insight: &Insight, event: WebhookEvent) {
    let hooks: Vec<WebhookConfig> = HOOKS
        .lock()
        .map(|s| {
            s.values()
                .filter(|h| h.active && h.events.contains(&event))
                .cloned()
                .collect()
        })
        .unwrap_or_default();

    if hooks.is_empty() {
        return;
    }

    let payload = serde_json::json!({
        "event": format!("{:?}", event),
        "insight_id": insight.id,
        "title": insight.title,
        "dataset": insight.dataset,
        "severity": format!("{:?}", insight.severity),
        "description": insight.description,
        "timestamp": crate::openhuman::util::now_epoch(),
    });

    for hook in hooks {
        let payload = payload.clone();
        let url = hook.url.clone();
        let host = reqwest::Url::parse(&url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_else(|| "redacted".into());
        tokio::spawn(async move {
            debug!("{LOG_PREFIX} firing to host={host}");
            match reqwest::Client::new()
                .post(&url)
                .json(&payload)
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
            {
                Ok(resp) => debug!("{LOG_PREFIX} host={host} responded {}", resp.status()),
                Err(e) => warn!("{LOG_PREFIX} host={host} failed: {e}"),
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_list() {
        let id = register_webhook(
            "http://example.com:9999/hook",
            vec![WebhookEvent::AnomalyDetected],
        )
        .unwrap();
        assert!(id.starts_with("wh-"));
        let hooks = list_webhooks();
        assert!(hooks.iter().any(|h| h.id == id));
        unregister_webhook(&id).unwrap();
    }

    #[test]
    fn rejects_localhost() {
        let r = register_webhook(
            "http://localhost:9999/hook",
            vec![WebhookEvent::AnomalyDetected],
        );
        assert!(r.is_err());
    }
}
