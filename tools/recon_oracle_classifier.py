#!/usr/bin/env python3
"""
═══════════════════════════════════════════════════════════════════
[C5-REAL] CORTEX-Persist: Oracle Recon & Target Classifier
═══════════════════════════════════════════════════════════════════
Phase 0 of the Anvil Kill Chain. Classifies protocol oracle
architecture BEFORE investing Z3 compute on exploit synthesis.

Pipeline: Recon → Classify → Filter → Z3 Strike

Usage:
    python3 tools/recon_oracle_classifier.py --address 0x... --chain optimism
    python3 tools/recon_oracle_classifier.py --scan-immunefi
═══════════════════════════════════════════════════════════════════
"""

import json
import re
import sys
import os
from dataclasses import dataclass, field, asdict
from enum import Enum
from typing import Optional
from datetime import datetime, timezone

# ── Oracle Classification Taxonomy ──────────────────────────────

class OracleType(Enum):
    CHAINLINK = "chainlink"          # AggregatorV3Interface — NOT exploitable via flashloan
    TWAP = "twap"                    # Time-weighted — resistant to single-block manipulation
    SPOT_AMM = "spot_amm"            # reserve_x/reserve_y — EXPLOITABLE via flashloan
    BAND_PROTOCOL = "band"           # Band Protocol — NOT exploitable via flashloan
    PYTH = "pyth"                    # Pyth Network — NOT exploitable via flashloan
    REDSTONE = "redstone"            # RedStone — NOT exploitable via flashloan
    CUSTOM = "custom"                # Unknown/custom oracle — needs manual review
    NONE_DETECTED = "none_detected"  # No oracle pattern found

class ExploitViability(Enum):
    HIGH = "HIGH"         # Spot AMM oracle, no TWAP, flashloanable
    MEDIUM = "MEDIUM"     # Custom oracle, needs manual analysis
    LOW = "LOW"           # TWAP with short window, theoretically attackable
    NONE = "NONE"         # Chainlink/Band/Pyth — not flashloan-exploitable

@dataclass
class OracleFingerprint:
    """Fingerprint of a protocol's oracle architecture."""
    address: str
    chain: str
    oracle_type: OracleType
    viability: ExploitViability
    confidence: float  # 0.0 - 1.0
    evidence: list = field(default_factory=list)
    chainlink_feeds: list = field(default_factory=list)
    amm_dependencies: list = field(default_factory=list)
    timestamp: str = ""
    
    def __post_init__(self):
        if not self.timestamp:
            self.timestamp = datetime.now(timezone.utc).isoformat()


# ── Signature Database ──────────────────────────────────────────

# Function selectors and patterns that identify oracle types
ORACLE_SIGNATURES = {
    OracleType.CHAINLINK: [
        "latestRoundData()",          # 0xfeaf968c
        "latestAnswer()",             # 0x50d25bcd
        "AggregatorV3Interface",
        "getRoundData(",
        "priceFeed",
        "chainlinkPrice",
        "getChainlinkPrice",
        "oracle.latestRoundData",
    ],
    OracleType.TWAP: [
        "consult(",                   # TWAP oracle consult
        "observe(",                   # Uniswap V3 TWAP
        "getTimeWeightedAverage",
        "cumulativePrice",
        "price0CumulativeLast",
        "price1CumulativeLast",
        "TWAP",
        "twap",
        "timeWeightedAverage",
        "OracleLibrary.consult",
    ],
    OracleType.SPOT_AMM: [
        "getReserves()",              # 0x0902f1ac — Uniswap V2 spot
        "reserve0",
        "reserve1",
        "getAmountOut(",
        "getAmountsOut(",
        "quote(",                     # Uniswap V2 Router
        "spot_price",
        "spotPrice",
        "getSpotPrice",
        # Patterns for inline price calculation
        "reserveA * reserveB",
        "reserve_quote / reserve_base",
        "tokenBalanceOf",
    ],
    OracleType.BAND_PROTOCOL: [
        "IStdReference",
        "getReferenceData(",
        "bandOracle",
    ],
    OracleType.PYTH: [
        "IPyth",
        "getPriceUnsafe(",
        "getPrice(",
        "pythOracle",
        "PythStructs",
    ],
    OracleType.REDSTONE: [
        "RedstoneConsumerBase",
        "getOracleNumericValueFromTxMsg",
        "redstone",
    ],
}

# Selectors as 4-byte hex for bytecode analysis
BYTECODE_SELECTORS = {
    "feaf968c": OracleType.CHAINLINK,    # latestRoundData()
    "50d25bcd": OracleType.CHAINLINK,    # latestAnswer()
    "9a6fc8f5": OracleType.CHAINLINK,    # getRoundData(uint80)
    "0902f1ac": OracleType.SPOT_AMM,     # getReserves()
    "d06ca61f": OracleType.SPOT_AMM,     # getAmountsOut(uint256,address[])
    "ad615dec": OracleType.SPOT_AMM,     # quote(uint256,uint256,uint256)
}


# ── Source Code Classifier ──────────────────────────────────────

def classify_source(source_code: str) -> OracleFingerprint:
    """
    Classify oracle type from Solidity/Vyper source code.
    Returns fingerprint with evidence trail.
    """
    evidence = []
    scores = {ot: 0 for ot in OracleType}
    
    source_lower = source_code.lower()
    source_lines = source_code.split("\n")
    
    for oracle_type, patterns in ORACLE_SIGNATURES.items():
        for pattern in patterns:
            pattern_lower = pattern.lower()
            # Count occurrences
            count = source_lower.count(pattern_lower)
            if count > 0:
                scores[oracle_type] += count
                # Find the line for evidence
                for i, line in enumerate(source_lines):
                    if pattern_lower in line.lower():
                        evidence.append({
                            "type": oracle_type.value,
                            "pattern": pattern,
                            "line": i + 1,
                            "context": line.strip()[:120],
                        })
                        break  # One evidence per pattern is enough
    
    # Determine primary oracle type
    max_score = max(scores.values())
    if max_score == 0:
        primary = OracleType.NONE_DETECTED
        confidence = 0.0
    else:
        primary = max(scores, key=scores.get)
        total = sum(scores.values())
        confidence = min(scores[primary] / total if total > 0 else 0, 1.0)
    
    # Determine exploit viability
    viability = _assess_viability(primary, scores, source_code)
    
    # Extract Chainlink feed addresses if present
    chainlink_feeds = _extract_chainlink_feeds(source_code)
    
    # Extract AMM dependencies
    amm_deps = _extract_amm_dependencies(source_code)
    
    return OracleFingerprint(
        address="source_analysis",
        chain="unknown",
        oracle_type=primary,
        viability=viability,
        confidence=round(confidence, 3),
        evidence=evidence[:10],  # Cap at 10 evidence items
        chainlink_feeds=chainlink_feeds,
        amm_dependencies=amm_deps,
    )


def classify_bytecode(bytecode: str) -> OracleFingerprint:
    """
    Classify oracle type from contract bytecode (hex).
    Less accurate than source but works for unverified contracts.
    """
    bytecode_clean = bytecode.lower().replace("0x", "")
    evidence = []
    scores = {ot: 0 for ot in OracleType}
    
    for selector, oracle_type in BYTECODE_SELECTORS.items():
        if selector in bytecode_clean:
            scores[oracle_type] += 1
            evidence.append({
                "type": oracle_type.value,
                "selector": f"0x{selector}",
                "method": "bytecode_scan",
            })
    
    max_score = max(scores.values())
    if max_score == 0:
        primary = OracleType.NONE_DETECTED
        confidence = 0.0
    else:
        primary = max(scores, key=scores.get)
        confidence = 0.6  # Bytecode analysis is inherently less confident
    
    viability = _assess_viability(primary, scores, "")
    
    return OracleFingerprint(
        address="bytecode_analysis",
        chain="unknown",
        oracle_type=primary,
        viability=viability,
        confidence=round(confidence, 3),
        evidence=evidence,
    )


# ── Viability Assessment ────────────────────────────────────────

def _assess_viability(
    primary: OracleType, 
    scores: dict, 
    source: str
) -> ExploitViability:
    """
    Determine if the oracle architecture is exploitable via flashloan.
    """
    # Chainlink/Band/Pyth/RedStone → External oracle, not flashloanable
    if primary in (
        OracleType.CHAINLINK, 
        OracleType.BAND_PROTOCOL, 
        OracleType.PYTH,
        OracleType.REDSTONE,
    ):
        return ExploitViability.NONE
    
    # TWAP → Resistant but theoretically attackable with multi-block
    if primary == OracleType.TWAP:
        # Check TWAP window length if available
        if source:
            # Short TWAP windows (<10 min) are more vulnerable
            twap_window = _extract_twap_window(source)
            if twap_window and twap_window < 600:  # < 10 minutes
                return ExploitViability.LOW
        return ExploitViability.NONE
    
    # Spot AMM → EXPLOITABLE
    if primary == OracleType.SPOT_AMM:
        # Check if there's also a Chainlink fallback
        if scores.get(OracleType.CHAINLINK, 0) > 0:
            return ExploitViability.MEDIUM  # Has fallback, needs deeper analysis
        return ExploitViability.HIGH
    
    # Custom/None → Unknown, needs manual review
    return ExploitViability.MEDIUM


def _extract_twap_window(source: str) -> Optional[int]:
    """Extract TWAP observation window in seconds from source."""
    patterns = [
        r'secondsAgo\s*=\s*(\d+)',
        r'TWAP_PERIOD\s*=\s*(\d+)',
        r'twapPeriod\s*=\s*(\d+)',
        r'PERIOD\s*=\s*(\d+)',
        r'observe\(\s*\[(\d+)',
    ]
    for pattern in patterns:
        match = re.search(pattern, source)
        if match:
            return int(match.group(1))
    return None


def _extract_chainlink_feeds(source: str) -> list:
    """Extract Chainlink price feed addresses from source."""
    feeds = []
    # Match Ethereum addresses near chainlink/oracle/feed keywords
    oracle_region = re.findall(
        r'(?:feed|oracle|price|aggregator)\s*[=:]\s*(0x[a-fA-F0-9]{40})',
        source, re.IGNORECASE
    )
    feeds.extend(oracle_region)
    return list(set(feeds))


def _extract_amm_dependencies(source: str) -> list:
    """Extract AMM pool addresses or references from source."""
    deps = []
    pool_refs = re.findall(
        r'(?:pool|pair|amm|router)\s*[=:]\s*(0x[a-fA-F0-9]{40})',
        source, re.IGNORECASE
    )
    deps.extend(pool_refs)
    return list(set(deps))


# ── Report Generator ────────────────────────────────────────────

def generate_report(fingerprint: OracleFingerprint) -> str:
    """Generate human-readable recon report."""
    viability_emoji = {
        ExploitViability.HIGH: "🔴 HIGH — EXPLOITABLE VIA FLASHLOAN",
        ExploitViability.MEDIUM: "🟡 MEDIUM — NEEDS MANUAL ANALYSIS",
        ExploitViability.LOW: "🟠 LOW — THEORETICALLY ATTACKABLE",
        ExploitViability.NONE: "🟢 NONE — NOT FLASHLOAN EXPLOITABLE",
    }
    
    lines = [
        "=" * 60,
        "  CORTEX RECON: Oracle Architecture Classification",
        "=" * 60,
        f"  Target:      {fingerprint.address}",
        f"  Chain:       {fingerprint.chain}",
        f"  Oracle Type: {fingerprint.oracle_type.value.upper()}",
        f"  Viability:   {viability_emoji[fingerprint.viability]}",
        f"  Confidence:  {fingerprint.confidence:.1%}",
        f"  Timestamp:   {fingerprint.timestamp}",
        "=" * 60,
    ]
    
    if fingerprint.viability == ExploitViability.HIGH:
        lines.extend([
            "",
            "  ⚡ TARGET VIABLE FOR ANVIL Z3 STRIKE",
            "  → Proceed to Phase 1: Z3 Proof Generation",
            "  → Proceed to Phase 2: Mainnet Fork PoC",
        ])
    elif fingerprint.viability == ExploitViability.NONE:
        lines.extend([
            "",
            "  ✗ TARGET NOT VIABLE — DO NOT SUBMIT",
            f"  → Protocol uses {fingerprint.oracle_type.value}",
            "  → Flashloan oracle manipulation does not apply",
            "  → Move to next target",
        ])
    
    if fingerprint.evidence:
        lines.extend(["", "  Evidence Trail:"])
        for ev in fingerprint.evidence[:5]:
            lines.append(f"    [{ev['type']}] L{ev.get('line', '?')}: {ev.get('context', ev.get('selector', ''))}")
    
    if fingerprint.chainlink_feeds:
        lines.extend(["", "  Chainlink Feeds Detected:"])
        for feed in fingerprint.chainlink_feeds:
            lines.append(f"    → {feed}")
    
    lines.append("")
    return "\n".join(lines)


# ── File Scanner ────────────────────────────────────────────────

def scan_directory(path: str) -> list:
    """Scan a directory of Solidity files and classify each."""
    results = []
    sol_files = []
    
    for root, dirs, files in os.walk(path):
        for f in files:
            if f.endswith(('.sol', '.vy')):
                sol_files.append(os.path.join(root, f))
    
    for sol_file in sol_files:
        try:
            with open(sol_file, 'r') as fh:
                source = fh.read()
            fp = classify_source(source)
            fp.address = sol_file
            results.append(fp)
        except Exception as e:
            print(f"  [!] Error scanning {sol_file}: {e}")
    
    return results


# ── CLI Entry Point ─────────────────────────────────────────────

def main():
    import argparse
    
    parser = argparse.ArgumentParser(
        description="CORTEX Recon: Oracle Architecture Classifier"
    )
    parser.add_argument(
        "--source", "-s",
        help="Path to Solidity source file or directory"
    )
    parser.add_argument(
        "--bytecode", "-b",
        help="Contract bytecode (hex string)"
    )
    parser.add_argument(
        "--json", "-j",
        action="store_true",
        help="Output as JSON"
    )
    
    args = parser.parse_args()
    
    if args.source:
        path = os.path.expanduser(args.source)
        if os.path.isdir(path):
            results = scan_directory(path)
            for fp in results:
                if args.json:
                    print(json.dumps(asdict(fp), default=str))
                else:
                    print(generate_report(fp))
        elif os.path.isfile(path):
            with open(path, 'r') as f:
                source = f.read()
            fp = classify_source(source)
            fp.address = path
            if args.json:
                print(json.dumps(asdict(fp), default=str))
            else:
                print(generate_report(fp))
        else:
            print(f"[!] Path not found: {path}")
            sys.exit(1)
    
    elif args.bytecode:
        fp = classify_bytecode(args.bytecode)
        if args.json:
            print(json.dumps(asdict(fp), default=str))
        else:
            print(generate_report(fp))
    
    else:
        # Demo: scan our own test files
        test_dir = os.path.join(os.path.dirname(__file__), '..', 'tests')
        if os.path.isdir(test_dir):
            print("[*] Demo: Scanning tests/ directory...")
            results = scan_directory(test_dir)
            viable = [r for r in results if r.viability in (ExploitViability.HIGH, ExploitViability.MEDIUM)]
            safe = [r for r in results if r.viability == ExploitViability.NONE]
            
            print(f"\n  Scanned: {len(results)} contracts")
            print(f"  Viable targets: {len(viable)}")
            print(f"  Safe (skip): {len(safe)}")
            
            for fp in results:
                print(generate_report(fp))
        else:
            parser.print_help()


if __name__ == "__main__":
    main()
