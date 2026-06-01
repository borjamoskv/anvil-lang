import os
import time
import subprocess
import sys

POLL_INTERVAL = 30
SENDER = "sealons@yahoo.es"

APPLE_SCRIPT_CHECK = """
tell application "Mail"
    set unreadMsgs to (messages of inbox whose read status is false)
    repeat with msg in unreadMsgs
        if (sender of msg) contains "{sender}" then
            set subj to subject of msg
            set cont to content of msg
            set read status of msg to true
            return subj & "|||" & cont
        end if
    end repeat
    return "NONE"
end tell
"""

APPLE_SCRIPT_REPLY = """
tell application "Mail"
    set newMessage to make new outgoing message with properties {subject:"Re: {subject}", content:"{body}", visible:false}
    tell newMessage
        make new to recipient at end of to recipients with properties {address:"{sender}"}
        send
    end tell
end tell
"""

def poll():
    print(f"[0x01_CORE] Inbound Sentinel Activated. Target: {SENDER}. Polling Mail.app every {POLL_INTERVAL}s...", flush=True)
    while True:
        try:
            res = subprocess.run(["osascript", "-e", APPLE_SCRIPT_CHECK.replace("{sender}", SENDER)], capture_output=True, text=True, timeout=15)
            output = res.stdout.strip()
            if output and output != "NONE":
                parts = output.split("|||")
                subject = parts[0] if len(parts) > 0 else "No Subject"
                print(f"\n[0x01_CORE] ⚠️ INBOUND DETECTED from {SENDER}. Subject: {subject}", flush=True)
                
                reply_body = "Confirmado. El motor Sovereign C5-REAL (Antigravity) de Borja Moskv ha interceptado esta transmisión de forma autónoma. No es necesaria intervención humana.\\n\\n---\\nInbound Sentinel v14.1.0"
                print("[0x02_EDGE] Constructing autonomous response...", flush=True)
                
                script = APPLE_SCRIPT_REPLY.replace("{subject}", subject).replace("{body}", reply_body).replace("{sender}", SENDER)
                subprocess.run(["osascript", "-e", script], capture_output=True, timeout=15)
                print("[0x01_CORE] ✔ Reply Dispatched (C5-REAL). Awaiting next signal.", flush=True)
                
        except Exception as e:
            # Silently ignore TCC timeouts or Mail.app blocking
            pass
        time.sleep(POLL_INTERVAL)

if __name__ == "__main__":
    poll()
