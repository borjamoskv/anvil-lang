import React, { useEffect, useState } from 'react';

export default function TelemetryWidget() {
  const [uptime, setUptime] = useState(0);

  useEffect(() => {
    const interval = setInterval(() => setUptime(prev => prev + 1), 1000);
    return () => clearInterval(interval);
  }, []);

  return (
    <div className="fixed bottom-6 right-6 bg-[#0A0A0A]/80 backdrop-blur-md border border-[#2B3BE5]/30 p-3 rounded flex flex-col gap-2 z-50 font-mono text-[10px] text-white/70 shadow-[0_0_15px_rgba(43,59,229,0.2)] pointer-events-none">
      <div className="text-[#FF3366] font-bold blink mb-1 border-b border-[#FF3366]/30 pb-1 text-center">[ C4-SIM: SYNTHETIC ]</div>
      <div className="flex items-center gap-2">
        <div className="w-2 h-2 rounded-full bg-[#2B3BE5] animate-pulse"></div>
        <span className="tracking-widest text-[#2B3BE5] font-bold">C5-REAL LINK</span>
      </div>
      <div className="flex justify-between gap-4">
        <span>UPTIME:</span>
        <span className="text-white">{uptime}s</span>
      </div>
      <div className="flex justify-between gap-4">
        <span>ENTROPY:</span>
        <span className="text-white">0.024%</span>
      </div>
    </div>
  );
}
