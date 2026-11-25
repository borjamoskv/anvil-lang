use crate::evidence::ledger::LedgerEvent;
use chrono::Utc;
use sqlx::sqlite::SqlitePool;
use uuid::Uuid;

pub async fn initialize_ledger_table(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // Production initialization must preserve all existing evidence.
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

    for statement in [
        "CREATE TRIGGER IF NOT EXISTS traceability_no_update BEFORE UPDATE ON traceability_ledger BEGIN SELECT RAISE(ABORT, 'append-only ledger'); END",
        "CREATE TRIGGER IF NOT EXISTS traceability_no_delete BEFORE DELETE ON traceability_ledger BEGIN SELECT RAISE(ABORT, 'append-only ledger'); END",
    ] {
        sqlx::query(statement).execute(pool).await?;
    }
    Ok(())
}

// Append-only ledger interface. No updates allowed.
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

#[cfg(test)]
mod omega_tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    #[tokio::test]
    async fn omega_ledger_survives_reopen_and_rejects_mutation() {
        let directory = tempfile::tempdir().unwrap();
        let options = SqliteConnectOptions::new()
            .filename(directory.path().join("ledger.db"))
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new().max_connections(1)
            .connect_with(options.clone()).await.unwrap();
        initialize_ledger_table(&pool).await.unwrap();
        append_ledger_event(&pool, "test".into(), 1.0, None, "test".into(),
            "source".into(), "test".into(), "output".into()).await.unwrap();
        pool.close().await;
        let reopened = SqlitePoolOptions::new().max_connections(1)
            .connect_with(options).await.unwrap();
        initialize_ledger_table(&reopened).await.unwrap();
        assert!(get_ledger_event(&reopened, "test").await.unwrap().is_some());
        assert!(sqlx::query("UPDATE traceability_ledger SET metric_value = 2").execute(&reopened).await.is_err());
        assert!(sqlx::query("DELETE FROM traceability_ledger").execute(&reopened).await.is_err());
    }
}
