import React from 'react';

export default function TrampaFiscal() {
  return (
    <div className="overflow-x-auto border border-white/10 rounded-lg bg-black/40 backdrop-blur-sm">
      <table className="w-full text-left border-collapse text-sm font-mono">
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
