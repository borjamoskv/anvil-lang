import hashlib
import json
import time
from typing import Dict, Any, List

class ArchiTrace:
    """
    Archi-Trace: The Flight Recorder for Autonomous Agents.
    Captures deterministic execution trails for C5-REAL verification.
    """
    
    def __init__(self, agent_id: str, task_name: str, logic_hash: str = None):
        self.agent_id = agent_id
        self.task_name = task_name
        self.logic_hash = logic_hash
        self.timestamp_start = time.time()
        
        # State vectors
        self.inputs: Dict[str, Any] = {}
        self.tool_calls: List[Dict[str, Any]] = []
        self.outputs: Dict[str, Any] = {}
        self.status = "INITIALIZED"

    @staticmethod
    def compute_logic_hash(file_path: str) -> str:
        """Computes SHA-256 of the source code file."""
        with open(file_path, "rb") as f:
            return hashlib.sha256(f.read()).hexdigest()

    def record_input(self, data: Dict[str, Any]):
        """Register the initial deterministic inputs (prompts, raw data)."""
        self.inputs.update(data)
        
    def record_tool_call(self, tool_name: str, arguments: Dict[str, Any], result: str):
        """Register a specific action taken by the agent."""
        self.tool_calls.append({
            "tool": tool_name,
            "arguments": arguments,
            "result": result,
            "timestamp": time.time()
        })
        
    def finalize(self, output_data: Dict[str, Any], success: bool = True):
        """Close the trace and prepare for hashing."""
        self.outputs = output_data
        self.status = "SUCCESS" if success else "FAILED"
        self.timestamp_end = time.time()

    def generate_proof(self) -> str:
        """
        Synthesize the execution trace into a single deterministic SHA-256 hash.
        This represents the 'Archi-Trace-Hash'.
        """
        if self.status == "INITIALIZED":
            raise ValueError("Trace not finalized.")
            
        payload = {
            "agent_id": self.agent_id,
            "task": self.task_name,
            "logic_hash": self.logic_hash,
            "inputs": self.inputs,
            "tool_calls": self.tool_calls,
            "outputs": self.outputs,
            "status": self.status
        }
        
        # Serialize deterministically (sorted keys, no spaces)
        serialized_payload = json.dumps(payload, sort_keys=True, separators=(',', ':'))
        
        # Generate Cryptographic Hash (SHA-256)
        trace_hash = hashlib.sha256(serialized_payload.encode('utf-8')).hexdigest()
        
        return f"0x{trace_hash}"

    def submit_to_gateway(self, gateway_url: str = "http://127.0.0.1:8000/api/v1/traces"):
        """Submits the trace to the Archi Gateway API."""
        import requests
        proof = self.generate_proof()
        payload = {
            "agent_id": self.agent_id,
            "session_id": f"session_{int(time.time())}",
            "domain": "audit",
            "trace_hash": proof,
            "timestamp": self.timestamp_start,
            "state": self.status,
            "metadata": {
                "task": self.task_name,
                "logic_hash": self.logic_hash
            }
        }
        try:
            resp = requests.post(gateway_url, json=payload, timeout=5)
            return resp.json()
        except Exception as e:
            return {"status": "error", "message": str(e)}

# --- SIMULATION ---
if __name__ == "__main__":
    print("[Ω] Initializing Archi-SDK Test Protocol...\n")
    
    # 1. Initialize the trace for a specific agent
    trace = ArchiTrace(agent_id="0x4f2e...8a92", task_name="capital_extraction_v1")
    
    # 2. Record inputs
    trace.record_input({"target_contract": "0x123...", "gas_limit": 500000})
    
    # 3. Simulate tool calls (The agent working)
    print("-> Agent scanning contract...")
    time.sleep(0.5)
    trace.record_tool_call(
        tool_name="read_contract",
        arguments={"address": "0x123..."},
        result="Vulnerable function 'withdraw' found."
    )
    
    print("-> Agent executing extraction...")
    time.sleep(0.5)
    trace.record_tool_call(
        tool_name="execute_tx",
        arguments={"function": "withdraw", "value": "10 ETH"},
        result="Transaction Confirmed: TxHash 0xabc..."
    )
    
    # 4. Finalize the trace
    trace.finalize(output_data={"extracted_value": "10 ETH", "status": "CONFIRMED"})
    
    # 5. Generate the Cryptographic Proof
    proof_hash = trace.generate_proof()
    
    print("\n[+] Trace Finalized Successfully.")
    print(f"[+] ARCHI-TRACE-HASH: {proof_hash}")
    print("[+] This hash is immutable and ready for on-chain verification.")
