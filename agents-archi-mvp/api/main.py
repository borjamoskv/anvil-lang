"""
[Ω] Archi Agent Verification Gateway v2.0
FastAPI service with automatic on-chain anchoring.
Reality levels:
  - C5-REAL: Connected to Anvil/L2, traces anchored on-chain
  - C4-SIMULACIÓN: No RPC, traces stored off-chain only
"""
from fastapi import FastAPI
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

# Lazy-load the anchorer at startup
anchorer = None
try:
    from real_anchorer import RealAnchorer
    _inst = RealAnchorer()
    if _inst.account:
        anchorer = _inst
        logging.info("[C5-REAL] RealAnchorer connected")
    else:
        logging.warning(
            "[C4-SIM] RealAnchorer init but no RPC"
        )
except Exception as exc:
    logging.warning(f"[C4-SIM] Anchorer unavailable: {exc}")

app = FastAPI(
    title="Archi Agent Verification Gateway",
    version="2.0.0",
)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
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
    except Exception:
        return {
            "traces": [],
            "leaderboard": [],
            "feed": [],
        }


def save_registry(data):
    with open(REGISTRY_PATH, "w") as f:
        json.dump(data, f, indent=4)


def try_anchor_onchain(
    trace_hash: str, agent_id: str, domain: str
) -> Optional[str]:
    """Attempt C5-REAL on-chain anchoring. Returns tx hash
    or None on failure/unavailability."""
    if not anchorer:
        return None
    try:
        tx_hash = anchorer.anchor(
            trace_hash, agent_id, domain
        )
        return tx_hash
    except Exception as exc:
        logging.error(f"[!] On-chain anchor failed: {exc}")
        return None


@app.post("/api/v1/traces")
async def submit_trace(payload: TracePayload):
    """
    Receive an Archi-Trace-Hash from a remote agent.
    1. Store in local registry
    2. Attempt on-chain anchor (if RPC available)
    """
    registry = load_registry()

    # Attempt on-chain anchoring
    onchain_tx = try_anchor_onchain(
        payload.trace_hash,
        payload.agent_id,
        payload.domain,
    )

    reality = "C5-REAL" if onchain_tx else "C4-SIMULACIÓN"

    trace_entry = {
        "agent_id": payload.agent_id,
        "session_id": payload.session_id,
        "domain": payload.domain,
        "timestamp": payload.timestamp,
        "trace_hash": payload.trace_hash,
        "state": payload.state,
        "onchain_tx": onchain_tx,
        "reality_level": reality,
        "metadata": payload.metadata,
    }

    registry["traces"].insert(0, trace_entry)
    save_registry(registry)

    return {
        "status": "success",
        "message": f"Trace registered ({reality})",
        "trace_hash": payload.trace_hash,
        "onchain_tx": onchain_tx,
        "reality_level": reality,
    }


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
    mode = "C5-REAL" if anchorer else "C4-SIMULACIÓN"
    return {
        "status": mode,
        "engine": "Archi Gateway V2",
        "onchain_ready": anchorer is not None,
    }
