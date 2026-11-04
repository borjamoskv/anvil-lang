/-
  C5-REAL Sovereign Formal Kernel — Moloch Landauer Gate & Energetic Preflight Policy
  Author: Borja Moskv (borjamoskv)
  Formal verification of thermodynamic externality internalization (`moloch_landauer_gate`)
  and energetic preflight governance (`preflight_energy_gate` + `DeltaPhi` activation).
-/

import Anvil.Core

namespace Anvil.MolochLandauer

open Anvil.Core

/-!
  ## 1. Moloch Neutralization via Landauer Thermodynamic Balance (`moloch_landauer_gate`)
-/

/--
  Agent action profile in a decentralized coordination / proof-market game.
  - `uClaim`: gross utility claimed by the agent.
  - `sInt`: entropy / thermodynamic cost absorbed internally by the agent.
  - `sExt`: entropy / negative externality offloaded onto the substrate or peer network.
-/
structure AgentMove where
  uClaim : Nat
  sInt   : Nat
  sExt   : Nat
  deriving DecidableEq, Repr

/--
  Extractive coordination condition (Moloch):
  An action is extractive iff externalized entropy strictly exceeds internalized entropy
  (`sExt > sInt`) while claiming positive utility (`uClaim > 0`).
-/
def isExtractive (m : AgentMove) : Bool :=
  decide (m.sExt > m.sInt ∧ m.uClaim > 0)

/--
  Landauer Balance Surcharge (`landauerSurcharge`):
  When an action is extractive (`sExt > sInt` and `uClaim > 0`), the gate internalizes
  the thermodynamic externality plus the claimed extractive rent (`(sExt - sInt) + uClaim`),
  collapsing the net extractive payoff to zero.
-/
def landauerSurcharge (m : AgentMove) : Nat :=
  if m.sExt > m.sInt ∧ m.uClaim > 0 then
    (m.sExt - m.sInt) + m.uClaim
  else
    0

/--
  Net effective payoff after passing through `moloch_landauer_gate` (truncated at 0 in `Nat`).
-/
def moloch_landauer_gate (m : AgentMove) : Nat :=
  m.uClaim - landauerSurcharge m

/--
  Theorem ML.1 (Collapse of Extractive Rent):
  For any extractive move (`m.sExt > m.sInt` and `m.uClaim > 0`), `moloch_landauer_gate`
  collapses the net payoff identically to `0`.
-/
theorem extractive_payoff_collapses_to_zero (m : AgentMove)
    (h_ext : m.sExt > m.sInt) (h_claim : m.uClaim > 0) :
    moloch_landauer_gate m = 0 := by
  unfold moloch_landauer_gate landauerSurcharge
  split
  · omega
  · next h_not =>
      exfalso
      exact h_not ⟨h_ext, h_claim⟩

/--
  Theorem ML.2 (Preservation of Sovereign Cooperative Exergy):
  For any cooperative move that internalizes its entropy (`m.sExt ≤ m.sInt`),
  the Landauer surcharge is `0` and the agent retains its full utility `m.uClaim`.
-/
theorem cooperative_payoff_preserved (m : AgentMove)
    (h_coop : m.sExt ≤ m.sInt) :
    moloch_landauer_gate m = m.uClaim := by
  unfold moloch_landauer_gate landauerSurcharge
  split
  · next h_cond =>
      have h_gt : m.sExt > m.sInt := h_cond.1
      omega
  · omega

/--
  Theorem ML.3 (Strict Pareto Dominance of Cooperation without Central Agency):
  Given any cooperative move `c` (`c.sExt ≤ c.sInt`, `c.uClaim > 0`) and any extractive
  move `e` (`e.sExt > e.sInt`, `e.uClaim > 0`), the post-gate payoff of cooperation
  strictly dominates extraction (`moloch_landauer_gate c > moloch_landauer_gate e`),
  while externalized substrate entropy is strictly lower (`c.sExt - c.sInt = 0 < e.sExt - e.sInt`).
-/
theorem cooperation_strictly_dominates_extraction (c e : AgentMove)
    (h_coop : c.sExt ≤ c.sInt) (h_c_pos : c.uClaim > 0)
    (h_ext : e.sExt > e.sInt) (h_e_pos : e.uClaim > 0) :
    moloch_landauer_gate c > moloch_landauer_gate e ∧
    (c.sExt - c.sInt) < (e.sExt - e.sInt) := by
  have hc := cooperative_payoff_preserved c h_coop
  have he := extractive_payoff_collapses_to_zero e h_ext h_e_pos
  rw [hc, he]
  omega

/-- Theorem ML.4: AP-10 VerifiedInvariant for `moloch_landauer_gate`. -/
theorem moloch_landauer_gate_verified :
    VerifiedInvariant (AgentMove × AgentMove)
      (fun p => p.1.sExt ≤ p.1.sInt ∧ p.1.uClaim > 0)
      (fun p => p.2.sExt > p.2.sInt ∧ p.2.uClaim > 0)
      (fun p => moloch_landauer_gate p.1 > moloch_landauer_gate p.2 ∧ moloch_landauer_gate p.2 = 0) := by
  refine ⟨?_, ?_⟩
  · intro p h_reach h_trig
    have h_dom := cooperation_strictly_dominates_extraction p.1 p.2 h_reach.1 h_reach.2 h_trig.1 h_trig.2
    have h_zero := extractive_payoff_collapses_to_zero p.2 h_trig.1 h_trig.2
    exact ⟨h_dom.1, h_zero⟩
  · refine ⟨({ uClaim := 100, sInt := 20, sExt := 5 }, { uClaim := 500, sInt := 10, sExt := 250 }), ?_, ?_⟩
    · dsimp
      omega
    · dsimp
      omega

/-!
  ## 2. Energetic Preflight Policy (`preflight_energy_gate` & `DeltaPhi` Activation)
-/

/--
  Thermodynamic preflight telemetry and resource envelope for `anvil-lang` CI & Z3/Lean execution.
-/
structure PreflightEnvelope where
  rssMib            : Nat
  maxRssBudgetMib   : Nat
  z3TimeoutMs       : Nat
  buildJobs         : Nat
  expectedFailCost  : Nat
  formalizationCost : Nat
  deriving DecidableEq, Repr

inductive PreflightVerdict : Type where
  | approved               : Nat → PreflightVerdict
  | rejectedRssBudget      : PreflightVerdict
  | rejectedTimeout        : PreflightVerdict
  | rejectedConcurrency    : PreflightVerdict
  | rejectedNegativeExergy : PreflightVerdict
  deriving DecidableEq, Repr

/--
  Deterministic energetic preflight gate (`preflight_energy_gate`):
  Enforces:
  1. Memory confinement: `rssMib ≤ maxRssBudgetMib`
  2. Solver horizon confinement: `100 ≤ z3TimeoutMs ∧ z3TimeoutMs ≤ 10000`
  3. Single-job serialization in lowmem profile: `buildJobs = 1`
  4. Positive thermodynamic activation (`ΔΦ > 0`): `expectedFailCost > formalizationCost`
-/
def preflight_energy_gate (env : PreflightEnvelope) : PreflightVerdict :=
  if env.rssMib > env.maxRssBudgetMib then
    .rejectedRssBudget
  else if env.z3TimeoutMs < 100 ∨ env.z3TimeoutMs > 10000 then
    .rejectedTimeout
  else if env.buildJobs ≠ 1 then
    .rejectedConcurrency
  else if env.expectedFailCost ≤ env.formalizationCost then
    .rejectedNegativeExergy
  else
    .approved (env.expectedFailCost - env.formalizationCost)

/--
  Theorem EP.1: Whenever `preflight_energy_gate` returns `.approved deltaPhi`,
  all resource bounds hold and the net exergy surplus `deltaPhi` is strictly positive (`deltaPhi > 0`).
-/
theorem preflight_approved_sound (env : PreflightEnvelope) (deltaPhi : Nat)
    (h : preflight_energy_gate env = .approved deltaPhi) :
    env.rssMib ≤ env.maxRssBudgetMib ∧
    100 ≤ env.z3TimeoutMs ∧ env.z3TimeoutMs ≤ 10000 ∧
    env.buildJobs = 1 ∧
    env.expectedFailCost > env.formalizationCost ∧
    deltaPhi = env.expectedFailCost - env.formalizationCost ∧
    deltaPhi > 0 := by
  unfold preflight_energy_gate at h
  split at h
  · contradiction
  · split at h
    · contradiction
    · split at h
      · contradiction
      · split at h
        · contradiction
        · injection h with h_eq
          omega

/-- Canonical `anvil-lang` preflight configuration (`lowmem-env.sh` + `ci_runner.py` 5000ms Z3 gate). -/
def canonicalAnvilPreflight : PreflightEnvelope :=
  { rssMib            := 88
  , maxRssBudgetMib   := 2048
  , z3TimeoutMs       := 5000
  , buildJobs         := 1
  , expectedFailCost  := 10000
  , formalizationCost := 250
  }

/-- Theorem EP.2: AP-10 VerifiedInvariant for `preflight_energy_gate`. -/
theorem preflight_energy_gate_verified :
    VerifiedInvariant PreflightEnvelope
      (fun env => env.maxRssBudgetMib = 2048 ∧ env.buildJobs = 1)
      (fun env => env = canonicalAnvilPreflight)
      (fun env => preflight_energy_gate env = .approved 9750) := by
  refine ⟨?_, ?_⟩
  · intro env _ h_trig
    rw [h_trig]
    rfl
  · refine ⟨canonicalAnvilPreflight, ?_, rfl⟩
    dsimp [canonicalAnvilPreflight]
    exact ⟨rfl, rfl⟩

end Anvil.MolochLandauer
