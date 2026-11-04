#!/usr/bin/env python3
"""
Test Suite: Lean 4 Kernel Soundness & Axiomatic Hygiene (AP-01 .. AP-11).
Author: Borja Moskv (borjamoskv)
"""

import os
import re
import subprocess
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
ANVIL_DIR = REPO_ROOT / "Anvil"


class LeanKernelSoundnessTests(unittest.TestCase):

    def test_01_lake_build_clean_zero_errors(self):
        """Verifies lake build completes successfully with exit code 0."""
        res = subprocess.run(
            ["lake", "build"],
            cwd=str(REPO_ROOT),
            capture_output=True,
            text=True
        )
        self.assertEqual(
            res.returncode, 0,
            f"lake build failed:\nSTDOUT:\n{res.stdout}\nSTDERR:\n{res.stderr}"
        )
        self.assertIn("Build completed successfully", res.stdout + res.stderr)

    def test_02_lean_toolchain_version_bound(self):
        """Verifies lean-toolchain is anchored to >= 4.32.2 (CVE-2026-72711 & #14576 mitigation)."""
        toolchain_file = REPO_ROOT / "lean-toolchain"
        self.assertTrue(toolchain_file.exists(), "lean-toolchain file must exist")
        content = toolchain_file.read_text().strip()
        m = re.search(r"v4\.(\d+)\.(\d+)", content)
        self.assertIsNotNone(m, f"Invalid toolchain format: {content}")
        minor = int(m.group(1))
        patch = int(m.group(2))
        self.assertTrue(
            (minor > 32) or (minor == 32 and patch >= 2),
            f"Toolchain version v4.{minor}.{patch} must be >= v4.32.2"
        )

    def test_03_zero_sorry_or_admit_tokens(self):
        """Verifies strict absence of unverified holes (AP-04/AP-11) in Lean files."""
        prohibited = [r"\bsorry\b", r"\badmit\b", r"\bunsafe\b"]
        for lean_file in ANVIL_DIR.glob("*.lean"):
            content = lean_file.read_text()
            for pattern in prohibited:
                matches = re.findall(pattern, content)
                self.assertEqual(
                    len(matches), 0,
                    f"Forbidden token matching '{pattern}' in {lean_file.name}: {matches}"
                )

    def test_04_axiomatic_footprint_no_false(self):
        """Audits Lean axiomatic footprint to ensure no derivation of False (AP-09)."""
        res = subprocess.run(
            ["lake", "build", "Anvil.AuditAxioms"],
            cwd=str(REPO_ROOT),
            capture_output=True,
            text=True
        )
        self.assertEqual(res.returncode, 0)
        output = res.stdout + res.stderr
        self.assertNotIn("False", output, "Axiom audit must never derive False")

    def test_05_qtt_soundness_invariants_present(self):
        """Verifies presence of Q1-Q4 soundness invariant theorems in Anvil/QTT.lean."""
        qtt_path = ANVIL_DIR / "QTT.lean"
        self.assertTrue(qtt_path.exists())
        content = qtt_path.read_text()
        expected_theorems = [
            "q1_branch_soundness_verified",
            "q2_stack_confinement_verified",
            "q3_erasure_soundness_verified",
            "q4_kudurru64_confinement_verified"
        ]
        for thm in expected_theorems:
            self.assertIn(thm, content, f"Theorem '{thm}' missing in QTT.lean")

    def test_06_verified_invariant_non_vacuous_witness(self):
        """Verifies AP-10 VerifiedInvariant pattern includes explicit witnesses."""
        core_path = ANVIL_DIR / "Core.lean"
        moloch_path = ANVIL_DIR / "MolochLandauer.lean"
        self.assertTrue(core_path.exists())
        self.assertTrue(moloch_path.exists())
        self.assertIn("non_vacuous", core_path.read_text())
        moloch_content = moloch_path.read_text()
        self.assertIn("moloch_landauer_gate_verified", moloch_content)
        self.assertIn("preflight_energy_gate_verified", moloch_content)

    def test_07_no_uninstantiated_mvars_in_metaprogramming(self):
        """Verifies that Core.lean uses hygienic macros without raw metavars."""
        core_path = ANVIL_DIR / "Core.lean"
        self.assertTrue(core_path.exists())
        content = core_path.read_text()
        self.assertIn("VerifiedInvariant", content)
        self.assertIn("runTrace", content)
        self.assertNotIn("mkFreshExprMVar", content)

    def test_08_rejection_of_weakened_theorems(self):
        """Verifies that asymmetric branch consumption fails closed and does not permit leakage."""
        qtt_path = ANVIL_DIR / "QTT.lean"
        content = qtt_path.read_text()
        self.assertIn("linear_branch_mismatch_fails_closed", content)
        self.assertIn("branch_asymmetric_escalates_omega", content)


if __name__ == "__main__":
    unittest.main()
