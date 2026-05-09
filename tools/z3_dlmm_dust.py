"""z3_dlmm_dust.py — Z3 DLMM Dust Drainage Formal Prover (C5-REAL)"""

import time
from z3 import Int, Solver, sat
from _cortex_common import BANNER


def main() -> None:
    print(BANNER)
    print("[C5-REAL] CORTEX-Persist: Z3 DLMM Dust Drainage")
    print(BANNER)

    s = Solver()

    # State variables
    x_bal = Int("x_bal")
    y_bal = Int("y_bal")
    total_shares = Int("total_shares")
    bin_price = Int("bin_price")

    # Attacker: Deposit
    deposit_x = Int("deposit_x")
    deposit_y = Int("deposit_y")
    shares_minted = Int("shares_minted")

    # Attacker: Withdraw
    withdraw_x = Int("withdraw_x")
    withdraw_y = Int("withdraw_y")

    # Preconditions
    s.add(x_bal > 1000, y_bal > 1000, total_shares > 1000, bin_price > 0)
    s.add(deposit_x >= 0, deposit_y >= 0, deposit_x + deposit_y > 0)

    # Liquidity values
    L_pre = x_bal * bin_price + y_bal
    L_deposit = deposit_x * bin_price + deposit_y

    # 1. Deposit (floor division — VULNERABLE: no min-shares check)
    s.add(shares_minted == (L_deposit * total_shares) / L_pre)
    s.add(shares_minted >= 1)

    # Intermediate state
    x_bal_1 = x_bal + deposit_x
    y_bal_1 = y_bal + deposit_y
    total_shares_1 = total_shares + shares_minted

    # 2. Withdraw (burn exactly what was minted)
    s.add(withdraw_x == (shares_minted * x_bal_1) / total_shares_1)
    s.add(withdraw_y == (shares_minted * y_bal_1) / total_shares_1)

    L_withdraw = withdraw_x * bin_price + withdraw_y

    # 3. Vulnerability: can attacker withdraw MORE value than deposited?
    s.add(L_withdraw > L_deposit)
    s.add(deposit_x < x_bal / 10)  # Small-capital attack constraint

    print("[*] Running SMT Solver for Dust Drainage Trajectories...")
    start = time.time()
    res = s.check()
    elapsed = time.time() - start

    if res == sat:
        m = s.model()
        print(f"[!] SATISFIABLE: Exploit Model Found in {elapsed:.2f}s")
        print("\n--- Initial Bin State ---")
        print(f"x_bal:        {m[x_bal].as_long()}")
        print(f"y_bal:        {m[y_bal].as_long()}")
        print(f"total_shares: {m[total_shares].as_long()}")
        print(f"bin_price:    {m[bin_price].as_long()}")
        print("\n--- Attacker Action ---")
        print(f"deposit_x:    {m[deposit_x].as_long()}")
        print(f"deposit_y:    {m[deposit_y].as_long()}")
        print(f"shares_got:   {m[shares_minted].as_long()}")
        print("\n--- Extraction ---")
        print(f"withdraw_x:   {m[withdraw_x].as_long()}")
        print(f"withdraw_y:   {m[withdraw_y].as_long()}")
        l_dep = (
            m[deposit_x].as_long() * m[bin_price].as_long()
            + m[deposit_y].as_long()
        )
        l_wit = (
            m[withdraw_x].as_long() * m[bin_price].as_long()
            + m[withdraw_y].as_long()
        )
        print(f"\nNet Value Deposited: {l_dep}")
        print(f"Net Value Withdrawn: {l_wit}")
        print(f"Profit per cycle:    {l_wit - l_dep}")
    else:
        print(
            f"[*] UNSAT: No drainage found under current constraints "
            f"in {elapsed:.2f}s."
        )
        print("    Floor math protects the pool from single-tx cyclic drains.")
        print("    Next step: Model tick-crossing sequences.")


if __name__ == "__main__":
    main()
