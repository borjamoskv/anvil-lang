import urllib.request
import urllib.error
import json
import subprocess
import time
import sys
import os

PORT = 8000
URL = f"http://127.0.0.1:{PORT}/v1/prove"

def main():
    print("🚀 Booting Zero-Dependency Proof Market Oracle for Integration Test...")
    
    # Absolute path to services/api/main.py
    api_dir = os.path.abspath(os.path.join(os.path.dirname(__file__), "../services/api"))
    oracle_process = subprocess.Popen(["python3", "main.py"], cwd=api_dir)
    
    # Wait for the Oracle to boot
    time.sleep(2)
    
    payload = {
        "client_id": "cortex_client_001",
        "stripe_session_id": "cs_test_a1b2c3d4",
        "source_code": "contract SafeVault { fn deposit(bal: u64, amt: u64) -> u64 where { amt > 0, bal + amt >= bal, bal' == bal + amt } { bal += amt; return bal; } }"
    }
    
    data = json.dumps(payload).encode('utf-8')
    req = urllib.request.Request(URL, data=data, headers={'Content-Type': 'application/json'})
    
    success = False
    
    try:
        print("📨 Submitting Anvil invariant to Proof Market...")
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode('utf-8'))
            print("==================================================")
            print("ORACLE RESPONSE:")
            print(json.dumps(result, indent=2))
            print("==================================================")
            
            if result.get("status") == "PROVEN_SAFE" and result.get("certificate_hash"):
                print("✅ TEST PASSED: Z3 issued the cryptographic certificate.")
                success = True
            else:
                print("❌ TEST FAILED: Certificate denied.")
    
    except urllib.error.HTTPError as e:
        print(f"❌ TEST FAILED: HTTP Error {e.code}")
        print(e.read().decode('utf-8'))
    except Exception as e:
        print(f"❌ TEST FAILED: Exception {e}")
        
    finally:
        print("🛑 Terminating Oracle...")
        oracle_process.terminate()
        oracle_process.wait()
        
    if not success:
        sys.exit(1)

if __name__ == "__main__":
    main()
