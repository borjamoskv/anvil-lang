import re

with open("src/main.rs", "r") as f:
    content = f.read()

content = content.replace("pub mod backend;", "pub mod backend;\npub mod jit;")
content = content.replace("use backend::generate_x86_64_optimized;", "use backend::generate_x86_64_optimized;\nuse jit::SovereignJitEngine;")

jit_block = """
    // --- FASE 8: Ejecución JIT (Salto de Fase C5-REAL) ---
    println!("\\n[9] Sovereign JIT Engine: Materialización in-memory (W^X Invariant)...");
    
    // Opcodes correspondientes a: `mov rax, 42; ret`
    // (Demostración de ejecución inyectada desde memoria anónima sin I/O de disco)
    let machine_code: Vec<u8> = vec![
        0x48, 0xC7, 0xC0, 0x2A, 0x00, 0x00, 0x00, // mov rax, 42
        0xC3                                      // ret
    ];

    let engine = SovereignJitEngine::new(machine_code);
    unsafe {
        match engine.execute() {
            Ok(result) => {
                println!("[+] Colapso Gödeliano exitoso. No I/O. Cero Burocracia.");
                println!("[+] Código Máquina Directo Retornó: {}", result);
            }
            Err(e) => {
                eprintln!("[!] Fallo Termodinámico JIT: {}", e);
            }
        }
    }
}
"""
content = content.replace("println!(\"-------------------------------------------------\");\n}", "println!(\"-------------------------------------------------\");\n" + jit_block)

with open("src/main.rs", "w") as f:
    f.write(content)
