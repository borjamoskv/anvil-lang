use sovereign_c::lexer::Token;
use sovereign_c::parser::Parser;
use sovereign_c::borrowck::{BorrowContext, check_statement};
use sovereign_c::lowering::IrLoweringVisitor;
use sovereign_c::ir::IrOp;
use sovereign_c::backend::generate_x86_64_from_ir;
use logos::Logos;
use std::collections::HashSet;

#[test]
fn test_affine_alloc_and_synthetic_drop_lowering() {
    let code = r#"
    {
        let mem: owned u8 = 512;
    }
    "#;
    let lexer = Token::lexer(code);
    let tokens: Vec<Token> = lexer.filter_map(Result::ok).collect();
    let mut parser = Parser::new(tokens.into_iter());
    let ast = parser.parse_block().expect("Parse failed");

    let mut ctx = BorrowContext::new();
    let transformed = check_statement(&ast, &mut ctx).expect("Borrowck failed");

    let visitor = IrLoweringVisitor::new();
    let ir = visitor.lower_ast(&transformed, "test_alloc_drop");

    // Verificar que existe Alloc y Free sobre el mismo registro SSA
    let mut allocated_reg = None;
    let mut freed_reg = None;

    for block in &ir.blocks {
        for op in &block.instructions {
            match op {
                IrOp::Alloc { dest, size } => {
                    assert_eq!(*size, 512);
                    allocated_reg = Some(*dest);
                }
                IrOp::Free { ptr } => {
                    freed_reg = Some(*ptr);
                }
                _ => {}
            }
        }
    }

    assert!(allocated_reg.is_some(), "Debe existir instrucción Alloc");
    assert!(freed_reg.is_some(), "Debe existir instrucción Free para SyntheticDrop");
    assert_eq!(allocated_reg.unwrap(), freed_reg.unwrap(), "El registro liberado debe coincidir unívocamente con el asignado");
}

#[test]
fn test_mmio_volatile_store_barrier() {
    let code = r#"
    {
        unsafe {
            let port: raw u32 = 65536;
            *port = 255;
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
    let ir = visitor.lower_ast(&transformed, "test_volatile");

    let mut has_volatile_store = false;
    for block in &ir.blocks {
        for op in &block.instructions {
            if let IrOp::VolatileStore { ptr: _, value: _ } = op {
                has_volatile_store = true;
            }
        }
    }

    assert!(has_volatile_store, "Todo acceso a raw ptr en unsafe debe emitir VolatileStore");

    let asm = generate_x86_64_from_ir(&ir);
    assert!(asm.contains("VOLATILE STORE RING-0 MMIO"), "El ensamblador debe contener el marcador de barrera volátil");
    assert!(asm.contains("mfence"), "El ensamblador debe emitir la instrucción mfence");
}

#[test]
fn test_ssa_single_assignment_invariant() {
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
    let ir = visitor.lower_ast(&transformed, "test_ssa");

    let mut assigned_regs = HashSet::new();
    for block in &ir.blocks {
        for op in &block.instructions {
            let dest_opt = match op {
                IrOp::AssignImm { dest, .. } => Some(*dest),
                IrOp::Alloc { dest, .. } => Some(*dest),
                IrOp::Load { dest, .. } => Some(*dest),
                IrOp::VolatileLoad { dest, .. } => Some(*dest),
                IrOp::Add { dest, .. } => Some(*dest),
                IrOp::Call { dest, .. } => *dest,
                IrOp::Store { .. } | IrOp::VolatileStore { .. } | IrOp::Free { .. } => None,
            };

            if let Some(dest) = dest_opt {
                assert!(assigned_regs.insert(dest.0), "Violación de Invariante SSA: Registro %{} asignado más de una vez", dest.0);
            }
        }
    }
}

#[test]
fn test_raw_ptr_outside_unsafe_rejected() {
    let code = r#"
    {
        let port: raw u32 = 65536;
    }
    "#;
    let lexer = Token::lexer(code);
    let tokens: Vec<Token> = lexer.filter_map(Result::ok).collect();
    let mut parser = Parser::new(tokens.into_iter());
    let ast = parser.parse_block().expect("Parse failed");

    let mut ctx = BorrowContext::new();
    let res = check_statement(&ast, &mut ctx);
    assert!(res.is_err(), "Vincular raw ptr fuera de unsafe debe fallar");
}
