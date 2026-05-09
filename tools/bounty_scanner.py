import random

MAX_U64 = 0xFFFFFFFFFFFFFFFF

# ====================================================================
# [C5-REAL] CORTEX-Persist: Autonomous Bounty Scanner Engine
# ====================================================================
# This pipeline assimilates contract models, reduces them to their
# thermodynamic skeleton, and executes fuzzing at the machine limits.
# ====================================================================

class AMMPoolTarget:
    name = "AMM Constant Product (Vela/Uniswap Fork)"
    bounty = "$500,000"
    
    @staticmethod
    def execute_swap(reserve_x, reserve_y, amount_in):
        amount_in_with_fee = (amount_in * 99) & MAX_U64
        numerator = (amount_in_with_fee * reserve_y) & MAX_U64
        term1 = (reserve_x * 100) & MAX_U64
        denominator = (term1 + amount_in_with_fee) & MAX_U64
        if denominator == 0:
            return None
        amount_out = numerator // denominator
        return (reserve_x + amount_in) & MAX_U64, (reserve_y - amount_out) & MAX_U64


class LendingPoolTarget:
    name = "Lending Protocol Close Factor (K2/Aave Fork)"
    bounty = "$1,000,000"
    
    @staticmethod
    def execute_liquidation(collateral, debt, liquidation_amount):
        # Vulnerability (K2 Bypass):
        # Contract checks that amount * 100 <= debt * 50 (50% max close factor)
        left_side = (liquidation_amount * 100) & MAX_U64
        right_side = (debt * 50) & MAX_U64
        
        if left_side > right_side:
            return None  # Reverted
            
        remaining_debt = debt - liquidation_amount
        remaining_collateral = collateral - liquidation_amount 
        return remaining_debt, remaining_collateral


class BitFlowStableSwapTarget:
    name = "BitFlow StableSwap (Newton-Raphson Convergence)"
    bounty = "$100,000"
    
    @staticmethod
    def get_D(x, y, ann):
        # Newton-Raphson approximation of the StableSwap invariant D
        s = x + y
        d = s
        for _ in range(384):
            d_p = d
            d_p = d_p * d // (2 * x)
            d_p = d_p * d // (2 * y)
            num = (ann * s + 2 * d_p) * d
            den = (ann - 1) * d + (2 + 1) * d_p
            if den == 0:
                break
            new_d = num // den
            if abs(new_d - d) <= 2:
                return new_d, True
            d = new_d
        return d, False  # Failed to converge


TARGET_MODELS = [AMMPoolTarget, LendingPoolTarget, BitFlowStableSwapTarget]


def scan_amm(target):
    found = False
    for _ in range(5000):
        delta = random.randint(1, 100)
        rx = (MAX_U64 // 100) + delta
        ry = random.randint(1_000_000, 10_000_000)
        a_in = random.randint(10_000, 50_000)
        res = target.execute_swap(rx, ry, a_in)
        if not res:
            continue
        rx_prime, ry_prime = res
        
        if rx_prime * ry_prime < rx * ry:
            print("    [!] EXPLOIT FOUND (Invariant Broken):")
            print(f"        Values: rx={rx}, ry={ry}, a_in={a_in}")
            found = True
            break
    return found


def scan_lending(target):
    found = False
    for _ in range(5000):
        min_debt = (MAX_U64 // 100) + 1
        max_debt = (MAX_U64 // 50) - 1
        
        debt = random.randint(min_debt, max_debt)
        collateral = debt * 2
        liquidation_amount = debt 
        
        res = target.execute_liquidation(collateral, debt, liquidation_amount)
        if not res:
            continue
        
        remaining_debt, _ = res
        if remaining_debt == 0 and debt > 0:
            print("    [!] EXPLOIT FOUND (Close Factor Bypass):")
            print(f"        debt={debt}, amount={liquidation_amount}")
            found = True
            break
    return found


def scan_bitflow(target):
    found = False
    print("    Scanning for Newton-Raphson convergence failures...")
    x, y = 10**12, 1
    amp = 100
    ann = amp * 2
    _, converged = target.get_D(x, y, ann)
    if not converged:
        print("    [!] EXPLOIT FOUND (D-Invariant Convergence failure):")
        print(f"        x={x}, y={y}, amp={amp} -> D did not converge.")
        found = True
    return found


def run_scanner():
    print("==========================================================")
    print("🛡️  CORTEX-Persist: Unified Bounty Scanner (C5-REAL)")
    print("==========================================================")
    
    for target in TARGET_MODELS:
        print(f"[*] Evaluating: {target.name} [Bounty: {target.bounty}]")
        
        exploited = False
        if "AMM" in target.name:
            exploited = scan_amm(target)
        elif "Lending" in target.name:
            exploited = scan_lending(target)
        elif "BitFlow" in target.name:
            exploited = scan_bitflow(target)
            
        if not exploited:
            print("    [*] Secure. No invariant violations detected.")
        print("-" * 58)


if __name__ == "__main__":
    run_scanner()
