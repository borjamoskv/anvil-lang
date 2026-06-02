export interface DataOrigin {
  ledger: boolean;
  mock: boolean;
  replay: boolean;
}

export interface ProvenanceInfo {
  level: string;
  source: { registry_id: string };
  observations: { count: number; treatments: number };
  derivation: { method: string; timestamp: string };
}

export interface MetricProvenanceResponse {
  metric_id: string;
  value: number;
  provenance: ProvenanceInfo;
  data_origin: DataOrigin;
}

/**
 * C5-REAL: Strict Data Origin Verification
 * If the data is mocked or not from the ledger, the UI must FAIL to render.
 */
export async function fetchLedgerProvenance(metricId: string): Promise<MetricProvenanceResponse> {
  // If running in the browser during dev, point to the Rust backend
  // For production, they will be on the same domain
  const API_URL = import.meta.env.DEV ? 'http://localhost:3000' : '';
  
  const res = await fetch(`${API_URL}/api/provenance/${metricId}`);
  
  if (!res.ok) {
    throw new Error(`[LedgerClient] Failed to fetch metric ${metricId}: ${res.status} ${res.statusText}`);
  }

  const data: MetricProvenanceResponse = await res.json();

  if (data.data_origin.mock === true) {
    throw new Error(`[EPISTEMIC_VIOLATION] Metric ${metricId} returned MOCK data. Rendering aborted.`);
  }

  if (data.data_origin.ledger === false) {
    throw new Error(`[EPISTEMIC_VIOLATION] Metric ${metricId} did not originate from the Traceability Ledger.`);
  }

  return data;
}
