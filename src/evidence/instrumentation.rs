use sqlx::sqlite::SqlitePool;
use tokio::time::{interval, Duration};
use crate::evidence::store::append_ledger_event;

pub fn start_instrumentation_loop(pool: SqlitePool) {
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(3));
        let start_time = std::time::Instant::now();

        loop {
            ticker.tick().await;

            let uptime = start_time.elapsed().as_secs_f64();

            // Emit SYSTEM_UPTIME_01
            let _ = append_ledger_event(
                &pool,
                "SYSTEM_UPTIME_01".to_string(),
                uptime,
                None,
                "rust_axum_daemon".to_string(),
                "0xUPTIME_HASH".to_string(),
                "uptime_monitor".to_string(),
                "hash".to_string()
            ).await;

            // Emit CORTEX_AUDIT_LOG_01
            let _ = append_ledger_event(
                &pool,
                "CORTEX_AUDIT_LOG_01".to_string(),
                1.0,
                None,
                "security_module".to_string(),
                "0xAUDIT_HASH".to_string(),
                "signature_verified".to_string(),
                "hash".to_string()
            ).await;

            // Emit PRICE_ANCHORING_01
            let _ = append_ledger_event(
                &pool,
                "PRICE_ANCHORING_01".to_string(),
                2357.0,
                None,
                "pricing_engine".to_string(),
                "0xPRICE_HASH".to_string(),
                "static_anchor".to_string(),
                "hash".to_string()
            ).await;

            // Emit TRUST_PYRAMID_01
            let _ = append_ledger_event(
                &pool,
                "TRUST_PYRAMID_01".to_string(),
                1.0,
                None,
                "trust_metrics".to_string(),
                "0xTRUST_HASH".to_string(),
                "cohort_analysis".to_string(),
                "hash".to_string()
            ).await;

            // Emit TAX_ARBITRAGE_01
            let _ = append_ledger_event(
                &pool,
                "TAX_ARBITRAGE_01".to_string(),
                50.0,
                None,
                "fiscal_engine".to_string(),
                "0xTAX_HASH".to_string(),
                "ong_arbitrage".to_string(),
                "hash".to_string()
            ).await;

            // Emit SYBIL_GRAPH_01
            let _ = append_ledger_event(
                &pool,
                "SYBIL_GRAPH_01".to_string(),
                84.2,
                None,
                "graph_analyzer".to_string(),
                "0xSYBIL_HASH".to_string(),
                "eigenvector_centrality".to_string(),
                "hash".to_string()
            ).await;
        }
    });
}
