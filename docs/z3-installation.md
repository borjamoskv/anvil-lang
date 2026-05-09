# Z3 Installation Guide

Anvil uses [Z3](https://github.com/Z3Prover/z3) (Microsoft's SMT solver) for formal verification. You need both the Z3 binary and development headers installed.

## Quick Install

### Ubuntu / Debian

```bash
sudo apt-get update
sudo apt-get install -y z3 libz3-dev
z3 --version
```

### macOS (Homebrew)

```bash
brew install z3
z3 --version
```

### Windows (Chocolatey)

```powershell
choco install z3
z3 --version
```

### Arch Linux

```bash
sudo pacman -S z3
```

### Fedora / RHEL

```bash
sudo dnf install z3 z3-devel
```

## Building from Source

If your distribution doesn't provide Z3 packages, or you need a specific version:

```bash
git clone https://github.com/Z3Prover/z3.git
cd z3
python scripts/mk_make.py
cd build
make -j$(nproc)
sudo make install

# Verify
z3 --version
```

## Verifying the Installation

After installing, verify that both the binary and libraries are available:

```bash
# Check binary
z3 --version
# Expected: Z3 version 4.x.x - 64 bit

# Check library (Linux)
ldconfig -p | grep libz3
# Expected: libz3.so.4 (libc6,x86-64) => /usr/lib/x86_64-linux-gnu/libz3.so.4

# Check library (macOS)
ls /opt/homebrew/lib/libz3* 2>/dev/null || ls /usr/local/lib/libz3*
```

## Environment Variables

If Z3 is installed in a non-standard location, set these before building Anvil:

```bash
# Linux
export Z3_SYS_Z3_HEADER=/path/to/z3/include/z3.h
export LD_LIBRARY_PATH=/path/to/z3/lib:$LD_LIBRARY_PATH

# macOS
export Z3_SYS_Z3_HEADER=/opt/homebrew/include/z3.h
export DYLD_LIBRARY_PATH=/opt/homebrew/lib:$DYLD_LIBRARY_PATH
```

## Troubleshooting

### `error: could not find native static library z3`

**Cause:** The `z3` Rust crate can't find `libz3-dev` headers.

**Fix (Ubuntu):**
```bash
sudo apt-get install -y libz3-dev
```

**Fix (macOS):**
```bash
brew install z3
export Z3_SYS_Z3_HEADER=$(brew --prefix z3)/include/z3.h
```

### `error: linking with cc failed: exit code: 1`

**Cause:** Missing C/C++ toolchain needed for Z3 FFI bindings.

**Fix:**
```bash
# Ubuntu
sudo apt-get install -y build-essential clang

# macOS
xcode-select --install
```

### `Z3 undecidable` on all invariants

**Cause:** Z3 version too old (< 4.8) or timeout too low.

**Fix:** Update Z3 to latest version. Anvil sets a 5000ms timeout by default.

### CI: `libz3.so: cannot open shared object file`

**Cause:** Z3 library not in the linker path at runtime.

**Fix:** Add to your CI workflow:
```yaml
- name: Install Z3 Solver
  run: |
    sudo apt-get update
    sudo apt-get install -y z3 libz3-dev
```

## Minimum Versions

| Component | Minimum | Recommended |
|---|---|---|
| Z3 | 4.8 | Latest stable |
| Rust | 1.80 | Latest stable |
| Clang | 11 | Latest stable |
