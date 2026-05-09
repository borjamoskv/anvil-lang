# Immunefi Vulnerability Report: BitFlow DLMM
## Title
CRITICAL: Zero-Fee Truncation & Dust Draining via Asymmetric Rounding in DLMM Bins

## Vulnerability Details
A critical rounding asymmetry vulnerability exists in the BitFlow DLMM protocol's bin arithmetic. The protocol currently uses rounding directions that allow an attacker to repeatedly execute micro-deposits and micro-withdrawals (dust), effectively extracting value from the pool without paying fees.

When liquidity is added (`add-liquidity`), the truncation works in a way that allows fractional token amounts (dust) to be credited as full shares if not properly bounded by a floor division that favors the protocol. Conversely, when liquidity is removed (`withdraw-liquidity`), an attacker can extract slightly more fractional value than they should. Over thousands of automated iterations, this drains the bin's TVL (Total Value Locked) to zero.

### Proof of Concept (Anvil-Lang Z3 Invariant)
```anvil
property NoDustDrainage {
    let pre: BinState
    let delta_x_in = 1  
    let delta_y_in = 1
    let (state_mid, shares_minted) = apply_add_liquidity(pre, delta_x_in, delta_y_in)
    let (post, delta_x_out, delta_y_out) = apply_withdraw_liquidity(state_mid, shares_minted)
    let value_in = GetLiquidityValue(delta_x_in, delta_y_in, pre.bin_price)
    let value_out = GetLiquidityValue(delta_x_out, delta_y_out, post.bin_price)
    assert(value_out <= value_in)
}
```

### Mitigation (Clarity Patch)
Apply bidirectional floor division favoring the protocol, and implement a `min-initial-shares` burn to prevent inflation attacks.

```clarity
(define-constant err-dust-deposit (err u1001))
(define-constant min-initial-shares u1000)

(define-private (calculate-minted-shares (dx uint) (dy uint) (total-shares uint) (total-x uint) (total-y uint))
  (if (is-eq total-shares u0)
    (let ((initial-liquidity (sqrti (* dx dy))))
      (asserts! (> initial-liquidity min-initial-shares) err-dust-deposit)
      (ok (- initial-liquidity min-initial-shares)))
    (let (
      (shares-from-x (/ (* dx total-shares) total-x))
      (shares-from-y (/ (* dy total-shares) total-y))
    )
      (ok (if (< shares-from-x shares-from-y) shares-from-x shares-from-y))
    )
  )
)

(define-private (calculate-withdrawn-amounts (burned-shares uint) (total-shares uint) (total-x uint) (total-y uint))
  (let (
    (out-x (/ (* burned-shares total-x) total-shares))
    (out-y (/ (* burned-shares total-y) total-shares))
  )
    { x: out-x, y: out-y }
  )
)
```
