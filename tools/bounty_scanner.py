"""
bounty_scanner.py — CORTEX-Persist Unified Bounty Scanner (C5-REAL)

Scans AMM, Lending, and StableSwap models for invariant violations.
Uses shared primitives from _cortex_common to avoid code duplication.
"""

import random
from _cortex_common import MAX_U64, execute_swap_u64, BANNER


# ── Target Models ──────────────────────────────────────────────────────────────

class AMMPoolTarget:
    name = "AMM Constant Product (Vela/Uniswap Fork)"
    bounty = "$500,000"

    @staticmethod
    def execute_swap(reserve_x: int, reserve_y: int, amount_in: int):
        return execute_swap_u64(reserve_x, reserve_y, amount_in)


class LendingPoolTarget:
    name = "Lending Protocol Close Factor (K2/Aave Fork)"
    bounty = "$1,000,000"

    @staticmethod
    def execute_liquidation(
        collateral: int, debt: int, liquidation_amount: int
    ) -> tuple[int, int] | None:
        """Returns (remaining_debt, remaining_collateral) or None if reverted."""
        left_side = (liquidation_amount * 100) & MAX_U64
        right_side = (debt * 50) & MAX_U64
        if left_side > right_side:
            return None
        return debt - liquidation_amount, collateral - liquidation_amount


class BitFlowStableSwapTarget:
    name = "BitFlow StableSwap (Newton-Raphson Convergence)"
    bounty = "$100,000"

    @staticmethod
    def get_D(x: int, y: int, ann: int) -> tuple[int, bool]:
        s = x + y
        d = s
        for _ in range(384):
            d_p = d * d // (2 * x) * d // (2 * y)
            num = (ann * s + 2 * d_p) * d
            den = (ann - 1) * d + 3 * d_p
            if den == 0:
                break
            new_d = num // den
            if abs(new_d - d) <= 2:
                return new_d, True
            d = new_d
        return d, False


TARGET_MODELS = [AMMPoolTarget, LendingPoolTarget, BitFlowStableSwapTarget]


# ── Scanner logic ──────────────────────────────────────────────────────────────

def scan_amm(target) -> bool:
    for _ in range(5_000):
        delta = random.randint(1, 100)
        rx = (MAX_U64 // 100) + delta
        ry = random.randint(1_000_000, 10_000_000)
        a_in = random.randint(10_000, 50_000)
        res = target.execute_swap(rx, ry, a_in)
        if res and res[0] * res[1] < rx * ry:
            print(f"    [!] EXPLOIT FOUND (Invariant Broken): rx={rx}, ry={ry}, a_in={a_in}")
            return True
    return False


def scan_lending(target) -> bool:
    for _ in range(5_000):
        min_debt = (MAX_U64 // 100) + 1
        max_debt = (MAX_U64 // 50) - 1
        debt = random.randint(min_debt, max_debt)
        res = target.execute_liquidation(debt * 2, debt, debt)
        if res and res[0] == 0 and debt > 0:
            print(f"    [!] EXPLOIT FOUND (Close Factor Bypass): debt={debt}")
            return True
    return False


def scan_bitflow(target) -> bool:
    _, converged = target.get_D(x=10**12, y=1, ann=200)
    if not converged:
        print("    [!] EXPLOIT FOUND (D-Invariant Convergence failure): x=10^12, y=1, amp=100")
        return True
    return False


_SCANNERS = {
    "AMM": scan_amm,
    "Lending": scan_lending,
    "BitFlow": scan_bitflow,
}


def run_scanner() -> None:
    print(BANNER)
    print("🛡️  CORTEX-Persist: Unified Bounty Scanner (C5-REAL)")
    print(BANNER)

    for target in TARGET_MODELS:
        print(f"[*] Evaluating: {target.name} [Bounty: {target.bounty}]")
        scanner_key = next((k for k in _SCANNERS if k in target.name), None)
        exploited = _SCANNERS[scanner_key](target) if scanner_key else False
        if not exploited:
            print("    [*] Secure. No invariant violations detected.")
        print("-" * 58)


if __name__ == "__main__":
    run_scanner()
