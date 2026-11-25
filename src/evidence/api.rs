use crate::engine::saas::AppState;
use crate::evidence::ledger::{
    DataOrigin, MetricProvenanceResponse, ProvenanceDerivation, ProvenanceInfo,
    ProvenanceObservations, ProvenanceSource,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};

// C5-REAL: This fulfills the Phase C Provenance API Contract.
pub async fn get_provenance(
    State(state): State<AppState>,
    Path(metric_id): Path<String>,
) -> impl IntoResponse {
    let event = match crate::evidence::store::get_ledger_event(&state.pool, &metric_id).await {
        Ok(Some(e)) => e,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                "Metric provenance not found in ledger",
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Database error: {}", e),
            )
                .into_response();
        }
    };

    // Persisted does not mean attested. Legacy/demo rows cannot acquire authority.
    let observed = event.source_type == "runtime_observation";
    let response = MetricProvenanceResponse {
        metric_id: event.metric_id,
        value: event.metric_value,
        provenance: ProvenanceInfo {
            level: if observed {
                "OBSERVED_UNATTESTED"
            } else {
                "LEGACY_UNATTESTED"
            }
            .to_string(),
            source: ProvenanceSource {
                registry_id: event.source_type,
            },
            observations: ProvenanceObservations {
                count: None,
                treatments: None,
            },
            derivation: ProvenanceDerivation {
                method: event.transformation,
                timestamp: event.timestamp,
            },
        },
        data_origin: DataOrigin {
            ledger: true,
            mock: !observed,
            replay: false,
        },
    };

    Json(response).into_response()
}
