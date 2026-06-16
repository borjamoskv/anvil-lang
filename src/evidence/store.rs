use crate::evidence::ledger::LedgerEvent;
use chrono::Utc;
use sqlx::sqlite::SqlitePool;
use uuid::Uuid;

pub async fn initialize_ledger_table(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query("DROP TABLE IF EXISTS traceability_ledger")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS traceability_ledger (
            event_id TEXT PRIMARY KEY,
            metric_id TEXT NOT NULL,
            metric_value REAL NOT NULL,
            parent_event_id TEXT,
            source_type TEXT NOT NULL,
            source_hash TEXT NOT NULL,
            transformation TEXT NOT NULL,
            output_hash TEXT NOT NULL,
            timestamp TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_traceability_metric_id ON traceability_ledger (metric_id)",
    )
    .execute(pool)
    .await?;

    Ok(())
}

// C5-REAL: Append-only ledger interface. No updates allowed.
#[allow(clippy::too_many_arguments)]
pub async fn append_ledger_event(
    pool: &SqlitePool,
    metric_id: String,
    metric_value: f64,
    parent_event_id: Option<String>,
    source_type: String,
    source_hash: String,
    transformation: String,
    output_hash: String,
) -> Result<LedgerEvent, sqlx::Error> {
    let event_id = Uuid::new_v4().to_string();
    let timestamp = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO traceability_ledger (event_id, metric_id, metric_value, parent_event_id, source_type, source_hash, transformation, output_hash, timestamp)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&event_id)
    .bind(&metric_id)
    .bind(metric_value)
    .bind(&parent_event_id)
    .bind(&source_type)
    .bind(&source_hash)
    .bind(&transformation)
    .bind(&output_hash)
    .bind(&timestamp)
    .execute(pool)
    .await?;

    Ok(LedgerEvent {
        event_id,
        metric_id,
        metric_value,
        parent_event_id,
        source_type,
        source_hash,
        transformation,
        output_hash,
        timestamp,
    })
}

// Read the latest event for a metric stream
pub async fn get_ledger_event(
    pool: &SqlitePool,
    metric_id: &str,
) -> Result<Option<LedgerEvent>, sqlx::Error> {
    use sqlx::Row;
    let record = sqlx::query(
        "SELECT event_id, metric_id, metric_value, parent_event_id, source_type, source_hash, transformation, output_hash, timestamp
        FROM traceability_ledger
        WHERE metric_id = ?
        ORDER BY timestamp DESC
        LIMIT 1"
    )
    .bind(metric_id)
    .fetch_optional(pool)
    .await?
    .map(|row| LedgerEvent {
        event_id: row.get("event_id"),
        metric_id: row.get("metric_id"),
        metric_value: row.get("metric_value"),
        parent_event_id: row.get("parent_event_id"),
        source_type: row.get("source_type"),
        source_hash: row.get("source_hash"),
        transformation: row.get("transformation"),
        output_hash: row.get("output_hash"),
        timestamp: row.get("timestamp"),
    });

    Ok(record)
}
