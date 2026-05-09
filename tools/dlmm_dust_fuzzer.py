import random
import time

print("==========================================================")
print("🐍 [OUROBOROS] DLMM Dust Drainage Fuzzer (C5-REAL)")
print("==========================================================")

PRICE_SCALE = 1000000


def simulate_dlmm_cycle(x_bal, y_bal, total_shares, bin_price, dep_x, dep_y):
    # Initial Liquidity Value
    l_pre = x_bal * bin_price + y_bal
    if l_pre == 0:
        return 0

    # Deposit Value
    l_deposit = dep_x * bin_price + dep_y

    # 1. Deposit (Mint Shares) - Floor Division
    shares_minted = (l_deposit * total_shares) // l_pre
    if shares_minted == 0:
        return 0

    # New State
    x_bal_new = x_bal + dep_x
    y_bal_new = y_bal + dep_y
    total_shares_new = total_shares + shares_minted

    # 2. Withdraw (Burn exactly what we minted) - Floor Division
    withdraw_x = (shares_minted * x_bal_new) // total_shares_new
    withdraw_y = (shares_minted * y_bal_new) // total_shares_new

    # Net Value Withdrawn
    l_withdraw = withdraw_x * bin_price + withdraw_y

    return l_withdraw - l_deposit


def run_fuzzer(iterations=1000000):
    print(f"[*] Fuzzing {iterations} liquidity/deposit combinations...")
    start_time = time.time()
    found = 0

    for i in range(iterations):
        # Realistic bin parameters
        x_bal = random.randint(100, 1000000)
        y_bal = random.randint(100, 1000000)
        total_shares = random.randint(100, 1000000)
        bin_price = random.randint(1, 2000)

        # Attacker uses small dust amounts
        dep_x = random.randint(0, 10)
        dep_y = random.randint(0, 10)
        if dep_x == 0 and dep_y == 0:
            continue

        profit = simulate_dlmm_cycle(x_bal, y_bal, total_shares, bin_price, dep_x, dep_y)

        if profit > 0:
            found += 1
            if found <= 5:
                print(f"\n[!] VULNERABILITY DETECTED (Profit: {profit})")
                print(f"    Bin: x={x_bal}, y={y_bal}, shares={total_shares}, price={bin_price}")
                print(f"    Dep: x={dep_x}, y={dep_y} (Value: {dep_x * bin_price + dep_y})")

    end_time = time.time()
    print(f"\n[*] Fuzzing complete in {end_time - start_time:.2f}s")
    print(f"[*] Total exploits found: {found}")

    if found > 0:
        print("\n[RESULT] C5-REAL: Rounding asymmetry confirmed in DLMM bin logic.")
    else:
        print("\n[RESULT] SAFE: Floor division in both directions protects the pool.")


if __name__ == "__main__":
    run_fuzzer()
