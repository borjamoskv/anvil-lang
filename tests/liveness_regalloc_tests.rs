use sovereign_c::lexer::Token;
use sovereign_c::parser::Parser;
use sovereign_c::borrowck::{BorrowContext, check_statement};
use sovereign_c::lowering::IrLoweringVisitor;
use sovereign_c::liveness::{compute_live_intervals, LiveInterval};
use sovereign_c::regalloc::{LinearScanAllocator, PhysReg, RegLocation};
use sovereign_c::backend::generate_x86_64_optimized;
use sovereign_c::ir::IrReg;
use logos::Logos;

#[test]
fn test_liveness_intervals_computation() {
    let code = r#"
    {
        let a: u32 = 10;
        let b: u32 = 20;
    }
    "#;
    let lexer = Token::lexer(code);
    let tokens: Vec<Token> = lexer.filter_map(Result::ok).collect();
    let mut parser = Parser::new(tokens.into_iter());
    let ast = parser.parse_block().expect("Parse failed");

    let mut ctx = BorrowContext::new();
    let transformed = check_statement(&ast, &mut ctx).expect("Borrowck failed");

    let visitor = IrLoweringVisitor::new();
    let ir = visitor.lower_ast(&transformed, "test_liveness");

    let intervals = compute_live_intervals(&ir);
    assert_eq!(intervals.len(), 2);
    assert!(intervals[0].start <= intervals[0].end);
    assert!(intervals[1].start <= intervals[1].end);
}

#[test]
fn test_physical_register_reuse_non_overlapping() {
    // Definir manualmente dos intervalos que no se solapan: [0, 2] y [4, 6]
    let intervals = vec![
        LiveInterval { reg: IrReg(0), start: 0, end: 2 },
        LiveInterval { reg: IrReg(1), start: 4, end: 6 },
    ];

    // Con un pool de un único registro físico, el segundo debe reutilizarlo
    let allocator = LinearScanAllocator::new(&[PhysReg::Rax]);
    let assignment = allocator.allocate(intervals);

    assert_eq!(assignment.num_spills, 0, "No debe haber spills si los intervalos no se solapan");
    assert_eq!(assignment.mapping.get(&IrReg(0)), Some(&RegLocation::Reg(PhysReg::Rax)));
    assert_eq!(assignment.mapping.get(&IrReg(1)), Some(&RegLocation::Reg(PhysReg::Rax)));
}

#[test]
fn test_spill_under_high_pressure() {
    // 3 variables que se solapan en [0, 10], [1, 10], [2, 10]
    let intervals = vec![
        LiveInterval { reg: IrReg(0), start: 0, end: 10 },
        LiveInterval { reg: IrReg(1), start: 1, end: 8 },
        LiveInterval { reg: IrReg(2), start: 2, end: 6 },
    ];

    // Solo 2 registros físicos disponibles: Rax, Rcx
    let allocator = LinearScanAllocator::new(&[PhysReg::Rax, PhysReg::Rcx]);
    let assignment = allocator.allocate(intervals);

    assert_eq!(assignment.num_spills, 1, "Debe ocurrir exactamente 1 spill");
    
    let mut phys_count = 0;
    let mut spill_count = 0;
    for loc in assignment.mapping.values() {
        match loc {
            RegLocation::Reg(_) => phys_count += 1,
            RegLocation::Spill(_) => spill_count += 1,
        }
    }
    assert_eq!(phys_count, 2);
    assert_eq!(spill_count, 1);
}

#[test]
fn test_volatile_store_register_correctness() {
    let code = r#"
    {
        let buffer: owned u8 = 1024;
        let number: u32 = 42;
        
        unsafe {
            let mmio_port: raw u32 = 400000;
            *mmio_port = number;
        }
    }
    "#;
    let lexer = Token::lexer(code);
    let tokens: Vec<Token> = lexer.filter_map(Result::ok).collect();
    let mut parser = Parser::new(tokens.into_iter());
    let ast = parser.parse_block().expect("Parse failed");

    let mut ctx = BorrowContext::new();
    let transformed = check_statement(&ast, &mut ctx).expect("Borrowck failed");

    let visitor = IrLoweringVisitor::new();
    let ir = visitor.lower_ast(&transformed, "test_opt");

    let intervals = compute_live_intervals(&ir);
    let allocator = LinearScanAllocator::default_x86_64();
    let assignment = allocator.allocate(intervals);

    assert_eq!(assignment.num_spills, 0, "El código con 3 variables cabe completamente en los 9 registros físicos");

    let asm = generate_x86_64_optimized(&ir, &assignment);
    assert!(asm.contains("0 SPILLS - Pila intacta"), "Debe reportar 0 spills");
    assert!(asm.contains("VOLATILE STORE RING-0 MMIO"), "Debe preservar la barrera MMIO");
    assert!(asm.contains("mfence"), "Debe emitir mfence");
    assert!(asm.contains("sys_free_os"), "Debe emitir el destructor SyntheticDrop");
}
