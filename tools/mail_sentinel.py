#!/usr/bin/env python3
"""
CORTEX Mail Sentinel — Monitors Gmail for replies from a specific sender.
Sends macOS native notification when a reply is detected.
"""

import imaplib
import email
import time
import subprocess
import os
import sys
from datetime import datetime
from _cortex_common import BANNER

# --- Config ---
GMAIL_USER = os.environ.get("GMAIL_USER", "moskvtoken@gmail.com")
GMAIL_APP_PASSWORD = os.environ.get("GMAIL_APP_PASSWORD", "")
WATCH_SENDER = "sealons@yahoo.es"
WATCH_NAME = "Sergio Alonso"
CHECK_INTERVAL_SECONDS = 60  # Check every 60 seconds
SEEN_IDS_FILE = os.path.expanduser("~/.cortex_mail_sentinel_seen.txt")

def notify_macos(title: str, message: str):
    """Send a native macOS notification."""
    subprocess.run([
        "osascript", "-e",
        f'display notification "{message}" with title "{title}" sound name "Glass"'
    ])

def say_macos(text: str):
    """Speak notification aloud."""
    subprocess.run(["say", "-v", "Mónica", text])

def load_seen_ids() -> set:
    if os.path.exists(SEEN_IDS_FILE):
        with open(SEEN_IDS_FILE) as f:
            return set(f.read().strip().split("\n"))
    return set()

def save_seen_ids(ids: set):
    with open(SEEN_IDS_FILE, "w") as f:
        f.write("\n".join(ids))

def check_for_reply():
    """Connect to Gmail IMAP and check for emails from WATCH_SENDER."""
    try:
        mail = imaplib.IMAP4_SSL("imap.gmail.com")
        mail.login(GMAIL_USER, GMAIL_APP_PASSWORD)
        mail.select("INBOX")
        
        # Search for emails from the watched sender
        _, data = mail.search(None, f'(FROM "{WATCH_SENDER}")')
        
        if not data[0]:
            mail.logout()
            return []
        
        seen = load_seen_ids()
        new_emails = []
        
        for num in data[0].split():
            msg_id = num.decode()
            if msg_id in seen:
                continue
                
            _, msg_data = mail.fetch(num, "(RFC822)")
            raw = msg_data[0][1]
            msg = email.message_from_bytes(raw)
            
            subject = str(email.header.decode_header(msg["Subject"])[0][0])
            if isinstance(subject, bytes):
                subject = subject.decode("utf-8", errors="replace")
            
            new_emails.append({
                "id": msg_id,
                "subject": subject,
                "from": msg["From"],
                "date": msg["Date"],
            })
            seen.add(msg_id)
        
        save_seen_ids(seen)
        mail.logout()
        return new_emails
        
    except Exception as e:
        print(f"[{datetime.now().strftime('%H:%M:%S')}] Error: {e}")
        return []

def main():
    if not GMAIL_APP_PASSWORD:
        print(BANNER)
        print("CORTEX MAIL SENTINEL — Setup Required")
        print(BANNER)
        print()
        print("Need a Gmail App Password to monitor inbox.")
        print("1. Go to: https://myaccount.google.com/apppasswords")
        print("2. Generate an app password for 'Mail'")
        print("3. Run with:")
        print()
        print(f"  GMAIL_APP_PASSWORD='xxxx xxxx xxxx xxxx' python3 {sys.argv[0]}")
        print()
        sys.exit(1)
    
    print(BANNER)
    print(f"  CORTEX MAIL SENTINEL v1.0")
    print(f"  Watching: {WATCH_SENDER} ({WATCH_NAME})")
    print(f"  Interval: {CHECK_INTERVAL_SECONDS}s")
    print(f"  Account: {GMAIL_USER}")
    print(BANNER)
    
    # Initial notification
    notify_macos(
        "Mail Sentinel Active",
        f"Monitoring inbox for replies from {WATCH_NAME}"
    )
    
    while True:
        now = datetime.now().strftime("%H:%M:%S")
        new = check_for_reply()
        
        if new:
            for e in new:
                print(f"\n[{now}] 🔔 NEW EMAIL from {WATCH_NAME}!")
                print(f"  Subject: {e['subject']}")
                print(f"  Date: {e['date']}")
                
                # macOS notification
                notify_macos(
                    f"📬 {WATCH_NAME} ha respondido",
                    e['subject']
                )
                say_macos(f"Borja, {WATCH_NAME} ha respondido al email sobre Anvil")
        else:
            print(f"[{now}] No new emails from {WATCH_NAME}", end="\r")
        
        time.sleep(CHECK_INTERVAL_SECONDS)

if __name__ == "__main__":
    main()
