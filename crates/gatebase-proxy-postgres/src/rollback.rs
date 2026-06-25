use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use gatebase_audit::{JsonlRollbackSink, RollbackSink, SqliteRollbackSink};
use gatebase_config::{RollbackConfig, RollbackSinkConfig, TargetConfig};
use gatebase_core::{DbEngine, RollbackArtifact, Session};
use gatebase_session::SessionStore;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::time::{timeout, Duration};
use tokio_postgres::SimpleQueryMessage;
use uuid::Uuid;

const IO_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) async fn build_rollback_sinks(
    config: &RollbackConfig,
    store: &SessionStore,
) -> Result<Vec<Arc<dyn RollbackSink>>> {
    let mut sinks: Vec<Arc<dyn RollbackSink>> = Vec::new();
    if !config.enabled {
        return Ok(sinks);
    }
    for sink in &config.sinks {
        match sink {
            RollbackSinkConfig::Sqlite => sinks.push(Arc::new(
                SqliteRollbackSink::new(store.metadata().clone()).await?,
            )),
            RollbackSinkConfig::Jsonl { path } => {
                sinks.push(Arc::new(JsonlRollbackSink::new(path.clone())))
            }
        }
    }
    Ok(sinks)
}

#[derive(Clone)]
pub(crate) struct RollbackContext<'a> {
    pub(crate) config: &'a RollbackConfig,
    pub(crate) sinks: &'a [Arc<dyn RollbackSink>],
    pub(crate) session: &'a Session,
    pub(crate) target: &'a TargetConfig,
    pub(crate) fail_closed: bool,
}

/// Capture and persist a rollback artifact for `statement`, returning its id so
/// the caller can link the corresponding audit event to it. Returns `None` when
/// no artifact applies (rollback disabled, no sinks, or non-rollback statement).
pub(crate) async fn capture_rollback_artifact(
    statement: &str,
    upstream: &tokio_postgres::Client,
    context: &RollbackContext<'_>,
) -> Result<Option<String>> {
    if !context.config.enabled || context.sinks.is_empty() {
        return Ok(None);
    }
    let Some(request) = parse_rollback_request(statement) else {
        return Ok(None);
    };

    let artifact = match build_supported_artifact(statement, upstream, context, &request).await {
        Ok(artifact) => artifact,
        Err(error) => manual_artifact(
            statement,
            context,
            request.table.as_deref(),
            Some(error.to_string()),
        ),
    };
    write_artifact(&artifact, context).await?;
    Ok(Some(artifact.id.clone()))
}

async fn build_supported_artifact(
    statement: &str,
    upstream: &tokio_postgres::Client,
    context: &RollbackContext<'_>,
    request: &RollbackRequest,
) -> Result<RollbackArtifact> {
    let Some(table) = &request.table else {
        return Ok(manual_artifact(
            statement,
            context,
            None,
            Some("unsupported rollback SQL shape".to_owned()),
        ));
    };
    let Some(where_column) = &request.where_column else {
        return Ok(manual_artifact(
            statement,
            context,
            Some(table),
            Some("rollback requires WHERE <primary_key> IN (...)".to_owned()),
        ));
    };
    let Some(values) = &request.where_values else {
        return Ok(manual_artifact(
            statement,
            context,
            Some(table),
            Some("rollback requires WHERE <primary_key> IN (...)".to_owned()),
        ));
    };

    let select = format!(
        "SELECT * FROM {} WHERE {} IN ({}) LIMIT {}",
        quote_ident(table),
        quote_ident(where_column),
        values.join(", "),
        context.config.max_rows + 1
    );
    let rows = fetch_before_rows(upstream, &select).await?;
    if rows.len() as u64 > context.config.max_rows {
        return Ok(manual_artifact(
            statement,
            context,
            Some(table),
            Some(format!(
                "rollback row count exceeds max_rows {}",
                context.config.max_rows
            )),
        ));
    }

    let primary_key = match fetch_single_primary_key(upstream, table).await {
        Ok(primary_key) => primary_key,
        Err(error) => {
            return Ok(manual_artifact_with_rows(
                statement,
                context,
                Some(table),
                None,
                rows,
                Some(error.to_string()),
            ));
        }
    };
    if !ident_eq(where_column, &primary_key) {
        return Ok(manual_artifact_with_rows(
            statement,
            context,
            Some(table),
            Some(primary_key.clone()),
            rows,
            Some(format!(
                "WHERE column {where_column} is not primary key {primary_key}"
            )),
        ));
    }

    let inverse_sql = match request.operation {
        RollbackOperation::Delete => build_insert_inverse(table, &rows),
        RollbackOperation::Update => build_update_inverse(table, &primary_key, &rows),
    };
    let manual_required = inverse_sql.is_none();

    Ok(RollbackArtifact {
        id: new_artifact_id(),
        session_id: context.session.id.clone(),
        actor: context.session.actor.clone(),
        target: context.target.name.clone(),
        engine: DbEngine::Postgres,
        statement: statement.to_owned(),
        table: Some(table.clone()),
        primary_key_column: Some(primary_key),
        before_rows: Value::Array(rows),
        inverse_sql,
        manual_required,
        reason: None,
        created_at: Utc::now(),
    })
}

async fn write_artifact(artifact: &RollbackArtifact, context: &RollbackContext<'_>) -> Result<()> {
    for sink in context.sinks {
        if let Err(error) = sink.write(artifact).await {
            if context.fail_closed {
                return Err(error);
            }
            tracing::warn!(%error, "rollback artifact sink write failed");
        }
    }
    Ok(())
}

async fn fetch_single_primary_key(
    upstream: &tokio_postgres::Client,
    table: &str,
) -> Result<String> {
    let (schema, table_name) = split_table_name(table);
    let query = format!(
        "SELECT kcu.column_name \
         FROM information_schema.table_constraints tc \
         JOIN information_schema.key_column_usage kcu \
           ON tc.constraint_name = kcu.constraint_name \
          AND tc.table_schema = kcu.table_schema \
          AND tc.table_name = kcu.table_name \
         WHERE tc.constraint_type = 'PRIMARY KEY' \
           AND tc.table_schema = {} \
           AND tc.table_name = {} \
         ORDER BY kcu.ordinal_position",
        sql_string(schema),
        sql_string(table_name)
    );
    let messages = timeout(IO_TIMEOUT, upstream.simple_query(&query))
        .await
        .context("primary key lookup timed out")??;
    let columns: Vec<String> = messages
        .into_iter()
        .filter_map(|message| match message {
            SimpleQueryMessage::Row(row) => row.get(0).map(ToOwned::to_owned),
            _ => None,
        })
        .collect();
    match columns.as_slice() {
        [column] => Ok(column.clone()),
        [] => Err(anyhow!("table {table} has no primary key")),
        _ => Err(anyhow!("table {table} has composite primary key")),
    }
}

async fn fetch_before_rows(upstream: &tokio_postgres::Client, query: &str) -> Result<Vec<Value>> {
    let messages = timeout(IO_TIMEOUT, upstream.simple_query(query))
        .await
        .context("before-image query timed out")??;
    let mut columns: Vec<String> = Vec::new();
    let mut rows = Vec::new();
    for message in messages {
        match message {
            SimpleQueryMessage::RowDescription(description) => {
                columns = description
                    .iter()
                    .map(|column| column.name().to_owned())
                    .collect();
            }
            SimpleQueryMessage::Row(row) => {
                let mut object = serde_json::Map::new();
                for (index, column) in columns.iter().enumerate() {
                    object.insert(
                        column.clone(),
                        row.get(index).map_or(Value::Null, |value| json!(value)),
                    );
                }
                rows.push(Value::Object(object));
            }
            _ => {}
        }
    }
    Ok(rows)
}

fn parse_rollback_request(statement: &str) -> Option<RollbackRequest> {
    let statement = statement.trim().trim_end_matches(';').trim();
    let lower = statement.to_ascii_lowercase();
    if lower.starts_with("delete") {
        Some(parse_delete(statement))
    } else if lower.starts_with("update") {
        Some(parse_update(statement))
    } else {
        None
    }
}

fn parse_delete(statement: &str) -> RollbackRequest {
    let lower = statement.to_ascii_lowercase();
    let Some(after_from) = lower.find("from").map(|index| index + 4) else {
        return RollbackRequest::manual(RollbackOperation::Delete);
    };
    let rest = statement[after_from..].trim_start();
    let Some((table, after_table)) = take_ident(rest) else {
        return RollbackRequest::manual(RollbackOperation::Delete);
    };
    let where_clause = after_table.trim_start();
    let Some((column, values)) = parse_where_predicate(where_clause) else {
        return RollbackRequest::manual_with_table(RollbackOperation::Delete, table);
    };
    RollbackRequest {
        operation: RollbackOperation::Delete,
        table: Some(table),
        where_column: Some(column),
        where_values: Some(values),
    }
}

fn parse_update(statement: &str) -> RollbackRequest {
    let rest = statement["update".len()..].trim_start();
    let Some((table, after_table)) = take_ident(rest) else {
        return RollbackRequest::manual(RollbackOperation::Update);
    };
    let lower = after_table.to_ascii_lowercase();
    let Some(where_index) = lower.rfind(" where ") else {
        return RollbackRequest::manual_with_table(RollbackOperation::Update, table);
    };
    let where_clause = after_table[where_index..].trim_start();
    let Some((column, values)) = parse_where_predicate(where_clause) else {
        return RollbackRequest::manual_with_table(RollbackOperation::Update, table);
    };
    RollbackRequest {
        operation: RollbackOperation::Update,
        table: Some(table),
        where_column: Some(column),
        where_values: Some(values),
    }
}

fn parse_where_predicate(where_clause: &str) -> Option<(String, Vec<String>)> {
    let lower = where_clause.to_ascii_lowercase();
    let rest = lower.strip_prefix("where ")?;
    if let Some(in_index) = rest.find(" in ") {
        let column = where_clause["where ".len().."where ".len() + in_index]
            .trim()
            .trim_matches('"')
            .to_owned();
        let values_start = where_clause.find('(')?;
        let values_end = where_clause.rfind(')')?;
        if values_end <= values_start || !where_clause[values_end + 1..].trim().is_empty() {
            return None;
        }
        let values: Vec<String> = where_clause[values_start + 1..values_end]
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        return if values.is_empty() {
            None
        } else {
            Some((column, values))
        };
    }

    let eq_index = rest.find('=')?;
    let column = where_clause["where ".len().."where ".len() + eq_index]
        .trim()
        .trim_matches('"')
        .to_owned();
    let value = where_clause["where ".len() + eq_index + 1..].trim();
    if column.is_empty()
        || value.is_empty()
        || value.to_ascii_lowercase().contains(" and ")
        || value.to_ascii_lowercase().contains(" or ")
    {
        return None;
    }
    Some((column, vec![value.to_owned()]))
}

fn take_ident(input: &str) -> Option<(String, &str)> {
    let input = input.trim_start();
    let (first, rest) = take_ident_segment(input)?;
    let rest = rest.trim_start();
    if let Some(after_dot) = rest.strip_prefix('.') {
        let (second, rest) = take_ident_segment(after_dot)?;
        Some((format!("{first}.{second}"), rest))
    } else {
        Some((first, rest))
    }
}

fn take_ident_segment(input: &str) -> Option<(String, &str)> {
    let input = input.trim_start();
    if let Some(rest) = input.strip_prefix('"') {
        let end = rest.find('"')?;
        return Some((rest[..end].to_owned(), &rest[end + 1..]));
    }
    let end = input
        .find(|character: char| character.is_whitespace() || character == '.')
        .unwrap_or(input.len());
    let ident = input[..end].trim();
    if ident.is_empty() {
        None
    } else {
        Some((ident.to_owned(), &input[end..]))
    }
}

fn build_insert_inverse(table: &str, rows: &[Value]) -> Option<String> {
    let mut statements = Vec::new();
    for row in rows {
        let object = row.as_object()?;
        let columns: Vec<&String> = object.keys().collect();
        let values: Vec<String> = columns
            .iter()
            .map(|column| sql_value(&object[*column]))
            .collect();
        statements.push(format!(
            "INSERT INTO {} ({}) VALUES ({});",
            quote_ident(table),
            columns
                .iter()
                .map(|column| quote_ident(column))
                .collect::<Vec<_>>()
                .join(", "),
            values.join(", ")
        ));
    }
    Some(statements.join("\n"))
}

fn build_update_inverse(table: &str, primary_key: &str, rows: &[Value]) -> Option<String> {
    let mut statements = Vec::new();
    for row in rows {
        let object = row.as_object()?;
        let pk_value = object.get(primary_key)?;
        let assignments: Vec<String> = object
            .iter()
            .filter(|(column, _)| !ident_eq(column, primary_key))
            .map(|(column, value)| format!("{} = {}", quote_ident(column), sql_value(value)))
            .collect();
        statements.push(format!(
            "UPDATE {} SET {} WHERE {} = {};",
            quote_ident(table),
            assignments.join(", "),
            quote_ident(primary_key),
            sql_value(pk_value)
        ));
    }
    Some(statements.join("\n"))
}

fn manual_artifact(
    statement: &str,
    context: &RollbackContext<'_>,
    table: Option<&str>,
    reason: Option<String>,
) -> RollbackArtifact {
    RollbackArtifact {
        id: new_artifact_id(),
        session_id: context.session.id.clone(),
        actor: context.session.actor.clone(),
        target: context.target.name.clone(),
        engine: DbEngine::Postgres,
        statement: statement.to_owned(),
        table: table.map(ToOwned::to_owned),
        primary_key_column: None,
        before_rows: Value::Array(Vec::new()),
        inverse_sql: None,
        manual_required: true,
        reason,
        created_at: Utc::now(),
    }
}

fn manual_artifact_with_rows(
    statement: &str,
    context: &RollbackContext<'_>,
    table: Option<&str>,
    primary_key_column: Option<String>,
    before_rows: Vec<Value>,
    reason: Option<String>,
) -> RollbackArtifact {
    RollbackArtifact {
        id: new_artifact_id(),
        session_id: context.session.id.clone(),
        actor: context.session.actor.clone(),
        target: context.target.name.clone(),
        engine: DbEngine::Postgres,
        statement: statement.to_owned(),
        table: table.map(ToOwned::to_owned),
        primary_key_column,
        before_rows: Value::Array(before_rows),
        inverse_sql: None,
        manual_required: true,
        reason,
        created_at: Utc::now(),
    }
}

fn split_table_name(table: &str) -> (&str, &str) {
    table
        .split_once('.')
        .map_or(("public", table), |(schema, table)| (schema, table))
}

fn quote_ident(value: &str) -> String {
    value
        .split('.')
        .map(|part| format!("\"{}\"", part.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(".")
}

fn sql_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn sql_value(value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_owned(),
        Value::String(value) => sql_string(value),
        other => sql_string(&other.to_string()),
    }
}

fn ident_eq(left: &str, right: &str) -> bool {
    left.trim_matches('"')
        .eq_ignore_ascii_case(right.trim_matches('"'))
}

fn new_artifact_id() -> String {
    format!("rb_{}", Uuid::new_v4().simple())
}

struct RollbackRequest {
    operation: RollbackOperation,
    table: Option<String>,
    where_column: Option<String>,
    where_values: Option<Vec<String>>,
}

impl RollbackRequest {
    fn manual(operation: RollbackOperation) -> Self {
        Self {
            operation,
            table: None,
            where_column: None,
            where_values: None,
        }
    }

    fn manual_with_table(operation: RollbackOperation, table: String) -> Self {
        Self {
            operation,
            table: Some(table),
            where_column: None,
            where_values: None,
        }
    }
}

enum RollbackOperation {
    Delete,
    Update,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_ident_reads_qualified_identifiers() {
        assert_eq!(
            take_ident(r#""public"."cli_tokens" WHERE id = 1"#)
                .unwrap()
                .0,
            "public.cli_tokens"
        );
        assert_eq!(
            take_ident("public.users WHERE id = 1").unwrap().0,
            "public.users"
        );
        assert_eq!(take_ident(r#""users" WHERE id = 1"#).unwrap().0, "users");
        assert_eq!(take_ident("users WHERE id = 1").unwrap().0, "users");
        assert_eq!(
            split_table_name("public.cli_tokens"),
            ("public", "cli_tokens")
        );
    }

    #[test]
    fn parse_where_predicate_reads_in_and_equals() {
        assert_eq!(
            parse_where_predicate(r#"WHERE "id" = 'x'"#).unwrap(),
            ("id".to_owned(), vec!["'x'".to_owned()])
        );
        assert_eq!(
            parse_where_predicate("WHERE id IN (1,2)").unwrap(),
            ("id".to_owned(), vec!["1".to_owned(), "2".to_owned()])
        );
        assert!(parse_where_predicate("WHERE id = 1 AND other = 2").is_none());
    }

    #[test]
    fn parse_delete_reads_qualified_equals_predicate() {
        let request = parse_delete(r#"DELETE FROM "public"."cli_tokens" WHERE "id" = 'x'"#);
        assert_eq!(request.table.as_deref(), Some("public.cli_tokens"));
        assert_eq!(request.where_column.as_deref(), Some("id"));
        assert_eq!(request.where_values, Some(vec!["'x'".to_owned()]));
    }

    #[test]
    fn parse_update_reads_qualified_equals_predicate() {
        let request =
            parse_update(r#"UPDATE "public"."cli_tokens" SET name = 'y' WHERE "id" = 'x'"#);
        assert_eq!(request.table.as_deref(), Some("public.cli_tokens"));
        assert_eq!(request.where_column.as_deref(), Some("id"));
        assert_eq!(request.where_values, Some(vec!["'x'".to_owned()]));
    }

    #[test]
    fn parse_delete_keeps_unsupported_shape_manual() {
        let request =
            parse_delete(r#"DELETE FROM "public"."cli_tokens" WHERE "id" = 'x' AND name = 'y'"#);
        assert_eq!(request.table.as_deref(), Some("public.cli_tokens"));
        assert_eq!(request.where_column, None);
        assert_eq!(request.where_values, None);
    }
}
