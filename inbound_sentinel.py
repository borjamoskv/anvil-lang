"""Automatic email replies are quarantined pending a reviewed message boundary."""

def poll():
    raise SystemExit("OMEGA-21: automatic replies disabled; review messages manually.")

if __name__ == "__main__":
    poll()
