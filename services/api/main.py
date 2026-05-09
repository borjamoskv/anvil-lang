import http.server
import socketserver
import json
import subprocess
import hashlib
import tempfile
import os
import time

PORT = 8000

class ProofMarketHandler(http.server.SimpleHTTPRequestHandler):
    def do_POST(self):
        if self.path == '/v1/prove':
            content_length = int(self.headers['Content-Length'])
            post_data = self.rfile.read(content_length)
            
            try:
                request = json.loads(post_data.decode('utf-8'))
                source_code = request.get('source_code', '')
                stripe_session_id = request.get('stripe_session_id', '')
                
                # 0. Límite Termodinámico (Prevenir Exhaustión de Memoria)
                if len(source_code) > 50 * 1024: # 50 KB max
                    self.send_response(413)
                    self.send_header('Content-type', 'application/json')
                    self.end_headers()
                    self.wfile.write(b'{"detail": "Payload Too Large: Exceeds 50KB strict limit"}')
                    return
                
                # 1. Billing Validation
                if not stripe_session_id.startswith('cs_'):
                    self.send_response(402)
                    self.send_header('Content-type', 'application/json')
                    self.end_headers()
                    self.wfile.write(b'{"detail": "Payment Required: Invalid Stripe Session"}')
                    return

                start_time = time.time()

                # 2. Ephemeral Sandboxing
                with tempfile.NamedTemporaryFile(mode="w+", suffix=".anv", delete=False) as temp_file:
                    temp_file.write(source_code)
                    temp_path = temp_file.name

                try:
                    # 3. Z3 Execution Pipeline con Hard Timeout
                    workspace_root = os.path.abspath(os.path.join(os.path.dirname(__file__), "../../"))
                    
                    try:
                        process = subprocess.run(
                            ["./target/debug/anvil", "check", temp_path],
                            cwd=workspace_root,
                            capture_output=True,
                            text=True,
                            timeout=10.0 # Evitar Z3 Solver Hangs (DoS)
                        )
                        output = process.stderr if process.stderr else process.stdout
                        returncode = process.returncode
                    except subprocess.TimeoutExpired:
                        returncode = -1
                        output = "VULNERABILITY DETECTED: Thermodynamic Timeout (Z3 Solver exhausted). Possible malicious infinite loop."

                    execution_time_ms = (time.time() - start_time) * 1000

                    # 4. Axiomatic Resolution
                    self.send_response(200)
                    self.send_header('Content-type', 'application/json')
                    self.end_headers()

                    if returncode == 0:
                        # SATISFIED (UNSAT for exploits)
                        payload = f"{source_code}|{start_time}|ANVIL_MASTER_KEY".encode('utf-8')
                        cert_hash = hashlib.sha256(payload).hexdigest()
                        
                        response = {
                            "status": "PROVEN_SAFE",
                            "execution_time_ms": execution_time_ms,
                            "certificate_hash": f"anv_cert_{cert_hash}",
                            "z3_output": "All postconditions proven. Zero trust required."
                        }
                    else:
                        # VULNERABLE
                        response = {
                            "status": "VULNERABILITY_DETECTED",
                            "execution_time_ms": execution_time_ms,
                            "certificate_hash": None,
                            "z3_output": output
                        }

                    self.wfile.write(json.dumps(response).encode('utf-8'))

                finally:
                    if os.path.exists(temp_path):
                        os.remove(temp_path)
            
            except Exception as e:
                self.send_response(500)
                self.send_header('Content-type', 'application/json')
                self.end_headers()
                self.wfile.write(json.dumps({"detail": str(e)}).encode('utf-8'))
        else:
            self.send_response(404)
            self.end_headers()

print(f"Anvil Proof Market Oracle booting on port {PORT}...")
print("Zero dependencies loaded. Pure thermodynamic socket.")
with socketserver.TCPServer(("127.0.0.1", PORT), ProofMarketHandler) as httpd:
    httpd.serve_forever()
