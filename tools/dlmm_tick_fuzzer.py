"""dlmm_tick_fuzzer.py — DLMM Tick Crossing Fuzzer (C5-REAL)"""

import random
from _cortex_common import PRICE_SCALE, run_fuzzer_loop


def simulate_tick_cross(
    bin1_price: int, bin2_price: int, bin1_y: int, swap_in_x: int
) -> int:
    amount_in_1_floor = (bin1_y * PRICE_SCALE) // bin1_price
    if amount_in_1_floor >= swap_in_x:
        return 0
    rem_x = swap_in_x - amount_in_1_floor
    y_out_2 = (rem_x * bin2_price) // PRICE_SCALE
    rev_x_2 = (y_out_2 * PRICE_SCALE) // bin2_price
    rev_x_1 = (bin1_y * PRICE_SCALE) // bin1_price
    return rev_x_1 + rev_x_2 - swap_in_x


def _probe() -> bool:
    bin1_price = random.randint(100, 100_000)
    bin2_price = bin1_price + random.randint(1, 1_000)
    bin1_y = random.randint(10, 10_000)
    min_x = (bin1_y * PRICE_SCALE) // bin1_price
    swap_in_x = min_x + random.randint(1, 1_000)
    profit = simulate_tick_cross(bin1_price, bin2_price, bin1_y, swap_in_x)
    if profit > 0:
        print(f"[!] MONEY PRINTER (Profit X: {profit})")
        print(f"    Bin1: {bin1_price} | Bin2: {bin2_price} | SwapIn: {swap_in_x}")
        return True
    return False


if __name__ == "__main__":
    run_fuzzer_loop("DLMM Tick Crossing Fuzzer", iterations=1_000_000, probe=_probe)
