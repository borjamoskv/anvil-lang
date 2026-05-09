"""
github_ingestor.py — CORTEX-Persist GitHub Source Ingestor (C5-REAL)

Fetches contract source code from GitHub and writes it to the targets/ dir.
Tries 'main' first, falls back to 'master'.
"""

import os
import urllib.request
import urllib.error
from _cortex_common import BASE, BANNER

TARGETS_DIR = os.path.join(BASE, "anvil-lang", "targets")


def _fetch_raw(owner: str, repo: str, path: str, branch: str) -> str | None:
    """Fetch raw file content from GitHub. Returns text or None on failure."""
    url = (
        f"https://raw.githubusercontent.com/{owner}/{repo}/{branch}/{path}"
    )
    try:
        with urllib.request.urlopen(urllib.request.Request(url)) as resp:
            return resp.read().decode("utf-8")
    except urllib.error.URLError:
        return None


def fetch_github_file(owner: str, repo: str, path: str) -> None:
    """Try 'main' then 'master'. Write result to targets/."""
    print(f"[*] Fetching: {owner}/{repo} → {path}")

    source = _fetch_raw(owner, repo, path, "main")
    if source is None:
        print("[*] 'main' not found — trying 'master'...")
        source = _fetch_raw(owner, repo, path, "master")

    if source is None:
        print(f"[!] Fetch failed for {owner}/{repo}/{path}")
        return

    os.makedirs(TARGETS_DIR, exist_ok=True)
    filename = os.path.basename(path)
    target_file = os.path.join(TARGETS_DIR, filename)

    with open(target_file, "w") as f:
        f.write(source)

    print(f"[+] Saved to {target_file} ({len(source):,} bytes)")


if __name__ == "__main__":
    print(BANNER)
    print("[OUROBOROS] CORTEX-Persist GitHub Ingestor (C5-REAL)")
    print(BANNER)
    fetch_github_file("Uniswap", "v2-core", "contracts/UniswapV2Pair.sol")
