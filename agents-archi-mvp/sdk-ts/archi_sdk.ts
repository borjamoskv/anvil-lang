import * as crypto from 'crypto';
import { ethers } from 'ethers';

export interface TraceData {
    agentId: string;
    domain: string;
    timestamp: number;
    inputs: any;
    outputs: any;
    toolCalls: any[];
}

export class ArchiTrace {
    private agentId: string;
    private domain: string;
    private inputs: any;
    private outputs: any;
    private toolCalls: any[];
    private startTime: number;

    constructor(agentId: string, domain: string = "general", inputs: any = null) {
        this.agentId = agentId;
        this.domain = domain;
        this.inputs = inputs;
        this.outputs = null;
        this.toolCalls = [];
        this.startTime = Date.now() / 1000.0;
        console.log(`[Archi-SDK] Initialized trace for agent: ${this.agentId}`);
    }

    public addToolCall(toolName: string, params: any, result: any): void {
        this.toolCalls.push({
            tool_name: toolName,
            params: params,
            result: result,
            timestamp: Date.now() / 1000.0
        });
        console.log(`[Archi-SDK] Logged tool call: ${toolName}`);
    }

    public finalize(outputs: any): string {
        this.outputs = outputs;
        
        const traceData: TraceData = {
            agentId: this.agentId,
            domain: this.domain,
            timestamp: this.startTime,
            inputs: this.inputs,
            outputs: this.outputs,
            toolCalls: this.toolCalls
        };

        const traceString = JSON.stringify(traceData, Object.keys(traceData).sort());
        const hash = crypto.createHash('sha256').update(traceString).digest('hex');
        
        console.log(`[Archi-SDK] Finalized trace. Hash: ${hash}`);
        return hash;
    }

    /**
     * Anchors the trace hash directly on-chain using EVM JSON-RPC (C5-REAL).
     * Uses default local Anvil credentials unless environment variables are provided.
     */
    public async anchorOnChain(
        traceHash: string,
        rpcUrl: string = process.env.ARCHI_RPC_URL || "http://127.0.0.1:8545",
        contractAddress: string = process.env.ARCHI_CONTRACT_ADDRESS || "0x5FbDB2315678afecb367f032d93F642f64180aa3",
        privateKey: string = process.env.ARCHI_PRIVATE_KEY || "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
    ): Promise<string | null> {
        try {
            console.log(`[Archi-SDK] Anchoring trace on-chain...`);
            console.log(`[Archi-SDK] RPC URL: ${rpcUrl}`);
            console.log(`[Archi-SDK] Contract: ${contractAddress}`);

            const provider = new ethers.JsonRpcProvider(rpcUrl);
            const wallet = new ethers.Wallet(privateKey, provider);

            const abi = [
                "function anchorTrace(bytes32 _traceHash, string calldata _agentId, string calldata _domain) external",
                "function verifyTrace(bytes32 _traceHash) external view returns (bool)"
            ];

            const contract = new ethers.Contract(contractAddress, abi, wallet) as any;

            // Format hash to bytes32 format (0x...)
            const formattedHash = traceHash.startsWith("0x") ? traceHash : `0x${traceHash}`;

            console.log(`[Archi-SDK] Sending anchor transaction for hash ${formattedHash}...`);
            const tx = await contract.anchorTrace(formattedHash, this.agentId, this.domain);
            console.log(`[Archi-SDK] Transaction sent. Hash: ${tx.hash}`);

            const receipt = await tx.wait();
            if (receipt && receipt.status === 1) {
                console.log(`[Archi-SDK] [C5-REAL] Anchor successful in block ${receipt.blockNumber}`);
                return tx.hash;
            } else {
                console.error(`[Archi-SDK] Transaction failed.`);
                return null;
            }
        } catch (error: any) {
            console.error(`[Archi-SDK] [ERROR] Failed to anchor trace: ${error.message}`);
            return null;
        }
    }
}
