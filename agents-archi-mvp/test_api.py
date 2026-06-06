import requests
import time
import json
import hashlib
import os

API_URL = "http://127.0.0.1:8000/api/v1/traces"

def generate_mock_trace():
    timestamp = time.time()
    data = {
        "agentId": "CORTEX_REAPER_99",
        "domain": "security_audit",
        "timestamp": timestamp,
        "inputs": {"target": "0xDefiProtocol"},
        "outputs": {"vulnerabilities_found": 2},
        "toolCalls": []
    }
    
    trace_string = json.dumps(data, sort_keys=True)
    trace_hash = hashlib.sha256(trace_string.encode('utf-8')).hexdigest()
    
    return {
        "agent_id": "CORTEX_REAPER_99",
        "session_id": "session_" + os.urandom(4).hex(),
        "domain": "security_audit",
        "trace_hash": "0x" + trace_hash,
        "timestamp": timestamp,
        "state": "COMPLETED",
        "metadata": {"execution_time_ms": 1245}
    }

def run_test():
    print("Iniciando prueba de inyección de Trace al Gateway...")
    
    payload = generate_mock_trace()
    print(f"[*] Payload generado con Hash: {payload['trace_hash'][:15]}...")
    
    try:
        response = requests.post(API_URL, json=payload)
        if response.status_code == 200:
            print("[+] Trace inyectado correctamente en el Ledger a través del API.")
            print("Respuesta:", response.json())
        else:
            print("[-] Error en la inyección:", response.status_code, response.text)
    except Exception:
        print("[-] Error de conexión. ¿Está levantado el servidor FastAPI en el puerto 8000?")
        print("    Ejecuta: uvicorn api.main:app --reload")
        
if __name__ == "__main__":
    run_test()
