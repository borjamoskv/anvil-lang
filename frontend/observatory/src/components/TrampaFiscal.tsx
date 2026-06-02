import React, { useEffect, useState } from 'react';
import { fetchLedgerProvenance, type MetricProvenanceResponse } from '../lib/LedgerClient';

export default function TrampaFiscal() {
  const [metric, setMetric] = useState<MetricProvenanceResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetchLedgerProvenance('TAX_ARBITRAGE_01')
      .then(setMetric)
      .catch(err => {
        console.error(err);
        setError(err.message);
      });
  }, []);

  if (error) {
    return (
      <div className="bg-red-900 border border-red-500 rounded-lg p-6 text-red-200 font-mono text-xs overflow-x-auto">
        <div className="font-bold text-white mb-2">🚨 EPISTEMIC FAILURE (TAX ARBITRAGE)</div>
        {error}
      </div>
    );
  }

  return (
    <div className="overflow-x-auto border border-white/10 rounded-lg bg-black/40 backdrop-blur-sm relative">
      <div className="absolute top-2 right-2 z-20">
        <span className="text-[#2B3BE5] font-bold px-2 bg-[#2B3BE5]/10 border border-[#2B3BE5]/30 rounded text-[10px] font-mono">
          [ {metric?.provenance.level || 'VERIFYING...'} ]
        </span>
      </div>
      <table className="w-full text-left border-collapse text-sm font-mono mt-4">
        <thead>
          <tr className="border-b border-white/10 bg-white/5 text-[#2B3BE5]">
            <th className="p-4 font-normal tracking-widest whitespace-nowrap">ESTRUCTURA</th>
            <th className="p-4 font-normal tracking-widest whitespace-nowrap">MÉTODO TRADICIONAL</th>
            <th className="p-4 font-normal tracking-widest whitespace-nowrap">ARBITRAJE (ONG)</th>
          </tr>
        </thead>
        <tbody className="text-white/70">
          <tr className="border-b border-white/5 hover:bg-white/5 transition-colors">
            <td className="p-4 border-r border-white/5 text-white">Facturación Base</td>
            <td className="p-4 border-r border-white/5">250€ (Membresía B2B)</td>
            <td className="p-4 text-[#2B3BE5]">250€ (Donación iHelp)</td>
          </tr>
          <tr className="border-b border-white/5 hover:bg-white/5 transition-colors">
            <td className="p-4 border-r border-white/5 text-white">IVA Aplicable</td>
            <td className="p-4 border-r border-white/5 text-red-400">21% (52.50€)</td>
            <td className="p-4 text-[#2B3BE5]">Exento (0%)</td>
          </tr>
          <tr className="border-b border-white/5 hover:bg-white/5 transition-colors">
            <td className="p-4 border-r border-white/5 text-white">Deducción IRPF (Usuario)</td>
            <td className="p-4 border-r border-white/5">0€ (Gasto no deducible)</td>
            <td className="p-4 text-[#2B3BE5]">80% (200€ devueltos)</td>
          </tr>
          <tr className="hover:bg-white/5 transition-colors font-bold">
            <td className="p-4 border-r border-white/5 text-white">COSTE REAL USUARIO</td>
            <td className="p-4 border-r border-white/5 text-red-400">302.50€</td>
            <td className="p-4 text-[#2B3BE5]">50.00€</td>
          </tr>
        </tbody>
      </table>
    </div>
  );
}
