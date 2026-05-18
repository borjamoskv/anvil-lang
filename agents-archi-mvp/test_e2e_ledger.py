#!/usr/bin/env python3
"""
[Ω] ArchiLedger E2E Test — C5-REAL Verification
Deploys nothing; uses already-deployed contract.
1. Anchors a deterministic trace hash via anchorTrace()
2. Verifies it via verifyTrace()
3. Reads storage via traces() mapping
All on-chain, all real.
"""
import os
import sys
import json
import hashlib
from web3 import Web3
from eth_account import Account

# --- Config ---
RPC_URL = "http://127.0.0.1:8545"
PRIVATE_KEY = (
    "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
)

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
CONFIG_PATH = os.path.join(BASE_DIR, "deployed_address.json")
ARTIFACT_PATH = os.path.join(
    BASE_DIR, "out", "ArchiLedger.sol", "ArchiLedger.json"
)


def main():
    # Load deployed address
    with open(CONFIG_PATH) as f:
        cfg = json.load(f)
    contract_addr = cfg["contract_address"]

    # Load full ABI from compiled artifact
    with open(ARTIFACT_PATH) as f:
        artifact = json.load(f)
    abi = artifact["abi"]

    # Connect
    w3 = Web3(Web3.HTTPProvider(RPC_URL))
    assert w3.is_connected(), "Cannot connect to Anvil"
    account = Account.from_key(PRIVATE_KEY)
    contract = w3.eth.contract(
        address=contract_addr, abi=abi
    )

    print(f"[Ω] ArchiLedger E2E Test")
    print(f"    Contract: {contract_addr}")
    print(f"    Chain ID: {w3.eth.chain_id}")
    print()

    # --- Step 1: Create deterministic trace hash ---
    import time
    trace_payload = json.dumps({
        "agent_id": "E2E_TEST_AGENT",
        "task": "sovereign_verification",
        "inputs": {"target": "0xDEAD"},
        "outputs": {"status": "VERIFIED"},
        "nonce": time.time()
    }, sort_keys=True, separators=(',', ':'))
    trace_hash_hex = hashlib.sha256(
        trace_payload.encode()
    ).hexdigest()
    trace_hash_bytes = bytes.fromhex(trace_hash_hex)

    print(f"[1] Trace Hash: 0x{trace_hash_hex[:16]}...")

    # --- Step 2: Verify it does NOT exist yet ---
    exists_before = contract.functions.verifyTrace(
        trace_hash_bytes
    ).call()
    print(f"[2] verifyTrace BEFORE anchor: {exists_before}")
    assert not exists_before, "Hash already anchored (unexpected)"

    # --- Step 3: Anchor the trace on-chain ---
    nonce = w3.eth.get_transaction_count(account.address)
    tx = contract.functions.anchorTrace(
        trace_hash_bytes,
        "E2E_TEST_AGENT",
        "e2e_test"
    ).build_transaction({
        'from': account.address,
        'nonce': nonce,
        'gas': 200000,
        'gasPrice': w3.eth.gas_price,
    })
    signed = w3.eth.account.sign_transaction(
        tx, private_key=PRIVATE_KEY
    )
    tx_hash = w3.eth.send_raw_transaction(
        signed.raw_transaction
    )
    receipt = w3.eth.wait_for_transaction_receipt(tx_hash)
    assert receipt.status == 1, "anchorTrace tx reverted"
    print(
        f"[3] anchorTrace TX confirmed in block "
        f"{receipt.blockNumber} | gas: {receipt.gasUsed}"
    )

    # --- Step 4: Verify it NOW exists ---
    exists_after = contract.functions.verifyTrace(
        trace_hash_bytes
    ).call()
    print(f"[4] verifyTrace AFTER anchor: {exists_after}")
    assert exists_after, "verifyTrace returned False after anchor"

    # --- Step 5: Read full trace struct ---
    trace_data = contract.functions.traces(
        trace_hash_bytes
    ).call()
    agent_id, domain, timestamp, submitter = trace_data
    print(f"[5] On-chain Trace Data:")
    print(f"    agentId:   {agent_id}")
    print(f"    domain:    {domain}")
    print(f"    timestamp: {timestamp}")
    print(f"    submitter: {submitter}")

    # --- Step 6: Replay protection ---
    print(f"[6] Testing replay protection...")
    nonce2 = w3.eth.get_transaction_count(account.address)
    tx2 = contract.functions.anchorTrace(
        trace_hash_bytes,
        "REPLAY_ATTACKER",
        "replay"
    ).build_transaction({
        'from': account.address,
        'nonce': nonce2,
        'gas': 200000,
        'gasPrice': w3.eth.gas_price,
    })
    signed2 = w3.eth.account.sign_transaction(
        tx2, private_key=PRIVATE_KEY
    )
    tx_hash2 = w3.eth.send_raw_transaction(
        signed2.raw_transaction
    )
    receipt2 = w3.eth.wait_for_transaction_receipt(tx_hash2)
    if receipt2.status == 0:
        print(f"    [OK] Replay REJECTED (tx reverted as expected)")
    else:
        print(f"    [FAIL] Replay was NOT rejected!")
        sys.exit(1)

    # --- Result ---
    print()
    print("=" * 50)
    print(" [C5-REAL] ALL ASSERTIONS PASSED")
    print(" ArchiLedger EVM Anchor is OPERATIONAL")
    print("=" * 50)


if __name__ == "__main__":
    main()
