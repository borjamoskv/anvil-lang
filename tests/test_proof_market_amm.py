import urllib.request
import urllib.error
import json
import subprocess
import time
import sys
import os

PORT = 8001 # Use different port to avoid conflicts
URL = f"http://127.0.0.1:{PORT}/v1/prove"

def main():
    print("🚀 Booting Zero-Dependency Proof Market Oracle for AMM Exploit Test...")
    
    # We patch the PORT for this specific test to avoid collision
    api_dir = os.path.abspath(os.path.join(os.path.dirname(__file__), "../services/api"))
    
    # Create a temporary patched main.py for the test to run on PORT 8001
    with open(os.path.join(api_dir, "main.py"), "r") as f:
        api_code = f.read()
    
    api_code = api_code.replace("PORT = 8000", f"PORT = {PORT}")
    
    with open(os.path.join(api_dir, "test_main_amm.py"), "w") as f:
        f.write(api_code)

    oracle_process = subprocess.Popen(["python3", "test_main_amm.py"], cwd=api_dir)
    
    # Wait for the Oracle to boot
    time.sleep(2)
    
    amm_vulnerable_code = """
struct AMMPool {
    reserve_x: u64,
    reserve_y: u64
}

fn swap(reserve_x: u64, reserve_y: u64, amount_in_x: u64, amount_out_y: u64) -> u64
    where {
        reserve_x > 0,
        reserve_y > 0,
        amount_in_x > 0,
        reserve_x' * reserve_y' >= reserve_x * reserve_y
    }
{
    reserve_x += amount_in_x;
    reserve_y -= amount_out_y;
    return amount_out_y;
}
"""

    payload = {
        "client_id": "cortex_client_002",
        "stripe_session_id": "cs_test_amm_exploit",
        "source_code": amm_vulnerable_code
    }
    
    data = json.dumps(payload).encode('utf-8')
    req = urllib.request.Request(URL, data=data, headers={'Content-Type': 'application/json'})
    
    success = False
    
    try:
        print("📨 Submitting vulnerable AMM invariant to Proof Market...")
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode('utf-8'))
            print("==================================================")
            print("ORACLE RESPONSE:")
            print(json.dumps(result, indent=2))
            print("==================================================")
            
            if result.get("status") == "VULNERABILITY_DETECTED" and "VERIFICATION FAILED" in result.get("z3_output", ""):
                print("✅ TEST PASSED: Z3 successfully extracted the AMM vulnerability.")
                print("🔒 No certificate issued. The protocol is protected.")
                success = True
            else:
                print("❌ TEST FAILED: Vulnerability missed or certificate incorrectly issued.")
    
    except urllib.error.HTTPError as e:
        print(f"❌ TEST FAILED: HTTP Error {e.code}")
        print(e.read().decode('utf-8'))
    except Exception as e:
        print(f"❌ TEST FAILED: Exception {e}")
        
    finally:
        print("🛑 Terminating Oracle...")
        oracle_process.terminate()
        oracle_process.wait()
        if os.path.exists(os.path.join(api_dir, "test_main_amm.py")):
            os.remove(os.path.join(api_dir, "test_main_amm.py"))
        
    if not success:
        sys.exit(1)

if __name__ == "__main__":
    main()
