//! SQL generation and validation for chat-with-data.
//!
//! Generates SQL from natural language questions using pattern-based
//! heuristics for common aggregation queries, with sqlparser validation
//! to ensure generated SQL is syntactically correct before execution.
//!
//! ## Log prefix
//!
//! `[chat-with-data-sql]`

use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;
use tracing::{debug, warn};

/// A generated SQL query with metadata.
#[derive(Debug, Clone)]
pub struct GeneratedSql {
    /// The SQL query string.
    pub sql: String,
    /// Columns referenced in the query.
    pub columns_used: Vec<String>,
    /// Whether the SQL passed validation.
    pub is_valid: bool,
    /// Validation error if any.
    pub validation_error: Option<String>,
    /// The generation method used.
    pub method: SqlGenMethod,
}

/// How the SQL was generated.
#[derive(Debug, Clone, PartialEq)]
pub enum SqlGenMethod {
    /// Pattern-based heuristic (fast, no LLM needed).
    Pattern,
    /// Template-based with slot filling.
    Template,
    /// Fallback generic query.
    Fallback,
}

/// Validate a SQL string using sqlparser.
///
/// Returns `Ok(())` if the SQL is syntactically valid, or an error message.
pub fn validate_sql(sql: &str) -> Result<(), String> {
    if sql.trim().is_empty() {
        return Err("SQL validation failed: empty input".into());
    }
    let dialect = GenericDialect {};
    let stmts =
        Parser::parse_sql(&dialect, sql).map_err(|e| format!("SQL validation failed: {e}"))?;
    if stmts.is_empty() {
        return Err("SQL validation failed: no statements".into());
    }
    Ok(())
}

/// Generate SQL from a natural language question given a table schema.
///
/// Uses pattern matching for common aggregation queries (average, count,
/// max, min, sum, group by). Falls back to a generic SELECT when no
/// pattern matches.
pub fn generate_sql_for_question(
    table_name: &str,
    columns: &[String],
    question: &str,
) -> GeneratedSql {
    let lower = question.to_lowercase();
    let (sql, cols_used, method) =
        if let Some(result) = try_group_pattern(table_name, columns, &lower) {
            result
        } else if let Some(result) = try_aggregation_pattern(table_name, columns, &lower) {
            result
        } else if let Some(result) = try_filter_pattern(table_name, columns, &lower) {
            result
        } else {
            // Fallback: SELECT all columns with LIMIT
            let cols = if columns.is_empty() {
                "*".to_string()
            } else {
                columns.join(", ")
            };
            (
                format!("SELECT {cols} FROM {table_name} LIMIT 100"),
                columns.to_vec(),
                SqlGenMethod::Fallback,
            )
        };

    // Validate the generated SQL.
    let validation = validate_sql(&sql);
    let is_valid = validation.is_ok();
    let validation_error = validation.err();

    if !is_valid {
        warn!(
            sql = %sql,
            error = ?validation_error,
            "[chat-with-data-sql] generated invalid SQL"
        );
    } else {
        debug!(sql = %sql, method = ?method, "[chat-with-data-sql] SQL generated");
    }

    GeneratedSql {
        sql,
        columns_used: cols_used,
        is_valid,
        validation_error,
        method,
    }
}

/// Try to match aggregation patterns (average, count, max, min, sum).
fn try_aggregation_pattern(
    table: &str,
    columns: &[String],
    question: &str,
) -> Option<(String, Vec<String>, SqlGenMethod)> {
    let agg_fn =
        if question.contains("average") || question.contains("mean") || question.contains("avg") {
            "AVG"
        } else if question.contains("count")
            || question.contains("how many")
            || question.contains("total number")
        {
            "COUNT"
        } else if question.contains("maximum")
            || question.contains("max")
            || question.contains("highest")
            || question.contains("largest")
        {
            "MAX"
        } else if question.contains("minimum")
            || question.contains("min")
            || question.contains("lowest")
            || question.contains("smallest")
        {
            "MIN"
        } else if question.contains("sum") || question.contains("total") {
            "SUM"
        } else {
            return None;
        };

    // Find the most likely numeric column to aggregate.
    let target_col = find_numeric_column(columns, question);

    let sql = if agg_fn == "COUNT" {
        format!("SELECT COUNT(*) AS cnt FROM {table}")
    } else {
        format!("SELECT {agg_fn}({target_col}) AS result FROM {table}")
    };

    Some((sql, vec![target_col], SqlGenMethod::Pattern))
}

/// Try to match filter patterns (where X = Y, last N days, etc.).
fn try_filter_pattern(
    table: &str,
    columns: &[String],
    question: &str,
) -> Option<(String, Vec<String>, SqlGenMethod)> {
    // "last N days/weeks/months" pattern
    if let Some(days) = extract_time_filter(question) {
        let date_col = columns
            .iter()
            .find(|c| {
                c.contains("date")
                    || c.contains("time")
                    || c.contains("created")
                    || c.contains("updated")
            })
            .cloned()
            .unwrap_or_else(|| "created_at".to_string());

        let cols = if columns.is_empty() {
            "*".to_string()
        } else {
            columns.join(", ")
        };
        let sql = format!(
            "SELECT {cols} FROM {table} WHERE {date_col} >= datetime('now', '-{days} days') ORDER BY {date_col} DESC LIMIT 100"
        );
        return Some((sql, columns.to_vec(), SqlGenMethod::Template));
    }

    None
}

/// Try to match group-by patterns.
fn try_group_pattern(
    table: &str,
    columns: &[String],
    question: &str,
) -> Option<(String, Vec<String>, SqlGenMethod)> {
    if !(question.contains("by")
        || question.contains("per")
        || question.contains("each")
        || question.contains("group"))
    {
        return None;
    }

    // Find the grouping column (usually categorical).
    let group_col = columns
        .iter()
        .find(|c| {
            let cl = c.to_lowercase();
            cl.contains("category")
                || cl.contains("type")
                || cl.contains("status")
                || cl.contains("name")
                || cl.contains("group")
        })
        .cloned()?;

    let value_col = find_numeric_column(columns, question);
    let agg = if question.contains("count") {
        "COUNT(*)"
    } else {
        &format!("SUM({value_col})")
    };

    let sql = format!(
        "SELECT {group_col}, {agg} AS result FROM {table} GROUP BY {group_col} ORDER BY result DESC"
    );
    Some((sql, vec![group_col, value_col], SqlGenMethod::Template))
}

/// Find the most likely numeric column from the schema.
fn find_numeric_column(columns: &[String], question: &str) -> String {
    // First check if any column name is mentioned in the question.
    for col in columns {
        if question.contains(&col.to_lowercase()) {
            return col.clone();
        }
    }
    // Heuristic: prefer columns named "value", "amount", "price", "count", "total".
    let numeric_hints = [
        "value", "amount", "price", "count", "total", "quantity", "score", "revenue", "cost",
    ];
    for hint in &numeric_hints {
        if let Some(col) = columns.iter().find(|c| c.to_lowercase().contains(hint)) {
            return col.clone();
        }
    }
    // Fallback to first column or "*".
    columns.first().cloned().unwrap_or_else(|| "*".to_string())
}

/// Extract a time filter from natural language (e.g., "last 7 days" → 7).
fn extract_time_filter(question: &str) -> Option<u32> {
    // Match "last N days/weeks/months"
    let patterns = [
        ("last ", " day"),
        ("past ", " day"),
        ("last ", " week"),
        ("past ", " week"),
        ("last ", " month"),
        ("past ", " month"),
    ];

    for (prefix, suffix) in &patterns {
        if let Some(start) = question.find(prefix) {
            let after_prefix = &question[start + prefix.len()..];
            if let Some(end) = after_prefix.find(suffix) {
                let num_str = after_prefix[..end].trim();
                if let Ok(n) = num_str.parse::<u32>() {
                    let multiplier = if suffix.contains("week") {
                        7
                    } else if suffix.contains("month") {
                        30
                    } else {
                        1
                    };
                    return Some(n * multiplier);
                }
            }
        }
    }

    // "today" = 1 day, "this week" = 7 days
    if question.contains("today") {
        return Some(1);
    }
    if question.contains("this week") {
        return Some(7);
    }
    if question.contains("this month") {
        return Some(30);
    }

    None
}

/// Check if a SQL query contains dangerous operations.
pub fn is_safe_query(sql: &str) -> Result<(), String> {
    let upper = sql.to_uppercase();

    // Reject multiple statements (semicolons).
    if sql.contains(';') {
        return Err("Query contains multiple statements (semicolons not allowed)".into());
    }

    let dangerous = [
        "DROP", "DELETE", "TRUNCATE", "ALTER", "INSERT", "UPDATE", "CREATE", "EXEC", "EXECUTE",
    ];
    for keyword in &dangerous {
        // Check it's a standalone keyword, not part of a column name.
        if upper.split_whitespace().any(|w| w == *keyword) {
            return Err(format!("Query contains dangerous operation: {keyword}"));
        }
    }

    // Reject UNION-based injection attempts.
    if upper.split_whitespace().any(|w| w == "UNION") {
        return Err("Query contains UNION (not allowed for safety)".into());
    }

    // Reject subqueries (parenthesized SELECT).
    if upper.contains("(SELECT") {
        return Err("Query contains subquery (not allowed for safety)".into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_valid_sql() {
        assert!(validate_sql("SELECT * FROM users").is_ok());
        assert!(validate_sql("SELECT COUNT(*) FROM orders WHERE status = 'active'").is_ok());
        assert!(validate_sql("SELECT name, AVG(score) FROM students GROUP BY name").is_ok());
    }

    #[test]
    fn validate_invalid_sql() {
        assert!(validate_sql("SELEC * FORM users").is_err());
        assert!(validate_sql("").is_err());
        assert!(validate_sql("not sql at all").is_err());
    }

    #[test]
    fn generate_average_query() {
        let cols = vec!["date".into(), "value".into(), "category".into()];
        let result = generate_sql_for_question("metrics", &cols, "What is the average value?");
        assert!(result.is_valid);
        assert!(result.sql.contains("AVG"));
        assert!(result.sql.contains("value"));
        assert_eq!(result.method, SqlGenMethod::Pattern);
    }

    #[test]
    fn generate_count_query() {
        let cols = vec!["id".into(), "name".into()];
        let result = generate_sql_for_question("users", &cols, "How many users are there?");
        assert!(result.is_valid);
        assert!(result.sql.contains("COUNT(*)"));
        assert_eq!(result.method, SqlGenMethod::Pattern);
    }

    #[test]
    fn generate_max_query() {
        let cols = vec!["date".into(), "price".into()];
        let result = generate_sql_for_question("products", &cols, "What is the maximum price?");
        assert!(result.is_valid);
        assert!(result.sql.contains("MAX"));
        assert!(result.sql.contains("price"));
    }

    #[test]
    fn generate_time_filter_query() {
        let cols = vec!["created_at".into(), "amount".into()];
        let result = generate_sql_for_question("orders", &cols, "Show orders from last 7 days");
        assert!(result.is_valid);
        assert!(result.sql.contains("datetime"));
        assert!(result.sql.contains("-7 days"));
        assert_eq!(result.method, SqlGenMethod::Template);
    }

    #[test]
    fn generate_group_by_query() {
        let cols = vec!["category".into(), "amount".into()];
        let result = generate_sql_for_question("sales", &cols, "Total amount by category");
        assert!(result.is_valid);
        assert!(result.sql.contains("GROUP BY"));
        assert!(result.sql.contains("category"));
        assert_eq!(result.method, SqlGenMethod::Template);
    }

    #[test]
    fn generate_fallback_query() {
        let cols = vec!["a".into(), "b".into()];
        let result = generate_sql_for_question("data", &cols, "Show me everything");
        assert!(result.is_valid);
        assert!(result.sql.contains("LIMIT 100"));
        assert_eq!(result.method, SqlGenMethod::Fallback);
    }

    #[test]
    fn safety_check_blocks_dangerous() {
        assert!(is_safe_query("DROP TABLE users").is_err());
        assert!(is_safe_query("DELETE FROM orders").is_err());
        assert!(is_safe_query("SELECT * FROM users").is_ok());
        // Column named "drop_count" should NOT trigger.
        assert!(is_safe_query("SELECT drop_count FROM metrics").is_ok());
    }

    #[test]
    fn safety_check_blocks_semicolons() {
        assert!(is_safe_query("SELECT 1; DROP TABLE users").is_err());
        assert!(is_safe_query("SELECT * FROM t;").is_err());
    }

    #[test]
    fn safety_check_blocks_union() {
        assert!(is_safe_query("SELECT * FROM users UNION SELECT * FROM secrets").is_err());
    }

    #[test]
    fn time_filter_extraction() {
        assert_eq!(extract_time_filter("last 7 days"), Some(7));
        assert_eq!(extract_time_filter("past 2 weeks"), Some(14));
        assert_eq!(extract_time_filter("last 3 months"), Some(90));
        assert_eq!(extract_time_filter("today"), Some(1));
        assert_eq!(extract_time_filter("this week"), Some(7));
        assert_eq!(extract_time_filter("random text"), None);
    }

    #[test]
    fn empty_columns_handled() {
        let result = generate_sql_for_question("t", &[], "count everything");
        assert!(result.is_valid);
    }

    #[test]
    fn safety_check_blocks_exec_and_subqueries() {
        assert!(is_safe_query("EXEC sp_executesql @sql").is_err());
        assert!(is_safe_query("EXECUTE xp_cmdshell 'dir'").is_err());
        assert!(is_safe_query("SELECT * FROM (SELECT password FROM users)").is_err());
    }
}
