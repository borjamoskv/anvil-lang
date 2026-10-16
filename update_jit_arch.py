import re

with open("src/main.rs", "r") as f:
    content = f.read()

# Replace x86_64 payload with aarch64 payload
old_payload = """    let machine_code: Vec<u8> = vec![
        0x48, 0xC7, 0xC0, 0x2A, 0x00, 0x00, 0x00, // mov rax, 42
        0xC3                                      // ret
    ];"""

new_payload = """    // Detección C5-REAL: Target ARM64 detectado (macOS M-series)
    // Opcodes correspondientes a: `mov x0, #42; ret`
    let machine_code: Vec<u8> = vec![
        0x40, 0x05, 0x80, 0xD2, // mov x0, #42
        0xC0, 0x03, 0x5F, 0xD6  // ret
    ];"""

content = content.replace(old_payload, new_payload)

with open("src/main.rs", "w") as f:
    f.write(content)
