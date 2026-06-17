import os
import sys
import time
import subprocess
import Quartz

def set_clipboard(text):
    # Usar pbcopy para guardar texto en el portapapeles de macOS
    p = subprocess.Popen(['pbcopy'], stdin=subprocess.PIPE)
    p.communicate(input=text.encode('utf-8'))

def press_key(key_code, flags=0):
    down = Quartz.CGEventCreateKeyboardEvent(None, key_code, True)
    up = Quartz.CGEventCreateKeyboardEvent(None, key_code, False)
    if flags:
        Quartz.CGEventSetFlags(down, flags)
        Quartz.CGEventSetFlags(up, flags)
    Quartz.CGEventPost(Quartz.kCGHIDEventTap, down)
    time.sleep(0.05)
    Quartz.CGEventPost(Quartz.kCGHIDEventTap, up)
    time.sleep(0.05)

def press_cmd_v():
    # 9 es el key code de 'v'
    # kCGEventFlagMaskCommand es 0x00100000 (o 1048576)
    cmd_flag = 0x00100000
    press_key(9, cmd_flag)

def press_tab():
    # 48 es el key code de Tab
    press_key(48)

def type_unicode(text):
    down = Quartz.CGEventCreateKeyboardEvent(None, 0, True)
    up = Quartz.CGEventCreateKeyboardEvent(None, 0, False)
    Quartz.CGEventKeyboardSetUnicodeString(down, len(text), text)
    Quartz.CGEventKeyboardSetUnicodeString(up, len(text), text)
    Quartz.CGEventPost(Quartz.kCGHIDEventTap, down)
    time.sleep(0.05)
    Quartz.CGEventPost(Quartz.kCGHIDEventTap, up)
    time.sleep(0.05)

def publish():
    draft_path = "/Users/borjafernandezangulo/10_PROJECTS/anvil-lang/reports/substack_draft_david_dominguez.md"
    if not os.path.exists(draft_path):
        print(f"Error: {draft_path} no existe.")
        sys.exit(1)
        
    with open(draft_path, "r", encoding="utf-8") as f:
        lines = f.readlines()
        
    title = ""
    body_lines = []
    
    # Extraer el título y el cuerpo
    for line in lines:
        if line.startswith("# ") and not title:
            title = line.replace("# ", "").strip()
        else:
            body_lines.append(line)
            
    body = "".join(body_lines).strip()
    subtitle = "Cómo los modelos de simulación entran en cortocircuito cognitivo ante el mínimo input de la realidad fiscal y técnica."

    print("Abriendo Google Chrome en la URL de nuevo post de Substack...")
    # Usar AppleScript para activar Chrome y abrir la URL (no requiere permisos de keystroke)
    applescript_open = f"""
    tell application "Google Chrome"
        activate
        open location "https://borjamoskv.substack.com/dashboard/post/new?type=post"
    end tell
    """
    subprocess.run(["osascript", "-e", applescript_open])
    
    print("Esperando 10 segundos para la carga del editor de Substack...")
    time.sleep(10)
    
    print("Inyectando título...")
    set_clipboard(title)
    press_cmd_v()
    time.sleep(1)
    
    print("Pasando al campo de subtítulo (Tab)...")
    press_tab()
    time.sleep(0.5)
    
    print("Inyectando subtítulo...")
    set_clipboard(subtitle)
    press_cmd_v()
    time.sleep(1)
    
    print("Pasando al campo de cuerpo (Tab)...")
    press_tab()
    time.sleep(0.5)
    
    print("Copiando cuerpo del artículo al portapapeles...")
    set_clipboard(body)
    time.sleep(0.5)
    
    print("Pegando cuerpo del artículo (Cmd+V)...")
    press_cmd_v()
    time.sleep(1)
    
    print("[SUCCESS] Automatización de bajo nivel completada con éxito.")
    print("Revisa tu navegador. El borrador de Substack debe estar listo para publicar en tu pantalla.")

if __name__ == "__main__":
    publish()
