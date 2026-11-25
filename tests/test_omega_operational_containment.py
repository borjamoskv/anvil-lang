"""Execute extracted production functions with mocks; never run mail or RPC."""
import ast
import asyncio
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import Mock

ROOT = Path(__file__).resolve().parent.parent


def extract(path, name, globals_=None, owner=None):
    tree = ast.parse((ROOT / path).read_text(encoding="utf-8"))
    body = tree.body
    if owner:
        body = next(node for node in body
                    if isinstance(node, ast.ClassDef) and node.name == owner).body
    fn = next(node for node in body
              if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
              and node.name == name)
    fn.decorator_list = []
    for arg in fn.args.posonlyargs + fn.args.args + fn.args.kwonlyargs:
        arg.annotation = None
    fn.returns = None
    ns = dict(globals_ or {})
    exec(compile(ast.fix_missing_locations(ast.Module(body=[fn], type_ignores=[])),
                 path, "exec"), ns)
    return ns[name]


class OperationalContainment(unittest.TestCase):
    def test_inbound_does_not_read_or_send(self):
        poll = extract("inbound_sentinel.py", "poll")
        with self.assertRaises(SystemExit):
            poll()

    def test_notification_uses_static_script(self):
        run = Mock()
        notify = extract("tools/mail_sentinel.py", "notify_macos",
                         {"subprocess": SimpleNamespace(run=run)})
        notify("title", 'text with "quotes" and newline\n')
        first = run.call_args.args[0]
        notify("different title", "different text")
        second = run.call_args.args[0]
        self.assertEqual(first[:4], second[:4])
        self.assertEqual(first[4:], ["title", 'text with "quotes" and newline\n'])
        self.assertEqual(second[4:], ["different title", "different text"])

    def test_remote_delivery_blocked_even_direct_call(self):
        deliver = extract("tools/ouroboros_auto_submit.py",
                          "submit_vulnerability_report")
        for dry_run in (False, True):
            with self.assertRaises(RuntimeError):
                deliver("unused", "report", "taint", "hash", dry_run=dry_run)
        prepare = extract("tools/ouroboros_auto_submit.py", "cmd_prepare")
        for flags in ({"submit": True}, {"dry_run": True}):
            with self.assertRaises(SystemExit):
                prepare("unused", **flags)

    def test_legacy_proof_route_rejects_before_body_or_billing(self):
        write = Mock()
        response = lambda status, message: {
            "status": status, "certificate_hash": None, "message": message}
        post = extract("services/api/main.py", "do_POST",
                       {"write_json": write, "prove_response": response},
                       owner="ProofMarketHandler")
        request = SimpleNamespace(path="/v1/prove")
        post(request)
        self.assertEqual(write.call_args.args[1], 503)
        self.assertIsNone(write.call_args.args[2]["certificate_hash"])

    def test_trace_writes_have_no_side_effects(self):
        class HttpError(Exception):
            def __init__(self, status_code, detail):
                self.status_code = status_code
        submit = extract("agents-archi-mvp/api/main.py", "submit_trace",
                         {"HTTPException": HttpError})
        with self.assertRaises(HttpError) as raised:
            asyncio.run(submit(object()))
        self.assertEqual(raised.exception.status_code, 503)

    def test_missing_key_fails_before_rpc(self):
        init = extract("agents-archi-mvp/real_anchorer.py", "__init__",
                       {"PRIVATE_KEY": None}, owner="RealAnchorer")
        with self.assertRaises(RuntimeError):
            init(SimpleNamespace())

    def test_demo_cannot_overwrite_registry(self):
        generate = extract("agents-archi-mvp/archi_ledger.py",
                           "generate_swarm_ledger")
        with self.assertRaises(SystemExit):
            generate()


if __name__ == "__main__":
    unittest.main()
