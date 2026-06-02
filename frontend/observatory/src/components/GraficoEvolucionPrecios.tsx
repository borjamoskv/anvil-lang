import React from 'react';

export default function GraficoEvolucionPrecios() {
  const data = [
    { month: 'Jul 25', price: 15, label: 'Early Bird' },
    { month: 'Oct 25', price: 50, label: 'Subida #1' },
    { month: 'Ene 26', price: 150, label: 'Subida #2' },
    { month: 'Mar 26', price: 2357, label: 'Lifetime Anchor' },
  ];

  return (
    <div className="bg-black/40 border border-white/10 rounded-lg p-6">
      <div className="flex justify-between items-end mb-8 border-b border-white/10 pb-2">
        <span className="text-[#2B3BE5] font-mono text-xs uppercase tracking-widest">Evolución Artificial de Precios (EUR)</span>
        <span className="text-white/40 font-mono text-[10px]">Sesgo de Anclaje</span>
      </div>
      <div className="flex items-end justify-between h-48 gap-2">
        {data.map((item, i) => {
          const heightPercent = Math.max((item.price / 2357) * 100, 5);
          return (
            <div key={i} className="flex flex-col items-center flex-1 group">
              <div className="text-white/50 font-mono text-[10px] mb-2 opacity-0 group-hover:opacity-100 transition-opacity">
                {item.price}€
              </div>
              <div 
                className="w-full max-w-[60px] bg-gradient-to-t from-[#2B3BE5]/20 to-[#2B3BE5] rounded-t-sm relative overflow-hidden transition-all duration-500 hover:from-[#2B3BE5]/40 hover:to-white"
                style={{ height: `${heightPercent}%` }}
              >
                <div className="absolute top-0 left-0 w-full h-[1px] bg-white shadow-[0_0_10px_#fff]"></div>
              </div>
              <div className="mt-4 text-xs font-mono text-white/40 -rotate-45 origin-top-left translate-y-2 translate-x-2">
                {item.month}
              </div>
            </div>
          );
        })}
      </div>
      <div className="h-12"></div>
    </div>
  );
}
