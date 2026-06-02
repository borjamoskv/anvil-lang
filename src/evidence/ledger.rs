use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEvent {
    pub event_id: String,
    pub parent_event_id: Option<String>,
    pub source_type: String,
    pub source_hash: String,
    pub transformation: String,
    pub output_hash: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceSource {
    pub registry_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceObservations {
    pub count: u32,
    pub treatments: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceDerivation {
    pub method: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceInfo {
    pub level: String, // e.g., C4_ATTESTED
    pub source: ProvenanceSource,
    pub observations: ProvenanceObservations,
    pub derivation: ProvenanceDerivation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricProvenanceResponse {
    pub metric_id: String,
    pub value: f64,
    pub provenance: ProvenanceInfo,
}
