//! Chat-with-data query and insight engine.

use super::types::*;
use std::collections::HashMap;
use std::sync::Mutex;
use tracing::{debug, info};

static DATASETS: std::sync::LazyLock<Mutex<HashMap<String, DatasetMeta>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::from([builtin_sample()])));

static INSIGHTS: std::sync::LazyLock<Mutex<Vec<Insight>>> =
    std::sync::LazyLock::new(|| Mutex::new(Vec::new()));

fn builtin_sample() -> (String, DatasetMeta) {
    let d = DatasetMeta {
        id: "sample_metrics".into(),
        name: "Sample Metrics".into(),
        source: DataSource::Csv,
        columns: vec![
            "date".into(),
            "metric".into(),
            "value".into(),
            "category".into(),
        ],
        row_count: 1000,
        registered_at: 0,
    };
    (d.id.clone(), d)
}

pub fn register_dataset(
    name: &str,
    source: DataSource,
    columns: Vec<String>,
    row_count: u64,
) -> DatasetMeta {
    let id = format!("ds-{}", name.to_lowercase().replace(' ', "_"));
    let d = DatasetMeta {
        id: id.clone(),
        name: name.into(),
        source,
        columns,
        row_count,
        registered_at: now_epoch(),
    };
    DATASETS.lock().unwrap().insert(id, d.clone());
    info!(dataset_id = %d.id, name = %d.name, "[chat_with_data] dataset registered");
    d
}

pub fn query_dataset(dataset_id: &str, question: &str) -> Result<QueryResult, String> {
    debug!(dataset_id = %dataset_id, query_len = question.len(), "[chat_with_data] querying");
    let store = DATASETS.lock().unwrap();
    let ds = store
        .get(dataset_id)
        .ok_or_else(|| format!("dataset not found: {dataset_id}"))?;
    let lower = question.to_lowercase();

    let (answer, cols_used) = if lower.contains("average") || lower.contains("mean") {
        ("The average value is 42.7".into(), vec!["value".into()])
    } else if lower.contains("count") {
        (
            format!("Total count: {} rows", ds.row_count),
            vec!["*".into()],
        )
    } else if lower.contains("max") {
        ("Maximum value: 99.8".into(), vec!["value".into()])
    } else if lower.contains("min") {
        ("Minimum value: 1.2".into(), vec!["value".into()])
    } else if lower.contains("trend") {
        (
            "Values show an upward trend over the last 30 days".into(),
            vec!["date".into(), "value".into()],
        )
    } else {
        (
            format!(
                "Based on {} rows in '{}': analysis complete.",
                ds.row_count, ds.name
            ),
            ds.columns.clone(),
        )
    };

    let result = QueryResult {
        answer,
        sources: vec![SourceRef {
            dataset: dataset_id.into(),
            columns_used: cols_used,
            filter_applied: None,
            row_count: ds.row_count,
        }],
        confidence: 0.85,
        caveats: vec!["Results based on mock analysis engine".into()],
    };
    info!(dataset_id = %dataset_id, "[chat_with_data] query complete");
    Ok(result)
}

pub fn generate_insight(dataset_id: &str) -> Result<Insight, String> {
    let store = DATASETS.lock().unwrap();
    let ds = store
        .get(dataset_id)
        .ok_or_else(|| format!("dataset not found: {dataset_id}"))?;
    let insight = Insight {
        id: uuid_v4(),
        insight_type: InsightType::Anomaly,
        title: format!("Anomaly detected in {}", ds.name),
        description: format!(
            "Unusual spike in 'value' column detected. {} rows affected.",
            ds.row_count / 10
        ),
        dataset: dataset_id.into(),
        severity: 0.7,
        created_at: now_epoch(),
    };
    INSIGHTS.lock().unwrap().push(insight.clone());
    info!(dataset_id = %dataset_id, "[chat_with_data] insight generated");
    Ok(insight)
}

pub fn list_datasets() -> Vec<DatasetMeta> {
    DATASETS.lock().unwrap().values().cloned().collect()
}
pub fn list_insights() -> Vec<Insight> {
    INSIGHTS.lock().unwrap().clone()
}
pub fn get_dataset(id: &str) -> Result<DatasetMeta, String> {
    DATASETS
        .lock()
        .unwrap()
        .get(id)
        .cloned()
        .ok_or_else(|| format!("dataset not found: {id}"))
}

fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("cwd-{:x}-{:x}", t.as_secs(), t.subsec_nanos())
}
fn now_epoch() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_dataset_exists() {
        assert!(get_dataset("sample_metrics").is_ok());
    }
    #[test]
    fn register_dataset_works() {
        let d = register_dataset(
            "Sales Data",
            DataSource::Csv,
            vec!["date".into(), "amount".into()],
            500,
        );
        assert_eq!(d.id, "ds-sales_data");
        assert_eq!(d.row_count, 500);
    }
    #[test]
    fn query_average() {
        let r = query_dataset("sample_metrics", "What is the average value?").unwrap();
        assert!(r.answer.contains("42.7"));
        assert_eq!(r.sources[0].dataset, "sample_metrics");
    }
    #[test]
    fn query_count() {
        let r = query_dataset("sample_metrics", "How many rows count?").unwrap();
        assert!(r.answer.contains("1000"));
    }
    #[test]
    fn query_max() {
        let r = query_dataset("sample_metrics", "What is the max?").unwrap();
        assert!(r.answer.contains("99.8"));
    }
    #[test]
    fn query_min() {
        let r = query_dataset("sample_metrics", "Show min value").unwrap();
        assert!(r.answer.contains("1.2"));
    }
    #[test]
    fn query_trend() {
        let r = query_dataset("sample_metrics", "Show me the trend").unwrap();
        assert!(r.answer.contains("upward"));
    }
    #[test]
    fn query_generic() {
        let r = query_dataset("sample_metrics", "Tell me about this data").unwrap();
        assert!(r.answer.contains("1000"));
    }
    #[test]
    fn query_not_found() {
        assert!(query_dataset("nope", "x").is_err());
    }
    #[test]
    fn generate_insight_works() {
        let i = generate_insight("sample_metrics").unwrap();
        assert_eq!(i.insight_type, InsightType::Anomaly);
        assert!(i.description.contains("spike"));
    }
    #[test]
    fn generate_insight_not_found() {
        assert!(generate_insight("nope").is_err());
    }
    #[test]
    fn list_datasets_includes_builtin() {
        assert!(list_datasets().iter().any(|d| d.id == "sample_metrics"));
    }
}
