import os
import sys
import time
import subprocess

def publish():
    draft_path = "/Users/borjafernandezangulo/10_PROJECTS/anvil-lang/reports/substack_draft_david_dominguez.md"
    if not os.path.exists(draft_path):
        print(f"Error: {draft_path} no existe.")
        sys.exit(1)
        
    with open(draft_path, "r", encoding="utf-8") as f:
        lines = f.readlines()
        
    title = ""
    body_lines = []
    
    # Extraer el título (primera línea con #) y el cuerpo
    for line in lines:
        if line.startswith("# ") and not title:
            title = line.replace("# ", "").strip()
        else:
            body_lines.append(line)
            
    body = "".join(body_lines).strip()
    
    # Crear script AppleScript temporal para automatizar Chrome
    # Copiamos primero el título al portapapeles, abrimos Chrome, pegamos, copiamos el cuerpo, pegamos.
    # Usamos osascript para interactuar.
    
    # Escribimos los archivos temporales para el portapapeles
    title_tmp = "/tmp/substack_title.txt"
    body_tmp = "/tmp/substack_body.txt"
    
    with open(title_tmp, "w", encoding="utf-8") as f:
        f.write(title)
        
    with open(body_tmp, "w", encoding="utf-8") as f:
        f.write(body)
        
    applescript_content = f"""
    -- Copiar título al portapapeles de macOS
    set theTitle to read "/tmp/substack_title.txt" as «class utf8»
    set theBody to read "/tmp/substack_body.txt" as «class utf8»
    
    set the clipboard to theTitle
    
    tell application "Google Chrome"
        activate
        -- Abrir nueva pestaña con el editor de Substack
        open location "https://borjamoskv.substack.com/dashboard/post/new?type=post"
        delay 8 -- Esperar a que cargue la página del editor
    end tell
    
    tell application "System Events"
        tell process "Google Chrome"
            set frontmost to true
            -- El cursor debería estar automáticamente en el campo del título
            -- Pegar el título
            keystroke "v" using command down
            delay 1
            
            -- Copiar el cuerpo al portapapeles
            set the clipboard to theBody
            delay 0.5
            
            -- Presionar Tab para pasar al subtítulo / cuerpo
            keystroke tab
            delay 0.5
            -- Presionar Tab otra vez por si acaso entra en subtítulo antes del cuerpo
            keystroke tab
            delay 0.5
            
            -- Pegar el cuerpo del artículo
            keystroke "v" using command down
            delay 1
        end tell
    end tell
    """
    
    scpt_path = "/tmp/publish_substack.scpt"
    with open(scpt_path, "w", encoding="utf-8") as f:
        f.write(applescript_content)
        
    print("Iniciando automatización de Substack vía Google Chrome y AppleScript...")
    result = subprocess.run(["osascript", scpt_path], capture_output=True, text=True)
    
    if result.returncode == 0:
        print("[SUCCESS] Automatización completada. Revisa Google Chrome en tu escritorio.")
    else:
        print(f"[FAIL] Error en AppleScript: {result.stderr.strip()}")

if __name__ == "__main__":
    publish()
