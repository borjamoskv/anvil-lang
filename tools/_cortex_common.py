"""
_cortex_common.py — Shared primitives for the CORTEX Ouroboros toolchain.

[P0] Single source of truth for constants and fuzzing scaffolding.
All tools MUST import from here instead of re-declaring.
"""

import os
import time
from typing import Callable

# Constants
MAX_U64: int = 0xFFFF_FFFF_FFFF_FFFF
PRICE_SCALE: int = 1_000_000
BANNER: str = "=" * 58
BASE: str = os.path.expanduser("~/10_PROJECTS")


def execute_swap_u64(
    reserve_x: int,
    reserve_y: int,
    amount_in: int,
) -> tuple[int, int] | None:
    """Constant-product AMM swap with strict u64 wrapping.

    Previously duplicated verbatim in immunefi_ingestor and
    bounty_scanner. Single source of truth.

    Returns (rx', ry') or None on division by zero.
    """
    amount_in_with_fee = (amount_in * 99) & MAX_U64
    numerator = (amount_in_with_fee * reserve_y) & MAX_U64
    term1 = (reserve_x * 100) & MAX_U64
    denominator = (term1 + amount_in_with_fee) & MAX_U64
    if denominator == 0:
        return None
    amount_out = numerator // denominator
    return (reserve_x + amount_in) & MAX_U64, (reserve_y - amount_out) & MAX_U64


def run_fuzzer_loop(
    label: str,
    iterations: int,
    probe: Callable[[], bool],
    max_reports: int = 5,
) -> int:
    """Generic fuzzing loop.

    Calls `probe()` `iterations` times.
    `probe` returns True when an exploit is found and handles its
    own printing. Returns total exploit count.
    """
    print(BANNER)
    print(f"[OUROBOROS] {label}")
    print(BANNER)
    print(f"[*] Fuzzing {iterations:,} iterations...")

    start = time.time()
    found = 0

    for _ in range(iterations):
        if probe():
            found += 1
            if found > max_reports:
                continue

    elapsed = time.time() - start
    print(f"\n[*] Done in {elapsed:.2f}s — exploits found: {found}")
    return found
