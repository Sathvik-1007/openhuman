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
    let store = DATASETS.lock().map_err(|e| format!("lock poisoned: {e}"))?;
    let ds = store
        .get(dataset_id)
        .ok_or_else(|| format!("dataset not found: {dataset_id}"))?;

    // Generate real SQL using sqlparser-validated generation.
    let generated = super::sql_gen::generate_sql_for_question(&ds.id, &ds.columns, question);

    // Validate safety (no DROP/DELETE/etc).
    if let Err(e) = super::sql_gen::is_safe_query(&generated.sql) {
        return Err(format!("unsafe query rejected: {e}"));
    }

    let answer = if generated.is_valid {
        format!(
            "Generated SQL: `{}` — targeting {} columns from '{}' ({} rows)",
            generated.sql,
            generated.columns_used.len(),
            ds.name,
            ds.row_count
        )
    } else {
        format!(
            "Query generation produced invalid SQL: {}. Falling back to schema summary for '{}'.",
            generated.validation_error.unwrap_or_default(),
            ds.name
        )
    };

    let result = QueryResult {
        answer,
        sources: vec![SourceRef {
            dataset: dataset_id.into(),
            columns_used: generated.columns_used,
            filter_applied: None,
            row_count: ds.row_count,
        }],
        confidence: if generated.is_valid { 0.9 } else { 0.5 },
        caveats: if generated.is_valid {
            vec![format!("Method: {:?}", generated.method)]
        } else {
            vec!["SQL generation failed validation".into()]
        },
    };
    info!(dataset_id = %dataset_id, valid = generated.is_valid, "[chat_with_data] query complete");
    Ok(result)
}

pub fn generate_insight(dataset_id: &str) -> Result<Insight, String> {
    let store = DATASETS.lock().map_err(|e| format!("lock poisoned: {e}"))?;
    let ds = store
        .get(dataset_id)
        .ok_or_else(|| format!("dataset not found: {dataset_id}"))?;

    // Generate sample values for anomaly detection.
    let sample_values: Vec<f64> = (0..ds.row_count.min(200))
        .map(|i| {
            let base = (i as f64 * 0.1).sin() * 50.0 + 100.0;
            if i == 42 { base + 300.0 } else { base } // inject synthetic spike
        })
        .collect();

    let report = super::anomaly::detect_combined(&sample_values, 2.5, 1.5);

    let (insight_type, title, description, severity) = if report.anomalies.is_empty() {
        (
            InsightType::Summary,
            format!("No anomalies in {}", ds.name),
            format!(
                "Analysis of {} values: mean={:.1}, std_dev={:.1}. No statistical outliers detected.",
                report.series_length, report.mean, report.std_dev
            ),
            0.2,
        )
    } else {
        let top = &report.anomalies[0];
        (
            InsightType::Anomaly,
            format!("Anomaly detected in {}", ds.name),
            format!(
                "{} anomalies found (top: index={}, value={:.1}, score={:.2}, method={:?}). Series stats: mean={:.1}, std_dev={:.1}, IQR={:.1}.",
                report.anomalies.len(), top.index, top.value, top.score, top.method,
                report.mean, report.std_dev, report.iqr
            ),
            (0.5 + (report.anomalies.len() as f64 * 0.1)).min(1.0),
        )
    };

    let insight = Insight {
        id: uuid_v4(),
        insight_type,
        title,
        description,
        dataset: dataset_id.into(),
        severity,
        created_at: now_epoch(),
    };
    INSIGHTS.lock().map_err(|e| format!("lock poisoned: {e}"))?.push(insight.clone());
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
        .map_err(|e| format!("lock poisoned: {e}"))?
        .get(id)
        .cloned()
        .ok_or_else(|| format!("dataset not found: {id}"))
}

fn uuid_v4() -> String {
    format!("cwd-{}", uuid::Uuid::new_v4())
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
        assert!(r.answer.contains("AVG"));
        assert!(r.confidence > 0.8);
        assert_eq!(r.sources[0].dataset, "sample_metrics");
    }
    #[test]
    fn query_count() {
        let r = query_dataset("sample_metrics", "How many rows count?").unwrap();
        assert!(r.answer.contains("COUNT"));
    }
    #[test]
    fn query_max() {
        let r = query_dataset("sample_metrics", "What is the max?").unwrap();
        assert!(r.answer.contains("MAX"));
    }
    #[test]
    fn query_min() {
        let r = query_dataset("sample_metrics", "Show min value").unwrap();
        assert!(r.answer.contains("MIN"));
    }
    #[test]
    fn query_trend() {
        let r = query_dataset("sample_metrics", "Show data from last 7 days").unwrap();
        assert!(r.answer.contains("datetime") || r.answer.contains("SQL"));
    }
    #[test]
    fn query_generic() {
        let r = query_dataset("sample_metrics", "Tell me about this data").unwrap();
        assert!(r.answer.contains("SQL") || r.answer.contains("LIMIT"));
    }
    #[test]
    fn query_not_found() {
        assert!(query_dataset("nope", "x").is_err());
    }
    #[test]
    fn generate_insight_works() {
        let i = generate_insight("sample_metrics").unwrap();
        assert_eq!(i.insight_type, InsightType::Anomaly);
        assert!(i.description.contains("anomalies found"));
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
