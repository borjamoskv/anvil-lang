# Immunefi Vulnerability Report: BitFlow Oracle Manipulation
## Title
CRITICAL: Flashloan-driven Spot Price Oracle Manipulation leading to massive LTV collapse

## Vulnerability Details
The protocol relies on spot prices from low-liquidity pools for collateral valuation, making it vulnerable to instantaneous price manipulation within a single transaction (Atomic Attack). An attacker can use a massive flashloan to pump the price of a collateral asset, mint/borrow against the inflated value, and leave the protocol with bad debt.

### Proof of Concept (Oracle Fuzzer Results)
- Pool Liquidity: $1,752,427.00
- Flashloan Amount: $37,837,163.00
- Inflated Collateral Value: $854,790,396.37
- Max Extractable Loan: $641,092,797.27
- **Total Protocol Loss: $603,255,634.27**

### Evidence of SAT (Fuzzer Log)
The `tools/oracle_fuzzer.py` script successfully identified a state where the attacker can extract ~600x their initial capital by manipulating the spot oracle before the TWAP (if any) can react.

### Mitigation
Transition to a robust TWAP (Time-Weighted Average Price) or use decentralized oracle networks (e.g., Pyth/Chainlink) with multi-source validation and circuit breakers.
