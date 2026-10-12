import contextlib
import io
import json
import os
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import MagicMock, patch
from pathlib import Path


sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from scripts.ci_runner import PhaseFailure, Runner, rss_bytes, select_phases, test_groups
from scripts.smoke_health import check_health
from test_proof_market import Oracle, exit_on_signal


class CiRunnerTests(unittest.TestCase):
    def test_discovery_covers_new_and_nested_names_exactly_once(self) -> None:
        names = ["build_one", "regression_check_json_parse_error", "soundness_build_u256",
                 "nested::parse_one", "soundness_future_case", "new_category"]
        grouped = test_groups(names)
        flattened = [name for group in grouped.values() for name in group]
        self.assertCountEqual(flattened, names)
        self.assertEqual(grouped["other"], ["new_category"])
        self.assertEqual(grouped["soundness"], ["soundness_build_u256", "soundness_future_case"])

    def test_empty_or_duplicate_discovery_fails(self) -> None:
        for names in ([], ["parse_one", "parse_one"]):
            with self.assertRaises(ValueError):
                test_groups(names)

    def test_selected_http_phase_builds_both_required_binaries(self) -> None:
        self.assertEqual(select_phases("full", ["market-http"], None),
                         ["build-anvil", "build-market", "market-http"])

    def test_selected_integration_group_discovers_current_tests_first(self) -> None:
        self.assertEqual(select_phases("full", ["integration-other"], None),
                         ["integration-prepare", "integration-other"])

    def test_resume_release_does_not_repeat_tests(self) -> None:
        self.assertEqual(select_phases("full", [], "release-anvil"),
                         ["release-anvil", "release-market"])

    def test_invalid_phase_selection_fails(self) -> None:
        for phases, start in ((["missing"], None), ([], "missing"), (["unit-anvil"], "unit-anvil")):
            with self.assertRaises(ValueError):
                select_phases("full", phases, start)

    def test_peak_memory_uses_correct_units_on_mac_and_linux(self) -> None:
        self.assertEqual(rss_bytes(8192, "darwin"), 8192)
        self.assertEqual(rss_bytes(8192, "linux"), 8192 * 1024)

    def test_success_preserves_output_and_structured_metrics(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            runner = Runner(root, root / "logs", 8)
            with contextlib.redirect_stdout(io.StringIO()):
                output = runner.run([sys.executable, "-c", "print('ready')"], capture=True)
            events = [json.loads(line) for line in (runner.logs / "events.jsonl").read_text().splitlines()]
            self.assertEqual(output, "ready\n")
            self.assertEqual(events[-1]["status"], "passed")
            self.assertEqual(events[-1]["exit_code"], 0)
            self.assertEqual(Path(events[-1]["log"]).read_text(), "ready\n")
            self.assertGreater(events[-1]["peak_rss_bytes"], 0)

    def test_sigkill_is_recorded_without_claiming_oom(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            runner = Runner(root, root / "logs", 8)
            with contextlib.redirect_stdout(io.StringIO()), self.assertRaises(PhaseFailure) as failure:
                runner.run([sys.executable, "-c", "import os, signal; os.kill(os.getpid(), signal.SIGKILL)"])
            event = json.loads((runner.logs / "events.jsonl").read_text().splitlines()[-1])
            self.assertEqual(failure.exception.code, 137)
            self.assertEqual(event["signal"], "SIGKILL")
            self.assertEqual(event["status"], "failed")

    def test_explicit_exit_137_is_not_misreported_as_a_signal(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            runner = Runner(root, root / "logs", 8)
            with contextlib.redirect_stdout(io.StringIO()), self.assertRaises(PhaseFailure) as failure:
                runner.run([sys.executable, "-c", "import sys; sys.exit(137)"])
            event = json.loads((runner.logs / "events.jsonl").read_text().splitlines()[-1])
            self.assertEqual(failure.exception.code, 137)
            self.assertIsNone(event["signal"])

    def test_health_success_stops_server_and_preserves_its_log(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "saas.log"
            process = MagicMock()
            process.poll.return_value = None
            response = io.BytesIO(json.dumps({
                "status": "online", "engine": "Anvil+Z3 Formal Verification", "version": "0.6.0",
            }).encode())
            with patch.dict(os.environ, {"ANVIL_SAAS_LOG": str(log), "ANVIL_SMOKE_PORT": "0"}), \
                    patch("scripts.smoke_health.subprocess.Popen", return_value=process), \
                    patch("scripts.smoke_health.urllib.request.urlopen", return_value=response), \
                    contextlib.redirect_stdout(io.StringIO()):
                check_health("anvil")
            process.terminate.assert_called_once()
            process.wait.assert_called_once_with(timeout=5)
            self.assertTrue(log.exists())

    def test_invalid_health_response_still_stops_server(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            process = MagicMock()
            process.poll.return_value = None
            with patch.dict(os.environ, {"ANVIL_SAAS_LOG": str(Path(directory) / "saas.log"), "ANVIL_SMOKE_PORT": "0"}), \
                    patch("scripts.smoke_health.subprocess.Popen", return_value=process), \
                    patch("scripts.smoke_health.urllib.request.urlopen", return_value=io.BytesIO(b'{"status":"offline"}')), \
                    self.assertRaises(ValueError):
                check_health("anvil")
            process.terminate.assert_called_once()

    def test_oracle_interrupted_during_startup_stops_detached_server(self) -> None:
        process = MagicMock()
        with patch("test_proof_market.anvil_binary", return_value="anvil"), \
                patch("test_proof_market.proof_market_binary", return_value="proof-market"), \
                patch("test_proof_market.subprocess.Popen", return_value=process), \
                patch("test_proof_market.wait_for_oracle", side_effect=SystemExit(143)), \
                patch("test_proof_market.terminate_process") as terminate, \
                self.assertRaises(SystemExit):
            with Oracle({}):
                self.fail("Interrupted startup must not enter the context")
        terminate.assert_called_once_with(process, signal.SIGTERM)
        process.wait.assert_called_once()

    def test_http_signal_handler_unwinds_context_with_exit_143(self) -> None:
        with self.assertRaises(SystemExit) as result:
            exit_on_signal(signal.SIGTERM, None)
        self.assertEqual(result.exception.code, 143)

    def test_interrupted_runner_forwards_sigterm_and_records_failure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = (
                "import signal, sys\n"
                "from pathlib import Path\n"
                "from scripts.ci_runner import Runner, PhaseFailure\n"
                "runner = Runner(Path(sys.argv[1]), Path(sys.argv[1]) / 'logs', 8)\n"
                "signal.signal(signal.SIGTERM, runner.interrupt)\n"
                "try:\n"
                "    runner.run([sys.executable, '-c', \"import time; print('ready', flush=True); time.sleep(60)\"])\n"
                "except PhaseFailure as exc:\n"
                "    sys.exit(exc.code)\n"
            )
            with subprocess.Popen([sys.executable, "-c", source, str(root)],
                                  cwd=Path(__file__).resolve().parent.parent,
                                  stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL) as process:
                try:
                    deadline = time.monotonic() + 5
                    log = root / "logs/preflight.001.log"
                    while not log.exists() or "ready" not in log.read_text():
                        if process.poll() is not None or time.monotonic() >= deadline:
                            self.fail("Child did not become ready")
                        time.sleep(0.02)
                    os.kill(process.pid, signal.SIGTERM)
                    self.assertEqual(process.wait(timeout=5), 143)
                finally:
                    if process.poll() is None:
                        process.terminate()
                        process.wait(timeout=5)
            event = json.loads((root / "logs/events.jsonl").read_text().splitlines()[-1])
            self.assertEqual(event["signal"], "SIGTERM")
            self.assertEqual(event["status"], "failed")


if __name__ == "__main__":
    unittest.main()
