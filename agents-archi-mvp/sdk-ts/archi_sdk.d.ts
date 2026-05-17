export interface TraceData {
    agentId: string;
    domain: string;
    timestamp: number;
    inputs: any;
    outputs: any;
    toolCalls: any[];
}
export declare class ArchiTrace {
    private agentId;
    private domain;
    private inputs;
    private outputs;
    private toolCalls;
    private startTime;
    constructor(agentId: string, domain?: string, inputs?: any);
    addToolCall(toolName: string, params: any, result: any): void;
    finalize(outputs: any): string;
    /**
     * Anchors the trace hash directly on-chain using EVM JSON-RPC (C5-REAL).
     * Uses default local Anvil credentials unless environment variables are provided.
     */
    anchorOnChain(traceHash: string, rpcUrl?: string, contractAddress?: string, privateKey?: string): Promise<string | null>;
}
//# sourceMappingURL=archi_sdk.d.ts.map