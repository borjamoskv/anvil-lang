#!/usr/bin/env python3
"""
Test Suite: Moloch-Landauer Thermodynamic Gate & Preflight Policy.
Author: Borja Moskv (borjamoskv)
"""

import unittest
from dataclasses import dataclass
from typing import Optional


@dataclass(frozen=True)
class AgentMove:
    u_claim: int
    s_int: int
    s_ext: int


def is_extractive(m: AgentMove) -> bool:
    return m.s_ext > m.s_int and m.u_claim > 0


def landauer_surcharge(m: AgentMove) -> int:
    if is_extractive(m):
        return (m.s_ext - m.s_int) + m.u_claim
    return 0


def moloch_landauer_gate(m: AgentMove) -> int:
    surcharge = landauer_surcharge(m)
    return max(0, m.u_claim - surcharge)


@dataclass(frozen=True)
class PreflightEnvelope:
    rss_mib: int
    max_rss_budget_mib: int
    z3_timeout_ms: int
    build_jobs: int
    expected_fail_cost: int
    formalization_cost: int


@dataclass(frozen=True)
class PreflightVerdict:
    approved: bool
    delta_phi: int
    reason: str


def preflight_energy_gate(env: PreflightEnvelope) -> PreflightVerdict:
    if env.rss_mib > env.max_rss_budget_mib:
        return PreflightVerdict(False, 0, "rejectedRssBudget")
    if env.z3_timeout_ms < 100 or env.z3_timeout_ms > 10000:
        return PreflightVerdict(False, 0, "rejectedTimeout")
    if env.build_jobs != 1:
        return PreflightVerdict(False, 0, "rejectedConcurrency")
    if env.expected_fail_cost <= env.formalization_cost:
        return PreflightVerdict(False, 0, "rejectedNegativeExergy")
    return PreflightVerdict(True, env.expected_fail_cost - env.formalization_cost, "approved")


class MolochLandauerGateTests(unittest.TestCase):

    def test_01_extractive_surcharge_calculation(self):
        """Verifies that extractive moves incur surcharge = (sExt - sInt) + uClaim."""
        move = AgentMove(u_claim=500, s_int=10, s_ext=250)
        expected = (250 - 10) + 500
        self.assertEqual(landauer_surcharge(move), expected)

    def test_02_extractive_payoff_collapses_to_zero(self):
        """Theorem ML.1: Post-gate payoff of extractive actions collapses identically to 0."""
        move = AgentMove(u_claim=500, s_int=10, s_ext=250)
        self.assertEqual(moloch_landauer_gate(move), 0)

    def test_03_cooperative_payoff_preserved(self):
        """Theorem ML.2: Cooperative moves retain 100% of claimed utility without surcharge."""
        move = AgentMove(u_claim=100, s_int=20, s_ext=5)
        self.assertEqual(landauer_surcharge(move), 0)
        self.assertEqual(moloch_landauer_gate(move), 100)

    def test_04_cooperation_strictly_dominates_extraction(self):
        """Theorem ML.3: Cooperation strictly Pareto-dominates extraction post-gate."""
        coop = AgentMove(u_claim=100, s_int=20, s_ext=5)
        extr = AgentMove(u_claim=500, s_int=10, s_ext=250)
        self.assertGreater(moloch_landauer_gate(coop), moloch_landauer_gate(extr))

    def test_05_externalized_entropy_strictly_lower_in_cooperation(self):
        """Verifies substrate entropy delta is strictly lower for cooperative moves."""
        coop = AgentMove(u_claim=100, s_int=20, s_ext=5)
        extr = AgentMove(u_claim=500, s_int=10, s_ext=250)
        delta_coop = max(0, coop.s_ext - coop.s_int)
        delta_extr = max(0, extr.s_ext - extr.s_int)
        self.assertEqual(delta_coop, 0)
        self.assertEqual(delta_extr, 240)
        self.assertLess(delta_coop, delta_extr)

    def test_06_preflight_approves_canonical_anvil_envelope(self):
        """Theorem EP.2: Preflight gate approves canonical Anvil lowmem envelope."""
        canonical = PreflightEnvelope(
            rss_mib=88,
            max_rss_budget_mib=2048,
            z3_timeout_ms=5000,
            build_jobs=1,
            expected_fail_cost=10000,
            formalization_cost=250
        )
        verdict = preflight_energy_gate(canonical)
        self.assertTrue(verdict.approved)
        self.assertEqual(verdict.delta_phi, 9750)
        self.assertEqual(verdict.reason, "approved")

    def test_07_preflight_rejects_rss_overflow(self):
        """Verifies preflight rejects envelope exceeding RSS budget."""
        overflow = PreflightEnvelope(
            rss_mib=2500,
            max_rss_budget_mib=2048,
            z3_timeout_ms=5000,
            build_jobs=1,
            expected_fail_cost=10000,
            formalization_cost=250
        )
        verdict = preflight_energy_gate(overflow)
        self.assertFalse(verdict.approved)
        self.assertEqual(verdict.reason, "rejectedRssBudget")

    def test_08_preflight_rejects_timeout_out_of_bounds(self):
        """Verifies preflight rejects Z3 solver timeout outside [100, 10000] ms."""
        too_short = PreflightEnvelope(
            rss_mib=88, max_rss_budget_mib=2048, z3_timeout_ms=50,
            build_jobs=1, expected_fail_cost=10000, formalization_cost=250
        )
        too_long = PreflightEnvelope(
            rss_mib=88, max_rss_budget_mib=2048, z3_timeout_ms=15000,
            build_jobs=1, expected_fail_cost=10000, formalization_cost=250
        )
        self.assertEqual(preflight_energy_gate(too_short).reason, "rejectedTimeout")
        self.assertEqual(preflight_energy_gate(too_long).reason, "rejectedTimeout")

    def test_09_preflight_rejects_negative_delta_phi(self):
        """Verifies preflight enforces DeltaPhi > 0 (activation threshold)."""
        negative_exergy = PreflightEnvelope(
            rss_mib=88, max_rss_budget_mib=2048, z3_timeout_ms=5000,
            build_jobs=1, expected_fail_cost=100, formalization_cost=250
        )
        verdict = preflight_energy_gate(negative_exergy)
        self.assertFalse(verdict.approved)
        self.assertEqual(verdict.reason, "rejectedNegativeExergy")


if __name__ == "__main__":
    unittest.main()
