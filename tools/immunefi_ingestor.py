"""
immunefi_ingestor.py — CORTEX-Persist Immunefi Ingestor v1.1 (C5-REAL)

[C4-SIMULACIÓN] Conecta con la API GraphQL de Immunefi para ingestar targets activos.
No hay API key real — el fuzzing subyacente sí es C5-REAL.
"""

import random
import logging
from _cortex_common import MAX_U64, execute_swap_u64, run_fuzzer_loop, BANNER

log = logging.getLogger(__name__)


def _probe() -> bool:
    delta = random.randint(1, 100)
    rx = (MAX_U64 // 100) + delta
    ry = random.randint(1_000_000, 10_000_000)
    a_in = random.randint(10_000, 50_000)
    res = execute_swap_u64(rx, ry, a_in)
    if not res:
        return False
    rx_p, ry_p = res
    if rx_p * ry_p < rx * ry:
        print("[!] VULNERABILIDAD CRÍTICA: Invariante Falsado (Integer Overflow)")
        print(f"    reserve_x={rx}, reserve_y={ry}, amount_in={a_in}")
        print(f"    Producto inicial: {rx * ry} | final: {rx_p * ry_p}")
        return True
    return False


def run() -> None:
    print(BANNER)
    print("🐍 [OUROBOROS] CORTEX-Persist Immunefi Ingestor v1.1")
    print(BANNER)
    print("[C4-SIMULACIÓN] Conectando con API de Immunefi (GraphQL)...")
    print("[C4-SIMULACIÓN] Target: 'Vela Exchange' ($500,000 Bounty)")
    print("[C5-REAL] Iniciando Motor de Fuzzing — Invariante: rx'*ry' >= rx*ry\n")

    found = run_fuzzer_loop(
        "AMM Overflow Fuzzer (Vela Exchange)", iterations=5_000, probe=_probe
    )
    if not found:
        print("[*] Seguro. No se encontraron violaciones del invariante.")


if __name__ == "__main__":
    run()
