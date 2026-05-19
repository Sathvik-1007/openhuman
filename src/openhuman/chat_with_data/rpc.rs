//! RPC handlers for chat_with_data domain.
use super::{engine, types::*};
use serde_json::{json, Map, Value};

pub async fn handle_register_dataset(p: Map<String, Value>) -> Result<Value, String> {
    let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("unnamed");
    let source = match p.get("source").and_then(|v| v.as_str()).unwrap_or("csv") {
        "json" => DataSource::Json,
        "sqlite" => DataSource::Sqlite,
        "api" => DataSource::Api,
        _ => DataSource::Csv,
    };
    let columns: Vec<String> = p
        .get("columns")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let row_count = p.get("row_count").and_then(|v| v.as_u64()).unwrap_or(0);
    let d = engine::register_dataset(name, source, columns, row_count);
    Ok(
        json!({"ok":true,"dataset_id":d.id,"name":d.name,"columns":d.columns,"row_count":d.row_count}),
    )
}

pub async fn handle_query(p: Map<String, Value>) -> Result<Value, String> {
    let id = p.get("dataset_id").and_then(|v| v.as_str()).unwrap_or("");
    let question = p.get("question").and_then(|v| v.as_str()).unwrap_or("");
    match engine::query_dataset(id, question) {
        Ok(r) => Ok(
            json!({"ok":true,"answer":r.answer,"confidence":r.confidence,"sources":r.sources.len(),"caveats":r.caveats}),
        ),
        Err(e) => Ok(json!({"ok":false,"error":e})),
    }
}

pub async fn handle_generate_insight(p: Map<String, Value>) -> Result<Value, String> {
    let id = p.get("dataset_id").and_then(|v| v.as_str()).unwrap_or("");
    match engine::generate_insight(id) {
        Ok(i) => Ok(
            json!({"ok":true,"insight_id":i.id,"type":i.insight_type,"title":i.title,"severity":i.severity}),
        ),
        Err(e) => Ok(json!({"ok":false,"error":e})),
    }
}

pub async fn handle_list_datasets(_p: Map<String, Value>) -> Result<Value, String> {
    let all: Vec<Value> = engine::list_datasets()
        .iter()
        .map(|d| json!({"id":d.id,"name":d.name,"source":d.source,"row_count":d.row_count}))
        .collect();
    Ok(json!({"ok":true,"datasets":all}))
}

pub async fn handle_list_insights(_p: Map<String, Value>) -> Result<Value, String> {
    let all: Vec<Value> = engine::list_insights()
        .iter()
        .map(|i| json!({"id":i.id,"type":i.insight_type,"title":i.title,"severity":i.severity}))
        .collect();
    Ok(json!({"ok":true,"insights":all}))
}

pub async fn handle_get_dataset(p: Map<String, Value>) -> Result<Value, String> {
    let id = p.get("dataset_id").and_then(|v| v.as_str()).unwrap_or("");
    match engine::get_dataset(id) {
        Ok(d) => Ok(
            json!({"ok":true,"dataset_id":d.id,"name":d.name,"source":d.source,"columns":d.columns,"row_count":d.row_count}),
        ),
        Err(e) => Ok(json!({"ok":false,"error":e})),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn query_rpc() {
        let mut p = Map::new();
        p.insert("dataset_id".into(), Value::String("sample_metrics".into()));
        p.insert("question".into(), Value::String("average value".into()));
        let r = handle_query(p).await.unwrap();
        assert_eq!(r["ok"], true);
        assert!(r["answer"].as_str().unwrap().contains("42.7"));
    }
    #[tokio::test]
    async fn list_datasets_rpc() {
        let r = handle_list_datasets(Map::new()).await.unwrap();
        assert_eq!(r["ok"], true);
    }
}
