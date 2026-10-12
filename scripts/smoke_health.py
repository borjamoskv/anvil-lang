#!/usr/bin/env python3
"""Check SaaS health and always reap the server process."""

import json
import os
import signal
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path


def check_health(binary: str) -> None:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", int(os.environ.get("ANVIL_SMOKE_PORT", "0"))))
        port = sock.getsockname()[1]
    url = f"http://127.0.0.1:{port}/health"
    log_path = Path(os.environ["ANVIL_SAAS_LOG"])
    with log_path.open("w") as log:
        process = subprocess.Popen([binary, "saas", "--port", str(port)], stdout=log, stderr=log)
        try:
            for _ in range(40):
                if process.poll() is not None:
                    raise RuntimeError(f"SaaS exited before becoming healthy: {log_path.read_text()}")
                try:
                    with urllib.request.urlopen(url, timeout=0.25) as response:
                        payload = json.load(response)
                except (urllib.error.URLError, TimeoutError):
                    time.sleep(0.25)
                    continue
                if (payload.get("status") != "online"
                        or payload.get("engine") != "Anvil+Z3 Formal Verification"
                        or not payload.get("version")):
                    raise ValueError(f"Unexpected health response: {payload}")
                print(json.dumps(payload), flush=True)
                return
            raise TimeoutError(f"Timed out waiting for {url}; log: {log_path}")
        finally:
            if process.poll() is None:
                process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()


def interrupted(signum: int, _frame: object) -> None:
    raise SystemExit(128 + signum)


if __name__ == "__main__":
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    check_health(sys.argv[1])
