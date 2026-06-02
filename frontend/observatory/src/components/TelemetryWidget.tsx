import React, { useEffect, useState } from 'react';
import { fetchLedgerProvenance, MetricProvenanceResponse } from '../lib/LedgerClient';

export default function TelemetryWidget() {
  const [uptime, setUptime] = useState(0);
  const [metric, setMetric] = useState<MetricProvenanceResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const interval = setInterval(() => setUptime(prev => prev + 1), 1000);
    return () => clearInterval(interval);
  }, []);

  useEffect(() => {
    // Fetch provenance for a system-level metric (e.g., telemetry)
    fetchLedgerProvenance('SYSTEM_UPTIME_01')
      .then(setMetric)
      .catch(err => {
        console.error(err);
        setError(err.message);
      });
  }, []);

  if (error) {
    // Hard fail the UI if there is an epistemic violation or mock data
    return (
      <div className="fixed bottom-6 right-6 bg-red-900/90 border border-red-500 p-4 rounded z-50 font-mono text-xs text-white max-w-sm pointer-events-auto">
        <div className="font-bold mb-2">🚨 C5-REAL EPISTEMIC FAILURE</div>
        <div className="text-red-200">{error}</div>
      </div>
    );
  }

  return (
    <div className="fixed bottom-6 right-6 bg-[#0A0A0A]/80 backdrop-blur-md border border-[#2B3BE5]/30 p-3 rounded flex flex-col gap-2 z-50 font-mono text-[10px] text-white/70 shadow-[0_0_15px_rgba(43,59,229,0.2)] pointer-events-none">
      <div className="text-[#2B3BE5] font-bold mb-1 border-b border-[#2B3BE5]/30 pb-1 text-center">
        [ {metric?.provenance.level || 'VERIFYING...'} ]
      </div>
      <div className="flex items-center gap-2">
        <div className="w-2 h-2 rounded-full bg-[#2B3BE5] animate-pulse"></div>
        <span className="tracking-widest text-[#2B3BE5] font-bold">C5-REAL LINK</span>
      </div>
      <div className="flex justify-between gap-4">
        <span>UPTIME:</span>
        <span className="text-white">{uptime}s</span>
      </div>
      <div className="flex justify-between gap-4">
        <span>DATA_ORIGIN:</span>
        <span className="text-white">{metric ? (metric.data_origin.ledger ? 'LEDGER' : 'UNKNOWN') : 'AWAITING'}</span>
      </div>
    </div>
  );
}
