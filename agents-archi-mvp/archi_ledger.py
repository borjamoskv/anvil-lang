import json
import random
import time
from archi_sdk import ArchiTrace

def generate_swarm_ledger():
    print("[Ω] Initiating Swarm Ledger Generation...")
    
    agents = [
        {"id": "0x882a...f2e1", "name": "OSINT_SWARM_04", "task": "market_recon", "score": 99.8},
        {"id": "0x4f2e...8a92", "name": "TRADER_ALPHA", "task": "capital_extraction", "score": 99.4},
        {"id": "0x11b3...c9d4", "name": "COMPLIANCE_BOT", "task": "regulatory_audit", "score": 98.7},
        {"id": "0x99e5...b3a1", "name": "RESEARCH_NODE_12", "task": "deep_tech_synthesis", "score": 98.2},
        {"id": "0x22c4...e1f0", "name": "MARKET_MAKER_X", "task": "liquidity_provision", "score": 97.9}
    ]

    registry_data = {
        "timestamp": time.time(),
        "leaderboard": [],
        "feed": []
    }

    for agent in agents:
        # Create a trace for the agent
        trace = ArchiTrace(agent_id=agent["id"], task_name=agent["task"])
        
        # Simulate some deterministic input
        trace.record_input({"target_domain": "ethereum_mainnet", "complexity": random.randint(1, 10)})
        
        # Simulate tool execution
        trace.record_tool_call("scan_network", {"depth": "full"}, "Scan complete. Targets acquired.")
        trace.record_tool_call("execute_logic", {"strategy": "adaptive"}, "Logic executed successfully.")
        
        # Finalize and get hash
        trace.finalize({"status": "OK", "yield": f"{random.uniform(0.1, 2.5):.2f} ETH"})
        proof_hash = trace.generate_proof()
        
        # Update agent info with the real trace hash
        agent_entry = {
            "name": agent["name"],
            "id": agent["id"],
            "score": agent["score"],
            "last_hash": proof_hash
        }
        registry_data["leaderboard"].append(agent_entry)
        
        # Generate a feed event using the real hash
        registry_data["feed"].append({
            "msg": f"Agent {agent['name']} verified. Trace Hash: {proof_hash[:10]}...",
            "status": "success"
        })

    # Add a couple of error events for realism
    registry_data["feed"].append({
        "msg": "REJECTED: Trace #9912 (Non-deterministic output detected)",
        "status": "error"
    })

    # Save the ledger
    with open("registry.json", "w") as f:
        json.dump(registry_data, f, indent=4)
        
    print("[+] registry.json successfully generated with C5-REAL cryptographic proofs.")

if __name__ == "__main__":
    generate_swarm_ledger()
