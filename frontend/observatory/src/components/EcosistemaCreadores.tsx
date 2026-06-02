import React, { useEffect, useState } from 'react';
import { fetchLedgerProvenance, type MetricProvenanceResponse } from '../lib/LedgerClient';

export default function EcosistemaCreadores() {
  const [metric, setMetric] = useState<MetricProvenanceResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetchLedgerProvenance('SYBIL_GRAPH_01')
      .then(setMetric)
      .catch(err => {
        console.error(err);
        setError(err.message);
      });
  }, []);

  const nodes = [
    { id: 'Core-A', label: 'Domínguez', x: 50, y: 50, size: 24 },
    { id: 'Core-B', label: 'Bosch', x: 30, y: 70, size: 20 },
    { id: 'Core-C', label: 'Samuel', x: 70, y: 30, size: 20 },
    { id: 'Core-D', label: 'Mas', x: 70, y: 70, size: 18 },
    { id: 'Sat-1', label: 'Satélite 1', x: 20, y: 40, size: 8 },
    { id: 'Sat-2', label: 'Satélite 2', x: 80, y: 50, size: 8 },
    { id: 'Sat-3', label: 'Satélite 3', x: 40, y: 20, size: 8 },
    { id: 'Sat-4', label: 'Satélite 4', x: 50, y: 85, size: 8 },
    { id: 'Sat-5', label: 'Satélite 5', x: 85, y: 80, size: 8 },
  ];

  if (error) {
    return (
      <div className="w-full h-80 bg-red-900 border border-red-500 rounded-lg flex items-center justify-center p-6 text-red-200 font-mono text-xs">
        <div>
          <div className="font-bold text-white mb-2">🚨 EPISTEMIC FAILURE (COLLUSIVE GRAPH)</div>
          {error}
        </div>
      </div>
    );
  }

  return (
    <div className="relative w-full h-80 bg-[#0A0A0A] border border-white/10 rounded-lg overflow-hidden group">
      {/* Background Grid */}
      <div className="absolute inset-0 opacity-20" style={{ backgroundImage: 'linear-gradient(rgba(255,255,255,0.1) 1px, transparent 1px), linear-gradient(90deg, rgba(255,255,255,0.1) 1px, transparent 1px)', backgroundSize: '20px 20px' }}></div>
      
      <div className="absolute top-4 right-4 z-20 pointer-events-none">
        <span className="text-[#2B3BE5] font-bold px-2 bg-[#2B3BE5]/10 border border-[#2B3BE5]/30 rounded text-[10px] font-mono">
          [ {metric?.provenance.level || 'VERIFYING...'} ]
        </span>
      </div>

      <div className="absolute top-4 left-4 z-20 pointer-events-none">
        <div className="text-[#2B3BE5] font-mono text-xs tracking-widest uppercase font-bold bg-black/50 p-1">Grafo Colusivo</div>
        <div className="text-white/40 font-mono text-[10px] bg-black/50 p-1">Tasa de endogamia: 84.2%</div>
      </div>

      {/* Edges (SVG) */}
      <svg className="absolute inset-0 w-full h-full pointer-events-none">
        {nodes.map((nodeA, i) => 
          nodes.map((nodeB, j) => {
            if (i < j && (nodeA.size > 15 || nodeB.size > 15)) {
              return (
                <line 
                  key={`${nodeA.id}-${nodeB.id}`}
                  x1={`${nodeA.x}%`} y1={`${nodeA.y}%`} 
                  x2={`${nodeB.x}%`} y2={`${nodeB.y}%`} 
                  stroke="rgba(43, 59, 229, 0.2)" 
                  strokeWidth="1"
                  className="group-hover:stroke-[rgba(43,59,229,0.5)] transition-colors duration-1000"
                />
              );
            }
            return null;
          })
        )}
      </svg>

      {/* Nodes */}
      {nodes.map(node => (
        <div 
          key={node.id}
          className="absolute -translate-x-1/2 -translate-y-1/2 flex flex-col items-center justify-center cursor-crosshair group/node z-10"
          style={{ left: `${node.x}%`, top: `${node.y}%` }}
        >
          <div 
            className="rounded-full bg-black border-2 border-[#2B3BE5] shadow-[0_0_15px_rgba(43,59,229,0.5)] transition-all group-hover/node:bg-[#2B3BE5] group-hover/node:scale-125"
            style={{ width: `${node.size}px`, height: `${node.size}px` }}
          ></div>
          <span className="mt-2 font-mono text-[10px] text-white/50 group-hover/node:text-white transition-colors bg-black/80 px-1 rounded pointer-events-none">
            {node.label}
          </span>
        </div>
      ))}
    </div>
  );
}
