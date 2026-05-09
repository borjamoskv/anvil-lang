import time
from z3 import *

print("==========================================================")
print("🛡️  CORTEX-Persist: Z3 DLMM Dust Drainage (C5-REAL)")
print("==========================================================")

s = Solver()

# State variables
x_bal = Int('x_bal')
y_bal = Int('y_bal')
total_shares = Int('total_shares')
bin_price = Int('bin_price')

# Attacker transaction 1: Deposit
deposit_x = Int('deposit_x')
deposit_y = Int('deposit_y')
shares_minted = Int('shares_minted')

# Attacker transaction 2: Withdraw
withdraw_x = Int('withdraw_x')
withdraw_y = Int('withdraw_y')

# Preconditions (Realistic constraints to avoid trivial division by zero)
s.add(x_bal > 1000)
s.add(y_bal > 1000)
s.add(total_shares > 1000)
s.add(bin_price > 0)
s.add(deposit_x >= 0)
s.add(deposit_y >= 0)
s.add(deposit_x + deposit_y > 0)

# Liquidity Value (L) calculations
L_pre = x_bal * bin_price + y_bal
L_deposit = deposit_x * bin_price + deposit_y

# 1. Deposit Logic (VULNERABLE: Proportional value without min-shares check)
# Some implementations use (dx * total_shares / total_x) which is safer,
# but using the total value (L_deposit * total_shares / L_pre) can lead to rounding arbitrage.
s.add(shares_minted == (L_deposit * total_shares) / L_pre)

# Invariant: Attacker MUST get at least 1 share to perform a withdrawal
s.add(shares_minted >= 1)

# Intermediate State
x_bal_1 = x_bal + deposit_x
y_bal_1 = y_bal + deposit_y
total_shares_1 = total_shares + shares_minted

# 2. Withdraw Logic (Burn exactly the shares just minted)
s.add(withdraw_x == (shares_minted * x_bal_1) / total_shares_1)
s.add(withdraw_y == (shares_minted * y_bal_1) / total_shares_1)

L_withdraw = withdraw_x * bin_price + withdraw_y

# 3. The Vulnerability Condition (Dust Drainage / Value Extraction)
# Can the attacker withdraw strictly MORE value than they deposited?
s.add(L_withdraw > L_deposit)

# To find practical micro-draining, we constrain the deposit size
s.add(deposit_x < x_bal / 10)  # Attack with small capital

print("[*] Running SMT Solver for Arbitrage/Dust Drainage Trajectories...")
start = time.time()
res = s.check()
elapsed = time.time() - start

if res == sat:
    print(f"[!] SATISFIABLE: Exploit Model Found in {elapsed:.2f}s")
    m = s.model()
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
    
    l_dep = m[deposit_x].as_long() * m[bin_price].as_long() + m[deposit_y].as_long()
    l_wit = m[withdraw_x].as_long() * m[bin_price].as_long() + m[withdraw_y].as_long()
    print(f"\nNet Value Deposited: {l_dep}")
    print(f"Net Value Withdrawn: {l_wit}")
    print(f"Profit per cycle:    {l_wit - l_dep}")
else:
    print(f"[*] UNSAT: No direct deterministic drainage found under current constraints in {elapsed:.2f}s.")
    print("    This means standard floor math protects the pool from single-transaction cyclic drains.")
    print("    Next step: Model sequence of active_id tick crossings.")
