from fastapi import FastAPI, HTTPException
from pydantic import BaseModel
from typing import Dict, Any, List
import json
import os
import time
from fastapi.middleware.cors import CORSMiddleware

app = FastAPI(title="Archi Agent Verification Gateway", version="1.0.0")

# CORS setup for the frontend
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

REGISTRY_PATH = os.path.join(os.path.dirname(__file__), "..", "registry.json")

class TracePayload(BaseModel):
    agent_id: str
    session_id: str
    domain: str
    trace_hash: str
    timestamp: float
    state: str = "COMPLETED"
    metadata: Dict[str, Any] = {}

def load_registry():
    if not os.path.exists(REGISTRY_PATH):
        return {"traces": [], "leaderboard": [], "feed": []}
    try:
        with open(REGISTRY_PATH, "r") as f:
            data = json.load(f)
            if "traces" not in data:
                data["traces"] = []
            if "feed" not in data:
                data["feed"] = []
            if "leaderboard" not in data:
                data["leaderboard"] = []
            return data
    except Exception:
        return {"traces": [], "leaderboard": [], "feed": []}

def save_registry(data):
    with open(REGISTRY_PATH, "w") as f:
        json.dump(data, f, indent=4)

@app.post("/api/v1/traces")
async def submit_trace(payload: TracePayload):
    """
    Recibe un Archi-Trace-Hash de un agente remoto y lo registra en el Ledger.
    """
    registry = load_registry()
    
    # Create the trace entry matching the local sdk format
    trace_entry = {
        "agent_id": payload.agent_id,
        "session_id": payload.session_id,
        "domain": payload.domain,
        "timestamp": payload.timestamp,
        "trace_hash": payload.trace_hash,
        "state": payload.state,
        "onchain_tx": None,  # Pending blockchain anchoring
        "metadata": payload.metadata
    }
    
    # Insert at the beginning to act like a feed
    registry["traces"].insert(0, trace_entry)
    save_registry(registry)
    
    return {
        "status": "success", 
        "message": "Trace registered successfully",
        "trace_hash": payload.trace_hash
    }

@app.get("/api/v1/traces")
async def get_traces(limit: int = 50):
    """
    Recupera el historial de trazas para el Dashboard.
    """
    registry = load_registry()
    return {"traces": registry["traces"][:limit]}

@app.get("/api/v1/registry")
async def get_registry():
    """
    Devuelve todo el contenido del registry (Leaderboard, Feed legacy y Traces).
    """
    return load_registry()

@app.get("/api/v1/health")
async def health_check():
    return {"status": "C5-REAL", "engine": "Archi Gateway V1"}
