use sqlx::sqlite::SqlitePool;
use uuid::Uuid;
use chrono::Utc;
use crate::evidence::ledger::LedgerEvent;

pub async fn initialize_ledger_table(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS traceability_ledger (
            event_id TEXT PRIMARY KEY,
            parent_event_id TEXT,
            source_type TEXT NOT NULL,
            source_hash TEXT NOT NULL,
            transformation TEXT NOT NULL,
            output_hash TEXT NOT NULL,
            timestamp TEXT NOT NULL
        )"
    )
    .execute(pool)
    .await?;

    Ok(())
}

// C5-REAL: Append-only ledger interface. No updates allowed.
pub async fn append_ledger_event(
    pool: &SqlitePool,
    parent_event_id: Option<String>,
    source_type: String,
    source_hash: String,
    transformation: String,
    output_hash: String,
) -> Result<LedgerEvent, sqlx::Error> {
    let event_id = Uuid::new_v4().to_string();
    let timestamp = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO traceability_ledger (event_id, parent_event_id, source_type, source_hash, transformation, output_hash, timestamp)
         VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&event_id)
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
        parent_event_id,
        source_type,
        source_hash,
        transformation,
        output_hash,
        timestamp,
    })
}

// Read an event by ID
pub async fn get_ledger_event(pool: &SqlitePool, event_id: &str) -> Result<Option<LedgerEvent>, sqlx::Error> {
    use sqlx::Row;
    let record = sqlx::query(
        "SELECT event_id, parent_event_id, source_type, source_hash, transformation, output_hash, timestamp
        FROM traceability_ledger
        WHERE event_id = ?"
    )
    .bind(event_id)
    .fetch_optional(pool)
    .await?
    .map(|row| LedgerEvent {
        event_id: row.get("event_id"),
        parent_event_id: row.get("parent_event_id"),
        source_type: row.get("source_type"),
        source_hash: row.get("source_hash"),
        transformation: row.get("transformation"),
        output_hash: row.get("output_hash"),
        timestamp: row.get("timestamp"),
    });

    Ok(record)
}
