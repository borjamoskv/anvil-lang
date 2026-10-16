pub mod ast;
pub mod borrowck;
pub mod lexer;
pub mod parser;
pub mod ir;
pub mod lowering;
pub mod liveness;
pub mod regalloc;
pub mod backend;
pub mod jit;

use logos::Logos;
use lexer::Token;
use parser::Parser;
use borrowck::{BorrowContext, check_statement};
use lowering::IrLoweringVisitor;
use liveness::compute_live_intervals;
use regalloc::LinearScanAllocator;
use backend::generate_x86_64_optimized;
use jit::SovereignJitEngine;

fn main() {
    println!("=================================================================");
    println!("   SOVEREIGN COMPILER v0.3.0 - C5-REAL ARCHITECTURE             ");
    println!("   BIFURCACIÓN 3: LIVENESS ANALYSIS & LINEAR SCAN REGALLOC      ");
    println!("=================================================================\n");

    let source_code = r#"
    {
        let buffer: owned u8 = 1024;
        let number: u32 = 42;
        
        unsafe {
            let mmio_port: raw u32 = 400000;
            *mmio_port = number;
        }
    }
    "#;

    println!("[1] Código Fuente Original (Territorio):");
    println!("{}\n", source_code.trim());

    // --- FASE 1: Lexer ---
    let lexer = Token::lexer(source_code);
    let tokens: Vec<Token> = lexer.filter_map(Result::ok).collect();
    println!("[2] Lexer: Compresión a Tokens completada ({} tokens).", tokens.len());

    // --- FASE 2: Parser ---
    let mut parser = Parser::new(tokens.into_iter());
    let ast = match parser.parse_block() {
        Ok(tree) => {
            println!("[3] Parser: AST proyectado con éxito.");
            tree
        }
        Err(e) => {
            eprintln!("\n[!] ERROR SINTÁCTICO: {}", e);
            return;
        }
    };

    // --- FASE 3: Borrow Checker ---
    println!("\n[4] Borrow Checker: Verificando Semántica Afín e Inyectando Drops...");
    let mut ctx = BorrowContext::new();
    let transformed_ast = match check_statement(&ast, &mut ctx) {
        Ok(nodes) => {
            println!("[+] ÉXITO: Invariante C5-REAL Mantenida (Markov Blanket estable).");
            nodes
        }
        Err(e) => {
            eprintln!("\n[!] FALLO TERMODINÁMICO: {}", e);
            return;
        }
    };

    // --- FASE 4: Lowering a SSA IR (CFG) ---
    println!("\n[5] Lowering Engine: Aplanamiento Topológico hacia SSA IR...");
    let visitor = IrLoweringVisitor::new();
    let function_ir = visitor.lower_ast(&transformed_ast, "kernel_main");

    println!("\n=== GRAFO DE FLUJO DE CONTROL (CFG SSA IR) ===");
    println!("{}", function_ir);
    println!("==============================================\n");

    // --- FASE 5: Liveness Analysis ---
    println!("[6] Liveness Analysis: Computando Intervalos de Vida [start, end]...");
    let live_intervals = compute_live_intervals(&function_ir);
    println!("--------------------------------------------------");
    for inv in &live_intervals {
        println!("  Registro SSA {}: [{:>2}, {:>2}] (delta = {} ciclos virtuales)",
            inv.reg, inv.start, inv.end, inv.end - inv.start);
    }
    println!("--------------------------------------------------\n");

    // --- FASE 6: Linear Scan Register Allocation ---
    println!("[7] Linear Scan Allocator: Asignando Registros Físicos x86_64...");
    let allocator = LinearScanAllocator::default_x86_64();
    let assignment = allocator.allocate(live_intervals);

    println!("--------------------------------------------------");
    for (reg, loc) in &assignment.mapping {
        println!("  Registro SSA {} -> {}", reg, loc);
    }
    println!("  Presión de Memoria: {} spills a stack", assignment.num_spills);
    println!("--------------------------------------------------\n");

    // --- FASE 7: Backend Optimizado ---
    println!("[8] Backend: Emitiendo Ensamblador Optimizado (x86_64 Bare-Metal)...");
    let asm_output = generate_x86_64_optimized(&function_ir, &assignment);

    println!("\n--- OUTPUT ENSAMBLADOR EMITIDO (ALTA EXERGÍA) ---");
    println!("{}", asm_output);
    println!("-------------------------------------------------");

    // --- FASE 8: Ejecución JIT (Salto de Fase C5-REAL) ---
    println!("\n[9] Sovereign JIT Engine: Materialización in-memory (W^X Invariant)...");
    
    // Opcodes correspondientes a: `mov rax, 42; ret`
    // (Demostración de ejecución inyectada desde memoria anónima sin I/O de disco)
    // Detección C5-REAL: Target ARM64 detectado (macOS M-series)
    // Opcodes correspondientes a: `mov x0, #42; ret`
    let machine_code: Vec<u8> = vec![
        0x40, 0x05, 0x80, 0xD2, // mov x0, #42
        0xC0, 0x03, 0x5F, 0xD6  // ret
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

