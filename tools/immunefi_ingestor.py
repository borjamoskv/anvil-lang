import sys
import os
import urllib.request
import urllib.parse
import json

print("🐍 [OUROBOROS] CORTEX-Persist Ingestor C5-REAL (Etherscan Bridge)")
print("=================================================================")

# Clave de Etherscan (puedes inyectar la tuya en ENV o usar esta de pruebas limitada)
ETHERSCAN_API_KEY = os.environ.get("ETHERSCAN_API_KEY", "YourApiKeyToken")

def fetch_verified_contract(address: str):
    print(f"[*] Escaneando Ethereum Mainnet para objetivo: {address}")
    url = f"https://api.etherscan.io/api?module=contract&action=getsourcecode&address={address}&apikey={ETHERSCAN_API_KEY}"
    
    try:
        req = urllib.request.Request(url, headers={'User-Agent': 'CORTEX-Ouroboros/1.0'})
        with urllib.request.urlopen(req) as response:
            data = json.loads(response.read().decode('utf-8'))
            
            if data['status'] != '1':
                print(f"❌ Error de extracción: {data['message']} - {data['result']}")
                sys.exit(1)
                
            contract_info = data['result'][0]
            source_code = contract_info['SourceCode']
            contract_name = contract_info['ContractName']
            
            if not source_code:
                print(f"❌ El contrato en {address} NO está verificado en Etherscan.")
                sys.exit(1)
                
            print(f"✅ Contrato verificado localizado: {contract_name}")
            
            # Limpiar posible JSON anidado de Etherscan
            if source_code.startswith("{{") and source_code.endswith("}}"):
                source_code = source_code[1:-1]
                try:
                    parsed = json.loads(source_code)
                    source_code = "\n".join([v['content'] for k, v in parsed['sources'].items()])
                except Exception:
                    pass

            # Guardar el artefacto en crudo
            target_file = f"targets/{contract_name}_{address[:6]}.sol"
            os.makedirs("targets", exist_ok=True)
            with open(target_file, "w") as f:
                f.write(source_code)
                
            print(f"📦 [EXTRACCIÓN COMPLETA] Código fuente de {contract_name} guardado en {target_file}")
            print(f"⚖️ Tamaño de la entropía: {len(source_code)} bytes")
            print("=================================================================")
            print(f"Siguiente paso: Extrae el invariante del bloque de código y tradúcelo a .anv para Z3.")

    except Exception as e:
        print(f"❌ Fallo crítico en red: {e}")
        sys.exit(1)

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Uso: python3 immunefi_ingestor.py <0xContractAddress>")
        # Default to Uniswap V2 Router for demo purposes if no arg provided
        target = "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D"
        print(f"Inyectando objetivo por defecto (Uniswap V2 Router): {target}")
        fetch_verified_contract(target)
    else:
        fetch_verified_contract(sys.argv[1])
