"""z3_dlmm_tick_cross.py — Z3 DLMM Tick Crossing Formal Prover (C5-REAL)"""

import time
from z3 import Int, Solver, sat
from _cortex_common import PRICE_SCALE, BANNER


def build_solver() -> Solver:
    """Construct the Z3 model for tick-crossing arbitrage."""
    s = Solver()

    bin1_price = Int("bin1_price")
    bin1_x = Int("bin1_x")
    bin1_y_scaled = Int("bin1_y_scaled")
    bin2_price = Int("bin2_price")
    bin2_x = Int("bin2_x")
    bin2_y_scaled = Int("bin2_y_scaled")
    swap_in_x = Int("swap_in_x")

    s.add(bin1_price > 0)
    s.add(bin2_price > bin1_price)
    s.add(bin1_x >= 0, bin1_y_scaled > 0)
    s.add(bin2_x >= 0, bin2_y_scaled > 0)
    s.add(swap_in_x > 0)

    # Step 1: Drain Bin 1 (X -> Y)
    amount_in_1 = Int("amount_in_1")
    y_out_1 = Int("y_out_1")
    s.add(y_out_1 == bin1_y_scaled)
    s.add(amount_in_1 == (y_out_1 * PRICE_SCALE) / bin1_price)

    # Step 2: Remainder into Bin 2
    amount_in_2 = swap_in_x - amount_in_1
    y_out_2 = Int("y_out_2")
    s.add(amount_in_2 > 0)
    s.add(y_out_2 == (amount_in_2 * bin2_price) / PRICE_SCALE)

    total_y_out = y_out_1 + y_out_2

    # Step 3: Reverse swap (Y -> X)
    x_out_2 = Int("x_out_2")
    s.add(x_out_2 == (y_out_2 * PRICE_SCALE) / bin2_price)

    amount_in_rev_1 = total_y_out - y_out_2
    x_out_1 = Int("x_out_1")
    s.add(x_out_1 == (amount_in_rev_1 * PRICE_SCALE) / bin1_price)

    total_x_out = x_out_1 + x_out_2

    # Vulnerability condition: attacker exits with more X than entered
    s.add(total_x_out > swap_in_x)

    # Expose named vars for model reading
    s._named = {
        "swap_in_x": swap_in_x,
        "bin1_price": bin1_price,
        "bin1_y_scaled": bin1_y_scaled,
        "bin2_price": bin2_price,
        "bin2_y_scaled": bin2_y_scaled,
        "total_y_out": total_y_out,
        "total_x_out": total_x_out,
    }
    return s


def main() -> None:
    print(BANNER)
    print("[C5-REAL] CORTEX-Persist: Z3 DLMM Tick Crossing")
    print(BANNER)

    s = build_solver()
    v = s._named

    print("[*] Buscando asimetría de redondeo en el cruce de Bins...")
    start = time.time()
    res = s.check()
    elapsed = time.time() - start

    if res == sat:
        m = s.model()
        print(f"[!] SATISFIABLE: Money Printer encontrado en {elapsed:.2f}s!")
        print(f"\n--- Parámetros del Exploit ---")
        print(f"Swap Inicial X: {m[v['swap_in_x']].as_long()}")
        print(
            f"Bin1 Price: {m[v['bin1_price']].as_long()} "
            f"| Bin1 Y: {m[v['bin1_y_scaled']].as_long()}"
        )
        print(
            f"Bin2 Price: {m[v['bin2_price']].as_long()} "
            f"| Bin2 Y: {m[v['bin2_y_scaled']].as_long()}"
        )
        print(f"\n--- Resultado del Arbitraje ---")
        print(f"Total Y obtenido:   {m.eval(v['total_y_out']).as_long()}")
        print(f"Total X recuperado: {m.eval(v['total_x_out']).as_long()}")
    else:
        print(
            f"[*] UNSAT: Redondeo en Tick Crossing es conservador. "
            f"Tiempo: {elapsed:.2f}s"
        )


if __name__ == "__main__":
    main()
