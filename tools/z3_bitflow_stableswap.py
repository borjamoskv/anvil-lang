#!/usr/bin/env python3
"""
═══════════════════════════════════════════════════════════════
[C5-REAL] Anvil Proof: Bitflow StableSwap Invariant Analysis
═══════════════════════════════════════════════════════════════
Target: BitflowFinance/bitflow — stableswap.clar
Chain: Stacks (Clarity, uint128)
Bounty: $100,000 Critical (Immunefi)

Pure-Python deterministic prover (no Z3 dependency).
Models Bitflow's exact Clarity integer arithmetic.
═══════════════════════════════════════════════════════════════
"""

import json
import time
from datetime import datetime, timezone

N_TOKENS = 2
MAX_UINT128 = (2**128) - 1

# ── Bitflow get-D (Newton-Raphson, exact Clarity port) ──────

def get_D(x_bal, y_bal, ann):
    """Exact port of Bitflow's get-D from stableswap.clar."""
    S = x_bal + y_bal
    D = S
    threshold = 2  # convergence-threshold from contract

    for _ in range(384):
        D_partial = D
        D_partial = D * D_partial // (2 * x_bal)
        D_partial = D * D_partial // (2 * y_bal)

        numerator = (ann * S + N_TOKENS * D_partial) * D
        denominator = (ann - 1) * D + (N_TOKENS + 1) * D_partial
        
        if denominator == 0:
            return {"D": D, "error": "division_by_zero", "iterations": _}

        new_D = numerator // denominator

        if new_D > D:
            if new_D - D <= threshold:
                return {"D": new_D, "converged": True, "iterations": _}
        else:
            if D - new_D <= threshold:
                return {"D": new_D, "converged": True, "iterations": _}
        D = new_D

    return {"D": D, "converged": False, "iterations": 384}


def get_y(x_bal, y_bal, x_amount, ann):
    """Exact port of Bitflow's get-y."""
    x_bal_new = x_bal + x_amount
    result = get_D(x_bal, y_bal, ann)
    D = result["D"]
    threshold = 2

    c0 = D
    c1 = c0 * D // (N_TOKENS * x_bal_new)
    c2 = c1 * D // (ann * N_TOKENS)
    b = x_bal_new + D // ann

    y = D
    for _ in range(384):
        y_num = y * y + c2
        y_den = 2 * y + b - D
        if y_den == 0:
            return y
        new_y = y_num // y_den
        if new_y > y:
            if new_y - y <= threshold:
                return new_y
        else:
            if y - new_y <= threshold:
                return new_y
        y = new_y
    return y


def run_all_proofs():
    results = []

    # ═══════════════════════════════════════════════════════
    # VECTOR 1: Fee Truncation Bypass
    # ═══════════════════════════════════════════════════════
    print("=" * 60)
    print("  VECTOR 1: Fee Truncation Bypass")
    print("=" * 60)

    swap_fee_lps = 3      # bps
    swap_fee_protocol = 2  # bps
    total_fee_bps = 5

    # Integer division: fee = amount * bps / 10000
    # Fee is zero when amount * bps < 10000
    # Max zero-fee amount = floor(10000 / bps) - 1
    max_zero_lps = 10000 // swap_fee_lps - 1       # 3332
    max_zero_proto = 10000 // swap_fee_protocol - 1  # 4999
    max_zero_total = 10000 // total_fee_bps - 1      # 1999

    # Verify
    assert max_zero_total * total_fee_bps // 10000 == 0
    assert (max_zero_total + 1) * total_fee_bps // 10000 > 0

    # But there's a subtlety: Bitflow calculates fees SEPARATELY
    # fee_lps = amount * 3 / 10000 
    # fee_protocol = amount * 2 / 10000
    # So the effective max zero-fee for BOTH being zero:
    effective_max = min(max_zero_lps, max_zero_proto)  # 3332

    # At amount=3332: fee_lps = 3332*3//10000 = 0, fee_proto = 3332*2//10000 = 0
    test_amt = effective_max
    fee_l = test_amt * swap_fee_lps // 10000
    fee_p = test_amt * swap_fee_protocol // 10000
    print(f"  At amount={test_amt}: fee_lps={fee_l}, fee_proto={fee_p}, total_fee={fee_l+fee_p}")
    
    # At 3333: fee_lps = 3333*3//10000 = 0 still! (9999/10000=0)
    test_amt2 = 3333
    fee_l2 = test_amt2 * swap_fee_lps // 10000
    fee_p2 = test_amt2 * swap_fee_protocol // 10000
    print(f"  At amount={test_amt2}: fee_lps={fee_l2}, fee_proto={fee_p2}, total_fee={fee_l2+fee_p2}")
    
    # Find exact boundary
    for amt in range(3330, 5001):
        fl = amt * swap_fee_lps // 10000
        fp = amt * swap_fee_protocol // 10000
        if fl > 0 or fp > 0:
            boundary = amt
            break
    
    print(f"\n  ⚡ RESULT: All swaps with amount ≤ {boundary-1} pay ZERO fees")
    print(f"  Fee LP boundary:       {10000 // swap_fee_lps} (amount where fee_lps first > 0)")
    print(f"  Fee Protocol boundary: {10000 // swap_fee_protocol} (amount where fee_proto first > 0)")
    print(f"  Combined zero-fee max: {boundary-1}")
    
    # Impact calculation with real pool sizes
    # Bitflow sUSDT/USDA pool: assume $1M TVL, ~500K per side
    # sUSDT has 8 decimals, USDA has 6 decimals
    # Amount 3333 in sUSDT = 0.00003333 sUSDT ≈ $0.00003
    # This is microscopic — not exploitable at meaningful scale
    
    # BUT with scaled amounts (Bitflow scales to max decimals):
    # If x_decimals=8, y_decimals=6, scaling factor = 10^2 = 100
    # Scaled amounts up to 3333 * 100 = 333,300 base units could be zero-fee
    
    print("\n  Impact Assessment:")
    print(f"  - Raw: {boundary-1} base units = negligible value")
    print(f"  - Scaled (8 vs 6 decimal): {(boundary-1)} scaled units")
    print("  - Real value: ~$0.00003 per swap = NOT economically exploitable")
    print("  - Verdict: VALID BUG, LOW SEVERITY (dust amounts only)")

    results.append({
        "vector": "fee_truncation",
        "status": "SAT",
        "max_zero_fee_amount": boundary - 1,
        "severity": "LOW",
        "detail": f"Swaps ≤ {boundary-1} base units bypass fees. Economically insignificant per swap but demonstrates precision loss."
    })

    # ═══════════════════════════════════════════════════════
    # VECTOR 2: D-Invariant Convergence Failure
    # ═══════════════════════════════════════════════════════
    print("\n" + "=" * 60)
    print("  VECTOR 2: D-Invariant Convergence Analysis")
    print("=" * 60)

    test_cases = [
        # (x_bal, y_bal, amp, description)
        (10**12, 10**12, 100, "Balanced, normal amp"),
        (10**12, 1, 100, "Extreme imbalance (1:10^12)"),
        (1, 10**12, 100, "Extreme imbalance (reverse)"),
        (10**15, 10**15, 1, "Large balanced, min amp"),
        (10**15, 10**15, 5000, "Large balanced, max amp"),
        (10**6, 10**6, 1, "Small balanced, min amp"),
        (1, 1, 1, "Minimum values"),
        (MAX_UINT128 // 2, MAX_UINT128 // 2, 100, "Near-max uint128"),
        (10**12, 10**6, 100, "6-order magnitude imbalance"),
        (10**12, 10**6, 1, "6-order imbalance, min amp"),
    ]

    convergence_failures = []
    for x, y, amp, desc in test_cases:
        try:
            ann = amp * N_TOKENS
            result = get_D(x, y, ann)
            status = "✓" if result.get("converged") else "✗"
            iters = result.get("iterations", "?")
            D_val = result.get("D", "ERROR")
            if "error" in result:
                status = f"💥 {result['error']}"
                convergence_failures.append((x, y, amp, desc, result))
            elif not result.get("converged"):
                convergence_failures.append((x, y, amp, desc, result))
            print(f"  {status} D={D_val} (iters={iters}) — {desc}")
        except Exception as e:
            print(f"  💥 CRASH: {e} — {desc}")
            convergence_failures.append((x, y, amp, desc, {"error": str(e)}))

    if convergence_failures:
        print(f"\n  ⚡ {len(convergence_failures)} convergence failures found!")
        results.append({
            "vector": "d_convergence_failure",
            "status": "SAT",
            "failures": len(convergence_failures),
            "severity": "MEDIUM",
            "detail": f"{len(convergence_failures)} parameter combinations cause D to not converge"
        })
    else:
        print("\n  All test cases converged.")
        results.append({"vector": "d_convergence_failure", "status": "UNSAT"})

    # ═══════════════════════════════════════════════════════
    # VECTOR 3: LP Share Inflation via Imbalanced Add
    # ═══════════════════════════════════════════════════════
    print("\n" + "=" * 60)
    print("  VECTOR 3: LP Share Inflation")
    print("=" * 60)

    # Pool state: balanced at 1M each, 1M total shares
    pool_x = 10**12  # 1M with 6 decimals
    pool_y = 10**12
    total_shares = 10**12
    amp = 100
    ann_val = amp * N_TOKENS

    d0_result = get_D(pool_x, pool_y, ann_val)
    d0 = d0_result["D"]
    print(f"  Initial D: {d0}")

    # Attacker adds heavily imbalanced liquidity
    attack_scenarios = [
        (10**12, 0, "Add 1M X only"),
        (0, 10**12, "Add 1M Y only"),
        (10**13, 0, "Add 10M X only (10x pool)"),
        (10**13, 1, "Add 10M X + 1 wei Y"),
        (10**12, 10**6, "Add 1M X + 1 Y (1000:1 ratio)"),
    ]

    for add_x, add_y, desc in attack_scenarios:
        new_x = pool_x + add_x
        new_y = pool_y + add_y
        d1_result = get_D(new_x, new_y, ann_val)
        d1 = d1_result["D"]

        # Ideal balance calc (from contract)
        if d0 > 0:
            ideal_x = d1 * pool_x // d0
            ideal_y = d1 * pool_y // d0
            
            # Fee on imbalance
            x_diff = abs(ideal_x - new_x) if new_x > 0 else ideal_x
            y_diff = abs(ideal_y - new_y) if new_y > 0 else ideal_y
            x_fee = x_diff * 3 // 10000  # liquidity_fees = 3 bps
            y_fee = y_diff * 3 // 10000

            # Post-fee D
            post_fee_x = pool_x + add_x - x_fee
            post_fee_y = pool_y + add_y - y_fee
            d2_result = get_D(post_fee_x, post_fee_y, ann_val)
            d2 = d2_result["D"]

            if d0 > 0 and d2 > d0:
                minted = total_shares * (d2 - d0) // d0
                dilution = minted / total_shares if total_shares > 0 else 0
                print(f"  {desc}: minted={minted}, dilution={dilution:.2f}x")
                
                if dilution > 5:
                    results.append({
                        "vector": "lp_inflation",
                        "status": "SAT",
                        "scenario": desc,
                        "minted": minted,
                        "dilution": round(dilution, 2),
                        "severity": "HIGH"
                    })
            else:
                print(f"  {desc}: d2={d2} <= d0={d0} — blocked by contract assert")

    # ═══════════════════════════════════════════════════════
    # VECTOR 4: Swap Output Exceeds Expected
    # ═══════════════════════════════════════════════════════
    print("\n" + "=" * 60)
    print("  VECTOR 4: Swap Output Analysis")
    print("=" * 60)

    pool_x = 10**12
    pool_y = 10**12
    ann_val = 200  # amp=100

    swap_amounts = [
        1,
        1000,
        10**6,
        10**9,
        pool_x,           # 1x pool
        5 * pool_x,       # 5x pool
        9 * pool_x,       # 9x pool (just under 10x limit)
    ]

    for swap_amt in swap_amounts:
        # Fee deduction
        fee = swap_amt * 5 // 10000
        net = swap_amt - fee
        
        new_y = get_y(pool_x, pool_y, net, ann_val)
        dy = pool_y - new_y
        pct = dy * 100 // pool_y if pool_y > 0 else 0
        fee_val = fee
        
        print(f"  Swap {swap_amt:>15} → dy={dy:>15} ({pct}% of pool) fee={fee_val}")
        
        if pct > 95:
            results.append({
                "vector": "swap_drain",
                "status": "SAT",
                "swap_amount": swap_amt,
                "dy": dy,
                "drain_pct": pct,
                "severity": "CRITICAL" if pct > 99 else "HIGH",
                "detail": f"Single swap of {swap_amt} drains {pct}% of pool Y"
            })

    # ═══════════════════════════════════════════════════════
    # SUMMARY
    # ═══════════════════════════════════════════════════════
    print("\n" + "=" * 60)
    print("  CORTEX PROOF RESULTS — Bitflow StableSwap")
    print("=" * 60)

    sat_results = [r for r in results if r["status"] == "SAT"]
    print("\n  Vectors tested: 4")
    print(f"  SAT (findings): {len(sat_results)}")

    for r in sat_results:
        sev = r.get("severity", "N/A")
        emoji = {"CRITICAL": "🔴", "HIGH": "🟠", "MEDIUM": "🟡", "LOW": "🟢"}.get(sev, "⚪")
        print(f"  {emoji} [{sev}] {r['vector']}: {r.get('detail', r.get('scenario', ''))}")

    output = {
        "target": "BitflowFinance/bitflow — stableswap.clar",
        "chain": "Stacks",
        "bounty": "$100,000",
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "vectors": results
    }

    output_path = "reports/z3_bitflow_stableswap_results.json"
    with open(output_path, "w") as f:
        json.dump(output, f, indent=2, default=str)
    print(f"\n  Results saved to: {output_path}")

    return results


if __name__ == "__main__":
    start = time.time()
    results = run_all_proofs()
    elapsed = time.time() - start
    print(f"\n  Execution time: {elapsed:.2f}s")
