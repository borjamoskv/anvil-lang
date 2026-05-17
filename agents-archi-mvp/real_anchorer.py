import os
import json
from web3 import Web3
from eth_account import Account

# [Ω] C5-REAL Configuration
RPC_URL = os.getenv("ARCHI_RPC_URL", "http://127.0.0.1:8545")
PRIVATE_KEY = os.getenv("ARCHI_PRIVATE_KEY", "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80") # Default Anvil Account 0

# Load dynamic contract address from deployment config if exists
BASE_DIR = os.path.dirname(os.path.abspath(__file__))
CONFIG_PATH = os.path.join(BASE_DIR, "deployed_address.json")
DEFAULT_ADDRESS = "0x5FbDB2315678afecb367f032d93F642f64180aa3"

CONTRACT_ADDRESS = os.getenv("ARCHI_CONTRACT_ADDRESS")
if not CONTRACT_ADDRESS:
    if os.path.exists(CONFIG_PATH):
        try:
            with open(CONFIG_PATH, "r") as f:
                cfg = json.load(f)
                CONTRACT_ADDRESS = cfg.get("contract_address", DEFAULT_ADDRESS)
                print(f"[Ω] Loaded deployed contract address: {CONTRACT_ADDRESS}")
        except Exception:
            CONTRACT_ADDRESS = DEFAULT_ADDRESS
    else:
        CONTRACT_ADDRESS = DEFAULT_ADDRESS

ABI = [
    {
        "inputs": [
            {"internalType": "bytes32", "name": "_traceHash", "type": "bytes32"},
            {"internalType": "string", "name": "_agentId", "type": "string"},
            {"internalType": "string", "name": "_domain", "type": "string"}
        ],
        "name": "anchorTrace",
        "outputs": [],
        "stateMutability": "nonpayable",
        "type": "function"
    },
    {
        "inputs": [{"internalType": "bytes32", "name": "_traceHash", "type": "bytes32"}],
        "name": "verifyTrace",
        "outputs": [{"internalType": "bool", "name": "", "type": "bool"}],
        "stateMutability": "view",
        "type": "function"
    }
]

class RealAnchorer:
    def __init__(self):
        self.w3 = Web3(Web3.HTTPProvider(RPC_URL))
        if not self.w3.is_connected():
            print(f"[!] Critical Error: Could not connect to RPC at {RPC_URL}")
            self.account = None
        else:
            self.account = Account.from_key(PRIVATE_KEY)
            self.contract = self.w3.eth.contract(address=CONTRACT_ADDRESS, abi=ABI)
            print(f"[Ω] Connected to ArchiLedger at {CONTRACT_ADDRESS}")


    def anchor(self, trace_hash: str, agent_id: str, domain: str = "audit"):
        """Executes a real transaction to anchor the trace hash."""
        if not self.account:
            return None

        # Convert hex hash to bytes32
        if trace_hash.startswith("0x"):
            trace_hash_bytes = bytes.fromhex(trace_hash[2:])
        else:
            trace_hash_bytes = bytes.fromhex(trace_hash)

        print(f"[*] Anchoring {trace_hash[:18]}... on-chain")
        
        nonce = self.w3.eth.get_transaction_count(self.account.address)
        
        tx = self.contract.functions.anchorTrace(
            trace_hash_bytes,
            agent_id,
            domain
        ).build_transaction({
            'from': self.account.address,
            'nonce': nonce,
            'gas': 200000,
            'gasPrice': self.w3.eth.gas_price
        })

        signed_tx = self.w3.eth.account.sign_transaction(tx, private_key=PRIVATE_KEY)
        tx_hash = self.w3.eth.send_raw_transaction(signed_tx.raw_transaction)
        
        print(f"[+] Transaction sent: {tx_hash.hex()}")
        receipt = self.w3.eth.wait_for_transaction_receipt(tx_hash)
        
        if receipt.status == 1:
            print(f"[C5-REAL] Trace anchored successfully in block {receipt.blockNumber}")
            return tx_hash.hex()
        else:
            print("[-] Transaction failed.")
            return None

if __name__ == "__main__":
    # Test anchoring
    anchorer = RealAnchorer()
    mock_hash = "0x" + "a" * 64
    anchorer.anchor(mock_hash, "CLI_TEST_AGENT", "test")
