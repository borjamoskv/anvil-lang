import asyncio
from playwright.async_api import async_playwright
import os
import sys
import hashlib
from datetime import datetime, timezone

# ====================================================================
# [C5-REAL] CORTEX-Persist: Ouroboros Auto-Submit (Immunefi Bridge)
# ====================================================================
# Este script secuestra tu navegador anfitrión vía CDP (puerto 9222)
# para inyectar automáticamente el payload y el reporte matemático en 
# el dashboard de Immunefi sin perder la sesión de tu wallet Web3.
# ====================================================================

REPORT_PATH = os.path.expanduser("~/10_PROJECTS/anvil-lang/docs/immunefi_oracle_report_final.md")
ZIP_PATH = os.path.expanduser("~/10_PROJECTS/anvil-lang/CORTEX_Immunefi_Payload.zip")

def generate_cortex_taint(file_path: str) -> str:
    """Genera la firma criptográfica CORTEX-TAINT (SHA3-256) del payload."""
    sha3 = hashlib.sha3_256()
    with open(file_path, "rb") as f:
        for chunk in iter(lambda: f.read(4096), b""):
            sha3.update(chunk)
    timestamp = datetime.now(timezone.utc).isoformat()
    return f"\n\n---\n**CORTEX-TAINT**: `taint:ouroboros:session_01:{timestamp}:{sha3.hexdigest()}`"

async def run():
    print("==========================================================")
    print("🐍 [OUROBOROS] CORTEX Auto-Submit Protocol Initiated")
    print("==========================================================")
    
    if not os.path.exists(REPORT_PATH) or not os.path.exists(ZIP_PATH):
        print("[!] Error: Faltan los artefactos del payload (Report.md o el ZIP).")
        sys.exit(1)

    with open(REPORT_PATH, 'r') as f:
        report_content = f.read()

    # Generar Taint Criptográfico
    print("[*] Calculando entropía y firma criptográfica SHA3-256 del payload...")
    cortex_taint = generate_cortex_taint(ZIP_PATH)
    report_content += cortex_taint

    print("[*] Conectando con el navegador anfitrión (CDP puerto 9222)...")
    
    async with async_playwright() as p:
        browser = None
        try:
            # Nos conectamos a tu Chrome/Brave real abierto con --remote-debugging-port=9222
            browser = await p.chromium.connect_over_cdp("http://localhost:9222")
            contexts = browser.contexts
            if not contexts:
                print("[!] No se encontró contexto activo. ¿Está Chrome abierto?")
                return
            
            page = contexts[0].pages[0]
            print("[+] Enlace CDP establecido.")
            
            # Buscar la pestaña de Immunefi abierta
            immunefi_page = None
            for p_ctx in contexts[0].pages:
                if "bugs.immunefi.com/dashboard/new-submission" in p_ctx.url:
                    immunefi_page = p_ctx
                    break
            
            if not immunefi_page:
                print("[*] Pestaña de Immunefi no detectada. Navegando...")
                immunefi_page = contexts[0].pages[0]
                await immunefi_page.goto("https://bugs.immunefi.com/dashboard/new-submission")
                await immunefi_page.wait_for_load_state("networkidle")
            else:
                print(f"[+] Pestaña Immunefi localizada: {immunefi_page.title()}")
                await immunefi_page.bring_to_front()

            print("[*] Inyectando Vector de Vulnerabilidad (C5-REAL)...")
            
            # 1. Rellenar Título (Esperando dinámicamente al DOM)
            title_selector = 'input[name="title"]'
            await immunefi_page.wait_for_selector(title_selector, state="visible", timeout=15000)
            await immunefi_page.fill(title_selector, 'CRITICAL: Spot Price Oracle Manipulation via Flashloan (Integer Over/Underflow)')
            
            # 2. Seleccionar Smart Contract / Blockchain
            # Asumiremos la inyección del reporte en el textarea principal
            print("[*] Escribiendo reporte Z3 y firma criptográfica...")
            desc_selector = 'textarea[name="bugDescription"]'
            await immunefi_page.wait_for_selector(desc_selector, state="visible", timeout=15000)
            await immunefi_page.fill(desc_selector, report_content)

            # 3. Subir el ZIP (Proof of Concept)
            print("[*] Adjuntando el payload del exploit (ZIP)...")
            file_selector = 'input[type="file"]'
            await immunefi_page.wait_for_selector(file_selector, state="attached", timeout=15000)
            file_input = immunefi_page.locator(file_selector)
            await file_input.set_input_files(ZIP_PATH)

            # Esperar a que la carga del archivo se procese visualmente
            await immunefi_page.wait_for_load_state("domcontentloaded")

            print("==========================================================")
            print("🛡️ [C5-REAL] PAYLOAD INYECTADO Y FIRMADO CRIPTOGRÁFICAMENTE.")
            print(f"Firma: {cortex_taint.strip()}")
            print("El formulario está lleno. Revisa manualmente la Severidad (Critical)")
            print("y presiona 'Submit' en tu navegador para sellar el contrato.")
            print("==========================================================")
            
        except Exception as e:
            print(f"[!] Error de inyección/CDP: {e}")
            print("\nInstrucciones para activar CDP en Mac:")
            print("/Applications/Google\\ Chrome.app/Contents/MacOS/Google\\ Chrome --remote-debugging-port=9222 --no-first-run --no-default-browser-check --user-data-dir=$(mktemp -d -t 'chrome-remote_data_dir')")
        finally:
            if browser:
                print("[*] Cerrando enlace CDP de forma segura...")
                await browser.disconnect()

if __name__ == "__main__":
    asyncio.run(run())
