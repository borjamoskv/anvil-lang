import os
import sys
import hashlib
import subprocess
import pyperclip
from datetime import datetime, timezone

# ====================================================================
# [C5-REAL] CORTEX-Persist: Ouroboros Auto-Submit (Immunefi/C4 Bridge)
# ====================================================================

TARGETS = {
    "1": {
        "name": "BitFlow DLMM Rounding Asymmetry",
        "report": "/Users/borjafernandezangulo/10_PROJECTS/anvil-lang/docs/immunefi_dlmm_report_final.md",
        "zip": "/tmp/CORTEX_Immunefi_Payload.zip",
        "title": "CRITICAL: Zero-Fee Truncation & Dust Draining via Asymmetric Rounding in DLMM Bins"
    },
    "2": {
        "name": "Price Oracle Manipulation",
        "report": "/Users/borjafernandezangulo/10_PROJECTS/anvil-lang/docs/immunefi_oracle_report_final.md",
        "zip": "/tmp/CORTEX_Immunefi_Payload_Oracle.zip",
        "title": "CRITICAL: Flashloan-driven Spot Price Oracle Manipulation leading to massive LTV collapse"
    },
    "3": {
        "name": "EigenLayer AVS Slashing Desync",
        "report": "/Users/borjafernandezangulo/10_PROJECTS/anvil-lang/docs/c4_eigen_slashing_desync.md",
        "zip": "/tmp/CORTEX_C4_Eigen_Payload.zip",
        "title": "HIGH: MEV-Driven Front-running of Slashing Events leading to Protocol Insolvency"
    }
}

def generate_cortex_taint(file_path: str) -> str:
    sha3 = hashlib.sha3_256()
    if os.path.exists(file_path):
        with open(file_path, "rb") as f:
            for chunk in iter(lambda: f.read(4096), b""):
                sha3.update(chunk)
    else:
        sha3.update(b"placeholder")
    timestamp = datetime.now(timezone.utc).isoformat()
    return f"\n\n---\n**CORTEX-TAINT**: `taint:ouroboros:session_01:{timestamp}:{sha3.hexdigest()}`"

def run():
    print("==========================================================")
    print("🐍 [OUROBOROS] CORTEX Auto-Submit Protocol Initiated")
    print("==========================================================")
    
    if len(sys.argv) < 2:
        print("[?] Uso: python3 ouroboros_auto_submit.py <ID_TARGET>")
        print("    Targets disponibles:")
        for k, v in TARGETS.items():
            print(f"    {k}: {v['name']}")
        sys.exit(0)

    target_id = sys.argv[1]
    if target_id not in TARGETS:
        print(f"[!] Error: Target ID {target_id} no reconocido.")
        sys.exit(1)

    target = TARGETS[target_id]
    report_path = target["report"]
    zip_path = target["zip"]

    if not os.path.exists(report_path):
        print(f"[!] Error: No se encuentra el reporte en {report_path}")
        sys.exit(1)
    
    # 1. Generar Firma
    taint = generate_cortex_taint(zip_path)
    
    # 2. Leer Reporte y añadir Taint
    with open(report_path, "r") as f:
        content = f.read()
    
    final_payload = f"{content}{taint}"
    
    # 3. Inyectar en Clipboard (Bypass Sandbox)
    pyperclip.copy(final_payload)
    
    print(f"[+] PROCESO COMPLETADO PARA: {target['name']}")
    print(f"[+] TITLE: {target['title']}")
    print(f"[+] Payload inyectado en Clipboard con éxito.")
    print(f"[+] CORTEX-TAINT: {taint.strip().split(':')[-1]}")
    print("==========================================================")
    print("⚠️  MANDATO: Pega el contenido en la plataforma y adjunta el ZIP.")
    print("==========================================================")

if __name__ == "__main__":
    run()
