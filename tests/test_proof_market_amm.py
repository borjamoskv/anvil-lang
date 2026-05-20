import json
import sys

from test_proof_market import Oracle, assert_response, post_json


AMM_VULNERABLE_CODE = """
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


def main():
    print("Booting Proof Market HTTP AMM exploit test...")
    with Oracle({
        "ANVIL_CERTIFICATE_SECRET": "ci-proof-market-secret",
        "ANVIL_ALLOW_MOCK_PAYMENT": "1",
    }) as oracle:
        http_status, result = post_json(oracle.base_url, {
            "client_id": "cortex_client_amm",
            "payment_mode": "mock",
            "source_code": AMM_VULNERABLE_CODE,
        })

    assert_response("AMM counterexample", http_status, result, 200, "VULNERABILITY_DETECTED")
    if result.get("certificate_hash"):
        print("FAIL AMM counterexample: certificate_hash should be absent")
        print(json.dumps(result, indent=2))
        sys.exit(1)
    if "Postcondition #1 violated" not in result.get("z3_output", ""):
        print("FAIL AMM counterexample: solver output did not include the violated postcondition")
        print(json.dumps(result, indent=2))
        sys.exit(1)
    print("Proof Market denied the vulnerable AMM certificate.")


def test_proof_market_amm_counterexample():
    main()


if __name__ == "__main__":
    main()
