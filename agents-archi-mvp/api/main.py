"""
[Ω] Archi Agent Verification Gateway v2.0
FastAPI service with automatic on-chain anchoring.
Reality levels:
  - C5-REAL: Connected to Anvil/L2, traces anchored on-chain
  - C4-SIMULACIÓN: No RPC, traces stored off-chain only
"""
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel
from typing import Dict, Any, Optional
import json
import os
import sys
import logging
from fastapi.middleware.cors import CORSMiddleware

# Add parent dir so we can import real_anchorer
sys.path.insert(
    0, os.path.join(os.path.dirname(__file__), "..")
)

# No RPC or signer initialization during import while writes are quarantined.
anchorer = None

app = FastAPI(
    title="Archi Agent Verification Gateway",
    version="2.0.0",
)

app.add_middleware(
    CORSMiddleware,
    allow_origins=[],
    allow_credentials=False,
    allow_methods=["GET"],
    allow_headers=[],
)

BASE_DIR = os.path.join(os.path.dirname(__file__), "..")
REGISTRY_PATH = os.path.join(BASE_DIR, "registry.json")


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
        return {
            "traces": [],
            "leaderboard": [],
            "feed": [],
        }
    try:
        with open(REGISTRY_PATH, "r") as f:
            data = json.load(f)
            data.setdefault("traces", [])
            data.setdefault("feed", [])
            data.setdefault("leaderboard", [])
            return data
    except (OSError, ValueError) as exc:
        raise HTTPException(status_code=503, detail="Registry unavailable") from exc


@app.post("/api/v1/traces")
async def submit_trace(payload: TracePayload):
    # No writes or on-chain effects before authenticated ingestion and
    # persistence have an independently verified protocol.
    raise HTTPException(status_code=503, detail="OMEGA-21: trace writes and anchoring quarantined")


@app.get("/api/v1/traces")
async def get_traces(limit: int = 50):
    """Retrieve trace history for the Dashboard."""
    registry = load_registry()
    return {"traces": registry["traces"][:limit]}


@app.get("/api/v1/registry")
async def get_registry():
    """Return full registry content."""
    return load_registry()


@app.get("/api/v1/health")
async def health_check():
    return {
        "status": "READ_ONLY_QUARANTINE",
        "engine": "Archi Gateway V2",
        "onchain_ready": False,
    }
