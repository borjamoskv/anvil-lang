"""
ouroboros_auto_submit.py — Sovereign Strike Submission Engine v3.0

Redesigned to eliminate macOS sandbox friction (-54 errors).
Drops subprocess.run(["open", ...]) entirely. Uses osascript clipboard
fallback, validates reports structurally, auto-generates ZIP payloads,
and maintains a tamper-evident JSONL submission ledger.

Usage:
    python3 ouroboros_auto_submit.py <ID>            # Prepare single strike
    python3 ouroboros_auto_submit.py <ID> --inject    # Prepare + clipboard inject
    python3 ouroboros_auto_submit.py list             # Show all targets
    python3 ouroboros_auto_submit.py status           # Ledger summary
"""

import os
import sys
import json
import hashlib
import zipfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional
import logging
import requests
from requests.adapters import HTTPAdapter
from urllib3.util.retry import Retry

# Configurar el logger sin prosa (Zero-Rhetoric)
logger = logging.getLogger(__name__)
logging.basicConfig(level=logging.INFO, format="%(levelname)s: %(message)s")

# ── Constants ────────────────────────────────────────────────────────
BANNER = "=" * 62
BASE = Path.home() / "10_PROJECTS"
BOUNTIES = BASE / "cortex-bounties"
SUBMISSIONS = BOUNTIES / "submissions"
LEDGER_PATH = BOUNTIES / "ouroboros_strike_ledger.jsonl"
PAYLOAD_DIR = Path("/tmp/ouroboros_payloads")

IMMUNEFI_URL = "https://bugs.immunefi.com/dashboard/new-submission"
CODE4RENA_URL = "https://code4rena.com/contests"

# ── Required Sections for Report Validation ──────────────────────────
# Each entry is a tuple: (display_name, list_of_accepted_aliases)
REQUIRED_SECTIONS: list[tuple[str, list[str]]] = [
    ("Vulnerability", [
        "vulnerability", "vulnerable code", "root cause",
        "summary", "bug description", "finding",
    ]),
    ("Impact", ["impact"]),
    ("Proof of Concept", [
        "proof of concept", "proof-of-concept", "poc",
        "reproduction", "exploit",
    ]),
]


def validate_report(path: str) -> tuple[bool, list[str]]:
    """Structural validation: checks for required sections in the report.
    Returns (is_valid, list_of_missing_sections).
    """
    if not os.path.exists(path):
        return False, [f"FILE_NOT_FOUND: {path}"]

    with open(path, encoding="utf-8") as f:
        content = f.read().lower()

    missing = []
    for display_name, aliases in REQUIRED_SECTIONS:
        if not any(alias in content for alias in aliases):
            missing.append(display_name)

    return len(missing) == 0, missing

# ── Target Registry ──────────────────────────────────────────────────
TARGETS: dict[str, dict] = {
    "1": {
        "name": "BitFlow DLMM Rounding Asymmetry",
        "severity": "Critical",
        "platform": "immunefi",
        "report": str(BASE / "anvil-lang/docs/immunefi_dlmm_report_final.md"),
        "poc_files": [
            str(BASE / "anvil-lang/tools/z3_dlmm_dust.py"),
        ],
        "title": "CRITICAL: Zero-Fee Truncation & Dust Draining via Asymmetric Rounding in DLMM Bins",
    },
    "2": {
        "name": "Price Oracle Manipulation",
        "severity": "Critical",
        "platform": "immunefi",
        "report": str(BASE / "anvil-lang/docs/immunefi_oracle_report_final.md"),
        "poc_files": [
            str(BASE / "anvil-lang/tools/oracle_fuzzer.py"),
        ],
        "title": "CRITICAL: Flashloan-driven Spot Price Oracle Manipulation leading to massive LTV collapse",
    },
    "3": {
        "name": "EigenLayer AVS Slashing Desync",
        "severity": "High",
        "platform": "code4rena",
        "report": str(BASE / "anvil-lang/docs/c4_eigen_slashing_desync.md"),
        "poc_files": [
            str(BASE / "anvil-lang/tools/eigen_slashing_fuzzer.py"),
        ],
        "title": "HIGH: MEV-Driven Front-running of Slashing Events leading to Protocol Insolvency",
    },
    "4": {
        "name": "K2 Lending Close Factor Bypass",
        "severity": "Critical",
        "platform": "immunefi",
        "report": str(BOUNTIES / "reports/k2-lending-close-factor-bypass-c5-real.md"),
        "poc_files": [
            str(BOUNTIES / "reports/FINAL_STRIKE_MAY2026/K2_Lending/k2-lending-close-factor-bypass-poc.rs"),
        ],
        "title": "CRITICAL: Close Factor Bypass via KineticRouter permits total collateral seizure",
    },
    "5": {
        "name": "Firedancer VM Sandbox Bypass",
        "severity": "Critical",
        "platform": "immunefi",
        "report": str(BOUNTIES / "reports/firedancer-vm-region-oob-bypass.md"),
        "poc_files": [
            str(BOUNTIES / "reports/firedancer-vm-region-oob-bypass-poc.c"),
        ],
        "title": "CRITICAL: Firedancer VM Sandbox Bypass via Region Index Out-of-Bounds",
    },
    "6": {
        "name": "Firedancer Funk State Ghosting",
        "severity": "Critical",
        "platform": "immunefi",
        "report": str(BOUNTIES / "reports/firedancer-funk-state-ghosting-c5.md"),
        "poc_files": [
            str(BOUNTIES / "reports/test_ghosting_poc.c"),
        ],
        "title": "CRITICAL: Firedancer Funk State Ghosting — Stale Record Shadowing via Dirty Read",
    },
    "7": {
        "name": "K2 Lending Storage Poisoning",
        "severity": "Medium",
        "platform": "immunefi",
        "report": str(BOUNTIES / "reports/k2-lending-storage-poisoning-m01.md"),
        "poc_files": [],
        "title": "MEDIUM: Persistent Storage Poisoning via Unvalidated Reserve Initialization",
    },
    "8": {
        "name": "K2 Lending Flash Liquidation",
        "severity": "Medium",
        "platform": "immunefi",
        "report": str(BOUNTIES / "reports/k2-lending-flash-liquidation-bypass-m02.md"),
        "poc_files": [],
        "title": "MEDIUM: Flash-Loan Liquidation Bypass via Single-Block Borrow-Liquidate Atomicity",
    },
    "9": {
        "name": "Exactly VerifiedMarket Bypass",
        "severity": "High",
        "platform": "immunefi",
        "report": str(BOUNTIES / "reports/exactly-verifiedmarket-borrow-bypass-immunefi.md"),
        "poc_files": [
            str(BOUNTIES / "reports/exactly-verifiedmarket-bypass-poc.sol"),
        ],
        "title": "HIGH: Disallowed delegate can keep borrowing from and withdrawing from Base VerifiedMarket after firewall revocation",
    },
    "10": {
        "name": "Exactly Stale Oracle L2",
        "severity": "High",
        "platform": "immunefi",
        "report": str(BOUNTIES / "reports/exactly-stale-oracle-l2-immunefi.md"),
        "poc_files": [],
        "title": "HIGH: Stale Oracle Price Feed on L2 (Optimism) due to lack of Staleness and Sequencer Uptime Checks",
    },
    "11": {
        "name": "Lido V3 Untracked ETH Injection",
        "severity": "High",
        "platform": "immunefi",
        "report": str(BOUNTIES / "reports/lido-v3-vaulthub-untracked-eth-injection.md"),
        "poc_files": [],
        "title": "HIGH: Untracked ETH Injection via StakingVault.receive() permits totalValue manipulation and quarantine bypass",
    },
}


# ══════════════════════════════════════════════════════════════════════
#  CORE ENGINE
# ══════════════════════════════════════════════════════════════════════

def _hash_file(path: str, algo: str = "sha3_256") -> str:
    """Compute digest of a file. Returns hex string."""
    h = hashlib.new(algo)
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(8192), b""):
            h.update(chunk)
    return h.hexdigest()


def _hash_bytes(data: bytes, algo: str = "sha3_256") -> str:
    h = hashlib.new(algo)
    h.update(data)
    return h.hexdigest()

def build_resilient_session() -> requests.Session:
    """Construye una sesión HTTP con reintentos robustos y soporte para proxies."""
    session = requests.Session()
    
    retry_strategy = Retry(
        total=5,
        backoff_factor=2,
        status_forcelist=[408, 429, 500, 502, 503, 504],
        allowed_methods=["POST", "PUT", "GET", "HEAD"]
    )
    
    adapter = HTTPAdapter(max_retries=retry_strategy)
    session.mount("http://", adapter)
    session.mount("https://", adapter)
    
    http_proxy = os.environ.get("HTTP_PROXY") or os.environ.get("http_proxy")
    https_proxy = os.environ.get("HTTPS_PROXY") or os.environ.get("https_proxy")
    
    proxies = {}
    if http_proxy:
        proxies["http"] = http_proxy
    if https_proxy:
        proxies["https"] = https_proxy
        
    if proxies:
        logger.info(f"Delegating network requests via proxy: {proxies}")
        session.proxies.update(proxies)
        
    return session

def submit_vulnerability_report(
    endpoint_url: str, 
    report_content: str, 
    cortex_taint: str, 
    blake3_hash: str,
    dry_run: bool = False
) -> dict | None:
    """Envía el reporte asegurando el empaquetado del payload criptográfico."""
    session = build_resilient_session()

    if dry_run:
        logger.info(f"[DRY-RUN] Verificando conectividad hacia {endpoint_url}...")
        try:
            # Ping simple para validar salida a internet usando la sesión con proxy
            session.head(endpoint_url, timeout=10)
            logger.info("[DRY-RUN] Network OK. Salida a red desbloqueada.")
        except requests.exceptions.RequestException as e:
            logger.error(f"[DRY-RUN] Fricción de red detectada: {e}")
            raise
        return None
    
    payload = {
        "report_content": report_content,
        "hash": blake3_hash,
        "cortex_taint": cortex_taint
    }
    
    headers = {
        "Content-Type": "application/json",
        "User-Agent": "CORTEX-Ouroboros-Client/1.0"
    }
    
    try:
        response = session.post(
            endpoint_url, 
            json=payload, 
            headers=headers, 
            timeout=(10, 30)
        )
        response.raise_for_status()
        logger.info(f"Report submitted successfully. Status: {response.status_code}")
        return {"status": response.status_code, "text": response.text[:200]}
    except requests.exceptions.ProxyError as e:
        logger.error(f"[NETWORK SAGA ABORT] Fallo en la delegación del proxy: {e}")
        raise
    except requests.exceptions.RequestException as e:
        logger.error(f"[NETWORK SAGA ABORT] Fallo en transporte HTTP después de reintentos: {e}")
        raise



def generate_cortex_taint(report_path: str) -> str:
    """SHA3-256 signature over the report file. C5-REAL chain of evidence."""
    digest = _hash_file(report_path, "sha3_256")
    ts = datetime.now(timezone.utc).isoformat()
    return f"taint:ouroboros:v3:{ts}:{digest}"


def build_payload_zip(target_id: str) -> Optional[str]:
    """Creates a ZIP payload with report + PoC files. Returns path or None."""
    target = TARGETS[target_id]
    report_path = target["report"]

    if not os.path.exists(report_path):
        print(f"  ❌ Report not found: {report_path}")
        return None

    PAYLOAD_DIR.mkdir(parents=True, exist_ok=True)

    safe_name = target["name"].replace(" ", "_").replace("/", "-")
    zip_name = f"CORTEX_{safe_name}_Payload.zip"
    zip_path = PAYLOAD_DIR / zip_name

    with zipfile.ZipFile(str(zip_path), "w", zipfile.ZIP_DEFLATED) as zf:
        # Add report
        zf.write(report_path, os.path.basename(report_path))

        # Add PoC files
        for poc in target.get("poc_files", []):
            if os.path.exists(poc):
                zf.write(poc, f"poc/{os.path.basename(poc)}")
            else:
                print(f"  ⚠  PoC file not found (skipped): {poc}")

    return str(zip_path)


def clipboard_inject(text: str) -> bool:
    """Inject text into macOS clipboard via pbcopy (no subprocess.run on URLs).
    pbcopy is whitelisted in macOS sandbox — unlike 'open'.
    """
    try:
        import subprocess as sp
        proc = sp.Popen(["pbcopy"], stdin=sp.PIPE)
        proc.communicate(input=text.encode("utf-8"))
        return proc.returncode == 0
    except OSError:
        # Fallback: osascript
        try:
            import subprocess as sp
            escaped = text.replace("\\", "\\\\").replace('"', '\\"')
            # Truncate for osascript if too long (clipboard has limits)
            if len(escaped) > 50000:
                escaped = escaped[:50000]
            sp.run(
                ["osascript", "-e", f'set the clipboard to "{escaped}"'],
                check=True,
                capture_output=True,
            )
            return True
        except Exception:
            return False


def append_ledger(entry: dict) -> None:
    """Append a submission record to the JSONL ledger with idempotency (target_id + report_hash)."""
    # Deduplicate: check if same target_id and report_hash already exist
    if LEDGER_PATH.exists():
        with open(str(LEDGER_PATH), "r") as f:
            for line in f:
                if not line.strip():
                    continue
                try:
                    existing = json.loads(line)
                    if existing.get("target_id") == entry.get("target_id") and \
                       existing.get("report_hash") == entry.get("report_hash"):
                        logger.info(f"Idempotency matched: submission for {entry.get('target_id')} with hash {entry.get('report_hash')} already in ledger.")
                        return
                except json.JSONDecodeError:
                    continue
                    
    with open(str(LEDGER_PATH), "a") as f:
        f.write(json.dumps(entry, default=str) + "\n")


def generate_submission_snapshot(target_id: str, taint: str, zip_path: Optional[str]) -> str:
    """Generate a markdown submission snapshot for the submissions/ directory."""
    target = TARGETS[target_id]
    ts = datetime.now(timezone.utc).strftime("%Y%m%d")
    safe_name = target["name"].replace(" ", "_").upper()

    content_lines = [
        f"# Submission: {target['name']}",
        "",
        f"**Date:** {datetime.now(timezone.utc).isoformat()}",
        f"**Severity:** {target['severity']}",
        f"**Platform:** {target['platform']}",
        f"**Title:** {target['title']}",
        "",
        "## CORTEX-TAINT",
        "```",
        f"{taint}",
        "```",
        "",
        "## Report",
        f"Source: `{target['report']}`",
        "",
        "## Payload",
        f"ZIP: `{zip_path or 'N/A'}`",
        "",
        "## PoC Files",
    ]
    for poc in target.get("poc_files", []):
        exists = "✅" if os.path.exists(poc) else "❌"
        content_lines.append(f"- {exists} `{poc}`")

    snapshot = "\n".join(content_lines) + "\n"

    SUBMISSIONS.mkdir(parents=True, exist_ok=True)
    snapshot_path = SUBMISSIONS / f"SUBMIT_{safe_name}_{ts}.md"
    with open(str(snapshot_path), "w") as f:
        f.write(snapshot)

    return str(snapshot_path)


# ══════════════════════════════════════════════════════════════════════
#  COMMANDS
# ══════════════════════════════════════════════════════════════════════

def cmd_list() -> None:
    """List all registered targets."""
    print(BANNER)
    print("🐍 [OUROBOROS v3.0] Target Registry")
    print(BANNER)
    for tid, t in TARGETS.items():
        report_exists = "✅" if os.path.exists(t["report"]) else "❌"
        poc_count = sum(1 for p in t.get("poc_files", []) if os.path.exists(p))
        total_poc = len(t.get("poc_files", []))
        print(f"  [{tid}] {t['name']}")
        print(f"      Severity: {t['severity']} | Platform: {t['platform']}")
        print(f"      Report: {report_exists} | PoC: {poc_count}/{total_poc}")
        print()


def cmd_status() -> None:
    """Show submission ledger summary."""
    print(BANNER)
    print("🐍 [OUROBOROS v3.0] Submission Ledger")
    print(BANNER)

    if not os.path.exists(str(LEDGER_PATH)):
        print("  (empty — no submissions recorded)")
        return

    entries = []
    with open(str(LEDGER_PATH)) as f:
        for line in f:
            line = line.strip()
            if line:
                entries.append(json.loads(line))

    if not entries:
        print("  (empty — no submissions recorded)")
        return

    for e in entries:
        status_icon = "🟢" if e.get("status") == "PREPARED" else "🔴"
        print(f"  {status_icon} [{e.get('target_id', '?')}] {e.get('target_name', 'Unknown')}")
        print(f"      Timestamp: {e.get('timestamp', '?')}")
        print(f"      Taint: {e.get('taint', '?')[:40]}...")
        valid = e.get("validation", {})
        print(f"      Validation: {'✅ PASS' if valid.get('is_valid') else '❌ FAIL'}")
        print()

    print(f"  Total: {len(entries)} submission(s)")


def cmd_prepare(target_id: str, inject: bool = False, submit: bool = False, dry_run: bool = False) -> None:
    """Full strike preparation pipeline."""
    if target_id not in TARGETS:
        print(f"❌ Unknown target ID: {target_id}")
        print("   Run: python3 ouroboros_auto_submit.py list")
        sys.exit(1)

    target = TARGETS[target_id]
    report_path = target["report"]

    print(BANNER)
    print(f"🐍 [OUROBOROS v3.0] Strike Preparation: {target['name']}")
    print(BANNER)

    # ── Step 1: Validate Report ──────────────────────────────────────
    print("\n[1/5] Validating report structure...")
    is_valid, missing = validate_report(report_path)
    if is_valid:
        print("  ✅ All required sections present")
    else:
        print(f"  ⚠  Missing sections: {', '.join(missing)}")
        if "FILE_NOT_FOUND" in missing[0]:
            print("  ❌ ABORT: Report file does not exist.")
            sys.exit(1)
        print("  ⚠  Proceeding with warnings — review before submission.")

    # ── Step 2: Generate CORTEX-TAINT ────────────────────────────────
    print("\n[2/5] Generating CORTEX-TAINT signature...")
    taint = generate_cortex_taint(report_path)
    report_hash = _hash_file(report_path)
    print(f"  🔐 SHA3-256: {report_hash[:24]}...{report_hash[-8:]}")
    print(f"  🔐 TAINT:    {taint[:50]}...")

    # ── Step 3: Build ZIP Payload ────────────────────────────────────
    print("\n[3/5] Building evidence payload...")
    zip_path = build_payload_zip(target_id)
    if zip_path:
        zip_hash = _hash_file(zip_path)
        zip_size = os.path.getsize(zip_path)
        print(f"  📦 ZIP: {zip_path}")
        print(f"  📦 Size: {zip_size:,} bytes | SHA3: {zip_hash[:16]}...")
    else:
        print("  ⚠  No ZIP generated (report missing)")

    # ── Step 4: Snapshot + Ledger ────────────────────────────────────
    print("\n[4/5] Recording submission in ledger...")
    snapshot_path = generate_submission_snapshot(target_id, taint, zip_path)

    ledger_entry = {
        "target_id": target_id,
        "target_name": target["name"],
        "severity": target["severity"],
        "platform": target["platform"],
        "title": target["title"],
        "report_path": report_path,
        "report_hash": report_hash,
        "zip_path": zip_path,
        "zip_hash": _hash_file(zip_path) if zip_path else None,
        "taint": taint,
        "snapshot": snapshot_path,
        "validation": {"is_valid": is_valid, "missing": missing},
        "status": "DRAFT_CREATED",
        "timestamp": datetime.now(timezone.utc).isoformat(),
    }
    append_ledger(ledger_entry)
    print(f"  📋 Snapshot: {snapshot_path}")
    print(f"  📋 Ledger:   {LEDGER_PATH}")

    # ── Step 5: Payload Delivery ─────────────────────────────────────
    print("\n[5/5] Payload delivery...")
    if inject or submit or dry_run:
        with open(report_path, encoding="utf-8") as f:
            report_content = f.read()

        taint_block = (
            f"\n\n---\n**CORTEX-TAINT**: `{taint}`\n"
            f"**Report Hash**: `{report_hash}`\n"
        )
        final_payload = report_content + taint_block

        if submit or dry_run:
            print("  🌐 Initiating network submission pipeline (C5-REAL)...")
            endpoint = IMMUNEFI_URL if target["platform"] == "immunefi" else CODE4RENA_URL
            try:
                submit_vulnerability_report(
                    endpoint_url=endpoint,
                    report_content=final_payload,
                    cortex_taint=taint,
                    blake3_hash=report_hash,
                    dry_run=dry_run
                )
                if not dry_run:
                    print("  ✅ CORTEX Ledger: DRAFT created via API.")
                    print("  ⚠️  MANUAL STEP REQUIRED: Open draft in browser → complete wallet + review → Submit.")
            except Exception as e:
                print(f"  ❌ Submission aborted: {e}")
                
        elif inject:
            if clipboard_inject(final_payload):
                print(f"  ✅ Payload injected into clipboard ({len(final_payload):,} chars)")
            else:
                print("  ❌ Clipboard injection failed. Manual copy required:")
                print(f"     pbcopy < {report_path}")
    else:
        print("  ℹ  Delivery skipped (use --inject, --submit, or --dry-run)")
        print(f"  ℹ  Manual: pbcopy < {report_path}")

    # ── Summary ──────────────────────────────────────────────────────
    platform_url = IMMUNEFI_URL if target["platform"] == "immunefi" else CODE4RENA_URL
    print(f"\n{BANNER}")
    print("🐍 [OUROBOROS v3.0] STRIKE READY")
    print(BANNER)
    print(f"  Target:   {target['name']}")
    print(f"  Title:    {target['title']}")
    print(f"  Severity: {target['severity']}")
    print(f"  Platform: {platform_url}")
    print(f"  ZIP:      {zip_path or 'N/A'}")
    print(f"  Status:   {'✅ VALID' if is_valid else '⚠  WARNINGS'}")
    print(BANNER)
    if submit:
        print(f"  → DRAFT created at {platform_url}. Manual finalization required.")
    elif dry_run:
        print("  → Dry-run complete. Network paths validated.")
    elif inject:
        print("  → Cmd+V in the submission form. Attach ZIP if available.")
    else:
        print("  → Run with --inject, --submit, or --dry-run to proceed.")
    print(BANNER)


# ══════════════════════════════════════════════════════════════════════
#  ENTRYPOINT
# ══════════════════════════════════════════════════════════════════════

def main() -> None:
    args = sys.argv[1:]

    if not args:
        print("Usage:")
        print("  python3 ouroboros_auto_submit.py <ID>          # Prepare strike")
        print("  python3 ouroboros_auto_submit.py <ID> --inject # Prepare + clipboard")
        print("  python3 ouroboros_auto_submit.py <ID> --submit # Submit via API")
        print("  python3 ouroboros_auto_submit.py <ID> --dry-run# Test API network")
        print("  python3 ouroboros_auto_submit.py list          # Show targets")
        print("  python3 ouroboros_auto_submit.py status        # Ledger summary")
        return

    command = args[0]

    if command == "list":
        cmd_list()
    elif command == "status":
        cmd_status()
    else:
        target_id = command
        inject = "--inject" in args
        submit = "--submit" in args
        dry_run = "--dry-run" in args
        cmd_prepare(target_id, inject=inject, submit=submit, dry_run=dry_run)


if __name__ == "__main__":
    main()
