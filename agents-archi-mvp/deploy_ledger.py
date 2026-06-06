#!/usr/bin/env python3
"""
[Ω] ArchiLedger Deployment Suite (C5-REAL / C4-SIMULACIÓN)
Deploys ArchiLedger.sol to local Anvil or L2 Base network.
Enforces the Sovereign Singularity Policy (GEMINI.md).
"""

import os
import sys
import json
import argparse
from web3 import Web3
from eth_account import Account

# Define workspace-relative paths
BASE_DIR = os.path.dirname(os.path.abspath(__file__))
ARTIFACT_PATH = os.path.join(BASE_DIR, "out", "ArchiLedger.sol", "ArchiLedger.json")
CONFIG_PATH = os.path.join(BASE_DIR, "deployed_address.json")

def load_contract_artifact():
    if not os.path.exists(ARTIFACT_PATH):
        print(f"[!] Error: Solidity compilation artifact not found at: {ARTIFACT_PATH}")
        print("    Please run: forge build")
        sys.exit(1)
        
    with open(ARTIFACT_PATH, "r") as f:
        return json.load(f)

def deploy(rpc_url: str, private_key: str):
    print("[Ω] Initiating Sovereign ArchiLedger EVM Anchor Deployment...")
    print(f"[*] Target RPC Node: {rpc_url}")
    
    # Initialize connection
    w3 = Web3(Web3.HTTPProvider(rpc_url))
    if not w3.is_connected():
        print(f"[!] Critical Error: Cannot connect to Ethereum RPC endpoint at {rpc_url}")
        print("    Running in C4-SIMULACIÓN (Mock) mode instead.")
        simulate_mock_deployment()
        return

    # Set account from private key
    try:
        deployer = Account.from_key(private_key)
        print(f"[+] Deployer Address: {deployer.address}")
    except Exception as e:
        print(f"[!] Invalid Private Key format: {e}")
        sys.exit(1)

    # Check deployer balance
    balance_wei = w3.eth.get_balance(deployer.address)
    balance_eth = w3.from_wei(balance_wei, "ether")
    print(f"[*] Deployer Balance: {balance_eth:.4f} ETH")
    
    if balance_wei == 0:
        print("[!] Critical Error: Deployer account has 0 ETH.")
        sys.exit(1)

    # Load artifacts
    artifact = load_contract_artifact()
    abi = artifact["abi"]
    bytecode = artifact["bytecode"]["object"]
    
    if not bytecode or bytecode == "0x":
        print("[!] Critical Error: Bytecode is empty. Ensure contract compiled correctly.")
        sys.exit(1)

    # Instantiate contract factory
    contract = w3.eth.contract(abi=abi, bytecode=bytecode)
    
    # Prepare transaction
    nonce = w3.eth.get_transaction_count(deployer.address)
    gas_estimate = contract.constructor().estimate_gas({'from': deployer.address})
    print(f"[*] Estimated gas for deployment: {gas_estimate} units")
    
    # Exergy and gas parameter optimization (1.2x buffer)
    gas_limit = int(gas_estimate * 1.2)
    gas_price = w3.eth.gas_price
    
    tx = contract.constructor().build_transaction({
        'from': deployer.address,
        'nonce': nonce,
        'gas': gas_limit,
        'gasPrice': gas_price
    })

    print("[*] Signing deployment transaction...")
    signed_tx = w3.eth.account.sign_transaction(tx, private_key=private_key)
    
    print("[*] Sending transaction to mempool...")
    tx_hash = w3.eth.send_raw_transaction(signed_tx.raw_transaction)
    print(f"[+] Transaction sent. Tx Hash: {tx_hash.hex()}")
    
    print("[*] Awaiting mining receipt (C5-REAL verification)...")
    receipt = w3.eth.wait_for_transaction_receipt(tx_hash)
    
    if receipt.status != 1:
        print("[!] Contract deployment FAILED in execution block.")
        sys.exit(1)

    deployed_address = receipt.contractAddress
    print("\n[C5-REAL] ArchiLedger Deployed Successfully!")
    print(f"  -> Contract Address: {deployed_address}")
    print(f"  -> Block Number: {receipt.blockNumber}")
    print(f"  -> Gas Used: {receipt.gasUsed} units")
    
    # Save deployed state
    config_data = {
        "contract_address": deployed_address,
        "rpc_url": rpc_url,
        "deployment_timestamp": w3.eth.get_block(receipt.blockNumber).timestamp,
        "deployer": deployer.address,
        "tx_hash": tx_hash.hex(),
        "network_id": w3.eth.chain_id,
        "reality_level": "C5-REAL"
    }
    with open(CONFIG_PATH, "w") as f:
        json.dump(config_data, f, indent=4)
    print(f"[+] Address written to: {CONFIG_PATH}")

    # Thermodynamic Exergy Calculation (Ω₂ requirement)
    eth_usd_rate = 3000.0  # Reference price for gas exergy metric
    gas_price_eth = w3.from_wei(receipt.gasUsed * gas_price, "ether")
    exergy_metric = float(gas_price_eth) * eth_usd_rate * 100 # S=100
    
    print("\n" + "="*50)
    print(" THERMODYNAMIC EXERGY LOG (Rule Ω₂)")
    print("="*50)
    print("Claim: Gas Exergy Consumption")
    print("Proof:")
    print(f"  Base: {receipt.gasUsed} gas * {w3.from_wei(gas_price, 'gwei')} Gwei/gas * {eth_usd_rate} USD/ETH * 100")
    print("  Variables:")
    print(f"    r: {w3.from_wei(gas_price, 'ether'):.18f} (Gas price in ETH)")
    print("    d: 1 (Dimension)")
    print(f"    n: {receipt.gasUsed} (Gas units used)")
    print("    S: 100 (Singularity constant)")
    print(f"  Range: [{exergy_metric*0.9:.4f} USD*S, {exergy_metric*1.1:.4f} USD*S]")
    print("  Confidence: C5-REAL (On-chain verified transaction)")
    print("="*50 + "\n")

def simulate_mock_deployment():
    mock_address = "0x5FbDB2315678afecb367f032d93F642f64180aa3"
    print("\n[C4-SIMULACIÓN] Mock deployment initiated.")
    print(f"  -> Simulated Contract Address: {mock_address}")
    
    config_data = {
        "contract_address": mock_address,
        "rpc_url": "http://127.0.0.1:8545",
        "deployment_timestamp": 1778622962257 / 1000,
        "deployer": "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266",
        "tx_hash": "0x" + "0"*64,
        "network_id": 31337,
        "reality_level": "C4-SIMULACIÓN"
    }
    with open(CONFIG_PATH, "w") as f:
        json.dump(config_data, f, indent=4)
        
    print(f"[+] Address written to: {CONFIG_PATH}")
    print("\n" + "="*50)
    print(" THERMODYNAMIC EXERGY LOG (Rule Ω₂)")
    print("="*50)
    print("Claim: Gas Exergy Consumption (Simulated)")
    print("Proof:")
    print("  Base: 165000 gas * 0.000000001 ETH/gas * 3000 USD/ETH * 100")
    print("  Variables:")
    print("    r: 0.000000001000000000 (Gas price in ETH)")
    print("    d: 1 (Dimension)")
    print("    n: 165000 (Gas units used)")
    print("    S: 100 (Singularity constant)")
    print("  Range: [49.5000 USD*S, 49.5000 USD*S]")
    print("  Confidence: C4-SIMULACIÓN (Simulated context)")
    print("="*50 + "\n")

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Deploy ArchiLedger contract.")
    parser.add_argument("--rpc-url", default=os.getenv("ARCHI_RPC_URL", "http://127.0.0.1:8545"), help="Ethereum node RPC URL")
    parser.add_argument("--private-key", default=os.getenv("ARCHI_PRIVATE_KEY", "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"), help="Deployer Private Key")
    args = parser.parse_args()
    
    deploy(args.rpc_url, args.private_key)
