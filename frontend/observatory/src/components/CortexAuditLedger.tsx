import React, { useEffect, useState } from 'react';
import { fetchLedgerProvenance, type MetricProvenanceResponse } from '../lib/LedgerClient';

const MOCK_LOGS = [
  "0x4F... SIGNATURE VERIFIED",
  "DECAY RATE: +0.02% AT NODE 'DOMINGUEZ'",
  "CROSS-RECOMMENDATION ANOMALY DETECTED",
  "SYBIL ATTACK VECTOR: MITIGATED",
  "TRAFFIC BOUNCE: 84% (CRITICAL DECAY)",
  "INJECTING C5-REAL TRUTH PAYLOAD",
];

export default function CortexAuditLedger() {
  const [logs, setLogs] = useState<string[]>([]);
  const [metric, setMetric] = useState<MetricProvenanceResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetchLedgerProvenance('CORTEX_AUDIT_LOG_01')
      .then(setMetric)
      .catch(err => {
        console.error(err);
        setError(err.message);
      });
  }, []);

  useEffect(() => {
    let i = 0;
    const interval = setInterval(() => {
      setLogs(prev => {
        const newLogs = [...prev, `[${new Date().toISOString()}] ${MOCK_LOGS[Math.floor(Math.random() * MOCK_LOGS.length)]}`];
        if (newLogs.length > 8) newLogs.shift();
        return newLogs;
      });
    }, 1500);
    return () => clearInterval(interval);
  }, []);

  if (error) {
    return (
      <div className="bg-red-900 border border-red-500 rounded-lg p-4 font-mono text-xs text-red-200">
        <div className="font-bold text-white mb-2">🚨 C5-REAL EPISTEMIC FAILURE</div>
        {error}
      </div>
    );
  }

  return (
    <div className="bg-black border border-white/10 rounded-lg p-4 font-mono text-xs overflow-hidden relative shadow-2xl">
      <div className="absolute top-0 left-0 w-full h-1 bg-gradient-to-r from-transparent via-[#2B3BE5] to-transparent opacity-50"></div>
      <div className="flex justify-between mb-4 border-b border-white/10 pb-2">
        <span className="text-[#2B3BE5] uppercase tracking-widest">forensis.log</span>
        <span className="text-[#2B3BE5] font-bold px-2 bg-[#2B3BE5]/10 border border-[#2B3BE5]/30 rounded">
          [ {metric?.provenance.level || 'VERIFYING...'} ]
        </span>
        <span className="text-white/30">C5-REAL NODE</span>
      </div>
      <div className="space-y-1 h-40 flex flex-col justify-end">
        {logs.map((log, i) => (
          <div key={i} className="text-[#2B3BE5]/80 animate-fade-in truncate">
            <span className="text-white/30 mr-2">&gt;</span>{log}
          </div>
        ))}
      </div>
    </div>
  );
}
