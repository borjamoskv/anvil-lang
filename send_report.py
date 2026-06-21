#!/usr/bin/env python3
# ============================================================
# CORTEX-Persist: Sovereign Ledger Report Dispatcher (C5-REAL)
# Fuses local sqlite telemetry with Mail.app AppleScript delivery.
# ============================================================

import os
import sys
import sqlite3
import argparse
import subprocess
import datetime

DB_PATH = "anvil.db"
DEFAULT_RECIPIENT = "sealons@yahoo.es"

def get_ledger_summary(conn):
    cursor = conn.cursor()
    
    # 1. Total records
    cursor.execute("SELECT COUNT(*) FROM traceability_ledger")
    total_records = cursor.fetchone()[0]
    
    # 2. Key metrics summary
    cursor.execute("""
        SELECT metric_id, AVG(metric_value), MAX(metric_value), COUNT(*) 
        FROM traceability_ledger 
        GROUP BY metric_id
    """)
    metrics = cursor.fetchall()
    
    # 3. Recent 10 entries
    cursor.execute("""
        SELECT event_id, metric_id, metric_value, source_type, timestamp 
        FROM traceability_ledger 
        ORDER BY timestamp DESC 
        LIMIT 10
    """)
    recent_entries = cursor.fetchall()
    
    return total_records, metrics, recent_entries

def get_keys_summary(conn):
    cursor = conn.cursor()
    
    # 1. Active keys count by tier
    cursor.execute("""
        SELECT tier, COUNT(*) 
        FROM exergy_keys 
        WHERE status = 'ACTIVE' 
        GROUP BY tier
    """)
    key_tiers = cursor.fetchall()
    
    # 2. Total keys
    cursor.execute("SELECT COUNT(*) FROM exergy_keys")
    total_keys = cursor.fetchone()[0]
    
    return total_keys, key_tiers

def generate_report(total_records, metrics, recent_entries, total_keys, key_tiers):
    timestamp = datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    
    report = []
    report.append("======================================================================")
    report.append("   CORTEX-PERSIST: SOVEREIGN LEDGER & TELEMETRY REPORT (C5-REAL)")
    report.append(f"   Generated At: {timestamp}")
    report.append("======================================================================")
    report.append("")
    
    report.append("--- [1. EXERGY KEY REGISTRY STATUS] ---")
    report.append(f"Total Keys Registered: {total_keys}")
    for tier, count in key_tiers:
        report.append(f"  Tier [{tier}]: {count} active keys")
    report.append("")
    
    report.append("--- [2. LEDGER METRIC DENSITY] ---")
    report.append(f"Total Traceability Events: {total_records}")
    report.append(f"{'METRIC ID':<25} | {'AVG VALUE':<12} | {'MAX VALUE':<12} | {'EVENT COUNT':<6}")
    report.append("-" * 65)
    for metric_id, avg_val, max_val, count in metrics:
        report.append(f"{metric_id:<25} | {avg_val:<12.4f} | {max_val:<12.4f} | {count:<6}")
    report.append("")
    
    report.append("--- [3. RECENT AUDIT LEDGER EVENTS] ---")
    report.append(f"{'TIMESTAMP':<25} | {'EVENT ID':<15} | {'METRIC ID':<20} | {'VALUE':<10}")
    report.append("-" * 75)
    for event_id, metric_id, val, source, ts in recent_entries:
        truncated_event = event_id[:12] + "..." if len(event_id) > 15 else event_id
        report.append(f"{ts:<25} | {truncated_event:<15} | {metric_id:<20} | {val:<10.2f}")
    
    report.append("")
    report.append("======================================================================")
    report.append("   End of Transmission // C5-REAL Provenance Shield")
    report.append("======================================================================")
    
    return "\n".join(report)

def send_via_applescript(to_address, subject, body):
    escaped_subject = subject.replace('\\', '\\\\').replace('"', '\\"')
    escaped_body = body.replace('\\', '\\\\').replace('"', '\\"').replace('\n', '\\n')
    
    script = f'''
    tell application "Mail"
        set newMessage to make new outgoing message with properties {{subject:"{escaped_subject}", content:"{escaped_body}", visible:false}}
        tell newMessage
            make new to recipient at end of to recipients with properties {{address:"{to_address}"}}
            send
        end tell
    end tell
    '''
    
    res = subprocess.run(["osascript", "-e", script], capture_output=True, text=True, timeout=15)
    if res.returncode != 0:
        raise RuntimeError(f"AppleScript failed: {res.stderr}")
    return res.stdout

def main():
    parser = argparse.ArgumentParser(description="Send C5-REAL compiler telemetry reports via Apple Mail.")
    parser.add_argument("-t", "--to", default=DEFAULT_RECIPIENT, help="Recipient email address")
    parser.add_argument("-s", "--subject", default="[C5-REAL] Anvil Ledger & Telemetry Report", help="Email subject")
    parser.add_argument("--stdout-only", action="store_true", help="Print report to console without sending email")
    args = parser.parse_args()
    
    if not os.path.exists(DB_PATH):
        print(f"[✗] Database not found at {DB_PATH}. Run compiler check or build first.", file=sys.stderr)
        sys.exit(1)
        
    try:
        conn = sqlite3.connect(DB_PATH)
        total_records, metrics, recent_entries = get_ledger_summary(conn)
        total_keys, key_tiers = get_keys_summary(conn)
        conn.close()
    except Exception as e:
        print(f"[✗] Database read error: {e}", file=sys.stderr)
        sys.exit(1)
        
    report_text = generate_report(total_records, metrics, recent_entries, total_keys, key_tiers)
    
    print(report_text)
    print("\n" + "=" * 50)
    
    if args.stdout_only:
        print("[✓] Report printed to stdout. Email dispatch bypassed by --stdout-only flag.")
        return
        
    print(f"→ Dispatching report to {args.to} via Mail.app...")
    try:
        send_via_applescript(args.to, args.subject, report_text)
        print("[✓] Report successfully dispatched via Apple Mail.")
    except Exception as e:
        print(f"[✗] Failed to send email via AppleScript: {e}", file=sys.stderr)
        print("[!] Ensure Mail.app is configured and has automation permissions in System Settings.")

if __name__ == "__main__":
    main()
