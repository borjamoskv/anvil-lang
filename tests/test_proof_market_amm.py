import subprocess
import json
import time
import sys
import os
import tempfile

def main():
    print("🚀 Booting Zero-Dependency Proof Market Verification for AMM Exploit Test...")
    
    workspace_root = os.path.abspath(os.path.join(os.path.dirname(__file__), "../"))
    
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

    print("📨 Submitting vulnerable AMM invariant to Z3 Engine...")
    
    with tempfile.NamedTemporaryFile(mode="w+", suffix=".anv", delete=False) as temp_file:
        temp_file.write(amm_vulnerable_code)
        temp_path = temp_file.name
        
    try:
        process = subprocess.run(
            ["./target/debug/anvil", "check", temp_path],
            cwd=workspace_root,
            capture_output=True,
            text=True
        )
        
        output = process.stderr if process.stderr else process.stdout
        
        print("==================================================")
        print("ORACLE RESPONSE:")
        print(output)
        print("==================================================")
        
        if process.returncode != 0 and "VERIFICATION FAILED" in output:
            print("✅ TEST PASSED: Z3 successfully extracted the AMM vulnerability.")
            print("🔒 No certificate issued. The protocol is protected.")
            sys.exit(0)
        else:
            print("❌ TEST FAILED: Vulnerability missed or certificate incorrectly issued.")
            sys.exit(1)
            
    except Exception as e:
        print(f"❌ TEST FAILED: Exception {e}")
        sys.exit(1)
    finally:
        if os.path.exists(temp_path):
            os.remove(temp_path)

if __name__ == "__main__":
    main()
