import http.server
import socketserver
import base64
import json
import subprocess
import hashlib
import tempfile
import os
import signal
import sys
import time
import socket
import urllib.error
import urllib.parse
import urllib.request

try:
    import resource
except ImportError:
    resource = None

PORT = 8000
STRIPE_SESSION_ENDPOINT = "https://api.stripe.com/v1/checkout/sessions/"
MAX_SOURCE_BYTES = 50 * 1024
MAX_JSON_BODY_BYTES = MAX_SOURCE_BYTES * 6 + 4 * 1024
MAX_RESPONSE_OUTPUT_BYTES = 16 * 1024
MAX_CAPTURE_OUTPUT_BYTES = 1024 * 1024


def env_float(name, default):
    try:
        return float(os.environ.get(name, str(default)))
    except ValueError:
        return default


def env_int(name, default):
    try:
        return int(os.environ.get(name, str(default)))
    except ValueError:
        return default


PROCESS_TIMEOUT_SECONDS = max(1.0, env_float("ANVIL_PROCESS_TIMEOUT_SECS", 10.0))
PROCESS_MEMORY_MB = env_int("ANVIL_PROCESS_MEMORY_MB", 512)
CONSUMED_STRIPE_SESSIONS = set()


def write_json(handler, status_code, payload):
    handler.send_response(status_code)
    handler.send_header('Content-type', 'application/json')
    handler.end_headers()
    handler.wfile.write(json.dumps(payload).encode('utf-8'))


def prove_response(status, z3_output, execution_time_ms=0.0, certificate_hash=None):
    return {
        "status": status,
        "execution_time_ms": execution_time_ms,
        "certificate_hash": certificate_hash,
        "z3_output": z3_output,
    }


def parse_content_length(headers):
    try:
        content_length = int(headers.get('Content-Length', '0'))
    except ValueError:
        return None

    if content_length < 0:
        return None
    return content_length


def truncate_output(output):
    encoded = output.encode("utf-8")
    if len(encoded) <= MAX_RESPONSE_OUTPUT_BYTES:
        return output
    truncated = encoded[:MAX_RESPONSE_OUTPUT_BYTES]
    while truncated:
        try:
            return truncated.decode("utf-8") + "\n[output truncated]"
        except UnicodeDecodeError:
            truncated = truncated[:-1]
    return "[output truncated]"


def combined_process_output(process):
    if process.stderr and process.stdout:
        return f"{process.stderr}\n{process.stdout}".strip()
    return process.stderr or process.stdout


class OutputLimitExceeded(Exception):
    def __init__(self, stdout, stderr):
        super().__init__("Anvil output exceeded capture limit")
        self.stdout = stdout
        self.stderr = stderr


def read_capped_file(file):
    file.seek(0)
    data = file.read(MAX_CAPTURE_OUTPUT_BYTES + 1)
    truncated = len(data) > MAX_CAPTURE_OUTPUT_BYTES
    if truncated:
        data = data[:MAX_CAPTURE_OUTPUT_BYTES]
    text = data.decode("utf-8", errors="replace")
    if truncated:
        text += "\n[output capture truncated]"
    return text


def output_files_exceed_limit(stdout_file, stderr_file):
    return (
        os.fstat(stdout_file.fileno()).st_size > MAX_CAPTURE_OUTPUT_BYTES
        or os.fstat(stderr_file.fileno()).st_size > MAX_CAPTURE_OUTPUT_BYTES
    )


def child_preexec():
    os.setsid()
    apply_process_memory_limit()


def kill_process_tree(process):
    if process.poll() is not None:
        return
    if os.name == "posix":
        try:
            os.killpg(process.pid, signal.SIGKILL)
            return
        except ProcessLookupError:
            return
        except OSError:
            pass
    process.kill()


def run_anvil_command(args, cwd, timeout):
    stdout_file = tempfile.TemporaryFile()
    stderr_file = tempfile.TemporaryFile()
    try:
        process = subprocess.Popen(
            args,
            cwd=cwd,
            stdin=subprocess.DEVNULL,
            stdout=stdout_file,
            stderr=stderr_file,
            preexec_fn=child_preexec if os.name == "posix" else None,
        )
        deadline = time.monotonic() + timeout
        while True:
            returncode = process.poll()
            if returncode is not None:
                break

            if time.monotonic() >= deadline:
                kill_process_tree(process)
                process.wait()
                raise subprocess.TimeoutExpired(args, timeout)

            if output_files_exceed_limit(stdout_file, stderr_file):
                kill_process_tree(process)
                process.wait()
                raise OutputLimitExceeded(
                    read_capped_file(stdout_file),
                    read_capped_file(stderr_file),
                )

            time.sleep(0.025)

        return subprocess.CompletedProcess(
            args,
            returncode,
            stdout=read_capped_file(stdout_file),
            stderr=read_capped_file(stderr_file),
        )
    finally:
        stdout_file.close()
        stderr_file.close()


def unsupported_json_flag(output):
    lower = output.lower()
    return (
        "--json" in output
        and (
            "unexpected argument" in lower
            or "unknown argument" in lower
            or "found argument" in lower
            or "unrecognized option" in lower
        )
    )


def env_enabled(name):
    return os.environ.get(name, "").lower() in {"1", "true", "yes", "on"}


def legacy_anvil_output_enabled():
    return env_enabled("ANVIL_ALLOW_LEGACY_ANVIL_OUTPUT")


def parse_check_json(output):
    try:
        report = json.loads(output.strip())
    except (TypeError, json.JSONDecodeError):
        return None
    if isinstance(report, dict) and report.get("kind") == "check":
        return report
    return None


def check_report_succeeded(report):
    return (
        isinstance(report, dict)
        and report.get("schema_version") == "anvil.check.v1"
        and bool(str(report.get("anvil_version", "")).strip())
        and report.get("kind") == "check"
        and report.get("status") == "VERIFIED"
        and report.get("ok") is True
        and bool(str(report.get("proof_hash", "")).strip())
    )


def diagnostic_text(diagnostic):
    if not isinstance(diagnostic, dict):
        return None

    parts = []
    source = diagnostic.get("source")
    location = diagnostic.get("location")
    message = diagnostic.get("message")
    detail = diagnostic.get("detail")

    prefix = ""
    if source and location:
        prefix = f"{source} at {location}: "
    elif source:
        prefix = f"{source}: "

    if message:
        parts.append(f"{prefix}{message}")
    if detail:
        parts.append(str(detail))

    return "\n".join(parts) if parts else None


def check_json_output_text(report, fallback_output):
    if not isinstance(report, dict):
        return fallback_output

    lines = []
    status = report.get("status")
    message = report.get("message")
    if status and message:
        lines.append(f"{status}: {message}")
    elif message:
        lines.append(str(message))

    for diagnostic in report.get("errors") or []:
        text = diagnostic_text(diagnostic)
        if text:
            lines.append(text)

    for counterexample in report.get("counterexamples") or []:
        if isinstance(counterexample, dict) and counterexample.get("text"):
            lines.append(str(counterexample["text"]))

    if not lines:
        for result in report.get("results") or []:
            if isinstance(result, dict):
                fn_name = result.get("fn_name", "<unknown>")
                result_status = result.get("status", "UNKNOWN")
                lines.append(f"{fn_name}: {result_status}")

    return "\n".join(lines) if lines else fallback_output


def check_json_resource_exhausted(report):
    if not isinstance(report, dict):
        return False
    if report.get("status") == "Z3_RESOURCE_EXHAUSTED":
        return True
    return any(
        isinstance(result, dict) and result.get("status") == "Z3_RESOURCE_EXHAUSTED"
        for result in (report.get("results") or [])
    )


def apply_process_memory_limit():
    if resource is None or PROCESS_MEMORY_MB <= 0 or sys.platform == "darwin":
        return

    limit_resource = getattr(resource, "RLIMIT_AS", getattr(resource, "RLIMIT_DATA", None))
    if limit_resource is None:
        return

    limit = PROCESS_MEMORY_MB * 1024 * 1024
    resource.setrlimit(limit_resource, (limit, limit))


def z3_resource_exhausted(returncode, output):
    lower = output.lower()
    return (
        "z3 undecidable" in lower
        or "z3 unknown" in lower
        or "solver unknown" in lower
        or "is undecidable" in lower
        or "z3_resource_exhausted" in lower
        or "out of memory" in lower
        or "cannot allocate memory" in lower
        or "memory allocation" in lower
        or returncode in (-6, -9)
    )


def has_concrete_verification_failure(output):
    return any(
        not z3_resource_exhausted(0, line)
        and (
            "VERIFICATION FAILED" in line
            or "GLOBAL INVARIANT VIOLATED" in line
            or "Postcondition" in line
            or "Contract invariant" in line
        )
        for line in output.splitlines()
    )


def cargo_target_dir(cwd):
    metadata = subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=cwd,
        text=True,
        timeout=5.0,
    )
    return json.loads(metadata)["target_directory"]


def anvil_binary_path(workspace_root):
    if "ANVIL_BIN" in os.environ:
        path = os.path.abspath(os.environ["ANVIL_BIN"])
        if not is_executable_file(path):
            raise RuntimeError("ANVIL_BIN must point to an executable file")
        return path

    candidates = []
    target_dir = os.environ.get("CARGO_TARGET_DIR")
    if target_dir:
        if not os.path.isabs(target_dir):
            target_dir = os.path.abspath(os.path.join(workspace_root, target_dir))
        candidates.append(os.path.join(target_dir, "debug", "anvil"))

    try:
        metadata_target_dir = cargo_target_dir(workspace_root)
        candidates.append(os.path.join(metadata_target_dir, "debug", "anvil"))
    except (
        subprocess.CalledProcessError,
        subprocess.TimeoutExpired,
        FileNotFoundError,
        KeyError,
        json.JSONDecodeError,
    ):
        pass

    candidates.append(os.path.join(workspace_root, "target/debug/anvil"))

    for candidate in candidates:
        if is_executable_file(candidate):
            return candidate

    return candidates[0]


def is_executable_file(path):
    return os.path.isfile(path) and os.access(path, os.X_OK)


def required_env(name):
    value = os.environ.get(name, "")
    if not value.strip():
        raise RuntimeError(f"{name} is required")
    return value


def certificate_secret():
    return required_env("ANVIL_CERTIFICATE_SECRET")


def mock_payment_enabled():
    return env_enabled("ANVIL_ALLOW_MOCK_PAYMENT")


def wants_mock_payment(request):
    return str(request.get("payment_mode", "")).lower() == "mock"


def valid_stripe_session_id(session_id):
    return (
        isinstance(session_id, str)
        and (session_id.startswith("cs_test_") or session_id.startswith("cs_live_"))
        and 9 <= len(session_id) <= 255
        and session_id.isascii()
        and all(ch.isalnum() or ch in "_-" for ch in session_id)
    )


def reserve_stripe_session(session_id):
    if session_id in CONSUMED_STRIPE_SESSIONS:
        return False
    CONSUMED_STRIPE_SESSIONS.add(session_id)
    return True


def fetch_stripe_session(session_id):
    if not valid_stripe_session_id(session_id):
        return None

    api_key = required_env("STRIPE_API_KEY")
    auth = base64.b64encode(f"{api_key}:".encode("utf-8")).decode("ascii")
    url = STRIPE_SESSION_ENDPOINT + urllib.parse.quote(session_id, safe="")
    req = urllib.request.Request(url, headers={"Authorization": f"Basic {auth}"})

    try:
        with urllib.request.urlopen(
            req,
            timeout=max(1.0, env_float("ANVIL_STRIPE_TIMEOUT_SECS", 10.0)),
        ) as response:
            session = json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as exc:
        if exc.code in (401, 403):
            raise RuntimeError("Stripe API rejected STRIPE_API_KEY") from exc
        if exc.code in (400, 404):
            return None
        raise RuntimeError(f"Stripe API returned HTTP {exc.code}") from exc
    except (urllib.error.URLError, socket.timeout, TimeoutError, OSError, json.JSONDecodeError) as exc:
        raise RuntimeError(f"Stripe API Error: {exc}") from exc

    return session


def validate_stripe_session_details(session, client_id):
    if not isinstance(session, dict):
        return "Stripe API returned an invalid session object."

    if session.get("payment_status") != "paid":
        return "Stripe session is not marked as paid."

    metadata = session.get("metadata") or {}
    if not isinstance(metadata, dict):
        return "Stripe session metadata is invalid."

    bindings = [
        ("client_reference_id", session.get("client_reference_id")),
        ("metadata.client_id", metadata.get("client_id")),
    ]
    matched = False
    for label, value in bindings:
        if isinstance(value, str) and value.strip():
            if value.strip() != client_id:
                return f"Stripe session {label} does not match client_id."
            matched = True
    if not matched:
        return "Stripe session must include client_reference_id or metadata.client_id matching client_id."

    expected_amount = os.environ.get("ANVIL_STRIPE_EXPECTED_AMOUNT_CENTS")
    if not expected_amount:
        return "ANVIL_STRIPE_EXPECTED_AMOUNT_CENTS is required in Stripe mode."
    try:
        expected_amount = int(expected_amount)
    except ValueError:
        return "ANVIL_STRIPE_EXPECTED_AMOUNT_CENTS must be an integer."
    if expected_amount <= 0:
        return "ANVIL_STRIPE_EXPECTED_AMOUNT_CENTS must be greater than zero."
    if session.get("amount_total") != expected_amount:
        return (
            f"Stripe session amount_total does not match expected amount: "
            f"{session.get('amount_total')} != {expected_amount}"
        )

    expected_currency = os.environ.get("ANVIL_STRIPE_EXPECTED_CURRENCY", "").lower()
    if not expected_currency:
        return "ANVIL_STRIPE_EXPECTED_CURRENCY is required in Stripe mode."
    if str(session.get("currency", "")).lower() != expected_currency:
        return (
            f"Stripe session currency does not match expected currency: "
            f"{session.get('currency')} != {expected_currency}"
        )

    return None


class ProofMarketHandler(http.server.SimpleHTTPRequestHandler):
    def setup(self):
        super().setup()
        self.connection.settimeout(max(PROCESS_TIMEOUT_SECONDS, 10.0))

    def do_GET(self):
        if self.path == "/health":
            write_json(self, 200, {"status": "online", "engine": "Anvil Proof Market Python Oracle"})
        else:
            write_json(self, 404, {"detail": "Not Found"})

    def do_POST(self):
        # Reject before parsing, billing, or running a verifier.
        if self.path == '/v1/prove':
            write_json(self, 503, prove_response(
                "CERTIFICATION_UNAVAILABLE",
                "OMEGA-21: legacy proof issuance disabled pending independent validation.",
            ))
            return
        self.send_response(404)
        self.end_headers()


if __name__ == "__main__":
    print(f"Anvil legacy service booting on port {PORT}; proof issuance disabled.")
    with socketserver.TCPServer(("127.0.0.1", PORT), ProofMarketHandler) as httpd:
        httpd.serve_forever()
