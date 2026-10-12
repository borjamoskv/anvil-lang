#!/usr/bin/env python3
"""Run Anvil checks in logged, independently selectable phases."""

import argparse
import json
import os
import shlex
import signal
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, List, Optional, Sequence


ROOT = Path(__file__).resolve().parent.parent
MARKET = "services/proof-market/Cargo.toml"
GROUPS = ("build", "parse", "regression", "soundness", "verify", "other")
INTEGRATION = ["integration-prepare"] + [f"integration-{group}" for group in GROUPS]
QUICK = [
    "runner-tests", "fmt-anvil", "fmt-market", "check-anvil", "check-market",
    "unit-anvil", "unit-market",
]
SUITES = {
    "quick": QUICK,
    "smoke": ["check-anvil", "unit-anvil", "build-anvil"] + INTEGRATION
    + ["doctor", "transfer", "saas"],
    "full": ["runner-tests", "fmt-anvil", "fmt-market", "build-anvil", "build-market",
             "unit-anvil", "unit-market"] + INTEGRATION
    + ["market-http", "market-amm", "hello", "transfer", "doctor", "saas",
       "release-anvil", "release-market"],
}
DEPENDENCIES = {
    **{f"integration-{group}": ["integration-prepare"] for group in GROUPS},
    **{phase: ["build-anvil"] for phase in ("doctor", "hello", "transfer", "saas")},
    **{phase: ["build-anvil", "build-market"] for phase in ("market-http", "market-amm")},
}


class PhaseFailure(RuntimeError):
    def __init__(self, code: int, message: str, returncode: Optional[int] = None):
        super().__init__(message)
        self.code = code
        self.returncode = code if returncode is None else returncode


def exit_details(returncode: int) -> Dict[str, object]:
    return {
        "exit_code": 128 - returncode if returncode < 0 else returncode,
        "signal": signal.Signals(-returncode).name if returncode < 0 else None,
    }


def rss_bytes(max_rss: int, platform: str) -> int:
    return max_rss if platform == "darwin" else max_rss * 1024


def test_groups(names: Sequence[str]) -> Dict[str, List[str]]:
    if not names or len(set(names)) != len(names):
        raise ValueError("Test discovery must return a nonempty, unique list")
    groups: Dict[str, List[str]] = {name: [] for name in GROUPS}
    for name in sorted(names):
        short_name = name.rsplit("::", 1)[-1]
        group = next((g for g in GROUPS[:-1] if short_name.startswith(g + "_")), "other")
        groups[group].append(name)
    return groups


def select_phases(suite: str, phases: Sequence[str], start: Optional[str]) -> List[str]:
    available = SUITES[suite]
    if phases and start:
        raise ValueError("Use either --phase or --from")
    requested = list(phases) if phases else available[available.index(start):] if start else available
    unknown = set(requested) - set(available)
    if unknown:
        raise ValueError("Unknown phase: " + ", ".join(sorted(unknown)))
    selected = set(requested)
    for phase in requested:
        selected.update(DEPENDENCIES.get(phase, []))
    return [phase for phase in available if phase in selected]


class Runner:
    def __init__(self, root: Path, logs: Path, batch_size: int):
        self.root = root
        self.logs = logs
        self.logs.mkdir(parents=True)
        self.batch_size = batch_size
        self.target = root / "target"
        self.phase = "preflight"
        self.command_number = 0
        self.process: Optional[subprocess.Popen] = None
        self.interrupted: Optional[int] = None
        self.tests: Dict[str, List[str]] = {}
        self.harness: Optional[str] = None

    def event(self, payload: Dict[str, object]) -> None:
        with (self.logs / "events.jsonl").open("a", encoding="utf-8") as log:
            log.write(json.dumps({"time": datetime.now(timezone.utc).isoformat(), **payload}) + "\n")

    def interrupt(self, signum: int, _frame: object) -> None:
        self.interrupted = signum
        if self.process is not None and self.process.returncode is None:
            try:
                os.killpg(self.process.pid, signum)
            except ProcessLookupError:
                pass

    def run(self, command: Sequence[str], capture: bool = False,
            env: Optional[Dict[str, str]] = None) -> str:
        if self.interrupted:
            raise PhaseFailure(128 + self.interrupted, "Run interrupted", -self.interrupted)
        self.command_number += 1
        stem = f"{self.phase}.{self.command_number:03d}"
        output_path = self.logs / (stem + ".log")
        command = list(command)
        print("+ " + shlex.join(command), flush=True)
        details = {"phase": self.phase, "command": command, "log": str(output_path)}
        self.event({**details, "status": "running"})
        began = time.monotonic()
        lines: List[str] = []
        returncode = 1
        rss: Optional[int] = None
        try:
            with output_path.open("w", encoding="utf-8") as log:
                self.process = subprocess.Popen(
                    command, cwd=self.root, env=env, stdout=subprocess.PIPE,
                    stderr=subprocess.STDOUT, text=True, errors="replace", start_new_session=True,
                )
                assert self.process.stdout is not None
                for line in self.process.stdout:
                    log.write(line)
                    log.flush()
                    if capture:
                        lines.append(line)
                    else:
                        print(line, end="", flush=True)
                self.process.stdout.close()
                # wait4 retains both the original signal and this process's resource usage.
                _, status, usage = os.wait4(self.process.pid, 0)
                returncode = os.waitstatus_to_exitcode(status)
                self.process.returncode = returncode
                rss = rss_bytes(usage.ru_maxrss, sys.platform)
        finally:
            if self.process is not None:
                # Stop background children even when the command's leader already exited.
                try:
                    os.killpg(self.process.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                self.process.wait()
                self.process = None
            duration = time.monotonic() - began
            actual_returncode = -self.interrupted if self.interrupted else returncode
            result = exit_details(actual_returncode)
            self.event({**details, **result, "status": "passed" if result["exit_code"] == 0 else "failed",
                        "duration_seconds": round(duration, 3), "peak_rss_bytes": rss,
                        "returncode": returncode})
        memory = f", peak RSS {rss / 1024 / 1024:.1f} MiB" if rss is not None else ""
        print(f"  exit={result['exit_code']} ({duration:.2f}s{memory})", flush=True)
        if result["exit_code"] != 0:
            if capture:
                print("".join(lines), end="", flush=True)
            raise PhaseFailure(int(result["exit_code"]), f"Failed phase {self.phase}; log: {output_path}", actual_returncode)
        return "".join(lines)

    def preflight(self) -> None:
        print(f"Workspace: {self.root}\nLogs: {self.logs}", flush=True)
        self.run(["cargo", "--version"])
        self.run(["rustc", "--version"])
        metadata = self.run(["cargo", "metadata", "--no-deps", "--format-version", "1"], capture=True)
        self.target = Path(json.loads(metadata)["target_directory"])
        print(f"Cargo target: {self.target}", flush=True)
        environment = {key: os.environ[key] for key in (
            "CARGO_BUILD_JOBS", "CARGO_INCREMENTAL", "CARGO_PROFILE_DEV_DEBUG",
            "CARGO_PROFILE_TEST_DEBUG", "RUST_TEST_THREADS", "RUSTFLAGS",
            "Z3_SYS_Z3_HEADER", "LIBRARY_PATH",
        ) if key in os.environ}
        (self.logs / "environment.json").write_text(json.dumps({
            "workspace": str(self.root), "target": str(self.target), "env": environment,
        }, indent=2) + "\n")

    def prepare_integration(self) -> None:
        output = self.run(["cargo", "test", "--test", "integration_tests", "--no-run",
                           "--message-format=json-render-diagnostics"], capture=True)
        artifacts = [json.loads(line) for line in output.splitlines() if line.startswith("{")]
        executables = [item["executable"] for item in artifacts
                       if item.get("reason") == "compiler-artifact"
                       and item.get("target", {}).get("name") == "integration_tests"
                       and item.get("executable")]
        if len(executables) != 1:
            raise ValueError("Cargo did not report exactly one integration test executable")
        self.harness = executables[0]
        listing = self.run([self.harness, "--list"], capture=True)
        names = [line.removesuffix(": test") for line in listing.splitlines() if line.endswith(": test")]
        self.tests = test_groups(names)
        (self.logs / "integration-tests.json").write_text(json.dumps(self.tests, indent=2) + "\n")
        print(f"Discovered {len(names)} tests: " + ", ".join(
            f"{group}={len(tests)}" for group, tests in self.tests.items()), flush=True)

    def phase_action(self, phase: str) -> None:
        cargo = ["cargo"]
        commands = {
            "runner-tests": [sys.executable, "-m", "unittest", "discover", "-s", "tests", "-p", "test_ci_runner.py"],
            "fmt-anvil": cargo + ["fmt", "--", "--check"],
            "fmt-market": cargo + ["fmt", "--manifest-path", MARKET, "--", "--check"],
            "check-anvil": cargo + ["check", "--bin", "anvil"],
            "check-market": cargo + ["check", "--manifest-path", MARKET],
            "build-anvil": cargo + ["build", "--bin", "anvil"],
            "build-market": cargo + ["build", "--manifest-path", MARKET, "--bin", "proof-market"],
            "unit-anvil": cargo + ["test", "--bin", "anvil", "--", "--test-threads=1"],
            "unit-market": cargo + ["test", "--manifest-path", MARKET, "--", "--test-threads=1"],
            "release-anvil": cargo + ["build", "--release", "--bin", "anvil"],
            "release-market": cargo + ["build", "--release", "--manifest-path", MARKET, "--bin", "proof-market"],
        }
        if phase in commands:
            self.run(commands[phase])
        elif phase == "integration-prepare":
            self.prepare_integration()
        elif phase.startswith("integration-"):
            assert self.harness is not None
            names = self.tests[phase.removeprefix("integration-")]
            for index in range(0, len(names), self.batch_size):
                self.run([self.harness, "--exact", "--test-threads=1"] + names[index:index + self.batch_size])
            if not names:
                print("No tests in this group.", flush=True)
        elif phase in ("market-http", "market-amm"):
            env = os.environ.copy()
            env.update(ANVIL_BIN=str(self.target / "debug/anvil"),
                       PROOF_MARKET_BIN=str(self.target / "debug/proof-market"))
            test = "test_proof_market.py" if phase == "market-http" else "test_proof_market_amm.py"
            self.run([sys.executable, "tests/" + test], env=env)
        elif phase == "saas":
            with tempfile.TemporaryDirectory(prefix="anvil-saas-") as directory:
                env = os.environ.copy()
                env["ANVIL_SAAS_LOG"] = str(self.logs / "saas-server.log")
                env["DATABASE_URL"] = f"sqlite:{Path(directory) / 'anvil.db'}?mode=rwc"
                self.run([sys.executable, "scripts/smoke_health.py", str(self.target / "debug/anvil")], env=env)
        else:
            binary = str(self.target / "debug/anvil")
            command = [binary, "doctor", "--json"] if phase == "doctor" else [
                binary, "check", "--json", "--timeout", "5000", f"examples/{phase}.anv",
            ]
            payload = json.loads(self.run(command, capture=True))
            if payload.get("ok") is not True:
                raise ValueError(f"{phase} returned ok != true: {payload}")
            if phase == "doctor" and not payload.get("checks"):
                raise ValueError("Doctor returned no checks")
            if phase != "doctor" and (payload.get("status") != "VERIFIED" or payload.get("timeout_ms") != 5000):
                raise ValueError(f"Unexpected check result: {payload}")
            print(json.dumps(payload), flush=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("suite", choices=SUITES)
    parser.add_argument("--phase", action="append", default=[], help="Run a named phase and its build prerequisites")
    parser.add_argument("--from", dest="start", help="Run from this phase onwards, including build prerequisites")
    parser.add_argument("--list", action="store_true", help="List phases without building")
    parser.add_argument("--batch-size", type=int, default=8, help="Maximum integration tests per process (default: 8)")
    args = parser.parse_args()
    try:
        if args.batch_size < 1:
            raise ValueError("--batch-size must be positive")
        phases = select_phases(args.suite, args.phase, args.start)
    except ValueError as exc:
        parser.error(str(exc))
    if args.list:
        print("\n".join(phases))
        return 0
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
    log_root = Path(os.environ.get("ANVIL_CI_LOG_DIR", str(ROOT / "cache/ci")))
    runner = Runner(ROOT, log_root / f"{args.suite}-{stamp}-{os.getpid()}", args.batch_size)
    previous_handlers = {sig: signal.signal(sig, runner.interrupt) for sig in (signal.SIGINT, signal.SIGTERM)}
    completed: List[str] = []
    summary: Dict[str, object] = {"suite": args.suite, "phases": phases, "completed": completed, "status": "running"}
    summary_path = runner.logs / "summary.json"

    def save_summary() -> None:
        temporary = summary_path.with_suffix(".tmp")
        temporary.write_text(json.dumps(summary, indent=2) + "\n")
        temporary.replace(summary_path)

    save_summary()
    try:
        runner.preflight()
        for phase in phases:
            if runner.interrupted:
                raise PhaseFailure(128 + runner.interrupted, "Run interrupted", -runner.interrupted)
            runner.phase = phase
            summary["current_phase"] = phase
            save_summary()
            print(f"\n==> {phase}", flush=True)
            runner.phase_action(phase)
            completed.append(phase)
            save_summary()
        if runner.interrupted:
            raise PhaseFailure(128 + runner.interrupted, "Run interrupted", -runner.interrupted)
        summary.update(status="passed", exit_code=0)
        print(f"\n{args.suite} passed. Logs: {runner.logs}", flush=True)
        return 0
    except (PhaseFailure, OSError, ValueError) as exc:
        code = exc.code if isinstance(exc, PhaseFailure) else 1
        summary.update(status="failed", error=str(exc),
                       **exit_details(exc.returncode if isinstance(exc, PhaseFailure) else code))
        retry = [sys.executable, str(Path(__file__).resolve()), args.suite]
        if runner.phase != "preflight":
            retry += ["--phase", runner.phase]
        print(f"\n{exc}\nRepeat: {shlex.join(retry)}",
              file=sys.stderr, flush=True)
        return code
    finally:
        save_summary()
        for sig, handler in previous_handlers.items():
            signal.signal(sig, handler)


if __name__ == "__main__":
    sys.exit(main())
