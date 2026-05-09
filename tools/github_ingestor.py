import urllib.request
import os

print("🐍 [OUROBOROS] CORTEX-Persist GitHub Ingestor C5-REAL")
print("==================================================")

def fetch_github_file(owner, repo, path):
    print(f"[*] Extrayendo código de GitHub: {owner}/{repo} -> {path}")
    url = f"https://raw.githubusercontent.com/{owner}/{repo}/main/{path}"
    
    try:
        req = urllib.request.Request(url)
        with urllib.request.urlopen(req) as response:
            source_code = response.read().decode('utf-8')
            
            filename = os.path.basename(path)
            target_file = f"targets/{filename}"
            os.makedirs("targets", exist_ok=True)
            
            with open(target_file, "w") as f:
                f.write(source_code)
                
            print(f"✅ [EXTRACCIÓN COMPLETA] Código fuente guardado en {target_file}")
            print(f"⚖️ Tamaño de la entropía: {len(source_code)} bytes")
            print("==================================================")
    except Exception as e:
        print(f"[*] Buscando en la rama 'master'...")
        url = f"https://raw.githubusercontent.com/{owner}/{repo}/master/{path}"
        try:
            req = urllib.request.Request(url)
            with urllib.request.urlopen(req) as response:
                source_code = response.read().decode('utf-8')
                filename = os.path.basename(path)
                target_file = f"targets/{filename}"
                os.makedirs("targets", exist_ok=True)
                with open(target_file, "w") as f:
                    f.write(source_code)
                print(f"✅ [EXTRACCIÓN COMPLETA] Código fuente guardado en {target_file}")
                print(f"⚖️ Tamaño de la entropía: {len(source_code)} bytes")
                print("==================================================")
        except Exception as e2:
            print(f"❌ Fallo crítico en red: {e2}")

if __name__ == "__main__":
    # Extrayendo el contrato principal del protocolo Pendle Finance / Uniswap
    fetch_github_file("Uniswap", "v2-core", "contracts/UniswapV2Pair.sol")
