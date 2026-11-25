"""Legacy synthetic ledger generator is quarantined.

The old implementation replaced registry.json with simulated activity. It
must not run against the authoritative trace registry.
"""

def generate_swarm_ledger():
    raise SystemExit("OMEGA-21: synthetic ledger generation disabled.")

if __name__ == "__main__":
    generate_swarm_ledger()
