export const CortexFriction = {
  ping: (source: string, action: string) => {
    if (typeof window !== 'undefined') {
      console.log(`[C5-REAL TELEMETRY] Source: ${source} | Action: ${action} | Status: NOMINAL`);
    }
  }
};
