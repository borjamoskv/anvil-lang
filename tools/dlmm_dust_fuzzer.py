"""dlmm_dust_fuzzer.py — DLMM Dust Drainage Fuzzer (C5-REAL)"""

import random
from _cortex_common import run_fuzzer_loop


def simulate_dlmm_cycle(
    x_bal: int, y_bal: int, total_shares: int,
    bin_price: int, dep_x: int, dep_y: int,
) -> int:
    l_pre = x_bal * bin_price + y_bal
    if l_pre == 0:
        return 0
    l_deposit = dep_x * bin_price + dep_y
    shares_minted = (l_deposit * total_shares) // l_pre
    if shares_minted == 0:
        return 0
    x_new = x_bal + dep_x
    y_new = y_bal + dep_y
    shares_new = total_shares + shares_minted
    withdraw_x = (shares_minted * x_new) // shares_new
    withdraw_y = (shares_minted * y_new) // shares_new
    return withdraw_x * bin_price + withdraw_y - l_deposit


def _probe() -> bool:
    x_bal = random.randint(100, 1_000_000)
    y_bal = random.randint(100, 1_000_000)
    total_shares = random.randint(100, 1_000_000)
    bin_price = random.randint(1, 2_000)
    dep_x = random.randint(0, 10)
    dep_y = random.randint(0, 10)
    if dep_x == 0 and dep_y == 0:
        return False
    profit = simulate_dlmm_cycle(x_bal, y_bal, total_shares, bin_price, dep_x, dep_y)
    if profit > 0:
        print(f"[!] VULNERABILITY (Profit: {profit})")
        print(f"    Bin: x={x_bal}, y={y_bal}, shares={total_shares}, price={bin_price}")
        print(f"    Dep: x={dep_x}, y={dep_y}")
        return True
    return False


if __name__ == "__main__":
    found = run_fuzzer_loop("DLMM Dust Drainage Fuzzer", iterations=1_000_000, probe=_probe)
    if found:
        print("[RESULT] C5-REAL: Rounding asymmetry confirmed in DLMM bin logic.")
    else:
        print("[RESULT] SAFE: Floor division in both directions protects the pool.")
