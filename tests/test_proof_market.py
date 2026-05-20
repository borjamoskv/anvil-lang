import json
import os
import signal
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request


WORKSPACE_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "../"))
RUST_API_DIR = os.path.join(WORKSPACE_ROOT, "services/proof-market")
MAX_JSON_BODY_BYTES = 50 * 1024 * 6 + 4 * 1024
CARGO_METADATA_TIMEOUT_SECONDS = 30
CARGO_BUILD_TIMEOUT_SECONDS = 600
ORACLE_SHUTDOWN_TIMEOUT_SECONDS = 5

SAFE_TRANSFER = (
    "fn transfer(mut sender_balance: u64, mut receiver_balance: u64, amount: u64) -> u64 "
    "where { amount > 0, sender_balance >= amount, "
    "sender_balance' + receiver_balance' == sender_balance + receiver_balance, "
    "sender_balance' == sender_balance - amount, "
    "receiver_balance' == receiver_balance + amount } "
    "{ sender_balance -= amount; receiver_balance += amount; return sender_balance; }"
)

BAD_TRANSFER = SAFE_TRANSFER.replace("receiver_balance += amount; ", "")
assert BAD_TRANSFER != SAFE_TRANSFER


def free_port():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def wait_for_oracle(process, health_url):
    for _ in range(40):
        if process.poll() is not None:
            raise RuntimeError(f"Proof Market process exited early with code {process.returncode}")
        try:
            with urllib.request.urlopen(health_url, timeout=0.25):
                return
        except Exception:
            time.sleep(0.1)
    raise TimeoutError("Proof Market process did not become healthy")


def cargo_target_dir(cwd):
    metadata = subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=cwd,
        text=True,
        timeout=CARGO_METADATA_TIMEOUT_SECONDS,
    )
    return json.loads(metadata)["target_directory"]


def workspace_path(path):
    if os.path.isabs(path):
        return path
    return os.path.abspath(os.path.join(WORKSPACE_ROOT, path))


def is_executable_file(path):
    return os.path.isfile(path) and os.access(path, os.X_OK)


def debug_binary(env_var, binary_name, fallback_path, build_cwd, build_args):
    configured = os.environ.get(env_var)
    if configured:
        configured = workspace_path(configured)
        if not is_executable_file(configured):
            raise FileNotFoundError(f"{env_var} does not point to an executable file: {configured}")
        return configured

    candidates = []
    target_dir = os.environ.get("CARGO_TARGET_DIR")
    if target_dir:
        target_dir = workspace_path(target_dir)
    else:
        target_dir = cargo_target_dir(build_cwd)
    if target_dir:
        candidates.append(os.path.join(target_dir, "debug", binary_name))
    candidates.append(fallback_path)

    subprocess.run(
        build_args,
        cwd=build_cwd,
        check=True,
        timeout=CARGO_BUILD_TIMEOUT_SECONDS,
    )

    for candidate in candidates:
        if is_executable_file(candidate):
            return candidate

    raise FileNotFoundError(f"Could not build {binary_name}; checked: {candidates}")


def proof_market_binary():
    return debug_binary(
        "PROOF_MARKET_BIN",
        "proof-market",
        os.path.join(RUST_API_DIR, "target/debug/proof-market"),
        RUST_API_DIR,
        ["cargo", "build", "-q"],
    )


def anvil_binary():
    return debug_binary(
        "ANVIL_BIN",
        "anvil",
        os.path.join(WORKSPACE_ROOT, "target/debug/anvil"),
        WORKSPACE_ROOT,
        ["cargo", "build", "-q", "--bin", "anvil"],
    )


class Oracle:
    def __init__(self, extra_env):
        self.extra_env = extra_env
        self.port = None
        self.base_url = None
        self.process = None

    def __enter__(self):
        anvil_bin = anvil_binary()
        proof_market_bin = proof_market_binary()
        self.port = free_port()
        self.base_url = f"http://127.0.0.1:{self.port}"

        env = os.environ.copy()
        env["ANVIL_BIN"] = anvil_bin
        env["PROOF_MARKET_ADDR"] = f"127.0.0.1:{self.port}"
        for key, value in self.extra_env.items():
            if value is None:
                env.pop(key, None)
            else:
                env[key] = value

        self.process = subprocess.Popen(
            [proof_market_bin],
            cwd=RUST_API_DIR,
            env=env,
            start_new_session=(os.name == "posix"),
        )
        try:
            wait_for_oracle(self.process, f"{self.base_url}/health")
        except Exception:
            self.__exit__(None, None, None)
            raise
        return self

    def __exit__(self, exc_type, exc, tb):
        if self.process is None:
            return
        terminate_process(self.process, signal.SIGTERM)
        try:
            self.process.wait(timeout=ORACLE_SHUTDOWN_TIMEOUT_SECONDS)
        except subprocess.TimeoutExpired:
            terminate_process(self.process, signal.SIGKILL)
            self.process.wait(timeout=ORACLE_SHUTDOWN_TIMEOUT_SECONDS)


def terminate_process(process, sig):
    if process.poll() is not None:
        return
    try:
        if os.name == "posix":
            os.killpg(process.pid, sig)
        elif sig == signal.SIGKILL:
            process.kill()
        else:
            process.terminate()
    except ProcessLookupError:
        return


def post_json(base_url, payload):
    data = json.dumps(payload).encode("utf-8")
    return post_raw(base_url, data, "application/json")


def post_raw(base_url, data, content_type):
    req = urllib.request.Request(
        f"{base_url}/v1/prove",
        data=data,
        headers={"Content-Type": content_type},
    )

    try:
        with urllib.request.urlopen(req, timeout=20) as response:
            return response.status, json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as exc:
        body = exc.read().decode("utf-8", errors="replace")
        try:
            result = json.loads(body)
        except json.JSONDecodeError:
            status = "REJECTED" if exc.code == 413 else "NON_JSON_RESPONSE"
            return exc.code, {"status": status, "error": body}
        if not isinstance(result, dict):
            return exc.code, {"status": "NON_JSON_RESPONSE", "error": result}
        return exc.code, result


def assert_response(label, http_status, result, expected_http, expected_status):
    actual_status = result.get("status")
    if http_status != expected_http or actual_status != expected_status:
        print(f"FAIL {label}: expected HTTP {expected_http}/{expected_status}")
        print(f"got HTTP {http_status}/{actual_status}")
        print(json.dumps(result, indent=2))
        sys.exit(1)
    print(f"PASS {label}: HTTP {http_status} status={actual_status}")


def run_happy_and_rejection_paths():
    print("Booting Proof Market with explicit mock payment enabled...")
    with Oracle({
        "ANVIL_CERTIFICATE_SECRET": "ci-proof-market-secret",
        "ANVIL_ALLOW_MOCK_PAYMENT": "1",
    }) as oracle:
        http_status, result = post_json(oracle.base_url, {
            "client_id": "cortex_client_001",
            "payment_mode": "mock",
            "source_code": SAFE_TRANSFER,
        })
        assert_response("safe mock proof", http_status, result, 200, "PROVEN_SAFE")
        if not result.get("certificate_hash"):
            print("FAIL safe mock proof: missing certificate_hash")
            sys.exit(1)

        http_status, result = post_json(oracle.base_url, {
            "client_id": "cortex_client_001",
            "payment_mode": "mock",
            "source_code": BAD_TRANSFER,
        })
        assert_response("counterexample mock proof", http_status, result, 200, "VULNERABILITY_DETECTED")
        if result.get("certificate_hash"):
            print("FAIL counterexample mock proof: certificate_hash should be absent")
            sys.exit(1)

        http_status, result = post_json(oracle.base_url, {
            "client_id": "cortex_client_001",
            "source_code": SAFE_TRANSFER,
        })
        assert_response("missing Stripe session", http_status, result, 402, "REJECTED")

        http_status, result = post_json(oracle.base_url, {
            "client_id": "cortex_client_001",
            "payment_mode": "mock",
            "source_code": "x" * (50 * 1024 + 1),
        })
        assert_response("oversized source", http_status, result, 413, "REJECTED")

        http_status, result = post_json(oracle.base_url, {
            "client_id": "cortex_client_001",
            "payment_mode": "mock",
            "source_code": SAFE_TRANSFER,
            "padding": "x" * (MAX_JSON_BODY_BYTES + 1),
        })
        assert_response("oversized JSON envelope", http_status, result, 413, "REJECTED")

        http_status, result = post_raw(oracle.base_url, b"{", "application/json")
        assert_response("invalid JSON", http_status, result, 400, "REJECTED")

        http_status, result = post_raw(
            oracle.base_url,
            json.dumps({
                "client_id": "cortex_client_001",
                "payment_mode": "mock",
                "source_code": SAFE_TRANSFER,
            }).encode("utf-8"),
            "text/plain",
        )
        assert_response("unsupported content type", http_status, result, 415, "REJECTED")

        http_status, result = post_json(oracle.base_url, {
            "client_id": "",
            "payment_mode": "mock",
            "source_code": SAFE_TRANSFER,
        })
        assert_response("empty client id", http_status, result, 400, "REJECTED")

        http_status, result = post_json(oracle.base_url, {
            "client_id": "cortex_client_001",
            "payment_mode": "mock",
            "source_code": "",
        })
        assert_response("empty source", http_status, result, 400, "REJECTED")

        http_status, result = post_json(oracle.base_url, {
            "client_id": "cortex_client_001",
            "payment_mode": "mock",
            "source_code": SAFE_TRANSFER,
            "unexpected": "field",
        })
        assert_response("unknown field", http_status, result, 400, "REJECTED")


def run_mock_disabled_path():
    print("Booting Proof Market with mock payment disabled...")
    with Oracle({
        "ANVIL_CERTIFICATE_SECRET": "ci-proof-market-secret",
        "ANVIL_ALLOW_MOCK_PAYMENT": None,
    }) as oracle:
        http_status, result = post_json(oracle.base_url, {
            "client_id": "cortex_client_001",
            "payment_mode": "mock",
            "source_code": SAFE_TRANSFER,
        })
        assert_response("mock disabled", http_status, result, 403, "MOCK_PAYMENT_DISABLED")


def run_missing_secret_path():
    print("Booting Proof Market without certificate secret...")
    with Oracle({
        "ANVIL_CERTIFICATE_SECRET": None,
        "ANVIL_ALLOW_MOCK_PAYMENT": "1",
    }) as oracle:
        http_status, result = post_json(oracle.base_url, {
            "client_id": "cortex_client_001",
            "payment_mode": "mock",
            "source_code": SAFE_TRANSFER,
        })
        assert_response("missing certificate secret", http_status, result, 500, "CONFIGURATION_ERROR")


def main():
    print("Booting Proof Market HTTP integration tests...")
    run_happy_and_rejection_paths()
    run_mock_disabled_path()
    run_missing_secret_path()
    print("All Proof Market HTTP checks passed.")


def test_proof_market_http_flow():
    main()


if __name__ == "__main__":
    main()
