use crate::evidence::store::append_ledger_event;
use sha2::{Digest, Sha256};
use sqlx::sqlite::SqlitePool;
use tokio::time::{Duration, interval};

pub fn start_instrumentation_loop(pool: SqlitePool) {
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(3));
        let start_time = std::time::Instant::now();
        loop {
            ticker.tick().await;
            let uptime = start_time.elapsed().as_secs_f64();
            let digest = hex::encode(Sha256::digest(uptime.to_bits().to_be_bytes()));
            if let Err(error) = append_ledger_event(
                &pool,
                "SYSTEM_UPTIME_01".to_string(),
                uptime,
                None,
                "runtime_observation".to_string(),
                digest.clone(),
                "uptime_monitor".to_string(),
                digest,
            )
            .await
            {
                tracing::warn!(%error, "Unable to append runtime observation");
            }
        }
    });
}
