import React, { useEffect, useState } from 'react';
import { fetchLedgerProvenance, type MetricProvenanceResponse } from '../lib/LedgerClient';

export default function PiramideDesconfianza() {
  const [metric, setMetric] = useState<MetricProvenanceResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetchLedgerProvenance('TRUST_PYRAMID_01')
      .then(setMetric)
      .catch(err => {
        console.error(err);
        setError(err.message);
      });
  }, []);

  const tiers = [
    { name: "CÍRCULO INTERNO", price: "3.000€+", desc: "Acceso y Estatus (Masterminds)", color: "from-[#2B3BE5] to-white" },
    { name: "EL BOOTCAMP", price: "500€-1.000€", desc: "Escasez Artificial", color: "from-[#2B3BE5]/80 to-[#2B3BE5]" },
    { name: "LA MEMBRESÍA", price: "25€/mes", desc: "Recurrencia (Contenido Traducido)", color: "from-[#2B3BE5]/50 to-[#2B3BE5]/80" },
    { name: "EL CEBO", price: "0€", desc: "Captura Masiva de Emails", color: "from-[#2B3BE5]/20 to-[#2B3BE5]/50" },
  ];

  if (error) {
    return (
      <div className="flex flex-col items-center justify-center p-8 bg-red-900 border border-red-500 rounded-lg overflow-hidden text-red-200 font-mono text-xs">
        <div className="font-bold text-white mb-2">🚨 EPISTEMIC FAILURE (TRUST PYRAMID)</div>
        {error}
      </div>
    );
  }

  return (
    <div className="flex flex-col items-center justify-center p-8 bg-black/40 border border-white/10 rounded-lg overflow-hidden relative">
      <div className="absolute top-4 right-4 z-20">
        <span className="text-[#2B3BE5] font-bold px-2 bg-[#2B3BE5]/10 border border-[#2B3BE5]/30 rounded text-[10px] font-mono">
          [ {metric?.provenance.level || 'VERIFYING...'} ]
        </span>
      </div>
      <div className="w-full max-w-md space-y-1 relative">
        {/* Ambient Glow */}
        <div className="absolute inset-0 bg-[#2B3BE5]/10 blur-3xl rounded-full"></div>
        
        {tiers.map((tier, i) => {
          const width = `${100 - (i * 20)}%`;
          return (
            <div key={i} className="flex flex-col items-center relative z-10 group cursor-crosshair">
              <div 
                className={`h-16 flex flex-col items-center justify-center bg-gradient-to-t ${tier.color} border border-white/20 rounded-sm shadow-lg transition-transform group-hover:scale-105`}
                style={{ width }}
              >
                <span className="font-mono font-bold text-black text-xs md:text-sm tracking-wider mix-blend-plus-lighter">{tier.name}</span>
                <span className="font-mono text-[10px] text-black/70 font-semibold">{tier.price}</span>
              </div>
              {/* Tooltip */}
              <div className="absolute top-1/2 left-full ml-4 -translate-y-1/2 opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none w-48 z-20">
                <div className="bg-[#0A0A0A] border border-[#2B3BE5]/40 text-white/70 text-[10px] font-mono p-2 rounded shadow-2xl">
                  {tier.desc}
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
